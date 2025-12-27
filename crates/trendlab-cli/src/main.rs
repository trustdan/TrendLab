//! TrendLab CLI - Command-line interface for trend-following research.

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::artifact;
use commands::data::{self, DataConfig, RefreshSource};
use commands::html_report;
use commands::report;
use commands::run;
use commands::sweep;
use trendlab_cli::commands;

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

    /// Run a single backtest
    Run {
        /// Strategy ID
        #[arg(short, long)]
        strategy: String,

        /// Ticker symbol
        #[arg(short, long)]
        ticker: String,

        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start: String,

        /// End date (YYYY-MM-DD)
        #[arg(long)]
        end: String,
    },

    /// Run parameter sweep
    Sweep {
        /// Strategy ID to sweep
        #[arg(short, long)]
        strategy: String,

        /// Ticker symbol to test
        #[arg(short, long)]
        ticker: String,

        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start: String,

        /// End date (YYYY-MM-DD)
        #[arg(long)]
        end: String,

        /// Parameter grid specification
        /// Format: "entry:10,20,30;exit:5,10" or "entry:10..50:10;exit:5..20:5"
        #[arg(short, long)]
        grid: Option<String>,

        /// Number of top configurations to display
        #[arg(long, default_value = "5")]
        top_n: usize,

        /// Use sequential backtest instead of vectorized Polars (slower but useful for debugging)
        #[arg(long, default_value = "false")]
        sequential: bool,
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

        /// Number of top configurations to display
        #[arg(long, default_value = "5")]
        top_n: usize,
    },

    /// Generate self-contained HTML report
    Html {
        /// Run ID to generate report for
        #[arg(short, long)]
        run_id: String,

        /// Open report in browser after generation
        #[arg(long, default_value = "false")]
        open: bool,
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

    /// List available sweep runs
    List,
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
                run_refresh_yahoo(&tickers, &start, &end, force).await?;
            }
            DataCommands::Status { ticker } => {
                run_data_status(ticker.as_deref())?;
            }
        },

        Commands::Run {
            strategy,
            ticker,
            start,
            end,
        } => {
            let start_date = data::parse_date(&start)?;
            let end_date = data::parse_date(&end)?;
            let config = DataConfig::default();

            println!("Running backtest...");
            println!("  Strategy: {}", strategy);
            println!("  Ticker: {}", ticker);
            println!("  Range: {} to {}", start, end);
            println!();

            let result = run::execute_run(&strategy, &ticker, start_date, end_date, &config)?;
            println!("{}", run::format_metrics(&result));
        }

        Commands::Sweep {
            strategy,
            ticker,
            start,
            end,
            grid,
            top_n,
            sequential,
        } => {
            let start_date = data::parse_date(&start)?;
            let end_date = data::parse_date(&end)?;
            let config = DataConfig::default();

            println!("Running parameter sweep...");
            println!("  Strategy: {}", strategy);
            println!("  Ticker:   {}", ticker);
            println!("  Range:    {} to {}", start, end);
            if let Some(ref g) = grid {
                println!("  Grid:     {}", g);
            }
            println!(
                "  Engine:   {}",
                if sequential {
                    "Sequential"
                } else {
                    "Polars (vectorized)"
                }
            );
            println!();

            let result = sweep::execute_sweep(
                &strategy,
                &ticker,
                start_date,
                end_date,
                grid.as_deref(),
                &config,
                sequential,
            )?;

            println!("{}", sweep::format_sweep_summary(&result, top_n));
        }

        Commands::Report { command } => match command {
            ReportCommands::Summary { run_id, top_n } => {
                report::execute_summary(&run_id, top_n)?;
            }
            ReportCommands::Html { run_id, open } => {
                html_report::execute_html_report(&run_id, open)?;
            }
            ReportCommands::Export { run_id, output } => {
                report::execute_export(&run_id, &output)?;
            }
            ReportCommands::List => {
                let runs = report::list_runs()?;
                if runs.is_empty() {
                    println!("No sweep runs found in reports/runs/");
                } else {
                    println!("Available sweep runs:");
                    for run in runs {
                        println!("  {}", run);
                    }
                }
            }
        },

        Commands::Artifact { command } => match command {
            ArtifactCommands::Export { run_id, config_id } => {
                artifact::execute_export(&run_id, &config_id)?;
            }
            ArtifactCommands::Validate { path } => {
                artifact::execute_validate(&path)?;
            }
        },
    }

    Ok(())
}

