//! Polars-native backtest kernel.
//!
//! Provides a fully vectorized backtest implementation using Polars LazyFrames.
//! The key challenge is the position state machine (Short↔Flat↔Long) which
//! requires sequential logic. We solve this using a stateful scan.
//!
//! Assumptions (same as sequential backtest):
//! - Signals are computed on bar close
//! - Fills occur on the next bar open
//! - Supports Long-only, Short-only, or Long/Short trading modes
//!
//! Position States:
//! - -1 = Short (negative position)
//! -  0 = Flat (no position)
//! -  1 = Long (positive position)

use crate::backtest::{BacktestResult, CostModel, EquityPoint, Fill, Side, Trade, TradeDirection};
use crate::error::{Result, TrendLabError};
use crate::indicators_polars::donchian_channel_exprs;
use crate::strategy_v2::StrategyV2;
use chrono::{TimeZone, Utc};
use polars::prelude::*;

/// Generic configuration for Polars-native backtests.
/// Works with any strategy that implements StrategyV2.
#[derive(Debug, Clone)]
pub struct PolarsBacktestConfig {
    /// Initial cash
    pub initial_cash: f64,
    /// Position quantity per trade
    pub qty: f64,
    /// Cost model (fees and slippage)
    pub cost_model: CostModel,
    /// Trading mode (LongOnly, ShortOnly, or LongShort)
    pub trading_mode: crate::strategy::TradingMode,
}

impl Default for PolarsBacktestConfig {
    fn default() -> Self {
        Self {
            initial_cash: 100_000.0,
            qty: 1.0,
            cost_model: CostModel::default(),
            trading_mode: crate::strategy::TradingMode::LongOnly,
        }
    }
}

impl PolarsBacktestConfig {
    pub fn new(initial_cash: f64, qty: f64) -> Self {
        Self {
            initial_cash,
            qty,
            cost_model: CostModel::default(),
            trading_mode: crate::strategy::TradingMode::LongOnly,
        }
    }

    pub fn with_cost_model(mut self, cost_model: CostModel) -> Self {
        self.cost_model = cost_model;
        self
    }

    pub fn with_trading_mode(mut self, trading_mode: crate::strategy::TradingMode) -> Self {
        self.trading_mode = trading_mode;
        self
    }
}

/// Configuration for Polars-native Donchian backtest.
#[derive(Debug, Clone)]
pub struct DonchianBacktestConfig {
    /// Entry channel lookback period
    pub entry_lookback: usize,
    /// Exit channel lookback period
    pub exit_lookback: usize,
    /// Initial cash
    pub initial_cash: f64,
    /// Position quantity per trade
    pub qty: f64,
    /// Cost model (fees and slippage)
    pub cost_model: CostModel,
}

impl Default for DonchianBacktestConfig {
    fn default() -> Self {
        Self {
            entry_lookback: 20,
            exit_lookback: 10,
            initial_cash: 100_000.0,
            qty: 1.0,
            cost_model: CostModel::default(),
        }
    }
}

impl DonchianBacktestConfig {
    pub fn new(entry_lookback: usize, exit_lookback: usize) -> Self {
        Self {
            entry_lookback,
            exit_lookback,
            ..Default::default()
        }
    }

    pub fn with_initial_cash(mut self, cash: f64) -> Self {
        self.initial_cash = cash;
        self
    }

    pub fn with_qty(mut self, qty: f64) -> Self {
        self.qty = qty;
        self
    }

    pub fn with_cost_model(mut self, cost_model: CostModel) -> Self {
        self.cost_model = cost_model;
        self
    }

    /// Convert to generic PolarsBacktestConfig.
    pub fn to_generic(&self) -> PolarsBacktestConfig {
        PolarsBacktestConfig {
            initial_cash: self.initial_cash,
            qty: self.qty,
            cost_model: self.cost_model,
            trading_mode: crate::strategy::TradingMode::LongOnly,
        }
    }
}

impl From<&DonchianBacktestConfig> for PolarsBacktestConfig {
    fn from(config: &DonchianBacktestConfig) -> Self {
        config.to_generic()
    }
}

/// Result from Polars-native backtest with DataFrame format.
#[derive(Debug, Clone)]
pub struct PolarsBacktestResult {
    /// Full DataFrame with all computed columns
    pub df: DataFrame,
    /// Summary metrics (computed lazily)
    pub final_equity: f64,
    pub total_return: f64,
    pub num_trades: usize,
}

impl PolarsBacktestResult {
    /// Convert to traditional BacktestResult for compatibility.
    pub fn to_backtest_result(&self) -> Result<BacktestResult> {
        let n = self.df.height();
        if n == 0 {
            return Ok(BacktestResult {
                fills: vec![],
                trades: vec![],
                pyramid_trades: vec![],
                equity: vec![],
            });
        }

        // Extract columns
        let ts_col = self
            .df
            .column("ts")
            .map_err(TrendLabError::Polars)?
            .datetime()
            .map_err(TrendLabError::Polars)?;
        let cash_col = self
            .df
            .column("cash")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;
        let position_qty_col = self
            .df
            .column("position_qty")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;
        let close_col = self
            .df
            .column("close")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;
        let equity_col = self
            .df
            .column("equity")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;

        // Build equity points
        let mut equity = Vec::with_capacity(n);
        for i in 0..n {
            let ts_ms = ts_col.get(i).unwrap_or(0);
            let ts = Utc
                .timestamp_millis_opt(ts_ms)
                .single()
                .unwrap_or_else(Utc::now);

            equity.push(EquityPoint {
                ts,
                cash: cash_col.get(i).unwrap_or(0.0),
                position_qty: position_qty_col.get(i).unwrap_or(0.0),
                close: close_col.get(i).unwrap_or(0.0),
                equity: equity_col.get(i).unwrap_or(0.0),
            });
        }

        // Extract fills and trades
        let (fills, trades) = self.extract_fills_and_trades()?;

        Ok(BacktestResult {
            fills,
            trades,
            pyramid_trades: vec![],
            equity,
        })
    }

