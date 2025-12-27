//! Polars expression-based indicators for vectorized computation.
//!
//! These functions return Polars `Expr` types that can be composed in LazyFrame pipelines.
//! They provide the same semantics as the sequential indicators in `indicators.rs`.
//!
//! Key invariant: indicator values at index `t` must depend only on bars `0..=t`.

use polars::prelude::*;

/// Donchian channel as Polars expressions.
///
/// Donchian channel is defined as:
/// - Upper = highest high over the prior N bars (NOT including current bar)
/// - Lower = lowest low over the prior N bars (NOT including current bar)
///
/// Returns `(upper_expr, lower_expr)` that can be added to a LazyFrame.
/// Results are null during warmup period (first `lookback` bars).
pub fn donchian_channel_exprs(lookback: usize) -> (Expr, Expr) {
    // Shift by 1 to exclude current bar, then take rolling max/min
    // The rolling window looks at the lookback bars ending at the shifted position
    let upper = col("high")
        .shift(lit(1)) // Exclude current bar
        .rolling_max(RollingOptionsFixedWindow {
            window_size: lookback,
            min_periods: lookback,
            weights: None,
            center: false,
            fn_params: None,
        })
        .alias("dc_upper");

    let lower = col("low")
        .shift(lit(1)) // Exclude current bar
        .rolling_min(RollingOptionsFixedWindow {
            window_size: lookback,
            min_periods: lookback,
            weights: None,
            center: false,
            fn_params: None,
        })
        .alias("dc_lower");

    (upper, lower)
}

/// Simple moving average of close price as a Polars expression.
///
/// Returns null until there are enough bars to fill the window.
pub fn sma_close_expr(window: usize) -> Expr {
    col("close").rolling_mean(RollingOptionsFixedWindow {
        window_size: window,
        min_periods: window,
        weights: None,
        center: false,
        fn_params: None,
    })
}

/// Simple moving average with custom alias.
pub fn sma_close_expr_aliased(window: usize, alias: &str) -> Expr {
    sma_close_expr(window).alias(alias)
}

/// Exponential moving average of close price as a Polars expression.
///
/// Uses the standard EMA formula:
/// - Multiplier (k) = 2 / (window + 1)
/// - EMA[t] = close[t] * k + EMA[t-1] * (1 - k)
///
/// Alpha = 2/(span+1) where span = window.
pub fn ema_close_expr(window: usize) -> Expr {
    // alpha = 2 / (span + 1)
    let alpha = 2.0 / (window as f64 + 1.0);
    col("close").ewm_mean(EWMOptions {
        alpha,
        adjust: true,
        bias: false,
        min_periods: window,
        ignore_nulls: true,
    })
}

/// Exponential moving average with custom alias.
pub fn ema_close_expr_aliased(window: usize, alias: &str) -> Expr {
    ema_close_expr(window).alias(alias)
}

/// True Range as a Polars expression.
///
/// True Range is defined as the maximum of:
/// - Current high - current low
/// - |current high - previous close|
/// - |current low - previous close|
///
/// For the first bar, only (high - low) is used since there's no previous close.
pub fn true_range_expr() -> Expr {
    let hl = col("high") - col("low");
    let prev_close = col("close").shift(lit(1));
    let hc = (col("high") - prev_close.clone()).abs();
    let lc = (col("low") - prev_close).abs();

    // Compute max(hl, hc, lc) using when/then/otherwise
    // For the first bar, hc and lc are null, so we default to hl
    when(hc.clone().is_null())
        .then(hl.clone()) // First bar: just use high - low
        .otherwise(
            // max(hl, max(hc, lc))
            when(
                hl.clone()
                    .gt_eq(hc.clone())
                    .and(hl.clone().gt_eq(lc.clone())),
            )
            .then(hl.clone())
            .otherwise(when(hc.clone().gt_eq(lc.clone())).then(hc).otherwise(lc)),
        )
        .alias("true_range")
}

/// Average True Range (ATR) using simple moving average.
///
/// ATR is the simple moving average of True Range values.
/// Returns null until there are enough bars to fill the window.
pub fn atr_sma_expr(window: usize) -> Expr {
    // First compute true range, then SMA
    // We need a two-step approach: compute TR column, then rolling mean
    // This is a composite expression that assumes TR column exists
    col("true_range").rolling_mean(RollingOptionsFixedWindow {
        window_size: window,
        min_periods: window,
        weights: None,
        center: false,
        fn_params: None,
    })
}

/// Average True Range using Wilder smoothing (exponential).
///
/// This is the "classic" ATR as originally defined by Welles Wilder:
/// - First ATR = SMA of first `window` TRs
/// - Subsequent: ATR[t] = ATR[t-1] * (window-1)/window + TR[t] / window
///
/// Wilder smoothing is equivalent to EMA with alpha = 1/window.
pub fn atr_wilder_expr(window: usize) -> Expr {
    // Wilder smoothing uses alpha = 1/window
    let alpha = 1.0 / window as f64;
    col("true_range").ewm_mean(EWMOptions {
        alpha,
        adjust: true,
        bias: false,
        min_periods: window,
        ignore_nulls: true,
    })
}

// =============================================================================
// Phase 2: Momentum & Direction Indicators
// =============================================================================

/// Rolling standard deviation of close price.
///
/// Uses population standard deviation (divide by N, not N-1).
/// Returns null until there are enough bars to fill the window.
pub fn rolling_std_expr(window: usize) -> Expr {
    col("close").rolling_std(RollingOptionsFixedWindow {
        window_size: window,
        min_periods: window,
        weights: None,
        center: false,
        fn_params: None,
    })
}

