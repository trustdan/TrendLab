---
description: Add a new performance metric end-to-end (calc + schema + tests + docs)
argument-hint: [metric_name]
allowed-tools: Bash(rg:*), Bash(cargo:*), Edit
---

Add metric: $1

Reference:
- @.claude/agents/metrics-analyst.md
- @.claude/agents/bdd-test-author.md
- @CLAUDE.md

## Your tasks
1. Define the metric precisely (formula + annualization or normalization rules).
2. Add it to the metrics output schema (Parquet/CSV columns).
3. Implement the calculation in core.
4. Add tests (BDD scenario or unit tests with deterministic fixtures).
5. Update `README.md` or `docs/metrics.md` with the new metric definition.
