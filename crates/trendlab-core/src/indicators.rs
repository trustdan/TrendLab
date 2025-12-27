//! Indicator calculations (pure functions, no IO).
//!
//! Key invariant: indicator values at index `t` must depend only on bars `0..=t`.

use crate::bar::Bar;
use chrono::Datelike;

/// Donchian channel values (upper and lower bands).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DonchianChannel {
    pub upper: f64,
    pub lower: f64,
}

/// Compute Donchian channel over a lookback period.
///
/// Donchian channel is defined as:
/// - Upper = highest high over the prior N bars (NOT including current bar)
/// - Lower = lowest low over the prior N bars (NOT including current bar)
///
/// This matches the Turtle trading system convention where a breakout is
/// triggered when the current close exceeds the prior N-day high.
///
/// Returns `None` until there are enough bars to fill the lookback period.
pub fn donchian_channel(bars: &[Bar], lookback: usize) -> Vec<Option<DonchianChannel>> {
    if lookback == 0 {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    for (i, out_slot) in out.iter_mut().enumerate() {
        // Need at least `lookback` prior bars before index i
        if i < lookback {
            continue;
        }

        // Look at bars [i-lookback, i-1] (prior N bars, not including current)
        let start = i - lookback;

        let (highest_high, lowest_low) = bars[start..i]
            .iter()
            .fold((f64::NEG_INFINITY, f64::INFINITY), |(h, l), bar| {
                (h.max(bar.high), l.min(bar.low))
            });

        *out_slot = Some(DonchianChannel {
            upper: highest_high,
            lower: lowest_low,
        });
    }

    out
}

/// Simple moving average of `close` over a fixed window.
///
/// Returns a vector of length `bars.len()`, where values are `None` until there
/// are enough bars to fill the window.
pub fn sma_close(bars: &[Bar], window: usize) -> Vec<Option<f64>> {
    if window == 0 {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];
    let mut sum = 0.0;

    for i in 0..bars.len() {
        sum += bars[i].close;

        if i >= window {
            sum -= bars[i - window].close;
        }

        if i + 1 >= window {
            out[i] = Some(sum / window as f64);
        }
    }

    out
}

/// Exponential moving average of `close` over a fixed window.
///
/// Uses the standard EMA formula:
/// - Multiplier (k) = 2 / (window + 1)
/// - EMA[t] = close[t] * k + EMA[t-1] * (1 - k)
///
/// Returns a vector of length `bars.len()`, where values are `None` until there
/// are enough bars to seed the EMA (uses SMA for first value).
pub fn ema_close(bars: &[Bar], window: usize) -> Vec<Option<f64>> {
    if window == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];
    let k = 2.0 / (window as f64 + 1.0);

    // Seed EMA with SMA of first `window` bars
    if bars.len() >= window {
        let initial_sma: f64 = bars[..window].iter().map(|b| b.close).sum::<f64>() / window as f64;
        out[window - 1] = Some(initial_sma);

        // Calculate EMA for remaining bars
        let mut prev_ema = initial_sma;
        for i in window..bars.len() {
            let ema = bars[i].close * k + prev_ema * (1.0 - k);
            out[i] = Some(ema);
            prev_ema = ema;
        }
    }

    out
}

/// Moving average type for strategy configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MAType {
    SMA,
    EMA,
}

impl MAType {
    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::SMA => "SMA",
            Self::EMA => "EMA",
        }
    }
}

/// Compute True Range for each bar.
///
/// True Range is defined as the maximum of:
/// - Current high - current low
/// - |current high - previous close|
/// - |current low - previous close|
///
/// For the first bar, TR = high - low (no previous close available).
pub fn true_range(bars: &[Bar]) -> Vec<f64> {
    if bars.is_empty() {
        return vec![];
    }

    let mut out = Vec::with_capacity(bars.len());

    // First bar: TR = high - low
    out.push(bars[0].high - bars[0].low);

    // Subsequent bars: TR = max(h-l, |h-prev_c|, |l-prev_c|)
    for i in 1..bars.len() {
        let h = bars[i].high;
        let l = bars[i].low;
        let prev_c = bars[i - 1].close;

        let tr = (h - l).max((h - prev_c).abs()).max((l - prev_c).abs());
        out.push(tr);
    }

    out
}

/// Average True Range (ATR) over a fixed window.
///
/// ATR is the simple moving average of True Range values.
/// Returns `None` until there are enough bars to fill the window.
///
/// This is the standard Wilder ATR calculation used by the Turtle traders:
/// - ATR[t] = SMA(TR, window) for the first ATR value
/// - For subsequent values, can optionally use Wilder smoothing
///
/// This implementation uses simple moving average for consistency with other indicators.
pub fn atr(bars: &[Bar], window: usize) -> Vec<Option<f64>> {
    if window == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let tr = true_range(bars);
    let mut out = vec![None; bars.len()];
    let mut sum = 0.0;

    for i in 0..tr.len() {
        sum += tr[i];

        if i >= window {
            sum -= tr[i - window];
        }

        if i + 1 >= window {
            out[i] = Some(sum / window as f64);
        }
    }

    out
}

/// Average True Range using Wilder smoothing (exponential).
///
/// This is the "classic" ATR as originally defined by Welles Wilder:
/// - First ATR = SMA of first `window` TRs
/// - Subsequent: ATR[t] = ATR[t-1] * (window-1)/window + TR[t] / window
///
/// Wilder smoothing is equivalent to EMA with alpha = 1/window.
pub fn atr_wilder(bars: &[Bar], window: usize) -> Vec<Option<f64>> {
    if window == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let tr = true_range(bars);
    let mut out = vec![None; bars.len()];

    if bars.len() < window {
        return out;
    }

    // First ATR: SMA of first `window` TRs
    let initial_sum: f64 = tr[..window].iter().sum();
    let mut prev_atr = initial_sum / window as f64;
    out[window - 1] = Some(prev_atr);

    // Wilder smoothing for subsequent values
    let alpha = 1.0 / window as f64;
    for i in window..bars.len() {
        let atr_val = prev_atr * (1.0 - alpha) + tr[i] * alpha;
        out[i] = Some(atr_val);
        prev_atr = atr_val;
    }

    out
}

// =============================================================================
// Phase 1: ATR-Based Channel Indicators
// =============================================================================

/// Keltner Channel values (center, upper, and lower bands).
///
/// Keltner Channels use an EMA center line with ATR-based bands:
/// - Center = EMA(close, ema_period)
/// - Upper = Center + multiplier * ATR(atr_period)
/// - Lower = Center - multiplier * ATR(atr_period)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeltnerChannel {
    pub center: f64,
    pub upper: f64,
    pub lower: f64,
}

/// Compute Keltner Channel over the given periods.
///
/// Returns `None` until there are enough bars to compute both EMA and ATR.
pub fn keltner_channel(
    bars: &[Bar],
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
) -> Vec<Option<KeltnerChannel>> {
    if bars.is_empty() || ema_period == 0 || atr_period == 0 {
        return vec![None; bars.len()];
    }

    let ema_values = ema_close(bars, ema_period);
    let atr_values = atr(bars, atr_period);

    bars.iter()
        .enumerate()
        .map(|(i, _)| {
            match (
                ema_values.get(i).copied().flatten(),
                atr_values.get(i).copied().flatten(),
            ) {
                (Some(center), Some(atr_val)) => Some(KeltnerChannel {
                    center,
                    upper: center + multiplier * atr_val,
                    lower: center - multiplier * atr_val,
                }),
                _ => None,
            }
        })
        .collect()
}

/// STARC Bands values (center, upper, and lower bands).
///
/// STARC (Stoller Average Range Channel) Bands use an SMA center line with ATR-based bands:
/// - Center = SMA(close, sma_period)
/// - Upper = Center + multiplier * ATR(atr_period)
/// - Lower = Center - multiplier * ATR(atr_period)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct STARCBands {
    pub center: f64,
    pub upper: f64,
    pub lower: f64,
}

/// Compute STARC Bands over the given periods.
///
/// Returns `None` until there are enough bars to compute both SMA and ATR.
pub fn starc_bands(
    bars: &[Bar],
    sma_period: usize,
    atr_period: usize,
    multiplier: f64,
) -> Vec<Option<STARCBands>> {
    if bars.is_empty() || sma_period == 0 || atr_period == 0 {
        return vec![None; bars.len()];
    }

    let sma_values = sma_close(bars, sma_period);
    let atr_values = atr(bars, atr_period);

    bars.iter()
        .enumerate()
        .map(|(i, _)| {
            match (
                sma_values.get(i).copied().flatten(),
                atr_values.get(i).copied().flatten(),
            ) {
                (Some(center), Some(atr_val)) => Some(STARCBands {
                    center,
                    upper: center + multiplier * atr_val,
                    lower: center - multiplier * atr_val,
                }),
                _ => None,
            }
        })
        .collect()
}

