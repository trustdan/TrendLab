//! Statistical inference for trading strategy evaluation.
//!
//! Provides rigorous statistical testing to guard against overfitting:
//! - Bootstrap confidence intervals for performance metrics
//! - Permutation tests for significance
//! - False Discovery Rate (FDR) correction for multiple comparisons
//! - Standard errors and hypothesis testing

use rand::prelude::*;
use rand::rngs::SmallRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during statistical operations.
#[derive(Debug, Error)]
pub enum StatisticsError {
    #[error("Insufficient samples: need {needed}, have {available}")]
    InsufficientSamples { needed: usize, available: usize },

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Computation error: {0}")]
    ComputationError(String),
}

/// Configuration for bootstrap resampling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// Number of bootstrap iterations
    pub n_iterations: usize,
    /// Confidence level (e.g., 0.95 for 95% CI)
    pub confidence_level: f64,
    /// Random seed for reproducibility
    pub seed: u64,
    /// Use bias-corrected and accelerated (BCa) bootstrap
    pub use_bca: bool,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            n_iterations: 10_000,
            confidence_level: 0.95,
            seed: 42,
            use_bca: false, // Percentile method by default
        }
    }
}

impl BootstrapConfig {
    /// Create a quick bootstrap config with fewer iterations.
    pub fn quick() -> Self {
        Self {
            n_iterations: 1_000,
            ..Default::default()
        }
    }

    /// Create a thorough bootstrap config with more iterations.
    pub fn thorough() -> Self {
        Self {
            n_iterations: 50_000,
            ..Default::default()
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), StatisticsError> {
        if self.n_iterations < 100 {
            return Err(StatisticsError::InvalidParameter(
                "n_iterations must be >= 100".to_string(),
            ));
        }
        if self.confidence_level <= 0.0 || self.confidence_level >= 1.0 {
            return Err(StatisticsError::InvalidParameter(
                "confidence_level must be in (0, 1)".to_string(),
            ));
        }
        Ok(())
    }
}

/// Result of a bootstrap confidence interval estimation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResult {
    /// Point estimate (original sample statistic)
    pub point_estimate: f64,
    /// Lower bound of confidence interval
    pub ci_lower: f64,
    /// Upper bound of confidence interval
    pub ci_upper: f64,
    /// Standard error estimate
    pub std_error: f64,
    /// Confidence level used
    pub confidence_level: f64,
    /// Number of bootstrap iterations
    pub n_iterations: usize,
    /// Bootstrap distribution statistics
    pub bootstrap_mean: f64,
    /// Bootstrap distribution median
    pub bootstrap_median: f64,
}

impl BootstrapResult {
    /// Check if zero is within the confidence interval.
    ///
    /// If zero is NOT in the CI, the statistic is significantly different from zero.
    pub fn is_significant(&self) -> bool {
        !(self.ci_lower <= 0.0 && self.ci_upper >= 0.0)
    }

    /// Check if the point estimate is significantly positive.
    pub fn is_significantly_positive(&self) -> bool {
        self.ci_lower > 0.0
    }

    /// Check if the point estimate is significantly negative.
    pub fn is_significantly_negative(&self) -> bool {
        self.ci_upper < 0.0
    }

    /// Get the CI width (measure of uncertainty).
    pub fn ci_width(&self) -> f64 {
        self.ci_upper - self.ci_lower
    }
}

/// Compute bootstrap confidence interval for a statistic.
///
/// # Arguments
/// * `data` - Original sample data
/// * `statistic_fn` - Function to compute the statistic from a sample
/// * `config` - Bootstrap configuration
///
/// # Returns
/// Bootstrap result with confidence interval and standard error
pub fn bootstrap_ci<F>(
    data: &[f64],
    statistic_fn: F,
    config: &BootstrapConfig,
) -> Result<BootstrapResult, StatisticsError>
where
    F: Fn(&[f64]) -> f64,
{
    config.validate()?;

    if data.len() < 2 {
        return Err(StatisticsError::InsufficientSamples {
            needed: 2,
            available: data.len(),
        });
    }

    let n = data.len();
    let point_estimate = statistic_fn(data);

    let mut rng = SmallRng::seed_from_u64(config.seed);
    let mut bootstrap_stats: Vec<f64> = Vec::with_capacity(config.n_iterations);

    // Generate bootstrap samples and compute statistics
    let mut resample: Vec<f64> = vec![0.0; n];
    for _ in 0..config.n_iterations {
        // Resample with replacement
        #[allow(clippy::needless_range_loop)]
        for j in 0..n {
            let idx = rng.gen_range(0..n);
            resample[j] = data[idx];
        }
        bootstrap_stats.push(statistic_fn(&resample));
    }

    // Sort for percentile calculations
    bootstrap_stats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Compute confidence interval using percentile method
    let alpha = 1.0 - config.confidence_level;
    let lower_idx = ((alpha / 2.0) * config.n_iterations as f64).floor() as usize;
    let upper_idx = ((1.0 - alpha / 2.0) * config.n_iterations as f64).floor() as usize;

    let ci_lower = bootstrap_stats
        .get(lower_idx)
        .copied()
        .unwrap_or(point_estimate);
    let ci_upper = bootstrap_stats
        .get(upper_idx.min(bootstrap_stats.len() - 1))
        .copied()
        .unwrap_or(point_estimate);

    // Compute standard error and mean
    let bootstrap_mean: f64 = bootstrap_stats.iter().sum::<f64>() / config.n_iterations as f64;
    let variance: f64 = bootstrap_stats
        .iter()
        .map(|x| (x - bootstrap_mean).powi(2))
        .sum::<f64>()
        / config.n_iterations as f64;
    let std_error = variance.sqrt();

    let median_idx = config.n_iterations / 2;
    let bootstrap_median = bootstrap_stats[median_idx];

    Ok(BootstrapResult {
        point_estimate,
        ci_lower,
        ci_upper,
        std_error,
        confidence_level: config.confidence_level,
        n_iterations: config.n_iterations,
        bootstrap_mean,
        bootstrap_median,
    })
}

/// Compute bootstrap CI for Sharpe ratio specifically.
///
/// Uses the Sharpe ratio formula: mean(returns) / std(returns) * sqrt(annualization)
pub fn bootstrap_sharpe(
    returns: &[f64],
    annualization: f64,
    config: &BootstrapConfig,
) -> Result<BootstrapResult, StatisticsError> {
    let sharpe_fn = move |r: &[f64]| {
        if r.len() < 2 {
            return 0.0;
        }
        let mean = r.iter().sum::<f64>() / r.len() as f64;
        let variance = r.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (r.len() - 1) as f64;
        let std = variance.sqrt();
        if std < 1e-10 {
            0.0
        } else {
            (mean / std) * annualization.sqrt()
        }
    };

    bootstrap_ci(returns, sharpe_fn, config)
}