    fn extract_fills_and_trades(&self) -> Result<(Vec<Fill>, Vec<Trade>)> {
        // Get signal columns for long trades
        let entry_fill_col = self
            .df
            .column("entry_fill")
            .map_err(TrendLabError::Polars)?
            .bool()
            .map_err(TrendLabError::Polars)?;
        let exit_fill_col = self
            .df
            .column("exit_fill")
            .map_err(TrendLabError::Polars)?
            .bool()
            .map_err(TrendLabError::Polars)?;

        // Get signal columns for short trades (optional, default to false)
        let entry_short_fill_col = self
            .df
            .column("entry_short_fill")
            .ok()
            .and_then(|c| c.bool().ok());
        let exit_short_fill_col = self
            .df
            .column("exit_short_fill")
            .ok()
            .and_then(|c| c.bool().ok());

        let ts_col = self
            .df
            .column("ts")
            .map_err(TrendLabError::Polars)?
            .datetime()
            .map_err(TrendLabError::Polars)?;
        let open_col = self
            .df
            .column("open")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;
        let fill_price_col = self
            .df
            .column("fill_price")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;
        let fill_fees_col = self
            .df
            .column("fill_fees")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;
        let fill_qty_col = self
            .df
            .column("fill_qty")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;

        let mut fills = Vec::new();
        let mut trades = Vec::new();
        let mut current_long_entry: Option<Fill> = None;
        let mut current_short_entry: Option<Fill> = None;

        let n = self.df.height();
        for i in 0..n {
            let is_long_entry = entry_fill_col.get(i).unwrap_or(false);
            let is_long_exit = exit_fill_col.get(i).unwrap_or(false);
            let is_short_entry = entry_short_fill_col
                .as_ref()
                .and_then(|c| c.get(i))
                .unwrap_or(false);
            let is_short_exit = exit_short_fill_col
                .as_ref()
                .and_then(|c| c.get(i))
                .unwrap_or(false);

            if is_long_entry || is_long_exit || is_short_entry || is_short_exit {
                let ts_ms = ts_col.get(i).unwrap_or(0);
                let ts = Utc
                    .timestamp_millis_opt(ts_ms)
                    .single()
                    .unwrap_or_else(Utc::now);
                let raw_price = open_col.get(i).unwrap_or(0.0);
                let price = fill_price_col.get(i).unwrap_or(raw_price);
                let fees = fill_fees_col.get(i).unwrap_or(0.0);
                let qty = fill_qty_col.get(i).unwrap_or(0.0);

                // Determine side based on fill type
                let side = if is_long_entry || is_short_exit {
                    Side::Buy // Buy to open long, or buy to cover short
                } else {
                    Side::Sell // Sell to close long, or sell to open short
                };

                let fill = Fill {
                    ts,
                    side,
                    qty,
                    price,
                    fees,
                    raw_price,
                    atr_at_fill: None,
                };

                fills.push(fill.clone());

                // Handle long trades
                if is_long_entry {
                    current_long_entry = Some(fill.clone());
                } else if is_long_exit {
                    if let Some(entry) = current_long_entry.take() {
                        let gross_pnl = (fill.price - entry.price) * entry.qty;
                        let net_pnl = gross_pnl - entry.fees - fill.fees;
                        trades.push(Trade {
                            entry,
                            exit: fill.clone(),
                            gross_pnl,
                            net_pnl,
                            direction: TradeDirection::Long,
                        });
                    }
                }

                // Handle short trades
                if is_short_entry {
                    current_short_entry = Some(fill.clone());
                } else if is_short_exit {
                    if let Some(entry) = current_short_entry.take() {
                        // Short PnL: profit when exit price < entry price
                        let gross_pnl = (entry.price - fill.price) * entry.qty;
                        let net_pnl = gross_pnl - entry.fees - fill.fees;
                        trades.push(Trade {
                            entry,
                            exit: fill,
                            gross_pnl,
                            net_pnl,
                            direction: TradeDirection::Short,
                        });
                    }
                }
            }
        }

        Ok((fills, trades))
    }
}

/// Run a Polars-native Donchian breakout backtest.
///
/// This is the main entry point for vectorized backtesting. It:
/// 1. Computes indicators using Polars expressions
/// 2. Generates raw entry/exit signals
/// 3. Applies the position state machine (stateful)
/// 4. Computes fills with next-bar-open execution
/// 5. Tracks equity over time
pub fn run_donchian_backtest_polars(
    lf: LazyFrame,
    config: &DonchianBacktestConfig,
) -> Result<PolarsBacktestResult> {
    // Validate config
    if config.initial_cash <= 0.0 {
        return Err(TrendLabError::Config("initial_cash must be > 0".into()));
    }
    if config.qty <= 0.0 {
        return Err(TrendLabError::Config("qty must be > 0".into()));
    }

    // 1. Add Donchian channel indicators for entry and exit
    let (entry_upper, entry_lower) = donchian_channel_exprs(config.entry_lookback);
    let (exit_upper, exit_lower) = donchian_channel_exprs(config.exit_lookback);

    let lf = lf.with_columns([
        entry_upper.alias("entry_upper"),
        entry_lower.alias("entry_lower"),
        exit_upper.alias("exit_upper"),
        exit_lower.alias("exit_lower"),
    ]);

    // 2. Compute raw entry/exit signals (ignoring position state)
    // Entry: close breaks above entry channel upper
    // Exit: close breaks below exit channel lower
    // Use standardized column names that match StrategyV2 trait
    let lf = lf.with_columns([
        col("close").gt(col("entry_upper")).alias("raw_entry"),
        col("close").lt(col("exit_lower")).alias("raw_exit"),
    ]);

    // Collect to DataFrame for stateful processing
    let df = lf.collect().map_err(TrendLabError::Polars)?;

    if df.height() == 0 {
        return Ok(PolarsBacktestResult {
            df,
            final_equity: config.initial_cash,
            total_return: 0.0,
            num_trades: 0,
        });
    }

    // 3. Apply position state machine (sequential - cannot be vectorized)
    let generic_config = config.to_generic();
    let df = apply_position_state_machine(df, &generic_config)?;

    // Compute summary metrics
    let equity_col = df
        .column("equity")
        .map_err(TrendLabError::Polars)?
        .f64()
        .map_err(TrendLabError::Polars)?;

    let final_equity = equity_col
        .get(df.height() - 1)
        .unwrap_or(config.initial_cash);
    let total_return = (final_equity - config.initial_cash) / config.initial_cash;

    // Count trades (both long and short)
    let exit_fill_col = df
        .column("exit_fill")
        .map_err(TrendLabError::Polars)?
        .bool()
        .map_err(TrendLabError::Polars)?;
    let long_trades = exit_fill_col.sum().unwrap_or(0) as usize;

    // Count short trades if column exists
    let short_trades = df
        .column("exit_short_fill")
        .ok()
        .and_then(|c| c.bool().ok())
        .map(|c| c.sum().unwrap_or(0) as usize)
        .unwrap_or(0);

    let num_trades = long_trades + short_trades;

    Ok(PolarsBacktestResult {
        df,
        final_equity,
        total_return,
        num_trades,
    })
}