/// Bollinger Bands as Polars expressions.
///
/// Returns `(middle, upper, lower, bandwidth)` expressions:
/// - Middle = SMA(close, period)
/// - Upper = Middle + multiplier * std
/// - Lower = Middle - multiplier * std
/// - Bandwidth = (Upper - Lower) / Middle
///
/// Results are null during warmup period.
pub fn bollinger_bands_exprs(period: usize, multiplier: f64) -> (Expr, Expr, Expr, Expr) {
    let middle = sma_close_expr(period).alias("bb_middle");

    let std = rolling_std_expr(period);

    let upper = (sma_close_expr(period) + std.clone() * lit(multiplier)).alias("bb_upper");

    let lower = (sma_close_expr(period) - std * lit(multiplier)).alias("bb_lower");

    // Bandwidth = (upper - lower) / middle
    // We compute it after adding middle, upper, lower columns
    let bandwidth = ((col("bb_upper") - col("bb_lower")) / col("bb_middle")).alias("bb_bandwidth");

    (middle, upper, lower, bandwidth)
}

/// Plus Directional Movement (+DM) expression.
///
/// +DM = high - prev_high if positive AND > (prev_low - low), else 0
pub fn plus_dm_expr() -> Expr {
    let up_move = col("high") - col("high").shift(lit(1));
    let down_move = col("low").shift(lit(1)) - col("low");

    when(
        up_move
            .clone()
            .gt(lit(0.0))
            .and(up_move.clone().gt(down_move.clone())),
    )
    .then(up_move)
    .otherwise(lit(0.0))
    .alias("plus_dm")
}

/// Minus Directional Movement (-DM) expression.
///
/// -DM = prev_low - low if positive AND > (high - prev_high), else 0
pub fn minus_dm_expr() -> Expr {
    let up_move = col("high") - col("high").shift(lit(1));
    let down_move = col("low").shift(lit(1)) - col("low");

    when(
        down_move
            .clone()
            .gt(lit(0.0))
            .and(down_move.clone().gt(up_move)),
    )
    .then(down_move)
    .otherwise(lit(0.0))
    .alias("minus_dm")
}

/// Wilder-smoothed +DM expression (requires plus_dm column exists).
///
/// Uses alpha = 1/period for Wilder smoothing.
pub fn plus_dm_smoothed_expr(period: usize) -> Expr {
    let alpha = 1.0 / period as f64;
    col("plus_dm")
        .ewm_mean(EWMOptions {
            alpha,
            adjust: true,
            bias: false,
            min_periods: period,
            ignore_nulls: true,
        })
        .alias("plus_dm_smooth")
}

/// Wilder-smoothed -DM expression (requires minus_dm column exists).
///
/// Uses alpha = 1/period for Wilder smoothing.
pub fn minus_dm_smoothed_expr(period: usize) -> Expr {
    let alpha = 1.0 / period as f64;
    col("minus_dm")
        .ewm_mean(EWMOptions {
            alpha,
            adjust: true,
            bias: false,
            min_periods: period,
            ignore_nulls: true,
        })
        .alias("minus_dm_smooth")
}

/// Plus Directional Indicator (+DI) expression.
///
/// +DI = 100 * smoothed(+DM) / ATR
/// Requires plus_dm_smooth and atr_wilder columns to exist.
pub fn plus_di_expr() -> Expr {
    (lit(100.0) * col("plus_dm_smooth") / col("atr_wilder")).alias("plus_di")
}

/// Minus Directional Indicator (-DI) expression.
///
/// -DI = 100 * smoothed(-DM) / ATR
/// Requires minus_dm_smooth and atr_wilder columns to exist.
pub fn minus_di_expr() -> Expr {
    (lit(100.0) * col("minus_dm_smooth") / col("atr_wilder")).alias("minus_di")
}

/// Directional Movement Index (DX) expression.
///
/// DX = 100 * |+DI - -DI| / (+DI + -DI)
/// Requires plus_di and minus_di columns to exist.
pub fn dx_expr() -> Expr {
    let diff = (col("plus_di") - col("minus_di")).abs();
    let sum = col("plus_di") + col("minus_di");

    // Avoid division by zero
    when(sum.clone().eq(lit(0.0)))
        .then(lit(0.0))
        .otherwise(lit(100.0) * diff / sum)
        .alias("dx")
}

/// Average Directional Index (ADX) expression.
///
/// ADX = Wilder smooth of DX
/// Requires dx column to exist.
pub fn adx_expr(period: usize) -> Expr {
    let alpha = 1.0 / period as f64;
    col("dx")
        .ewm_mean(EWMOptions {
            alpha,
            adjust: true,
            bias: false,
            min_periods: period,
            ignore_nulls: true,
        })
        .alias("adx")
}

/// Apply full DMI/ADX indicator set to a LazyFrame.
///
/// Adds columns: plus_dm, minus_dm, plus_dm_smooth, minus_dm_smooth,
/// true_range, atr_wilder, plus_di, minus_di, dx, adx
///
/// This is a convenience function that applies all DMI-related columns in order.
pub fn apply_dmi_exprs(lf: LazyFrame, period: usize) -> LazyFrame {
    lf.with_column(true_range_expr())
        .with_column(atr_wilder_expr(period).alias("atr_wilder"))
        .with_columns([plus_dm_expr(), minus_dm_expr()])
        .with_columns([
            plus_dm_smoothed_expr(period),
            minus_dm_smoothed_expr(period),
        ])
        .with_columns([plus_di_expr(), minus_di_expr()])
        .with_column(dx_expr())
        .with_column(adx_expr(period))
}

