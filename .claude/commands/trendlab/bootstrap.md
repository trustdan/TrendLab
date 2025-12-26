---
description: Bootstrap the Rust + Polars workspace skeleton for TrendLab
argument-hint: [workspace-name]
allowed-tools: Bash(mkdir:*), Bash(ls:*), Bash(git:*), Bash(cargo:*), Edit
---

You are bootstrapping a new research-grade trend-following backtesting lab in Rust + Polars.

Workspace name: $ARGUMENTS (if empty, use `trendlab`)

## Context (run these)
- Current directory listing: !`ls -la`
- Git status: !`git status --porcelain=v1 || true`

## Your tasks
1. Create the recommended repo skeleton for this project:
   - `crates/trendlab-core`
   - `crates/trendlab-cli`
   - `data/raw/`, `data/parquet/`, `reports/`, `artifacts/`, `fixtures/`
   - `.claude/agents/` and `.claude/commands/` already exist; do not overwrite.
2. Initialize Cargo workspace with the above crates.
3. Add baseline dependencies (minimal, don’t overdo):
   - Polars (for dataframes)
   - clap (CLI)
   - serde + serde_json (artifacts/reports)
   - thiserror/anyhow (errors)
4. Create a `README.md` with:
   - one-paragraph project description
   - how to fetch Yahoo daily data (planned), run a sweep (planned), export report (planned)
5. Add a first-pass `trendlab-cli --help` that shows subcommands placeholders:
   - `data refresh-yahoo`
   - `sweep`
   - `report`
   - `artifact export`

## Rules
- Keep it simple: deterministic research backtester, not a live trading engine.
- After changes: run `cargo fmt` and `cargo test`.
