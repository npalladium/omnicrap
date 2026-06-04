# VISION: Omni-Crap

## The "Fog of Development"
In modern software engineering, we are drowning in data but starving for insight. We have linters, coverage trackers, and git history, but they live in silos. A developer looking at a 500-line function doesn't know if it's a stable legacy core or a high-churn bug factory that hasn't been tested in months.

**Omni-Crap** is the "Risk Compass" for the modern codebase.

## Mission
Our mission is to provide a language-agnostic, zero-config tool that identifies the most dangerous areas of a repository by correlating **Complexity**, **Coverage**, and **Churn**.

## Core Philosophy
1. **Agnostic by Default:** A tool should not care if you write in Rust, Python, or COBOL. If the data is there, we should surface it.
2. **The "High-Risk" Triad:** 
    - **Complexity** is the cost.
    - **Coverage** is the safety net.
    - **Churn** is the frequency of impact.
    Our goal is to find where the cost is high, the safety net is missing, and the impact is frequent.
3. **Semantic Precision:** By utilizing **Tree-sitter**, we move beyond simple line-counting. We understand the structure of the code, allowing us to map risk directly to functions and methods, and track "Semantic Churn"—knowing not just that a file changed, but exactly which logic was touched.
4. **Action, Not Just Observation:** We don't just want to show a score; we want to tell a Tech Lead where to assign the next senior engineer and tell a QA where to write the next integration test.

## The Future
Omni-Crap aims to become the standard "Health Check" in every CI/CD pipeline, moving beyond the binary "pass/fail" of coverage and into the nuanced world of **Risk-Based Engineering**.
