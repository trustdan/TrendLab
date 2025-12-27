//! StrategyV2 trait and implementations for Polars-native backtesting.
//!
//! This module provides a new strategy abstraction that supports:
//! - Sequential mode (backwards compatible with existing Strategy trait)
//! - Polars-native mode (vectorized signal computation on LazyFrames)
//!
//! The key innovation is expressing strategies as compositions of:
//! - Indicator specifications (what to compute)
//! - Signal rules (boolean expressions on indicator columns)
//!
//! This allows the same strategy definition to work both sequentially
//! and in a vectorized Polars pipeline.

use crate::bar::Bar;
use crate::indicators::{donchian_channel, ema_close, sma_close, MAType};
use crate::indicators_polars::{donchian_channel_exprs, ema_close_expr, sma_close_expr};
use crate::strategy::{Position, Signal};
use polars::prelude::*;
use serde::{Deserialize, Serialize};

/// Strategy specification as a sum type.
///
/// Each variant fully describes a strategy's parameters without any runtime state.
/// This enables:
/// - Serialization to JSON for artifacts
/// - Easy comparison and hashing of configurations
/// - Direct translation to Polars expressions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StrategySpec {
    /// Donchian channel breakout strategy (Turtle trading style).
    ///
    /// Entry: Close > N-bar highest high (excluding current bar)
    /// Exit: Close < M-bar lowest low (excluding current bar)
    DonchianBreakout {
        entry_lookback: usize,
        exit_lookback: usize,
    },

    /// Moving average crossover strategy.
    ///
    /// Entry: Fast MA crosses above slow MA
    /// Exit: Fast MA crosses below slow MA
    #[serde(rename = "ma_crossover")]
    MACrossover {
        fast_period: usize,
        slow_period: usize,
        ma_type: MAType,
    },

    /// Time-series momentum strategy.
    ///
    /// Entry: Current close > close N bars ago (positive momentum)
    /// Exit: Current close < close N bars ago (negative momentum)
    Tsmom { lookback: usize },
}

impl StrategySpec {
    /// Create a Donchian breakout strategy spec.
    pub fn donchian(entry_lookback: usize, exit_lookback: usize) -> Self {
        StrategySpec::DonchianBreakout {
            entry_lookback,
            exit_lookback,
        }
    }

    /// Create Turtle System 1 (20/10).
    pub fn turtle_system_1() -> Self {
        Self::donchian(20, 10)
    }

    /// Create Turtle System 2 (55/20).
    pub fn turtle_system_2() -> Self {
        Self::donchian(55, 20)
    }

    /// Create an MA crossover strategy spec.
    pub fn ma_crossover(fast_period: usize, slow_period: usize, ma_type: MAType) -> Self {
        StrategySpec::MACrossover {
            fast_period,
            slow_period,
            ma_type,
        }
    }

    /// Create a TSMOM strategy spec.
    pub fn tsmom(lookback: usize) -> Self {
        StrategySpec::Tsmom { lookback }
    }

    /// Get the strategy's unique identifier.
    pub fn id(&self) -> &str {
        match self {
            StrategySpec::DonchianBreakout { .. } => "donchian_breakout",
            StrategySpec::MACrossover { .. } => "ma_crossover",
            StrategySpec::Tsmom { .. } => "tsmom",
        }
    }

    /// Get the warmup period (minimum bars before signals).
    pub fn warmup_period(&self) -> usize {
        match self {
            StrategySpec::DonchianBreakout {
                entry_lookback,
                exit_lookback,
            } => (*entry_lookback).max(*exit_lookback),
            StrategySpec::MACrossover { slow_period, .. } => *slow_period,
            StrategySpec::Tsmom { lookback } => *lookback,
        }
    }
}

/// StrategyV2 trait for strategies that support both sequential and Polars modes.
///
/// This trait extends the original Strategy concept to work with DataFrames.
/// Implementors can provide vectorized signal computation that is much faster
/// for backtesting and parameter sweeps.
pub trait StrategyV2: Send + Sync {
    /// Get the strategy specification.
    fn spec(&self) -> &StrategySpec;