/// Aroon Up expression.
///
/// Aroon-Up = 100 * (period - bars_since_period_high) / period
///
/// Uses a clever technique: for each bar, check all bars in the window
/// to find how many bars ago the maximum occurred. We achieve this by:
/// 1. Computing rolling max of high
/// 2. Marking where high == rolling_max (a new period high)
/// 3. Tracking distance from the last "new high" using cumulative grouping
///
/// Returns null during warmup (first `period` bars).
pub fn aroon_up_expr(period: usize) -> Expr {
    // This expression assumes aroon_bars_since_high column exists
    // aroon_up = 100 * (period - bars_since_high) / period
    (lit(100.0) * (lit(period as f64) - col("aroon_bars_since_high")) / lit(period as f64))
        .alias("aroon_up")
}

/// Aroon Down expression.
///
/// Aroon-Down = 100 * (period - bars_since_period_low) / period
///
/// Returns null during warmup (first `period` bars).
pub fn aroon_down_expr(period: usize) -> Expr {
    // This expression assumes aroon_bars_since_low column exists
    // aroon_down = 100 * (period - bars_since_low) / period
    (lit(100.0) * (lit(period as f64) - col("aroon_bars_since_low")) / lit(period as f64))
        .alias("aroon_down")
}

/// Helper: Compute bars since period high using rolling analysis.
///
/// This computes how many bars ago the highest high in the period occurred.
/// We iterate through each offset in the lookback window and find the minimum
/// distance where the shifted high equals the rolling max.
/// Ties prefer the most recent bar (offset 0 checked first).
fn bars_since_high_expr(period: usize) -> Expr {
    // For each offset 0..period, check if high[t-offset] == rolling_max_high[t]
    // Find the minimum offset where this is true (most recent max)
    let mut min_distance: Option<Expr> = None;

    for offset in 0..period {
        let shifted_high = col("high").shift(lit(offset as i64));
        let is_max_at_offset = shifted_high.eq(col("rolling_max_high"));

        let distance_at_offset = when(is_max_at_offset)
            .then(lit(offset as f64))
            .otherwise(lit(f64::INFINITY));

        min_distance = Some(match min_distance {
            None => distance_at_offset,
            Some(prev) => when(distance_at_offset.clone().lt(prev.clone()))
                .then(distance_at_offset)
                .otherwise(prev),
        });
    }

    min_distance
        .unwrap_or(lit(NULL))
        .alias("aroon_bars_since_high")
}

/// Helper: Compute bars since period low using rolling analysis.
fn bars_since_low_expr(period: usize) -> Expr {
    let mut min_distance: Option<Expr> = None;

    for offset in 0..period {
        let shifted_low = col("low").shift(lit(offset as i64));
        let is_min_at_offset = shifted_low.clone().eq(col("rolling_min_low"));

        let distance_at_offset = when(is_min_at_offset)
            .then(lit(offset as f64))
            .otherwise(lit(f64::INFINITY));

        min_distance = Some(match min_distance {
            None => distance_at_offset,
            Some(prev) => when(distance_at_offset.clone().lt(prev.clone()))
                .then(distance_at_offset)
                .otherwise(prev),
        });
    }

    min_distance
        .unwrap_or(lit(NULL))
        .alias("aroon_bars_since_low")
}

/// Aroon Oscillator expression.
///
/// Aroon Oscillator = Aroon-Up - Aroon-Down
/// Requires aroon_up and aroon_down columns to exist.
pub fn aroon_oscillator_expr() -> Expr {
    (col("aroon_up") - col("aroon_down")).alias("aroon_oscillator")
}

/// Apply full Aroon indicator set to a LazyFrame.
///
/// Adds columns: rolling_max_high, rolling_min_low, aroon_bars_since_high,
/// aroon_bars_since_low, aroon_up, aroon_down, aroon_oscillator
pub fn apply_aroon_exprs(lf: LazyFrame, period: usize) -> LazyFrame {
    // Step 1: Add rolling max/min columns needed for bars_since calculations
    let rolling_max_high = col("high")
        .rolling_max(RollingOptionsFixedWindow {
            window_size: period,
            min_periods: period,
            weights: None,
            center: false,
            fn_params: None,
        })
        .alias("rolling_max_high");

    let rolling_min_low = col("low")
        .rolling_min(RollingOptionsFixedWindow {
            window_size: period,
            min_periods: period,
            weights: None,
            center: false,
            fn_params: None,
        })
        .alias("rolling_min_low");

    // Step 2: Add the rolling max/min columns
    let lf = lf.with_columns([rolling_max_high, rolling_min_low]);

    // Step 3: Add bars_since calculations (depend on rolling max/min)
    let lf = lf.with_columns([bars_since_high_expr(period), bars_since_low_expr(period)]);

    // Step 4: Add aroon up/down (depend on bars_since)
    let lf = lf.with_columns([aroon_up_expr(period), aroon_down_expr(period)]);

    // Step 5: Add oscillator (depends on aroon up/down)
    lf.with_column(aroon_oscillator_expr())
}

