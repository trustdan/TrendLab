use std::path::PathBuf;
use trendlab_core::leaderboard::CrossSymbolLeaderboard;

fn main() {
    // Show current exe for reference
    if let Ok(exe) = std::env::current_exe() {
        println!("Current exe: {:?}", exe);
        if let Some(parent) = exe.parent() {
            println!("Parent dir: {:?}", parent);
            println!(
                "Parent ends_with 'release': {}",
                parent.ends_with("release")
            );
            println!(
                "Parent ends_with 'examples': {}",
                parent.ends_with("examples")
            );
        }
    }

    // Test artifacts_dir
    let artifacts = trendlab_core::artifacts_dir();
    println!("\nartifacts_dir: {:?}", artifacts);
    println!("artifacts exists: {}", artifacts.exists());

    // Also test what the TUI path would resolve to
    let tui_exe = PathBuf::from(r"c:\Users\Dan\TrendLab\target\release\trendlab-tui.exe");
    let tui_parent = tui_exe.parent().unwrap();
    println!("\nTUI exe: {:?}", tui_exe);
    println!("TUI parent: {:?}", tui_parent);
    println!(
        "TUI parent ends_with 'release': {}",
        tui_parent.ends_with("release")
    );

    // Manually compute what artifacts_dir would return for TUI
    if tui_parent.ends_with("release") || tui_parent.ends_with("debug") {
        if let Some(target) = tui_parent.parent() {
            if let Some(project_root) = target.parent() {
                let tui_artifacts = project_root.join("artifacts");
                println!("TUI would use: {:?}", tui_artifacts);
                println!("TUI artifacts exists: {}", tui_artifacts.exists());
            }
        }
    }

    // Try to load cross-symbol leaderboard
    let cross_symbol_path = artifacts.join("cross_symbol_leaderboard.json");
    println!("\nCross-symbol path: {:?}", cross_symbol_path);
    println!("File exists: {}", cross_symbol_path.exists());

    if cross_symbol_path.exists() {
        match CrossSymbolLeaderboard::load(&cross_symbol_path) {
            Ok(lb) => {
                println!("\nLoaded successfully!");
                println!("Entries: {}", lb.entries.len());
                let sessions: std::collections::HashSet<_> = lb
                    .entries
                    .iter()
                    .filter_map(|e| e.session_id.as_ref())
                    .collect();
                println!("Unique sessions: {:?}", sessions);
            }
            Err(e) => {
                println!("\nFailed to load: {}", e);
            }
        }
    } else {
        println!("File does not exist!");
    }

    // Also try loading from the TUI path
    let tui_artifacts = PathBuf::from(r"c:\Users\Dan\TrendLab\artifacts");
    let tui_cross_symbol_path = tui_artifacts.join("cross_symbol_leaderboard.json");
    println!("\n--- Testing TUI path directly ---");
    println!("TUI cross-symbol path: {:?}", tui_cross_symbol_path);
    println!("File exists: {}", tui_cross_symbol_path.exists());

    if tui_cross_symbol_path.exists() {
        match CrossSymbolLeaderboard::load(&tui_cross_symbol_path) {
            Ok(lb) => {
                println!("\nLoaded from TUI path successfully!");
                println!("Entries: {}", lb.entries.len());
                let sessions: std::collections::HashSet<_> = lb
                    .entries
                    .iter()
                    .filter_map(|e| e.session_id.as_ref())
                    .collect();
                println!("Unique sessions: {:?}", sessions);
            }
            Err(e) => {
                println!("\nFailed to load from TUI path: {}", e);
            }
        }
    }
}
