//! GUI deprecation handling.
//!
//! The GUI has been deprecated in favor of the TUI. This module handles
//! showing the deprecation message and opening the resurrection roadmap.

use std::io::{self, Write};
use std::process::Command;

/// Path to the resurrection roadmap relative to the project root.
pub const RESURRECTION_ROADMAP: &str = "docs/roadmap-gui-resurrection.md";

/// Display the GUI deprecation message and open the resurrection roadmap.
pub fn handle_gui_deprecation() -> io::Result<()> {
    show_deprecation_message()?;
    open_roadmap_file()?;
    Ok(())
}

fn show_deprecation_message() -> io::Result<()> {
    let mut stdout = io::stdout();

    writeln!(stdout)?;
    writeln!(stdout, "==================================")?;
    writeln!(stdout, "   GUI Mode Deprecated")?;
    writeln!(stdout, "==================================")?;
    writeln!(stdout)?;
    writeln!(stdout, "The TrendLab GUI has been deprecated.")?;
    writeln!(stdout)?;
    writeln!(stdout, "Reasons:")?;
    writeln!(
        stdout,
        "  - Feature gaps (missing YOLO mode, risk profiles, analysis)"
    )?;
    writeln!(
        stdout,
        "  - Architectural mismatch (parallel impl, not TUI wrapper)"
    )?;
    writeln!(stdout, "  - Runtime stability issues")?;
    writeln!(stdout)?;
    writeln!(stdout, "The TUI provides the complete feature set.")?;
    writeln!(stdout, "Use: trendlab --tui")?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "GUI code remains intact for potential resurrection."
    )?;
    writeln!(stdout, "Opening resurrection roadmap...")?;
    writeln!(stdout)?;

    stdout.flush()
}

fn open_roadmap_file() -> io::Result<()> {
    let path = get_roadmap_path()?;

    if !path.exists() {
        writeln!(
            io::stderr(),
            "Warning: Roadmap file not found at {}",
            path.display()
        )?;
        writeln!(
            io::stderr(),
            "See: docs/roadmap-gui-resurrection.md in the project root"
        )?;
        return Ok(());
    }

    open_file_cross_platform(&path)
}

fn get_roadmap_path() -> io::Result<std::path::PathBuf> {
    // Try to find project root by looking for Cargo.toml
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        while let Some(d) = dir {
            let cargo_toml = d.join("Cargo.toml");
            if cargo_toml.exists() {
                return Ok(d.join(RESURRECTION_ROADMAP));
            }
            dir = d.parent();
        }
    }

    // Fallback: relative to current directory
    Ok(std::path::PathBuf::from(RESURRECTION_ROADMAP))
}

/// Open a file with the system default application.
fn open_file_cross_platform(path: &std::path::Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", "", &path.display().to_string()])
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}