/// Supertrend indicator value and state.
///
/// Supertrend is a trend-following indicator that uses ATR to determine trend direction:
/// - Basic band = (High + Low) / 2
/// - Upper band = Basic + multiplier * ATR
/// - Lower band = Basic - multiplier * ATR
/// - In uptrend: use lower band as support (trailing stop)
/// - In downtrend: use upper band as resistance
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SupertrendValue {
    /// The current supertrend line value (lower band in uptrend, upper in downtrend)
    pub supertrend: f64,
    /// True if in uptrend (price above supertrend), false if in downtrend
    pub is_uptrend: bool,
    /// The upper band value
    pub upper_band: f64,
    /// The lower band value
    pub lower_band: f64,
}

/// Compute Supertrend indicator.
///
/// The Supertrend indicator flips between using upper and lower bands based on price action:
/// - Uptrend continues while close > lower band (lower band rises but never falls)
/// - Downtrend continues while close < upper band (upper band falls but never rises)
/// - Trend flips when price crosses the current supertrend line
///
/// Returns `None` until there are enough bars for ATR calculation.
pub fn supertrend(
    bars: &[Bar],
    atr_period: usize,
    multiplier: f64,
) -> Vec<Option<SupertrendValue>> {
    if bars.is_empty() || atr_period == 0 {
        return vec![None; bars.len()];
    }

    let atr_values = atr(bars, atr_period);
    let mut out = vec![None; bars.len()];

    // Track state
    let mut prev_upper_band = f64::MAX;
    let mut prev_lower_band = f64::MIN;
    let mut prev_is_uptrend = true;
    let mut first_valid = true;

    for i in 0..bars.len() {
        let atr_val = match atr_values[i] {
            Some(v) => v,
            None => continue,
        };

        let basic = (bars[i].high + bars[i].low) / 2.0;
        let mut upper_band = basic + multiplier * atr_val;
        let mut lower_band = basic - multiplier * atr_val;

        // First valid bar - initialize state
        if first_valid {
            prev_upper_band = upper_band;
            prev_lower_band = lower_band;
            prev_is_uptrend = true;
            first_valid = false;

            out[i] = Some(SupertrendValue {
                supertrend: lower_band,
                is_uptrend: true,
                upper_band,
                lower_band,
            });
            continue;
        }

        let prev_close = bars[i - 1].close;

        // Upper band adjustment: can only decrease (in downtrend), never increase
        if !(upper_band > prev_upper_band || prev_close > prev_upper_band) {
            upper_band = prev_upper_band; // Keep previous (lower) value
        }

        // Lower band adjustment: can only increase (in uptrend), never decrease
        if !(lower_band < prev_lower_band || prev_close < prev_lower_band) {
            lower_band = prev_lower_band; // Keep previous (higher) value
        }

        // Determine trend direction
        let is_uptrend = if prev_is_uptrend {
            // Was in uptrend - stay in uptrend unless close goes below lower band
            bars[i].close >= lower_band
        } else {
            // Was in downtrend - flip to uptrend if close goes above upper band
            bars[i].close > upper_band
        };

        // Supertrend line
        let supertrend_val = if is_uptrend { lower_band } else { upper_band };

        out[i] = Some(SupertrendValue {
            supertrend: supertrend_val,
            is_uptrend,
            upper_band,
            lower_band,
        });

        // Update state for next iteration
        prev_upper_band = upper_band;
        prev_lower_band = lower_band;
        prev_is_uptrend = is_uptrend;
    }

    out
}

// =============================================================================
// Phase 3: Price Structure Indicators
// =============================================================================

/// Heikin-Ashi bar representation.
///
/// Heikin-Ashi candles smooth price action to make trend identification easier.
/// The formulas are:
/// - HA Close = (Open + High + Low + Close) / 4
/// - HA Open = (Previous HA Open + Previous HA Close) / 2
/// - HA High = max(High, HA Open, HA Close)
/// - HA Low = min(Low, HA Open, HA Close)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HABar {
    pub ha_open: f64,
    pub ha_high: f64,
    pub ha_low: f64,
    pub ha_close: f64,
}

impl HABar {
    /// Returns true if this is a bullish HA candle (close > open).
    pub fn is_bullish(&self) -> bool {
        self.ha_close > self.ha_open
    }

    /// Returns true if this is a bearish HA candle (close < open).
    pub fn is_bearish(&self) -> bool {
        self.ha_close < self.ha_open
    }

    /// Returns true if this is a strong bullish candle (no lower wick).
    ///
    /// A strong bullish candle has the low equal to the open (or close),
    /// indicating strong upward momentum with no selling pressure.
    pub fn is_strong_bullish(&self) -> bool {
        self.is_bullish() && (self.ha_low - self.ha_open.min(self.ha_close)).abs() < 1e-10
    }

    /// Returns true if this is a strong bearish candle (no upper wick).
    ///
    /// A strong bearish candle has the high equal to the open (or close),
    /// indicating strong downward momentum with no buying pressure.
    pub fn is_strong_bearish(&self) -> bool {
        self.is_bearish() && (self.ha_high - self.ha_open.max(self.ha_close)).abs() < 1e-10
    }

    /// Returns the body size (absolute difference between open and close).
    pub fn body_size(&self) -> f64 {
        (self.ha_close - self.ha_open).abs()
    }

    /// Returns the upper wick size.
    pub fn upper_wick(&self) -> f64 {
        self.ha_high - self.ha_open.max(self.ha_close)
    }

    /// Returns the lower wick size.
    pub fn lower_wick(&self) -> f64 {
        self.ha_open.min(self.ha_close) - self.ha_low
    }
}

/// Darvas Box state for tracking box formation.
///
/// Nicolas Darvas's box theory identifies consolidation ranges that,
/// when broken, signal strong trend continuation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DarvasBox {
    /// The top of the box (resistance level).
    pub top: f64,
    /// The bottom of the box (support level).
    pub bottom: f64,
    /// Whether the top has been confirmed.
    pub top_confirmed: bool,
    /// Whether the bottom has been confirmed.
    pub bottom_confirmed: bool,
    /// Bar index where box formation started.
    pub formation_started_at: usize,
}

impl DarvasBox {
    /// Returns true if both top and bottom are confirmed.
    pub fn is_complete(&self) -> bool {
        self.top_confirmed && self.bottom_confirmed
    }

    /// Returns the box height (top - bottom).
    pub fn height(&self) -> f64 {
        self.top - self.bottom
    }
}

/// 52-week high proximity indicator.
///
/// Tracks the rolling maximum close and the current price's proximity to it.
/// Based on George & Hwang (2004) "The 52-Week High and Momentum Investing".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighProximity {
    /// The period high (rolling maximum close).
    pub period_high: f64,
    /// The period low (rolling minimum close).
    pub period_low: f64,
    /// Current close as a percentage of period high (0.0 to 1.0+).
    pub proximity_pct: f64,
}

/// Compute rolling maximum of close prices over a lookback period.
///
/// Returns `None` until there are enough bars to fill the lookback.
/// The value at index `i` is the maximum close over bars `[i-period+1, i]` (inclusive).
pub fn rolling_max_close(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if i + 1 < period {
            continue;
        }

        let start = i + 1 - period;
        let max_val = bars[start..=i]
            .iter()
            .map(|b| b.close)
            .fold(f64::NEG_INFINITY, f64::max);
        out[i] = Some(max_val);
    }

    out
}

/// Compute rolling minimum of close prices over a lookback period.
///
/// Returns `None` until there are enough bars to fill the lookback.
/// The value at index `i` is the minimum close over bars `[i-period+1, i]` (inclusive).
pub fn rolling_min_close(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if i + 1 < period {
            continue;
        }

        let start = i + 1 - period;
        let min_val = bars[start..=i]
            .iter()
            .map(|b| b.close)
            .fold(f64::INFINITY, f64::min);
        out[i] = Some(min_val);
    }

    out
}

/// Compute rolling maximum of high prices over a lookback period.
///
/// Returns `None` until there are enough bars to fill the lookback.
pub fn rolling_max_high(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if i + 1 < period {
            continue;
        }

        let start = i + 1 - period;
        let max_val = bars[start..=i]
            .iter()
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max);
        out[i] = Some(max_val);
    }

    out
}

/// Compute rolling minimum of low prices over a lookback period.
///
/// Returns `None` until there are enough bars to fill the lookback.
pub fn rolling_min_low(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if i + 1 < period {
            continue;
        }

        let start = i + 1 - period;
        let min_val = bars[start..=i]
            .iter()
            .map(|b| b.low)
            .fold(f64::INFINITY, f64::min);
        out[i] = Some(min_val);
    }

    out
}

/// Compute 52-week (or N-period) high proximity for each bar.
///
/// Returns the rolling max/min and the current price's proximity to the high.
/// Useful for identifying stocks trading near their highs.
pub fn high_proximity(bars: &[Bar], period: usize) -> Vec<Option<HighProximity>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let max_closes = rolling_max_close(bars, period);
    let min_closes = rolling_min_close(bars, period);
    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if let (Some(period_high), Some(period_low)) = (max_closes[i], min_closes[i]) {
            let proximity_pct = if period_high > 0.0 {
                bars[i].close / period_high
            } else {
                0.0
            };
            out[i] = Some(HighProximity {
                period_high,
                period_low,
                proximity_pct,
            });
        }
    }

    out
}

