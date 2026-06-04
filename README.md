# Omni-Crap: The Semantic Risk Radar 🎯

Omni-Crap is a language-agnostic behavioral forensics tool that identifies "High Risk" hotspots in your codebase. It goes beyond static analysis by correlating **Complexity**, **Test Coverage**, and **Git Churn**, augmented with behavioral metrics derived from *Software Design X-Rays*.

## Features

- **Semantic Complexity (Arborium):** Uses [Arborium](https://github.com/bearcove/arborium) (powered by Tree-sitter) to precisely identify functions and calculate cyclomatic complexity natively for 15+ major languages (Rust, Python, JS/TS, C/C++, Java, Go, Swift, etc.).
- **Polyglot Fallback (SCC):** Gracefully degrades to a robust regex-based heuristic engine for 250+ other languages, utilizing definitions from the industry-standard `scc`.
- **Coverage Integration:** Parses standard `lcov.info` and `cobertura.xml` reports to map test coverage directly to identified code blocks.
- **Behavioral Forensics (SDX):**
  - **Change Churn:** Tracks how often files change to identify volatile logic.
  - **Author Diffusion:** Penalizes files edited by many minor contributors (Diffusion of Responsibility).
  - **Code Age (Stability Discount):** Discounts the risk of old, untouched code ("your best bug fix is time").
  - **Rising Hotspots:** Calculates historical complexity trends to predict future maintenance pain.
  - **Change Coupling:** Detects hidden architectural dependencies by finding files that frequently co-change.

## Installation

```bash
cargo install --path .
```

## Usage

**Basic Analysis (Current Directory):**
```bash
omni-crap .
```

**Full Risk Radar (Coverage + Threshold):**
```bash
omni-crap . --coverage ./coverage/lcov.info --threshold 15.0
```

**Rising Hotspots (Complexity Trend over 1 month):**
```bash
omni-crap . --trend
```

**Detect Hidden Dependencies (Change Coupling):**
```bash
omni-crap . --coupling
```

**CI/CD Integration (JSON Output):**
```bash
omni-crap . --format json
```

## The Risk Formula

Omni-Crap calculates a unified risk score using the following formula:

```text
Structural Risk = ((Complexity + Halstead/100)² * (1 - Coverage)³ + StructuralWeight) * MockPenalty * ClonePenalty
Process Risk = ChurnMultiplier * AuthorMultiplier * ScatterPenalty * AgentPenalty
Final Score = (Structural Risk * Process Risk * StabilityFactor) / Baseline
```

- **Complexity:** Number of logic branches (e.g., `if`, `for`, `case`, `&&`).
- **Halstead:** Complexity derived from unique operators and operands (volume).
- **Coverage:** Percentage of the function exercised by tests (0.0 to 1.0).
- **Churn:** Log-scaled multiplier based on commit frequency.
- **Authors:** Linear penalty based on the number of unique contributors.
- **Scatter:** Penalty for files changed as part of large, cross-cutting commits.
- **Agent Ratio:** Penalty for code predominantly authored by AI agents or bots.
- **Stability Factor:** Discounts the risk based on code age and "redundancy" (boilerplate).
- **Mock/Clone Penalty:** Penalties for heavy mocking in tests or high duplication ratios.

## Attribution & Inspiration

Omni-Crap stands on the shoulders of giants. This tool is a synthesis of concepts and heuristics from the following projects and authors:

- 📚 **[Software Design X-Rays](https://software-design-xrays.org/) by Adam Tornhill:** The entire behavioral forensics philosophy—including Change Coupling, Code Age/Stability, Author Diffusion (Fractal Value), and Rising Hotspots—is derived from Adam's groundbreaking work on treating source code as a social liability.
- 🦀 **[cargo-crap](https://github.com/minikin/cargo-crap) by Oleksandr Prokhorenko:** The foundational implementation of the CRAP (Change Risk Anti-Patterns) formula in Rust, originally defined by Alberto Savoia and Bob Evans.
- 🦎 **[Lizard](https://github.com/terryyin/lizard) by Terry Yin:** Omni-Crap absorbs Lizard's highly refined, per-language cyclomatic complexity heuristics (e.g., handling Python's `elif`, Ruby's `elsif`, and JS `??`).
- ⚡ **[SCC (Sloc, Cloc and Code)](https://github.com/boyter/scc) by Ben Boyter:** The comprehensive `languages.json` database used for our regex-fallback engine and line-counting state machine was extracted from SCC, enabling support for 250+ languages.
- 🌳 **[Arborium](https://github.com/bearcove/arborium) by Amos Wenger:** The "batteries-included" Tree-sitter library that powers our high-precision semantic engine without the native linking headaches.
