//! Scan command for daily signal alerts.
//!
//! Scans a user-curated watchlist for today's signals across configured strategies.

use crate::commands::data::{refresh_yahoo, DataConfig};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use trendlab_core::{
    create_strategy_v2, dataframe_to_bars, scan_symbol_parquet_lazy, Position, Signal, StrategySpec,
};

// =============================================================================
// Watchlist Configuration Types
// =============================================================================

/// Top-level watchlist configuration from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct WatchlistConfig {
    pub watchlist: WatchlistMeta,
    #[serde(default)]
    pub tickers: Vec<WatchlistTicker>,
}

/// Watchlist metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct WatchlistMeta {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default_strategies: Vec<String>,
}

/// A ticker entry in the watchlist.
#[derive(Debug, Clone, Deserialize)]
pub struct WatchlistTicker {
    pub symbol: String,
    /// Optional strategy overrides; if empty, uses default_strategies
    #[serde(default)]
    pub strategies: Vec<String>,
}

impl WatchlistConfig {
    /// Load watchlist from a TOML file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref()).with_context(|| {
            format!("Failed to read watchlist file: {}", path.as_ref().display())
        })?;
        Self::from_toml(&content)
    }

    /// Parse watchlist from TOML string.
    pub fn from_toml(content: &str) -> Result<Self> {
        toml::from_str(content).context("Failed to parse watchlist TOML")
    }

    /// Get strategies for a given ticker (uses defaults if not overridden).
    pub fn strategies_for_ticker<'a>(&'a self, ticker: &'a WatchlistTicker) -> &'a [String] {
        if ticker.strategies.is_empty() {
            &self.watchlist.default_strategies
        } else {
            &ticker.strategies
        }
    }

    /// Get all unique ticker symbols.
    pub fn all_tickers(&self) -> Vec<String> {
        self.tickers.iter().map(|t| t.symbol.clone()).collect()
    }
}

// =============================================================================
// Scan Output Types
// =============================================================================

/// Signal type for JSON output.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    Entry,
    Exit,
    Hold,
}

impl From<Signal> for SignalType {
    fn from(sig: Signal) -> Self {
        match sig {
            Signal::EnterLong | Signal::EnterShort | Signal::AddLong | Signal::AddShort => {
                SignalType::Entry
            }
            Signal::ExitLong | Signal::ExitShort => SignalType::Exit,
            Signal::Hold => SignalType::Hold,
        }
    }
}

/// A signal for a single ticker-strategy combination.
#[derive(Debug, Clone, Serialize)]
pub struct TickerSignal {
    pub symbol: String,
    pub strategy: String,
    pub params: String,
    pub signal: SignalType,
    pub close_price: f64,
    pub timestamp: String,
}

/// Error for a single ticker scan.
#[derive(Debug, Clone, Serialize)]
pub struct ScanError {
    pub symbol: String,
    pub strategy: String,
    pub error: String,
}

/// Summary statistics for the scan.
#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub total_tickers: usize,
    pub total_checks: usize,
    pub entry_signals: usize,
    pub exit_signals: usize,
    pub hold_signals: usize,
    pub errors: Vec<ScanError>,
}

/// Complete scan output.
#[derive(Debug, Clone, Serialize)]
pub struct ScanOutput {
    pub scan_date: String,
    pub scan_timestamp: String,
    pub watchlist_name: String,
    pub signals: Vec<TickerSignal>,
    pub summary: ScanSummary,
}

// =============================================================================
// Strategy Parsing
// =============================================================================