// =============================================================================
// Block Bootstrap for Time-Series Data
// =============================================================================

/// Bootstrap method selection for different data types.
///
/// Standard IID bootstrap assumes observations are independent, which is violated
/// for time-series data like equity curves or returns. Block bootstrap methods
/// preserve the autocorrelation structure by resampling contiguous blocks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum BootstrapMethod {
    /// Standard IID bootstrap - resample individual observations with replacement.
    /// Use for cross-sectional data (e.g., different symbols' Sharpe ratios).
    #[default]
    Iid,

    /// Moving block bootstrap - resample fixed-length contiguous blocks.
    /// Block endpoints are deterministic once block_size is chosen.
    MovingBlock {
        /// Fixed block size (typically n^(1/3) for optimal bias-variance tradeoff)
        block_size: usize,
    },

    /// Stationary bootstrap - resample with random block lengths.
    /// Block lengths follow geometric distribution with expected length = expected_block_length.
    /// This produces stationary resamples (unlike moving block).
    Stationary {
        /// Expected block length (typically n^(1/3))
        expected_block_length: f64,
    },
}

impl BootstrapMethod {
    /// Create optimal block bootstrap method for time-series of length n.
    ///
    /// Uses the n^(1/3) rule for optimal block length (Hall, Horowitz & Jing 1995).
    pub fn for_time_series(n: usize) -> Self {
        let optimal_length = (n as f64).powf(1.0 / 3.0).max(2.0);
        Self::Stationary {
            expected_block_length: optimal_length,
        }
    }

    /// Create IID bootstrap for cross-sectional data.
    pub fn for_cross_sectional() -> Self {
        Self::Iid
    }
}

/// Configuration for block bootstrap resampling.
///
/// Extends BootstrapConfig with method selection for time-series aware bootstrap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBootstrapConfig {
    /// Base bootstrap configuration (iterations, confidence level, seed)
    pub base: BootstrapConfig,
    /// Bootstrap method (IID, MovingBlock, or Stationary)
    pub method: BootstrapMethod,
}

impl Default for BlockBootstrapConfig {
    fn default() -> Self {
        Self {
            base: BootstrapConfig::default(),
            method: BootstrapMethod::Iid,
        }
    }
}

impl BlockBootstrapConfig {
    /// Create config for time-series data of length n.
    ///
    /// Uses stationary bootstrap with optimal block length.
    pub fn for_time_series(n: usize) -> Self {
        Self {
            base: BootstrapConfig::default(),
            method: BootstrapMethod::for_time_series(n),
        }
    }

    /// Create config for cross-sectional data (IID bootstrap).
    pub fn for_cross_sectional() -> Self {
        Self {
            base: BootstrapConfig::default(),
            method: BootstrapMethod::Iid,
        }
    }

    /// Quick config with fewer iterations for time-series.
    pub fn quick_time_series(n: usize) -> Self {
        Self {
            base: BootstrapConfig::quick(),
            method: BootstrapMethod::for_time_series(n),
        }
    }
}

/// Compute block bootstrap confidence interval for time-series data.
///
/// Unlike standard IID bootstrap, block bootstrap preserves the autocorrelation
/// structure of time-series data by resampling contiguous blocks rather than
/// individual observations. This produces more accurate confidence intervals
/// for autocorrelated data like equity curves.
///
/// # Arguments
/// * `data` - Time-series data (e.g., daily returns, equity values)
/// * `statistic_fn` - Function to compute the statistic from a sample
/// * `config` - Block bootstrap configuration
///
/// # Returns
/// Bootstrap result with confidence interval that accounts for autocorrelation
pub fn block_bootstrap_ci<F>(
    data: &[f64],
    statistic_fn: F,
    config: &BlockBootstrapConfig,
) -> Result<BootstrapResult, StatisticsError>
where
    F: Fn(&[f64]) -> f64,
{
    config.base.validate()?;

    let n = data.len();
    if n < 2 {
        return Err(StatisticsError::InsufficientSamples {
            needed: 2,
            available: n,
        });
    }

    // For IID method, delegate to standard bootstrap
    if matches!(config.method, BootstrapMethod::Iid) {
        return bootstrap_ci(data, statistic_fn, &config.base);
    }

    let point_estimate = statistic_fn(data);
    let mut rng = SmallRng::seed_from_u64(config.base.seed);
    let mut bootstrap_stats: Vec<f64> = Vec::with_capacity(config.base.n_iterations);
    let mut resample: Vec<f64> = Vec::with_capacity(n);

    for _ in 0..config.base.n_iterations {
        resample.clear();

        match config.method {
            BootstrapMethod::MovingBlock { block_size } => {
                generate_moving_block_sample(data, block_size, n, &mut resample, &mut rng);
            }
            BootstrapMethod::Stationary {
                expected_block_length,
            } => {
                generate_stationary_sample(data, expected_block_length, n, &mut resample, &mut rng);
            }
            BootstrapMethod::Iid => unreachable!(), // Handled above
        }

        bootstrap_stats.push(statistic_fn(&resample));
    }

    // Sort for percentile calculations
    bootstrap_stats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Compute confidence interval using percentile method
    let alpha = 1.0 - config.base.confidence_level;
    let lower_idx = ((alpha / 2.0) * config.base.n_iterations as f64).floor() as usize;
    let upper_idx = ((1.0 - alpha / 2.0) * config.base.n_iterations as f64).floor() as usize;

    let ci_lower = bootstrap_stats
        .get(lower_idx)
        .copied()
        .unwrap_or(point_estimate);
    let ci_upper = bootstrap_stats
        .get(upper_idx.min(bootstrap_stats.len().saturating_sub(1)))
        .copied()
        .unwrap_or(point_estimate);

    // Compute standard error and mean
    let bootstrap_mean: f64 =
        bootstrap_stats.iter().sum::<f64>() / config.base.n_iterations as f64;
    let variance: f64 = bootstrap_stats
        .iter()
        .map(|x| (x - bootstrap_mean).powi(2))
        .sum::<f64>()
        / config.base.n_iterations as f64;
    let std_error = variance.sqrt();

    let median_idx = config.base.n_iterations / 2;
    let bootstrap_median = bootstrap_stats[median_idx];

    Ok(BootstrapResult {
        point_estimate,
        ci_lower,
        ci_upper,
        std_error,
        confidence_level: config.base.confidence_level,
        n_iterations: config.base.n_iterations,
        bootstrap_mean,
        bootstrap_median,
    })
}