/// Apply Bollinger Bands to a LazyFrame.
///
/// Adds columns: bb_middle, bb_upper, bb_lower, bb_bandwidth
pub fn apply_bollinger_exprs(lf: LazyFrame, period: usize, multiplier: f64) -> LazyFrame {
    let (middle, upper, lower, bandwidth) = bollinger_bands_exprs(period, multiplier);
    // We need to add middle, upper, lower first, then bandwidth (which depends on them)
    lf.with_columns([middle, upper, lower])
        .with_column(bandwidth)
}

/// Keltner Channel expressions.
///
/// Returns (center, upper, lower) expressions.
/// Center = EMA of close
/// Upper = Center + multiplier * ATR
/// Lower = Center - multiplier * ATR
///
/// Note: Requires true_range column to be computed first.
pub fn keltner_channel_exprs(
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
) -> (Expr, Expr, Expr) {
    let center = ema_close_expr(ema_period).alias("kc_center");

    // ATR using Wilder smoothing (standard for Keltner)
    let atr = atr_wilder_expr(atr_period);

    let upper = (ema_close_expr(ema_period) + atr.clone() * lit(multiplier)).alias("kc_upper");
    let lower = (ema_close_expr(ema_period) - atr * lit(multiplier)).alias("kc_lower");

    (center, upper, lower)
}

/// Apply Keltner Channel indicators to a LazyFrame.
///
/// Adds columns: true_range, kc_center, kc_upper, kc_lower
pub fn apply_keltner_exprs(
    lf: LazyFrame,
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
) -> LazyFrame {
    // First add true_range which is needed for ATR
    let lf = lf.with_column(true_range_expr().alias("true_range"));

    let (center, upper, lower) = keltner_channel_exprs(ema_period, atr_period, multiplier);
    lf.with_columns([center, upper, lower])
}

/// STARC Bands (Stoller Average Range Channel) expressions.
///
/// Returns (center, upper, lower) expressions.
/// Center = SMA of close
/// Upper = Center + multiplier * ATR
/// Lower = Center - multiplier * ATR
///
/// Note: Uses SMA (unlike Keltner which uses EMA). Requires true_range column.
pub fn starc_bands_exprs(
    sma_period: usize,
    atr_period: usize,
    multiplier: f64,
) -> (Expr, Expr, Expr) {
    let center = sma_close_expr(sma_period).alias("starc_center");

    // ATR using SMA (standard ATR, not Wilder for STARC)
    let atr = atr_sma_expr(atr_period);

    let upper = (sma_close_expr(sma_period) + atr.clone() * lit(multiplier)).alias("starc_upper");
    let lower = (sma_close_expr(sma_period) - atr * lit(multiplier)).alias("starc_lower");

    (center, upper, lower)
}

/// Apply STARC Bands indicators to a LazyFrame.
///
/// Adds columns: true_range, starc_center, starc_upper, starc_lower
pub fn apply_starc_exprs(
    lf: LazyFrame,
    sma_period: usize,
    atr_period: usize,
    multiplier: f64,
) -> LazyFrame {
    // First add true_range which is needed for ATR
    let lf = lf.with_column(true_range_expr().alias("true_range"));

    let (center, upper, lower) = starc_bands_exprs(sma_period, atr_period, multiplier);
    lf.with_columns([center, upper, lower])
}

/// Supertrend basic band expressions (before ratcheting).
///
/// Returns (basic_band, raw_upper, raw_lower) expressions.
/// - Basic = (High + Low) / 2
/// - Raw Upper = Basic + multiplier * ATR
/// - Raw Lower = Basic - multiplier * ATR
///
/// Note: These are the raw bands before stateful ratcheting adjustment.
/// The actual Supertrend requires sequential computation for proper band behavior.
pub fn supertrend_basic_exprs(atr_period: usize, multiplier: f64) -> (Expr, Expr, Expr) {
    let basic = ((col("high") + col("low")) / lit(2.0)).alias("st_basic");

    // ATR using SMA (standard ATR for Supertrend)
    let atr = atr_sma_expr(atr_period);

    let raw_upper = ((col("high") + col("low")) / lit(2.0) + atr.clone() * lit(multiplier))
        .alias("st_raw_upper");
    let raw_lower =
        ((col("high") + col("low")) / lit(2.0) - atr * lit(multiplier)).alias("st_raw_lower");

    (basic, raw_upper, raw_lower)
}

/// Apply Supertrend basic indicators to a LazyFrame.
///
/// Adds columns: true_range, st_basic, st_raw_upper, st_raw_lower, st_is_uptrend
///
/// Note: The st_is_uptrend column is a simplified approximation based on raw bands.
/// The actual Supertrend indicator has stateful band ratcheting that can only be
/// computed sequentially. Use the sequential `signal()` method for exact behavior.
pub fn apply_supertrend_exprs(lf: LazyFrame, atr_period: usize, multiplier: f64) -> LazyFrame {
    // First add true_range which is needed for ATR
    let lf = lf.with_column(true_range_expr().alias("true_range"));

    let (basic, raw_upper, raw_lower) = supertrend_basic_exprs(atr_period, multiplier);
    let lf = lf.with_columns([basic, raw_upper, raw_lower]);

    // Simplified trend detection: price above raw lower band = uptrend
    // This is an approximation - actual Supertrend uses ratcheted bands
    let is_uptrend = col("close").gt(col("st_raw_lower")).alias("st_is_uptrend");

    lf.with_column(is_uptrend)
}

// =============================================================================
// Heikin-Ashi Expressions
// =============================================================================

