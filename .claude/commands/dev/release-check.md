---
description: Run a full quality gate (fmt, clippy, tests) + produce a release summary
argument-hint: [notes]
allowed-tools: Bash(cargo:*), Bash(git:*), Edit
---

Run the full quality gate and write a summary report.

Notes: $ARGUMENTS

## Context
- Git status: !`git status`
- Current branch: !`git branch --show-current`

## Steps
1. Run:
   - !`cargo fmt`
   - !`cargo test`
   - !`cargo clippy --all-targets --all-features -D warnings`
2. If anything fails: fix it (minimal changes).
3. Produce `reports/dev/release-check-<timestamp>.md` including:
   - what ran
   - pass/fail
   - key changes since last check (use `git log --oneline -20`)
   - any known limitations / TODOs