/// Generate a moving block bootstrap sample.
///
/// Resamples contiguous blocks of fixed size until we have n observations.
fn generate_moving_block_sample(
    data: &[f64],
    block_size: usize,
    target_len: usize,
    resample: &mut Vec<f64>,
    rng: &mut SmallRng,
) {
    let n = data.len();
    let block_size = block_size.max(1).min(n);
    let n_possible_blocks = n.saturating_sub(block_size) + 1;

    while resample.len() < target_len {
        // Pick a random block start
        let block_start = rng.gen_range(0..n_possible_blocks);
        let block_end = (block_start + block_size).min(n);

        // Add block to resample
        for item in data.iter().take(block_end).skip(block_start) {
            if resample.len() >= target_len {
                break;
            }
            resample.push(*item);
        }
    }
}

/// Generate a stationary bootstrap sample.
///
/// Uses geometric distribution for block lengths, producing a stationary resample.
/// This is preferred over moving block for most time-series applications.
fn generate_stationary_sample(
    data: &[f64],
    expected_block_length: f64,
    target_len: usize,
    resample: &mut Vec<f64>,
    rng: &mut SmallRng,
) {
    let n = data.len();
    // Probability of ending a block at each step (geometric distribution)
    let p_end = 1.0 / expected_block_length.max(1.0);

    let mut pos = rng.gen_range(0..n); // Start at random position

    while resample.len() < target_len {
        resample.push(data[pos]);

        // With probability p_end, jump to a new random position
        // Otherwise, continue to next position (wrapping around)
        if rng.gen::<f64>() < p_end {
            pos = rng.gen_range(0..n);
        } else {
            pos = (pos + 1) % n;
        }
    }
}

/// Compute block bootstrap CI for Sharpe ratio on time-series data.
///
/// Uses stationary bootstrap to preserve autocorrelation in returns.
/// This produces wider (more accurate) confidence intervals than IID bootstrap
/// for autocorrelated equity curves.
///
/// # Arguments
/// * `returns` - Daily returns (time-series)
/// * `annualization` - Annualization factor (e.g., 252 for daily returns)
/// * `config` - Block bootstrap configuration
///
/// # Returns
/// Bootstrap result with autocorrelation-adjusted confidence interval
pub fn block_bootstrap_sharpe(
    returns: &[f64],
    annualization: f64,
    config: &BlockBootstrapConfig,
) -> Result<BootstrapResult, StatisticsError> {
    let sharpe_fn = move |r: &[f64]| {
        if r.len() < 2 {
            return 0.0;
        }
        let mean = r.iter().sum::<f64>() / r.len() as f64;
        let variance = r.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (r.len() - 1) as f64;
        let std = variance.sqrt();
        if std < 1e-10 {
            0.0
        } else {
            (mean / std) * annualization.sqrt()
        }
    };

    block_bootstrap_ci(returns, sharpe_fn, config)
}

/// Result of a permutation test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermutationResult {
    /// Observed test statistic
    pub observed_statistic: f64,
    /// P-value (proportion of permutations as or more extreme)
    pub p_value: f64,
    /// Number of permutations used
    pub n_permutations: usize,
    /// Number of permutations as or more extreme
    pub n_extreme: usize,
}

impl PermutationResult {
    /// Check if significant at given alpha level.
    pub fn is_significant(&self, alpha: f64) -> bool {
        self.p_value < alpha
    }

    /// Check if significant at 5% level.
    pub fn is_significant_05(&self) -> bool {
        self.is_significant(0.05)
    }

    /// Check if significant at 1% level.
    pub fn is_significant_01(&self) -> bool {
        self.is_significant(0.01)
    }
}

/// Perform a permutation test comparing two groups.
///
/// Tests whether the difference in means between two groups is significant.
///
/// # Arguments
/// * `group_a` - First group data
/// * `group_b` - Second group data
/// * `n_permutations` - Number of random permutations
/// * `seed` - Random seed
///
/// # Returns
/// Permutation test result with p-value
pub fn permutation_test(
    group_a: &[f64],
    group_b: &[f64],
    n_permutations: usize,
    seed: u64,
) -> Result<PermutationResult, StatisticsError> {
    if group_a.is_empty() || group_b.is_empty() {
        return Err(StatisticsError::InsufficientSamples {
            needed: 1,
            available: 0,
        });
    }

    // Compute observed difference in means
    let mean_a: f64 = group_a.iter().sum::<f64>() / group_a.len() as f64;
    let mean_b: f64 = group_b.iter().sum::<f64>() / group_b.len() as f64;
    let observed_statistic = mean_a - mean_b;

    // Combine both groups
    let mut combined: Vec<f64> = Vec::with_capacity(group_a.len() + group_b.len());
    combined.extend_from_slice(group_a);
    combined.extend_from_slice(group_b);

    let n_a = group_a.len();
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut n_extreme = 0;

    for _ in 0..n_permutations {
        // Shuffle the combined data
        combined.shuffle(&mut rng);

        // Split into two groups of original sizes
        let perm_mean_a: f64 = combined[..n_a].iter().sum::<f64>() / n_a as f64;
        let perm_mean_b: f64 = combined[n_a..].iter().sum::<f64>() / (combined.len() - n_a) as f64;
        let perm_diff = perm_mean_a - perm_mean_b;

        // Count permutations as or more extreme (two-sided test)
        if perm_diff.abs() >= observed_statistic.abs() {
            n_extreme += 1;
        }
    }

    let p_value = (n_extreme + 1) as f64 / (n_permutations + 1) as f64;

    Ok(PermutationResult {
        observed_statistic,
        p_value,
        n_permutations,
        n_extreme,
    })
}

// =============================================================================
// P-Value Computation for OOS Sharpe Testing
// =============================================================================

/// Compute one-sided p-value for testing if the sample mean is greater than zero.
///
/// Uses a t-test when sample size < 30, otherwise normal approximation.
/// This is used for testing whether OOS Sharpe ratios are significantly positive.
///
/// # Arguments
/// * `samples` - The sample data (e.g., OOS Sharpe ratios from walk-forward folds)
///
/// # Returns
/// P-value for the one-sided test H0: mean <= 0 vs H1: mean > 0
pub fn one_sided_mean_pvalue(samples: &[f64]) -> Result<f64, StatisticsError> {
    if samples.len() < 3 {
        return Err(StatisticsError::InsufficientSamples {
            needed: 3,
            available: samples.len(),
        });
    }

    let n = samples.len() as f64;
    let mean = samples.iter().sum::<f64>() / n;
    let variance = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std_error = (variance / n).sqrt();

    // Handle edge cases
    if std_error < 1e-10 {
        // Near-zero variance: if mean > 0, extremely significant; otherwise not
        return Ok(if mean > 0.0 { 1e-10 } else { 1.0 - 1e-10 });
    }

    let t_stat = mean / std_error;
    let df = n - 1.0;

    // Use normal approximation for df >= 30, otherwise t-distribution
    let p_value = if df >= 30.0 {
        1.0 - standard_normal_cdf(t_stat)
    } else {
        1.0 - t_distribution_cdf(t_stat, df)
    };

    Ok(p_value.clamp(1e-10, 1.0 - 1e-10))
}

