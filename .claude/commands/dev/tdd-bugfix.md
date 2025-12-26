---
description: TDD bugfix workflow: reproduce → failing test → fix → verify
argument-hint: [bug_description]
allowed-tools: Bash(rg:*), Bash(cargo:*), Bash(git:*), Edit
---

Bug to fix: $ARGUMENTS

Reference:
- @.claude/agents/bdd-test-author.md
- @.claude/agents/rust-architect.md
- @CLAUDE.md

## Your tasks
1. Reproduce the bug with the smallest deterministic fixture.
2. Write a failing test FIRST (BDD preferred; unit test ok).
3. Implement the fix without weakening the test.
4. Run: `cargo fmt`, `cargo test`, `cargo clippy --all-targets --all-features -D warnings`
5. Summarize root cause + fix in `reports/dev/bugfix-<date>.md`.
