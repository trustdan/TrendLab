//! Position sizing strategies.
//!
//! Position sizing determines how many units to trade based on various factors.
//! The Turtle trading system pioneered volatility-based sizing where position
//! size is inversely proportional to recent volatility (ATR).

use crate::bar::Bar;
use crate::indicators::{atr, atr_wilder};

/// Result of a position size calculation.
#[derive(Debug, Clone, Copy)]
pub struct SizeResult {
    /// Number of units to trade.
    pub units: f64,
    /// ATR value used for calculation (if applicable).
    pub atr: Option<f64>,
    /// Dollar volatility per unit (ATR * price).
    pub dollar_vol_per_unit: Option<f64>,
}

/// Configuration for position sizing.
#[derive(Debug, Clone)]
pub struct SizingConfig {
    /// Minimum position size in units.
    pub min_units: f64,
    /// Maximum position size in units.
    pub max_units: f64,
}

impl Default for SizingConfig {
    fn default() -> Self {
        Self {
            min_units: 1.0,
            max_units: f64::MAX,
        }
    }
}

impl SizingConfig {
    /// Clamp units to min/max bounds.
    pub fn clamp(&self, units: f64) -> f64 {
        units.clamp(self.min_units, self.max_units)
    }
}

/// Trait for position sizing strategies.
pub trait PositionSizer: Send + Sync {
    /// Calculate position size for the current bar.
    ///
    /// # Arguments
    /// * `bars` - Historical bars up to and including current bar
    /// * `price` - Current price (typically close or next bar open)
    ///
    /// # Returns
    /// Position size result, or None if sizing cannot be computed (e.g., during warmup)
    fn size(&self, bars: &[Bar], price: f64) -> Option<SizeResult>;

    /// Returns the warmup period needed before sizing can be computed.
    fn warmup_period(&self) -> usize;

    /// Returns a description of the sizing method.
    fn description(&self) -> String;
}

/// Fixed position sizing - always returns the same number of units.
#[derive(Debug, Clone)]
pub struct FixedSizer {
    units: f64,
}

impl FixedSizer {
    pub fn new(units: f64) -> Self {
        assert!(units > 0.0, "Units must be positive");
        Self { units }
    }
}

impl PositionSizer for FixedSizer {
    fn size(&self, _bars: &[Bar], _price: f64) -> Option<SizeResult> {
        Some(SizeResult {
            units: self.units,
            atr: None,
            dollar_vol_per_unit: None,
        })
    }

    fn warmup_period(&self) -> usize {
        0
    }

    fn description(&self) -> String {
        format!("Fixed: {} units", self.units)
    }
}

/// Volatility-based position sizing (Turtle-style).
///
/// Position size is calculated as:
/// Units = Target Dollar Volatility / (ATR × Price)
///
/// This ensures consistent dollar volatility exposure across different
/// instruments and market conditions.
#[derive(Debug, Clone)]
pub struct VolatilitySizer {
    /// Target dollar volatility per day.
    target_volatility: f64,
    /// ATR period for volatility calculation.
    atr_period: usize,
    /// Use Wilder smoothing for ATR.
    use_wilder: bool,
    /// Min/max bounds.
    config: SizingConfig,
}

impl VolatilitySizer {
    pub fn new(target_volatility: f64, atr_period: usize) -> Self {
        assert!(
            target_volatility > 0.0,
            "Target volatility must be positive"
        );
        assert!(atr_period > 0, "ATR period must be at least 1");
        Self {
            target_volatility,
            atr_period,
            use_wilder: false,
            config: SizingConfig::default(),
        }
    }

    /// Create a volatility sizer from account size and risk percentage.
    ///
    /// Target volatility = account_size * risk_percent / 100
    pub fn from_risk(account_size: f64, risk_percent: f64, atr_period: usize) -> Self {
        let target_volatility = account_size * risk_percent / 100.0;
        Self::new(target_volatility, atr_period)
    }

