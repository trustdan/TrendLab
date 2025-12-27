//! Sweep command implementation - parameter sweep execution.

use anyhow::{bail, Context, Result};
use chrono::{Datelike, NaiveDate, Utc};
use std::fs;

use trendlab_core::{
    backtest::{BacktestConfig, CostModel, FillModel},
    generate_summary_markdown, read_parquet, run_sweep, RankMetric, ResultPaths, RunManifest,
    SweepConfig, SweepGrid, SweepResult,
};

use super::data::DataConfig;
use super::terminal::{color_value, format_sweep_table_colored, print_section, sparkline};

/// Sweep grid specification from CLI args.
#[derive(Debug, Clone)]
pub struct GridSpec {
    pub entry_lookbacks: Vec<usize>,
    pub exit_lookbacks: Vec<usize>,
}

impl Default for GridSpec {
    fn default() -> Self {
        // Default grid: Turtle-style lookback ranges
        Self {
            entry_lookbacks: vec![10, 20, 30, 40, 50, 55],
            exit_lookbacks: vec![5, 10, 15, 20],
        }
    }
}

/// Parse grid specification string.
///
/// Format: "entry:10,20,30,40;exit:5,10,15" or "entry:10..50:10;exit:5..20:5" (range with step)
pub fn parse_grid(grid_str: &str) -> Result<GridSpec> {
    let mut entry_lookbacks = Vec::new();
    let mut exit_lookbacks = Vec::new();

    for part in grid_str.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let (key, values_str) = part
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Invalid grid format: {}", part))?;

        let values = parse_values(values_str)?;

        match key.to_lowercase().as_str() {
            "entry" => entry_lookbacks = values,
            "exit" => exit_lookbacks = values,
            _ => bail!("Unknown grid parameter: {}. Use 'entry' or 'exit'", key),
        }
    }

    if entry_lookbacks.is_empty() {
        entry_lookbacks = GridSpec::default().entry_lookbacks;
    }
    if exit_lookbacks.is_empty() {
        exit_lookbacks = GridSpec::default().exit_lookbacks;
    }

    Ok(GridSpec {
        entry_lookbacks,
        exit_lookbacks,
    })
}

/// Parse values string: "10,20,30" or "10..50:10" (range with step)
fn parse_values(s: &str) -> Result<Vec<usize>> {
    let s = s.trim();

    // Check for range syntax: "start..end:step"
    if s.contains("..") {
        let parts: Vec<&str> = s.split("..").collect();
        if parts.len() != 2 {
            bail!("Invalid range format: {}. Use start..end:step", s);
        }

        let start: usize = parts[0].parse()?;
        let rest: Vec<&str> = parts[1].split(':').collect();
        let end: usize = rest[0].parse()?;
        let step: usize = if rest.len() > 1 { rest[1].parse()? } else { 1 };

        if step == 0 {
            bail!("Step cannot be zero");
        }

        Ok((start..=end).step_by(step).collect())
    } else {
        // Comma-separated values
        s.split(',')
            .map(|v| {
                v.trim()
                    .parse()
                    .with_context(|| format!("Invalid number: {}", v))
            })
            .collect()
    }
}

/// Result of a sweep execution.
#[derive(Debug)]
#[allow(dead_code)]
pub struct SweepExecutionResult {
    pub sweep_id: String,
    pub sweep_result: SweepResult,
    pub output_paths: ResultPaths,
    pub num_configs: usize,
    pub elapsed_secs: f64,
}

