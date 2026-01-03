//! TrendLab Unified Launcher
//!
//! A single binary that can launch either the Terminal UI or Desktop GUI,
//! with a companion mode that shows GUI activity in the terminal.

use clap::Parser;

use trendlab_launcher::deprecation;
use trendlab_launcher::exec;
use trendlab_launcher::prompt::{self, LaunchMode};
use trendlab_logging::LogConfig;

/// TrendLab - Trend-Following Backtest Lab
#[derive(Parser, Debug)]
#[command(name = "trendlab")]
#[command(version, about, long_about = None)]
struct Args {
    /// Launch Terminal UI directly (skip mode selection)
    #[arg(long)]
    tui: bool,

    /// Launch Desktop GUI directly (skip mode selection)
    #[arg(long)]
    gui: bool,

    /// Enable logging output (shown in terminal for TUI, companion for GUI)
    #[arg(long)]
    log: bool,

    /// Disable logging (skip prompt, run without logs)
    #[arg(long)]
    no_log: bool,

    /// Log level filter (e.g., "debug", "trace", "trendlab=debug,polars=warn")
    #[arg(long, default_value = "info,trendlab=debug")]
    log_filter: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Determine logging preference from CLI flags or interactive prompt
    let logging_enabled = if args.log {
        true
    } else if args.no_log {
        false
    } else {
        // Show logging prompt first (before mode selection)
        prompt::show_logging_prompt()?
    };

    // Compute project root from binary location (binary is in target/release or target/debug)
    // This ensures logs go to the right place regardless of working directory
    let project_root = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf())) // target/release
        .and_then(|p| p.parent().map(|p| p.to_path_buf())) // target
        .and_then(|p| p.parent().map(|p| p.to_path_buf())) // project root
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let log_dir = project_root.join("data").join("logs");

    // Create log config with explicit log directory
    let log_config = LogConfig {
        enabled: logging_enabled,
        filter: args.log_filter.clone(),
        log_dir,
        ..LogConfig::default()
    };

    // Initialize launcher-level logging if enabled
    let _log_guard = if logging_enabled {
        trendlab_logging::init_launcher_logging(&log_config)
    } else {
        None
    };

    // Set environment variables for child processes
    log_config.set_env();

    // Determine mode from CLI flags or interactive prompt
    let mode = if args.tui {
        Some(LaunchMode::Tui)
    } else if args.gui {
        Some(LaunchMode::Gui)
    } else {
        // Show interactive prompt
        prompt::show()?
    };

    match mode {
        Some(LaunchMode::Tui) => {
            if logging_enabled {
                tracing::info!("Launching TUI mode");
            }
            // Launch TUI (replaces process on Unix)
            exec::launch_tui()?;
        }
        Some(LaunchMode::Gui) => {
            if logging_enabled {
                tracing::info!("GUI mode requested (deprecated)");
            }
            // GUI is deprecated - show message and open resurrection roadmap
            deprecation::handle_gui_deprecation()?;
        }
        None => {
            // User cancelled
            println!("Cancelled.");
        }
    }

    Ok(())
}