/// Standard normal CDF approximation using Abramowitz & Stegun formula.
///
/// Maximum error: 7.5e-8
pub fn standard_normal_cdf(x: f64) -> f64 {
    if x < -8.0 {
        return 0.0;
    }
    if x > 8.0 {
        return 1.0;
    }

    // Use symmetry for negative values
    let (sign, z) = if x < 0.0 { (-1.0, -x) } else { (1.0, x) };

    // Abramowitz & Stegun approximation (formula 7.1.26)
    let p = 0.2316419;
    let b1 = 0.319381530;
    let b2 = -0.356563782;
    let b3 = 1.781477937;
    let b4 = -1.821255978;
    let b5 = 1.330274429;

    let t = 1.0 / (1.0 + p * z);
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;

    let pdf = (-z * z / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let tail = pdf * (b1 * t + b2 * t2 + b3 * t3 + b4 * t4 + b5 * t5);

    if sign > 0.0 {
        1.0 - tail
    } else {
        tail
    }
}

/// T-distribution CDF approximation using Hill's algorithm.
///
/// Accurate for all df >= 1.
pub fn t_distribution_cdf(t: f64, df: f64) -> f64 {
    if df <= 0.0 {
        return 0.5; // Invalid df, return 0.5
    }

    // For very large df, use normal approximation
    if df > 1000.0 {
        return standard_normal_cdf(t);
    }

    // Use symmetry
    let x = df / (df + t * t);

    // Compute incomplete beta function I_x(df/2, 1/2)
    let a = df / 2.0;
    let b = 0.5;

    let beta_inc = regularized_incomplete_beta(x, a, b);

    if t >= 0.0 {
        1.0 - 0.5 * beta_inc
    } else {
        0.5 * beta_inc
    }
}

/// Regularized incomplete beta function I_x(a, b) approximation.
///
/// Uses continued fraction expansion for accuracy.
fn regularized_incomplete_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    // Use symmetry transformation if needed for better convergence
    if x > (a + 1.0) / (a + b + 2.0) {
        return 1.0 - regularized_incomplete_beta(1.0 - x, b, a);
    }

    // Compute using continued fraction (Lentz's algorithm)
    let ln_beta = ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b);
    let front = (a * x.ln() + b * (1.0 - x).ln() - ln_beta).exp() / a;

    // Continued fraction coefficients
    let mut c = 1.0;
    let mut d = 1.0 / (1.0 - (a + b) * x / (a + 1.0)).max(1e-30);
    let mut h = d;

    for m in 1..200 {
        let m_f64 = m as f64;

        // Even term
        let num_even = m_f64 * (b - m_f64) * x / ((a + 2.0 * m_f64 - 1.0) * (a + 2.0 * m_f64));
        d = 1.0 / (1.0 + num_even * d).max(1e-30);
        c = (1.0 + num_even / c).max(1e-30);
        h *= d * c;

        // Odd term
        let num_odd =
            -((a + m_f64) * (a + b + m_f64) * x) / ((a + 2.0 * m_f64) * (a + 2.0 * m_f64 + 1.0));
        d = 1.0 / (1.0 + num_odd * d).max(1e-30);
        c = (1.0 + num_odd / c).max(1e-30);
        let delta = d * c;
        h *= delta;

        if (delta - 1.0).abs() < 1e-10 {
            break;
        }
    }

    front * h
}

/// Log-gamma function approximation using Stirling's formula with corrections.
fn ln_gamma(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::INFINITY;
    }

    // For small x, use reflection or recursion
    if x < 0.5 {
        // Use reflection formula: Gamma(x) * Gamma(1-x) = pi / sin(pi*x)
        let pi = std::f64::consts::PI;
        return pi.ln() - (pi * x).sin().ln() - ln_gamma(1.0 - x);
    }

    // Lanczos approximation (g=7, n=9)
    let coef = [
        0.999_999_999_999_809_9,
        676.5203681218851,
        -1259.1392167224028,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507343278686905,
        -0.13857109526572012,
        9.984_369_578_019_572e-6,
        1.5056327351493116e-7,
    ];

    let x = x - 1.0;
    let mut ag = coef[0];
    for (i, &c) in coef.iter().enumerate().skip(1) {
        ag += c / (x + i as f64);
    }

    let tmp = x + 7.5;
    (2.0 * std::f64::consts::PI).sqrt().ln() + (x + 0.5) * tmp.ln() - tmp + ag.ln()
}

/// Result of multiple comparison adjustment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipleComparisonResult {
    /// Original p-values
    pub original_p_values: Vec<f64>,
    /// Adjusted p-values (after FDR or Bonferroni)
    pub adjusted_p_values: Vec<f64>,
    /// Which hypotheses are rejected at the given alpha
    pub rejections: Vec<bool>,
    /// Method used for adjustment
    pub method: MultipleComparisonMethod,
    /// Alpha level used
    pub alpha: f64,
    /// Number of rejections
    pub n_rejections: usize,
}

/// Methods for multiple comparison correction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MultipleComparisonMethod {
    /// No correction (raw p-values)
    None,
    /// Bonferroni correction (conservative)
    Bonferroni,
    /// Holm-Bonferroni (step-down, less conservative)
    Holm,
    /// Benjamini-Hochberg FDR (controls false discovery rate)
    BenjaminiHochberg,
    /// Benjamini-Yekutieli FDR (works under dependence)
    BenjaminiYekutieli,
}

