//! Run command implementation - single backtest execution.

use anyhow::{bail, Result};
use chrono::{Datelike, NaiveDate};
use colored::Colorize;

use trendlab_core::{
    backtest::{run_backtest, BacktestConfig, CostModel, FillModel},
    compute_metrics, read_parquet, DonchianBreakoutStrategy, Metrics,
};

use super::data::DataConfig;
use super::terminal::color_value;

/// Strategy configuration parsed from CLI arguments.
#[derive(Debug)]
pub struct StrategyConfig {
    pub id: String,
    pub entry_lookback: usize,
    pub exit_lookback: usize,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            id: "donchian".to_string(),
            entry_lookback: 20,
            exit_lookback: 10,
        }
    }
}

/// Run result containing backtest outputs.
#[derive(Debug)]
pub struct RunResult {
    pub symbol: String,
    pub strategy_id: String,
    pub config: BacktestConfig,
    pub metrics: Metrics,
    pub num_bars: usize,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

/// Parse strategy string into configuration.
///
/// Format: "donchian" or "donchian:20,10" (entry,exit lookbacks)
pub fn parse_strategy(strategy: &str) -> Result<StrategyConfig> {
    let parts: Vec<&str> = strategy.split(':').collect();
    let id = parts[0].to_lowercase();

    if id != "donchian" {
        bail!("Unknown strategy: {}. Available: donchian", id);
    }

    let (entry, exit) = if parts.len() > 1 {
        let params: Vec<&str> = parts[1].split(',').collect();
        if params.len() != 2 {
            bail!("Strategy params should be: donchian:entry,exit (e.g., donchian:20,10)");
        }
        let entry = params[0].parse::<usize>()?;
        let exit = params[1].parse::<usize>()?;
        (entry, exit)
    } else {
        (20, 10) // Default: Turtle System 1
    };

    Ok(StrategyConfig {
        id,
        entry_lookback: entry,
        exit_lookback: exit,
    })
}

/// Execute a single backtest.
pub fn execute_run(
    strategy_str: &str,
    ticker: &str,
    start: NaiveDate,
    end: NaiveDate,
    config: &DataConfig,
) -> Result<RunResult> {
    let strat_config = parse_strategy(strategy_str)?;

    // Load data from Parquet
    let parquet_dir = config.parquet_dir();
    let symbol_dir = parquet_dir.join(format!("1d/symbol={}", ticker));

    if !symbol_dir.exists() {
        bail!(
            "No data found for {}. Run 'trendlab data refresh-yahoo --tickers {}' first.",
            ticker,
            ticker
        );
    }

    // Find all Parquet files for this symbol within date range
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

    // Filter to date range and sort
    all_bars.retain(|b| {
        let d = b.ts.date_naive();
        d >= start && d <= end
    });
    all_bars.sort_by_key(|b| b.ts);

    if all_bars.is_empty() {
        bail!(
            "No bars found for {} in date range {} to {}",
            ticker,
            start,
            end
        );
    }

    let actual_start = all_bars.first().unwrap().ts.date_naive();
    let actual_end = all_bars.last().unwrap().ts.date_naive();

    // Create strategy
    let mut strategy =
        DonchianBreakoutStrategy::new(strat_config.entry_lookback, strat_config.exit_lookback);

    // Configure backtest
    let bt_config = BacktestConfig {
        initial_cash: 100_000.0,
        fill_model: FillModel::NextOpen,
        cost_model: CostModel {
            fees_bps_per_side: 10.0,
            slippage_bps: 5.0,
        },
        qty: 100.0,
        pyramid_config: trendlab_core::PyramidConfig::default(),
    };

    // Run backtest
    let result = run_backtest(&all_bars, &mut strategy, bt_config)?;

    // Compute metrics
    let metrics = compute_metrics(&result, bt_config.initial_cash);

    Ok(RunResult {
        symbol: ticker.to_string(),
        strategy_id: format!(
            "{}:{},{}",
            strat_config.id, strat_config.entry_lookback, strat_config.exit_lookback
        ),
        config: bt_config,
        metrics,
        num_bars: all_bars.len(),
        start_date: actual_start,
        end_date: actual_end,
    })
}

/// Format metrics for display with colored output.
pub fn format_metrics(result: &RunResult) -> String {
    let m = &result.metrics;
    let mut output = String::new();

    // Header
    output.push_str(&format!("\n{}\n", "═".repeat(70).cyan()));
    output.push_str(&format!(
        "  {} {}\n",
        "BACKTEST RESULTS".cyan().bold(),
        result.strategy_id.dimmed()
    ));
    output.push_str(&format!("{}\n\n", "═".repeat(70).cyan()));

    // Run info
    output.push_str(&format!(
        "  {} {}\n",
        "Symbol:".dimmed(),
        result.symbol.white()
    ));
    output.push_str(&format!(
        "  {} {} to {} ({} bars)\n",
        "Period:".dimmed(),
        result.start_date.to_string().white(),
        result.end_date.to_string().white(),
        result.num_bars.to_string().dimmed()
    ));

    // Performance metrics
    output.push_str(&format!("\n{}\n", "─".repeat(70).dimmed()));
    output.push_str(&format!("{}\n", "  Performance Metrics".cyan().bold()));
    output.push_str(&format!("{}\n", "─".repeat(70).dimmed()));

    output.push_str(&format!(
        "    {:<18} {}\n",
        "Total Return:".dimmed(),
        color_value(
            m.total_return,
            format!("{:.2}%", m.total_return * 100.0),
            false
        )
    ));
    output.push_str(&format!(
        "    {:<18} {}\n",
        "CAGR:".dimmed(),
        color_value(m.cagr, format!("{:.2}%", m.cagr * 100.0), false)
    ));
    output.push_str(&format!(
        "    {:<18} {}\n",
        "Max Drawdown:".dimmed(),
        color_value(
            m.max_drawdown,
            format!("{:.2}%", m.max_drawdown * 100.0),
            true
        )
    ));
    output.push_str(&format!(
        "    {:<18} {}\n",
        "Sharpe Ratio:".dimmed(),
        color_value(m.sharpe, format!("{:.3}", m.sharpe), false)
    ));
    output.push_str(&format!(
        "    {:<18} {}\n",
        "Sortino Ratio:".dimmed(),
        color_value(m.sortino, format!("{:.3}", m.sortino), false)
    ));
    output.push_str(&format!(
        "    {:<18} {}\n",
        "Calmar Ratio:".dimmed(),
        color_value(m.calmar, format!("{:.3}", m.calmar), false)
    ));

    // Trade statistics
    output.push_str(&format!("\n{}\n", "─".repeat(70).dimmed()));
    output.push_str(&format!("{}\n", "  Trade Statistics".cyan().bold()));
    output.push_str(&format!("{}\n", "─".repeat(70).dimmed()));

    output.push_str(&format!(
        "    {:<18} {}\n",
        "Total Trades:".dimmed(),
        m.num_trades.to_string().white()
    ));
    output.push_str(&format!(
        "    {:<18} {}\n",
        "Win Rate:".dimmed(),
        color_value(
            m.win_rate - 0.5,
            format!("{:.1}%", m.win_rate * 100.0),
            false
        )
    ));

    let pf_str = if m.profit_factor.is_infinite() {
        "∞".to_string()
    } else {
        format!("{:.2}", m.profit_factor)
    };
    output.push_str(&format!(
        "    {:<18} {}\n",
        "Profit Factor:".dimmed(),
        color_value(m.profit_factor - 1.0, pf_str, false)
    ));
    output.push_str(&format!(
        "    {:<18} {}x\n",
        "Annual Turnover:".dimmed(),
        format!("{:.2}", m.turnover).dimmed()
    ));

    // Configuration
    output.push_str(&format!("\n{}\n", "─".repeat(70).dimmed()));
    output.push_str(&format!("{}\n", "  Configuration".dimmed()));
    output.push_str(&format!("{}\n", "─".repeat(70).dimmed()));

    output.push_str(&format!(
        "    {:<18} ${:.0}\n",
        "Initial Cash:".dimmed(),
        result.config.initial_cash
    ));
    output.push_str(&format!(
        "    {:<18} {:.0} shares\n",
        "Position Size:".dimmed(),
        result.config.qty
    ));
    output.push_str(&format!(
        "    {:<18} {:.1} bps/side\n",
        "Fees:".dimmed(),
        result.config.cost_model.fees_bps_per_side
    ));
    output.push_str(&format!(
        "    {:<18} {:.1} bps\n",
        "Slippage:".dimmed(),
        result.config.cost_model.slippage_bps
    ));
    output.push_str(&format!("    {:<18} NextOpen\n", "Fill Model:".dimmed()));

    output.push_str(&format!("\n{}\n", "═".repeat(70).cyan()));

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_strategy_default() {
        let config = parse_strategy("donchian").unwrap();
        assert_eq!(config.id, "donchian");
        assert_eq!(config.entry_lookback, 20);
        assert_eq!(config.exit_lookback, 10);
    }

    #[test]
    fn test_parse_strategy_with_params() {
        let config = parse_strategy("donchian:55,20").unwrap();
        assert_eq!(config.id, "donchian");
        assert_eq!(config.entry_lookback, 55);
        assert_eq!(config.exit_lookback, 20);
    }

    #[test]
    fn test_parse_strategy_unknown() {
        assert!(parse_strategy("unknown").is_err());
    }
}
