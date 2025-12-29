//! Seeded, opt-in randomization of initial UI defaults.
//!
//! Intended for "Monte Carlo-ish" exploration: each launch can open near defaults
//! while remaining reproducible via the seed.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::NaiveDate;
use rand::Rng;

/// Seeded, opt-in randomization of initial UI defaults.
#[derive(Debug, Clone)]
pub struct RandomDefaults {
    pub enabled: bool,
    pub seed: u64,
}

impl Default for RandomDefaults {
    fn default() -> Self {
        Self {
            enabled: false,
            seed: 0,
        }
    }
}

pub fn env_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "y" | "on")
        }
        Err(_) => false,
    }
}

pub fn env_optional_bool(name: &str) -> Option<bool> {
    std::env::var(name).ok().map(|v| {
        let v = v.trim().to_ascii_lowercase();
        // Accept common falsy values explicitly.
        if matches!(v.as_str(), "0" | "false" | "no" | "n" | "off") {
            false
        } else {
            matches!(v.as_str(), "1" | "true" | "yes" | "y" | "on")
        }
    })
}

pub fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

pub fn generate_seed() -> u64 {
    // Stable enough on all platforms; not crypto-grade (doesn't need to be).
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn read_and_increment_launch_count() -> u64 {
    // Persist a simple counter so the "random defaults" change each time the TUI is opened,
    // even if you open it multiple times in the same second.
    let path = std::path::Path::new("configs").join("tui_launch_count.txt");
    let mut count: u64 = 0;
    if let Ok(s) = std::fs::read_to_string(&path) {
        count = s.trim().parse::<u64>().unwrap_or(0);
    }
    let next = count.saturating_add(1);
    let _ = std::fs::create_dir_all("configs");
    let _ = std::fs::write(&path, next.to_string());
    next
}

pub fn derive_nonrepeatable_seed() -> u64 {
    // Not crypto; just "looks random" and won't repeat in normal usage.
    let t = generate_seed();
    let launch = read_and_increment_launch_count();
    let pid = std::process::id() as u64;
    // Mix bits a bit (xorshift-ish).
    let mut x = t ^ (launch.rotate_left(17)) ^ (pid.rotate_left(7));
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    x.wrapping_mul(0x2545F4914F6CDD1D)
}

pub fn jitter_pct_delta(rng: &mut impl Rng, min_abs: f64, max_abs: f64) -> f64 {
    // delta in [-max_abs, -min_abs] U [min_abs, max_abs]
    let mag = rng.gen_range(min_abs..=max_abs);
    let sign = if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
    sign * mag
}

pub fn round_to_step_f64(value: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return value;
    }
    (value / step).round() * step
}

pub fn jitter_usize_percent(
    rng: &mut impl Rng,
    base: usize,
    min_pct: f64,
    max_pct: f64,
    step: usize,
    min: usize,
    max: usize,
) -> usize {
    let delta = jitter_pct_delta(rng, min_pct, max_pct);
    let mut candidate = (base as f64) * (1.0 + delta);
    if step > 1 {
        candidate = round_to_step_f64(candidate, step as f64);
    }
    let candidate = candidate.round().max(0.0) as usize;
    candidate.clamp(min, max)
}

pub fn jitter_f64_percent(
    rng: &mut impl Rng,
    base: f64,
    min_pct: f64,
    max_pct: f64,
    step: f64,
    min: f64,
    max: f64,
) -> f64 {
    let delta = jitter_pct_delta(rng, min_pct, max_pct);
    let candidate = base * (1.0 + delta);
    round_to_step_f64(candidate, step).clamp(min, max)
}

pub fn jitter_date_range_percent(
    rng: &mut impl Rng,
    start: NaiveDate,
    end: NaiveDate,
    min_pct: f64,
    max_pct: f64,
    min_date: NaiveDate,
    max_date: NaiveDate,
    min_span_days: i64,
) -> (NaiveDate, NaiveDate) {
    let span = (end - start).num_days().abs().max(1);
    // Only jitter the start date; end date stays fixed to maximize data coverage
    let mag_start = rng.gen_range(min_pct..=max_pct);
    // Start date can shift either direction
    let shift_start =
        (span as f64 * mag_start).round() as i64 * if rng.gen_bool(0.5) { 1 } else { -1 };
    // End date only shifts forward (never backward) to ensure we always include recent data.
    // This prevents YOLO mode from accidentally cutting off years of recent market data.
    // The shift is clamped to max_date anyway, so forward shifts have no effect when end == max_date.

    let mut s = start
        .checked_add_signed(chrono::Duration::days(shift_start))
        .unwrap_or(start)
        .clamp(min_date, max_date);
    // Keep end date at the original value (no jitter) to maximize data coverage
    let mut e = end.clamp(min_date, max_date);

    if e < s {
        std::mem::swap(&mut s, &mut e);
    }
    if (e - s).num_days() < min_span_days {
        e = s
            .checked_add_signed(chrono::Duration::days(min_span_days))
            .unwrap_or(e)
            .clamp(min_date, max_date);
    }
    (s, e)
}
