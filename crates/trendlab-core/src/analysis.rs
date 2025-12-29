//! Post-backtest statistical analysis module.
//!
//! Provides advanced statistical analysis for backtest results:
//! - Return distribution metrics (VaR, CVaR, skewness, kurtosis)
//! - Regime-based performance analysis (volatility regimes)
//! - Trade-level analysis (MAE, MFE, holding period, edge ratio)
//!
//! Designed for swing trading (2-10 week holding periods) and options overlay decisions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Configuration for statistical analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// ATR period for regime classification.
    pub atr_period: usize,
    /// High volatility threshold (multiple of median ATR, e.g., 1.5).
    pub high_vol_threshold: f64,
    /// Low volatility threshold (multiple of median ATR, e.g., 0.75).
    pub low_vol_threshold: f64,
    /// VaR confidence levels to compute (e.g., [0.95, 0.99]).
    pub var_levels: Vec<f64>,
    /// Holding period histogram bucket edges (in trading days).
    pub holding_buckets: Vec<usize>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            atr_period: 20,
            high_vol_threshold: 1.5,
            low_vol_threshold: 0.75,
            var_levels: vec![0.95, 0.99],
            // Default buckets: 1-5, 6-10, 11-20, 21-50, 50+ days
            holding_buckets: vec![5, 10, 20, 50],
        }
    }
}

/// Complete statistical analysis for a backtest configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalAnalysis {
    /// Distribution of daily returns.
    pub return_distribution: ReturnDistribution,
    /// Performance across volatility regimes.
    pub regime_analysis: RegimeAnalysis,
    /// Trade-level statistics.
    pub trade_analysis: TradeAnalysis,
    /// Timestamp when analysis was computed.
    pub computed_at: DateTime<Utc>,
    /// Configuration used for analysis.
    pub config: AnalysisConfig,
}

impl Default for StatisticalAnalysis {
    fn default() -> Self {
        Self {
            return_distribution: ReturnDistribution::default(),
            regime_analysis: RegimeAnalysis::default(),
            trade_analysis: TradeAnalysis::default(),
            computed_at: Utc::now(),
            config: AnalysisConfig::default(),
        }
    }
}

// =============================================================================
// RETURN DISTRIBUTION METRICS
// =============================================================================

/// Return distribution statistics for risk assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnDistribution {
    /// Value at Risk at 95% confidence (5th percentile daily loss, as positive).
    pub var_95: f64,
    /// Value at Risk at 99% confidence (1st percentile daily loss, as positive).
    pub var_99: f64,
    /// Conditional VaR (Expected Shortfall) at 95% - average loss beyond VaR.
    pub cvar_95: f64,
    /// Conditional VaR at 99%.
    pub cvar_99: f64,
    /// Skewness of daily returns (negative = fat left tail).
    pub skewness: f64,
    /// Excess kurtosis of daily returns (high = more extreme moves).
    pub kurtosis: f64,
    /// Mean daily return.
    pub mean_return: f64,
    /// Standard deviation of daily returns.
    pub std_return: f64,
    /// Minimum daily return (worst day).
    pub min_return: f64,
    /// Maximum daily return (best day).
    pub max_return: f64,
    /// Number of observations used.
    pub n_observations: usize,
}

impl Default for ReturnDistribution {
    fn default() -> Self {
        Self {
            var_95: 0.0,
            var_99: 0.0,
            cvar_95: 0.0,
            cvar_99: 0.0,
            skewness: 0.0,
            kurtosis: 0.0,
            mean_return: 0.0,
            std_return: 0.0,
            min_return: 0.0,
            max_return: 0.0,
            n_observations: 0,
        }
    }
}

// =============================================================================
// REGIME ANALYSIS
// =============================================================================

/// Regime-based performance analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeAnalysis {
    /// Performance in high-volatility periods (ATR > high_threshold * median).
    pub high_vol: RegimeMetrics,
    /// Performance in neutral volatility periods.
    pub neutral_vol: RegimeMetrics,
    /// Performance in low-volatility periods (ATR < low_threshold * median).
    pub low_vol: RegimeMetrics,
    /// Median ATR value used as baseline.
    pub median_atr: f64,
    /// ATR period used for classification.
    pub atr_period: usize,
}

impl Default for RegimeAnalysis {
    fn default() -> Self {
        Self {
            high_vol: RegimeMetrics::default(),
            neutral_vol: RegimeMetrics::default(),
            low_vol: RegimeMetrics::default(),
            median_atr: 0.0,
            atr_period: 20,
        }
    }
}