/// Apply Heikin-Ashi candle indicators to a LazyFrame.
///
/// Heikin-Ashi formulas:
/// - HA Close = (Open + High + Low + Close) / 4
/// - HA Open = (prev HA Open + prev HA Close) / 2  [recursive]
/// - HA High = max(High, HA Open, HA Close)
/// - HA Low = min(Low, HA Open, HA Close)
///
/// The HA Open calculation is recursive. We approximate it using EWM with alpha=0.5
/// on HA Close values, which gives us the midpoint sequence (HA_Open + HA_Close)/2,
/// then shift by 1 to get HA Open.
///
/// Adds columns: ha_close, ha_open, ha_high, ha_low, ha_bullish, ha_bearish
pub fn apply_heikin_ashi_exprs(lf: LazyFrame) -> LazyFrame {
    // Step 1: Compute HA Close (non-recursive)
    let ha_close =
        ((col("open") + col("high") + col("low") + col("close")) / lit(4.0)).alias("ha_close");

    let lf = lf.with_column(ha_close);

    // Step 2: Compute the midpoint sequence using EWM with alpha=0.5
    // mid[t] = 0.5 * mid[t-1] + 0.5 * ha_close[t]
    // This gives us (ha_open[t] + ha_close[t]) / 2
    let ha_mid = col("ha_close")
        .ewm_mean(EWMOptions {
            alpha: 0.5,
            adjust: true,
            bias: false,
            min_periods: 1,
            ignore_nulls: true,
        })
        .alias("ha_mid");

    let lf = lf.with_column(ha_mid);

    // Step 3: HA Open = previous midpoint
    // For the first bar, use (open + close) / 2 as the initial HA Open
    let ha_open = col("ha_mid")
        .shift(lit(1))
        .fill_null((col("open") + col("close")) / lit(2.0))
        .alias("ha_open");

    let lf = lf.with_column(ha_open);

    // Step 4: HA High and HA Low
    // HA High = max(high, ha_open, ha_close)
    // Use nested when/then/otherwise for 3-way max
    let ha_high = when(
        col("high")
            .gt_eq(col("ha_open"))
            .and(col("high").gt_eq(col("ha_close"))),
    )
    .then(col("high"))
    .when(col("ha_open").gt_eq(col("ha_close")))
    .then(col("ha_open"))
    .otherwise(col("ha_close"))
    .alias("ha_high");

    // HA Low = min(low, ha_open, ha_close)
    // Use nested when/then/otherwise for 3-way min
    let ha_low = when(
        col("low")
            .lt_eq(col("ha_open"))
            .and(col("low").lt_eq(col("ha_close"))),
    )
    .then(col("low"))
    .when(col("ha_open").lt_eq(col("ha_close")))
    .then(col("ha_open"))
    .otherwise(col("ha_close"))
    .alias("ha_low");

    let lf = lf.with_columns([ha_high, ha_low]);

    // Step 5: Bullish/Bearish detection
    let ha_bullish = col("ha_close").gt(col("ha_open")).alias("ha_bullish");
    let ha_bearish = col("ha_close").lt(col("ha_open")).alias("ha_bearish");

    lf.with_columns([ha_bullish, ha_bearish])
}

// =============================================================================
// Opening Range Expressions
// =============================================================================

use crate::indicators::OpeningPeriod;

/// Apply Opening Range Breakout indicators to a LazyFrame.
///
/// For **Rolling** period:
/// - range_high = max(high) over the prior N bars (excluding current bar)
/// - range_low = min(low) over the prior N bars (excluding current bar)
/// - is_range_complete = always true after warmup
///
/// For **Weekly/Monthly** periods:
/// - Detects period boundaries based on ISO week/month
/// - Computes the first N bars of each period
/// - range_high/low are the max/min of those first N bars
/// - is_range_complete becomes true after N bars in each period
///
/// Adds columns: orb_range_high, orb_range_low, orb_is_complete, orb_bars_in_range
pub fn apply_opening_range_exprs(
    lf: LazyFrame,
    range_bars: usize,
    period: OpeningPeriod,
) -> LazyFrame {
    match period {
        OpeningPeriod::Rolling => apply_opening_range_rolling(lf, range_bars),
        OpeningPeriod::Weekly => apply_opening_range_calendar(lf, range_bars, "week"),
        OpeningPeriod::Monthly => apply_opening_range_calendar(lf, range_bars, "month"),
    }
}

/// Rolling opening range: simple lookback over prior N bars.
fn apply_opening_range_rolling(lf: LazyFrame, range_bars: usize) -> LazyFrame {
    // Shift by 1 to exclude current bar, then compute rolling max/min
    let range_high = col("high")
        .shift(lit(1))
        .rolling_max(RollingOptionsFixedWindow {
            window_size: range_bars,
            min_periods: range_bars,
            weights: None,
            center: false,
            fn_params: None,
        })
        .alias("orb_range_high");

    let range_low = col("low")
        .shift(lit(1))
        .rolling_min(RollingOptionsFixedWindow {
            window_size: range_bars,
            min_periods: range_bars,
            weights: None,
            center: false,
            fn_params: None,
        })
        .alias("orb_range_low");

    // For rolling, range is always complete after warmup
    let is_complete = col("orb_range_high")
        .is_not_null()
        .alias("orb_is_complete");

    // Bars in range is always range_bars for rolling
    let bars_in_range = when(col("orb_range_high").is_not_null())
        .then(lit(range_bars as i32))
        .otherwise(lit(0))
        .alias("orb_bars_in_range");

    lf.with_columns([range_high, range_low])
        .with_columns([is_complete, bars_in_range])
}

