# Omni-Crap: Review & Decided Direction

Date: 2026-06-02
Status: Decisions captured from interactive review. Not yet implemented.

This document records the critique of the codebase's vision, design, and implementation, and the chosen approach for each open issue. Items are grouped by theme. Each section states the problem, the alternatives considered, the chosen approach, and concrete implementation notes.

---

## 1. Risk model

**Problem.** Current model in `lib.rs:133` is `structural * process * stability / baseline`, where `baseline` is a fixed analytic constant (lib.rs:160-167), not the repo's actual distribution. The "Nx baseline" output is therefore uninterpretable without per-repo calibration. README has ~10 multiplicative coefficients; `.omnicrap.toml` exposes 3.

**Alternatives considered.**
- Additive z-scores + percentile rank.
- Hybrid: additive intra-axis, multiplicative inter-axis, percentile-normalized.
- Keep multiplicative, expose every knob.
- Keep current, document precisely.

**Decision — Hybrid: additive intra-axis, multiplicative inter-axis (with percentile rank).**

```text
structural = z(complexity) + z(halstead) + z(clone)
process    = z(churn) + z(authors) + z(scatter)
stability  = z(redundancy) + z(age)
risk       = (1 + structural) * (1 + process) / (1 + stability)
output     = percentile_rank(risk, repo_distribution)
```

**Implementation notes.**
- z-score normalization per axis against the current run's distribution; cache mean/stddev per repo invocation.
- Each axis is exposed in JSON output as both raw value and z-score.
- Percentile rank replaces the current "Nx baseline" number. Output column becomes e.g. `p92` or `0.92`.
- Score is still monotonic in each input so it remains intuitive.
- Reweight coefficients per axis stay in `.omnicrap.toml` but become additive multipliers (not power-of-2 multipliers).

---

## 2. Process metric granularity

**Problem.** Churn, authors, scatter, age, agent_ratio are file-level in `vcs.rs` but applied as multipliers to scope-level structural risk (lib.rs:133). Every scope in a hot file inherits the same churn multiplier, contradicting VISION.md's "semantic churn" claim.

**Alternatives considered.**
- Per-scope via `git log -L` always.
- Line-range overlap heuristic from `--numstat`.
- Per-scope on-demand `--deep` flag.
- File-level forever, retract the VISION claim.

**Decision — `git log -L :scope:file` on demand under a `--deep` flag.**

**Implementation notes.**
- Default: file-level (cheap). With `--deep`, run `git log -L :{scope_name}:{file}` per top-N scope after first-pass scoring.
- N defaults to 50; configurable via `--deep-top-n` or config.
- Subprocess cost amortized by issuing log calls in parallel via rayon.
- When `--deep` is active, output marks affected metrics as `(scope)` in the per-axis grain tag (see §11).
- Without `--deep`, JSON/SARIF output marks process metrics as `grain: "file"`.

---

## 3. Regex fallback engine parity

**Problem.** TreeSitterEngine emits per-scope rows; RegexEngine emits one Global per file (regex_engine.rs:52). Same threshold applied to both is misleading.

**Alternatives considered.**
- Indent-based scope chunking in regex.
- Separate output streams for TS vs regex.
- Drop the regex engine entirely; cut the 250+ language claim.
- Per-engine thresholds.

**Decision — Indent-based scope chunking in `RegexEngine`.**