/// Compute Heikin-Ashi bars from standard OHLC bars.
///
/// Heikin-Ashi candles smooth price action:
/// - HA Close = (O + H + L + C) / 4
/// - HA Open = (prev HA Open + prev HA Close) / 2
/// - HA High = max(H, HA Open, HA Close)
/// - HA Low = min(L, HA Open, HA Close)
///
/// For the first bar, HA Open = (O + C) / 2.
pub fn heikin_ashi(bars: &[Bar]) -> Vec<HABar> {
    if bars.is_empty() {
        return vec![];
    }

    let mut out = Vec::with_capacity(bars.len());

    // First bar: HA Open = (O + C) / 2
    let first = &bars[0];
    let ha_close = (first.open + first.high + first.low + first.close) / 4.0;
    let ha_open = (first.open + first.close) / 2.0;
    let ha_high = first.high.max(ha_open).max(ha_close);
    let ha_low = first.low.min(ha_open).min(ha_close);

    out.push(HABar {
        ha_open,
        ha_high,
        ha_low,
        ha_close,
    });

    // Subsequent bars
    for i in 1..bars.len() {
        let bar = &bars[i];
        let prev_ha = &out[i - 1];

        let ha_close = (bar.open + bar.high + bar.low + bar.close) / 4.0;
        let ha_open = (prev_ha.ha_open + prev_ha.ha_close) / 2.0;
        let ha_high = bar.high.max(ha_open).max(ha_close);
        let ha_low = bar.low.min(ha_open).min(ha_close);

        out.push(HABar {
            ha_open,
            ha_high,
            ha_low,
            ha_close,
        });
    }

    out
}

/// Compute Darvas boxes for each bar.
///
/// Box formation rules (Nicolas Darvas):
/// 1. New high made → potential box top
/// 2. Wait for `confirmation_bars` consecutive lower highs → top confirmed
/// 3. Find lowest low during confirmation period → potential box bottom
/// 4. Wait for `confirmation_bars` consecutive higher lows → bottom confirmed
/// 5. Box is complete when both are confirmed
///
/// Returns the current box state at each bar (if any).
pub fn darvas_boxes(bars: &[Bar], confirmation_bars: usize) -> Vec<Option<DarvasBox>> {
    if bars.is_empty() || confirmation_bars == 0 {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];
    let mut current_box: Option<DarvasBox> = None;
    let mut potential_top: Option<(f64, usize)> = None; // (high, bar_index)
    let mut lower_high_count = 0;
    let mut higher_low_count = 0;
    let mut lowest_during_top_confirm = f64::INFINITY;
    let mut prev_low = f64::INFINITY;

    for i in 0..bars.len() {
        let bar = &bars[i];

        // If we already have a complete box, check for breakout
        if let Some(ref bx) = current_box {
            if bx.is_complete() {
                // Check for breakout above box top (new high)
                if bar.high > bx.top {
                    // Start forming a new box with this as potential top
                    potential_top = Some((bar.high, i));
                    lower_high_count = 0;
                    higher_low_count = 0;
                    lowest_during_top_confirm = bar.low;
                    prev_low = bar.low;
                    current_box = None;
                }
                // Otherwise keep the current box
                out[i] = current_box;
                continue;
            }
        }

        // Box formation state machine
        match potential_top {
            None => {
                // Looking for a new high to start box formation
                // For simplicity, start with the first bar or when we make a new high
                if i == 0
                    || bar.high
                        > bars[..i]
                            .iter()
                            .map(|b| b.high)
                            .fold(f64::NEG_INFINITY, f64::max)
                {
                    potential_top = Some((bar.high, i));
                    lower_high_count = 0;
                    lowest_during_top_confirm = bar.low;
                    prev_low = bar.low;
                }
            }
            Some((top_high, top_idx)) => {
                if !current_box.as_ref().is_some_and(|b| b.top_confirmed) {
                    // Still confirming the top
                    if bar.high < top_high {
                        lower_high_count += 1;
                        lowest_during_top_confirm = lowest_during_top_confirm.min(bar.low);

                        if lower_high_count >= confirmation_bars {
                            // Top is confirmed
                            current_box = Some(DarvasBox {
                                top: top_high,
                                bottom: lowest_during_top_confirm,
                                top_confirmed: true,
                                bottom_confirmed: false,
                                formation_started_at: top_idx,
                            });
                            higher_low_count = 0;
                            prev_low = bar.low;
                        }
                    } else {
                        // New high made, reset the potential top
                        potential_top = Some((bar.high, i));
                        lower_high_count = 0;
                        lowest_during_top_confirm = bar.low;
                    }
                }
            }
        }

        // If we have a box with confirmed top, work on confirming bottom
        if let Some(ref mut bx) = current_box {
            if bx.top_confirmed && !bx.bottom_confirmed {
                if bar.low > prev_low {
                    higher_low_count += 1;

                    if higher_low_count >= confirmation_bars {
                        // Bottom is confirmed
                        bx.bottom_confirmed = true;
                    }
                } else if bar.low < bx.bottom {
                    // New low, update bottom and reset count
                    bx.bottom = bar.low;
                    higher_low_count = 0;
                }
                prev_low = bar.low;
            }
        }

        out[i] = current_box;
    }

    out
}

// =============================================================================
// Phase 4: Stateful Trend Indicators
// =============================================================================

/// Parabolic SAR state for each bar.
///
/// The Parabolic SAR (Stop And Reverse) is Wilder's classic trend-following indicator.
/// It provides potential entry and exit points by trailing below price in uptrends
/// and above price in downtrends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParabolicSAR {
    /// Current SAR value (the stop level).
    pub sar: f64,
    /// Acceleration factor (increases as trend continues).
    pub af: f64,
    /// Extreme point (highest high in uptrend, lowest low in downtrend).
    pub ep: f64,
    /// True if currently in an uptrend (SAR below price).
    pub is_uptrend: bool,
}

impl ParabolicSAR {
    /// Returns true if SAR just flipped from downtrend to uptrend.
    pub fn just_flipped_bullish(&self, prev: Option<&ParabolicSAR>) -> bool {
        match prev {
            Some(p) => self.is_uptrend && !p.is_uptrend,
            None => false,
        }
    }

    /// Returns true if SAR just flipped from uptrend to downtrend.
    pub fn just_flipped_bearish(&self, prev: Option<&ParabolicSAR>) -> bool {
        match prev {
            Some(p) => !self.is_uptrend && p.is_uptrend,
            None => false,
        }
    }
}

/// Compute Parabolic SAR for each bar using Wilder's algorithm.
///
/// **Parameters:**
/// - `af_start`: Initial acceleration factor (typically 0.02)
/// - `af_step`: Acceleration factor increment (typically 0.02)
/// - `af_max`: Maximum acceleration factor (typically 0.20)
///
/// **Algorithm:**
/// - In uptrend: SAR trails below price, moves up toward extreme point (highest high)
/// - In downtrend: SAR trails above price, moves down toward extreme point (lowest low)
/// - When price crosses SAR, trend flips and SAR resets to the extreme point
///
/// **Warmup:** Returns `None` for first 5 bars to establish initial trend direction.
pub fn parabolic_sar(
    bars: &[Bar],
    af_start: f64,
    af_step: f64,
    af_max: f64,
) -> Vec<Option<ParabolicSAR>> {
    const WARMUP: usize = 5;

    if bars.len() < WARMUP || af_start <= 0.0 || af_step <= 0.0 || af_max <= 0.0 {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    // Determine initial trend from warmup period
    let warmup_high = bars[..WARMUP]
        .iter()
        .map(|b| b.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let warmup_low = bars[..WARMUP]
        .iter()
        .map(|b| b.low)
        .fold(f64::INFINITY, f64::min);
    let first_close = bars[0].close;
    let last_warmup_close = bars[WARMUP - 1].close;

    // Start with uptrend if price moved up during warmup
    let mut is_uptrend = last_warmup_close >= first_close;

    // Initialize SAR and EP based on initial trend
    let mut sar = if is_uptrend { warmup_low } else { warmup_high };
    let mut ep = if is_uptrend { warmup_high } else { warmup_low };
    let mut af = af_start;

    // Store previous two bars' high/low for SAR clamping
    let mut prev_high = bars[WARMUP - 1].high;
    let mut prev_prev_high = bars[WARMUP - 2].high;
    let mut prev_low = bars[WARMUP - 1].low;
    let mut prev_prev_low = bars[WARMUP - 2].low;

    // Set warmup period values
    out[WARMUP - 1] = Some(ParabolicSAR {
        sar,
        af,
        ep,
        is_uptrend,
    });

    for i in WARMUP..bars.len() {
        let bar = &bars[i];

        if is_uptrend {
            // Calculate new SAR
            let mut new_sar = sar + af * (ep - sar);

            // SAR cannot go above the prior two lows (protection)
            new_sar = new_sar.min(prev_low).min(prev_prev_low);

            // Check for reversal: close crosses below SAR
            if bar.close < new_sar {
                // Flip to downtrend
                is_uptrend = false;
                new_sar = ep; // SAR becomes the extreme point
                ep = bar.low;
                af = af_start;
            } else {
                // Continue uptrend
                if bar.high > ep {
                    ep = bar.high;
                    af = (af + af_step).min(af_max);
                }
            }

            sar = new_sar;
        } else {
            // Downtrend
            // Calculate new SAR
            let mut new_sar = sar - af * (sar - ep);

            // SAR cannot go below the prior two highs (protection)
            new_sar = new_sar.max(prev_high).max(prev_prev_high);

            // Check for reversal: close crosses above SAR
            if bar.close > new_sar {
                // Flip to uptrend
                is_uptrend = true;
                new_sar = ep; // SAR becomes the extreme point
                ep = bar.high;
                af = af_start;
            } else {
                // Continue downtrend
                if bar.low < ep {
                    ep = bar.low;
                    af = (af + af_step).min(af_max);
                }
            }

            sar = new_sar;
        }

        out[i] = Some(ParabolicSAR {
            sar,
            af,
            ep,
            is_uptrend,
        });

        // Update prior highs/lows for next iteration
        prev_prev_high = prev_high;
        prev_prev_low = prev_low;
        prev_high = bar.high;
        prev_low = bar.low;
    }

    out
}

/// Opening period type for Opening Range Breakout strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OpeningPeriod {
    /// Weekly: Range is computed from the first N bars of each week.
    Weekly,
    /// Monthly: Range is computed from the first N bars of each month.
    Monthly,
    /// Rolling: Range is computed from the prior N bars (no calendar alignment).
    Rolling,
}

impl OpeningPeriod {
    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Rolling => "Rolling",
        }
    }
}

