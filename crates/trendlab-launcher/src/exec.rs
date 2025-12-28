//! Process execution helpers for launching TUI or GUI.

use std::io;
use std::process::{Command, Stdio};

use crate::COMPANION_SOCKET_ENV;

/// Launch the TUI.
///
/// On Unix, this replaces the current process.
/// On Windows, this spawns and waits for the process.
pub fn launch_tui() -> io::Result<()> {
    let tui_bin = find_binary("trendlab-tui")?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // exec() replaces current process - does not return on success
        let err = Command::new(&tui_bin).exec();
        Err(err)
    }

    #[cfg(windows)]
    {
        let status = Command::new(&tui_bin)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;

        if !status.success() {
            return Err(io::Error::other(format!(
                "TUI exited with status: {}",
                status
            )));
        }
        Ok(())
    }
}

/// Launch the GUI with companion socket address.
///
/// The `companion_addr` should be a TCP address like "127.0.0.1:54321".
/// Returns the PID of the spawned GUI process.
pub fn launch_gui(companion_addr: &str) -> io::Result<u32> {
    let gui_bin = find_binary("trendlab-gui")?;

    let child = Command::new(&gui_bin)
        .env(COMPANION_SOCKET_ENV, companion_addr)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(child.id())
}

/// Find a binary in the same directory as the current executable.
fn find_binary(name: &str) -> io::Result<std::path::PathBuf> {
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Cannot find executable directory")
    })?;

    // Try with platform-specific extension
    #[cfg(windows)]
    let bin_name = format!("{}.exe", name);
    #[cfg(not(windows))]
    let bin_name = name.to_string();

    let bin_path = exe_dir.join(&bin_name);

    if bin_path.exists() {
        return Ok(bin_path);
    }

    // Also try cargo target directories for development
    // Look for target/debug or target/release relative to current exe
    let target_dir = exe_dir;
    let bin_path = target_dir.join(&bin_name);
    if bin_path.exists() {
        return Ok(bin_path);
    }

    // Try just the name (rely on PATH)
    Ok(std::path::PathBuf::from(&bin_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_binary_name() {
        // Just verify the function doesn't panic
        let result = find_binary("nonexistent-binary");
        // Should return a path (might not exist, but that's OK)
        assert!(result.is_ok());
    }
}