**Implementation notes.**
- Add a lightweight indent-block detector. Heuristic:
  - For brace-languages, find lines that begin a function-like definition by regex from `scc`'s `complexitychecks` and pair with matching braces.
  - For indent-based languages (Python-like, where TS isn't available), use indent dedent boundaries.
- Emit scopes with `name = "block_at_line_NN"` when no parseable name is recoverable.
- Complexity per chunk uses the same `complexitychecks` patterns as today, but applied to the chunk's slice instead of whole file.
- Falls back to whole-file when chunking confidence is low (single-scope mode, emit `name = "Global"`).

---

## 4. Advice generation

**Problem.** `generate_advice` (lib.rs:84-131) is a first-match cascade. The AI-generated short-circuit at lib.rs:85 masks more actionable advice (clones, mock-heavy tests, complexity).

**Alternatives considered.**
- Emit all matching findings, ranked.
- Priority constants per rule.
- Just move the AI check to the end.
- Keep current.

**Decision — Drop the AI-generated short-circuit.**

**Implementation notes.**
- Move the `agent_ratio > 0.4` check to the bottom of the cascade, below clone/mock/test/structural/process rules.
- Defer the broader "emit all matching findings" redesign to a later milestone — the immediate damage is masking, not lack of coverage.
- Add a regression test that asserts a heavily-mocked, high-clone, AI-authored scope emits the CLONE or HEAVY MOCKING advice, not the AI GENERATED one.

---

## 5. Agent / bot commit detection

**Problem.** `vcs.rs:52-85` uses substring match on `%an` and `%s`. False-positives on "Claude Martin"-style names; `Co-Authored-By:` trailers are in commit body but `--format` only captures `%s`, so the `co-authored-by: *bot` signature is unreachable; `[bot]` suffix convention used by GitHub Apps (`dependabot[bot]`) is missed.

**Alternatives considered.**
- Trailer-only.
- Config-driven regex list.
- Drop agent detection entirely.
- Allowlist of email/name patterns + trailer parsing.

**Decision — Allowlist of email/name patterns from author + trailers.**

```text
is_bot = author.ends_with("[bot]")
      || email_matches(r"\d+\+\w+\[bot\]@users\.noreply\.github\.com$")
      || trailers.any(|t| t.key == "Co-Authored-By" && t.value.ends_with("[bot]>"))
      || trailers.any(|t| t.key == "Generated-By")
```

**Implementation notes.**
- Switch `git log --format` from `COMMIT|%H|%ct|%an|%s` to a multi-line record using `%B` (full body) with a non-text record separator. Use a sentinel like `--format=%x00COMMIT%x00%H%x00%ct%x00%an%x00%ae%x00%B%x00` and split on `\0`.
- Add a per-commit trailer parser (lines matching `^[A-Z][A-Za-z-]+: `).
- Banned: substring match on "claude" / "gemini" / "copilot" anywhere in author or message. Strip these from the signature list.
- Optional: surface a `--agent-rules` config knob with the default rule set inline for users who want to extend.

---

## 6. Trend (Rising Hotspots) and renames

**Problem.** `main.rs:249-258` uses `git show <past>:<current-path>`. File renames break it; scope renames produce phantom-new scopes.

**Alternatives considered.**
- `git log --follow` only (path renames).
- Content-similarity scope matching.
- Both: `--follow` for files + name+kind for scopes.
- Document the limitation.

**Decision — `git log --follow` + match scopes by `(name, kind)`.**

**Implementation notes.**
- Resolve historical path for each file once using `git log --follow --name-only --format= -- <path>` and take the last entry before the trend cutoff.
- Use that historical path with `git show`.
- For each current scope, match the past scope by `(name, kind)`; if not found, `trend_delta = Some(complexity)` (current behavior, but now correct after rename).
- Renamed scopes are not detected; documented as a known limitation. Worth a follow-up using token-shingle similarity if it becomes a complaint.

---

## 7. Per-axis grain tagging

**Problem.** Scope-level (mock_count, clone_ratio) and file-level (churn, authors, scatter, agent_ratio) metrics are silently mixed in scoring and output.

**Decision — Tag each axis with its grain in JSON/SARIF output.**

**Implementation notes.**
- Extend `RiskReport` with per-axis grain metadata, or attach a `grain: "file" | "scope"` next to each metric in the serialized form.
- Table output is unaffected (visual clutter).
- With `--deep` (§2), process metrics report `grain: "scope"`.

---

## 8. Clone detection determinism & collision safety

**Problem.** `clone_engine.rs:60` reads `locations[0]` without verifying token equality; hash collisions silently merge unrelated clones. Parallel insertion order makes "first author" non-deterministic across runs.

**Alternatives considered.**
- Two-pass: parallel hash + serial canonicalize.
- Store full token windows + verify on report.
- Sort input files before parallel pass.
- Cryptographic hash, no collision check.

**Decision — Two-pass: parallel hash, serial canonicalize.**

**Implementation notes.**
- Pass 1 (parallel, current rayon walk): every file emits `(hash, location, token_window_snapshot)` into a `DashMap<u64, Vec<Entry>>`. Token window stored as the slice of hashed-and-anonymized tokens (small).
- Pass 2 (serial): for each hash bucket, sort entries by `(file_path, start_line)`, group by exact token-window equality, emit a `CloneMatch` between the canonical first entry and the rest.
- Deterministic across runs.
- Collision-safe: distinct token sequences sharing a hash become distinct groups.

---

## 9. Clone-detection configuration

**Problem.** `min_tokens` is hardcoded to 30 in `main.rs:83`.

**Decision — CLI flag + config key.**

**Implementation notes.**
- CLI: `--clone-min-tokens N` (default 30).
- Config: `[clone] min_tokens = N`. CLI overrides config.
- Mention in `--help` that lower values catch more shallow clones but increase noise.

---

## 10. Clone match details in JSON / SARIF

**Problem.** `lib.rs:53-82` manually implements `Serialize` for `RiskReport` and omits `clone_matches`, despite `CloneMatch` deriving `Serialize`. CI tools and downstream readers can't drill into clones.

**Decision — Include `clone_matches` in JSON; summarize in SARIF.**

**Implementation notes.**
- Drop the manual `Serialize` impl and use derive with `#[serde(skip)]` only where a field is intentionally excluded.
- Include `clone_matches` fully in JSON.
- SARIF: emit each `CloneMatch` as a `relatedLocations` entry under the result.
- Table output remains unchanged.

---

## 11. Halstead noise

**Problem.** `analyzer.rs:341-370` builds vocabulary from leaf text including identifiers and literals. Variable renames and formatting move the score, generating false trend deltas.

**Alternatives considered.**
- Replace with node-kind classification.
- Drop Halstead, lean on cyclomatic.
- Distinct-kinds-only.
- Keep behind `--experimental-halstead`.

**Decision — Replace with node-kind classification.**

**Implementation notes.**
- Per-language operator/operand kind sets. For Rust: operators include `+`, `*`, `&&`, `||`, `==`, `!=`, `<`, `>`, control-flow keywords (`if`, `for`, `while`, `match`). Operands include `identifier`, `integer_literal`, `string_literal`, etc.
- Vocabulary = set of distinct (operator_kind | operand_kind), not text.
- `n1 = distinct_operators`, `n2 = distinct_operands`, `N1`, `N2` counted as totals.
- Compute classical Halstead Volume `(N1 + N2) * log2(n1 + n2)`.
- Languages without a kind map fall back to the current text-based approach (with a `halstead_method: "text"` tag in output) until added.

---

## 12. NaN / Inf safety

**Problem.** `reports.sort_by(... partial_cmp(...).unwrap())` (main.rs:319) panics on NaN. Any non-finite intermediate from `calculate_risk` propagates.

**Decision — `f64::total_cmp` for sort + filter non-finite at compute time.**

**Implementation notes.**
- Replace `partial_cmp(...).unwrap()` with `b.risk_score.total_cmp(&a.risk_score)`.
- In `calculate_risk`, if any of `structural_risk`, `process_risk`, `stability_factor`, or `risk_score` is non-finite, return `0.0` and a `RiskProfile { structural: 0.0, process: 0.0, stability: 0.0 }`, and emit a single `eprintln!` per file noting the degenerate scope.
- Add a `DEGENERATE` advice template so the row is visible.

---

## 13. Testing strategy

**Problem.** One test (`tests/coverage_tests.rs`, ~30 lines) covering path normalization. Scoring, agent detection, clone detection, tree-sitter capture mapping, trend, coupling — all untested.

**Alternatives considered.**
- Unit only.
- Property tests via proptest.
- Integration only with checked-in repo.
- Golden-file integration + unit.

**Decision — Golden-file integration tests + unit tests on scoring.**

**Implementation notes.**
- `tests/fixtures/` with 5-10 small repos (small Git histories, mixed languages). Each fixture has an expected `expected.json`.
- Integration test walks each fixture, runs the analyzer, asserts JSON output matches snapshot (with float tolerance).
- Unit tests cover:
  - `calculate_risk` monotonicity in each input.
  - Agent detection: name-suffix `[bot]`, email pattern, trailer, banned substring "Claude Martin".
  - Clone canonicalization: parallel-vs-serial run produces same output.
  - LCOV path normalization with CRLF.
  - Cobertura content-sniff vs filename-sniff.
  - Tree-sitter capture mapping for Rust impl methods, traits, structs.
- Wire into CI via a `cargo test` invocation in pre-commit / GitHub Actions.

---

## 14. Config discovery

**Problem.** `Config::load` (config.rs:50) only reads `args.path/.omnicrap.toml`. Running from a subdir of a configured repo silently uses defaults.

**Decision — Walk up to git root or `$HOME`.**

**Implementation notes.**
- Search order: `args.path/.omnicrap.toml`, then parents until we find one, or until we hit a `.git` directory, or `$HOME`.
- The first file found wins (no merging across levels).
- If we hit `$HOME`, fall back to defaults (do not read `~/.omnicrap.toml` here — see global config below).
- No global config in this milestone; users wanting org-wide defaults can symlink.

---

## 15. Per-path / per-language weight overrides

**Problem.** Weights are global. Tests can't be deprioritized except by ignoring them entirely.

**Decision — TOML `[[overrides]]` sections by glob.**

```toml
[[overrides]]
path = "tests/**"
weights = { structural = 0.5, process = 0.0 }

[[overrides]]
path = "vendor/**"
weights = { structural = 0.0, process = 0.0, stability = 1.0 }
```

**Implementation notes.**
- Globs use the `ignore` crate's gitignore semantics (already a dependency).
- First matching override wins; if no override, fall back to top-level `[weights]`.
- Override is per-file, applied before scoring each scope in that file.
- Documented with examples in README.

---

## 16. File-size cutoff + color / TTY

**Problem.** Hardcoded 1MB skip (main.rs:201) silently drops files; ANSI codes (main.rs:380) emitted regardless of TTY.

**Decision — Configurable cutoff + auto-detect TTY (+ `NO_COLOR`).**

**Implementation notes.**
- CLI: `--max-file-size BYTES` (default 1_000_000). Config: `max_file_size = ...`.
- When a file is skipped due to size, emit one `eprintln!` per file with the path and size.
- Detect interactive output via `std::io::IsTerminal` on stdout.
- Respect `NO_COLOR` env var (always disables color when set, regardless of TTY).
- Optional `--color always|auto|never` flag.

---

## 17. `languages.json` loading

**Problem.** `LanguageDatabase::new()` (languages.rs:31) parses 148KB JSON eagerly on every relevant engine construction.

**Decision — `OnceLock<&'static LanguageDatabase>` + lazy on first use.**

**Implementation notes.**
- Single global `OnceLock`. First call to `LanguageDatabase::get()` parses; subsequent calls return the cached reference.
- Removes the parse cost for runs that touch only TS-supported languages and no stats output.
- No external API change for `RegexEngine::new()` / `StatsEngine::new()`.

---

## 18. LCOV CR handling + Cobertura over-broad match

**Problem.** `coverage/lcov.rs:29` (`current_file = line[3..].to_string()`) leaves trailing `\r` on CRLF reports. `coverage/cobertura.rs:60` (`can_parse`) accepts any `.xml`.

**Decision — Trim CR/whitespace in LCOV + content-sniff Cobertura.**

**Implementation notes.**
- LCOV: `current_file = line[3..].trim().to_string()` — trims both CR and surrounding whitespace.
- Cobertura: open and read the first ~512 bytes; require both `<coverage` and `<packages` to appear. Filename-based heuristic kept as a tiebreaker only when the sniff is ambiguous.
- Same content-sniff approach for LCOV: confirm the file starts with `TN:`, `SF:`, or `SF:` after BOM stripping.

---

## 19. Tree-sitter Parser + Query caching

**Problem.** `analyzer.rs:82-88` creates a new `Parser` and compiles a new `Query` per file. Under rayon, this is per-file per-worker. Query compilation is the expensive step.

**Decision — `thread_local!` cache of `(language → (Parser, Query))`.**

**Implementation notes.**
- Each rayon worker keeps a `RefCell<HashMap<&'static str, (Parser, Query)>>`.
- On `analyze()`, look up by language name; insert if absent.
- Parser is reused (`parser.parse()` doesn't mutate state across calls).
- Query is cloned per use only if grammar requires it; the `Query` itself is reusable across parses for the same grammar.
- No global lock contention.

---

## 20. Dead modules (`forge.rs`, `stats.rs`) + SARIF URL

**Problem.** `forge.rs` (traits with no implementations, never called) and `stats.rs` (`StatsEngine` never instantiated) compile but ship nothing. SARIF `informationUri` points to a non-existent repo (`https://github.com/omni-crap/omni-crap`).

**Decision — Wire `stats.rs` into the pipeline; mark `forge` as work-in-progress.**

**Implementation notes.**
- `stats.rs`:
  - Surface line counts (code / comments / blanks) per file in JSON output.
  - Optionally use the `code` count as the denominator for relative complexity in the structural axis (alternative to raw counts).
  - Add a `--stats` flag to print a roll-up table.
- `forge.rs`:
  - Gate behind a Cargo feature: `[features] wip = []`.
  - Default feature set excludes `wip`. Module compiles in CI under `--features wip` only.
  - Drop the `async-trait` dependency from the default build.
- SARIF URL:
  - Replace `https://github.com/omni-crap/omni-crap` with the actual repo URL once registered. Until then, set `information_uri` to the URL the user controls or omit the field. README's `cargo install --path .` already implies local development; a final URL is a publication-blocker, not a coding one.

---

## 21. Naming

**Problem.** Three spellings used across the project: `crap` (working dir, repo name), `omni-crap` (binary, crate), `.omnicrap.toml` (config file).

**Decision — Keep `omni-crap` everywhere; rename only the working directory and config file.**

**Implementation notes.**
- Config file: `.omnicrap.toml` → `.omni-crap.toml` (matches the binary).
- Working directory: `crap` → `omni-crap` (or move into a properly named repo when publishing).
- Crate name and binary stay `omni-crap`.
- Update `Config::load` path constant, README, VISION, and `.gitignore`.
- This avoids breaking the GitHub-URL-shape and the README's existing prose.

---

## Implementation order

Roughly priority-ordered for implementation:

1. NaN safety (§12) and tree-sitter Parser caching (§19) — small, low-risk, immediate quality wins.
2. Coverage parser fixes (§18).
3. Config discovery (§14) and naming (§21).
4. Agent detection rewrite (§5) and trend rename handling (§6) — both touch the same git-log subprocess, batch together.
5. Clone-detection determinism (§8) and clone config (§9). Test surface needed (§13).
6. Hybrid risk model (§1) — requires §11 (Halstead replacement) and §15 (overrides) to be useful.
7. Advice cascade fix (§4), per-axis grain tagging (§7), clone-match serialization (§10).
8. Regex engine indent chunking (§3).
9. Per-scope process metrics under `--deep` (§2).
10. File-size cutoff + TTY detection (§16), languages lazy load (§17).
11. Stats wiring + forge feature flag (§20).
12. Testing harness, fixtures, golden files (§13) — landed alongside relevant features.