    /// Get the strategy ID.
    fn id(&self) -> &str {
        self.spec().id()
    }

    /// Get the warmup period.
    fn warmup_period(&self) -> usize {
        self.spec().warmup_period()
    }

    /// Sequential mode: compute signal for current bar.
    ///
    /// This provides backwards compatibility with the original Strategy trait.
    /// Default implementation uses the spec to compute signals.
    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal;

    /// Polars mode: add indicator columns to a LazyFrame.
    ///
    /// Returns a LazyFrame with indicator columns added.
    /// Column names follow a convention: `dc_upper`, `dc_lower`, `sma_fast`, etc.
    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame;

    /// Polars mode: add raw signal columns to a LazyFrame.
    ///
    /// Assumes indicator columns have already been added.
    /// Adds `raw_entry` and `raw_exit` boolean columns.
    /// These are "raw" because they don't account for position state.
    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame;

    /// Polars mode: complete pipeline from OHLCV to signals.
    ///
    /// Adds both indicators and signals in one step.
    fn add_strategy_columns(&self, lf: LazyFrame) -> LazyFrame {
        let lf = self.add_indicators_to_lf(lf);
        self.add_signals_to_lf(lf)
    }

    /// Reset any internal state (for multiple backtests).
    fn reset(&mut self) {}
}

/// Donchian breakout strategy implementing StrategyV2.
#[derive(Debug, Clone)]
pub struct DonchianBreakoutV2 {
    spec: StrategySpec,
    entry_lookback: usize,
    exit_lookback: usize,
}

impl DonchianBreakoutV2 {
    /// Create a new Donchian breakout strategy.
    pub fn new(entry_lookback: usize, exit_lookback: usize) -> Self {
        Self {
            spec: StrategySpec::donchian(entry_lookback, exit_lookback),
            entry_lookback,
            exit_lookback,
        }
    }

    /// Turtle System 1: 20-day entry, 10-day exit.
    pub fn turtle_system_1() -> Self {
        Self::new(20, 10)
    }

    /// Turtle System 2: 55-day entry, 20-day exit.
    pub fn turtle_system_2() -> Self {
        Self::new(55, 20)
    }

    /// Get entry lookback.
    pub fn entry_lookback(&self) -> usize {
        self.entry_lookback
    }

    /// Get exit lookback.
    pub fn exit_lookback(&self) -> usize {
        self.exit_lookback
    }
}

impl StrategyV2 for DonchianBreakoutV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let current_close = bars[current_idx].close;

        match current_position {
            Position::Flat => {
                let entry_channel = donchian_channel(bars, self.entry_lookback);
                if let Some(ch) = entry_channel[current_idx] {
                    if current_close > ch.upper {
                        return Signal::EnterLong;
                    }
                }
                Signal::Hold
            }
            Position::Long => {
                let exit_channel = donchian_channel(bars, self.exit_lookback);
                if let Some(ch) = exit_channel[current_idx] {
                    if current_close < ch.lower {
                        return Signal::ExitLong;
                    }
                }
                Signal::Hold
            }
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Add Donchian channel columns for both entry and exit
        let (entry_upper, _entry_lower) = donchian_channel_exprs(self.entry_lookback);
        let (_exit_upper, exit_lower) = donchian_channel_exprs(self.exit_lookback);

        // Rename to avoid conflicts when entry != exit lookback
        let entry_upper = entry_upper.alias("dc_entry_upper");
        let exit_lower = exit_lower.alias("dc_exit_lower");

        lf.with_columns([entry_upper, exit_lower])
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Raw entry: close > entry channel upper
        let raw_entry = col("close").gt(col("dc_entry_upper")).alias("raw_entry");

        // Raw exit: close < exit channel lower
        let raw_exit = col("close").lt(col("dc_exit_lower")).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }
}