    /// Use Wilder smoothing for ATR calculation.
    pub fn with_wilder(mut self) -> Self {
        self.use_wilder = true;
        self
    }

    /// Set minimum position size.
    pub fn with_min_units(mut self, min: f64) -> Self {
        self.config.min_units = min;
        self
    }

    /// Set maximum position size.
    pub fn with_max_units(mut self, max: f64) -> Self {
        self.config.max_units = max;
        self
    }

    /// Set both min and max bounds.
    pub fn with_bounds(mut self, min: f64, max: f64) -> Self {
        self.config.min_units = min;
        self.config.max_units = max;
        self
    }

    /// Get the target volatility.
    pub fn target_volatility(&self) -> f64 {
        self.target_volatility
    }

    /// Get the ATR period.
    pub fn atr_period(&self) -> usize {
        self.atr_period
    }

    /// Compute position size for a specific ATR and price (for testing).
    pub fn compute_size(&self, atr_value: f64, price: f64) -> f64 {
        if atr_value <= 0.0 || price <= 0.0 {
            return 0.0;
        }
        let dollar_vol_per_unit = atr_value * price;
        let raw_units = self.target_volatility / dollar_vol_per_unit;
        self.config.clamp(raw_units)
    }
}

impl PositionSizer for VolatilitySizer {
    fn size(&self, bars: &[Bar], price: f64) -> Option<SizeResult> {
        if bars.len() < self.atr_period || price <= 0.0 {
            return None;
        }

        let current_idx = bars.len() - 1;

        // Compute ATR
        let atr_values = if self.use_wilder {
            atr_wilder(bars, self.atr_period)
        } else {
            atr(bars, self.atr_period)
        };

        let atr_value = atr_values[current_idx]?;

        if atr_value <= 0.0 {
            return None;
        }

        let dollar_vol_per_unit = atr_value * price;
        let raw_units = self.target_volatility / dollar_vol_per_unit;
        let units = self.config.clamp(raw_units);

        Some(SizeResult {
            units,
            atr: Some(atr_value),
            dollar_vol_per_unit: Some(dollar_vol_per_unit),
        })
    }

    fn warmup_period(&self) -> usize {
        self.atr_period
    }

    fn description(&self) -> String {
        format!(
            "Volatility: target=${:.0}, ATR({}){}",
            self.target_volatility,
            self.atr_period,
            if self.use_wilder { " Wilder" } else { "" }
        )
    }
}