/// Performance metrics for a specific volatility regime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeMetrics {
    /// Number of trading days in this regime.
    pub n_days: usize,
    /// Percentage of total days in this regime.
    pub pct_days: f64,
    /// Number of trades entered during this regime.
    pub n_trades_entered: usize,
    /// Win rate for trades entered in this regime.
    pub win_rate: f64,
    /// Average return per trade.
    pub avg_trade_return: f64,
    /// Total return during this regime.
    pub total_return: f64,
    /// Sharpe ratio during this regime (annualized).
    pub sharpe: f64,
}

impl Default for RegimeMetrics {
    fn default() -> Self {
        Self {
            n_days: 0,
            pct_days: 0.0,
            n_trades_entered: 0,
            win_rate: 0.0,
            avg_trade_return: 0.0,
            total_return: 0.0,
            sharpe: 0.0,
        }
    }
}

/// Volatility regime classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum VolRegime {
    High,
    Neutral,
    Low,
}

// =============================================================================
// TREND REGIME ANALYSIS
// =============================================================================

/// Trend regime classification based on MA slope or ADX thresholds.
///
/// Used to understand whether a strategy performs consistently across
/// different market trend conditions or is concentrated in specific regimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum TrendRegime {
    /// Strong uptrend: MA slope > high threshold (e.g., > +2% per period)
    StrongUp,
    /// Weak uptrend: 0 < MA slope <= high threshold
    WeakUp,
    /// Neutral/sideways: MA slope near zero
    Neutral,
    /// Weak downtrend: low threshold <= MA slope < 0
    WeakDown,
    /// Strong downtrend: MA slope < low threshold (e.g., < -2% per period)
    StrongDown,
}

impl TrendRegime {
    /// Returns all regime variants for iteration.
    pub fn all() -> &'static [TrendRegime] {
        &[
            TrendRegime::StrongUp,
            TrendRegime::WeakUp,
            TrendRegime::Neutral,
            TrendRegime::WeakDown,
            TrendRegime::StrongDown,
        ]
    }

    /// Display name for UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            TrendRegime::StrongUp => "Strong Up",
            TrendRegime::WeakUp => "Weak Up",
            TrendRegime::Neutral => "Neutral",
            TrendRegime::WeakDown => "Weak Down",
            TrendRegime::StrongDown => "Strong Down",
        }
    }
}

/// Trend regime analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendRegimeAnalysis {
    /// Performance metrics per trend regime.
    pub by_regime: std::collections::HashMap<TrendRegime, RegimeMetrics>,
    /// MA period used for trend classification.
    pub ma_period: usize,
    /// Slope threshold for strong trend classification (as % per period).
    pub strong_threshold: f64,
    /// Slope threshold for weak trend classification (as % per period).
    pub weak_threshold: f64,
}

impl Default for TrendRegimeAnalysis {
    fn default() -> Self {
        Self {
            by_regime: std::collections::HashMap::new(),
            ma_period: 50,
            strong_threshold: 0.02, // 2% per period
            weak_threshold: 0.005,  // 0.5% per period
        }
    }
}

// =============================================================================
// DRAWDOWN REGIME ANALYSIS
// =============================================================================

/// Drawdown regime classification based on current drawdown from peak.
///
/// Used to understand whether a strategy performs consistently across
/// different drawdown states or is concentrated in specific regimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum DrawdownRegime {
    /// Normal: drawdown < 5% from peak
    Normal,
    /// Shallow drawdown: 5% <= drawdown < 10%
    Shallow,
    /// Deep drawdown: 10% <= drawdown < 20%
    Deep,
    /// Recovery: drawdown >= 20% (crisis/recovery mode)
    Recovery,
}

impl DrawdownRegime {
    /// Returns all regime variants for iteration.
    pub fn all() -> &'static [DrawdownRegime] {
        &[
            DrawdownRegime::Normal,
            DrawdownRegime::Shallow,
            DrawdownRegime::Deep,
            DrawdownRegime::Recovery,
        ]
    }

    /// Display name for UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            DrawdownRegime::Normal => "Normal (<5%)",
            DrawdownRegime::Shallow => "Shallow (5-10%)",
            DrawdownRegime::Deep => "Deep (10-20%)",
            DrawdownRegime::Recovery => "Recovery (>20%)",
        }
    }

    /// Classify a drawdown percentage into a regime.
    pub fn from_drawdown_pct(dd_pct: f64) -> Self {
        let dd_abs = dd_pct.abs();
        if dd_abs < 0.05 {
            DrawdownRegime::Normal
        } else if dd_abs < 0.10 {
            DrawdownRegime::Shallow
        } else if dd_abs < 0.20 {
            DrawdownRegime::Deep
        } else {
            DrawdownRegime::Recovery
        }
    }
}