/// Run a Polars-native backtest with any StrategyV2 implementation.
///
/// This is the generic entry point for vectorized backtesting. It:
/// 1. Adds indicators using the strategy's `add_indicators_to_lf()` method
/// 2. Adds raw entry/exit signals using `add_signals_to_lf()` method
/// 3. Applies the position state machine (stateful)
/// 4. Computes fills with next-bar-open execution
/// 5. Tracks equity over time
pub fn run_backtest_polars<S: StrategyV2 + ?Sized>(
    lf: LazyFrame,
    strategy: &S,
    config: &PolarsBacktestConfig,
) -> Result<PolarsBacktestResult> {
    // Validate config
    if config.initial_cash <= 0.0 {
        return Err(TrendLabError::Config("initial_cash must be > 0".into()));
    }
    if config.qty <= 0.0 {
        return Err(TrendLabError::Config("qty must be > 0".into()));
    }

    // Add all strategy columns: indicators + long signals + short signals (based on trading mode)
    let lf = strategy.add_strategy_columns(lf);

    // Collect to DataFrame for stateful processing
    let df = lf.collect().map_err(TrendLabError::Polars)?;

    if df.height() == 0 {
        return Ok(PolarsBacktestResult {
            df,
            final_equity: config.initial_cash,
            total_return: 0.0,
            num_trades: 0,
        });
    }

    // 3. Apply position state machine (sequential - cannot be vectorized)
    let df = apply_position_state_machine(df, config)?;

    // Compute summary metrics
    let equity_col = df
        .column("equity")
        .map_err(TrendLabError::Polars)?
        .f64()
        .map_err(TrendLabError::Polars)?;

    let final_equity = equity_col
        .get(df.height() - 1)
        .unwrap_or(config.initial_cash);
    let total_return = (final_equity - config.initial_cash) / config.initial_cash;

    // Count trades (both long and short)
    let exit_fill_col = df
        .column("exit_fill")
        .map_err(TrendLabError::Polars)?
        .bool()
        .map_err(TrendLabError::Polars)?;
    let long_trades = exit_fill_col.sum().unwrap_or(0) as usize;

    // Count short trades if column exists
    let short_trades = df
        .column("exit_short_fill")
        .ok()
        .and_then(|c| c.bool().ok())
        .map(|c| c.sum().unwrap_or(0) as usize)
        .unwrap_or(0);

    let num_trades = long_trades + short_trades;

    Ok(PolarsBacktestResult {
        df,
        final_equity,
        total_return,
        num_trades,
    })
}