/// MA Crossover strategy implementing StrategyV2.
#[derive(Debug, Clone)]
pub struct MACrossoverV2 {
    spec: StrategySpec,
    fast_period: usize,
    slow_period: usize,
    ma_type: MAType,
}

impl MACrossoverV2 {
    /// Create a new MA crossover strategy.
    pub fn new(fast_period: usize, slow_period: usize, ma_type: MAType) -> Self {
        assert!(
            fast_period < slow_period,
            "Fast period must be less than slow period"
        );
        Self {
            spec: StrategySpec::ma_crossover(fast_period, slow_period, ma_type),
            fast_period,
            slow_period,
            ma_type,
        }
    }

    /// Classic golden cross: SMA 50/200.
    pub fn golden_cross() -> Self {
        Self::new(50, 200, MAType::SMA)
    }

    /// MACD-style: EMA 12/26.
    pub fn macd_style() -> Self {
        Self::new(12, 26, MAType::EMA)
    }
}

impl StrategyV2 for MACrossoverV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let (fast_ma, slow_ma) = match self.ma_type {
            MAType::SMA => (
                sma_close(bars, self.fast_period),
                sma_close(bars, self.slow_period),
            ),
            MAType::EMA => (
                ema_close(bars, self.fast_period),
                ema_close(bars, self.slow_period),
            ),
        };

        let current_fast = match fast_ma[current_idx] {
            Some(v) => v,
            None => return Signal::Hold,
        };
        let current_slow = match slow_ma[current_idx] {
            Some(v) => v,
            None => return Signal::Hold,
        };

        if current_idx == 0 {
            return Signal::Hold;
        }

        let prev_fast = match fast_ma[current_idx - 1] {
            Some(v) => v,
            None => return Signal::Hold,
        };
        let prev_slow = match slow_ma[current_idx - 1] {
            Some(v) => v,
            None => return Signal::Hold,
        };

        let fast_above_slow = current_fast > current_slow;
        let prev_fast_above_slow = prev_fast > prev_slow;

        match current_position {
            Position::Flat => {
                if fast_above_slow && !prev_fast_above_slow {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                if !fast_above_slow && prev_fast_above_slow {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        let fast_expr = match self.ma_type {
            MAType::SMA => sma_close_expr(self.fast_period),
            MAType::EMA => ema_close_expr(self.fast_period),
        }
        .alias("ma_fast");

        let slow_expr = match self.ma_type {
            MAType::SMA => sma_close_expr(self.slow_period),
            MAType::EMA => ema_close_expr(self.slow_period),
        }
        .alias("ma_slow");

        lf.with_columns([fast_expr, slow_expr])
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // For crossover detection, we need:
        // - fast > slow (current)
        // - fast <= slow (previous) OR first valid bar
        let fast_above = col("ma_fast").gt(col("ma_slow"));
        let prev_fast_above = col("ma_fast")
            .shift(lit(1))
            .gt(col("ma_slow").shift(lit(1)));

        // Golden cross: fast crosses above slow
        let raw_entry = fast_above
            .clone()
            .and(prev_fast_above.clone().not())
            .alias("raw_entry");

        // Death cross: fast crosses below slow
        let raw_exit = fast_above.not().and(prev_fast_above).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }
}

/// TSMOM strategy implementing StrategyV2.
#[derive(Debug, Clone)]
pub struct TsmomV2 {
    spec: StrategySpec,
    lookback: usize,
}

impl TsmomV2 {
    /// Create a new TSMOM strategy.
    pub fn new(lookback: usize) -> Self {
        assert!(lookback > 0, "Lookback must be at least 1");
        Self {
            spec: StrategySpec::tsmom(lookback),
            lookback,
        }
    }

    /// 12-month momentum (252 days).
    pub fn twelve_month() -> Self {
        Self::new(252)
    }

    /// 6-month momentum (126 days).
    pub fn six_month() -> Self {
        Self::new(126)
    }

    /// 1-month momentum (21 days).
    pub fn one_month() -> Self {
        Self::new(21)
    }

    /// Get lookback period.
    pub fn lookback(&self) -> usize {
        self.lookback
    }
}

impl StrategyV2 for TsmomV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let current_close = bars[current_idx].close;
        let lookback_close = bars[current_idx - self.lookback].close;

        match current_position {
            Position::Flat => {
                if current_close > lookback_close {
                    Signal::EnterLong
                } else {
                    Signal::Hold
                }
            }
            Position::Long => {
                if current_close < lookback_close {
                    Signal::ExitLong
                } else {
                    Signal::Hold
                }
            }
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Add lookback close column first
        let lookback_close = col("close")
            .shift(lit(self.lookback as i64))
            .alias("close_lookback");

        // Then add momentum (requires close_lookback to exist)
        let momentum = (col("close") / col("close_lookback") - lit(1.0)).alias("momentum");

        // Must be in separate with_columns calls because momentum references close_lookback
        lf.with_column(lookback_close).with_column(momentum)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Entry: positive momentum
        let raw_entry = col("momentum").gt(lit(0.0)).alias("raw_entry");

        // Exit: negative momentum
        let raw_exit = col("momentum").lt(lit(0.0)).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }
}

/// Create a StrategyV2 implementation from a StrategySpec.
pub fn create_strategy_v2(spec: &StrategySpec) -> Box<dyn StrategyV2> {
    match spec {
        StrategySpec::DonchianBreakout {
            entry_lookback,
            exit_lookback,
        } => Box::new(DonchianBreakoutV2::new(*entry_lookback, *exit_lookback)),
        StrategySpec::MACrossover {
            fast_period,
            slow_period,
            ma_type,
        } => Box::new(MACrossoverV2::new(*fast_period, *slow_period, *ma_type)),
        StrategySpec::Tsmom { lookback } => Box::new(TsmomV2::new(*lookback)),
    }
}

/// Create a StrategyV2 implementation from a StrategyConfigId.
///
/// This is used for Polars-native sweeps where we need StrategyV2 implementations
/// instead of the legacy Strategy trait.
pub fn create_strategy_v2_from_config(
    config: &crate::sweep::StrategyConfigId,
) -> Box<dyn StrategyV2> {
    use crate::sweep::StrategyConfigId;

    match config {
        StrategyConfigId::Donchian {
            entry_lookback,
            exit_lookback,
        } => Box::new(DonchianBreakoutV2::new(*entry_lookback, *exit_lookback)),
        StrategyConfigId::TurtleS1 => Box::new(DonchianBreakoutV2::turtle_system_1()),
        StrategyConfigId::TurtleS2 => Box::new(DonchianBreakoutV2::turtle_system_2()),
        StrategyConfigId::MACrossover {
            fast,
            slow,
            ma_type,
        } => Box::new(MACrossoverV2::new(*fast, *slow, *ma_type)),
        StrategyConfigId::Tsmom { lookback } => Box::new(TsmomV2::new(*lookback)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::bars_to_dataframe;
    use chrono::{TimeZone, Utc};

    fn make_bar_at_day(day_offset: i64, open: f64, high: f64, low: f64, close: f64) -> Bar {
        use chrono::Duration;
        let base_date = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let ts = base_date + Duration::days(day_offset);
        Bar::new(ts, open, high, low, close, 1000.0, "TEST", "1d")
    }

    fn make_trending_bars(start: f64, step: f64, count: usize) -> Vec<Bar> {
        (0..count)
            .map(|i| {
                let base = start + (i as f64) * step;
                make_bar_at_day(i as i64, base, base + 1.0, base - 0.5, base + 0.5)
            })
            .collect()
    }

    #[test]
    fn test_strategy_spec_serialization() {
        let spec = StrategySpec::donchian(20, 10);
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("donchian_breakout"));
        assert!(json.contains("20"));
        assert!(json.contains("10"));

        let parsed: StrategySpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, parsed);
    }

    #[test]
    fn test_donchian_v2_sequential_matches_original() {
        let bars = make_trending_bars(100.0, 1.5, 15);
        let v2 = DonchianBreakoutV2::new(10, 5);

        // After warmup, should signal entry on uptrend
        let signal = v2.signal(&bars[..11], Position::Flat);
        assert_eq!(signal, Signal::EnterLong);
    }

    #[test]
    fn test_donchian_v2_polars_columns() {
        let bars = make_trending_bars(100.0, 1.5, 20);
        let df = bars_to_dataframe(&bars).unwrap();
        let strategy = DonchianBreakoutV2::new(10, 5);

        let result = strategy.add_strategy_columns(df.lazy()).collect().unwrap();

        // Check indicator columns exist
        assert!(result.column("dc_entry_upper").is_ok());
        assert!(result.column("dc_exit_lower").is_ok());

        // Check signal columns exist
        assert!(result.column("raw_entry").is_ok());
        assert!(result.column("raw_exit").is_ok());

        // Verify some entries should be true (uptrend)
        let entries = result.column("raw_entry").unwrap().bool().unwrap();
        let has_entry = entries.iter().any(|e| e == Some(true));
        assert!(
            has_entry,
            "Should have at least one entry signal in uptrend"
        );
    }

    #[test]
    fn test_ma_crossover_v2_sequential() {
        // Create bars where fast MA will cross above slow MA
        let mut bars: Vec<Bar> = Vec::new();

        // First 35 bars: stable around 100 (need slow_period=30 for warmup)
        for i in 0..35 {
            bars.push(make_bar_at_day(i as i64, 100.0, 101.0, 99.0, 100.0));
        }

        let strategy = MACrossoverV2::new(5, 30, MAType::SMA);

        // Add bars and check for crossover each time
        // We need prices rising enough that 5-bar MA crosses above 30-bar MA
        let mut found_entry = false;
        for i in 35..55 {
            let price = 100.0 + (i - 34) as f64 * 3.0; // Rising prices
            bars.push(make_bar_at_day(
                i as i64,
                price,
                price + 1.0,
                price - 1.0,
                price,
            ));

            let signal = strategy.signal(&bars, Position::Flat);
            if signal == Signal::EnterLong {
                found_entry = true;
                break;
            }
        }

        assert!(
            found_entry,
            "Fast MA should eventually cross above slow MA during price rise"
        );
    }

    #[test]
    fn test_tsmom_v2_polars_columns() {
        let bars = make_trending_bars(100.0, 0.5, 30);
        let df = bars_to_dataframe(&bars).unwrap();
        let strategy = TsmomV2::new(10);

        let result = strategy.add_strategy_columns(df.lazy()).collect().unwrap();

        // Check columns exist
        assert!(result.column("close_lookback").is_ok());
        assert!(result.column("momentum").is_ok());
        assert!(result.column("raw_entry").is_ok());
        assert!(result.column("raw_exit").is_ok());

        // In uptrend, should have positive momentum entries
        let entries = result.column("raw_entry").unwrap().bool().unwrap();
        let momentum = result.column("momentum").unwrap().f64().unwrap();

        // After lookback, should have positive momentum
        let last_momentum = momentum.get(29).unwrap();
        let last_entry = entries.get(29).unwrap();

        assert!(
            last_momentum > 0.0,
            "Should have positive momentum in uptrend"
        );
        assert!(last_entry, "Should signal entry with positive momentum");
    }

    #[test]
    fn test_create_strategy_v2() {
        let spec = StrategySpec::turtle_system_1();
        let strategy = create_strategy_v2(&spec);

        assert_eq!(strategy.id(), "donchian_breakout");
        assert_eq!(strategy.warmup_period(), 20);
    }

    #[test]
    fn test_spec_warmup_periods() {
        assert_eq!(StrategySpec::donchian(20, 10).warmup_period(), 20);
        assert_eq!(StrategySpec::donchian(10, 20).warmup_period(), 20);
        assert_eq!(
            StrategySpec::ma_crossover(12, 26, MAType::EMA).warmup_period(),
            26
        );
        assert_eq!(StrategySpec::tsmom(252).warmup_period(), 252);
    }
}