/// Opening Range state for each bar.
///
/// Used for Opening Range Breakout strategies adapted for daily bars.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpeningRange {
    /// The high of the opening range.
    pub range_high: f64,
    /// The low of the opening range.
    pub range_low: f64,
    /// True if the opening range is complete (N bars have passed).
    pub is_range_complete: bool,
    /// Number of bars included in the current range (0 = not in range yet).
    pub bars_in_range: usize,
}

impl OpeningRange {
    /// Returns the range size (high - low).
    pub fn range_size(&self) -> f64 {
        self.range_high - self.range_low
    }

    /// Returns true if close is above the range high (bullish breakout).
    pub fn is_breakout_high(&self, close: f64) -> bool {
        self.is_range_complete && close > self.range_high
    }

    /// Returns true if close is below the range low (bearish breakdown).
    pub fn is_breakout_low(&self, close: f64) -> bool {
        self.is_range_complete && close < self.range_low
    }
}

/// Compute Opening Range for each bar.
///
/// **Parameters:**
/// - `range_bars`: Number of bars that define the opening range
/// - `period`: How to determine when a new period starts (Weekly, Monthly, Rolling)
///
/// **For Weekly/Monthly periods:**
/// - Detects the first bar of each week/month based on timestamp
/// - Accumulates high/low of first N bars after period start
/// - After N bars, range is "complete" and breakout signals are valid
///
/// **For Rolling period:**
/// - Always uses the prior N bars as the "opening range"
/// - Range is always "complete" after initial warmup
///
/// **Warmup:** Returns `None` for first `range_bars` bars.
pub fn opening_range(
    bars: &[Bar],
    range_bars: usize,
    period: OpeningPeriod,
) -> Vec<Option<OpeningRange>> {
    if bars.is_empty() || range_bars == 0 {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    match period {
        OpeningPeriod::Rolling => {
            // Rolling: use prior N bars as the range
            for i in range_bars..bars.len() {
                let start = i - range_bars;
                let range_high = bars[start..i]
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let range_low = bars[start..i]
                    .iter()
                    .map(|b| b.low)
                    .fold(f64::INFINITY, f64::min);

                out[i] = Some(OpeningRange {
                    range_high,
                    range_low,
                    is_range_complete: true,
                    bars_in_range: range_bars,
                });
            }
        }
        OpeningPeriod::Weekly | OpeningPeriod::Monthly => {
            // Track the start of each period and accumulate range
            let mut period_start_idx: Option<usize> = None;
            let mut range_high = f64::NEG_INFINITY;
            let mut range_low = f64::INFINITY;
            let mut bars_in_range = 0usize;
            let mut prev_week = None;
            let mut prev_month = None;

            for i in 0..bars.len() {
                let bar = &bars[i];
                let ts = bar.ts;

                // Detect period change
                let is_new_period = match period {
                    OpeningPeriod::Weekly => {
                        let week = ts.iso_week().week();
                        let year = ts.iso_week().year();
                        let key = (year, week);
                        let is_new = prev_week != Some(key);
                        prev_week = Some(key);
                        is_new
                    }
                    OpeningPeriod::Monthly => {
                        let month = ts.month();
                        let year = ts.year();
                        let key = (year, month);
                        let is_new = prev_month != Some(key);
                        prev_month = Some(key);
                        is_new
                    }
                    OpeningPeriod::Rolling => unreachable!(),
                };

                if is_new_period {
                    // Start a new opening range
                    period_start_idx = Some(i);
                    range_high = bar.high;
                    range_low = bar.low;
                    bars_in_range = 1;
                } else if let Some(_start) = period_start_idx {
                    // Continue building the range
                    if bars_in_range < range_bars {
                        range_high = range_high.max(bar.high);
                        range_low = range_low.min(bar.low);
                        bars_in_range += 1;
                    }
                }

                // Output the current range state (only if we have a valid period)
                if period_start_idx.is_some() {
                    out[i] = Some(OpeningRange {
                        range_high,
                        range_low,
                        is_range_complete: bars_in_range >= range_bars,
                        bars_in_range,
                    });
                }
            }
        }
    }

    out
}

/// Compute prior day's range for Larry Williams volatility breakout.
///
/// Returns the range (high - low) of the previous bar for each bar.
/// First bar returns 0.0 (no prior bar).
pub fn prior_day_range(bars: &[Bar]) -> Vec<f64> {
    if bars.is_empty() {
        return vec![];
    }

    let mut out = Vec::with_capacity(bars.len());
    out.push(0.0); // First bar has no prior

    for i in 1..bars.len() {
        out.push(bars[i - 1].high - bars[i - 1].low);
    }

    out
}

/// Range breakout levels for Larry Williams strategy.
///
/// Computes upper and lower breakout levels based on:
/// - upper = open + multiplier * prior_range
/// - lower = open - multiplier * prior_range
///
/// Returns (upper_breakout, lower_breakout) for each bar.
pub fn range_breakout_levels(bars: &[Bar], multiplier: f64) -> Vec<(f64, f64)> {
    if bars.is_empty() {
        return vec![];
    }

    let ranges = prior_day_range(bars);
    let mut out = Vec::with_capacity(bars.len());

    for i in 0..bars.len() {
        let range_component = multiplier * ranges[i];
        let upper = bars[i].open + range_component;
        let lower = bars[i].open - range_component;
        out.push((upper, lower));
    }

    out
}

// =============================================================================
// Phase 2: Momentum & Direction Indicators
// =============================================================================

/// Bollinger Bands with bandwidth for squeeze detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BollingerBands {
    /// Middle band (SMA)
    pub middle: f64,
    /// Upper band (middle + std_mult * std)
    pub upper: f64,
    /// Lower band (middle - std_mult * std)
    pub lower: f64,
    /// Bandwidth = (upper - lower) / middle
    pub bandwidth: f64,
}

impl BollingerBands {
    /// Returns true if bandwidth is below threshold (squeeze condition).
    pub fn is_squeeze(&self, threshold: f64) -> bool {
        self.bandwidth < threshold
    }
}

/// Directional Movement Index (DMI) components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DMI {
    /// Positive directional indicator (+DI)
    pub plus_di: f64,
    /// Negative directional indicator (-DI)
    pub minus_di: f64,
    /// Average Directional Index (ADX)
    pub adx: f64,
}

impl DMI {
    /// Returns true if +DI > -DI (bullish directional movement).
    pub fn is_bullish(&self) -> bool {
        self.plus_di > self.minus_di
    }

    /// Returns true if ADX is above threshold (strong trend).
    pub fn is_trending(&self, threshold: f64) -> bool {
        self.adx > threshold
    }
}

/// Aroon indicator components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AroonIndicator {
    /// Aroon-Up: measures time since highest high
    pub aroon_up: f64,
    /// Aroon-Down: measures time since lowest low
    pub aroon_down: f64,
    /// Oscillator: aroon_up - aroon_down
    pub oscillator: f64,
}