/// Apply position state machine to compute valid signals, fills, and equity.
///
/// This is inherently sequential but we process it efficiently in a single pass.
/// Expects DataFrame to have `raw_entry` and `raw_exit` boolean columns for long signals.
/// Optionally supports `raw_entry_short` and `raw_exit_short` for short signals.
///
/// Position states:
/// - -1 = Short (negative position)
/// -  0 = Flat (no position)
/// -  1 = Long (positive position)
fn apply_position_state_machine(
    mut df: DataFrame,
    config: &PolarsBacktestConfig,
) -> Result<DataFrame> {
    let n = df.height();

    // Extract required columns - use standardized names from StrategyV2 trait
    let raw_entry = df
        .column("raw_entry")
        .map_err(TrendLabError::Polars)?
        .bool()
        .map_err(TrendLabError::Polars)?;
    let raw_exit = df
        .column("raw_exit")
        .map_err(TrendLabError::Polars)?
        .bool()
        .map_err(TrendLabError::Polars)?;

    // Optional short signal columns (default to false if not present)
    let raw_entry_short = df
        .column("raw_entry_short")
        .ok()
        .and_then(|c| c.bool().ok());
    let raw_exit_short = df.column("raw_exit_short").ok().and_then(|c| c.bool().ok());

    let open_col = df
        .column("open")
        .map_err(TrendLabError::Polars)?
        .f64()
        .map_err(TrendLabError::Polars)?;
    let close_col = df
        .column("close")
        .map_err(TrendLabError::Polars)?
        .f64()
        .map_err(TrendLabError::Polars)?;

    // Output arrays - position_state: -1=Short, 0=Flat, 1=Long
    let mut position_state: Vec<i32> = Vec::with_capacity(n);
    let mut entry_fill: Vec<bool> = Vec::with_capacity(n); // Long entry
    let mut exit_fill: Vec<bool> = Vec::with_capacity(n); // Long exit
    let mut entry_short_fill: Vec<bool> = Vec::with_capacity(n); // Short entry
    let mut exit_short_fill: Vec<bool> = Vec::with_capacity(n); // Short exit (cover)
    let mut fill_price: Vec<f64> = Vec::with_capacity(n);
    let mut fill_fees: Vec<f64> = Vec::with_capacity(n);
    let mut fill_qty: Vec<f64> = Vec::with_capacity(n);
    let mut cash: Vec<f64> = Vec::with_capacity(n);
    let mut position_qty: Vec<f64> = Vec::with_capacity(n);
    let mut equity: Vec<f64> = Vec::with_capacity(n);

    // State variables
    let mut current_cash = config.initial_cash;
    let mut current_position_qty = 0.0; // Positive for long, negative for short
    let mut current_state = 0_i32; // -1=Short, 0=Flat, 1=Long

    // Pending signals from previous bar
    let mut pending_entry_long = false;
    let mut pending_exit_long = false;
    let mut pending_entry_short = false;
    let mut pending_exit_short = false;

    let fee_rate = config.cost_model.fees_bps_per_side / 10_000.0;
    let slippage_rate = config.cost_model.slippage_bps / 10_000.0;

    for i in 0..n {
        let open = open_col.get(i).unwrap_or(0.0);
        let close = close_col.get(i).unwrap_or(0.0);

        let mut is_entry_fill = false;
        let mut is_exit_fill = false;
        let mut is_entry_short_fill = false;
        let mut is_exit_short_fill = false;
        let mut bar_fill_price = 0.0;
        let mut bar_fill_fees = 0.0;
        let mut bar_fill_qty = 0.0;

        // Execute pending signals on this bar's open
        if i > 0 {
            if pending_entry_long && current_state == 0 {
                // Execute long entry (buy to open)
                let price = open * (1.0 + slippage_rate); // Slippage makes price worse for buyer
                let fees = price * config.qty * fee_rate;

                current_cash -= price * config.qty;
                current_cash -= fees;
                current_position_qty = config.qty;
                current_state = 1;

                is_entry_fill = true;
                bar_fill_price = price;
                bar_fill_fees = fees;
                bar_fill_qty = config.qty;
            } else if pending_exit_long && current_state == 1 {
                // Execute long exit (sell to close)
                let price = open * (1.0 - slippage_rate); // Slippage makes price worse for seller
                let fees = price * current_position_qty * fee_rate;

                current_cash += price * current_position_qty;
                current_cash -= fees;
                current_position_qty = 0.0;
                current_state = 0;

                is_exit_fill = true;
                bar_fill_price = price;
                bar_fill_fees = fees;
                bar_fill_qty = config.qty;
            } else if pending_entry_short && current_state == 0 {
                // Execute short entry (sell to open)
                // When shorting: receive cash from sale, but need to eventually buy back
                let price = open * (1.0 - slippage_rate); // Slippage makes price worse for seller
                let fees = price * config.qty * fee_rate;

                current_cash += price * config.qty; // Receive cash from short sale
                current_cash -= fees;
                current_position_qty = -config.qty; // Negative position
                current_state = -1;

                is_entry_short_fill = true;
                bar_fill_price = price;
                bar_fill_fees = fees;
                bar_fill_qty = config.qty;
            } else if pending_exit_short && current_state == -1 {
                // Execute short exit (buy to cover)
                let price = open * (1.0 + slippage_rate); // Slippage makes price worse for buyer
                let qty_to_cover = current_position_qty.abs();
                let fees = price * qty_to_cover * fee_rate;

                current_cash -= price * qty_to_cover; // Pay to buy back shares
                current_cash -= fees;
                current_position_qty = 0.0;
                current_state = 0;

                is_exit_short_fill = true;
                bar_fill_price = price;
                bar_fill_fees = fees;
                bar_fill_qty = qty_to_cover;
            }
        }

        // Reset pending signals
        pending_entry_long = false;
        pending_exit_long = false;
        pending_entry_short = false;
        pending_exit_short = false;

        // Record state after fills
        position_state.push(current_state);
        entry_fill.push(is_entry_fill);
        exit_fill.push(is_exit_fill);
        entry_short_fill.push(is_entry_short_fill);
        exit_short_fill.push(is_exit_short_fill);
        fill_price.push(bar_fill_price);
        fill_fees.push(bar_fill_fees);
        fill_qty.push(bar_fill_qty);
        cash.push(current_cash);
        position_qty.push(current_position_qty);

        // Equity calculation handles both long and short positions
        // For long: cash + qty * close
        // For short: cash + qty * close (qty is negative, so this reduces equity when price rises)
        equity.push(current_cash + current_position_qty * close);

        // Generate signals for next bar (only if we have indicator values)
        // Read raw signals
        let raw_entry_long = raw_entry.get(i).unwrap_or(false);
        let raw_exit_long = raw_exit.get(i).unwrap_or(false);
        let raw_entry_short_signal = raw_entry_short
            .as_ref()
            .and_then(|c| c.get(i))
            .unwrap_or(false);
        let raw_exit_short_signal = raw_exit_short
            .as_ref()
            .and_then(|c| c.get(i))
            .unwrap_or(false);

        // Filter signals based on trading mode
        use crate::strategy::TradingMode;
        let has_entry_long = match config.trading_mode {
            TradingMode::LongOnly | TradingMode::LongShort => raw_entry_long,
            TradingMode::ShortOnly => false,
        };
        let has_exit_long = match config.trading_mode {
            TradingMode::LongOnly | TradingMode::LongShort => raw_exit_long,
            TradingMode::ShortOnly => false,
        };
        let has_entry_short = match config.trading_mode {
            TradingMode::ShortOnly | TradingMode::LongShort => raw_entry_short_signal,
            TradingMode::LongOnly => false,
        };
        let has_exit_short = match config.trading_mode {
            TradingMode::ShortOnly | TradingMode::LongShort => raw_exit_short_signal,
            TradingMode::LongOnly => false,
        };

        // Generate pending signals based on current state
        match current_state {
            0 => {
                // Flat: can enter long or short (based on trading mode)
                if has_entry_long {
                    pending_entry_long = true;
                } else if has_entry_short {
                    pending_entry_short = true;
                }
            }
            1 => {
                // Long: can only exit long
                if has_exit_long {
                    pending_exit_long = true;
                }
            }
            -1 => {
                // Short: can only exit short
                if has_exit_short {
                    pending_exit_short = true;
                }
            }
            _ => {}
        }
    }

    // Add computed columns to DataFrame
    df = df
        .with_column(Series::new("position_state".into(), position_state))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("entry_fill".into(), entry_fill))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("exit_fill".into(), exit_fill))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("entry_short_fill".into(), entry_short_fill))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("exit_short_fill".into(), exit_short_fill))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("fill_price".into(), fill_price))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("fill_fees".into(), fill_fees))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("fill_qty".into(), fill_qty))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("cash".into(), cash))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("position_qty".into(), position_qty))
        .map_err(TrendLabError::Polars)?
        .clone();
    df = df
        .with_column(Series::new("equity".into(), equity))
        .map_err(TrendLabError::Polars)?
        .clone();

    Ok(df)
}

