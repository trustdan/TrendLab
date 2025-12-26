---
description: Create a Pine parity checklist + test vector snippet for validating a generated Pine script
argument-hint: [artifact_json_path]
allowed-tools: Bash(cat:*), Bash(head:*), Edit
---

Given the StrategyArtifact at: $1

Reference:
- @.claude/agents/pine-artifact-writer.md

## Your tasks
1. Produce `artifacts/parity/<artifact_basename>-parity.md` containing:
   - what “must match” (indicator values, entry/exit booleans, fill convention)
   - a small table of parity test vectors (timestamp, indicator(s), expected entry, expected exit)
2. Include a template prompt that instructs an LLM to generate Pine and explicitly validate against the parity vector.
