---
description: Export StrategyArtifact JSON for a specific run/config (for Pine generation)
argument-hint: [run_id] [config_id]
allowed-tools: Bash(ls:*), Bash(cat:*), Bash(jq:*), Edit
---

Export a StrategyArtifact suitable for generating Pine that matches the selected configuration.

Inputs:
- run_id: $1
- config_id: $2

Reference:
- @.claude/agents/pine-artifact-writer.md
- @CLAUDE.md

## Your tasks
1. Locate the run outputs under `reports/runs/<run_id>/`.
2. Create `artifacts/exports/<run_id>/<config_id>.json` that includes:
   - all parameters
   - fill convention + costs
   - Pine-friendly rule DSL
   - parity test vector window (timestamps + expected entry/exit)
3. Write `artifacts/exports/<run_id>/<config_id>.md` describing how to use it to prompt an LLM.