/// Calendar-based opening range (Weekly or Monthly).
///
/// This uses a simplified approach:
/// 1. Extract period key (week+year or month+year)
/// 2. Detect period changes
/// 3. Compute cumulative bar count within each period
/// 4. Compute running max/min within each period's first N bars
fn apply_opening_range_calendar(lf: LazyFrame, range_bars: usize, period_type: &str) -> LazyFrame {
    // Add period key column for grouping
    let period_key = match period_type {
        "week" => {
            // Create a unique key for each ISO week: year * 100 + week_number
            (col("ts").dt().iso_year() * lit(100) + col("ts").dt().week()).alias("_period_key")
        }
        "month" => {
            // Create a unique key for each month: year * 100 + month
            (col("ts").dt().year() * lit(100) + col("ts").dt().month()).alias("_period_key")
        }
        _ => panic!("Invalid period type"),
    };

    let lf = lf.with_column(period_key);

    // Detect period changes: when period_key differs from previous
    let period_change = col("_period_key")
        .neq(col("_period_key").shift(lit(1)))
        .fill_null(lit(true)) // First bar starts a new period
        .alias("_period_change");

    let lf = lf.with_column(period_change);

    // Compute period index: cumsum of period changes gives us a unique period ID
    let period_idx = col("_period_change")
        .cast(DataType::Int32)
        .cum_sum(false)
        .alias("_period_idx");

    let lf = lf.with_column(period_idx);

    // Compute bar index within each period using over() expression
    // Note: bars_in_period is 1-based (first bar of period = 1)
    let bar_in_period = lit(1)
        .cum_sum(false)
        .over([col("_period_idx")])
        .alias("orb_bars_in_range");

    let lf = lf.with_column(bar_in_period);

    // Determine if range is complete (bar_in_period > range_bars)
    let is_complete = col("orb_bars_in_range")
        .gt(lit(range_bars as i32))
        .alias("orb_is_complete");

    let lf = lf.with_column(is_complete);

    // Compute range_high and range_low within each period
    // We need max/min of the first N bars of each period
    // Use conditional accumulation: only include bars where bar_in_period <= range_bars

    // First, create masked high/low that are null after range_bars
    let range_high_masked = when(col("orb_bars_in_range").lt_eq(lit(range_bars as i32)))
        .then(col("high"))
        .otherwise(lit(NULL).cast(DataType::Float64))
        .alias("_range_high_masked");

    let range_low_masked = when(col("orb_bars_in_range").lt_eq(lit(range_bars as i32)))
        .then(col("low"))
        .otherwise(lit(NULL).cast(DataType::Float64))
        .alias("_range_low_masked");

    let lf = lf.with_columns([range_high_masked, range_low_masked]);

    // Compute running max/min within each period
    // Use max().over() but only on non-null values
    let range_high = col("_range_high_masked")
        .max()
        .over([col("_period_idx")])
        .alias("orb_range_high");

    let range_low = col("_range_low_masked")
        .min()
        .over([col("_period_idx")])
        .alias("orb_range_low");

    let lf = lf.with_columns([range_high, range_low]);

    // Clean up temporary columns
    lf.select([
        col("*").exclude([
            "_period_key",
            "_period_change",
            "_period_idx",
            "_range_high_masked",
            "_range_low_masked",
        ]),
    ])
}

// =============================================================================
// Parabolic SAR Expressions
// =============================================================================

/// Apply Parabolic SAR approximation indicators to a LazyFrame.
///
/// **Note:** Parabolic SAR is inherently stateful (the SAR value depends on
/// previous SAR, acceleration factor history, and extreme points). This function
/// provides a simplified approximation suitable for trend detection in Polars mode.
/// For exact SAR behavior, use the sequential `signal()` method.
///
/// The approximation uses ATR-based bands:
/// - Basic band = (High + Low) / 2
/// - Upper band = Basic + af_equiv * ATR (downtrend SAR approximation)
/// - Lower band = Basic - af_equiv * ATR (uptrend SAR approximation)
/// - is_uptrend = close > lower band
///
/// The af_equiv parameter approximates the effective acceleration factor.
/// A typical value is around 0.10-0.15 (midway between af_start and af_max).
///
/// Adds columns: sar_basic, sar_upper_approx, sar_lower_approx, sar_is_uptrend
pub fn apply_parabolic_sar_exprs(lf: LazyFrame, atr_period: usize, af_equiv: f64) -> LazyFrame {
    // First add true_range which is needed for ATR
    let lf = lf.with_column(true_range_expr().alias("true_range"));

    // Compute ATR using SMA
    let atr = atr_sma_expr(atr_period);

    // Basic band = midpoint of price range
    let basic = ((col("high") + col("low")) / lit(2.0)).alias("sar_basic");

    // Upper band approximates SAR in downtrend (SAR trails above price)
    // This is where SAR would be if we were in a downtrend
    let upper = ((col("high") + col("low")) / lit(2.0) + atr.clone() * lit(af_equiv))
        .alias("sar_upper_approx");

    // Lower band approximates SAR in uptrend (SAR trails below price)
    // This is where SAR would be if we were in an uptrend
    let lower =
        ((col("high") + col("low")) / lit(2.0) - atr * lit(af_equiv)).alias("sar_lower_approx");

    let lf = lf.with_columns([basic, upper, lower]);

    // Simplified trend detection: close > lower band = uptrend
    // In uptrend, SAR is below price, so if close > approximated SAR, we're bullish
    let is_uptrend = col("close").gt(col("sar_lower_approx")).alias("sar_is_uptrend");

    lf.with_column(is_uptrend)
}