/// Parse a strategy string into a StrategySpec.
///
/// Supported formats:
///   - "donchian:55,20"
///   - "52wk_high:252,0.95,0.90"
///   - "supertrend:10,3.0"
///   - "psar:0.02,0.02,0.2"
///   - "ma_cross:50,200,sma"
///   - "tsmom:252"
pub fn parse_strategy(s: &str) -> Result<(StrategySpec, String, String)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.is_empty() {
        return Err(anyhow!("Empty strategy string"));
    }

    let strategy_type = parts[0].to_lowercase();
    let params_str = parts.get(1).unwrap_or(&"").to_string();
    let params: Vec<&str> = if params_str.is_empty() {
        vec![]
    } else {
        params_str.split(',').collect()
    };

    let spec = match strategy_type.as_str() {
        "donchian" => {
            let entry = params.first().and_then(|s| s.parse().ok()).unwrap_or(55);
            let exit = params.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
            StrategySpec::DonchianBreakout {
                entry_lookback: entry,
                exit_lookback: exit,
            }
        }
        "52wk_high" | "52wkhigh" | "fiftytwoweek" => {
            let period = params.first().and_then(|s| s.parse().ok()).unwrap_or(252);
            let entry_pct = params.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.95);
            let exit_pct = params.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.90);
            StrategySpec::FiftyTwoWeekHigh {
                period,
                entry_pct,
                exit_pct,
            }
        }
        "supertrend" => {
            let atr_period = params.first().and_then(|s| s.parse().ok()).unwrap_or(10);
            let multiplier = params.get(1).and_then(|s| s.parse().ok()).unwrap_or(3.0);
            StrategySpec::Supertrend {
                atr_period,
                multiplier,
            }
        }
        "psar" | "parabolic_sar" => {
            let af_start = params.first().and_then(|s| s.parse().ok()).unwrap_or(0.02);
            let af_step = params.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.02);
            let af_max = params.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.2);
            StrategySpec::ParabolicSar {
                af_start,
                af_step,
                af_max,
            }
        }
        "ma_cross" | "macross" | "ma_crossover" => {
            use trendlab_core::MAType;
            let fast = params.first().and_then(|s| s.parse().ok()).unwrap_or(50);
            let slow = params.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
            let ma_type = params
                .get(2)
                .map(|s| match s.to_lowercase().as_str() {
                    "ema" => MAType::EMA,
                    _ => MAType::SMA,
                })
                .unwrap_or(MAType::SMA);
            StrategySpec::MACrossover {
                fast_period: fast,
                slow_period: slow,
                ma_type,
            }
        }
        "tsmom" => {
            let lookback = params.first().and_then(|s| s.parse().ok()).unwrap_or(252);
            StrategySpec::Tsmom { lookback }
        }
        "aroon" => {
            let period = params.first().and_then(|s| s.parse().ok()).unwrap_or(25);
            StrategySpec::Aroon { period }
        }
        "keltner" => {
            let ema_period = params.first().and_then(|s| s.parse().ok()).unwrap_or(20);
            let atr_period = params.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
            let multiplier = params.get(2).and_then(|s| s.parse().ok()).unwrap_or(2.0);
            StrategySpec::Keltner {
                ema_period,
                atr_period,
                multiplier,
            }
        }
        "heikin_ashi" | "heikinashi" => {
            let confirmation_bars = params.first().and_then(|s| s.parse().ok()).unwrap_or(3);
            StrategySpec::HeikinAshi { confirmation_bars }
        }
        "dmi" | "dmi_adx" => {
            let di_period = params.first().and_then(|s| s.parse().ok()).unwrap_or(14);
            let adx_period = params.get(1).and_then(|s| s.parse().ok()).unwrap_or(14);
            let adx_threshold = params.get(2).and_then(|s| s.parse().ok()).unwrap_or(25.0);
            StrategySpec::DmiAdx {
                di_period,
                adx_period,
                adx_threshold,
            }
        }
        _ => return Err(anyhow!("Unknown strategy type: {}", strategy_type)),
    };

    Ok((spec, strategy_type, params_str))
}

// =============================================================================
// Core Scan Logic
// =============================================================================

/// Compute the current signal for a ticker-strategy pair.
/// Only returns Entry/Exit if it's a fresh transition (signal just fired in last `freshness_bars`).
fn compute_signal(
    symbol: &str,
    strategy_str: &str,
    data_config: &DataConfig,
    freshness_bars: usize,
) -> Result<TickerSignal> {
    // Parse strategy
    let (spec, strategy_type, params_str) = parse_strategy(strategy_str)?;

    // Load bars from Parquet
    let parquet_dir = data_config.parquet_dir();
    let lf = scan_symbol_parquet_lazy(&parquet_dir, symbol, "1d", None, None)
        .map_err(|e| anyhow!("Failed to load data for {}: {}", symbol, e))?;

    let df = lf
        .sort(["ts"], SortMultipleOptions::default())
        .collect()
        .map_err(|e| anyhow!("Failed to collect DataFrame for {}: {}", symbol, e))?;

    let bars = dataframe_to_bars(&df)
        .map_err(|e| anyhow!("Failed to convert to bars for {}: {}", symbol, e))?;

    if bars.is_empty() {
        return Err(anyhow!("No bars found for {}", symbol));
    }

    // Create strategy and check warmup
    let strategy = create_strategy_v2(&spec);
    let warmup = strategy.warmup_period();

    if bars.len() <= warmup + freshness_bars {
        return Err(anyhow!(
            "Insufficient data for {}: {} bars, need > {}",
            symbol,
            bars.len(),
            warmup + freshness_bars
        ));
    }

    let last_bar = bars.last().unwrap();

    // Get current signal
    let current_signal = strategy.signal(&bars, Position::Flat);
    let current_type: SignalType = current_signal.into();

    // For Entry/Exit signals, check if this is a fresh transition
    // by looking at where the signal first appeared
    let final_signal = match current_type {
        SignalType::Entry => {
            // Find how many bars back this entry signal started
            let bars_since_entry = find_signal_age(&bars, strategy.as_ref(), true);
            if bars_since_entry <= freshness_bars {
                SignalType::Entry
            } else {
                SignalType::Hold // Stale entry, already in position
            }
        }
        SignalType::Exit => {
            // Find how many bars back this exit signal started
            let bars_since_exit = find_signal_age(&bars, strategy.as_ref(), false);
            if bars_since_exit <= freshness_bars {
                SignalType::Exit
            } else {
                SignalType::Hold // Stale exit, already exited
            }
        }
        SignalType::Hold => SignalType::Hold,
    };

    Ok(TickerSignal {
        symbol: symbol.to_string(),
        strategy: strategy_type,
        params: params_str,
        signal: final_signal,
        close_price: last_bar.close,
        timestamp: last_bar.ts.to_rfc3339(),
    })
}