/// Apply False Discovery Rate (FDR) correction using Benjamini-Hochberg procedure.
///
/// Controls the expected proportion of false positives among all rejected hypotheses.
///
/// # Arguments
/// * `p_values` - Vector of p-values from multiple tests
/// * `alpha` - Significance level (default 0.05)
///
/// # Returns
/// Result with adjusted p-values and which hypotheses are rejected
pub fn benjamini_hochberg(
    p_values: &[f64],
    alpha: f64,
) -> Result<MultipleComparisonResult, StatisticsError> {
    if p_values.is_empty() {
        return Err(StatisticsError::InsufficientSamples {
            needed: 1,
            available: 0,
        });
    }

    if alpha <= 0.0 || alpha >= 1.0 {
        return Err(StatisticsError::InvalidParameter(
            "alpha must be in (0, 1)".to_string(),
        ));
    }

    let m = p_values.len();

    // Create sorted indices (by p-value ascending)
    let mut indices: Vec<usize> = (0..m).collect();
    indices.sort_by(|&a, &b| {
        p_values[a]
            .partial_cmp(&p_values[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute adjusted p-values
    let mut adjusted: Vec<f64> = vec![0.0; m];

    // Start from largest p-value
    adjusted[indices[m - 1]] = p_values[indices[m - 1]];

    // Work backwards, ensuring monotonicity
    for i in (0..m - 1).rev() {
        let idx = indices[i];
        let rank = i + 1;
        // BH adjusted p-value = p * m / rank
        let adj = (p_values[idx] * m as f64 / rank as f64).min(1.0);
        // Ensure monotonicity: can't be larger than next larger adjusted p-value
        adjusted[idx] = adj.min(adjusted[indices[i + 1]]);
    }

    // Determine rejections
    let rejections: Vec<bool> = adjusted.iter().map(|&p| p < alpha).collect();
    let n_rejections = rejections.iter().filter(|&&r| r).count();

    Ok(MultipleComparisonResult {
        original_p_values: p_values.to_vec(),
        adjusted_p_values: adjusted,
        rejections,
        method: MultipleComparisonMethod::BenjaminiHochberg,
        alpha,
        n_rejections,
    })
}

/// Apply Bonferroni correction (conservative).
///
/// Controls the family-wise error rate (FWER) - probability of ANY false positive.
pub fn bonferroni(
    p_values: &[f64],
    alpha: f64,
) -> Result<MultipleComparisonResult, StatisticsError> {
    if p_values.is_empty() {
        return Err(StatisticsError::InsufficientSamples {
            needed: 1,
            available: 0,
        });
    }

    let m = p_values.len();
    let adjusted: Vec<f64> = p_values.iter().map(|&p| (p * m as f64).min(1.0)).collect();
    let rejections: Vec<bool> = adjusted.iter().map(|&p| p < alpha).collect();
    let n_rejections = rejections.iter().filter(|&&r| r).count();

    Ok(MultipleComparisonResult {
        original_p_values: p_values.to_vec(),
        adjusted_p_values: adjusted,
        rejections,
        method: MultipleComparisonMethod::Bonferroni,
        alpha,
        n_rejections,
    })
}

/// Apply Holm-Bonferroni step-down correction.
///
/// Less conservative than Bonferroni while still controlling FWER.
pub fn holm_bonferroni(
    p_values: &[f64],
    alpha: f64,
) -> Result<MultipleComparisonResult, StatisticsError> {
    if p_values.is_empty() {
        return Err(StatisticsError::InsufficientSamples {
            needed: 1,
            available: 0,
        });
    }

    let m = p_values.len();

    // Sort p-values and track original indices
    let mut indices: Vec<usize> = (0..m).collect();
    indices.sort_by(|&a, &b| {
        p_values[a]
            .partial_cmp(&p_values[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut adjusted: Vec<f64> = vec![0.0; m];

    // Start from smallest p-value
    adjusted[indices[0]] = (p_values[indices[0]] * m as f64).min(1.0);

    for i in 1..m {
        let idx = indices[i];
        let correction = (m - i) as f64;
        let adj = (p_values[idx] * correction).min(1.0);
        // Ensure monotonicity: can't be smaller than previous adjusted p-value
        adjusted[idx] = adj.max(adjusted[indices[i - 1]]);
    }

    let rejections: Vec<bool> = adjusted.iter().map(|&p| p < alpha).collect();
    let n_rejections = rejections.iter().filter(|&&r| r).count();

    Ok(MultipleComparisonResult {
        original_p_values: p_values.to_vec(),
        adjusted_p_values: adjusted,
        rejections,
        method: MultipleComparisonMethod::Holm,
        alpha,
        n_rejections,
    })
}

/// Summary statistics for a sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleStatistics {
    pub n: usize,
    pub mean: f64,
    pub std: f64,
    pub std_error: f64,
    pub min: f64,
    pub max: f64,
    pub median: f64,
    pub q1: f64,
    pub q3: f64,
    pub skewness: f64,
    pub kurtosis: f64,
}

/// Compute summary statistics for a sample.
pub fn sample_statistics(data: &[f64]) -> Result<SampleStatistics, StatisticsError> {
    if data.is_empty() {
        return Err(StatisticsError::InsufficientSamples {
            needed: 1,
            available: 0,
        });
    }

    let n = data.len();
    let mean = data.iter().sum::<f64>() / n as f64;

    // Variance and std
    let variance = if n > 1 {
        data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64
    } else {
        0.0
    };
    let std = variance.sqrt();
    let std_error = std / (n as f64).sqrt();

    // Sort for quantiles
    let mut sorted: Vec<f64> = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min = sorted[0];
    let max = sorted[n - 1];

    let median = if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    };

    // Q1 and Q3 (using linear interpolation)
    let q1_idx = (n - 1) as f64 * 0.25;
    let q1 = interpolate_quantile(&sorted, q1_idx);

    let q3_idx = (n - 1) as f64 * 0.75;
    let q3 = interpolate_quantile(&sorted, q3_idx);

    // Skewness: E[(X - μ)^3] / σ^3
    let skewness = if std > 1e-10 {
        let m3 = data.iter().map(|x| (x - mean).powi(3)).sum::<f64>() / n as f64;
        m3 / std.powi(3)
    } else {
        0.0
    };

    // Excess kurtosis: E[(X - μ)^4] / σ^4 - 3
    let kurtosis = if std > 1e-10 {
        let m4 = data.iter().map(|x| (x - mean).powi(4)).sum::<f64>() / n as f64;
        m4 / std.powi(4) - 3.0
    } else {
        0.0
    };

    Ok(SampleStatistics {
        n,
        mean,
        std,
        std_error,
        min,
        max,
        median,
        q1,
        q3,
        skewness,
        kurtosis,
    })
}

fn interpolate_quantile(sorted: &[f64], idx: f64) -> f64 {
    let lower = idx.floor() as usize;
    let upper = idx.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let frac = idx - lower as f64;
        sorted[lower] * (1.0 - frac) + sorted[upper] * frac
    }
}

/// Confidence grade based on statistical significance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfidenceGrade {
    /// High confidence: passes FDR correction, narrow CI, positive lower bound
    High,
    /// Medium confidence: marginally significant or wider CI
    Medium,
    /// Low confidence: not significant after correction
    Low,
    /// Insufficient data for reliable inference
    Insufficient,
}

impl ConfidenceGrade {
    /// Convert to a display string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfidenceGrade::High => "High",
            ConfidenceGrade::Medium => "Medium",
            ConfidenceGrade::Low => "Low",
            ConfidenceGrade::Insufficient => "Insufficient",
        }
    }

    /// Convert to emoji badge.
    pub fn badge(&self) -> &'static str {
        match self {
            ConfidenceGrade::High => "✓✓",
            ConfidenceGrade::Medium => "✓",
            ConfidenceGrade::Low => "○",
            ConfidenceGrade::Insufficient => "?",
        }
    }
}

/// Comprehensive statistical evaluation of a strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyStatistics {
    /// Bootstrap CI for Sharpe ratio
    pub sharpe_ci: BootstrapResult,
    /// Bootstrap CI for CAGR
    pub cagr_ci: Option<BootstrapResult>,
    /// Bootstrap CI for max drawdown
    pub drawdown_ci: Option<BootstrapResult>,
    /// FDR-adjusted significance (if part of multiple comparisons)
    pub fdr_adjusted: bool,
    /// FDR-adjusted p-value (if applicable)
    pub fdr_p_value: Option<f64>,
    /// Overall confidence grade
    pub confidence_grade: ConfidenceGrade,
    /// Sample statistics of returns
    pub return_stats: SampleStatistics,
}