/// Indicator specification for building indicator sets.
#[derive(Debug, Clone)]
pub enum IndicatorSpec {
    /// Donchian channel with specified lookback
    Donchian { lookback: usize },
    /// Simple moving average of close
    SMA { window: usize, alias: String },
    /// Exponential moving average of close
    EMA { window: usize, alias: String },
    /// Average True Range (SMA-based)
    ATR { window: usize },
    /// Average True Range (Wilder smoothing)
    ATRWilder { window: usize },
    /// Bollinger Bands with period and multiplier
    Bollinger { period: usize, multiplier: f64 },
    /// DMI/ADX indicators (full set)
    DMI { period: usize },
    /// Aroon indicators (full set)
    Aroon { period: usize },
}

/// Collection of indicators to compute together.
#[derive(Debug, Clone, Default)]
pub struct IndicatorSet {
    pub indicators: Vec<IndicatorSpec>,
}

impl IndicatorSet {
    /// Create a new empty indicator set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a Donchian channel indicator.
    pub fn with_donchian(mut self, lookback: usize) -> Self {
        self.indicators.push(IndicatorSpec::Donchian { lookback });
        self
    }

    /// Add an SMA indicator.
    pub fn with_sma(mut self, window: usize, alias: impl Into<String>) -> Self {
        self.indicators.push(IndicatorSpec::SMA {
            window,
            alias: alias.into(),
        });
        self
    }

    /// Add an EMA indicator.
    pub fn with_ema(mut self, window: usize, alias: impl Into<String>) -> Self {
        self.indicators.push(IndicatorSpec::EMA {
            window,
            alias: alias.into(),
        });
        self
    }

    /// Add ATR with SMA smoothing.
    pub fn with_atr(mut self, window: usize) -> Self {
        self.indicators.push(IndicatorSpec::ATR { window });
        self
    }

    /// Add ATR with Wilder smoothing.
    pub fn with_atr_wilder(mut self, window: usize) -> Self {
        self.indicators.push(IndicatorSpec::ATRWilder { window });
        self
    }

    /// Add Bollinger Bands.
    pub fn with_bollinger(mut self, period: usize, multiplier: f64) -> Self {
        self.indicators
            .push(IndicatorSpec::Bollinger { period, multiplier });
        self
    }

    /// Add DMI/ADX indicator set.
    pub fn with_dmi(mut self, period: usize) -> Self {
        self.indicators.push(IndicatorSpec::DMI { period });
        self
    }

    /// Add Aroon indicator set.
    pub fn with_aroon(mut self, period: usize) -> Self {
        self.indicators.push(IndicatorSpec::Aroon { period });
        self
    }
}

