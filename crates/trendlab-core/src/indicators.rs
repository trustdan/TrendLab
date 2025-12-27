//! Indicator calculations (pure functions, no IO).
//!
//! Key invariant: indicator values at index `t` must depend only on bars `0..=t`.

use crate::bar::Bar;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MAType {
    SMA,
    EMA,
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
}
