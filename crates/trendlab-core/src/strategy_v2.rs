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
use crate::indicators::{
    aroon, atr, darvas_boxes, dmi, donchian_channel, ema_close, heikin_ashi, keltner_channel,
    opening_range, parabolic_sar, range_breakout_levels, sma_close, starc_bands, supertrend,
    MACDEntryMode, MAType, OpeningPeriod,
};
use crate::indicators_polars::{
    apply_aroon_exprs, apply_dmi_exprs, apply_heikin_ashi_exprs, apply_keltner_exprs,
    apply_opening_range_exprs, apply_parabolic_sar_exprs, apply_starc_exprs,
    apply_supertrend_exprs, donchian_channel_exprs, ema_close_expr, sma_close_expr,
};
use crate::strategy::{Position, Signal, TradingMode, VotingMethod};
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

    /// 52-Week High trend-following strategy.
    ///
    /// Entry: Close >= entry_pct * period high (e.g., within 5% of high)
    /// Exit: Close < exit_pct * period high (e.g., 10% below high)
    FiftyTwoWeekHigh {
        period: usize,
        entry_pct: f64,
        exit_pct: f64,
    },

    /// DMI/ADX Directional Movement Index strategy (Welles Wilder).
    ///
    /// Entry: +DI > -DI AND ADX > threshold (bullish trend with strength)
    /// Exit: -DI >= +DI (bearish direction)
    DmiAdx {
        di_period: usize,
        adx_period: usize,
        adx_threshold: f64,
    },

    /// Bollinger Squeeze strategy (volatility breakout).
    ///
    /// Entry: In squeeze (or was in squeeze) AND close > upper band
    /// Exit: Close < middle band (SMA)
    BollingerSqueeze {
        period: usize,
        std_mult: f64,
        squeeze_threshold: f64,
    },

    /// Aroon Cross strategy (Tushar Chande).
    ///
    /// Entry: Aroon-Up crosses above Aroon-Down (bullish trend emerging)
    /// Exit: Aroon-Up crosses below Aroon-Down (bearish trend emerging)
    Aroon { period: usize },

    /// Keltner Channel breakout strategy.
    ///
    /// Entry: Close > upper band (EMA + mult * ATR)
    /// Exit: Close < lower band OR close < center (EMA)
    Keltner {
        ema_period: usize,
        atr_period: usize,
        multiplier: f64,
    },

    /// Heikin-Ashi regime change strategy.
    ///
    /// Entry: First bullish HA candle after N bearish candles (regime flip)
    /// Exit: First bearish HA candle
    ///
    /// Heikin-Ashi smooths price action to make trend identification easier.
    HeikinAshi { confirmation_bars: usize },

    /// STARC Bands strategy (Stoller Average Range Channel).
    ///
    /// Entry: Close > upper band (SMA + mult * ATR)
    /// Exit: Close < lower band (SMA - mult * ATR)
    ///
    /// Similar to Keltner but uses SMA instead of EMA for the center line.
    Starc {
        sma_period: usize,
        atr_period: usize,
        multiplier: f64,
    },

    /// Supertrend strategy.
    ///
    /// Entry: Trend flips from bearish to bullish (close crosses above supertrend line)
    /// Exit: Trend flips from bullish to bearish (close crosses below supertrend line)
    ///
    /// Supertrend uses ATR-based bands with ratcheting behavior.
    Supertrend { atr_period: usize, multiplier: f64 },

    /// Darvas Box breakout strategy (Nicolas Darvas).
    ///
    /// Entry: Close breaks above a confirmed box top
    /// Exit: Close breaks below box bottom
    ///
    /// Box formation requires N consecutive lower highs (top) and higher lows (bottom).
    DarvasBox { confirmation_bars: usize },

    /// Larry Williams Volatility Breakout strategy.
    ///
    /// Entry: Close > open + range_mult * prior_range (range expansion)
    /// Exit: ATR-based trailing stop (close < recent_high - atr_mult * ATR)
    LarryWilliams {
        range_mult: f64,
        atr_stop_mult: f64,
        atr_period: usize,
    },

    /// Opening Range Breakout strategy.
    ///
    /// Entry: Close breaks above range high (after range is complete)
    /// Exit: Close breaks below range low
    ///
    /// The opening range is defined by the first N bars of each period (weekly/monthly)
    /// or as a rolling lookback window.
    OpeningRangeBreakout {
        range_bars: usize,
        period: OpeningPeriod,
    },

    /// Parabolic SAR (Stop And Reverse) strategy.
    ///
    /// Entry: SAR flips from above price to below price (bullish reversal)
    /// Exit: SAR flips from below price to above price (bearish reversal)
    ///
    /// Uses Wilder's Parabolic SAR indicator with configurable acceleration factor.
    ParabolicSar {
        af_start: f64,
        af_step: f64,
        af_max: f64,
    },

    /// Multi-Horizon Ensemble strategy.
    ///
    /// Combines multiple child strategies with different parameterizations and
    /// aggregates their signals using a voting mechanism.
    ///
    /// Entry: Determined by voting method across child strategy signals
    /// Exit: Determined by voting method across child strategy signals
    Ensemble {
        /// Child strategy specifications (can be any strategy type)
        children: Vec<StrategySpec>,
        /// Horizons for each child (used for weighted voting)
        horizons: Vec<usize>,
        /// Voting method for signal aggregation
        voting: VotingMethod,
    },

    // =========================================================================
    // Phase 5: Oscillator Strategies
    // =========================================================================
    /// RSI (Relative Strength Index) strategy.
    ///
    /// Entry: RSI crosses above oversold threshold from below
    /// Exit: RSI crosses below overbought threshold from above
    Rsi {
        period: usize,
        oversold: f64,
        overbought: f64,
    },

    /// MACD (Moving Average Convergence Divergence) strategy.
    ///
    /// Entry: Based on entry_mode (CrossSignal, CrossZero, or Histogram)
    /// Exit: Opposite of entry condition
    Macd {
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
        entry_mode: MACDEntryMode,
    },

    /// Stochastic Oscillator strategy.
    ///
    /// Entry: %K crosses above %D when both are in oversold territory
    /// Exit: %K crosses below %D when both are in overbought territory
    Stochastic {
        k_period: usize,
        k_smooth: usize,
        d_period: usize,
        oversold: f64,
        overbought: f64,
    },

    /// Williams %R strategy.
    ///
    /// Entry: %R crosses above oversold threshold (-80) from below
    /// Exit: %R crosses below overbought threshold (-20) from above
    WilliamsR {
        period: usize,
        oversold: f64,
        overbought: f64,
    },

    /// CCI (Commodity Channel Index) strategy.
    ///
    /// Entry: CCI crosses above entry_threshold (+100)
    /// Exit: CCI crosses below exit_threshold (-100)
    Cci {
        period: usize,
        entry_threshold: f64,
        exit_threshold: f64,
    },

    /// ROC (Rate of Change) strategy.
    ///
    /// Entry: ROC crosses above 0 (positive momentum)
    /// Exit: ROC crosses below 0 (negative momentum)
    Roc { period: usize },

    /// RSI + Bollinger Bands hybrid strategy.
    ///
    /// Entry: RSI < oversold AND close <= lower Bollinger Band
    /// Exit: Close > middle band OR RSI > exit threshold
    RsiBollinger {
        rsi_period: usize,
        rsi_oversold: f64,
        rsi_exit: f64,
        bb_period: usize,
        bb_std_mult: f64,
    },

    /// MACD + ADX filter strategy.
    ///
    /// Entry: MACD crosses above signal AND ADX > threshold
    /// Exit: MACD crosses below signal
    MacdAdx {
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
        adx_period: usize,
        adx_threshold: f64,
    },

    /// Multi-oscillator confluence strategy.
    ///
    /// Entry: RSI bullish crossover AND Stochastic K > D
    /// Exit: RSI bearish crossover OR Stochastic K crosses below D
    OscillatorConfluence {
        rsi_period: usize,
        rsi_oversold: f64,
        rsi_overbought: f64,
        stoch_k_period: usize,
        stoch_k_smooth: usize,
        stoch_d_period: usize,
    },

    /// Ichimoku Cloud strategy.
    ///
    /// Entry: Price above cloud AND Tenkan crosses above Kijun
    /// Exit: Price below cloud OR Tenkan crosses below Kijun
    Ichimoku {
        tenkan_period: usize,
        kijun_period: usize,
        senkou_b_period: usize,
    },
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

    /// Create a 52-Week High strategy spec.
    pub fn fifty_two_week_high(period: usize, entry_pct: f64, exit_pct: f64) -> Self {
        StrategySpec::FiftyTwoWeekHigh {
            period,
            entry_pct,
            exit_pct,
        }
    }

    /// Create a DMI/ADX strategy spec.
    pub fn dmi_adx(di_period: usize, adx_period: usize, adx_threshold: f64) -> Self {
        StrategySpec::DmiAdx {
            di_period,
            adx_period,
            adx_threshold,
        }
    }

    /// Create a Bollinger Squeeze strategy spec.
    pub fn bollinger_squeeze(period: usize, std_mult: f64, squeeze_threshold: f64) -> Self {
        StrategySpec::BollingerSqueeze {
            period,
            std_mult,
            squeeze_threshold,
        }
    }

    /// Create an Aroon strategy spec.
    pub fn aroon(period: usize) -> Self {
        StrategySpec::Aroon { period }
    }

    /// Create a Keltner Channel strategy spec.
    pub fn keltner(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        StrategySpec::Keltner {
            ema_period,
            atr_period,
            multiplier,
        }
    }

    /// Create a Heikin-Ashi strategy spec.
    pub fn heikin_ashi(confirmation_bars: usize) -> Self {
        StrategySpec::HeikinAshi { confirmation_bars }
    }

    /// Create a STARC Bands strategy spec.
    pub fn starc(sma_period: usize, atr_period: usize, multiplier: f64) -> Self {
        StrategySpec::Starc {
            sma_period,
            atr_period,
            multiplier,
        }
    }

    /// Create a Supertrend strategy spec.
    pub fn supertrend(atr_period: usize, multiplier: f64) -> Self {
        StrategySpec::Supertrend {
            atr_period,
            multiplier,
        }
    }

    /// Create a Darvas Box strategy spec.
    pub fn darvas_box(confirmation_bars: usize) -> Self {
        StrategySpec::DarvasBox { confirmation_bars }
    }

    /// Create a Larry Williams strategy spec.
    pub fn larry_williams(range_mult: f64, atr_stop_mult: f64, atr_period: usize) -> Self {
        StrategySpec::LarryWilliams {
            range_mult,
            atr_stop_mult,
            atr_period,
        }
    }

    /// Create an Opening Range Breakout strategy spec.
    pub fn opening_range_breakout(range_bars: usize, period: OpeningPeriod) -> Self {
        StrategySpec::OpeningRangeBreakout { range_bars, period }
    }

    /// Create a Parabolic SAR strategy spec.
    pub fn parabolic_sar(af_start: f64, af_step: f64, af_max: f64) -> Self {
        StrategySpec::ParabolicSar {
            af_start,
            af_step,
            af_max,
        }
    }

    /// Create a standard Parabolic SAR (0.02/0.02/0.20 - Wilder's default).
    pub fn parabolic_sar_standard() -> Self {
        Self::parabolic_sar(0.02, 0.02, 0.20)
    }

    /// Create a slow Parabolic SAR (0.01/0.01/0.10 - fewer whipsaws).
    pub fn parabolic_sar_slow() -> Self {
        Self::parabolic_sar(0.01, 0.01, 0.10)
    }

    /// Create a fast Parabolic SAR (0.03/0.03/0.30 - quicker signals).
    pub fn parabolic_sar_fast() -> Self {
        Self::parabolic_sar(0.03, 0.03, 0.30)
    }

    /// Create an Ensemble strategy spec.
    pub fn ensemble(
        children: Vec<StrategySpec>,
        horizons: Vec<usize>,
        voting: VotingMethod,
    ) -> Self {
        assert!(
            !children.is_empty(),
            "Ensemble must have at least one child strategy"
        );
        assert_eq!(
            children.len(),
            horizons.len(),
            "Children and horizons must have same length"
        );
        StrategySpec::Ensemble {
            children,
            horizons,
            voting,
        }
    }

    /// Create a Donchian Triple ensemble (20/55/100 day breakouts).
    pub fn donchian_triple() -> Self {
        Self::ensemble(
            vec![
                Self::donchian(20, 10),
                Self::donchian(55, 20),
                Self::donchian(100, 40),
            ],
            vec![20, 55, 100],
            VotingMethod::Majority,
        )
    }

    /// Create an MA Triple ensemble (10/50/200 crossovers).
    pub fn ma_triple() -> Self {
        use crate::indicators::MAType;
        Self::ensemble(
            vec![
                Self::ma_crossover(5, 10, MAType::EMA),
                Self::ma_crossover(20, 50, MAType::SMA),
                Self::ma_crossover(50, 200, MAType::SMA),
            ],
            vec![10, 50, 200],
            VotingMethod::WeightedByHorizon,
        )
    }

    /// Create a TSMOM Multi ensemble (21/63/126/252 day momentum).
    pub fn tsmom_multi() -> Self {
        Self::ensemble(
            vec![
                Self::tsmom(21),
                Self::tsmom(63),
                Self::tsmom(126),
                Self::tsmom(252),
            ],
            vec![21, 63, 126, 252],
            VotingMethod::Majority,
        )
    }

    /// Get the strategy's unique identifier.
    pub fn id(&self) -> &str {
        match self {
            StrategySpec::DonchianBreakout { .. } => "donchian_breakout",
            StrategySpec::MACrossover { .. } => "ma_crossover",
            StrategySpec::Tsmom { .. } => "tsmom",
            StrategySpec::FiftyTwoWeekHigh { .. } => "52wk_high",
            StrategySpec::DmiAdx { .. } => "dmi_adx",
            StrategySpec::BollingerSqueeze { .. } => "bollinger_squeeze",
            StrategySpec::Aroon { .. } => "aroon",
            StrategySpec::Keltner { .. } => "keltner",
            StrategySpec::HeikinAshi { .. } => "heikin_ashi",
            StrategySpec::Starc { .. } => "starc",
            StrategySpec::Supertrend { .. } => "supertrend",
            StrategySpec::DarvasBox { .. } => "darvas_box",
            StrategySpec::LarryWilliams { .. } => "larry_williams",
            StrategySpec::OpeningRangeBreakout { .. } => "opening_range_breakout",
            StrategySpec::ParabolicSar { .. } => "parabolic_sar",
            StrategySpec::Ensemble { .. } => "ensemble",
            // Phase 5: Oscillator strategies
            StrategySpec::Rsi { .. } => "rsi",
            StrategySpec::Macd { .. } => "macd",
            StrategySpec::Stochastic { .. } => "stochastic",
            StrategySpec::WilliamsR { .. } => "williams_r",
            StrategySpec::Cci { .. } => "cci",
            StrategySpec::Roc { .. } => "roc",
            StrategySpec::RsiBollinger { .. } => "rsi_bollinger",
            StrategySpec::MacdAdx { .. } => "macd_adx",
            StrategySpec::OscillatorConfluence { .. } => "oscillator_confluence",
            StrategySpec::Ichimoku { .. } => "ichimoku",
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
            StrategySpec::FiftyTwoWeekHigh { period, .. } => *period,
            // DMI requires double smoothing: once for DI, once for ADX
            StrategySpec::DmiAdx {
                di_period,
                adx_period,
                ..
            } => 2 * (*di_period).max(*adx_period),
            // Bollinger needs period for SMA/std + 1 for prev squeeze detection
            StrategySpec::BollingerSqueeze { period, .. } => *period + 1,
            StrategySpec::Aroon { period } => *period,
            StrategySpec::Keltner {
                ema_period,
                atr_period,
                ..
            } => (*ema_period).max(*atr_period),
            // HeikinAshi needs confirmation_bars for counting consecutive candles
            StrategySpec::HeikinAshi { confirmation_bars } => *confirmation_bars,
            // STARC needs max of SMA and ATR periods
            StrategySpec::Starc {
                sma_period,
                atr_period,
                ..
            } => (*sma_period).max(*atr_period),
            // Supertrend needs ATR period
            StrategySpec::Supertrend { atr_period, .. } => *atr_period,
            // DarvasBox needs 2x confirmation bars + 2 for box formation
            StrategySpec::DarvasBox { confirmation_bars } => *confirmation_bars * 2 + 2,
            // LarryWilliams needs at least 2 bars for prior range + ATR period
            StrategySpec::LarryWilliams { atr_period, .. } => (*atr_period).max(2),
            // OpeningRangeBreakout needs range_bars before signals are valid
            StrategySpec::OpeningRangeBreakout { range_bars, .. } => *range_bars,
            // ParabolicSar uses 5-bar warmup (per Wilder's algorithm)
            StrategySpec::ParabolicSar { .. } => 5,
            // Ensemble warmup is max of all child warmups
            StrategySpec::Ensemble { children, .. } => children
                .iter()
                .map(|c| c.warmup_period())
                .max()
                .unwrap_or(0),
            // Phase 5: Oscillator strategies
            StrategySpec::Rsi { period, .. } => *period + 1,
            StrategySpec::Macd {
                slow_period,
                signal_period,
                ..
            } => *slow_period + *signal_period,
            StrategySpec::Stochastic {
                k_period,
                k_smooth,
                d_period,
                ..
            } => *k_period + *k_smooth + *d_period,
            StrategySpec::WilliamsR { period, .. } => *period,
            StrategySpec::Cci { period, .. } => *period,
            StrategySpec::Roc { period } => *period,
            StrategySpec::RsiBollinger {
                rsi_period,
                bb_period,
                ..
            } => (*rsi_period + 1).max(*bb_period),
            StrategySpec::MacdAdx {
                slow_period,
                signal_period,
                adx_period,
                ..
            } => (*slow_period + *signal_period).max(2 * *adx_period),
            StrategySpec::OscillatorConfluence {
                rsi_period,
                stoch_k_period,
                stoch_k_smooth,
                stoch_d_period,
                ..
            } => (*rsi_period + 1).max(*stoch_k_period + *stoch_k_smooth + *stoch_d_period),
            StrategySpec::Ichimoku {
                senkou_b_period,
                kijun_period,
                ..
            } => *senkou_b_period + *kijun_period,
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

    /// Get the trading mode (default: LongOnly for backwards compatibility).
    fn trading_mode(&self) -> TradingMode {
        TradingMode::LongOnly
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

    /// Polars mode: add raw long signal columns to a LazyFrame.
    ///
    /// Assumes indicator columns have already been added.
    /// Adds `raw_entry` and `raw_exit` boolean columns for long trades.
    /// These are "raw" because they don't account for position state.
    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame;

    /// Polars mode: add raw short signal columns to a LazyFrame.
    ///
    /// Assumes indicator columns have already been added.
    /// Adds `raw_entry_short` and `raw_exit_short` boolean columns for short trades.
    /// Default implementation adds false columns (no short signals).
    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Default: no short signals
        lf.with_columns([
            lit(false).alias("raw_entry_short"),
            lit(false).alias("raw_exit_short"),
        ])
    }

    /// Polars mode: complete pipeline from OHLCV to signals.
    ///
    /// Adds indicators, long signals, and short signals (if enabled) in one step.
    fn add_strategy_columns(&self, lf: LazyFrame) -> LazyFrame {
        let lf = self.add_indicators_to_lf(lf);
        let lf = self.add_signals_to_lf(lf);

        // Add short signals if trading mode supports shorts
        match self.trading_mode() {
            TradingMode::ShortOnly | TradingMode::LongShort => self.add_short_signals_to_lf(lf),
            TradingMode::LongOnly => lf,
        }
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
    trading_mode: TradingMode,
}

impl DonchianBreakoutV2 {
    /// Create a new Donchian breakout strategy (long-only by default).
    pub fn new(entry_lookback: usize, exit_lookback: usize) -> Self {
        Self {
            spec: StrategySpec::donchian(entry_lookback, exit_lookback),
            entry_lookback,
            exit_lookback,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Create a new Donchian breakout strategy with specified trading mode.
    pub fn with_mode(entry_lookback: usize, exit_lookback: usize, mode: TradingMode) -> Self {
        Self {
            spec: StrategySpec::donchian(entry_lookback, exit_lookback),
            entry_lookback,
            exit_lookback,
            trading_mode: mode,
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

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for DonchianBreakoutV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
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
                // Check long entry
                if matches!(
                    self.trading_mode,
                    TradingMode::LongOnly | TradingMode::LongShort
                ) {
                    let entry_channel = donchian_channel(bars, self.entry_lookback);
                    if let Some(ch) = entry_channel[current_idx] {
                        if current_close > ch.upper {
                            return Signal::EnterLong;
                        }
                    }
                }
                // Check short entry
                if matches!(
                    self.trading_mode,
                    TradingMode::ShortOnly | TradingMode::LongShort
                ) {
                    let entry_channel = donchian_channel(bars, self.entry_lookback);
                    if let Some(ch) = entry_channel[current_idx] {
                        if current_close < ch.lower {
                            return Signal::EnterShort;
                        }
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
            Position::Short => {
                let exit_channel = donchian_channel(bars, self.exit_lookback);
                if let Some(ch) = exit_channel[current_idx] {
                    if current_close > ch.upper {
                        return Signal::ExitShort;
                    }
                }
                Signal::Hold
            }
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Add all Donchian channel columns for both entry and exit
        let (entry_upper, entry_lower) = donchian_channel_exprs(self.entry_lookback);
        let (exit_upper, exit_lower) = donchian_channel_exprs(self.exit_lookback);

        lf.with_columns([
            entry_upper.alias("dc_entry_upper"),
            entry_lower.alias("dc_entry_lower"),
            exit_upper.alias("dc_exit_upper"),
            exit_lower.alias("dc_exit_lower"),
        ])
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Raw long entry: close > entry channel upper
        let raw_entry = col("close").gt(col("dc_entry_upper")).alias("raw_entry");

        // Raw long exit: close < exit channel lower
        let raw_exit = col("close").lt(col("dc_exit_lower")).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Raw short entry: close < entry channel lower (breakdown)
        let raw_entry_short = col("close")
            .lt(col("dc_entry_lower"))
            .alias("raw_entry_short");

        // Raw short exit: close > exit channel upper (breakout = cover)
        let raw_exit_short = col("close")
            .gt(col("dc_exit_upper"))
            .alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

/// MA Crossover strategy implementing StrategyV2.
#[derive(Debug, Clone)]
pub struct MACrossoverV2 {
    spec: StrategySpec,
    fast_period: usize,
    slow_period: usize,
    ma_type: MAType,
    trading_mode: TradingMode,
}

impl MACrossoverV2 {
    /// Create a new MA crossover strategy (long-only by default).
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
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Create with specified trading mode.
    pub fn with_mode(
        fast_period: usize,
        slow_period: usize,
        ma_type: MAType,
        mode: TradingMode,
    ) -> Self {
        assert!(
            fast_period < slow_period,
            "Fast period must be less than slow period"
        );
        Self {
            spec: StrategySpec::ma_crossover(fast_period, slow_period, ma_type),
            fast_period,
            slow_period,
            ma_type,
            trading_mode: mode,
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

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for MACrossoverV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
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
        let golden_cross = fast_above_slow && !prev_fast_above_slow;
        let death_cross = !fast_above_slow && prev_fast_above_slow;

        match current_position {
            Position::Flat => {
                // Check for long entry (golden cross)
                if matches!(
                    self.trading_mode,
                    TradingMode::LongOnly | TradingMode::LongShort
                ) && golden_cross
                {
                    return Signal::EnterLong;
                }
                // Check for short entry (death cross)
                if matches!(
                    self.trading_mode,
                    TradingMode::ShortOnly | TradingMode::LongShort
                ) && death_cross
                {
                    return Signal::EnterShort;
                }
                Signal::Hold
            }
            Position::Long => {
                if death_cross {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => {
                if golden_cross {
                    return Signal::ExitShort;
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

        // Golden cross: fast crosses above slow (long entry)
        let raw_entry = fast_above
            .clone()
            .and(prev_fast_above.clone().not())
            .alias("raw_entry");

        // Death cross: fast crosses below slow (long exit)
        let raw_exit = fast_above
            .clone()
            .not()
            .and(prev_fast_above.clone())
            .alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        let fast_above = col("ma_fast").gt(col("ma_slow"));
        let prev_fast_above = col("ma_fast")
            .shift(lit(1))
            .gt(col("ma_slow").shift(lit(1)));

        // Death cross: fast crosses below slow (short entry)
        let raw_entry_short = fast_above
            .clone()
            .not()
            .and(prev_fast_above.clone())
            .alias("raw_entry_short");

        // Golden cross: fast crosses above slow (short exit/cover)
        let raw_exit_short = fast_above
            .and(prev_fast_above.not())
            .alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

/// TSMOM strategy implementing StrategyV2.
#[derive(Debug, Clone)]
pub struct TsmomV2 {
    spec: StrategySpec,
    lookback: usize,
    trading_mode: TradingMode,
}

impl TsmomV2 {
    /// Create a new TSMOM strategy (long-only by default).
    pub fn new(lookback: usize) -> Self {
        assert!(lookback > 0, "Lookback must be at least 1");
        Self {
            spec: StrategySpec::tsmom(lookback),
            lookback,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Create with specified trading mode.
    pub fn with_mode(lookback: usize, mode: TradingMode) -> Self {
        assert!(lookback > 0, "Lookback must be at least 1");
        Self {
            spec: StrategySpec::tsmom(lookback),
            lookback,
            trading_mode: mode,
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

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for TsmomV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
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
        let positive_momentum = current_close > lookback_close;
        let negative_momentum = current_close < lookback_close;

        match current_position {
            Position::Flat => {
                // Long entry on positive momentum
                if matches!(
                    self.trading_mode,
                    TradingMode::LongOnly | TradingMode::LongShort
                ) && positive_momentum
                {
                    return Signal::EnterLong;
                }
                // Short entry on negative momentum
                if matches!(
                    self.trading_mode,
                    TradingMode::ShortOnly | TradingMode::LongShort
                ) && negative_momentum
                {
                    return Signal::EnterShort;
                }
                Signal::Hold
            }
            Position::Long => {
                if negative_momentum {
                    Signal::ExitLong
                } else {
                    Signal::Hold
                }
            }
            Position::Short => {
                if positive_momentum {
                    Signal::ExitShort
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
        // Long entry: positive momentum
        let raw_entry = col("momentum").gt(lit(0.0)).alias("raw_entry");

        // Long exit: negative momentum
        let raw_exit = col("momentum").lt(lit(0.0)).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Short entry: negative momentum
        let raw_entry_short = col("momentum").lt(lit(0.0)).alias("raw_entry_short");

        // Short exit: positive momentum
        let raw_exit_short = col("momentum").gt(lit(0.0)).alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

/// 52-Week High strategy implementing StrategyV2.
///
/// Enters when price is within a percentage of the period high,
/// exits when price falls below a lower percentage threshold.
#[derive(Debug, Clone)]
pub struct FiftyTwoWeekHighV2 {
    spec: StrategySpec,
    period: usize,
    entry_pct: f64,
    exit_pct: f64,
    trading_mode: TradingMode,
}

impl FiftyTwoWeekHighV2 {
    /// Create a new 52-Week High strategy.
    ///
    /// # Arguments
    /// * `period` - Lookback period for computing rolling high
    /// * `entry_pct` - Entry threshold as percentage of period high (e.g., 0.95 = 95%)
    /// * `exit_pct` - Exit threshold as percentage of period high (e.g., 0.90 = 90%)
    pub fn new(period: usize, entry_pct: f64, exit_pct: f64) -> Self {
        assert!(period > 0, "Period must be at least 1");
        assert!(
            entry_pct > 0.0 && entry_pct <= 1.0,
            "Entry percentage must be between 0 and 1"
        );
        assert!(
            exit_pct > 0.0 && exit_pct <= 1.0,
            "Exit percentage must be between 0 and 1"
        );
        assert!(
            exit_pct < entry_pct,
            "Exit percentage must be less than entry percentage"
        );

        Self {
            spec: StrategySpec::fifty_two_week_high(period, entry_pct, exit_pct),
            period,
            entry_pct,
            exit_pct,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard 52-week high (252 trading days), entry at 95%, exit at 90%.
    pub fn annual() -> Self {
        Self::new(252, 0.95, 0.90)
    }

    /// 6-month high (126 trading days), entry at 95%, exit at 90%.
    pub fn semi_annual() -> Self {
        Self::new(126, 0.95, 0.90)
    }

    /// Get the period.
    pub fn period(&self) -> usize {
        self.period
    }

    /// Get the entry percentage.
    pub fn entry_pct(&self) -> f64 {
        self.entry_pct
    }

    /// Get the exit percentage.
    pub fn exit_pct(&self) -> f64 {
        self.exit_pct
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for FiftyTwoWeekHighV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        use crate::indicators::rolling_max_close;

        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let rolling_max = rolling_max_close(bars, self.period);
        let period_high = match rolling_max[current_idx] {
            Some(h) => h,
            None => return Signal::Hold,
        };

        let current_close = bars[current_idx].close;
        let proximity = current_close / period_high;

        match current_position {
            Position::Flat => {
                // Entry: close >= entry_pct * period_high
                if proximity >= self.entry_pct {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: close < exit_pct * period_high
                if proximity < self.exit_pct {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold, // Long-only strategy
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Add rolling max of close over the period
        let period_high = col("close")
            .rolling_max(RollingOptionsFixedWindow {
                window_size: self.period,
                min_periods: self.period,
                weights: None,
                center: false,
                fn_params: None,
            })
            .alias("period_high");

        // Add proximity (close / period_high)
        let proximity = (col("close") / col("period_high")).alias("proximity");

        // Need separate with_columns calls since proximity depends on period_high
        lf.with_column(period_high).with_column(proximity)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Long entry: proximity >= entry_pct
        let raw_entry = col("proximity")
            .gt_eq(lit(self.entry_pct))
            .alias("raw_entry");

        // Long exit: proximity < exit_pct
        let raw_exit = col("proximity").lt(lit(self.exit_pct)).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }
}

/// DMI/ADX strategy implementing StrategyV2.
///
/// Uses Welles Wilder's Directional Movement Index with ADX filter.
/// Entry: +DI > -DI AND ADX > threshold (bullish direction with trend strength)
/// Exit: -DI >= +DI (bearish direction)
#[derive(Debug, Clone)]
pub struct DmiAdxV2 {
    spec: StrategySpec,
    di_period: usize,
    adx_period: usize,
    adx_threshold: f64,
    trading_mode: TradingMode,
}

impl DmiAdxV2 {
    /// Create a new DMI/ADX strategy.
    ///
    /// # Arguments
    /// * `di_period` - Period for +DI/-DI calculation (Wilder smoothing)
    /// * `adx_period` - Period for ADX smoothing
    /// * `adx_threshold` - Minimum ADX value for trend strength (typically 20-25)
    pub fn new(di_period: usize, adx_period: usize, adx_threshold: f64) -> Self {
        assert!(di_period > 0, "DI period must be at least 1");
        assert!(adx_period > 0, "ADX period must be at least 1");
        assert!(adx_threshold > 0.0, "ADX threshold must be positive");

        Self {
            spec: StrategySpec::dmi_adx(di_period, adx_period, adx_threshold),
            di_period,
            adx_period,
            adx_threshold,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard configuration: 14/14/25.
    pub fn standard() -> Self {
        Self::new(14, 14, 25.0)
    }

    /// Smoother configuration: 20/20/20.
    pub fn smoother() -> Self {
        Self::new(20, 20, 20.0)
    }

    /// Get the DI period.
    pub fn di_period(&self) -> usize {
        self.di_period
    }

    /// Get the ADX period.
    pub fn adx_period(&self) -> usize {
        self.adx_period
    }

    /// Get the ADX threshold.
    pub fn adx_threshold(&self) -> f64 {
        self.adx_threshold
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for DmiAdxV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        // Compute DMI indicators
        let dmi_values = dmi(bars, self.di_period);
        let current_dmi = match dmi_values[current_idx] {
            Some(ref d) => d,
            None => return Signal::Hold,
        };

        let plus_above_minus = current_dmi.plus_di > current_dmi.minus_di;
        let strong_trend = current_dmi.adx > self.adx_threshold;

        match current_position {
            Position::Flat => {
                // Entry: +DI > -DI AND ADX > threshold
                if plus_above_minus && strong_trend {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: -DI >= +DI (bearish direction)
                if !plus_above_minus {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold, // Long-only strategy
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // apply_dmi_exprs adds: true_range, atr_wilder, plus_dm, minus_dm,
        // plus_dm_smooth, minus_dm_smooth, plus_di, minus_di, dx, adx
        apply_dmi_exprs(lf, self.di_period)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Long entry: +DI > -DI AND ADX > threshold
        let raw_entry = col("plus_di")
            .gt(col("minus_di"))
            .and(col("adx").gt(lit(self.adx_threshold)))
            .alias("raw_entry");

        // Long exit: -DI >= +DI (bearish direction)
        let raw_exit = col("minus_di").gt_eq(col("plus_di")).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Short entry: -DI > +DI AND ADX > threshold
        let raw_entry_short = col("minus_di")
            .gt(col("plus_di"))
            .and(col("adx").gt(lit(self.adx_threshold)))
            .alias("raw_entry_short");

        // Short exit: +DI >= -DI (bullish direction)
        let raw_exit_short = col("plus_di")
            .gt_eq(col("minus_di"))
            .alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

/// Bollinger Squeeze strategy implementing StrategyV2.
///
/// Detects volatility compression (squeeze) and trades breakouts.
/// Entry: (In squeeze OR was in squeeze) AND close > upper band
/// Exit: Close < middle band (SMA)
#[derive(Debug, Clone)]
pub struct BollingerSqueezeV2 {
    spec: StrategySpec,
    period: usize,
    std_mult: f64,
    squeeze_threshold: f64,
    trading_mode: TradingMode,
}

impl BollingerSqueezeV2 {
    /// Create a new Bollinger Squeeze strategy.
    ///
    /// # Arguments
    /// * `period` - Period for SMA and standard deviation
    /// * `std_mult` - Standard deviation multiplier for bands (typically 2.0)
    /// * `squeeze_threshold` - Bandwidth threshold for squeeze detection (typically 0.04)
    pub fn new(period: usize, std_mult: f64, squeeze_threshold: f64) -> Self {
        assert!(period > 0, "Period must be at least 1");
        assert!(std_mult > 0.0, "Std multiplier must be positive");
        assert!(
            squeeze_threshold > 0.0,
            "Squeeze threshold must be positive"
        );

        Self {
            spec: StrategySpec::bollinger_squeeze(period, std_mult, squeeze_threshold),
            period,
            std_mult,
            squeeze_threshold,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Create standard configuration (period 20, mult 2.0, threshold 0.04).
    pub fn standard() -> Self {
        Self::new(20, 2.0, 0.04)
    }

    /// Create tight squeeze configuration (period 20, mult 2.5, threshold 0.03).
    pub fn tight_squeeze() -> Self {
        Self::new(20, 2.5, 0.03)
    }

    /// Set trading mode.
    pub fn with_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }

    /// Check if bandwidth indicates a squeeze.
    fn is_in_squeeze(&self, bandwidth: f64) -> bool {
        bandwidth < self.squeeze_threshold
    }
}

impl StrategyV2 for BollingerSqueezeV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        use crate::indicators::bollinger_bands;

        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;

        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let bb_values = bollinger_bands(bars, self.period, self.std_mult);

        let current_bb = match bb_values[current_idx] {
            Some(ref bb) => bb,
            None => return Signal::Hold,
        };

        let current_close = bars[current_idx].close;
        let in_squeeze = self.is_in_squeeze(current_bb.bandwidth);

        // Check previous bar squeeze state
        let prev_in_squeeze = if current_idx > 0 {
            bb_values[current_idx - 1]
                .as_ref()
                .map(|bb| self.is_in_squeeze(bb.bandwidth))
                .unwrap_or(false)
        } else {
            false
        };

        match current_position {
            Position::Flat => {
                // Entry: In squeeze (or was in squeeze) AND close breaks above upper band
                if (in_squeeze || prev_in_squeeze) && current_close > current_bb.upper {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: Close falls below middle band (SMA)
                if current_close < current_bb.middle {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold,
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        use crate::indicators_polars::apply_bollinger_exprs;
        apply_bollinger_exprs(lf, self.period, self.std_mult)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Compute squeeze condition: bandwidth < threshold
        let in_squeeze = col("bb_bandwidth")
            .lt(lit(self.squeeze_threshold))
            .alias("in_squeeze");

        // Previous bar in squeeze
        let prev_in_squeeze = col("in_squeeze").shift(lit(1)).alias("prev_in_squeeze");

        // Long entry: (in_squeeze OR prev_in_squeeze) AND close > upper band
        let raw_entry = (col("in_squeeze").or(col("prev_in_squeeze")))
            .and(col("close").gt(col("bb_upper")))
            .alias("raw_entry");

        // Long exit: close < middle band
        let raw_exit = col("close").lt(col("bb_middle")).alias("raw_exit");

        // Need to chain with_columns for dependencies
        lf.with_column(in_squeeze)
            .with_column(prev_in_squeeze)
            .with_columns([raw_entry, raw_exit])
    }
}

/// Aroon strategy implementing StrategyV2.
///
/// Uses Tushar Chande's Aroon oscillator for trend following.
/// Entry: Aroon-Up crosses above Aroon-Down (bullish trend emerging)
/// Exit: Aroon-Up crosses below Aroon-Down (bearish trend emerging)
#[derive(Debug, Clone)]
pub struct AroonV2 {
    spec: StrategySpec,
    period: usize,
    trading_mode: TradingMode,
}

impl AroonV2 {
    /// Create a new Aroon strategy.
    ///
    /// # Arguments
    /// * `period` - Lookback period for Aroon calculation (typically 14-25)
    pub fn new(period: usize) -> Self {
        assert!(period > 0, "Period must be at least 1");

        Self {
            spec: StrategySpec::aroon(period),
            period,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard configuration: 25-period.
    pub fn standard() -> Self {
        Self::new(25)
    }

    /// Short-term configuration: 14-period.
    pub fn short_term() -> Self {
        Self::new(14)
    }

    /// Get the period.
    pub fn period(&self) -> usize {
        self.period
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for AroonV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        // Compute Aroon indicators
        let aroon_values = aroon(bars, self.period);

        let current_aroon = match aroon_values[current_idx] {
            Some(ref a) => a,
            None => return Signal::Hold,
        };

        // For crossover detection, we need previous values
        if current_idx == 0 {
            return Signal::Hold;
        }

        let prev_aroon = match aroon_values[current_idx - 1] {
            Some(ref a) => a,
            None => return Signal::Hold,
        };

        let up_above_down = current_aroon.aroon_up > current_aroon.aroon_down;
        let prev_up_above_down = prev_aroon.aroon_up > prev_aroon.aroon_down;

        // Golden cross: Aroon-Up crosses above Aroon-Down
        let bullish_cross = up_above_down && !prev_up_above_down;
        // Death cross: Aroon-Up crosses below Aroon-Down
        let bearish_cross = !up_above_down && prev_up_above_down;

        match current_position {
            Position::Flat => {
                // Entry: Aroon-Up crosses above Aroon-Down
                if bullish_cross {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: Aroon-Up crosses below Aroon-Down
                if bearish_cross {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold, // Long-only strategy
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // apply_aroon_exprs adds: rolling_max_high, rolling_min_low,
        // aroon_bars_since_high, aroon_bars_since_low, aroon_up, aroon_down, aroon_oscillator
        apply_aroon_exprs(lf, self.period)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // For crossover detection:
        // - aroon_up > aroon_down (current)
        // - aroon_up <= aroon_down (previous)
        let up_above = col("aroon_up").gt(col("aroon_down"));
        let prev_up_above = col("aroon_up")
            .shift(lit(1))
            .gt(col("aroon_down").shift(lit(1)));

        // Bullish cross: up crosses above down (long entry)
        let raw_entry = up_above
            .clone()
            .and(prev_up_above.clone().not())
            .alias("raw_entry");

        // Bearish cross: up crosses below down (long exit)
        let raw_exit = up_above
            .clone()
            .not()
            .and(prev_up_above.clone())
            .alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        let up_above = col("aroon_up").gt(col("aroon_down"));
        let prev_up_above = col("aroon_up")
            .shift(lit(1))
            .gt(col("aroon_down").shift(lit(1)));

        // Bearish cross: up crosses below down (short entry)
        let raw_entry_short = up_above
            .clone()
            .not()
            .and(prev_up_above.clone())
            .alias("raw_entry_short");

        // Bullish cross: up crosses above down (short exit/cover)
        let raw_exit_short = up_above.and(prev_up_above.not()).alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

/// Keltner Channel strategy implementing StrategyV2.
///
/// Uses EMA center with ATR-based bands.
/// Entry: Close > upper band
/// Exit: Close < lower band OR close < center (EMA)
#[derive(Debug, Clone)]
pub struct KeltnerV2 {
    spec: StrategySpec,
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
    trading_mode: TradingMode,
}

impl KeltnerV2 {
    /// Create a new Keltner Channel strategy.
    ///
    /// # Arguments
    /// * `ema_period` - Period for EMA (center line)
    /// * `atr_period` - Period for ATR calculation
    /// * `multiplier` - ATR multiplier for bands (typically 1.5-2.5)
    pub fn new(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        assert!(ema_period > 0, "EMA period must be at least 1");
        assert!(atr_period > 0, "ATR period must be at least 1");
        assert!(multiplier > 0.0, "Multiplier must be positive");

        Self {
            spec: StrategySpec::keltner(ema_period, atr_period, multiplier),
            ema_period,
            atr_period,
            multiplier,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard configuration: 20/10/2.0.
    pub fn standard() -> Self {
        Self::new(20, 10, 2.0)
    }

    /// Tight bands configuration: 20/10/1.5.
    pub fn tight() -> Self {
        Self::new(20, 10, 1.5)
    }

    /// Wide bands configuration: 20/10/2.5.
    pub fn wide() -> Self {
        Self::new(20, 10, 2.5)
    }

    /// Get the EMA period.
    pub fn ema_period(&self) -> usize {
        self.ema_period
    }

    /// Get the ATR period.
    pub fn atr_period(&self) -> usize {
        self.atr_period
    }

    /// Get the multiplier.
    pub fn multiplier(&self) -> f64 {
        self.multiplier
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for KeltnerV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let keltner = keltner_channel(bars, self.ema_period, self.atr_period, self.multiplier);
        let current_close = bars[current_idx].close;

        let kc = match keltner[current_idx] {
            Some(k) => k,
            None => return Signal::Hold,
        };

        match current_position {
            Position::Flat => {
                // Entry: close > upper band
                if current_close > kc.upper {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: close < lower band OR close < center (EMA)
                if current_close < kc.lower || current_close < kc.center {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold, // Long-only strategy
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // apply_keltner_exprs adds: true_range, kc_center, kc_upper, kc_lower
        apply_keltner_exprs(lf, self.ema_period, self.atr_period, self.multiplier)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Long entry: close > upper band
        let raw_entry = col("close").gt(col("kc_upper")).alias("raw_entry");

        // Long exit: close < lower band OR close < center
        let raw_exit = col("close")
            .lt(col("kc_lower"))
            .or(col("close").lt(col("kc_center")))
            .alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Short entry: close < lower band (breakdown)
        let raw_entry_short = col("close").lt(col("kc_lower")).alias("raw_entry_short");

        // Short exit: close > upper band OR close > center
        let raw_exit_short = col("close")
            .gt(col("kc_upper"))
            .or(col("close").gt(col("kc_center")))
            .alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

// =============================================================================
// HeikinAshiV2 - Heikin-Ashi Regime Strategy
// =============================================================================

/// Heikin-Ashi Regime strategy (V2 Polars-native implementation).
///
/// Entry: First bullish HA candle after N consecutive bearish candles (regime flip)
/// Exit: First bearish HA candle (immediate exit)
///
/// Heikin-Ashi candles smooth price action to make trend identification easier.
#[derive(Debug, Clone)]
pub struct HeikinAshiV2 {
    spec: StrategySpec,
    confirmation_bars: usize,
    trading_mode: TradingMode,
}

impl HeikinAshiV2 {
    /// Create a new Heikin-Ashi strategy.
    pub fn new(confirmation_bars: usize) -> Self {
        assert!(
            confirmation_bars > 0,
            "Confirmation bars must be at least 1"
        );
        Self {
            spec: StrategySpec::heikin_ashi(confirmation_bars),
            confirmation_bars,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard configuration: 2-bar confirmation.
    pub fn standard() -> Self {
        Self::new(2)
    }

    /// Get the confirmation bars.
    pub fn confirmation_bars(&self) -> usize {
        self.confirmation_bars
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for HeikinAshiV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let ha_bars = heikin_ashi(bars);
        let current_ha = &ha_bars[current_idx];

        match current_position {
            Position::Flat => {
                if current_ha.is_bullish() {
                    let mut bearish_count = 0;
                    for i in (0..current_idx).rev() {
                        if ha_bars[i].is_bearish() {
                            bearish_count += 1;
                        } else {
                            break;
                        }
                    }
                    if bearish_count >= self.confirmation_bars {
                        return Signal::EnterLong;
                    }
                }
                Signal::Hold
            }
            Position::Long => {
                if current_ha.is_bearish() {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold,
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        apply_heikin_ashi_exprs(lf)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        let mut bearish_streak_expr: Option<Expr> = None;
        for offset in 1..=self.confirmation_bars {
            let is_bearish_at_offset = col("ha_bearish").shift(lit(offset as i64));
            bearish_streak_expr = Some(match bearish_streak_expr {
                None => is_bearish_at_offset,
                Some(prev) => prev.and(is_bearish_at_offset),
            });
        }

        let raw_entry_long = col("ha_bullish")
            .and(bearish_streak_expr.unwrap_or(lit(true)))
            .alias("raw_entry_long");
        let raw_exit_long = col("ha_bearish").alias("raw_exit_long");

        lf.with_columns([raw_entry_long, raw_exit_long])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        let mut bullish_streak_expr: Option<Expr> = None;
        for offset in 1..=self.confirmation_bars {
            let is_bullish_at_offset = col("ha_bullish").shift(lit(offset as i64));
            bullish_streak_expr = Some(match bullish_streak_expr {
                None => is_bullish_at_offset,
                Some(prev) => prev.and(is_bullish_at_offset),
            });
        }

        let raw_entry_short = col("ha_bearish")
            .and(bullish_streak_expr.unwrap_or(lit(true)))
            .alias("raw_entry_short");
        let raw_exit_short = col("ha_bullish").alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

// =============================================================================
// StarcV2 - STARC Bands Strategy
// =============================================================================

/// STARC Bands strategy (V2 Polars-native implementation).
///
/// Entry: Close > upper band (SMA + mult * ATR)
/// Exit: Close < lower band (SMA - mult * ATR)
///
/// STARC (Stoller Average Range Channel) is similar to Keltner but uses SMA
/// instead of EMA for the center line.
#[derive(Debug, Clone)]
pub struct StarcV2 {
    spec: StrategySpec,
    sma_period: usize,
    atr_period: usize,
    multiplier: f64,
    trading_mode: TradingMode,
}

impl StarcV2 {
    /// Create a new STARC Bands strategy.
    ///
    /// # Arguments
    /// * `sma_period` - Period for SMA (center line)
    /// * `atr_period` - Period for ATR calculation
    /// * `multiplier` - ATR multiplier for bands (typically 1.5-2.5)
    pub fn new(sma_period: usize, atr_period: usize, multiplier: f64) -> Self {
        assert!(sma_period > 0, "SMA period must be at least 1");
        assert!(atr_period > 0, "ATR period must be at least 1");
        assert!(multiplier > 0.0, "Multiplier must be positive");

        Self {
            spec: StrategySpec::starc(sma_period, atr_period, multiplier),
            sma_period,
            atr_period,
            multiplier,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard configuration: 20/15/2.0.
    pub fn standard() -> Self {
        Self::new(20, 15, 2.0)
    }

    /// Tight bands configuration: 20/15/1.5.
    pub fn tight() -> Self {
        Self::new(20, 15, 1.5)
    }

    /// Wide bands configuration: 20/15/2.5.
    pub fn wide() -> Self {
        Self::new(20, 15, 2.5)
    }

    /// Get the SMA period.
    pub fn sma_period(&self) -> usize {
        self.sma_period
    }

    /// Get the ATR period.
    pub fn atr_period(&self) -> usize {
        self.atr_period
    }

    /// Get the multiplier.
    pub fn multiplier(&self) -> f64 {
        self.multiplier
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for StarcV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let starc = starc_bands(bars, self.sma_period, self.atr_period, self.multiplier);
        let current_close = bars[current_idx].close;

        let bands = match starc[current_idx] {
            Some(b) => b,
            None => return Signal::Hold,
        };

        match current_position {
            Position::Flat => {
                // Entry: close > upper band
                if current_close > bands.upper {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: close < lower band
                if current_close < bands.lower {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold, // Long-only strategy
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // apply_starc_exprs adds: true_range, starc_center, starc_upper, starc_lower
        apply_starc_exprs(lf, self.sma_period, self.atr_period, self.multiplier)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Long entry: close > upper band
        let raw_entry = col("close").gt(col("starc_upper")).alias("raw_entry");

        // Long exit: close < lower band
        let raw_exit = col("close").lt(col("starc_lower")).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Short entry: close < lower band (breakdown)
        let raw_entry_short = col("close").lt(col("starc_lower")).alias("raw_entry_short");

        // Short exit: close > upper band
        let raw_exit_short = col("close").gt(col("starc_upper")).alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

// =============================================================================
// SupertrendV2 - Supertrend Strategy
// =============================================================================

/// Supertrend strategy (V2 Polars-native implementation).
///
/// Sequential mode uses the exact stateful Supertrend indicator.
/// Polars mode uses a simplified approximation (see `apply_supertrend_exprs` docs).
#[derive(Debug, Clone)]
pub struct SupertrendV2 {
    spec: StrategySpec,
    atr_period: usize,
    multiplier: f64,
    trading_mode: TradingMode,
}

impl SupertrendV2 {
    pub fn new(atr_period: usize, multiplier: f64) -> Self {
        assert!(atr_period > 0, "ATR period must be at least 1");
        assert!(multiplier > 0.0, "Multiplier must be positive");
        Self {
            spec: StrategySpec::supertrend(atr_period, multiplier),
            atr_period,
            multiplier,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for SupertrendV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let st_values = supertrend(bars, self.atr_period, self.multiplier);
        let current = match st_values[current_idx] {
            Some(v) => v,
            None => return Signal::Hold,
        };

        if current_idx == 0 {
            return Signal::Hold;
        }
        let prev = match st_values[current_idx - 1] {
            Some(v) => v,
            None => return Signal::Hold,
        };

        match current_position {
            Position::Flat => {
                if current.is_uptrend && !prev.is_uptrend {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                if !current.is_uptrend && prev.is_uptrend {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold, // Keep long-only behavior for now
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        apply_supertrend_exprs(lf, self.atr_period, self.multiplier)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Approx trend flip detection on st_is_uptrend.
        let up = col("st_is_uptrend");
        let prev_up = col("st_is_uptrend").shift(lit(1)).fill_null(lit(false));

        let raw_entry = up.clone().and(prev_up.clone().not()).alias("raw_entry");
        let raw_exit = up.not().and(prev_up).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }
}

// =============================================================================
// DarvasBoxV2 - Darvas Box Breakout Strategy
// =============================================================================

/// Darvas Box strategy (V2 Polars-native implementation).
///
/// Sequential mode uses the exact stateful Darvas box algorithm.
/// Polars mode uses a simplified approximation based on rolling highs/lows.
#[derive(Debug, Clone)]
pub struct DarvasBoxV2 {
    spec: StrategySpec,
    confirmation_bars: usize,
    trading_mode: TradingMode,
}

impl DarvasBoxV2 {
    pub fn new(confirmation_bars: usize) -> Self {
        assert!(
            confirmation_bars > 0,
            "Confirmation bars must be at least 1"
        );
        Self {
            spec: StrategySpec::darvas_box(confirmation_bars),
            confirmation_bars,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard Darvas with 3-bar confirmation.
    pub fn standard() -> Self {
        Self::new(3)
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for DarvasBoxV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let boxes = darvas_boxes(bars, self.confirmation_bars);
        let current_close = bars[current_idx].close;

        // Get current and previous box states
        let current_box = boxes[current_idx];
        let prev_box = if current_idx > 0 {
            boxes[current_idx - 1]
        } else {
            None
        };

        match current_position {
            Position::Flat => {
                // Entry: close breaks above a confirmed box top
                if let Some(ref bx) = current_box {
                    if bx.is_complete() && current_close > bx.top {
                        return Signal::EnterLong;
                    }
                }
                // Also check if we broke above previous box
                if let Some(ref bx) = prev_box {
                    if bx.is_complete() && current_close > bx.top {
                        return Signal::EnterLong;
                    }
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: close breaks below box bottom
                if let Some(ref bx) = current_box {
                    if current_close < bx.bottom {
                        return Signal::ExitLong;
                    }
                }
                if let Some(ref bx) = prev_box {
                    if current_close < bx.bottom {
                        return Signal::ExitLong;
                    }
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold,
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Darvas box approximation using rolling windows:
        // - Box top: rolling max high (potential top boundary)
        // - Box bottom: rolling min low (potential bottom boundary)
        // We use 2x confirmation_bars as the window for smoothing
        let window = self.confirmation_bars * 2;

        let box_top = col("high")
            .rolling_max(RollingOptionsFixedWindow {
                window_size: window,
                min_periods: window,
                weights: None,
                center: false,
                fn_params: None,
            })
            .alias("darvas_top");

        let box_bottom = col("low")
            .rolling_min(RollingOptionsFixedWindow {
                window_size: window,
                min_periods: window,
                weights: None,
                center: false,
                fn_params: None,
            })
            .alias("darvas_bottom");

        lf.with_columns([box_top, box_bottom])
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Entry: close breaks above darvas_top (with some hysteresis)
        // Exit: close breaks below darvas_bottom
        let prev_top = col("darvas_top").shift(lit(1));
        let prev_bottom = col("darvas_bottom").shift(lit(1));

        // Entry: close > previous top (breakout above consolidation)
        let raw_entry = col("close")
            .gt(prev_top)
            .fill_null(lit(false))
            .alias("raw_entry");

        // Exit: close < previous bottom (breakdown)
        let raw_exit = col("close")
            .lt(prev_bottom)
            .fill_null(lit(false))
            .alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }
}

// =============================================================================
// LarryWilliamsV2 - Larry Williams Volatility Breakout Strategy
// =============================================================================

/// Larry Williams Volatility Breakout strategy (V2 Polars-native implementation).
///
/// Sequential mode uses range_breakout_levels and ATR for trailing stops.
/// Polars mode computes the same logic vectorized.
#[derive(Debug, Clone)]
pub struct LarryWilliamsV2 {
    spec: StrategySpec,
    range_mult: f64,
    atr_stop_mult: f64,
    atr_period: usize,
    trading_mode: TradingMode,
}

impl LarryWilliamsV2 {
    pub fn new(range_mult: f64, atr_stop_mult: f64, atr_period: usize) -> Self {
        assert!(range_mult > 0.0, "Range multiplier must be positive");
        assert!(atr_stop_mult > 0.0, "ATR stop multiplier must be positive");
        assert!(atr_period > 0, "ATR period must be at least 1");

        Self {
            spec: StrategySpec::larry_williams(range_mult, atr_stop_mult, atr_period),
            range_mult,
            atr_stop_mult,
            atr_period,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard configuration: range_mult=0.5, atr_stop=2.0, atr_period=14.
    pub fn standard() -> Self {
        Self::new(0.5, 2.0, 14)
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for LarryWilliamsV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let current_bar = &bars[current_idx];
        let breakout_levels = range_breakout_levels(bars, self.range_mult);
        let (upper_breakout, _lower_breakout) = breakout_levels[current_idx];

        let atr_values = atr(bars, self.atr_period);
        let current_atr = atr_values[current_idx];

        match current_position {
            Position::Flat => {
                // Entry: close > upper_breakout (open + k * prior_range)
                if current_bar.close > upper_breakout {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: trailing stop using ATR
                if let Some(current_atr_val) = current_atr {
                    if current_idx > 0 {
                        // Look at last 5 bars for recent high
                        let recent_high = bars[..=current_idx]
                            .iter()
                            .rev()
                            .take(5)
                            .map(|b| b.high)
                            .fold(f64::NEG_INFINITY, f64::max);

                        let stop_level = recent_high - self.atr_stop_mult * current_atr_val;
                        if current_bar.close < stop_level {
                            return Signal::ExitLong;
                        }
                    }
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold,
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Prior day range: high - low of previous bar
        let prior_range = (col("high") - col("low"))
            .shift(lit(1))
            .alias("prior_range");

        // Upper breakout level: open + range_mult * prior_range
        let upper_breakout =
            (col("open") + lit(self.range_mult) * col("prior_range")).alias("upper_breakout");

        // ATR for trailing stop - compute true range using when/then/otherwise
        let tr = {
            let hl = col("high") - col("low");
            let prev_close = col("close").shift(lit(1));
            let hc = (col("high") - prev_close.clone()).abs();
            let lc = (col("low") - prev_close).abs();
            // max(hl, hc, lc) using when/then/otherwise
            when(hc.clone().is_null()).then(hl.clone()).otherwise(
                when(
                    hl.clone()
                        .gt_eq(hc.clone())
                        .and(hl.clone().gt_eq(lc.clone())),
                )
                .then(hl.clone())
                .otherwise(when(hc.clone().gt_eq(lc.clone())).then(hc).otherwise(lc)),
            )
        };

        let atr_col = tr
            .rolling_mean(RollingOptionsFixedWindow {
                window_size: self.atr_period,
                min_periods: self.atr_period,
                weights: None,
                center: false,
                fn_params: None,
            })
            .alias("lw_atr");

        // Recent high (5-bar rolling max) for trailing stop
        let recent_high = col("high")
            .rolling_max(RollingOptionsFixedWindow {
                window_size: 5,
                min_periods: 1,
                weights: None,
                center: false,
                fn_params: None,
            })
            .alias("recent_high_5");

        // Stop level: recent_high - atr_stop_mult * ATR
        let stop_level =
            (col("recent_high_5") - lit(self.atr_stop_mult) * col("lw_atr")).alias("lw_stop_level");

        lf.with_columns([prior_range])
            .with_columns([upper_breakout, atr_col, recent_high])
            .with_columns([stop_level])
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Entry: close > upper_breakout
        let raw_entry = col("close")
            .gt(col("upper_breakout"))
            .fill_null(lit(false))
            .alias("raw_entry");

        // Exit: close < stop_level
        let raw_exit = col("close")
            .lt(col("lw_stop_level"))
            .fill_null(lit(false))
            .alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }
}

// =============================================================================
// OpeningRangeBreakoutV2 - Opening Range Breakout Strategy
// =============================================================================

/// Opening Range Breakout strategy (V2 Polars-native implementation).
///
/// Entry: Close breaks above range high (after range is complete)
/// Exit: Close breaks below range low
///
/// The opening range is defined by the first N bars of each period (weekly/monthly)
/// or as a rolling lookback window.
#[derive(Debug, Clone)]
pub struct OpeningRangeBreakoutV2 {
    spec: StrategySpec,
    range_bars: usize,
    period: OpeningPeriod,
    trading_mode: TradingMode,
}

impl OpeningRangeBreakoutV2 {
    /// Create a new Opening Range Breakout strategy.
    ///
    /// # Arguments
    /// * `range_bars` - Number of bars that define the opening range
    /// * `period` - How to determine when a new period starts (Weekly, Monthly, Rolling)
    pub fn new(range_bars: usize, period: OpeningPeriod) -> Self {
        assert!(range_bars > 0, "Range bars must be at least 1");

        Self {
            spec: StrategySpec::opening_range_breakout(range_bars, period),
            range_bars,
            period,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard weekly range with 5 bars.
    pub fn weekly_5() -> Self {
        Self::new(5, OpeningPeriod::Weekly)
    }

    /// Standard monthly range with 10 bars.
    pub fn monthly_10() -> Self {
        Self::new(10, OpeningPeriod::Monthly)
    }

    /// Rolling range with 3 bars.
    pub fn rolling_3() -> Self {
        Self::new(3, OpeningPeriod::Rolling)
    }

    /// Get the range bars.
    pub fn range_bars(&self) -> usize {
        self.range_bars
    }

    /// Get the period.
    pub fn period(&self) -> OpeningPeriod {
        self.period
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for OpeningRangeBreakoutV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;

        // During warmup, no signals
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let or_values = opening_range(bars, self.range_bars, self.period);

        let current_or = match or_values[current_idx] {
            Some(ref r) => r,
            None => return Signal::Hold,
        };

        // Only trade after range is complete
        if !current_or.is_range_complete {
            return Signal::Hold;
        }

        let current_close = bars[current_idx].close;

        match current_position {
            Position::Flat => {
                // Entry: close breaks above range high
                if current_or.is_breakout_high(current_close) {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: close breaks below range low
                if current_or.is_breakout_low(current_close) {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold,
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        apply_opening_range_exprs(lf, self.range_bars, self.period)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Entry: close > range_high AND range is complete
        let raw_entry_long = col("orb_is_complete")
            .and(col("close").gt(col("orb_range_high")))
            .fill_null(lit(false))
            .alias("raw_entry_long");

        // Exit: close < range_low
        let raw_exit_long = col("close")
            .lt(col("orb_range_low"))
            .fill_null(lit(false))
            .alias("raw_exit_long");

        lf.with_columns([raw_entry_long, raw_exit_long])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Entry short: close < range_low AND range is complete
        let raw_entry_short = col("orb_is_complete")
            .and(col("close").lt(col("orb_range_low")))
            .fill_null(lit(false))
            .alias("raw_entry_short");

        // Exit short: close > range_high
        let raw_exit_short = col("close")
            .gt(col("orb_range_high"))
            .fill_null(lit(false))
            .alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

// =============================================================================
// ParabolicSARV2 - Parabolic SAR Strategy
// =============================================================================

/// Parabolic SAR strategy (V2 Polars-native implementation).
///
/// Sequential mode uses the exact stateful Parabolic SAR indicator.
/// Polars mode uses a simplified ATR-based approximation (see `apply_parabolic_sar_exprs`).
#[derive(Debug, Clone)]
pub struct ParabolicSARV2 {
    spec: StrategySpec,
    af_start: f64,
    af_step: f64,
    af_max: f64,
    trading_mode: TradingMode,
}

impl ParabolicSARV2 {
    pub fn new(af_start: f64, af_step: f64, af_max: f64) -> Self {
        assert!(af_start > 0.0, "AF start must be positive");
        assert!(af_step > 0.0, "AF step must be positive");
        assert!(af_max > 0.0, "AF max must be positive");
        assert!(af_start <= af_max, "AF start must not exceed AF max");
        Self {
            spec: StrategySpec::parabolic_sar(af_start, af_step, af_max),
            af_start,
            af_step,
            af_max,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Standard Parabolic SAR (0.02/0.02/0.20 - Wilder's default).
    pub fn standard() -> Self {
        Self::new(0.02, 0.02, 0.20)
    }

    /// Slow Parabolic SAR (0.01/0.01/0.10 - fewer whipsaws).
    pub fn slow() -> Self {
        Self::new(0.01, 0.01, 0.10)
    }

    /// Fast Parabolic SAR (0.03/0.03/0.30 - quicker signals).
    pub fn fast() -> Self {
        Self::new(0.03, 0.03, 0.30)
    }

    /// Set trading mode (builder pattern).
    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for ParabolicSARV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }

    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }

        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        // Use the exact sequential Parabolic SAR indicator
        let sar_values = parabolic_sar(bars, self.af_start, self.af_step, self.af_max);
        let current = match sar_values[current_idx] {
            Some(v) => v,
            None => return Signal::Hold,
        };

        // Need previous value for flip detection
        if current_idx == 0 {
            return Signal::Hold;
        }
        let prev = match sar_values[current_idx - 1] {
            Some(v) => v,
            None => return Signal::Hold,
        };

        match current_position {
            Position::Flat => {
                // Entry: SAR flips from downtrend to uptrend
                if current.just_flipped_bullish(Some(&prev)) {
                    return Signal::EnterLong;
                }
                // Also enter if we're in uptrend at warmup boundary
                if current_idx == self.warmup_period() && current.is_uptrend {
                    return Signal::EnterLong;
                }
                Signal::Hold
            }
            Position::Long => {
                // Exit: SAR flips from uptrend to downtrend
                if current.just_flipped_bearish(Some(&prev)) {
                    return Signal::ExitLong;
                }
                Signal::Hold
            }
            Position::Short => Signal::Hold,
        }
    }

    fn add_indicators_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Use simplified ATR-based approximation for Polars mode
        // af_equiv = midpoint between af_start and af_max scaled to ATR units
        // A typical effective AF is around 0.10-0.15
        let af_equiv = (self.af_start + self.af_max) / 2.0 * 10.0; // Scale for ATR
        apply_parabolic_sar_exprs(lf, 14, af_equiv)
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Trend flip detection using sar_is_uptrend column
        let up = col("sar_is_uptrend");
        let prev_up = col("sar_is_uptrend").shift(lit(1)).fill_null(lit(false));

        // Entry: uptrend starts (was not uptrend, now is uptrend)
        let raw_entry = up.clone().and(prev_up.clone().not()).alias("raw_entry");

        // Exit: uptrend ends (was uptrend, now is not uptrend)
        let raw_exit = up.not().and(prev_up).alias("raw_exit");

        lf.with_columns([raw_entry, raw_exit])
    }

    fn add_short_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        // Short signals are opposite of long signals
        let up = col("sar_is_uptrend");
        let prev_up = col("sar_is_uptrend").shift(lit(1)).fill_null(lit(false));

        // Short entry: downtrend starts
        let raw_entry_short = up
            .clone()
            .not()
            .and(prev_up.clone())
            .alias("raw_entry_short");

        // Short exit: downtrend ends
        let raw_exit_short = up.and(prev_up.not()).alias("raw_exit_short");

        lf.with_columns([raw_entry_short, raw_exit_short])
    }
}

// =============================================================================
// EnsembleV2 - Multi-Horizon Ensemble Strategy
// =============================================================================

/// Multi-Horizon Ensemble strategy (V2 Polars-native implementation).
///
/// Combines multiple child strategies with different parameterizations and
/// aggregates their signals using a voting mechanism.
///
/// Sequential mode uses VotingMethod::vote() to aggregate child signals.
/// Polars mode computes child signals in parallel and aggregates vectorized.
pub struct EnsembleV2 {
    spec: StrategySpec,
    children: Vec<Box<dyn StrategyV2>>,
    horizons: Vec<usize>,
    voting: VotingMethod,
    trading_mode: TradingMode,
}

impl std::fmt::Debug for EnsembleV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnsembleV2")
            .field("spec", &self.spec)
            .field("children", &format!("[{} strategies]", self.children.len()))
            .field("horizons", &self.horizons)
            .field("voting", &self.voting)
            .field("trading_mode", &self.trading_mode)
            .finish()
    }
}

impl EnsembleV2 {
    /// Create a new Ensemble strategy from child V2 strategies.
    pub fn new(
        children: Vec<Box<dyn StrategyV2>>,
        horizons: Vec<usize>,
        voting: VotingMethod,
    ) -> Self {
        assert!(
            !children.is_empty(),
            "Ensemble must have at least one child"
        );
        assert_eq!(
            children.len(),
            horizons.len(),
            "Children and horizons must match"
        );

        let child_specs: Vec<StrategySpec> = children.iter().map(|c| c.spec().clone()).collect();
        let spec = StrategySpec::ensemble(child_specs, horizons.clone(), voting);

        Self {
            spec,
            children,
            horizons,
            voting,
            trading_mode: TradingMode::LongOnly,
        }
    }

    /// Create from child StrategySpecs.
    pub fn from_specs(
        child_specs: Vec<StrategySpec>,
        horizons: Vec<usize>,
        voting: VotingMethod,
    ) -> Self {
        let children: Vec<Box<dyn StrategyV2>> =
            child_specs.iter().map(create_strategy_v2).collect();
        Self::new(children, horizons, voting)
    }

    /// Create a Donchian Triple ensemble (20/55/100 day breakouts).
    pub fn donchian_triple() -> Self {
        Self::new(
            vec![
                Box::new(DonchianBreakoutV2::new(20, 10)),
                Box::new(DonchianBreakoutV2::new(55, 20)),
                Box::new(DonchianBreakoutV2::new(100, 40)),
            ],
            vec![20, 55, 100],
            VotingMethod::Majority,
        )
    }

    /// Create an MA Triple ensemble (10/50/200 crossovers).
    pub fn ma_triple() -> Self {
        Self::new(
            vec![
                Box::new(MACrossoverV2::new(5, 10, MAType::EMA)),
                Box::new(MACrossoverV2::new(20, 50, MAType::SMA)),
                Box::new(MACrossoverV2::new(50, 200, MAType::SMA)),
            ],
            vec![10, 50, 200],
            VotingMethod::WeightedByHorizon,
        )
    }

    /// Create a TSMOM Multi ensemble (21/63/126/252 day momentum).
    pub fn tsmom_multi() -> Self {
        Self::new(
            vec![
                Box::new(TsmomV2::new(21)),
                Box::new(TsmomV2::new(63)),
                Box::new(TsmomV2::new(126)),
                Box::new(TsmomV2::new(252)),
            ],
            vec![21, 63, 126, 252],
            VotingMethod::Majority,
        )
    }

    /// Create from a base strategy type and horizons.
    pub fn from_base_strategy(
        base: crate::sweep::StrategyTypeId,
        horizons: Vec<usize>,
        voting: VotingMethod,
    ) -> Self {
        use crate::sweep::StrategyTypeId;
        let children: Vec<Box<dyn StrategyV2>> = horizons
            .iter()
            .map(|&h| -> Box<dyn StrategyV2> {
                match base {
                    StrategyTypeId::Donchian => Box::new(DonchianBreakoutV2::new(h, h / 2)),
                    StrategyTypeId::TurtleS1 => Box::new(DonchianBreakoutV2::turtle_system_1()),
                    StrategyTypeId::TurtleS2 => Box::new(DonchianBreakoutV2::turtle_system_2()),
                    StrategyTypeId::MACrossover => {
                        Box::new(MACrossoverV2::new(h / 4, h, MAType::SMA))
                    }
                    StrategyTypeId::Tsmom => Box::new(TsmomV2::new(h)),
                    _ => Box::new(DonchianBreakoutV2::new(h, h / 2)),
                }
            })
            .collect();
        Self::new(children, horizons, voting)
    }

    pub fn voting(&self) -> VotingMethod {
        self.voting
    }
    pub fn horizons(&self) -> &[usize] {
        &self.horizons
    }
    pub fn num_children(&self) -> usize {
        self.children.len()
    }

    pub fn trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }
}

impl StrategyV2 for EnsembleV2 {
    fn spec(&self) -> &StrategySpec {
        &self.spec
    }
    fn trading_mode(&self) -> TradingMode {
        self.trading_mode
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        if bars.is_empty() {
            return Signal::Hold;
        }
        let current_idx = bars.len() - 1;
        if current_idx < self.warmup_period() {
            return Signal::Hold;
        }

        let signals: Vec<Signal> = self
            .children
            .iter()
            .map(|child| child.signal(bars, current_position))
            .collect();
        self.voting.vote(&signals, &self.horizons)
    }

    fn add_indicators_to_lf(&self, mut lf: LazyFrame) -> LazyFrame {
        for child in &self.children {
            lf = child.add_indicators_to_lf(lf);
        }
        lf
    }

    fn add_signals_to_lf(&self, lf: LazyFrame) -> LazyFrame {
        let mut child_entry_cols: Vec<Expr> = Vec::with_capacity(self.children.len());
        let mut child_exit_cols: Vec<Expr> = Vec::with_capacity(self.children.len());
        let mut result_lf = lf;

        for (i, child) in self.children.iter().enumerate() {
            result_lf = child.add_indicators_to_lf(result_lf);
            result_lf = child.add_signals_to_lf(result_lf);

            let entry_name = format!("child_{}_entry", i);
            let exit_name = format!("child_{}_exit", i);

            result_lf = result_lf.with_columns([
                col("raw_entry").alias(&entry_name),
                col("raw_exit").alias(&exit_name),
            ]);

            child_entry_cols.push(col(&entry_name));
            child_exit_cols.push(col(&exit_name));
        }

        let (raw_entry, raw_exit) = match self.voting {
            VotingMethod::Majority => {
                let n = self.children.len();
                let majority = (n / 2 + 1) as i32;

                let mut entry_sum = lit(0i32);
                for c in &child_entry_cols {
                    entry_sum = entry_sum + when(c.clone()).then(lit(1i32)).otherwise(lit(0i32));
                }
                let mut exit_sum = lit(0i32);
                for c in &child_exit_cols {
                    exit_sum = exit_sum + when(c.clone()).then(lit(1i32)).otherwise(lit(0i32));
                }

                (
                    entry_sum.gt_eq(lit(majority)).alias("raw_entry"),
                    exit_sum.gt_eq(lit(majority)).alias("raw_exit"),
                )
            }
            VotingMethod::WeightedByHorizon => {
                let total: usize = self.horizons.iter().sum();
                let threshold = (total / 2) as i32;

                let mut entry_w = lit(0i32);
                for (c, &h) in child_entry_cols.iter().zip(&self.horizons) {
                    entry_w = entry_w + when(c.clone()).then(lit(h as i32)).otherwise(lit(0i32));
                }
                let mut exit_w = lit(0i32);
                for (c, &h) in child_exit_cols.iter().zip(&self.horizons) {
                    exit_w = exit_w + when(c.clone()).then(lit(h as i32)).otherwise(lit(0i32));
                }

                (
                    entry_w.gt(lit(threshold)).alias("raw_entry"),
                    exit_w.gt(lit(threshold)).alias("raw_exit"),
                )
            }
            VotingMethod::UnanimousEntry => {
                let mut all_entry = lit(true);
                for c in &child_entry_cols {
                    all_entry = all_entry.and(c.clone());
                }
                let mut any_exit = lit(false);
                for c in &child_exit_cols {
                    any_exit = any_exit.or(c.clone());
                }

                (all_entry.alias("raw_entry"), any_exit.alias("raw_exit"))
            }
        };

        result_lf.with_columns([raw_entry, raw_exit])
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
        StrategySpec::FiftyTwoWeekHigh {
            period,
            entry_pct,
            exit_pct,
        } => Box::new(FiftyTwoWeekHighV2::new(*period, *entry_pct, *exit_pct)),
        StrategySpec::DmiAdx {
            di_period,
            adx_period,
            adx_threshold,
        } => Box::new(DmiAdxV2::new(*di_period, *adx_period, *adx_threshold)),
        StrategySpec::BollingerSqueeze {
            period,
            std_mult,
            squeeze_threshold,
        } => Box::new(BollingerSqueezeV2::new(
            *period,
            *std_mult,
            *squeeze_threshold,
        )),
        StrategySpec::Aroon { period } => Box::new(AroonV2::new(*period)),
        StrategySpec::Keltner {
            ema_period,
            atr_period,
            multiplier,
        } => Box::new(KeltnerV2::new(*ema_period, *atr_period, *multiplier)),
        StrategySpec::HeikinAshi { confirmation_bars } => {
            Box::new(HeikinAshiV2::new(*confirmation_bars))
        }
        StrategySpec::Starc {
            sma_period,
            atr_period,
            multiplier,
        } => Box::new(StarcV2::new(*sma_period, *atr_period, *multiplier)),
        StrategySpec::Supertrend {
            atr_period,
            multiplier,
        } => Box::new(SupertrendV2::new(*atr_period, *multiplier)),
        StrategySpec::DarvasBox { confirmation_bars } => {
            Box::new(DarvasBoxV2::new(*confirmation_bars))
        }
        StrategySpec::LarryWilliams {
            range_mult,
            atr_stop_mult,
            atr_period,
        } => Box::new(LarryWilliamsV2::new(
            *range_mult,
            *atr_stop_mult,
            *atr_period,
        )),
        StrategySpec::OpeningRangeBreakout { range_bars, period } => {
            Box::new(OpeningRangeBreakoutV2::new(*range_bars, *period))
        }
        StrategySpec::ParabolicSar {
            af_start,
            af_step,
            af_max,
        } => Box::new(ParabolicSARV2::new(*af_start, *af_step, *af_max)),
        StrategySpec::Ensemble {
            children,
            horizons,
            voting,
        } => Box::new(EnsembleV2::from_specs(
            children.clone(),
            horizons.clone(),
            *voting,
        )),
        // Phase 5 oscillator strategies - not yet implemented as V2
        _ => panic!("StrategyV2 not yet implemented for this StrategySpec variant. Use the legacy Strategy trait."),
    }
}

/// Create a StrategyV2 implementation from a StrategyConfigId.
///
/// This is used for Polars-native sweeps where we need StrategyV2 implementations
/// instead of the legacy Strategy trait.
///
/// Returns `Err` for strategy types that don't have V2 implementations yet.
pub fn create_strategy_v2_from_config(
    config: &crate::sweep::StrategyConfigId,
) -> crate::error::Result<Box<dyn StrategyV2>> {
    use crate::sweep::StrategyConfigId;

    match config {
        StrategyConfigId::Donchian {
            entry_lookback,
            exit_lookback,
        } => Ok(Box::new(DonchianBreakoutV2::new(
            *entry_lookback,
            *exit_lookback,
        ))),
        StrategyConfigId::TurtleS1 => Ok(Box::new(DonchianBreakoutV2::turtle_system_1())),
        StrategyConfigId::TurtleS2 => Ok(Box::new(DonchianBreakoutV2::turtle_system_2())),
        StrategyConfigId::MACrossover {
            fast,
            slow,
            ma_type,
        } => Ok(Box::new(MACrossoverV2::new(*fast, *slow, *ma_type))),
        StrategyConfigId::Tsmom { lookback } => Ok(Box::new(TsmomV2::new(*lookback))),
        StrategyConfigId::FiftyTwoWeekHigh {
            period,
            entry_pct,
            exit_pct,
        } => Ok(Box::new(FiftyTwoWeekHighV2::new(
            *period, *entry_pct, *exit_pct,
        ))),
        StrategyConfigId::DmiAdx {
            di_period,
            adx_period,
            adx_threshold,
        } => Ok(Box::new(DmiAdxV2::new(
            *di_period,
            *adx_period,
            *adx_threshold,
        ))),
        StrategyConfigId::BollingerSqueeze {
            period,
            std_mult,
            squeeze_threshold,
        } => Ok(Box::new(BollingerSqueezeV2::new(
            *period,
            *std_mult,
            *squeeze_threshold,
        ))),
        StrategyConfigId::Aroon { period } => Ok(Box::new(AroonV2::new(*period))),
        StrategyConfigId::Keltner {
            ema_period,
            atr_period,
            multiplier,
        } => Ok(Box::new(KeltnerV2::new(
            *ema_period,
            *atr_period,
            *multiplier,
        ))),
        StrategyConfigId::HeikinAshi { confirmation_bars } => {
            Ok(Box::new(HeikinAshiV2::new(*confirmation_bars)))
        }
        StrategyConfigId::STARC {
            sma_period,
            atr_period,
            multiplier,
        } => Ok(Box::new(StarcV2::new(
            *sma_period,
            *atr_period,
            *multiplier,
        ))),
        StrategyConfigId::Supertrend {
            atr_period,
            multiplier,
        } => Ok(Box::new(SupertrendV2::new(*atr_period, *multiplier))),
        StrategyConfigId::DarvasBox {
            box_confirmation_bars,
        } => Ok(Box::new(DarvasBoxV2::new(*box_confirmation_bars))),
        StrategyConfigId::LarryWilliams {
            range_multiplier,
            atr_stop_mult,
        } => Ok(Box::new(LarryWilliamsV2::new(
            *range_multiplier,
            *atr_stop_mult,
            14, // Default ATR period
        ))),
        StrategyConfigId::OpeningRangeBreakout { range_bars, period } => {
            Ok(Box::new(OpeningRangeBreakoutV2::new(*range_bars, *period)))
        }
        StrategyConfigId::ParabolicSar {
            af_start,
            af_step,
            af_max,
        } => Ok(Box::new(ParabolicSARV2::new(*af_start, *af_step, *af_max))),
        StrategyConfigId::Ensemble {
            base_strategy,
            horizons,
            voting,
        } => Ok(Box::new(EnsembleV2::from_base_strategy(
            *base_strategy,
            horizons.clone(),
            *voting,
        ))),
        // Phase 5 oscillator strategies - not yet implemented as V2
        _ => Err(crate::error::TrendLabError::Strategy(
            "StrategyV2 not yet implemented for this StrategyConfigId variant".to_string(),
        )),
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