/// Drawdown regime analysis results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrawdownRegimeAnalysis {
    /// Performance metrics per drawdown regime.
    pub by_regime: std::collections::HashMap<DrawdownRegime, RegimeMetrics>,
    /// Thresholds used for classification (as fractions, e.g., 0.05 = 5%).
    pub thresholds: DrawdownThresholds,
}

/// Drawdown thresholds for regime classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawdownThresholds {
    /// Threshold for shallow drawdown (default 5%).
    pub shallow: f64,
    /// Threshold for deep drawdown (default 10%).
    pub deep: f64,
    /// Threshold for recovery/crisis (default 20%).
    pub recovery: f64,
}

impl Default for DrawdownThresholds {
    fn default() -> Self {
        Self {
            shallow: 0.05,
            deep: 0.10,
            recovery: 0.20,
        }
    }
}

// =============================================================================
// REGIME CONCENTRATION SCORING
// =============================================================================

/// Regime concentration analysis for a strategy.
///
/// Measures how concentrated a strategy's returns are in specific regimes.
/// Strategies that only work in one regime are penalized for lack of robustness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeConcentrationScore {
    /// Concentration score for volatility regimes (0-1, lower is better).
    /// 0 = perfectly balanced, 1 = all returns from one regime.
    pub vol_concentration: f64,

    /// Concentration score for trend regimes (0-1, lower is better).
    pub trend_concentration: f64,

    /// Concentration score for drawdown regimes (0-1, lower is better).
    pub drawdown_concentration: f64,

    /// Combined concentration score (weighted average).
    pub combined_score: f64,

    /// Which volatility regime contributes most returns (if concentrated).
    pub dominant_vol_regime: Option<VolRegime>,

    /// Which trend regime contributes most returns (if concentrated).
    pub dominant_trend_regime: Option<TrendRegime>,

    /// Which drawdown regime contributes most returns (if concentrated).
    pub dominant_drawdown_regime: Option<DrawdownRegime>,

    /// Percentage of total return from dominant vol regime.
    pub vol_regime_pct: f64,

    /// Percentage of total return from dominant trend regime.
    pub trend_regime_pct: f64,

    /// Percentage of total return from dominant drawdown regime.
    pub drawdown_regime_pct: f64,
}

impl Default for RegimeConcentrationScore {
    fn default() -> Self {
        Self {
            vol_concentration: 0.0,
            trend_concentration: 0.0,
            drawdown_concentration: 0.0,
            combined_score: 0.0,
            dominant_vol_regime: None,
            dominant_trend_regime: None,
            dominant_drawdown_regime: None,
            vol_regime_pct: 0.0,
            trend_regime_pct: 0.0,
            drawdown_regime_pct: 0.0,
        }
    }
}

impl RegimeConcentrationScore {
    /// Returns true if the strategy is highly concentrated (penalty-worthy).
    /// Default threshold: combined score > 0.7 (70%+ of returns from one regime).
    pub fn is_concentrated(&self) -> bool {
        self.combined_score > 0.7
    }

    /// Returns a penalty factor (0-1) based on concentration.
    /// 0 = no penalty, 1 = full penalty.
    /// Scales linearly from threshold (0.5) to maximum (1.0).
    pub fn penalty_factor(&self) -> f64 {
        let threshold = 0.5;
        if self.combined_score <= threshold {
            0.0
        } else {
            ((self.combined_score - threshold) / (1.0 - threshold)).min(1.0)
        }
    }
}

// =============================================================================
// TRADE-LEVEL ANALYSIS
// =============================================================================

/// Trade-level analysis for swing trading.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradeAnalysis {
    /// Holding period distribution.
    pub holding_period: HoldingPeriodStats,
    /// Maximum Adverse Excursion statistics.
    pub mae: ExcursionStats,
    /// Maximum Favorable Excursion statistics.
    pub mfe: ExcursionStats,
    /// Edge ratio (MFE/MAE) statistics.
    pub edge_ratio: EdgeRatioStats,
    /// Volatility at entry analysis.
    pub vol_at_entry: VolAtEntryStats,
    /// Number of trades analyzed.
    pub n_trades: usize,
}

/// Holding period distribution statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldingPeriodStats {
    /// Mean holding period (trading days).
    pub mean: f64,
    /// Median holding period.
    pub median: f64,
    /// Standard deviation of holding period.
    pub std: f64,
    /// Minimum holding period.
    pub min: usize,
    /// Maximum holding period.
    pub max: usize,
    /// 25th percentile.
    pub p25: f64,
    /// 75th percentile.
    pub p75: f64,
    /// Histogram: bucket label -> count.
    pub histogram: Vec<HoldingBucket>,
}

