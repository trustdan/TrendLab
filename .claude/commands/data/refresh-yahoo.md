---
description: Fetch/refresh daily bars from Yahoo Finance and normalize to Parquet cache
argument-hint: [tickers_or_file] [start_yyyy-mm-dd] [end_yyyy-mm-dd]
allowed-tools: Bash(cargo:*), Bash(ls:*), Bash(cat:*), Bash(head:*), Edit
---

Refresh daily OHLCV data from Yahoo and write canonical Parquet.

Inputs:
- tickers_or_file: $1
- start: $2
- end: $3

Reference:
- @.claude/agents/data-provider-expert.md
- @.claude/agents/polars-expert.md
- @CLAUDE.md

## Your tasks
1. Implement/complete `trendlab-cli data refresh-yahoo ...`
2. Use a cached raw layer (data/raw/yahoo/...) AND a normalized Parquet layer (data/parquet/1d/...).
3. Add data-quality checks:
   - duplicates
   - missing dates (market calendar-aware if feasible; otherwise report gaps)
4. Emit a small report to `reports/data/refresh-<timestamp>.md`.

## Notes
- If a ticker fails, continue and report failures.
- Do not refetch if cached and complete unless a `--force` flag is given.
