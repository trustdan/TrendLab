//! Report command implementation - summary and export functionality.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use trendlab_core::{RunManifest, SweepConfigResult};

use super::terminal::{color_value, format_sweep_table_colored, print_section, sparkline};

/// Get the reports base directory.
fn reports_dir() -> PathBuf {
    PathBuf::from("reports/runs")
}

/// Load sweep results from a run directory.
fn load_sweep_results(run_id: &str) -> Result<(RunManifest, Vec<SweepConfigResult>)> {
    let run_dir = reports_dir().join(run_id);

    if !run_dir.exists() {
        bail!("Run '{}' not found at {}", run_id, run_dir.display());
    }

    // Load manifest
    let manifest_path = run_dir.join("manifest.json");
    if !manifest_path.exists() {
        bail!("Manifest not found at {}", manifest_path.display());
    }

    let manifest_json = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read manifest: {}", manifest_path.display()))?;
    let manifest: RunManifest = serde_json::from_str(&manifest_json)
        .with_context(|| format!("Failed to parse manifest: {}", manifest_path.display()))?;

    // Load results (JSON format - could also support Parquet)
    let results_path = run_dir.join("results.json");
    if !results_path.exists() {
        bail!("Results not found at {}", results_path.display());
    }

    let results_json = fs::read_to_string(&results_path)
        .with_context(|| format!("Failed to read results: {}", results_path.display()))?;
    let results: Vec<SweepConfigResult> = serde_json::from_str(&results_json)
        .with_context(|| format!("Failed to parse results: {}", results_path.display()))?;

    Ok((manifest, results))
}