/// Execute a parameter sweep.
pub fn execute_sweep(
    strategy: &str,
    ticker: &str,
    start: NaiveDate,
    end: NaiveDate,
    grid_spec: Option<&str>,
    data_config: &DataConfig,
) -> Result<SweepExecutionResult> {
    let start_time = std::time::Instant::now();

    // Parse strategy (only donchian for now)
    if !strategy.to_lowercase().starts_with("donchian") {
        bail!("Unknown strategy: {}. Available: donchian", strategy);
    }

    // Parse grid
    let grid_spec = if let Some(gs) = grid_spec {
        parse_grid(gs)?
    } else {
        GridSpec::default()
    };

    let grid = SweepGrid::new(
        grid_spec.entry_lookbacks.clone(),
        grid_spec.exit_lookbacks.clone(),
    );
    let num_configs = grid.len();

    println!(
        "Grid: {} entry × {} exit = {} configurations",
        grid_spec.entry_lookbacks.len(),
        grid_spec.exit_lookbacks.len(),
        num_configs
    );

    // Load data
    let parquet_dir = data_config.parquet_dir();
    let symbol_dir = parquet_dir.join(format!("1d/symbol={}", ticker));

    if !symbol_dir.exists() {
        bail!(
            "No data found for {}. Run 'trendlab data refresh-yahoo --tickers {}' first.",
            ticker,
            ticker
        );
    }

    let mut all_bars = Vec::new();
    for year in start.year()..=end.year() {
        let year_path = symbol_dir.join(format!("year={}/data.parquet", year));
        if year_path.exists() {
            let bars = read_parquet(&year_path)?;
            all_bars.extend(bars);
        }
    }

    if all_bars.is_empty() {
        bail!(
            "No bars found for {} in date range {} to {}",
            ticker,
            start,
            end
        );
    }

    // Filter and sort
    all_bars.retain(|b| {
        let d = b.ts.date_naive();
        d >= start && d <= end
    });
    all_bars.sort_by_key(|b| b.ts);

    if all_bars.is_empty() {
        bail!("No bars in date range {} to {} for {}", start, end, ticker);
    }

    println!("Loaded {} bars for {}", all_bars.len(), ticker);

    // Configure backtest
    let backtest_config = BacktestConfig {
        initial_cash: 100_000.0,
        fill_model: FillModel::NextOpen,
        cost_model: CostModel {
            fees_bps_per_side: 10.0,
            slippage_bps: 5.0,
        },
        qty: 100.0,
        pyramid_config: trendlab_core::PyramidConfig::default(),
    };

    // Run sweep
    println!(
        "Running sweep with {} threads...",
        rayon::current_num_threads()
    );
    let sweep_result = run_sweep(&all_bars, &grid, backtest_config);

    let elapsed = start_time.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();

    println!(
        "Completed {} configurations in {:.2}s ({:.1} configs/sec)",
        num_configs,
        elapsed_secs,
        num_configs as f64 / elapsed_secs
    );

    // Save results
    let output_paths = ResultPaths::for_sweep(&sweep_result.sweep_id);
    save_sweep_results(&sweep_result, &grid, &backtest_config, ticker, start, end)?;

    Ok(SweepExecutionResult {
        sweep_id: sweep_result.sweep_id.clone(),
        sweep_result,
        output_paths,
        num_configs,
        elapsed_secs,
    })
}

/// Save sweep results to disk.
fn save_sweep_results(
    result: &SweepResult,
    grid: &SweepGrid,
    config: &BacktestConfig,
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<()> {
    let paths = ResultPaths::for_sweep(&result.sweep_id);

    // Create directory
    let dir = paths.manifest.parent().unwrap();
    fs::create_dir_all(dir)?;

    // Write manifest
    let sweep_config = SweepConfig {
        grid: grid.clone(),
        backtest_config: *config,
        symbol: symbol.to_string(),
        start_date: start.to_string(),
        end_date: end.to_string(),
    };

    let manifest = RunManifest {
        sweep_id: result.sweep_id.clone(),
        sweep_config,
        data_version: compute_data_hash(symbol, start, end),
        started_at: result.started_at,
        completed_at: result.completed_at,
        result_paths: paths.clone(),
    };

    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&paths.manifest, manifest_json)?;

    // Write summary markdown
    let summary = generate_summary_markdown(result, 10);
    fs::write(&paths.summary_md, summary)?;

    // Write results as JSON (Parquet would require polars in CLI crate)
    let results_json = serde_json::to_string_pretty(&result.config_results)?;
    let json_path = paths.results_parquet.with_extension("json");
    fs::write(&json_path, results_json)?;

    println!();
    println!("Results saved to:");
    println!("  Manifest: {}", paths.manifest.display());
    println!("  Summary:  {}", paths.summary_md.display());
    println!("  Results:  {}", json_path.display());

    Ok(())
}