impl AroonIndicator {
    /// Returns true if Aroon-Up > Aroon-Down (bullish).
    pub fn is_bullish(&self) -> bool {
        self.aroon_up > self.aroon_down
    }
}

/// Compute rolling standard deviation of close prices.
///
/// Uses population standard deviation formula:
/// std = sqrt(sum((close - mean)^2) / n)
///
/// Returns `None` until there are enough bars to fill the window.
pub fn rolling_std(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let sma = sma_close(bars, period);
    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if i + 1 < period {
            continue;
        }

        if let Some(mean) = sma[i] {
            let start = i + 1 - period;
            let variance: f64 = bars[start..=i]
                .iter()
                .map(|b| (b.close - mean).powi(2))
                .sum::<f64>()
                / period as f64;
            out[i] = Some(variance.sqrt());
        }
    }

    out
}

/// Compute Bollinger Bands for each bar.
///
/// - Middle = SMA(close, period)
/// - Upper = Middle + multiplier * std
/// - Lower = Middle - multiplier * std
/// - Bandwidth = (Upper - Lower) / Middle
///
/// Returns `None` until there are enough bars for the SMA.
pub fn bollinger_bands(
    bars: &[Bar],
    period: usize,
    multiplier: f64,
) -> Vec<Option<BollingerBands>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let sma = sma_close(bars, period);
    let std = rolling_std(bars, period);
    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if let (Some(middle), Some(s)) = (sma[i], std[i]) {
            let deviation = multiplier * s;
            let upper = middle + deviation;
            let lower = middle - deviation;
            let bandwidth = if middle > 0.0 {
                (upper - lower) / middle
            } else {
                0.0
            };
            out[i] = Some(BollingerBands {
                middle,
                upper,
                lower,
                bandwidth,
            });
        }
    }

    out
}

/// Compute raw positive directional movement (+DM) for each bar.
///
/// +DM = high - prev_high if positive AND > (prev_low - low), else 0
///
/// First bar returns 0.0 (no previous bar).
pub fn plus_dm(bars: &[Bar]) -> Vec<f64> {
    if bars.is_empty() {
        return vec![];
    }

    let mut out = Vec::with_capacity(bars.len());
    out.push(0.0); // First bar has no prior

    for i in 1..bars.len() {
        let up_move = bars[i].high - bars[i - 1].high;
        let down_move = bars[i - 1].low - bars[i].low;

        let dm = if up_move > down_move && up_move > 0.0 {
            up_move
        } else {
            0.0
        };
        out.push(dm);
    }

    out
}

/// Compute raw negative directional movement (-DM) for each bar.
///
/// -DM = prev_low - low if positive AND > (high - prev_high), else 0
///
/// First bar returns 0.0 (no previous bar).
pub fn minus_dm(bars: &[Bar]) -> Vec<f64> {
    if bars.is_empty() {
        return vec![];
    }

    let mut out = Vec::with_capacity(bars.len());
    out.push(0.0); // First bar has no prior

    for i in 1..bars.len() {
        let up_move = bars[i].high - bars[i - 1].high;
        let down_move = bars[i - 1].low - bars[i].low;

        let dm = if down_move > up_move && down_move > 0.0 {
            down_move
        } else {
            0.0
        };
        out.push(dm);
    }

    out
}

/// Apply Wilder smoothing to a series.
///
/// Wilder smoothing: first value = SMA of first `period` values
/// Subsequent: smoothed[i] = smoothed[i-1] * (period-1)/period + value[i] / period
fn wilder_smooth(values: &[f64], period: usize) -> Vec<Option<f64>> {
    if period == 0 || values.is_empty() {
        return vec![None; values.len()];
    }

    let mut out = vec![None; values.len()];

    if values.len() < period {
        return out;
    }

    // First smoothed value: SMA of first `period` values
    let initial_sum: f64 = values[..period].iter().sum();
    let mut prev_smooth = initial_sum / period as f64;
    out[period - 1] = Some(prev_smooth);

    // Wilder smoothing for subsequent values
    let alpha = 1.0 / period as f64;
    for i in period..values.len() {
        let smooth = prev_smooth * (1.0 - alpha) + values[i] * alpha;
        out[i] = Some(smooth);
        prev_smooth = smooth;
    }

    out
}

/// Compute positive directional indicator (+DI) for each bar.
///
/// +DI = 100 * wilder_smooth(+DM, period) / ATR(period)
///
/// Returns `None` until there are enough bars for the calculation.
pub fn plus_di(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let pdm = plus_dm(bars);
    let smoothed_pdm = wilder_smooth(&pdm, period);
    let atr_vals = atr_wilder(bars, period);
    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if let (Some(smooth_dm), Some(atr_val)) = (smoothed_pdm[i], atr_vals[i]) {
            if atr_val > 0.0 {
                out[i] = Some(100.0 * smooth_dm / atr_val);
            } else {
                out[i] = Some(0.0);
            }
        }
    }

    out
}

/// Compute negative directional indicator (-DI) for each bar.
///
/// -DI = 100 * wilder_smooth(-DM, period) / ATR(period)
///
/// Returns `None` until there are enough bars for the calculation.
pub fn minus_di(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let mdm = minus_dm(bars);
    let smoothed_mdm = wilder_smooth(&mdm, period);
    let atr_vals = atr_wilder(bars, period);
    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if let (Some(smooth_dm), Some(atr_val)) = (smoothed_mdm[i], atr_vals[i]) {
            if atr_val > 0.0 {
                out[i] = Some(100.0 * smooth_dm / atr_val);
            } else {
                out[i] = Some(0.0);
            }
        }
    }

    out
}

/// Compute Average Directional Index (ADX) for each bar.
///
/// ADX = wilder_smooth(DX, period)
/// where DX = 100 * |+DI - -DI| / (+DI + -DI)
///
/// Returns `None` until there are enough bars for the double smoothing.
/// Warmup period is approximately 2 * period.
pub fn adx(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let pdi = plus_di(bars, period);
    let mdi = minus_di(bars, period);

    // Compute DX values
    let mut dx_values = vec![0.0; bars.len()];
    for i in 0..bars.len() {
        if let (Some(p), Some(m)) = (pdi[i], mdi[i]) {
            let sum = p + m;
            if sum > 0.0 {
                dx_values[i] = 100.0 * (p - m).abs() / sum;
            }
        }
    }

    // Apply second Wilder smoothing to DX values
    // But we need to start from where DI values become valid
    let first_valid = (period - 1).min(bars.len().saturating_sub(1));

    // Shift DX values so Wilder smoothing starts from first valid DI
    let effective_dx: Vec<f64> = dx_values[first_valid..].to_vec();
    let smoothed_dx = wilder_smooth(&effective_dx, period);

    let mut out = vec![None; bars.len()];
    for (i, val) in smoothed_dx.iter().enumerate() {
        if let Some(v) = val {
            let target_idx = first_valid + i;
            if target_idx < bars.len() {
                out[target_idx] = Some(*v);
            }
        }
    }

    out
}

/// Compute combined DMI (Directional Movement Index) for each bar.
///
/// Returns +DI, -DI, and ADX as a single struct.
pub fn dmi(bars: &[Bar], period: usize) -> Vec<Option<DMI>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let pdi = plus_di(bars, period);
    let mdi = minus_di(bars, period);
    let adx_vals = adx(bars, period);
    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if let (Some(p), Some(m), Some(a)) = (pdi[i], mdi[i], adx_vals[i]) {
            out[i] = Some(DMI {
                plus_di: p,
                minus_di: m,
                adx: a,
            });
        }
    }

    out
}

/// Compute Aroon-Up indicator for each bar.
///
/// Aroon-Up = 100 * (period - bars_since_highest_high) / period
///
/// Returns `None` until there are enough bars for the period.
#[allow(clippy::needless_range_loop)]
pub fn aroon_up(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if i + 1 < period {
            continue;
        }

        // Find the index of highest high in the lookback window
        let start = i + 1 - period;
        let mut max_idx = start;
        let mut max_high = bars[start].high;

        for j in (start + 1)..=i {
            if bars[j].high >= max_high {
                max_high = bars[j].high;
                max_idx = j;
            }
        }

        let bars_since_high = i - max_idx;
        out[i] = Some(100.0 * (period - bars_since_high) as f64 / period as f64);
    }

    out
}

/// Compute Aroon-Down indicator for each bar.
///
/// Aroon-Down = 100 * (period - bars_since_lowest_low) / period
///
/// Returns `None` until there are enough bars for the period.
#[allow(clippy::needless_range_loop)]
pub fn aroon_down(bars: &[Bar], period: usize) -> Vec<Option<f64>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if i + 1 < period {
            continue;
        }

        // Find the index of lowest low in the lookback window
        let start = i + 1 - period;
        let mut min_idx = start;
        let mut min_low = bars[start].low;

        for j in (start + 1)..=i {
            if bars[j].low <= min_low {
                min_low = bars[j].low;
                min_idx = j;
            }
        }

        let bars_since_low = i - min_idx;
        out[i] = Some(100.0 * (period - bars_since_low) as f64 / period as f64);
    }

    out
}

