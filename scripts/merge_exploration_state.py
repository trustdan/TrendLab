#!/usr/bin/env python3
"""
Merge exploration states from two locations.
"""

import json
from pathlib import Path
from datetime import datetime, timezone

def load_exploration_state(path: Path) -> dict:
    """Load an exploration state JSON file."""
    if not path.exists():
        print(f"File not found: {path}")
        return {"coverage": {}, "contributing_sessions": []}

    with open(path, 'r', encoding='utf-8') as f:
        return json.load(f)

def merge_strategy_coverage(old_cov: dict, new_cov: dict) -> dict:
    """Merge coverage for a single strategy."""
    merged = {
        "total_tested": old_cov.get("total_tested", 0) + new_cov.get("total_tested", 0),
        "winners_found": old_cov.get("winners_found", 0) + new_cov.get("winners_found", 0),
        "cells": {}
    }

    # Merge cells
    old_cells = old_cov.get("cells", {})
    new_cells = new_cov.get("cells", {})
    all_keys = set(old_cells.keys()) | set(new_cells.keys())

    for key in all_keys:
        old_cell = old_cells.get(key, {"tested": 0, "winners": 0})
        new_cell = new_cells.get(key, {"tested": 0, "winners": 0})
        merged["cells"][key] = {
            "tested": old_cell.get("tested", 0) + new_cell.get("tested", 0),
            "winners": old_cell.get("winners", 0) + new_cell.get("winners", 0)
        }

    return merged

def merge_exploration_states(old_state: dict, new_state: dict) -> dict:
    """Merge two exploration states."""
    merged = {
        "coverage": {},
        "contributing_sessions": [],
        "last_updated": datetime.now(timezone.utc).isoformat()
    }

    # Merge contributing sessions (deduplicated)
    old_sessions = set(old_state.get("contributing_sessions", []))
    new_sessions = set(new_state.get("contributing_sessions", []))
    merged["contributing_sessions"] = sorted(old_sessions | new_sessions)

    # Merge coverage for each strategy
    old_coverage = old_state.get("coverage", {})
    new_coverage = new_state.get("coverage", {})
    all_strategies = set(old_coverage.keys()) | set(new_coverage.keys())

    for strategy in all_strategies:
        old_cov = old_coverage.get(strategy, {"total_tested": 0, "winners_found": 0, "cells": {}})
        new_cov = new_coverage.get(strategy, {"total_tested": 0, "winners_found": 0, "cells": {}})
        merged["coverage"][strategy] = merge_strategy_coverage(old_cov, new_cov)

    return merged

def main():
    project_root = Path(__file__).parent.parent

    old_path = project_root / "target" / "release" / "artifacts" / "exploration_state.json"
    new_path = project_root / "artifacts" / "exploration_state.json"
    backup_path = project_root / "artifacts" / "exploration_state.backup.json"

    print(f"Old (target): {old_path}")
    print(f"New (artifacts): {new_path}")
    print()

    if not old_path.exists():
        print("No old exploration state to merge.")
        return

    old_state = load_exploration_state(old_path)
    new_state = load_exploration_state(new_path)

    print(f"Old sessions: {old_state.get('contributing_sessions', [])}")
    print(f"New sessions: {new_state.get('contributing_sessions', [])}")
    print()

    # Backup the new file before merging
    if new_path.exists():
        import shutil
        shutil.copy2(new_path, backup_path)
        print(f"Backed up to: {backup_path}")

    merged = merge_exploration_states(old_state, new_state)

    print(f"Merged sessions: {merged.get('contributing_sessions', [])}")

    # Count total tested across all strategies
    total_tested = sum(cov.get("total_tested", 0) for cov in merged["coverage"].values())
    total_winners = sum(cov.get("winners_found", 0) for cov in merged["coverage"].values())
    print(f"Total tested: {total_tested}")
    print(f"Total winners: {total_winners}")

    # Write merged result
    new_path.parent.mkdir(parents=True, exist_ok=True)
    with open(new_path, 'w', encoding='utf-8') as f:
        json.dump(merged, f, indent=2)

    print(f"\nWrote merged exploration state to: {new_path}")

if __name__ == "__main__":
    main()