/// Turtle-style position sizing using the "N" concept.
///
/// The Turtles defined:
/// - N = 20-day ATR (using exponential average)
/// - Dollar Volatility = N × Dollars per Point
/// - Unit = 1% of Account / Dollar Volatility
///
/// This is equivalent to VolatilitySizer with risk_percent = 1%.
pub fn turtle_sizer(account_size: f64, atr_period: usize) -> VolatilitySizer {
    VolatilitySizer::from_risk(account_size, 1.0, atr_period).with_wilder()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn bars_from_ohlc(ohlc: &[(f64, f64, f64, f64)]) -> Vec<Bar> {
        ohlc.iter()
            .enumerate()
            .map(|(i, &(o, h, l, c))| {
                let ts = chrono::Utc
                    .with_ymd_and_hms(2024, 1, 1 + i as u32, 0, 0, 0)
                    .unwrap();
                Bar::new(ts, o, h, l, c, 0.0, "TEST", "1d")
            })
            .collect()
    }

    #[test]
    fn fixed_sizer_always_returns_same() {
        let sizer = FixedSizer::new(100.0);
        let bars = bars_from_ohlc(&[(100.0, 105.0, 95.0, 102.0)]);

        let result = sizer.size(&bars, 100.0).unwrap();
        assert_eq!(result.units, 100.0);
        assert!(result.atr.is_none());
    }

    #[test]
    fn volatility_sizer_inversely_proportional() {
        let sizer = VolatilitySizer::new(1000.0, 3);

        // Low volatility: ATR = 2, Price = 100
        // Units = 1000 / (2 * 100) = 5
        let low_vol_bars = bars_from_ohlc(&[
            (100.0, 101.0, 99.0, 100.0), // TR = 2
            (100.0, 101.0, 99.0, 100.0), // TR = 2
            (100.0, 101.0, 99.0, 100.0), // TR = 2
        ]);
        let low_result = sizer.size(&low_vol_bars, 100.0).unwrap();
        assert!((low_result.units - 5.0).abs() < 0.01);

        // High volatility: ATR = 4, Price = 100
        // Units = 1000 / (4 * 100) = 2.5
        let high_vol_bars = bars_from_ohlc(&[
            (100.0, 102.0, 98.0, 100.0), // TR = 4
            (100.0, 102.0, 98.0, 100.0), // TR = 4
            (100.0, 102.0, 98.0, 100.0), // TR = 4
        ]);
        let high_result = sizer.size(&high_vol_bars, 100.0).unwrap();
        assert!((high_result.units - 2.5).abs() < 0.01);

        // High vol should have smaller position
        assert!(high_result.units < low_result.units);
    }

    #[test]
    fn volatility_sizer_respects_bounds() {
        let sizer = VolatilitySizer::new(1000.0, 3)
            .with_min_units(1.0)
            .with_max_units(10.0);

        // Very low volatility would give huge position
        let low_vol_bars = bars_from_ohlc(&[
            (100.0, 100.1, 99.9, 100.0), // TR = 0.2
            (100.0, 100.1, 99.9, 100.0), // TR = 0.2
            (100.0, 100.1, 99.9, 100.0), // TR = 0.2
        ]);
        let result = sizer.size(&low_vol_bars, 100.0).unwrap();
        assert_eq!(result.units, 10.0); // Clamped to max

        // Very high volatility would give tiny position
        let high_vol_bars = bars_from_ohlc(&[
            (100.0, 150.0, 50.0, 100.0), // TR = 100
            (100.0, 150.0, 50.0, 100.0), // TR = 100
            (100.0, 150.0, 50.0, 100.0), // TR = 100
        ]);
        let result = sizer.size(&high_vol_bars, 100.0).unwrap();
        assert_eq!(result.units, 1.0); // Clamped to min
    }

    #[test]
    fn volatility_sizer_warmup() {
        let sizer = VolatilitySizer::new(1000.0, 3);

        // Not enough bars
        let bars = bars_from_ohlc(&[(100.0, 102.0, 98.0, 100.0), (100.0, 102.0, 98.0, 100.0)]);
        assert!(sizer.size(&bars, 100.0).is_none());

        // Warmup period is 3
        assert_eq!(sizer.warmup_period(), 3);
    }

    #[test]
    fn turtle_sizer_formula() {
        // Turtle formula: Units = (Account × 1%) / (ATR × Price)
        // Account = 100,000, Risk = 1%, ATR = 2.5, Price = 50
        // Units = 1000 / (2.5 * 50) = 1000 / 125 = 8
        let sizer = turtle_sizer(100_000.0, 3);

        // Create bars with ATR = 2.5
        let bars = bars_from_ohlc(&[
            (50.0, 51.25, 48.75, 50.0), // TR = 2.5
            (50.0, 51.25, 48.75, 50.0), // TR = 2.5
            (50.0, 51.25, 48.75, 50.0), // TR = 2.5
        ]);

        let result = sizer.size(&bars, 50.0).unwrap();
        assert!((result.units - 8.0).abs() < 0.01);
    }

    #[test]
    fn from_risk_constructor() {
        // Account = 100,000, Risk = 2%
        // Target vol = 100,000 * 0.02 = 2,000
        let sizer = VolatilitySizer::from_risk(100_000.0, 2.0, 14);
        assert_eq!(sizer.target_volatility(), 2000.0);
    }
}