/// Compute combined Aroon indicator for each bar.
///
/// Returns Aroon-Up, Aroon-Down, and the oscillator (up - down).
pub fn aroon(bars: &[Bar], period: usize) -> Vec<Option<AroonIndicator>> {
    if period == 0 || bars.is_empty() {
        return vec![None; bars.len()];
    }

    let up = aroon_up(bars, period);
    let down = aroon_down(bars, period);
    let mut out = vec![None; bars.len()];

    for i in 0..bars.len() {
        if let (Some(u), Some(d)) = (up[i], down[i]) {
            out[i] = Some(AroonIndicator {
                aroon_up: u,
                aroon_down: d,
                oscillator: u - d,
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn bars_from_closes(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let ts = chrono::Utc
                    .with_ymd_and_hms(2024, 1, 1 + i as u32, 0, 0, 0)
                    .unwrap();
                Bar::new(ts, c, c, c, c, 0.0, "TEST", "1d")
            })
            .collect()
    }

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
    fn sma_window_3_matches_definition() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0, 4.0]);
        let sma = sma_close(&bars, 3);
        assert_eq!(sma, vec![None, None, Some(2.0), Some(3.0)]);
    }

    #[test]
    fn donchian_warmup_period() {
        // With lookback 5, indices 0-4 should be None
        let ohlc = vec![
            (100.0, 102.0, 98.0, 101.0),
            (101.0, 103.0, 99.0, 102.0),
            (102.0, 104.0, 100.0, 103.0),
            (103.0, 105.0, 101.0, 104.0),
            (104.0, 106.0, 102.0, 105.0),
            (105.0, 107.0, 103.0, 104.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let dc = donchian_channel(&bars, 5);

        // Indices 0-4 should be None (warmup)
        assert!(dc[0].is_none());
        assert!(dc[1].is_none());
        assert!(dc[2].is_none());
        assert!(dc[3].is_none());
        assert!(dc[4].is_none());

        // Index 5 should have values from indices 0-4
        let ch = dc[5].unwrap();
        assert_eq!(ch.upper, 106.0); // max of highs [102, 103, 104, 105, 106]
        assert_eq!(ch.lower, 98.0); // min of lows [98, 99, 100, 101, 102]
    }

    #[test]
    fn donchian_no_lookahead() {
        let ohlc = vec![
            (100.0, 102.0, 98.0, 101.0),
            (101.0, 103.0, 99.0, 102.0),
            (102.0, 104.0, 100.0, 103.0),
            (103.0, 200.0, 50.0, 104.0), // Extreme values after lookback
        ];
        let bars = bars_from_ohlc(&ohlc);
        let dc = donchian_channel(&bars, 2);

        // Index 2: looks at bars 0-1, should not see bar 3's extreme values
        let ch = dc[2].unwrap();
        assert_eq!(ch.upper, 103.0); // max(102, 103)
        assert_eq!(ch.lower, 98.0); // min(98, 99)

        // Index 3: looks at bars 1-2, still shouldn't see its own bar
        let ch = dc[3].unwrap();
        assert_eq!(ch.upper, 104.0); // max(103, 104)
        assert_eq!(ch.lower, 99.0); // min(99, 100)
    }

    #[test]
    fn ema_window_3_seeded_with_sma() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0, 4.0]);
        let ema = ema_close(&bars, 3);

        // First two values are None (warmup)
        assert!(ema[0].is_none());
        assert!(ema[1].is_none());

        // Index 2: seeded with SMA = (1+2+3)/3 = 2.0
        assert!((ema[2].unwrap() - 2.0).abs() < 1e-10);

        // Index 3: EMA = 4 * (2/4) + 2.0 * (2/4) = 2 + 1 = 3.0
        // k = 2/(3+1) = 0.5
        let expected = 4.0 * 0.5 + 2.0 * 0.5;
        assert!((ema[3].unwrap() - expected).abs() < 1e-10);
    }

    #[test]
    fn ema_responds_faster_than_sma() {
        // EMA should respond faster to price changes
        let bars = bars_from_closes(&[100.0, 100.0, 100.0, 100.0, 100.0, 150.0]);
        let sma = sma_close(&bars, 5);
        let ema = ema_close(&bars, 5);

        // At index 5 (after the jump to 150):
        // SMA = (100+100+100+100+150)/5 = 110
        // EMA should be higher because it weights recent prices more
        let sma_val = sma[5].unwrap();
        let ema_val = ema[5].unwrap();

        assert_eq!(sma_val, 110.0);
        assert!(ema_val > sma_val, "EMA should respond faster to price jump");
    }

    #[test]
    fn ema_empty_bars() {
        let bars: Vec<Bar> = vec![];
        let ema = ema_close(&bars, 5);
        assert!(ema.is_empty());
    }

    #[test]
    fn ema_window_zero() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0]);
        let ema = ema_close(&bars, 0);
        assert_eq!(ema, vec![None, None, None]);
    }

    #[test]
    fn true_range_basic() {
        // Simple case: no gaps, TR = high - low
        let ohlc = vec![
            (100.0, 105.0, 95.0, 102.0),  // TR = 10
            (102.0, 108.0, 100.0, 106.0), // TR = max(8, |108-102|=6, |100-102|=2) = 8
        ];
        let bars = bars_from_ohlc(&ohlc);
        let tr = true_range(&bars);

        assert_eq!(tr.len(), 2);
        assert!((tr[0] - 10.0).abs() < 1e-10);
        assert!((tr[1] - 8.0).abs() < 1e-10);
    }

    #[test]
    fn true_range_with_gap_up() {
        // Gap up: previous close at 100, next bar opens at 110
        let ohlc = vec![
            (98.0, 102.0, 96.0, 100.0),   // TR = 6
            (110.0, 115.0, 108.0, 112.0), // TR = max(7, |115-100|=15, |108-100|=8) = 15
        ];
        let bars = bars_from_ohlc(&ohlc);
        let tr = true_range(&bars);

        assert!((tr[0] - 6.0).abs() < 1e-10);
        assert!((tr[1] - 15.0).abs() < 1e-10); // Gap dominates
    }

    #[test]
    fn true_range_with_gap_down() {
        // Gap down: previous close at 100, next bar opens at 90
        let ohlc = vec![
            (98.0, 102.0, 96.0, 100.0), // TR = 6
            (90.0, 95.0, 88.0, 92.0),   // TR = max(7, |95-100|=5, |88-100|=12) = 12
        ];
        let bars = bars_from_ohlc(&ohlc);
        let tr = true_range(&bars);

        assert!((tr[0] - 6.0).abs() < 1e-10);
        assert!((tr[1] - 12.0).abs() < 1e-10); // Gap down dominates
    }

    #[test]
    fn atr_sma_window_3() {
        // ATR should be SMA of true ranges
        let ohlc = vec![
            (100.0, 104.0, 98.0, 102.0),  // TR = 6
            (102.0, 106.0, 100.0, 104.0), // TR = 6
            (104.0, 108.0, 102.0, 106.0), // TR = 6
            (106.0, 110.0, 104.0, 108.0), // TR = 6
        ];
        let bars = bars_from_ohlc(&ohlc);
        let atr_vals = atr(&bars, 3);

        // Warmup: indices 0, 1 are None
        assert!(atr_vals[0].is_none());
        assert!(atr_vals[1].is_none());

        // Index 2: ATR = avg(6, 6, 6) = 6
        assert!((atr_vals[2].unwrap() - 6.0).abs() < 1e-10);

        // Index 3: ATR = avg(6, 6, 6) = 6
        assert!((atr_vals[3].unwrap() - 6.0).abs() < 1e-10);
    }

    #[test]
    fn atr_wilder_matches_formula() {
        // With varying TRs, Wilder smoothing should work
        let ohlc = vec![
            (100.0, 106.0, 98.0, 102.0),  // TR = 8
            (102.0, 108.0, 100.0, 104.0), // TR = 8
            (104.0, 110.0, 102.0, 106.0), // TR = 8
            (106.0, 120.0, 104.0, 118.0), // TR = 16 (big move)
        ];
        let bars = bars_from_ohlc(&ohlc);
        let atr_w = atr_wilder(&bars, 3);

        // Index 2: initial = avg(8, 8, 8) = 8
        assert!((atr_w[2].unwrap() - 8.0).abs() < 1e-10);

        // Index 3: ATR = 8 * (2/3) + 16 * (1/3) = 5.33 + 5.33 = 10.67
        let expected = 8.0 * (2.0 / 3.0) + 16.0 * (1.0 / 3.0);
        assert!((atr_w[3].unwrap() - expected).abs() < 1e-10);
    }

    #[test]
    fn atr_empty_bars() {
        let bars: Vec<Bar> = vec![];
        let tr = true_range(&bars);
        let atr_vals = atr(&bars, 14);

        assert!(tr.is_empty());
        assert!(atr_vals.is_empty());
    }

    // =============================================================================
    // Phase 3: Price Structure Indicator Tests
    // =============================================================================

    #[test]
    fn rolling_max_close_basic() {
        let bars = bars_from_closes(&[10.0, 12.0, 8.0, 15.0, 11.0]);
        let max_vals = rolling_max_close(&bars, 3);

        // First 2 indices are None (warmup)
        assert!(max_vals[0].is_none());
        assert!(max_vals[1].is_none());

        // Index 2: max of [10, 12, 8] = 12
        assert!((max_vals[2].unwrap() - 12.0).abs() < 1e-10);

        // Index 3: max of [12, 8, 15] = 15
        assert!((max_vals[3].unwrap() - 15.0).abs() < 1e-10);

        // Index 4: max of [8, 15, 11] = 15
        assert!((max_vals[4].unwrap() - 15.0).abs() < 1e-10);
    }

    #[test]
    fn rolling_min_close_basic() {
        let bars = bars_from_closes(&[10.0, 12.0, 8.0, 15.0, 11.0]);
        let min_vals = rolling_min_close(&bars, 3);

        // First 2 indices are None (warmup)
        assert!(min_vals[0].is_none());
        assert!(min_vals[1].is_none());

        // Index 2: min of [10, 12, 8] = 8
        assert!((min_vals[2].unwrap() - 8.0).abs() < 1e-10);

        // Index 3: min of [12, 8, 15] = 8
        assert!((min_vals[3].unwrap() - 8.0).abs() < 1e-10);

        // Index 4: min of [8, 15, 11] = 8
        assert!((min_vals[4].unwrap() - 8.0).abs() < 1e-10);
    }

    #[test]
    fn high_proximity_basic() {
        let bars = bars_from_closes(&[100.0, 110.0, 105.0]);
        let prox = high_proximity(&bars, 3);

        // First 2 are None
        assert!(prox[0].is_none());
        assert!(prox[1].is_none());

        // Index 2: high = 110, close = 105, proximity = 105/110 ≈ 0.954545
        let hp = prox[2].unwrap();
        assert!((hp.period_high - 110.0).abs() < 1e-10);
        assert!((hp.period_low - 100.0).abs() < 1e-10);
        assert!((hp.proximity_pct - (105.0 / 110.0)).abs() < 1e-10);
    }

    #[test]
    fn heikin_ashi_first_bar() {
        // First bar: O=100, H=110, L=95, C=105
        let ohlc = vec![(100.0, 110.0, 95.0, 105.0)];
        let bars = bars_from_ohlc(&ohlc);
        let ha = heikin_ashi(&bars);

        assert_eq!(ha.len(), 1);

        // HA Close = (100+110+95+105)/4 = 102.5
        assert!((ha[0].ha_close - 102.5).abs() < 1e-10);

        // HA Open = (100+105)/2 = 102.5
        assert!((ha[0].ha_open - 102.5).abs() < 1e-10);

        // HA High = max(110, 102.5, 102.5) = 110
        assert!((ha[0].ha_high - 110.0).abs() < 1e-10);

        // HA Low = min(95, 102.5, 102.5) = 95
        assert!((ha[0].ha_low - 95.0).abs() < 1e-10);
    }

    #[test]
    fn heikin_ashi_subsequent_bars() {
        let ohlc = vec![
            (100.0, 110.0, 95.0, 105.0), // HA: O=102.5, C=102.5, H=110, L=95
            (106.0, 115.0, 103.0, 112.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let ha = heikin_ashi(&bars);

        assert_eq!(ha.len(), 2);

        // Second bar:
        // HA Close = (106+115+103+112)/4 = 109
        assert!((ha[1].ha_close - 109.0).abs() < 1e-10);

        // HA Open = (102.5+102.5)/2 = 102.5
        assert!((ha[1].ha_open - 102.5).abs() < 1e-10);

        // HA High = max(115, 102.5, 109) = 115
        assert!((ha[1].ha_high - 115.0).abs() < 1e-10);

        // HA Low = min(103, 102.5, 109) = 102.5
        assert!((ha[1].ha_low - 102.5).abs() < 1e-10);
    }

    #[test]
    fn heikin_ashi_bullish_detection() {
        let ohlc = vec![(100.0, 110.0, 98.0, 108.0)];
        let bars = bars_from_ohlc(&ohlc);
        let ha = heikin_ashi(&bars);

        // HA Close = (100+110+98+108)/4 = 104
        // HA Open = (100+108)/2 = 104
        // Since ha_close == ha_open, neither bullish nor bearish
        assert!(!ha[0].is_bullish());
        assert!(!ha[0].is_bearish());

        // Create a clear bullish candle
        let ohlc2 = vec![(100.0, 120.0, 99.0, 118.0)];
        let bars2 = bars_from_ohlc(&ohlc2);
        let ha2 = heikin_ashi(&bars2);

        // HA Close = (100+120+99+118)/4 = 109.25
        // HA Open = (100+118)/2 = 109
        // 109.25 > 109 → bullish
        assert!(ha2[0].is_bullish());
    }

    #[test]
    fn prior_day_range_basic() {
        let ohlc = vec![
            (100.0, 105.0, 95.0, 102.0),  // range = 10
            (102.0, 110.0, 100.0, 108.0), // range = 10
            (108.0, 112.0, 106.0, 110.0), // range = 6
        ];
        let bars = bars_from_ohlc(&ohlc);
        let ranges = prior_day_range(&bars);

        assert_eq!(ranges.len(), 3);
        assert!((ranges[0] - 0.0).abs() < 1e-10); // First bar has no prior
        assert!((ranges[1] - 10.0).abs() < 1e-10); // Prior bar range
        assert!((ranges[2] - 10.0).abs() < 1e-10); // Prior bar range
    }

    #[test]
    fn range_breakout_levels_basic() {
        let ohlc = vec![
            (100.0, 110.0, 90.0, 105.0), // range = 20
            (105.0, 115.0, 100.0, 110.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let levels = range_breakout_levels(&bars, 0.5);

        // First bar: prior range = 0, so upper/lower = open
        assert!((levels[0].0 - 100.0).abs() < 1e-10);
        assert!((levels[0].1 - 100.0).abs() < 1e-10);

        // Second bar: prior range = 20, multiplier = 0.5
        // upper = 105 + 0.5*20 = 115
        // lower = 105 - 0.5*20 = 95
        assert!((levels[1].0 - 115.0).abs() < 1e-10);
        assert!((levels[1].1 - 95.0).abs() < 1e-10);
    }

    #[test]
    fn darvas_boxes_empty() {
        let bars: Vec<Bar> = vec![];
        let boxes = darvas_boxes(&bars, 3);
        assert!(boxes.is_empty());
    }

    #[test]
    fn ha_bar_strong_bullish() {
        // A strong bullish HA bar has no lower wick
        // This means ha_low == min(ha_open, ha_close)
        let ha = HABar {
            ha_open: 100.0,
            ha_high: 110.0,
            ha_low: 100.0,   // No lower wick (equals open which is min)
            ha_close: 108.0, // close > open = bullish
        };

        assert!(ha.is_bullish());
        assert!(ha.is_strong_bullish());
    }

    #[test]
    fn ha_bar_strong_bearish() {
        // A strong bearish HA bar has no upper wick
        // This means ha_high == max(ha_open, ha_close)
        let ha = HABar {
            ha_open: 108.0,
            ha_high: 108.0, // No upper wick (equals open which is max)
            ha_low: 95.0,
            ha_close: 100.0, // close < open = bearish
        };

        assert!(ha.is_bearish());
        assert!(ha.is_strong_bearish());
    }

    #[test]
    fn ha_bar_body_and_wick_sizes() {
        let ha = HABar {
            ha_open: 100.0,
            ha_high: 112.0,
            ha_low: 95.0,
            ha_close: 108.0,
        };

        // Body = |108 - 100| = 8
        assert!((ha.body_size() - 8.0).abs() < 1e-10);

        // Upper wick = 112 - max(100, 108) = 112 - 108 = 4
        assert!((ha.upper_wick() - 4.0).abs() < 1e-10);

        // Lower wick = min(100, 108) - 95 = 100 - 95 = 5
        assert!((ha.lower_wick() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn darvas_box_height() {
        let bx = DarvasBox {
            top: 110.0,
            bottom: 95.0,
            top_confirmed: true,
            bottom_confirmed: true,
            formation_started_at: 0,
        };

        assert!((bx.height() - 15.0).abs() < 1e-10);
        assert!(bx.is_complete());
    }

    // =============================================================================
    // Phase 4: Parabolic SAR and Opening Range Tests
    // =============================================================================

    #[test]
    fn parabolic_sar_warmup_period() {
        // SAR needs 5 bars for warmup
        let ohlc = vec![
            (100.0, 102.0, 98.0, 101.0),
            (101.0, 103.0, 99.0, 102.0),
            (102.0, 104.0, 100.0, 103.0),
            (103.0, 105.0, 101.0, 104.0),
            (104.0, 106.0, 102.0, 105.0),
            (105.0, 107.0, 103.0, 106.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let sar = parabolic_sar(&bars, 0.02, 0.02, 0.20);

        // First 4 indices should be None (warmup)
        assert!(sar[0].is_none());
        assert!(sar[1].is_none());
        assert!(sar[2].is_none());
        assert!(sar[3].is_none());

        // Index 4 onwards should have values
        assert!(sar[4].is_some());
        assert!(sar[5].is_some());
    }

    #[test]
    fn parabolic_sar_uptrend_trails_below_price() {
        // Create a clear uptrend
        let ohlc = vec![
            (100.0, 102.0, 98.0, 101.0),
            (101.0, 103.0, 99.0, 102.0),
            (102.0, 104.0, 100.0, 103.0),
            (103.0, 105.0, 101.0, 104.0),
            (104.0, 106.0, 102.0, 105.0),
            (105.0, 107.0, 103.0, 106.0),
            (106.0, 108.0, 104.0, 107.0),
            (107.0, 109.0, 105.0, 108.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let sar = parabolic_sar(&bars, 0.02, 0.02, 0.20);

        // In uptrend, SAR should be below the close
        for i in 4..bars.len() {
            let sar_val = sar[i].unwrap();
            assert!(sar_val.is_uptrend, "Should be in uptrend at index {}", i);
            assert!(
                sar_val.sar < bars[i].close,
                "SAR {} should be below close {} at index {}",
                sar_val.sar,
                bars[i].close,
                i
            );
        }
    }

    #[test]
    fn parabolic_sar_af_increases_with_new_highs() {
        // Create a strong uptrend making new highs
        let ohlc = vec![
            (100.0, 102.0, 98.0, 101.0),
            (101.0, 104.0, 100.0, 103.0),
            (103.0, 106.0, 102.0, 105.0),
            (105.0, 108.0, 104.0, 107.0),
            (107.0, 110.0, 106.0, 109.0),
            (109.0, 112.0, 108.0, 111.0),
            (111.0, 114.0, 110.0, 113.0),
            (113.0, 116.0, 112.0, 115.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let sar = parabolic_sar(&bars, 0.02, 0.02, 0.20);

        // AF should increase as new highs are made
        let af_at_5 = sar[5].unwrap().af;
        let af_at_7 = sar[7].unwrap().af;

        assert!(
            af_at_7 > af_at_5,
            "AF should increase: {} > {}",
            af_at_7,
            af_at_5
        );
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn parabolic_sar_af_respects_maximum() {
        // Create a very strong uptrend to hit AF max
        let ohlc: Vec<(f64, f64, f64, f64)> = (0..20)
            .map(|i| {
                let base = 100.0 + i as f64 * 3.0;
                (base, base + 3.0, base - 1.0, base + 2.0)
            })
            .collect();
        let bars = bars_from_ohlc(&ohlc);
        let sar = parabolic_sar(&bars, 0.02, 0.02, 0.20);

        // After many new highs, AF should be capped at 0.20
        for i in 10..bars.len() {
            if let Some(sar_val) = sar[i] {
                assert!(
                    sar_val.af <= 0.20 + 1e-10,
                    "AF {} should not exceed max 0.20 at index {}",
                    sar_val.af,
                    i
                );
            }
        }
    }

    #[test]
    fn parabolic_sar_flip_detection() {
        // Create uptrend followed by reversal
        let ohlc = vec![
            (100.0, 102.0, 98.0, 101.0),
            (101.0, 103.0, 99.0, 102.0),
            (102.0, 104.0, 100.0, 103.0),
            (103.0, 105.0, 101.0, 104.0),
            (104.0, 106.0, 102.0, 105.0),
            (105.0, 107.0, 103.0, 106.0),
            (106.0, 108.0, 104.0, 107.0),
            // Reversal begins
            (105.0, 106.0, 100.0, 101.0),
            (101.0, 102.0, 95.0, 96.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let sar = parabolic_sar(&bars, 0.02, 0.02, 0.20);

        // Check flip detection methods
        let prev = sar[7].as_ref();
        let curr = sar[8].as_ref();

        if let (Some(p), Some(c)) = (prev, curr) {
            if p.is_uptrend && !c.is_uptrend {
                assert!(c.just_flipped_bearish(Some(p)));
                assert!(!c.just_flipped_bullish(Some(p)));
            }
        }
    }

    #[test]
    fn parabolic_sar_empty_and_invalid() {
        let bars: Vec<Bar> = vec![];
        let sar = parabolic_sar(&bars, 0.02, 0.02, 0.20);
        assert!(sar.is_empty());

        // Invalid AF parameters
        let ohlc = vec![(100.0, 102.0, 98.0, 101.0); 10];
        let bars = bars_from_ohlc(&ohlc);
        let sar = parabolic_sar(&bars, 0.0, 0.02, 0.20);
        assert!(sar.iter().all(|s| s.is_none()));
    }

    #[test]
    fn opening_range_rolling_basic() {
        let ohlc = vec![
            (100.0, 105.0, 95.0, 102.0),
            (102.0, 108.0, 100.0, 106.0),
            (106.0, 112.0, 104.0, 110.0),
            (110.0, 115.0, 108.0, 112.0),
            (112.0, 118.0, 110.0, 116.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let orb = opening_range(&bars, 3, OpeningPeriod::Rolling);

        // First 3 bars are None (warmup)
        assert!(orb[0].is_none());
        assert!(orb[1].is_none());
        assert!(orb[2].is_none());

        // Index 3: range of bars 0-2
        let range = orb[3].unwrap();
        assert!((range.range_high - 112.0).abs() < 1e-10);
        assert!((range.range_low - 95.0).abs() < 1e-10);
        assert!(range.is_range_complete);
        assert_eq!(range.bars_in_range, 3);

        // Index 4: range of bars 1-3
        let range = orb[4].unwrap();
        assert!((range.range_high - 115.0).abs() < 1e-10);
        assert!((range.range_low - 100.0).abs() < 1e-10);
    }

    #[test]
    fn opening_range_breakout_detection() {
        let ohlc = vec![
            (100.0, 105.0, 95.0, 102.0),
            (102.0, 108.0, 100.0, 106.0),
            (106.0, 110.0, 104.0, 108.0),
            (108.0, 120.0, 106.0, 118.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let orb = opening_range(&bars, 3, OpeningPeriod::Rolling);

        // Index 3: close 118 > range_high 110 = breakout
        let range = orb[3].unwrap();
        assert!(range.is_breakout_high(bars[3].close));
        assert!(!range.is_breakout_low(bars[3].close));
    }

    #[test]
    fn opening_range_weekly_period() {
        let mut bars = Vec::new();

        // Week 1: Mon-Fri (Jan 1-5, 2024)
        for day in 1..=5 {
            let ts = chrono::Utc.with_ymd_and_hms(2024, 1, day, 0, 0, 0).unwrap();
            let price = 100.0 + day as f64;
            bars.push(Bar::new(
                ts,
                price,
                price + 2.0,
                price - 1.0,
                price + 1.0,
                0.0,
                "TEST",
                "1d",
            ));
        }

        // Week 2: Mon-Wed (Jan 8-10)
        for day in 8..=10 {
            let ts = chrono::Utc.with_ymd_and_hms(2024, 1, day, 0, 0, 0).unwrap();
            let price = 110.0 + (day - 8) as f64;
            bars.push(Bar::new(
                ts,
                price,
                price + 2.0,
                price - 1.0,
                price + 1.0,
                0.0,
                "TEST",
                "1d",
            ));
        }

        let orb = opening_range(&bars, 3, OpeningPeriod::Weekly);

        // After 3 bars of week 1, range should be complete
        let range_day3 = orb[2].unwrap();
        assert!(range_day3.is_range_complete);
        assert_eq!(range_day3.bars_in_range, 3);

        // At start of week 2, new range starts
        let range_week2_start = orb[5].unwrap();
        assert_eq!(range_week2_start.bars_in_range, 1);
        assert!(!range_week2_start.is_range_complete);
    }

    #[test]
    fn opening_range_empty() {
        let bars: Vec<Bar> = vec![];
        let orb = opening_range(&bars, 3, OpeningPeriod::Rolling);
        assert!(orb.is_empty());

        // Zero range_bars
        let ohlc = vec![(100.0, 102.0, 98.0, 101.0); 5];
        let bars = bars_from_ohlc(&ohlc);
        let orb = opening_range(&bars, 0, OpeningPeriod::Rolling);
        assert!(orb.iter().all(|r| r.is_none()));
    }

    #[test]
    fn opening_range_size_calculation() {
        let range = OpeningRange {
            range_high: 110.0,
            range_low: 95.0,
            is_range_complete: true,
            bars_in_range: 5,
        };

        assert!((range.range_size() - 15.0).abs() < 1e-10);
    }
}
