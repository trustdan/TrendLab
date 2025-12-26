//! TrendLab CLI - Command-line interface for trend-following research.

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "trendlab")]
#[command(author, version, about = "Research-grade trend-following backtester", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Data management commands
    Data {
        #[command(subcommand)]
        command: DataCommands,
    },

    /// Run parameter sweep
    Sweep {
        /// Strategy ID to sweep
        #[arg(short, long)]
        strategy: String,

        /// Ticker symbols (comma-separated) or path to file
        #[arg(short, long)]
        universe: String,

        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start: String,

        /// End date (YYYY-MM-DD)
        #[arg(long)]
        end: String,
    },

    /// Generate reports
    Report {
        #[command(subcommand)]
        command: ReportCommands,
    },

    /// Strategy artifact commands
    Artifact {
        #[command(subcommand)]
        command: ArtifactCommands,
    },
}

#[derive(Subcommand)]
enum DataCommands {
    /// Refresh daily bars from Yahoo Finance
    RefreshYahoo {
        /// Ticker symbols (comma-separated) or path to file
        #[arg(short, long)]
        tickers: String,

        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start: String,

        /// End date (YYYY-MM-DD)
        #[arg(long)]
        end: String,

        /// Force refresh even if cached
        #[arg(long, default_value = "false")]
        force: bool,
    },

    /// Show data status and quality report
    Status {
        /// Ticker symbol to check
        #[arg(short, long)]
        ticker: Option<String>,
    },
}

#[derive(Subcommand)]
enum ReportCommands {
    /// Generate summary report for a run
    Summary {
        /// Run ID to summarize
        #[arg(short, long)]
        run_id: String,
    },

    /// Export metrics to CSV
    Export {
        /// Run ID to export
        #[arg(short, long)]
        run_id: String,

        /// Output path
        #[arg(short, long)]
        output: String,
    },
}

#[derive(Subcommand)]
enum ArtifactCommands {
    /// Export strategy artifact for Pine generation
    Export {
        /// Run ID
        #[arg(short, long)]
        run_id: String,

        /// Config ID within the run
        #[arg(short, long)]
        config_id: String,
    },

    /// Validate artifact against schema
    Validate {
        /// Path to artifact JSON
        #[arg(short, long)]
        path: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Data { command } => match command {
            DataCommands::RefreshYahoo {
                tickers,
                start,
                end,
                force,
            } => {
                println!("📊 Refreshing Yahoo data...");
                println!("  Tickers: {}", tickers);
                println!("  Range: {} to {}", start, end);
                println!("  Force: {}", force);
                println!();
                println!("⚠️  Not yet implemented. Use /data:refresh-yahoo skill.");
            }
            DataCommands::Status { ticker } => {
                println!("📈 Data status...");
                if let Some(t) = ticker {
                    println!("  Ticker: {}", t);
                } else {
                    println!("  All tickers");
                }
                println!();
                println!("⚠️  Not yet implemented.");
            }
        },

        Commands::Sweep {
            strategy,
            universe,
            start,
            end,
        } => {
            println!("🔄 Running parameter sweep...");
            println!("  Strategy: {}", strategy);
            println!("  Universe: {}", universe);
            println!("  Range: {} to {}", start, end);
            println!();
            println!("⚠️  Not yet implemented. Use /trendlab:run-sweep skill.");
        }

        Commands::Report { command } => match command {
            ReportCommands::Summary { run_id } => {
                println!("📋 Generating summary for run: {}", run_id);
                println!();
                println!("⚠️  Not yet implemented.");
            }
            ReportCommands::Export { run_id, output } => {
                println!("📤 Exporting run {} to {}", run_id, output);
                println!();
                println!("⚠️  Not yet implemented.");
            }
        },

        Commands::Artifact { command } => match command {
            ArtifactCommands::Export { run_id, config_id } => {
                println!("🎯 Exporting artifact...");
                println!("  Run: {}", run_id);
                println!("  Config: {}", config_id);
                println!();
                println!("⚠️  Not yet implemented. Use /pine:export-artifact skill.");
            }
            ArtifactCommands::Validate { path } => {
                println!("✅ Validating artifact: {}", path);
                println!();
                println!("⚠️  Not yet implemented.");
            }
        },
    }

    Ok(())
}