/// Run multiple Donchian backtests with different parameter combinations.
///
/// This is optimized for sweeps: indicators are computed once per unique lookback,
/// then reused across configurations that share the same lookback.
pub fn run_donchian_sweep_polars(
    lf: LazyFrame,
    configs: &[DonchianBacktestConfig],
) -> Result<Vec<PolarsBacktestResult>> {
    // Collect unique lookback periods
    let mut unique_entry_lookbacks: Vec<usize> = configs.iter().map(|c| c.entry_lookback).collect();
    let mut unique_exit_lookbacks: Vec<usize> = configs.iter().map(|c| c.exit_lookback).collect();
    unique_entry_lookbacks.sort();
    unique_entry_lookbacks.dedup();
    unique_exit_lookbacks.sort();
    unique_exit_lookbacks.dedup();

    // Build indicator set with all needed lookbacks
    let mut exprs: Vec<Expr> = Vec::new();

    for &lookback in &unique_entry_lookbacks {
        let (upper, lower) = donchian_channel_exprs(lookback);
        exprs.push(upper.alias(format!("entry_upper_{}", lookback)));
        exprs.push(lower.alias(format!("entry_lower_{}", lookback)));
    }
    for &lookback in &unique_exit_lookbacks {
        let (upper, lower) = donchian_channel_exprs(lookback);
        exprs.push(upper.alias(format!("exit_upper_{}", lookback)));
        exprs.push(lower.alias(format!("exit_lower_{}", lookback)));
    }

    // Compute all indicators at once
    let lf = lf.with_columns(exprs);
    let base_df = lf.collect().map_err(TrendLabError::Polars)?;

    // Run backtest for each config
    let mut results = Vec::with_capacity(configs.len());
    for config in configs {
        // Select appropriate indicator columns for this config
        let entry_upper_col = format!("entry_upper_{}", config.entry_lookback);
        let exit_lower_col = format!("exit_lower_{}", config.exit_lookback);

        // Create a view with renamed columns
        let df = base_df
            .clone()
            .lazy()
            .with_columns([
                col(&entry_upper_col).alias("entry_upper"),
                col(&exit_lower_col).alias("exit_lower"),
            ])
            .with_columns([
                col("close").gt(col("entry_upper")).alias("raw_entry"),
                col("close").lt(col("exit_lower")).alias("raw_exit"),
            ])
            .collect()
            .map_err(TrendLabError::Polars)?;

        let generic_config = config.to_generic();
        let df = apply_position_state_machine(df, &generic_config)?;

        // Compute summary metrics
        let equity_col = df
            .column("equity")
            .map_err(TrendLabError::Polars)?
            .f64()
            .map_err(TrendLabError::Polars)?;

        let final_equity = equity_col
            .get(df.height() - 1)
            .unwrap_or(config.initial_cash);
        let total_return = (final_equity - config.initial_cash) / config.initial_cash;

        // Count trades (both long and short)
        let exit_fill_col = df
            .column("exit_fill")
            .map_err(TrendLabError::Polars)?
            .bool()
            .map_err(TrendLabError::Polars)?;
        let long_trades = exit_fill_col.sum().unwrap_or(0) as usize;

        let short_trades = df
            .column("exit_short_fill")
            .ok()
            .and_then(|c| c.bool().ok())
            .map(|c| c.sum().unwrap_or(0) as usize)
            .unwrap_or(0);

        let num_trades = long_trades + short_trades;

        results.push(PolarsBacktestResult {
            df,
            final_equity,
            total_return,
            num_trades,
        });
    }

    Ok(results)
}

/// Run a Polars-native sweep with any StrategyGridConfig.
///
/// This is the unified sweep runner that works with all strategy types.
/// It uses the generic `run_backtest_polars()` function for each configuration.
///
/// Returns a SweepResult that is compatible with the sequential sweep results.
pub fn run_strategy_sweep_polars(
    df: &DataFrame,
    strategy_config: &crate::sweep::StrategyGridConfig,
    config: &PolarsBacktestConfig,
) -> Result<crate::sweep::SweepResult> {
    use crate::metrics::compute_metrics;
    use crate::strategy_v2::create_strategy_v2_from_config;
    use crate::sweep::{ConfigId, SweepConfigResult, SweepResult};
    use chrono::Utc;

    let sweep_id = format!(
        "polars_{}_{}_{}",
        strategy_config.strategy_type.id(),
        df.height(),
        Utc::now().format("%Y%m%d_%H%M%S")
    );
    let started_at = Utc::now();

    let configs = strategy_config.generate_configs();

    // Run each config through the Polars backtest
    // Strategies without V2 implementations are gracefully skipped
    let mut config_results = Vec::with_capacity(configs.len());

    for strategy_config_id in &configs {
        // Skip strategies without V2 implementations
        let strategy = match create_strategy_v2_from_config(strategy_config_id) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Run Polars backtest
        let polars_result = run_backtest_polars(df.clone().lazy(), strategy.as_ref(), config)?;

        // Convert to traditional BacktestResult
        let backtest_result = polars_result.to_backtest_result()?;

        // Compute metrics
        let metrics = compute_metrics(&backtest_result, config.initial_cash);

        // Create legacy ConfigId for compatibility
        let legacy_config_id = strategy_config_id.to_legacy_config_id();

        config_results.push(SweepConfigResult {
            config_id: ConfigId::new(
                legacy_config_id.entry_lookback,
                legacy_config_id.exit_lookback,
            ),
            backtest_result,
            metrics,
        });
    }

    let completed_at = Utc::now();

    Ok(SweepResult {
        sweep_id,
        config_results,
        started_at,
        completed_at,
    })
}