/// Compute a simple hash for data versioning.
fn compute_data_hash(symbol: &str, start: NaiveDate, end: NaiveDate) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "{}_{}_{}_{}",
        symbol,
        start,
        end,
        Utc::now().timestamp()
    ));
    format!("{:x}", hasher.finalize())[..16].to_string()
}

/// Format sweep results for display with colored terminal output.
pub fn format_sweep_summary(result: &SweepExecutionResult, top_n: usize) -> String {
    use colored::Colorize;

    let mut output = String::new();

    // Header
    output.push_str(&format!("\n{}\n", "═".repeat(90).cyan()));
    output.push_str(&format!(
        "  {} {}\n",
        "SWEEP RESULTS".cyan().bold(),
        result.sweep_id.dimmed()
    ));
    output.push_str(&format!(
        "  {} configurations in {:.2}s ({:.1} configs/sec)\n",
        result.num_configs.to_string().white().bold(),
        result.elapsed_secs,
        result.num_configs as f64 / result.elapsed_secs
    ));
    output.push_str(&format!("{}\n\n", "═".repeat(90).cyan()));

    // Top by Sharpe with colored table and sparklines
    print_section("Top Configurations by Sharpe Ratio");
    output.push_str(&format_sweep_table_colored(
        &result.sweep_result.config_results,
        top_n,
    ));

    // Best configuration summary
    if let Some(best) = result
        .sweep_result
        .top_n(1, RankMetric::Sharpe, false)
        .first()
    {
        output.push_str(&format!("\n{}\n", "─".repeat(90).dimmed()));
        output.push_str(&format!(
            "\n  {} Entry: {}, Exit: {}\n",
            "Best Configuration:".cyan().bold(),
            best.config_id.entry_lookback,
            best.config_id.exit_lookback
        ));

        output.push_str(&format!(
            "    Sharpe:  {}  |  CAGR:  {}  |  Max DD:  {}\n",
            color_value(
                best.metrics.sharpe,
                format!("{:.3}", best.metrics.sharpe),
                false
            ),
            color_value(
                best.metrics.cagr,
                format!("{:.1}%", best.metrics.cagr * 100.0),
                false
            ),
            color_value(
                best.metrics.max_drawdown,
                format!("{:.1}%", best.metrics.max_drawdown * 100.0),
                true
            )
        ));

        // Equity sparkline for best config
        let equity: Vec<f64> = best
            .backtest_result
            .equity
            .iter()
            .map(|e| e.equity)
            .collect();
        output.push_str(&format!("    Equity:  {}\n", sparkline(&equity).green()));
    }

    output.push_str(&format!("\n{}\n", "═".repeat(90).cyan()));

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_grid_comma_separated() {
        let spec = parse_grid("entry:10,20,30;exit:5,10").unwrap();
        assert_eq!(spec.entry_lookbacks, vec![10, 20, 30]);
        assert_eq!(spec.exit_lookbacks, vec![5, 10]);
    }

    #[test]
    fn test_parse_grid_range() {
        let spec = parse_grid("entry:10..30:10;exit:5..15:5").unwrap();
        assert_eq!(spec.entry_lookbacks, vec![10, 20, 30]);
        assert_eq!(spec.exit_lookbacks, vec![5, 10, 15]);
    }

    #[test]
    fn test_parse_grid_default() {
        let spec = parse_grid("entry:10,20").unwrap();
        assert_eq!(spec.entry_lookbacks, vec![10, 20]);
        // Exit uses default
        assert!(!spec.exit_lookbacks.is_empty());
    }

    #[test]
    fn test_parse_values_comma() {
        let vals = parse_values("10,20,30,40").unwrap();
        assert_eq!(vals, vec![10, 20, 30, 40]);
    }

    #[test]
    fn test_parse_values_range() {
        let vals = parse_values("10..50:10").unwrap();
        assert_eq!(vals, vec![10, 20, 30, 40, 50]);
    }
}
