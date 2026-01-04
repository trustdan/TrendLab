//! Latin Hypercube Sampling for efficient parameter space exploration.
//!
//! Latin Hypercube Sampling (LHS) is a stratified sampling technique that ensures
//! better coverage of the parameter space than pure random sampling. It works by:
//!
//! 1. Dividing each dimension into N equal-probability strata
//! 2. Sampling exactly one point from each stratum per dimension
//! 3. Randomly shuffling the strata assignments across dimensions
//!
//! This ensures that projections onto any single dimension have exactly one sample
//! in each stratum, providing better space-filling properties than random sampling.

use rand::seq::SliceRandom;
use rand::Rng;

/// Configuration for Latin Hypercube Sampling.
#[derive(Debug, Clone)]
pub struct LhsConfig {
    /// Number of samples to generate (also determines number of strata per dimension)
    pub n_samples: usize,
    /// Parameter bounds: Vec<(min, max, step)> for each dimension
    pub bounds: Vec<(f64, f64, f64)>,
}

impl LhsConfig {
    /// Create a new LHS configuration.
    pub fn new(n_samples: usize, bounds: Vec<(f64, f64, f64)>) -> Self {
        Self { n_samples, bounds }
    }

    /// Get the number of dimensions.
    pub fn n_dims(&self) -> usize {
        self.bounds.len()
    }
}

/// Latin Hypercube Sampler for multi-dimensional parameter spaces.
#[derive(Debug)]
pub struct LatinHypercubeSampler {
    config: LhsConfig,
}

impl LatinHypercubeSampler {
    /// Create a new sampler with the given configuration.
    pub fn new(config: LhsConfig) -> Self {
        Self { config }
    }

    /// Generate Latin Hypercube samples.
    ///
    /// Returns a Vec of sample points, where each point is a Vec<f64> of parameter values.
    /// The values are quantized to the step sizes specified in the bounds.
    pub fn sample<R: Rng>(&self, rng: &mut R) -> Vec<Vec<f64>> {
        let n = self.config.n_samples;
        let n_dims = self.config.n_dims();

        if n == 0 || n_dims == 0 {
            return Vec::new();
        }

        // Generate stratum indices for each dimension
        // Each dimension gets the indices [0, 1, 2, ..., n-1] shuffled independently
        let strata_indices: Vec<Vec<usize>> = (0..n_dims)
            .map(|_| {
                let mut indices: Vec<usize> = (0..n).collect();
                indices.shuffle(rng);
                indices
            })
            .collect();

        // Generate samples
        let mut samples = Vec::with_capacity(n);

        for sample_idx in 0..n {
            let mut point = Vec::with_capacity(n_dims);

            for (dim, stratum_vec) in strata_indices.iter().enumerate().take(n_dims) {
                let stratum_idx = stratum_vec[sample_idx];
                let (min, max, step) = self.config.bounds[dim];

                // Calculate stratum boundaries
                let stratum_width = (max - min) / n as f64;
                let stratum_min = min + stratum_idx as f64 * stratum_width;
                let stratum_max = stratum_min + stratum_width;

                // Sample uniformly within the stratum
                let raw_value = rng.gen_range(stratum_min..stratum_max);

                // Quantize to step size and clamp to bounds
                let quantized = if step > 0.0 {
                    let stepped = ((raw_value - min) / step).round() * step + min;
                    stepped.clamp(min, max)
                } else {
                    raw_value.clamp(min, max)
                };

                point.push(quantized);
            }

            samples.push(point);
        }

        samples
    }

    /// Generate Latin Hypercube samples with integer quantization for specific dimensions.
    ///
    /// `int_dims` specifies which dimensions should be rounded to integers.
    pub fn sample_with_ints<R: Rng>(&self, rng: &mut R, int_dims: &[usize]) -> Vec<Vec<f64>> {
        let mut samples = self.sample(rng);

        for sample in &mut samples {
            for &dim in int_dims {
                if dim < sample.len() {
                    sample[dim] = sample[dim].round();
                }
            }
        }

        samples
    }
}

/// Generate a batch of LHS samples for a strategy's parameter space.
///
/// This is a convenience function that takes parameter bounds and generates
/// a set of well-distributed samples using Latin Hypercube Sampling.
///
/// # Arguments
/// * `n_samples` - Number of samples to generate
/// * `bounds` - Parameter bounds as Vec<(min, max, step)>
/// * `rng` - Random number generator
///
/// # Returns
/// Vec of sample points, each point is a Vec<f64> of parameter values
pub fn generate_lhs_samples<R: Rng>(
    n_samples: usize,
    bounds: Vec<(f64, f64, f64)>,
    rng: &mut R,
) -> Vec<Vec<f64>> {
    let config = LhsConfig::new(n_samples, bounds);
    let sampler = LatinHypercubeSampler::new(config);
    sampler.sample(rng)
}

