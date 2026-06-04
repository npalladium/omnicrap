# omni-crap

A code-risk radar that ranks every function and method in your codebase by a **hybrid Z-score** combining structural complexity, Git process history, and stability signals. Points you at the hotspots most likely to cause bugs or slow down future work.

## How it works

For each scope (function, method, class, module), omni-crap computes three axes:

```
structural = z(complexity × (1 + (1−coverage)³)) + z(halstead) + z(clone_ratio)
process    = z(churn) + z(authors) + z(scatter)
stability  = z(redundancy) + z(age_months)

risk = (1 + structural) × (1 + process) / max(0.1, 1 + stability)
```

All z-scores are computed against the current run's distribution, so the output is always a percentile rank (`p92`, `p100`) — comparable across repos, meaningful without calibration.

The classical CRAP and CCRAP formulas are also available (`--crap`, `--ccrap`) for teams that already use them.

## Features

**Structural analysis** (Arborium / Tree-sitter)
- Cyclomatic complexity per function/method for 15+ languages: Rust, Python, JS/TS, Go, Java, C/C++, C#, Ruby, Swift, Kotlin, Scala, Bash, PHP
- Halstead volume using node-kind classification (rename-invariant)
- Clone/duplicate detection with exact token matching and collision-safe canonicalization

**Regex fallback** for 250+ additional languages via the [scc](https://github.com/boyter/scc) language database — indent-based and brace-based scope extraction.

**Coverage integration** — lcov and Cobertura XML, content-sniffed (not filename-guessed).

**VCS / behavioral metrics** (Git):
- File churn, author count, scatter (avg commit size), code age
- AI/bot commit detection via author name `[bot]` suffix, known AI emails, `Co-Authored-By` trailers, `Generated-By` trailers
- `--deep`: per-scope churn via `git log -L :scope:file`, replacing file-level averages for the top-N riskiest scopes
- `--trend`: complexity delta vs. 1 month ago, with rename tracking via `git log --follow`
- `--coupling`: files that frequently co-change (hidden architectural dependencies)

**Generated/vendored file classifier** — auto-excludes or flags lock files, minified JS, vendored directories, proto generated code, etc. Configurable via `.omni-crap.toml`.

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# Analyze current directory (default: hybrid Z-score model)
omni-crap

# Include test coverage
omni-crap --coverage coverage/lcov.info

# JSON for CI
omni-crap --format json > report.json

# SARIF for GitHub Code Scanning
omni-crap --format sarif > results.sarif

# Only score files changed in the current diff (fast CI gate)
omni-crap --diff

# Deepen churn data for top 50 scopes using per-function git blame
omni-crap --deep

# Rising hotspots over 6 months
omni-crap --trend --since "180 days"

# Change-coupling report
omni-crap --coupling

# Line-count roll-up
omni-crap --stats

# Show all detected generated/vendored files
omni-crap --generated-report

# Classical CRAP score
omni-crap --crap --coverage coverage/lcov.info

# ASCII output for CI logs (also respects NO_COLOR env var)
omni-crap --color never
```

## Configuration

Drop `.omni-crap.toml` anywhere in your repo (walk-up discovery stops at `.git`):

```toml
# Risk score threshold — scopes below this are hidden from table output
threshold = 10.0

# Git history window for churn
# (overridden by --since)

# Skip files/paths matching these substrings
ignore = ["migrations/", "generated/"]

# Max file size to analyze (bytes)
max_file_size = 1_000_000

# Minimum token window for clone detection
[clone]
min_tokens = 30

# Global risk axis weights (multiplicative on z-scores)
[weights]
structural = 1.0
process    = 1.0
stability  = 1.0

# Per-path weight overrides (gitignore glob semantics, first match wins)
[[overrides]]
path = "tests/**"
weights = { structural = 0.5, process = 0.0 }

[[overrides]]
path = "vendor/**"
weights = { structural = 0.0, process = 0.0, stability = 1.0 }

# Classifier: add patterns, suppress built-ins, or change actions
[classifier]
generated = ["src/proto/", "_pb.go"]   # extra generated patterns
vendored  = ["third_party/"]
suppress  = ["Cargo.lock"]             # don't exclude this built-in pattern
generated_action = "exclude"           # or "flag"
vendored_action  = "flag"              # or "exclude"
```

## Output columns

| Column   | Description |
|----------|-------------|
| Scope    | Function / method / class / module name |
| Kind     | Function, Class, Module — with test kind if detected (Unit, Integration, E2E) |
| Mocks    | Mock call count in test scopes |
| Clone    | Percentage of tokens duplicated elsewhere |
| Comp     | Cyclomatic complexity |
| Struct   | Structural axis z-score (complexity + halstead + clones) |
| Process  | Process axis z-score (churn + authors + scatter) |
| Stable   | Stability axis z-score (redundancy + age) |
| Risk     | Final score and percentile rank |

## Advice codes

| Code | Meaning |
|------|---------|
| `CLONE` | High duplication — refactor into shared helper |
| `HEAVY MOCKING` | Test relies on many mocks — consider real dependencies |
| `TESTING DEBT` | High structural risk, low coverage — write tests first |
| `COMPLEXITY` | Break this scope into smaller parts |
| `HOTSPOT` | High churn — ensure changes are strictly necessary |
| `KNOWLEDGE SILO` | One author, high volatility — broaden review |
| `BOILERPLATE` | High redundancy — consolidate |
| `AI GENERATED` | Majority AI-authored and high risk — review for correctness |
| `MAINTAIN` | No acute signal — watch for rising complexity |

## Dogfooding

Running omni-crap against itself (`--no-vcs`):

- `src/analyzer.rs` — p100, CLONE (47%). `register_clones` and `analyze` share near-identical language query match arms. Real duplication.
- `src/main.rs:main` — p99, complexity 102. Monolithic entry point; candidate for decomposition.
- `src/classifier.rs` — p98, TESTING DEBT (coverage = 0, complexity = 44). Classifier logic is untested.

## Attribution

- **[Software Design X-Rays](https://software-design-xrays.org/) by Adam Tornhill** — behavioral forensics philosophy: change coupling, code age, author diffusion, rising hotspots.
- **[cargo-crap](https://github.com/minikin/cargo-crap) by Oleksandr Prokhorenko** — CRAP formula foundation.
- **[Lizard](https://github.com/terryyin/lizard) by Terry Yin** — per-language cyclomatic complexity heuristics.
- **[SCC](https://github.com/boyter/scc) by Ben Boyter** — `languages.json` database powering the regex fallback and line-count engine.
- **[Arborium](https://github.com/bearcove/arborium) by Amos Wenger** — batteries-included Tree-sitter bindings.
