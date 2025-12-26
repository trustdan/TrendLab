---
description: Run a parameter sweep and write results to Parquet/CSV + summary markdown
argument-hint: [strategy_id] [universe_or_csv] [start_yyyy-mm-dd] [end_yyyy-mm-dd]
allowed-tools: Bash(cargo:*), Bash(ls:*), Bash(cat:*), Bash(head:*), Edit
---

Run a sweep and produce artifacts that are easy to compare.

Inputs:
- strategy_id: $1
- universe: $2   (ticker list file, or a single ticker)
- start: $3
- end: $4

Reference:
- @CLAUDE.md
- @crates/trendlab-cli/src/main.rs (or CLI entry)
- @.claude/agents/metrics-analyst.md
- @.claude/agents/polars-expert.md

## Context
- Show CLI help: !`cargo run -p trendlab-cli -- --help || true`
- Show data folder: !`ls -la data || true`

## Your tasks
1. Implement (or complete) the CLI subcommand: `trendlab-cli sweep ...`
2. Define a simple sweep grid (hardcode a starter grid if none exists yet).
3. Run the sweep once on the provided universe.
4. Write outputs:
   - `reports/runs/<run_id>/trades.parquet`
   - `reports/runs/<run_id>/equity.parquet`
   - `reports/runs/<run_id>/metrics.parquet` (+ optional CSV)
   - `reports/runs/<run_id>/summary.md` (top 10 configs + notes)
5. Ensure reproducibility: store config JSON used for the run.

## Constraints
- Deterministic fills (default next-open).
- Costs must be explicit.
- Don’t add fancy execution simulation in phase 1.
