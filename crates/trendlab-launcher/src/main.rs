//! TrendLab Unified Launcher
//!
//! A single binary that can launch either the Terminal UI or Desktop GUI,
//! with a companion mode that shows GUI activity in the terminal.

use clap::Parser;

use trendlab_launcher::companion;
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

    // Create log config
    let log_config = LogConfig {
        enabled: logging_enabled,
        filter: args.log_filter.clone(),
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
                tracing::info!("Launching GUI mode");
            }
            // Start IPC server first (binds to ephemeral port)
            let (server, addr) = companion::start_server().await?;

            // Launch GUI with server address in environment
            let gui_pid = exec::launch_gui(&addr)?;

            println!("GUI launched (PID {})", gui_pid);
            println!("Companion listening on {}", addr);
            if logging_enabled {
                println!("Logging enabled (filter: {})", args.log_filter);
            }
            println!("Starting companion mode...\n");

            // Run companion mode
            companion::run(server, gui_pid).await?;
        }
        None => {
            // User cancelled
            println!("Cancelled.");
        }
    }

    Ok(())
}