impl Default for HoldingPeriodStats {
    fn default() -> Self {
        Self {
            mean: 0.0,
            median: 0.0,
            std: 0.0,
            min: 0,
            max: 0,
            p25: 0.0,
            p75: 0.0,
            histogram: Vec::new(),
        }
    }
}

/// A bucket in the holding period histogram.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldingBucket {
    /// Bucket label (e.g., "1-5 days").
    pub label: String,
    /// Number of trades in this bucket.
    pub count: usize,
    /// Percentage of total trades.
    pub pct: f64,
    /// Average return for trades in this bucket.
    pub avg_return: f64,
    /// Win rate for trades in this bucket.
    pub win_rate: f64,
}

/// Excursion statistics (for both MAE and MFE).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcursionStats {
    /// Mean excursion (as % of entry price).
    pub mean: f64,
    /// Median excursion.
    pub median: f64,
    /// Standard deviation.
    pub std: f64,
    /// Maximum excursion.
    pub max: f64,
    /// 75th percentile.
    pub p75: f64,
    /// 90th percentile.
    pub p90: f64,
    /// Mean for winning trades only.
    pub winners_mean: f64,
    /// Mean for losing trades only.
    pub losers_mean: f64,
}

impl Default for ExcursionStats {
    fn default() -> Self {
        Self {
            mean: 0.0,
            median: 0.0,
            std: 0.0,
            max: 0.0,
            p75: 0.0,
            p90: 0.0,
            winners_mean: 0.0,
            losers_mean: 0.0,
        }
    }
}

/// Edge ratio (MFE/MAE) statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRatioStats {
    /// Mean edge ratio.
    pub mean: f64,
    /// Median edge ratio.
    pub median: f64,
    /// Percentage of trades with edge ratio > 1 (MFE exceeded MAE).
    pub pct_favorable: f64,
    /// Mean edge ratio for winning trades.
    pub winners_mean: f64,
    /// Mean edge ratio for losing trades.
    pub losers_mean: f64,
}

impl Default for EdgeRatioStats {
    fn default() -> Self {
        Self {
            mean: 0.0,
            median: 0.0,
            pct_favorable: 0.0,
            winners_mean: 0.0,
            losers_mean: 0.0,
        }
    }
}

/// Volatility at entry analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolAtEntryStats {
    /// Mean ATR/price at entry (as percentage).
    pub mean_atr_pct: f64,
    /// Median ATR/price at entry.
    pub median_atr_pct: f64,
    /// Correlation between entry volatility and trade return.
    pub return_correlation: f64,
    /// Correlation between entry volatility and trade win/loss.
    pub win_correlation: f64,
    /// Mean entry vol for winning trades.
    pub winners_mean_vol: f64,
    /// Mean entry vol for losing trades.
    pub losers_mean_vol: f64,
}

impl Default for VolAtEntryStats {
    fn default() -> Self {
        Self {
            mean_atr_pct: 0.0,
            median_atr_pct: 0.0,
            return_correlation: 0.0,
            win_correlation: 0.0,
            winners_mean_vol: 0.0,
            losers_mean_vol: 0.0,
        }
    }
}

// =============================================================================
// TRADE EXCURSION DATA (for computation)
// =============================================================================

/// Raw excursion data for a single trade (used during computation).
#[derive(Debug, Clone)]
pub struct TradeExcursion {
    /// Maximum Adverse Excursion as percentage of entry price.
    pub mae_pct: f64,
    /// Maximum Favorable Excursion as percentage of entry price.
    pub mfe_pct: f64,
    /// Holding period in trading days.
    pub holding_days: usize,
    /// ATR at entry (as percentage of entry price).
    pub entry_atr_pct: f64,
    /// Trade return (net PnL as percentage of entry notional).
    pub return_pct: f64,
    /// Whether the trade was a winner.
    pub is_winner: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_config_default() {
        let config = AnalysisConfig::default();
        assert_eq!(config.atr_period, 20);
        assert_eq!(config.high_vol_threshold, 1.5);
        assert_eq!(config.low_vol_threshold, 0.75);
        assert_eq!(config.var_levels, vec![0.95, 0.99]);
        assert_eq!(config.holding_buckets, vec![5, 10, 20, 50]);
    }

    #[test]
    fn test_statistical_analysis_default() {
        let analysis = StatisticalAnalysis::default();
        assert_eq!(analysis.return_distribution.n_observations, 0);
        assert_eq!(analysis.regime_analysis.atr_period, 20);
        assert_eq!(analysis.trade_analysis.n_trades, 0);
    }

    #[test]
    fn test_vol_regime_enum() {
        let high = VolRegime::High;
        let neutral = VolRegime::Neutral;
        let low = VolRegime::Low;
        assert_ne!(high, neutral);
        assert_ne!(neutral, low);
    }
}