/// Find how many bars ago the current signal type first appeared.
/// Returns 1 if it just fired on the current bar, 2 if it fired yesterday, etc.
fn find_signal_age(
    bars: &[trendlab_core::Bar],
    strategy: &dyn trendlab_core::StrategyV2,
    is_entry: bool,
) -> usize {
    let n = bars.len();
    let max_lookback = 60.min(n - strategy.warmup_period()); // Don't look back more than 60 bars

    for i in 1..max_lookback {
        let check_idx = n - i;
        let slice = &bars[..check_idx];

        if slice.len() <= strategy.warmup_period() {
            break;
        }

        let signal = strategy.signal(slice, Position::Flat);
        let signal_type: SignalType = signal.into();

        let matches = if is_entry {
            signal_type == SignalType::Entry
        } else {
            signal_type == SignalType::Exit
        };

        if !matches {
            // Signal changed, so the current signal started at bar (i)
            return i;
        }
    }

    // Signal has been active for the entire lookback period
    max_lookback
}

/// Execute the full scan across all tickers and strategies.
///
/// # Arguments
/// * `freshness_bars` - Only report Entry/Exit signals that fired within this many bars.
///   Default is 2 (today or yesterday). Set to 0 to disable freshness check.
pub async fn execute_scan(
    watchlist_path: &Path,
    lookback_days: usize,
    actionable_only: bool,
    freshness_bars: usize,
    data_config: &DataConfig,
) -> Result<ScanOutput> {
    // Load watchlist
    let watchlist = WatchlistConfig::load(watchlist_path)?;

    // Determine date range
    let today = Utc::now().date_naive();
    let start = today - chrono::Duration::days(lookback_days as i64);

    // Refresh data for all tickers
    let tickers = watchlist.all_tickers();
    println!("Refreshing data for {} tickers...", tickers.len());
    refresh_yahoo(&tickers, start, today, false, data_config).await?;
    println!();

    // Run signal computation
    let mut signals = Vec::new();
    let mut errors = Vec::new();
    let mut total_checks = 0;

    // Use freshness_bars if set, otherwise use a large value to disable
    let effective_freshness = if freshness_bars == 0 {
        9999
    } else {
        freshness_bars
    };

    for ticker in &watchlist.tickers {
        let strategies = watchlist.strategies_for_ticker(ticker);

        for strategy_str in strategies {
            total_checks += 1;

            match compute_signal(
                &ticker.symbol,
                strategy_str,
                data_config,
                effective_freshness,
            ) {
                Ok(signal) => {
                    // Skip holds if actionable_only is set
                    if actionable_only && signal.signal == SignalType::Hold {
                        continue;
                    }
                    signals.push(signal);
                }
                Err(e) => {
                    errors.push(ScanError {
                        symbol: ticker.symbol.clone(),
                        strategy: strategy_str.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }
    }

    // Build output
    let entry_count = signals
        .iter()
        .filter(|s| s.signal == SignalType::Entry)
        .count();
    let exit_count = signals
        .iter()
        .filter(|s| s.signal == SignalType::Exit)
        .count();
    let hold_count = signals
        .iter()
        .filter(|s| s.signal == SignalType::Hold)
        .count();

    Ok(ScanOutput {
        scan_date: today.to_string(),
        scan_timestamp: Utc::now().to_rfc3339(),
        watchlist_name: watchlist.watchlist.name.clone(),
        signals,
        summary: ScanSummary {
            total_tickers: watchlist.tickers.len(),
            total_checks,
            entry_signals: entry_count,
            exit_signals: exit_count,
            hold_signals: hold_count,
            errors,
        },
    })
}

/// Format scan output for terminal display.
pub fn format_scan_output(output: &ScanOutput) -> String {
    use colored::Colorize;

    let mut result = String::new();

    result.push_str(&format!(
        "\n{} - {}\n",
        "Daily Signal Scan".bold(),
        output.scan_date.bright_blue()
    ));
    result.push_str(&format!("Watchlist: {}\n", output.watchlist_name));
    result.push_str(&format!("{:-<60}\n", ""));

    // Summary
    let strategies_per_ticker = if output.summary.total_tickers > 0 {
        output.summary.total_checks / output.summary.total_tickers
    } else {
        0
    };
    result.push_str(&format!(
        "Scanned {} tickers x {} strategies = {} checks\n",
        output.summary.total_tickers, strategies_per_ticker, output.summary.total_checks
    ));
    result.push_str(&format!(
        "Results: {} entries, {} exits, {} holds\n",
        output.summary.entry_signals.to_string().green(),
        output.summary.exit_signals.to_string().red(),
        output.summary.hold_signals.to_string().yellow()
    ));

    if !output.summary.errors.is_empty() {
        result.push_str(&format!(
            "Errors: {}\n",
            output.summary.errors.len().to_string().red()
        ));
    }

    result.push_str(&format!("{:-<60}\n", ""));

    // Collect actionable signals
    let entries: Vec<_> = output
        .signals
        .iter()
        .filter(|s| s.signal == SignalType::Entry)
        .collect();
    let exits: Vec<_> = output
        .signals
        .iter()
        .filter(|s| s.signal == SignalType::Exit)
        .collect();

    if !entries.is_empty() {
        result.push_str(&format!("\n{}\n", "ENTRY SIGNALS".green().bold()));
        for sig in &entries {
            result.push_str(&format!(
                "  {} - {} ({}) @ ${:.2}\n",
                sig.symbol.bright_white(),
                sig.strategy,
                sig.params,
                sig.close_price
            ));
        }
    }

    if !exits.is_empty() {
        result.push_str(&format!("\n{}\n", "EXIT SIGNALS".red().bold()));
        for sig in &exits {
            result.push_str(&format!(
                "  {} - {} ({}) @ ${:.2}\n",
                sig.symbol.bright_white(),
                sig.strategy,
                sig.params,
                sig.close_price
            ));
        }
    }

    if entries.is_empty() && exits.is_empty() {
        result.push_str(&format!("\n{}\n", "No actionable signals today.".yellow()));
    }

    // Errors
    if !output.summary.errors.is_empty() {
        result.push_str(&format!("\n{}\n", "ERRORS".red()));
        for err in &output.summary.errors {
            result.push_str(&format!(
                "  {} ({}): {}\n",
                err.symbol, err.strategy, err.error
            ));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_donchian() {
        let (spec, name, params) = parse_strategy("donchian:55,20").unwrap();
        assert_eq!(name, "donchian");
        assert_eq!(params, "55,20");
        assert!(matches!(
            spec,
            StrategySpec::DonchianBreakout {
                entry_lookback: 55,
                exit_lookback: 20
            }
        ));
    }

    #[test]
    fn test_parse_52wk_high() {
        let (spec, name, _) = parse_strategy("52wk_high:252,0.95,0.90").unwrap();
        assert_eq!(name, "52wk_high");
        assert!(matches!(
            spec,
            StrategySpec::FiftyTwoWeekHigh { period: 252, .. }
        ));
    }

    #[test]
    fn test_parse_supertrend() {
        let (spec, name, _) = parse_strategy("supertrend:10,3.0").unwrap();
        assert_eq!(name, "supertrend");
        assert!(matches!(
            spec,
            StrategySpec::Supertrend { atr_period: 10, .. }
        ));
    }

    #[test]
    fn test_watchlist_from_toml() {
        let toml = r#"
[watchlist]
name = "Test"
default_strategies = ["donchian:55,20"]

[[tickers]]
symbol = "AAPL"

[[tickers]]
symbol = "MSFT"
strategies = ["supertrend:10,3.0"]
"#;
        let config = WatchlistConfig::from_toml(toml).unwrap();
        assert_eq!(config.watchlist.name, "Test");
        assert_eq!(config.tickers.len(), 2);

        // AAPL uses defaults
        let aapl = &config.tickers[0];
        assert!(aapl.strategies.is_empty());
        assert_eq!(config.strategies_for_ticker(aapl), &["donchian:55,20"]);

        // MSFT has override
        let msft = &config.tickers[1];
        assert_eq!(config.strategies_for_ticker(msft), &["supertrend:10,3.0"]);
    }
}
