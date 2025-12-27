//! TrendLab Unified Launcher
//!
//! A single binary that can launch either the Terminal UI or Desktop GUI,
//! with a companion mode that shows GUI activity in the terminal.

use clap::Parser;

use trendlab_launcher::companion;
use trendlab_launcher::exec;
use trendlab_launcher::prompt::{self, LaunchMode};

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

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
            // Launch TUI (replaces process on Unix)
            exec::launch_tui()?;
        }
        Some(LaunchMode::Gui) => {
            // Start IPC server first (binds to ephemeral port)
            let (server, addr) = companion::start_server().await?;

            // Launch GUI with server address in environment
            let gui_pid = exec::launch_gui(&addr)?;

            println!("GUI launched (PID {})", gui_pid);
            println!("Companion listening on {}", addr);
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