/// Apply an indicator set to a LazyFrame.
///
/// The input LazyFrame must have columns: open, high, low, close, volume
/// (standard OHLCV schema).
///
/// Returns a new LazyFrame with indicator columns added.
pub fn apply_indicators(lf: LazyFrame, indicator_set: &IndicatorSet) -> LazyFrame {
    let mut lf = lf;
    let mut needs_true_range = false;

    // Check if we need true_range column
    for ind in &indicator_set.indicators {
        match ind {
            IndicatorSpec::ATR { .. }
            | IndicatorSpec::ATRWilder { .. }
            | IndicatorSpec::DMI { .. } => {
                needs_true_range = true;
                break;
            }
            _ => {}
        }
    }

    // Add true_range if needed by ATR or DMI indicators
    if needs_true_range {
        lf = lf.with_column(true_range_expr());
    }

    // Apply each indicator
    for ind in &indicator_set.indicators {
        lf = match ind {
            IndicatorSpec::Donchian { lookback } => {
                let (upper, lower) = donchian_channel_exprs(*lookback);
                lf.with_columns([upper, lower])
            }
            IndicatorSpec::SMA { window, alias } => {
                lf.with_column(sma_close_expr_aliased(*window, alias))
            }
            IndicatorSpec::EMA { window, alias } => {
                lf.with_column(ema_close_expr_aliased(*window, alias))
            }
            IndicatorSpec::ATR { window } => lf.with_column(atr_sma_expr(*window).alias("atr")),
            IndicatorSpec::ATRWilder { window } => {
                lf.with_column(atr_wilder_expr(*window).alias("atr_wilder"))
            }
            IndicatorSpec::Bollinger { period, multiplier } => {
                apply_bollinger_exprs(lf, *period, *multiplier)
            }
            IndicatorSpec::DMI { period } => apply_dmi_exprs(lf, *period),
            IndicatorSpec::Aroon { period } => apply_aroon_exprs(lf, *period),
        };
    }

    lf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::Bar;
    use crate::data::bars_to_dataframe;
    use chrono::{TimeZone, Utc};

    fn bars_from_ohlc(ohlc: &[(f64, f64, f64, f64)]) -> Vec<Bar> {
        ohlc.iter()
            .enumerate()
            .map(|(i, &(o, h, l, c))| {
                let ts = Utc
                    .with_ymd_and_hms(2024, 1, 1 + i as u32, 0, 0, 0)
                    .unwrap();
                Bar::new(ts, o, h, l, c, 1000.0, "TEST", "1d")
            })
            .collect()
    }

    fn bars_from_closes(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let ts = Utc
                    .with_ymd_and_hms(2024, 1, 1 + i as u32, 0, 0, 0)
                    .unwrap();
                Bar::new(ts, c, c, c, c, 1000.0, "TEST", "1d")
            })
            .collect()
    }

    #[test]
    fn test_donchian_polars_matches_sequential() {
        let ohlc = vec![
            (100.0, 102.0, 98.0, 101.0),
            (101.0, 103.0, 99.0, 102.0),
            (102.0, 104.0, 100.0, 103.0),
            (103.0, 105.0, 101.0, 104.0),
            (104.0, 106.0, 102.0, 105.0),
            (105.0, 107.0, 103.0, 104.0),
        ];
        let bars = bars_from_ohlc(&ohlc);

        // Sequential implementation
        let seq_dc = crate::indicators::donchian_channel(&bars, 5);

        // Polars implementation
        let df = bars_to_dataframe(&bars).unwrap();
        let (upper_expr, lower_expr) = donchian_channel_exprs(5);
        let result = df
            .lazy()
            .with_columns([upper_expr, lower_expr])
            .collect()
            .unwrap();

        let pol_upper = result.column("dc_upper").unwrap().f64().unwrap();
        let pol_lower = result.column("dc_lower").unwrap().f64().unwrap();

        // Compare results
        for (i, seq_val) in seq_dc.iter().enumerate() {
            match seq_val {
                None => {
                    assert!(
                        pol_upper.get(i).is_none(),
                        "Expected null upper at index {}",
                        i
                    );
                    assert!(
                        pol_lower.get(i).is_none(),
                        "Expected null lower at index {}",
                        i
                    );
                }
                Some(ch) => {
                    let pu = pol_upper.get(i).unwrap();
                    let pl = pol_lower.get(i).unwrap();
                    assert!(
                        (pu - ch.upper).abs() < 1e-10,
                        "Upper mismatch at {}: {} vs {}",
                        i,
                        pu,
                        ch.upper
                    );
                    assert!(
                        (pl - ch.lower).abs() < 1e-10,
                        "Lower mismatch at {}: {} vs {}",
                        i,
                        pl,
                        ch.lower
                    );
                }
            }
        }
    }

    #[test]
    fn test_sma_polars_matches_sequential() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0, 4.0, 5.0]);

        // Sequential
        let seq_sma = crate::indicators::sma_close(&bars, 3);

        // Polars
        let df = bars_to_dataframe(&bars).unwrap();
        let result = df
            .lazy()
            .with_column(sma_close_expr(3).alias("sma"))
            .collect()
            .unwrap();

        let pol_sma = result.column("sma").unwrap().f64().unwrap();

        for (i, seq_val) in seq_sma.iter().enumerate() {
            match seq_val {
                None => {
                    assert!(pol_sma.get(i).is_none(), "Expected null at index {}", i);
                }
                Some(v) => {
                    let pv = pol_sma.get(i).unwrap();
                    assert!(
                        (pv - v).abs() < 1e-10,
                        "SMA mismatch at {}: {} vs {}",
                        i,
                        pv,
                        v
                    );
                }
            }
        }
    }

    #[test]
    fn test_true_range_polars() {
        let ohlc = vec![
            (100.0, 105.0, 95.0, 102.0),  // TR = 10
            (102.0, 108.0, 100.0, 106.0), // TR = max(8, 6, 2) = 8
            (106.0, 112.0, 104.0, 110.0), // TR = max(8, 6, 2) = 8
        ];
        let bars = bars_from_ohlc(&ohlc);

        // Sequential
        let seq_tr = crate::indicators::true_range(&bars);

        // Polars
        let df = bars_to_dataframe(&bars).unwrap();
        let result = df.lazy().with_column(true_range_expr()).collect().unwrap();

        let pol_tr = result.column("true_range").unwrap().f64().unwrap();

        for (i, &seq_val) in seq_tr.iter().enumerate() {
            let pv = pol_tr.get(i).unwrap();
            assert!(
                (pv - seq_val).abs() < 1e-10,
                "TR mismatch at {}: {} vs {}",
                i,
                pv,
                seq_val
            );
        }
    }

    #[test]
    fn test_indicator_set_builder() {
        let set = IndicatorSet::new()
            .with_donchian(20)
            .with_sma(50, "sma_50")
            .with_ema(20, "ema_20")
            .with_atr_wilder(14);

        assert_eq!(set.indicators.len(), 4);
    }

    #[test]
    fn test_apply_indicators() {
        let ohlc = vec![
            (100.0, 105.0, 95.0, 102.0),
            (102.0, 108.0, 100.0, 106.0),
            (106.0, 112.0, 104.0, 110.0),
            (110.0, 115.0, 108.0, 113.0),
            (113.0, 118.0, 111.0, 116.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let df = bars_to_dataframe(&bars).unwrap();

        let set = IndicatorSet::new()
            .with_donchian(3)
            .with_sma(3, "sma_3")
            .with_atr_wilder(3);

        let result = apply_indicators(df.lazy(), &set).collect().unwrap();

        // Check that all expected columns exist
        assert!(result.column("dc_upper").is_ok());
        assert!(result.column("dc_lower").is_ok());
        assert!(result.column("sma_3").is_ok());
        assert!(result.column("true_range").is_ok());
        assert!(result.column("atr_wilder").is_ok());
    }
}
