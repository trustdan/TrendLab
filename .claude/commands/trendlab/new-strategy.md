---
description: Create a new strategy spec + BDD scenarios + implementation skeleton
argument-hint: [strategy_id]
allowed-tools: Bash(ls:*), Bash(rg:*), Edit
---

Create a new strategy end-to-end: spec → BDD → code skeleton.

Strategy id: $1

Reference project intent and conventions:
- @CLAUDE.md
- @.claude/agents/trend-following-expert.md
- @.claude/agents/bdd-test-author.md
- @.claude/agents/rust-architect.md
- @.claude/agents/pine-artifact-writer.md

## Your tasks
1. Delegate the strategy design to the trend-following expert agent.
2. Translate the design into:
   - `features/<strategy_id>.feature` with scenarios that prevent lookahead and pin the fill convention.
3. Create skeleton Rust module(s) under `crates/trendlab-core/src/strategies/<strategy_id>.rs`:
   - parameter struct
   - signal generation function signatures
   - artifact export stub
4. Add the strategy to CLI as a selectable option (even if implementation is stubbed).
5. Create a `StrategyArtifact` JSON example in `artifacts/examples/<strategy_id>.json` that the Pine generator could consume later.

## Output expectations
- You MUST create/modify files (don’t just describe).
- Keep the first version minimal and test-driven.