/// Display a summary of a sweep run with colored terminal output.
pub fn execute_summary(run_id: &str, top_n: usize) -> Result<()> {
    let (manifest, results) = load_sweep_results(run_id)?;

    // Header
    println!("\n{}", "═".repeat(90).cyan());
    println!(
        "  {} {}",
        "SWEEP SUMMARY".cyan().bold(),
        manifest.sweep_id.dimmed()
    );
    println!("{}", "═".repeat(90).cyan());

    // Run info
    println!();
    println!(
        "  {} {}",
        "Symbol:".dimmed(),
        manifest.sweep_config.symbol.white()
    );
    println!(
        "  {} {} to {}",
        "Period:".dimmed(),
        manifest.sweep_config.start_date.white(),
        manifest.sweep_config.end_date.white()
    );
    println!(
        "  {} {} entry × {} exit = {} configs",
        "Grid:".dimmed(),
        manifest
            .sweep_config
            .grid
            .entry_lookbacks
            .len()
            .to_string()
            .white(),
        manifest
            .sweep_config
            .grid
            .exit_lookbacks
            .len()
            .to_string()
            .white(),
        results.len().to_string().white().bold()
    );
    println!(
        "  {} {}",
        "Executed:".dimmed(),
        manifest
            .started_at
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string()
            .dimmed()
    );
    println!();

    // Top by Sharpe with colored table and sparklines
    print_section("Top Configurations by Sharpe Ratio");
    println!("{}", format_sweep_table_colored(&results, top_n));

    // Best configuration highlight
    let mut ranked: Vec<&SweepConfigResult> = results.iter().collect();
    ranked.sort_by(|a, b| {
        b.metrics
            .sharpe
            .partial_cmp(&a.metrics.sharpe)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if let Some(best) = ranked.first() {
        println!("{}", "─".repeat(90).dimmed());
        println!(
            "\n  {} Entry: {}, Exit: {}",
            "Best Configuration:".cyan().bold(),
            best.config_id.entry_lookback.to_string().white(),
            best.config_id.exit_lookback.to_string().white()
        );

        println!(
            "    {}  {}  |  {}  {}  |  {}  {}",
            "Sharpe:".dimmed(),
            color_value(
                best.metrics.sharpe,
                format!("{:.3}", best.metrics.sharpe),
                false
            ),
            "CAGR:".dimmed(),
            color_value(
                best.metrics.cagr,
                format!("{:.1}%", best.metrics.cagr * 100.0),
                false
            ),
            "Max DD:".dimmed(),
            color_value(
                best.metrics.max_drawdown,
                format!("{:.1}%", best.metrics.max_drawdown * 100.0),
                true
            )
        );

        // Equity sparkline
        let equity: Vec<f64> = best
            .backtest_result
            .equity
            .iter()
            .map(|e| e.equity)
            .collect();
        println!("    {}  {}", "Equity:".dimmed(), sparkline(&equity).green());
    }

    // Grid statistics
    print_section("Grid Statistics");

    let sharpes: Vec<f64> = results.iter().map(|r| r.metrics.sharpe).collect();
    let cagrs: Vec<f64> = results.iter().map(|r| r.metrics.cagr).collect();

    let mean_sharpe = sharpes.iter().sum::<f64>() / sharpes.len() as f64;
    let max_sharpe = sharpes.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_sharpe = sharpes.iter().cloned().fold(f64::INFINITY, f64::min);

    let mean_cagr = cagrs.iter().sum::<f64>() / cagrs.len() as f64;
    let max_cagr = cagrs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_cagr = cagrs.iter().cloned().fold(f64::INFINITY, f64::min);

    println!(
        "  {} min={}  mean={}  max={}",
        "Sharpe:".cyan(),
        color_value(min_sharpe, format!("{:.3}", min_sharpe), false),
        format!("{:.3}", mean_sharpe).yellow(),
        color_value(max_sharpe, format!("{:.3}", max_sharpe), false)
    );
    println!(
        "  {} min={}  mean={}  max={}",
        "CAGR:  ".cyan(),
        color_value(min_cagr, format!("{:.1}%", min_cagr * 100.0), false),
        format!("{:.1}%", mean_cagr * 100.0).yellow(),
        color_value(max_cagr, format!("{:.1}%", max_cagr * 100.0), false)
    );

    // Profitable configurations
    let profitable = results
        .iter()
        .filter(|r| r.metrics.total_return > 0.0)
        .count();
    let pct = (profitable as f64 / results.len() as f64) * 100.0;
    let pct_color = if pct > 50.0 {
        format!("{:.1}%", pct).green()
    } else {
        format!("{:.1}%", pct).red()
    };

    println!();
    println!(
        "  {} {} / {} ({})",
        "Profitable:".cyan(),
        profitable.to_string().white(),
        results.len(),
        pct_color
    );

    println!("\n{}", "═".repeat(90).cyan());

    Ok(())
}

/// Export sweep results to CSV.
pub fn execute_export(run_id: &str, output: &str) -> Result<()> {
    let (_manifest, results) = load_sweep_results(run_id)?;

    let mut csv = String::new();
    csv.push_str("entry_lookback,exit_lookback,sharpe,cagr,sortino,calmar,max_drawdown,total_return,win_rate,profit_factor,num_trades,turnover\n");

    for r in &results {
        csv.push_str(&format!(
            "{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{:.6}\n",
            r.config_id.entry_lookback,
            r.config_id.exit_lookback,
            r.metrics.sharpe,
            r.metrics.cagr,
            r.metrics.sortino,
            r.metrics.calmar,
            r.metrics.max_drawdown,
            r.metrics.total_return,
            r.metrics.win_rate,
            r.metrics.profit_factor,
            r.metrics.num_trades,
            r.metrics.turnover
        ));
    }

    fs::write(output, &csv).with_context(|| format!("Failed to write to {}", output))?;

    println!("Exported {} configurations to {}", results.len(), output);

    Ok(())
}

/// List available sweep runs.
pub fn list_runs() -> Result<Vec<String>> {
    let dir = reports_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut runs = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                // Check if it has a manifest
                if entry.path().join("manifest.json").exists() {
                    runs.push(name.to_string());
                }
            }
        }
    }

    runs.sort();
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reports_dir() {
        let dir = reports_dir();
        assert!(dir.to_string_lossy().contains("reports"));
    }
}