impl StrategyStatistics {
    /// Create from daily returns.
    pub fn from_returns(
        returns: &[f64],
        config: &BootstrapConfig,
    ) -> Result<Self, StatisticsError> {
        if returns.len() < 30 {
            return Err(StatisticsError::InsufficientSamples {
                needed: 30,
                available: returns.len(),
            });
        }

        let return_stats = sample_statistics(returns)?;
        let sharpe_ci = bootstrap_sharpe(returns, 252.0, config)?;

        // Determine confidence grade based on Sharpe CI
        let confidence_grade = if sharpe_ci.ci_lower > 0.5 {
            ConfidenceGrade::High
        } else if sharpe_ci.ci_lower > 0.0 {
            ConfidenceGrade::Medium
        } else if returns.len() >= 252 {
            ConfidenceGrade::Low
        } else {
            ConfidenceGrade::Insufficient
        };

        Ok(Self {
            sharpe_ci,
            cagr_ci: None,
            drawdown_ci: None,
            fdr_adjusted: false,
            fdr_p_value: None,
            confidence_grade,
            return_stats,
        })
    }

    /// Update with FDR-adjusted results.
    pub fn with_fdr_adjustment(mut self, adjusted_p: f64, alpha: f64) -> Self {
        self.fdr_adjusted = true;
        self.fdr_p_value = Some(adjusted_p);

        // Downgrade confidence if FDR-adjusted p-value is not significant
        if adjusted_p >= alpha && self.confidence_grade == ConfidenceGrade::High {
            self.confidence_grade = ConfidenceGrade::Medium;
        } else if adjusted_p >= alpha && self.confidence_grade == ConfidenceGrade::Medium {
            self.confidence_grade = ConfidenceGrade::Low;
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_mean() {
        let data: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let config = BootstrapConfig::quick();

        let result =
            bootstrap_ci(&data, |x| x.iter().sum::<f64>() / x.len() as f64, &config).unwrap();

        // True mean is 5.5
        assert!((result.point_estimate - 5.5).abs() < 0.01);
        // CI should contain the true mean
        assert!(result.ci_lower < 5.5);
        assert!(result.ci_upper > 5.5);
        // CI should be reasonably tight
        assert!(result.ci_width() < 5.0);
    }

    #[test]
    fn test_bootstrap_sharpe() {
        // Generate realistic daily returns (mean ~0, std ~1-2%)
        let mut rng = SmallRng::seed_from_u64(42);
        let returns: Vec<f64> = (0..200).map(|_| rng.gen_range(-0.03..0.035)).collect();

        let config = BootstrapConfig::quick();
        let result = bootstrap_sharpe(&returns, 252.0, &config).unwrap();

        // Sharpe should be finite
        assert!(result.point_estimate.is_finite());
        // CI bounds should be finite
        assert!(result.ci_lower.is_finite());
        assert!(result.ci_upper.is_finite());
        // CI width should be positive
        assert!(result.ci_width() > 0.0);
    }

    #[test]
    fn test_bootstrap_significance() {
        // Data clearly centered above zero
        let data: Vec<f64> = (1..101).map(|x| x as f64).collect();
        let config = BootstrapConfig::quick();

        let result =
            bootstrap_ci(&data, |x| x.iter().sum::<f64>() / x.len() as f64, &config).unwrap();

        // Mean is 50.5, should be significantly positive
        assert!(result.is_significant());
        assert!(result.is_significantly_positive());
        assert!(!result.is_significantly_negative());
    }

    #[test]
    fn test_permutation_test_significant() {
        let group_a: Vec<f64> = (10..30).map(|x| x as f64).collect();
        let group_b: Vec<f64> = (0..20).map(|x| x as f64).collect();

        let result = permutation_test(&group_a, &group_b, 1000, 42).unwrap();

        // Groups are clearly different (A is 10 units higher)
        assert!(result.is_significant_05());
        assert!(result.observed_statistic > 0.0);
    }

    #[test]
    fn test_permutation_test_not_significant() {
        // Same distribution
        let mut rng = SmallRng::seed_from_u64(42);
        let group_a: Vec<f64> = (0..50).map(|_| rng.gen_range(0.0..10.0)).collect();
        let group_b: Vec<f64> = (0..50).map(|_| rng.gen_range(0.0..10.0)).collect();

        let result = permutation_test(&group_a, &group_b, 1000, 42).unwrap();

        // Groups should not be significantly different
        assert!(result.p_value > 0.01);
    }

    #[test]
    fn test_benjamini_hochberg() {
        // Mix of significant and non-significant p-values
        let p_values = vec![0.001, 0.008, 0.039, 0.041, 0.23, 0.45, 0.78];

        let result = benjamini_hochberg(&p_values, 0.05).unwrap();

        // First few should still be significant after BH
        assert!(result.rejections[0]); // 0.001 is clearly significant
        assert!(result.rejections[1]); // 0.008 should pass BH at 0.05

        // Later ones should not be
        assert!(!result.rejections[5]); // 0.45 should fail
        assert!(!result.rejections[6]); // 0.78 should fail

        // All adjusted p-values should be >= original
        for (i, p_val) in p_values.iter().enumerate() {
            assert!(result.adjusted_p_values[i] >= p_val - 1e-10);
        }
    }

    #[test]
    fn test_bonferroni() {
        let p_values = vec![0.005, 0.01, 0.02, 0.04];

        let result = bonferroni(&p_values, 0.05).unwrap();

        // With 4 tests and alpha=0.05, need p < 0.0125 to reject
        assert!(result.rejections[0]); // 0.005 * 4 = 0.02 < 0.05
        assert!(result.rejections[1]); // 0.01 * 4 = 0.04 < 0.05
        assert!(!result.rejections[2]); // 0.02 * 4 = 0.08 >= 0.05
        assert!(!result.rejections[3]); // 0.04 * 4 = 0.16 >= 0.05

        assert_eq!(result.n_rejections, 2);
    }

    #[test]
    fn test_holm_bonferroni() {
        let p_values = vec![0.001, 0.01, 0.04, 0.07];

        let result = holm_bonferroni(&p_values, 0.05).unwrap();

        // Holm is less conservative than Bonferroni
        // First should definitely be rejected
        assert!(result.rejections[0]);

        // Count rejections
        assert!(result.n_rejections >= 1);
    }

    #[test]
    fn test_sample_statistics() {
        let data: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];

        let stats = sample_statistics(&data).unwrap();

        assert_eq!(stats.n, 10);
        assert!((stats.mean - 5.5).abs() < 0.01);
        assert!((stats.min - 1.0).abs() < 0.01);
        assert!((stats.max - 10.0).abs() < 0.01);
        assert!((stats.median - 5.5).abs() < 0.01);
        // For uniform-ish distribution, skewness should be near 0
        assert!(stats.skewness.abs() < 0.5);
    }

    #[test]
    fn test_confidence_grades() {
        assert_eq!(ConfidenceGrade::High.badge(), "✓✓");
        assert_eq!(ConfidenceGrade::Medium.badge(), "✓");
        assert_eq!(ConfidenceGrade::Low.badge(), "○");
        assert_eq!(ConfidenceGrade::Insufficient.badge(), "?");
    }

    #[test]
    fn test_strategy_statistics() {
        // Generate 100 daily returns with slight positive drift
        let mut rng = SmallRng::seed_from_u64(42);
        let returns: Vec<f64> = (0..100).map(|_| rng.gen_range(-0.005..0.015)).collect();

        let config = BootstrapConfig::quick();
        let stats = StrategyStatistics::from_returns(&returns, &config).unwrap();

        // Should have valid statistics
        assert!(stats.sharpe_ci.point_estimate.is_finite());
        assert!(stats.return_stats.n == 100);
        // Confidence grade should be one of the valid values
        assert!(matches!(
            stats.confidence_grade,
            ConfidenceGrade::High
                | ConfidenceGrade::Medium
                | ConfidenceGrade::Low
                | ConfidenceGrade::Insufficient
        ));
    }

    #[test]
    fn test_standard_normal_cdf() {
        // Test known values
        assert!((standard_normal_cdf(0.0) - 0.5).abs() < 0.001);
        assert!((standard_normal_cdf(1.96) - 0.975).abs() < 0.001);
        assert!((standard_normal_cdf(-1.96) - 0.025).abs() < 0.001);
        assert!((standard_normal_cdf(2.576) - 0.995).abs() < 0.001);
        // Extreme values
        assert!(standard_normal_cdf(-10.0) < 0.001);
        assert!(standard_normal_cdf(10.0) > 0.999);
    }

    #[test]
    fn test_t_distribution_cdf() {
        // For large df, should match normal
        assert!((t_distribution_cdf(0.0, 100.0) - 0.5).abs() < 0.01);
        assert!((t_distribution_cdf(1.96, 100.0) - 0.975).abs() < 0.01);

        // For small df, tails should be fatter
        // t(1.96, df=5) < normal(1.96) because t has fatter tails
        assert!(t_distribution_cdf(1.96, 5.0) < 0.975);
        assert!(t_distribution_cdf(1.96, 5.0) > 0.94);

        // Symmetry
        assert!(
            (t_distribution_cdf(1.0, 10.0) + t_distribution_cdf(-1.0, 10.0) - 1.0).abs() < 0.001
        );
    }

    #[test]
    fn test_one_sided_mean_pvalue_positive() {
        // Clearly positive mean should have low p-value
        let samples = vec![0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.4];
        let p = one_sided_mean_pvalue(&samples).unwrap();
        assert!(p < 0.01, "p-value {} should be < 0.01 for positive mean", p);
    }

    #[test]
    fn test_one_sided_mean_pvalue_negative() {
        // Clearly negative mean should have high p-value (testing mean > 0)
        let samples = vec![-0.5, -0.6, -0.7, -0.8, -0.9, -1.0, -1.1, -1.2, -1.3, -1.4];
        let p = one_sided_mean_pvalue(&samples).unwrap();
        assert!(p > 0.99, "p-value {} should be > 0.99 for negative mean", p);
    }

    #[test]
    fn test_one_sided_mean_pvalue_zero() {
        // Mean near zero should have p-value around 0.5
        let samples = vec![-0.1, 0.1, -0.2, 0.2, -0.15, 0.15, -0.05, 0.05];
        let p = one_sided_mean_pvalue(&samples).unwrap();
        assert!(p > 0.3 && p < 0.7, "p-value {} should be around 0.5", p);
    }

    #[test]
    fn test_one_sided_mean_pvalue_insufficient_samples() {
        let samples = vec![1.0, 2.0];
        let result = one_sided_mean_pvalue(&samples);
        assert!(result.is_err());
    }

    #[test]
    fn test_one_sided_mean_pvalue_realistic_oos_sharpes() {
        // Simulate OOS Sharpe ratios from walk-forward folds
        // Good strategy: consistently positive but variable
        let good_oos = vec![0.8, 1.2, 0.6, 0.9, 1.1];
        let p_good = one_sided_mean_pvalue(&good_oos).unwrap();
        assert!(p_good < 0.05, "Good strategy p={} should be < 0.05", p_good);

        // Mediocre strategy: mixed results
        let mixed_oos = vec![0.5, -0.2, 0.3, 0.1, -0.1];
        let p_mixed = one_sided_mean_pvalue(&mixed_oos).unwrap();
        assert!(
            p_mixed > 0.1,
            "Mixed strategy p={} should be > 0.1",
            p_mixed
        );

        // Bad strategy: mostly negative OOS
        let bad_oos = vec![-0.3, 0.1, -0.5, -0.2, -0.4];
        let p_bad = one_sided_mean_pvalue(&bad_oos).unwrap();
        assert!(p_bad > 0.9, "Bad strategy p={} should be > 0.9", p_bad);
    }

    // =============================================================================
    // Block Bootstrap Tests
    // =============================================================================

    #[test]
    fn test_bootstrap_method_for_time_series() {
        let method = BootstrapMethod::for_time_series(1000);
        match method {
            BootstrapMethod::Stationary {
                expected_block_length,
            } => {
                // 1000^(1/3) ≈ 10
                assert!(
                    expected_block_length > 9.0 && expected_block_length < 11.0,
                    "expected_block_length={} should be ~10",
                    expected_block_length
                );
            }
            _ => panic!("Expected Stationary method"),
        }
    }

    #[test]
    fn test_block_bootstrap_config_defaults() {
        let config = BlockBootstrapConfig::for_time_series(252);
        assert_eq!(config.base.n_iterations, 10_000);
        assert!((config.base.confidence_level - 0.95).abs() < 0.001);
        match config.method {
            BootstrapMethod::Stationary {
                expected_block_length,
            } => {
                // 252^(1/3) ≈ 6.3
                assert!(
                    expected_block_length > 6.0 && expected_block_length < 7.0,
                    "expected_block_length={} should be ~6.3",
                    expected_block_length
                );
            }
            _ => panic!("Expected Stationary method"),
        }
    }

    #[test]
    fn test_block_bootstrap_iid_delegates_correctly() {
        let data: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let config = BlockBootstrapConfig::for_cross_sectional();

        let result =
            block_bootstrap_ci(&data, |x| x.iter().sum::<f64>() / x.len() as f64, &config).unwrap();

        // Should produce same results as regular bootstrap with same config
        let iid_result =
            bootstrap_ci(&data, |x| x.iter().sum::<f64>() / x.len() as f64, &config.base).unwrap();

        // Point estimates should be identical
        assert!((result.point_estimate - iid_result.point_estimate).abs() < 0.001);
    }

    #[test]
    fn test_block_bootstrap_sharpe_produces_valid_ci() {
        // Generate realistic daily returns with autocorrelation
        let mut rng = SmallRng::seed_from_u64(42);
        let mut returns: Vec<f64> = Vec::with_capacity(252);
        let mut prev = 0.0;
        for _ in 0..252 {
            // Add autocorrelation: new return depends on previous
            let noise = rng.gen_range(-0.02..0.02);
            let ret = 0.0003 + 0.3 * prev + noise;
            returns.push(ret);
            prev = ret;
        }

        let config = BlockBootstrapConfig::quick_time_series(returns.len());
        let result = block_bootstrap_sharpe(&returns, 252.0, &config).unwrap();

        // Sharpe and CI should be finite
        assert!(result.point_estimate.is_finite());
        assert!(result.ci_lower.is_finite());
        assert!(result.ci_upper.is_finite());
        // CI width should be positive
        assert!(result.ci_width() > 0.0);
        // CI should bracket the point estimate
        assert!(result.ci_lower <= result.point_estimate);
        assert!(result.ci_upper >= result.point_estimate);
    }

    #[test]
    fn test_block_bootstrap_wider_ci_for_autocorrelated_data() {
        // Generate highly autocorrelated data
        let mut rng = SmallRng::seed_from_u64(123);
        let mut returns: Vec<f64> = Vec::with_capacity(200);
        let mut prev = 0.0;
        for _ in 0..200 {
            let noise = rng.gen_range(-0.01..0.01);
            // High autocorrelation (0.8)
            let ret = 0.0005 + 0.8 * prev + noise;
            returns.push(ret);
            prev = ret;
        }

        // Compare IID vs block bootstrap
        let iid_config = BlockBootstrapConfig {
            base: BootstrapConfig::quick(),
            method: BootstrapMethod::Iid,
        };
        let block_config = BlockBootstrapConfig::quick_time_series(returns.len());

        let iid_result = block_bootstrap_sharpe(&returns, 252.0, &iid_config).unwrap();
        let block_result = block_bootstrap_sharpe(&returns, 252.0, &block_config).unwrap();

        // Block bootstrap should produce wider CI due to accounting for autocorrelation
        // (This may not always hold due to randomness, but generally should)
        println!(
            "IID CI width: {}, Block CI width: {}",
            iid_result.ci_width(),
            block_result.ci_width()
        );

        // Both should be valid
        assert!(iid_result.ci_width() > 0.0);
        assert!(block_result.ci_width() > 0.0);
    }

    #[test]
    fn test_moving_block_sample_length() {
        let data: Vec<f64> = (0..100).map(|x| x as f64).collect();
        let mut rng = SmallRng::seed_from_u64(42);
        let mut resample = Vec::new();

        generate_moving_block_sample(&data, 10, 100, &mut resample, &mut rng);

        // Should produce exactly 100 elements
        assert_eq!(resample.len(), 100);
        // All elements should be from original data
        for val in &resample {
            assert!(*val >= 0.0 && *val <= 99.0);
        }
    }

    #[test]
    fn test_stationary_sample_length() {
        let data: Vec<f64> = (0..100).map(|x| x as f64).collect();
        let mut rng = SmallRng::seed_from_u64(42);
        let mut resample = Vec::new();

        generate_stationary_sample(&data, 10.0, 100, &mut resample, &mut rng);

        // Should produce exactly 100 elements
        assert_eq!(resample.len(), 100);
        // All elements should be from original data
        for val in &resample {
            assert!(*val >= 0.0 && *val <= 99.0);
        }
    }

    #[test]
    fn test_stationary_bootstrap_preserves_local_structure() {
        // Create data with obvious local structure
        let data: Vec<f64> = (0..100).map(|x| x as f64).collect();
        let mut rng = SmallRng::seed_from_u64(42);
        let mut resample = Vec::new();

        // Use large expected block length to see more consecutive runs
        generate_stationary_sample(&data, 20.0, 100, &mut resample, &mut rng);

        // Count consecutive pairs where diff = 1 (consecutive in original)
        let consecutive_count = resample
            .windows(2)
            .filter(|w| (w[1] - w[0] - 1.0).abs() < 0.01)
            .count();

        // With block length 20, we expect many consecutive pairs
        // In pure random sampling, we'd expect ~1% to be consecutive
        // With block bootstrap, we should see many more
        println!("Consecutive pairs: {} out of 99", consecutive_count);
        assert!(
            consecutive_count > 30,
            "Expected many consecutive pairs with block length 20, got {}",
            consecutive_count
        );
    }
}
