#!/usr/bin/env python3
"""
Merge cross-symbol leaderboards from two locations.
The old binary saved to target/release/artifacts/, the new one saves to artifacts/.
This script merges both and writes to artifacts/ (the new canonical location).
"""

import json
import sys
from pathlib import Path
from datetime import datetime

def load_leaderboard(path: Path) -> dict:
    """Load a leaderboard JSON file."""
    if not path.exists():
        print(f"File not found: {path}")
        return {"entries": []}

    with open(path, 'r', encoding='utf-8') as f:
        return json.load(f)

def config_hash(entry: dict) -> str:
    """Generate a hash key for deduplication based on strategy and config."""
    config_id = entry.get("config_id", {})
    strategy_type = entry.get("strategy_type", "")
    # Create a consistent string representation
    return f"{strategy_type}:{json.dumps(config_id, sort_keys=True)}"

def merge_leaderboards(old_lb: dict, new_lb: dict) -> dict:
    """Merge two leaderboards, keeping the best entry for each config."""
    merged_entries = {}

    # Process old entries first
    for entry in old_lb.get("entries", []):
        key = config_hash(entry)
        avg_sharpe = entry.get("aggregate_metrics", {}).get("avg_sharpe", 0)
        merged_entries[key] = (avg_sharpe, entry)

    print(f"Loaded {len(merged_entries)} entries from old leaderboard")

    # Process new entries, replacing if better
    replaced = 0
    added = 0
    for entry in new_lb.get("entries", []):
        key = config_hash(entry)
        avg_sharpe = entry.get("aggregate_metrics", {}).get("avg_sharpe", 0)

        if key in merged_entries:
            old_sharpe = merged_entries[key][0]
            if avg_sharpe > old_sharpe:
                merged_entries[key] = (avg_sharpe, entry)
                replaced += 1
        else:
            merged_entries[key] = (avg_sharpe, entry)
            added += 1

    print(f"From new leaderboard: {added} added, {replaced} replaced (better Sharpe)")

    # Sort by avg_sharpe descending and re-rank
    sorted_entries = sorted(
        [entry for _, entry in merged_entries.values()],
        key=lambda e: e.get("aggregate_metrics", {}).get("avg_sharpe", 0),
        reverse=True
    )

    # Update ranks
    for i, entry in enumerate(sorted_entries):
        entry["rank"] = i + 1

    # Use metadata from the newer leaderboard
    result = new_lb.copy()
    result["entries"] = sorted_entries
    result["last_updated"] = datetime.utcnow().isoformat() + "Z"

    # Sum up total iterations and configs tested
    old_iterations = old_lb.get("total_iterations", 0)
    new_iterations = new_lb.get("total_iterations", 0)
    result["total_iterations"] = old_iterations + new_iterations

    old_configs = old_lb.get("total_configs_tested", 0)
    new_configs = new_lb.get("total_configs_tested", 0)
    result["total_configs_tested"] = old_configs + new_configs

    return result

def main():
    project_root = Path(__file__).parent.parent

    old_path = project_root / "target" / "release" / "artifacts" / "cross_symbol_leaderboard.json"
    new_path = project_root / "artifacts" / "cross_symbol_leaderboard.json"
    backup_path = project_root / "artifacts" / "cross_symbol_leaderboard.backup.json"

    print(f"Old leaderboard: {old_path}")
    print(f"New leaderboard: {new_path}")
    print()

    if not old_path.exists():
        print("No old leaderboard to merge.")
        return

    old_lb = load_leaderboard(old_path)
    new_lb = load_leaderboard(new_path)

    print(f"Old entries: {len(old_lb.get('entries', []))}")
    print(f"New entries: {len(new_lb.get('entries', []))}")
    print()

    # Backup the new file before merging
    if new_path.exists():
        import shutil
        shutil.copy2(new_path, backup_path)
        print(f"Backed up new leaderboard to: {backup_path}")

    # Merge
    merged = merge_leaderboards(old_lb, new_lb)

    print(f"\nMerged entries: {len(merged.get('entries', []))}")
    print(f"Total iterations: {merged.get('total_iterations', 0)}")
    print(f"Total configs tested: {merged.get('total_configs_tested', 0)}")

    # Write merged result
    new_path.parent.mkdir(parents=True, exist_ok=True)
    with open(new_path, 'w', encoding='utf-8') as f:
        json.dump(merged, f, indent=2)

    print(f"\nWrote merged leaderboard to: {new_path}")

    # Show unique sessions
    sessions = set()
    for entry in merged.get("entries", []):
        sid = entry.get("session_id")
        if sid:
            sessions.add(sid)
    print(f"Unique sessions in merged leaderboard: {sorted(sessions)}")

if __name__ == "__main__":
    main()