/// Run a Polars-native sweep using parallel execution.
///
/// This is the parallelized version of `run_strategy_sweep_polars()`.
/// For large sweeps, this provides significant performance improvements.
pub fn run_strategy_sweep_polars_parallel(
    df: &DataFrame,
    strategy_config: &crate::sweep::StrategyGridConfig,
    config: &PolarsBacktestConfig,
) -> Result<crate::sweep::SweepResult> {
    use crate::metrics::compute_metrics;
    use crate::strategy_v2::create_strategy_v2_from_config;
    use crate::sweep::{ConfigId, SweepConfigResult, SweepResult};
    use chrono::Utc;
    use rayon::prelude::*;

    let sweep_id = format!(
        "polars_{}_{}_{}",
        strategy_config.strategy_type.id(),
        df.height(),
        Utc::now().format("%Y%m%d_%H%M%S")
    );
    let started_at = Utc::now();

    let configs = strategy_config.generate_configs();

    // Run each config through the Polars backtest in parallel
    // Strategies without V2 implementations are gracefully skipped
    let config_results: Vec<SweepConfigResult> = configs
        .par_iter()
        .filter_map(|strategy_config_id| {
            // Skip strategies without V2 implementations
            let strategy = create_strategy_v2_from_config(strategy_config_id).ok()?;

            // Run Polars backtest
            let polars_result =
                run_backtest_polars(df.clone().lazy(), strategy.as_ref(), config).ok()?;

            // Convert to traditional BacktestResult
            let backtest_result = polars_result.to_backtest_result().ok()?;

            // Compute metrics
            let metrics = compute_metrics(&backtest_result, config.initial_cash);

            // Create legacy ConfigId for compatibility
            let legacy_config_id = strategy_config_id.to_legacy_config_id();

            Some(SweepConfigResult {
                config_id: ConfigId::new(
                    legacy_config_id.entry_lookback,
                    legacy_config_id.exit_lookback,
                ),
                backtest_result,
                metrics,
            })
        })
        .collect();

    let completed_at = Utc::now();

    Ok(SweepResult {
        sweep_id,
        config_results,
        started_at,
        completed_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::Bar;
    use crate::data::bars_to_dataframe;
    use chrono::TimeZone;

    fn make_trending_bars(n: usize, trend: f64) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
                    + chrono::Duration::days(i as i64);
                let base = 100.0 + (i as f64 * trend);
                let noise = (i as f64 * 0.1).sin() * 2.0;
                let open = base + noise;
                let close = base + noise + trend.signum();
                let high = open.max(close) + 1.0;
                let low = open.min(close) - 1.0;
                Bar::new(ts, open, high, low, close, 1000.0, "TEST", "1d")
            })
            .collect()
    }

    #[test]
    fn test_polars_backtest_empty() {
        let bars: Vec<Bar> = vec![];
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let config = DonchianBacktestConfig::new(5, 3);
        let result = run_donchian_backtest_polars(lf, &config).unwrap();

        assert_eq!(result.final_equity, config.initial_cash);
        assert_eq!(result.num_trades, 0);
    }

    #[test]
    fn test_polars_backtest_uptrend() {
        // Create bars with strong uptrend - should trigger entries
        let bars = make_trending_bars(50, 1.0);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let config = DonchianBacktestConfig::new(5, 3)
            .with_initial_cash(10000.0)
            .with_qty(10.0);
        let result = run_donchian_backtest_polars(lf, &config).unwrap();

        // Should have made money in uptrend
        assert!(
            result.total_return > 0.0,
            "Expected positive return in uptrend, got {:.4}",
            result.total_return
        );
    }

    #[test]
    fn test_polars_backtest_to_traditional() {
        let bars = make_trending_bars(30, 0.5);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let config = DonchianBacktestConfig::new(5, 3)
            .with_initial_cash(10000.0)
            .with_qty(5.0);
        let result = run_donchian_backtest_polars(lf, &config).unwrap();

        // Convert to traditional result
        let traditional = result.to_backtest_result().unwrap();

        // Check equity curve matches
        assert_eq!(traditional.equity.len(), 30);
        assert!((traditional.equity.last().unwrap().equity - result.final_equity).abs() < 0.01);
    }

    #[test]
    fn test_polars_sweep() {
        let bars = make_trending_bars(60, 0.5);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let configs = vec![
            DonchianBacktestConfig::new(5, 3),
            DonchianBacktestConfig::new(10, 5),
            DonchianBacktestConfig::new(20, 10),
        ];

        let results = run_donchian_sweep_polars(lf, &configs).unwrap();

        assert_eq!(results.len(), 3);
        for result in &results {
            assert!(result.df.height() == 60);
        }
    }

    #[test]
    fn test_polars_backtest_with_costs() {
        let bars = make_trending_bars(50, 1.0);
        let df = bars_to_dataframe(&bars).unwrap();

        // Without costs
        let config_no_cost = DonchianBacktestConfig::new(5, 3)
            .with_initial_cash(10000.0)
            .with_qty(10.0);
        let result_no_cost =
            run_donchian_backtest_polars(df.clone().lazy(), &config_no_cost).unwrap();

        // With costs
        let config_with_cost = DonchianBacktestConfig::new(5, 3)
            .with_initial_cash(10000.0)
            .with_qty(10.0)
            .with_cost_model(CostModel {
                fees_bps_per_side: 10.0, // 10 bps = 0.1%
                slippage_bps: 5.0,       // 5 bps = 0.05%
            });
        let result_with_cost = run_donchian_backtest_polars(df.lazy(), &config_with_cost).unwrap();

        // Costs should reduce returns
        assert!(
            result_with_cost.total_return < result_no_cost.total_return,
            "Expected costs to reduce returns"
        );
    }

    #[test]
    fn test_polars_vs_sequential_parity() {
        use crate::backtest::{run_backtest, BacktestConfig};
        use crate::strategy::DonchianBreakoutStrategy;

        // Use a deterministic dataset
        let bars = make_trending_bars(100, 0.5);

        let entry_lookback = 10;
        let exit_lookback = 5;
        let initial_cash = 10000.0;
        let qty = 10.0;

        // Run sequential backtest
        let mut strategy = DonchianBreakoutStrategy::new(entry_lookback, exit_lookback);
        let seq_config = BacktestConfig {
            initial_cash,
            qty,
            ..BacktestConfig::default()
        };
        let seq_result = run_backtest(&bars, &mut strategy, seq_config).unwrap();

        // Run Polars backtest
        let df = bars_to_dataframe(&bars).unwrap();
        let polars_config = DonchianBacktestConfig::new(entry_lookback, exit_lookback)
            .with_initial_cash(initial_cash)
            .with_qty(qty);
        let polars_result = run_donchian_backtest_polars(df.lazy(), &polars_config).unwrap();

        // Compare final equity
        let seq_final = seq_result.equity.last().unwrap().equity;
        let polars_final = polars_result.final_equity;

        // They should match within reasonable tolerance
        let tolerance = initial_cash * 0.001; // 0.1% tolerance
        assert!(
            (seq_final - polars_final).abs() < tolerance,
            "Equity mismatch: sequential={:.2} vs polars={:.2} (diff={:.2})",
            seq_final,
            polars_final,
            (seq_final - polars_final).abs()
        );

        // Compare number of trades
        assert_eq!(
            seq_result.trades.len(),
            polars_result.num_trades,
            "Trade count mismatch: sequential={} vs polars={}",
            seq_result.trades.len(),
            polars_result.num_trades
        );
    }

    #[test]
    fn test_ma_crossover_polars_vs_sequential_parity() {
        use crate::backtest::{run_backtest, BacktestConfig};
        use crate::indicators::MAType;
        use crate::strategy::MACrossoverStrategy;
        use crate::strategy_v2::MACrossoverV2;

        // Use a deterministic dataset with enough bars for slow MA
        let bars = make_trending_bars(150, 0.3);

        let fast_period = 10;
        let slow_period = 50;
        let initial_cash = 10000.0;
        let qty = 10.0;

        // Run sequential backtest
        let mut strategy = MACrossoverStrategy::new(fast_period, slow_period, MAType::SMA);
        let seq_config = BacktestConfig {
            initial_cash,
            qty,
            ..BacktestConfig::default()
        };
        let seq_result = run_backtest(&bars, &mut strategy, seq_config).unwrap();

        // Run Polars backtest using generic function
        let df = bars_to_dataframe(&bars).unwrap();
        let polars_strategy = MACrossoverV2::new(fast_period, slow_period, MAType::SMA);
        let polars_config = PolarsBacktestConfig::new(initial_cash, qty);
        let polars_result =
            run_backtest_polars(df.lazy(), &polars_strategy, &polars_config).unwrap();

        // Compare final equity
        let seq_final = seq_result.equity.last().unwrap().equity;
        let polars_final = polars_result.final_equity;

        // They should match within reasonable tolerance
        let tolerance = initial_cash * 0.001; // 0.1% tolerance
        assert!(
            (seq_final - polars_final).abs() < tolerance,
            "MA Crossover equity mismatch: sequential={:.2} vs polars={:.2} (diff={:.2})",
            seq_final,
            polars_final,
            (seq_final - polars_final).abs()
        );

        // Compare number of trades
        assert_eq!(
            seq_result.trades.len(),
            polars_result.num_trades,
            "MA Crossover trade count mismatch: sequential={} vs polars={}",
            seq_result.trades.len(),
            polars_result.num_trades
        );
    }

    #[test]
    fn test_tsmom_polars_vs_sequential_parity() {
        use crate::backtest::{run_backtest, BacktestConfig};
        use crate::strategy::TsmomStrategy;
        use crate::strategy_v2::TsmomV2;

        // Use a deterministic dataset with enough bars for lookback
        let bars = make_trending_bars(100, 0.5);

        let lookback = 21; // 1-month lookback
        let initial_cash = 10000.0;
        let qty = 10.0;

        // Run sequential backtest
        let mut strategy = TsmomStrategy::new(lookback);
        let seq_config = BacktestConfig {
            initial_cash,
            qty,
            ..BacktestConfig::default()
        };
        let seq_result = run_backtest(&bars, &mut strategy, seq_config).unwrap();

        // Run Polars backtest using generic function
        let df = bars_to_dataframe(&bars).unwrap();
        let polars_strategy = TsmomV2::new(lookback);
        let polars_config = PolarsBacktestConfig::new(initial_cash, qty);
        let polars_result =
            run_backtest_polars(df.lazy(), &polars_strategy, &polars_config).unwrap();

        // Compare final equity
        let seq_final = seq_result.equity.last().unwrap().equity;
        let polars_final = polars_result.final_equity;

        // They should match within reasonable tolerance
        let tolerance = initial_cash * 0.001; // 0.1% tolerance
        assert!(
            (seq_final - polars_final).abs() < tolerance,
            "TSMOM equity mismatch: sequential={:.2} vs polars={:.2} (diff={:.2})",
            seq_final,
            polars_final,
            (seq_final - polars_final).abs()
        );

        // Compare number of trades
        assert_eq!(
            seq_result.trades.len(),
            polars_result.num_trades,
            "TSMOM trade count mismatch: sequential={} vs polars={}",
            seq_result.trades.len(),
            polars_result.num_trades
        );
    }

    fn make_downtrending_bars(n: usize, trend: f64) -> Vec<Bar> {
        // Create bars that actually break below the Donchian lower channel.
        // For short entry signals, close must be < min(low) of previous lookback bars.
        // We achieve this by making each bar's close well below previous bars' lows.
        // With trend = -3.0 and range of 2.0, each bar's close will be ~3 units below
        // the previous bar's low, ensuring breakdowns.
        (0..n)
            .map(|i| {
                let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
                    + chrono::Duration::days(i as i64);
                // Each bar's range is well below the previous bar
                // trend should be negative, e.g., -3.0 for aggressive downtrend
                let base = 200.0 + (i as f64 * trend);
                let open = base + 1.0;
                let close = base - 1.0; // Close at lower end of range
                let high = base + 2.0;
                let low = base - 2.0; // Low extends below close
                Bar::new(ts, open, high, low, close, 1000.0, "TEST", "1d")
            })
            .collect()
    }

    #[test]
    fn test_polars_backtest_short_only_downtrend() {
        use crate::strategy::TradingMode;
        use crate::strategy_v2::DonchianBreakoutV2;

        // Create bars with strong downtrend - shorts should profit
        // Use -3.0 to ensure close breaks below the Donchian lower channel
        let bars = make_downtrending_bars(50, -3.0);
        let df = bars_to_dataframe(&bars).unwrap();

        let strategy = DonchianBreakoutV2::new(5, 3).trading_mode(TradingMode::ShortOnly);
        let config =
            PolarsBacktestConfig::new(10000.0, 10.0).with_trading_mode(TradingMode::ShortOnly);
        let result = run_backtest_polars(df.lazy(), &strategy, &config).unwrap();

        // Should have made money shorting in downtrend
        assert!(
            result.total_return > 0.0,
            "Expected positive return from shorts in downtrend, got {:.4}",
            result.total_return
        );

        // Verify we entered short positions (check position_state has -1 values)
        let states = result.df.column("position_state").unwrap().i32().unwrap();
        let short_count = states.iter().filter(|s| *s == Some(-1)).count();
        assert!(
            short_count > 0,
            "Expected to be in short position for some bars, found {} bars in short",
            short_count
        );

        // Also verify we actually entered shorts (entry_short_fill has true values)
        let entry_fills = result
            .df
            .column("entry_short_fill")
            .unwrap()
            .bool()
            .unwrap();
        let entry_count = entry_fills.sum().unwrap_or(0);
        assert!(
            entry_count > 0,
            "Expected at least one short entry fill, got {}",
            entry_count
        );
    }

    #[test]
    fn test_polars_backtest_long_short_mode() {
        use crate::strategy::TradingMode;
        use crate::strategy_v2::DonchianBreakoutV2;

        // Create mixed bars - some up, some down
        let mut bars = make_trending_bars(30, 1.0);
        let downtrend = make_downtrending_bars(30, -3.0); // Strong downtrend for short signals
        bars.extend(downtrend);

        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let strategy = DonchianBreakoutV2::new(5, 3).trading_mode(TradingMode::LongShort);
        let config =
            PolarsBacktestConfig::new(10000.0, 10.0).with_trading_mode(TradingMode::LongShort);
        let result = run_backtest_polars(lf, &strategy, &config).unwrap();

        // In LongShort mode, should take trades in both directions
        // At minimum, the strategy should execute without error
        assert!(
            result.df.height() == 60,
            "DataFrame should have all 60 bars"
        );
    }

    #[test]
    fn test_polars_short_trade_pnl_calculation() {
        use crate::strategy::TradingMode;
        use crate::strategy_v2::DonchianBreakoutV2;

        // Create a clear downtrend for short trading
        let bars = make_downtrending_bars(50, -3.0); // Strong downtrend for short signals
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let strategy = DonchianBreakoutV2::new(5, 3).trading_mode(TradingMode::ShortOnly);
        let config =
            PolarsBacktestConfig::new(10000.0, 10.0).with_trading_mode(TradingMode::ShortOnly);
        let result = run_backtest_polars(lf, &strategy, &config).unwrap();

        // Convert to traditional result to check trades
        let traditional = result.to_backtest_result().unwrap();

        // Verify short trades have correct PnL (profit when price drops)
        for trade in &traditional.trades {
            // For a profitable short: entry_price > exit_price => positive PnL
            // The PnL should be (entry_price - exit_price) * qty
            if trade.gross_pnl > 0.0 {
                assert!(
                    trade.entry.price > trade.exit.price,
                    "Profitable short trade should have entry > exit, got entry={:.2} exit={:.2}",
                    trade.entry.price,
                    trade.exit.price
                );
            }
        }
    }

    #[test]
    fn test_polars_short_position_state() {
        use crate::strategy::TradingMode;
        use crate::strategy_v2::DonchianBreakoutV2;
        use polars::prelude::*;

        // Create downtrend data
        let bars = make_downtrending_bars(30, -1.5);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let strategy = DonchianBreakoutV2::new(5, 3).trading_mode(TradingMode::ShortOnly);
        let config =
            PolarsBacktestConfig::new(10000.0, 10.0).with_trading_mode(TradingMode::ShortOnly);
        let result = run_backtest_polars(lf, &strategy, &config).unwrap();

        // Check that position_state includes -1 for short positions
        let pos_state_col = result.df.column("position_state").unwrap();
        let pos_states: Vec<Option<i32>> = pos_state_col.i32().unwrap().iter().collect();

        // Should have at least some short positions (-1)
        let has_short = pos_states.contains(&Some(-1));
        assert!(
            has_short,
            "Expected position_state=-1 for short positions in downtrend"
        );

        // Should NOT have any long positions in ShortOnly mode
        let has_long = pos_states.contains(&Some(1));
        assert!(
            !has_long,
            "Should not have long positions in ShortOnly mode"
        );
    }

    #[test]
    fn test_polars_short_consistency() {
        use crate::strategy::TradingMode;
        use crate::strategy_v2::DonchianBreakoutV2;

        // Use deterministic downtrend data - strong trend to trigger short entries
        let bars = make_downtrending_bars(100, -3.0);

        let entry_lookback = 10;
        let exit_lookback = 5;
        let initial_cash = 10000.0;
        let qty = 10.0;

        // Run Polars backtest with short mode
        let df = bars_to_dataframe(&bars).unwrap();
        let polars_strategy = DonchianBreakoutV2::new(entry_lookback, exit_lookback)
            .trading_mode(TradingMode::ShortOnly);
        let polars_config =
            PolarsBacktestConfig::new(initial_cash, qty).with_trading_mode(TradingMode::ShortOnly);
        let result = run_backtest_polars(df.lazy(), &polars_strategy, &polars_config).unwrap();

        // Verify short trades in downtrend should be profitable
        assert!(
            result.total_return > 0.0,
            "Expected positive return from shorts in downtrend, got {:.4}",
            result.total_return
        );

        // Final equity should be greater than initial cash
        assert!(
            result.final_equity > initial_cash,
            "Short strategy should profit in downtrend: final={:.2} initial={:.2}",
            result.final_equity,
            initial_cash
        );

        // Should have entered short positions (check position_state for -1 values)
        let states = result.df.column("position_state").unwrap().i32().unwrap();
        let short_count = states.iter().filter(|s| *s == Some(-1)).count();
        assert!(
            short_count > 0,
            "Expected short positions (position_state=-1) in 100-bar downtrend"
        );
    }

    #[test]
    fn test_polars_short_equity_accounting() {
        use crate::strategy::TradingMode;
        use crate::strategy_v2::DonchianBreakoutV2;
        use polars::prelude::*;

        // Downtrend data - use strong trend to trigger short entries
        let bars = make_downtrending_bars(40, -3.0);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let initial_cash = 10000.0;
        let qty = 10.0;
        let strategy = DonchianBreakoutV2::new(5, 3).trading_mode(TradingMode::ShortOnly);
        let config =
            PolarsBacktestConfig::new(initial_cash, qty).with_trading_mode(TradingMode::ShortOnly);
        let result = run_backtest_polars(lf, &strategy, &config).unwrap();

        // Check position_qty column - should be negative when short
        let pos_qty_col = result.df.column("position_qty").unwrap();
        let pos_qtys: Vec<Option<f64>> = pos_qty_col.f64().unwrap().iter().collect();

        // When in short position, position_qty should be negative
        let pos_state_col = result.df.column("position_state").unwrap();
        let pos_states: Vec<Option<i32>> = pos_state_col.i32().unwrap().iter().collect();

        for (i, (state, qty_opt)) in pos_states.iter().zip(pos_qtys.iter()).enumerate() {
            if *state == Some(-1) {
                let q = qty_opt.unwrap_or(0.0);
                assert!(
                    q < 0.0,
                    "At bar {}: position_state=-1 but position_qty={:.2} (expected negative)",
                    i,
                    q
                );
            }
        }
    }
}