/// Execute the refresh-yahoo command.
async fn run_refresh_yahoo(tickers: &str, start: &str, end: &str, force: bool) -> Result<()> {
    let tickers = data::parse_tickers(tickers)?;
    let start_date = data::parse_date(start)?;
    let end_date = data::parse_date(end)?;
    let config = DataConfig::default();

    println!("Refreshing Yahoo Finance data...");
    println!("  Tickers: {}", tickers.join(", "));
    println!("  Range: {} to {}", start_date, end_date);
    println!("  Force: {}", force);
    println!();

    let results = data::refresh_yahoo(&tickers, start_date, end_date, force, &config).await?;

    // Print summary
    println!("Results:");
    println!("{:-<60}", "");

    for result in &results {
        let source_str = match result.source {
            RefreshSource::Cache => "cached",
            RefreshSource::Fresh => "fetched",
        };

        let quality_str = if result.quality_report.is_clean() {
            "clean".to_string()
        } else {
            format!(
                "{} issues",
                result.quality_report.duplicate_count
                    + result.quality_report.gap_count
                    + result.quality_report.out_of_order_count
                    + result.quality_report.invalid_ohlc_count
            )
        };

        println!(
            "  {:<8} {:>6} bars ({:<8}) - {}",
            result.symbol, result.bars_count, source_str, quality_str
        );
    }

    println!("{:-<60}", "");

    // Print file locations
    let total_bars: usize = results.iter().map(|r| r.bars_count).sum();
    let total_parquet_files: usize = results.iter().map(|r| r.parquet_paths.len()).sum();

    println!();
    println!(
        "Total: {} bars across {} Parquet files",
        total_bars, total_parquet_files
    );
    println!("  Raw cache: data/raw/yahoo/");
    println!("  Parquet: data/parquet/");
    println!("  Quality reports: data/reports/quality/");

    Ok(())
}

/// Execute the data status command.
fn run_data_status(ticker: Option<&str>) -> Result<()> {
    let config = DataConfig::default();

    if let Some(symbol) = ticker {
        // Show status for specific ticker
        println!("Data status for: {}", symbol);
        println!();

        // Check raw cache
        let raw_dir = config.raw_dir().join(format!("yahoo/{}", symbol));
        if raw_dir.exists() {
            println!("  Raw cache: {} exists", raw_dir.display());
            if let Ok(entries) = std::fs::read_dir(&raw_dir) {
                for entry in entries.flatten() {
                    if entry
                        .path()
                        .extension()
                        .map(|e| e == "csv")
                        .unwrap_or(false)
                    {
                        println!("    - {}", entry.file_name().to_string_lossy());
                    }
                }
            }
        } else {
            println!("  Raw cache: not found");
        }

        // Check Parquet files
        let parquet_pattern = config.parquet_dir().join(format!("1d/symbol={}", symbol));
        if parquet_pattern.exists() {
            println!("  Parquet: {} exists", parquet_pattern.display());
        } else {
            println!("  Parquet: not found");
        }

        // Check quality reports
        let reports_dir = config.reports_dir().join("quality");
        if reports_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&reports_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(symbol) {
                        println!("  Quality report: {}", name);
                    }
                }
            }
        }
    } else {
        // Show overall status
        println!("Data directory status:");
        println!();

        let raw_yahoo = config.raw_dir().join("yahoo");
        if raw_yahoo.exists() {
            if let Ok(entries) = std::fs::read_dir(&raw_yahoo) {
                let symbols: Vec<_> = entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();
                println!("  Raw cache symbols: {}", symbols.join(", "));
            }
        } else {
            println!("  Raw cache: empty");
        }

        let parquet_1d = config.parquet_dir().join("1d");
        if parquet_1d.exists() {
            if let Ok(entries) = std::fs::read_dir(&parquet_1d) {
                let symbols: Vec<_> = entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .strip_prefix("symbol=")
                            .map(|s| s.to_string())
                    })
                    .collect();
                println!("  Parquet symbols: {}", symbols.join(", "));
            }
        } else {
            println!("  Parquet: empty");
        }
    }

    Ok(())
}
