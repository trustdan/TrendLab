#!/usr/bin/env python3
"""
Merge per-symbol leaderboards from two locations.
"""

import json
from pathlib import Path
from datetime import datetime, timezone

def load_leaderboard(path: Path) -> dict:
    """Load a leaderboard JSON file."""
    if not path.exists():
        print(f"File not found: {path}")
        return {"entries": []}

    with open(path, 'r', encoding='utf-8') as f:
        return json.load(f)

def config_hash(entry: dict) -> str:
    """Generate a hash key for deduplication."""
    config = entry.get("config", {})
    strategy_type = entry.get("strategy_type", "")
    symbol = entry.get("symbol", "")
    return f"{strategy_type}:{symbol}:{json.dumps(config, sort_keys=True)}"

def merge_leaderboards(old_lb: dict, new_lb: dict) -> dict:
    """Merge two leaderboards, keeping the best entry for each config+symbol."""
    merged_entries = {}

    for entry in old_lb.get("entries", []):
        key = config_hash(entry)
        sharpe = entry.get("metrics", {}).get("sharpe", 0)
        merged_entries[key] = (sharpe, entry)

    print(f"Loaded {len(merged_entries)} entries from old leaderboard")

    replaced = 0
    added = 0
    for entry in new_lb.get("entries", []):
        key = config_hash(entry)
        sharpe = entry.get("metrics", {}).get("sharpe", 0)

        if key in merged_entries:
            old_sharpe = merged_entries[key][0]
            if sharpe > old_sharpe:
                merged_entries[key] = (sharpe, entry)
                replaced += 1
        else:
            merged_entries[key] = (sharpe, entry)
            added += 1

    print(f"From new leaderboard: {added} added, {replaced} replaced")

    sorted_entries = sorted(
        [entry for _, entry in merged_entries.values()],
        key=lambda e: e.get("metrics", {}).get("sharpe", 0),
        reverse=True
    )

    for i, entry in enumerate(sorted_entries):
        entry["rank"] = i + 1

    result = new_lb.copy()
    result["entries"] = sorted_entries
    result["last_updated"] = datetime.now(timezone.utc).isoformat()

    return result

def main():
    project_root = Path(__file__).parent.parent

    old_path = project_root / "target" / "release" / "artifacts" / "leaderboard.json"
    new_path = project_root / "artifacts" / "leaderboard.json"
    backup_path = project_root / "artifacts" / "leaderboard.backup.json"

    print(f"Old: {old_path}")
    print(f"New: {new_path}")
    print()

    if not old_path.exists():
        print("No old leaderboard to merge.")
        return

    old_lb = load_leaderboard(old_path)
    new_lb = load_leaderboard(new_path)

    print(f"Old entries: {len(old_lb.get('entries', []))}")
    print(f"New entries: {len(new_lb.get('entries', []))}")
    print()

    if new_path.exists():
        import shutil
        shutil.copy2(new_path, backup_path)
        print(f"Backed up to: {backup_path}")

    merged = merge_leaderboards(old_lb, new_lb)

    print(f"\nMerged entries: {len(merged.get('entries', []))}")

    new_path.parent.mkdir(parents=True, exist_ok=True)
    with open(new_path, 'w', encoding='utf-8') as f:
        json.dump(merged, f, indent=2)

    print(f"\nWrote: {new_path}")

    # Show unique sessions
    sessions = set()
    for entry in merged.get("entries", []):
        sid = entry.get("session_id")
        if sid:
            sessions.add(sid)
    print(f"Unique sessions: {sorted(sessions)}")

if __name__ == "__main__":
    main()
