//! Indicator calculations (pure functions, no IO).
//!
//! Key invariant: indicator values at index `t` must depend only on bars `0..=t`.

use crate::bar::Bar;

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

    #[test]
    fn sma_window_3_matches_definition() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0, 4.0]);
        let sma = sma_close(&bars, 3);
        assert_eq!(sma, vec![None, None, Some(2.0), Some(3.0)]);
    }
}