/// Generate LHS samples for a 2-parameter strategy (common case).
///
/// Returns Vec<(param1, param2)> as tuples for convenience.
pub fn generate_lhs_2d<R: Rng>(
    n_samples: usize,
    bounds1: (f64, f64, f64),
    bounds2: (f64, f64, f64),
    rng: &mut R,
) -> Vec<(f64, f64)> {
    let samples = generate_lhs_samples(n_samples, vec![bounds1, bounds2], rng);
    samples
        .into_iter()
        .map(|s| (s[0], s[1]))
        .collect()
}

/// Generate LHS samples for a 3-parameter strategy.
///
/// Returns Vec<(param1, param2, param3)> as tuples for convenience.
pub fn generate_lhs_3d<R: Rng>(
    n_samples: usize,
    bounds1: (f64, f64, f64),
    bounds2: (f64, f64, f64),
    bounds3: (f64, f64, f64),
    rng: &mut R,
) -> Vec<(f64, f64, f64)> {
    let samples = generate_lhs_samples(n_samples, vec![bounds1, bounds2, bounds3], rng);
    samples
        .into_iter()
        .map(|s| (s[0], s[1], s[2]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_lhs_basic() {
        let mut rng = StdRng::seed_from_u64(42);
        let config = LhsConfig::new(10, vec![(0.0, 1.0, 0.1), (0.0, 100.0, 1.0)]);
        let sampler = LatinHypercubeSampler::new(config);
        let samples = sampler.sample(&mut rng);

        assert_eq!(samples.len(), 10);
        assert!(samples.iter().all(|s| s.len() == 2));

        // Check bounds
        for sample in &samples {
            assert!(sample[0] >= 0.0 && sample[0] <= 1.0);
            assert!(sample[1] >= 0.0 && sample[1] <= 100.0);
        }
    }

    #[test]
    fn test_lhs_stratum_coverage() {
        let mut rng = StdRng::seed_from_u64(12345);
        let n = 10;
        let config = LhsConfig::new(n, vec![(0.0, 10.0, 0.0)]); // No step quantization
        let sampler = LatinHypercubeSampler::new(config);
        let samples = sampler.sample(&mut rng);

        // Check that we have one sample in each stratum
        let stratum_width = 10.0 / n as f64;
        let mut stratum_hits = vec![false; n];

        for sample in samples {
            let stratum = ((sample[0] / stratum_width).floor() as usize).min(n - 1);
            stratum_hits[stratum] = true;
        }

        // All strata should be covered
        assert!(stratum_hits.iter().all(|&hit| hit));
    }

    #[test]
    fn test_lhs_2d_convenience() {
        let mut rng = StdRng::seed_from_u64(42);
        let samples = generate_lhs_2d(
            5,
            (10.0, 50.0, 5.0),  // e.g., ATR period
            (1.0, 5.0, 0.5),   // e.g., multiplier
            &mut rng,
        );

        assert_eq!(samples.len(), 5);
        for (p1, p2) in &samples {
            assert!(*p1 >= 10.0 && *p1 <= 50.0);
            assert!(*p2 >= 1.0 && *p2 <= 5.0);
        }
    }

    #[test]
    fn test_lhs_3d_convenience() {
        let mut rng = StdRng::seed_from_u64(42);
        let samples = generate_lhs_3d(
            5,
            (50.0, 500.0, 10.0),  // period
            (0.70, 1.0, 0.01),    // entry_pct
            (0.40, 0.95, 0.01),   // exit_pct
            &mut rng,
        );

        assert_eq!(samples.len(), 5);
        for (p1, p2, p3) in &samples {
            assert!(*p1 >= 50.0 && *p1 <= 500.0);
            assert!(*p2 >= 0.70 && *p2 <= 1.0);
            assert!(*p3 >= 0.40 && *p3 <= 0.95);
        }
    }

    #[test]
    fn test_empty_config() {
        let mut rng = StdRng::seed_from_u64(42);

        // Empty samples
        let samples = generate_lhs_samples(0, vec![(0.0, 1.0, 0.1)], &mut rng);
        assert!(samples.is_empty());

        // Empty dimensions
        let samples = generate_lhs_samples(10, vec![], &mut rng);
        assert!(samples.is_empty());
    }

    #[test]
    fn test_step_quantization() {
        let mut rng = StdRng::seed_from_u64(42);
        let samples = generate_lhs_samples(
            20,
            vec![(0.0, 100.0, 5.0)], // Step of 5
            &mut rng,
        );

        // All values should be multiples of 5
        for sample in samples {
            let remainder = sample[0] % 5.0;
            assert!(
                remainder.abs() < 1e-10 || (5.0 - remainder).abs() < 1e-10,
                "Value {} is not a multiple of 5",
                sample[0]
            );
        }
    }
}
