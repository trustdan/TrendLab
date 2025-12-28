//! Machine Learning clustering for strategy analysis.
//!
//! Provides clustering capabilities for:
//! - Grouping similar strategy configurations
//! - Identifying performance regimes
//! - Finding representative strategies from each cluster

use linfa::prelude::*;
use linfa_clustering::KMeans;
use ndarray::{Array1, Array2};
use polars::prelude::*;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use thiserror::Error;

/// Errors that can occur during clustering operations.
#[derive(Debug, Error)]
pub enum ClusteringError {
    #[error("DataFrame is empty")]
    EmptyData,

    #[error("Missing required column: {0}")]
    MissingColumn(String),

    #[error("Clustering failed: {0}")]
    ClusteringFailed(String),

    #[error("Polars error: {0}")]
    PolarsError(#[from] PolarsError),

    #[error("Invalid k: must be between 2 and n_samples/2")]
    InvalidK,
}

/// Result of clustering analysis.
#[derive(Debug, Clone)]
pub struct ClusteringResult {
    /// Number of clusters
    pub k: usize,
    /// Cluster assignments for each row (0-indexed)
    pub labels: Vec<usize>,
    /// Cluster centers (k x n_features matrix)
    pub centers: Vec<Vec<f64>>,
    /// Feature names used for clustering
    pub feature_names: Vec<String>,
    /// Inertia (sum of squared distances to centers)
    pub inertia: f64,
}

impl ClusteringResult {
    /// Get the cluster assignment for a specific row.
    pub fn get_cluster(&self, row_idx: usize) -> Option<usize> {
        self.labels.get(row_idx).copied()
    }

    /// Get the center of a specific cluster.
    pub fn get_center(&self, cluster_idx: usize) -> Option<&Vec<f64>> {
        self.centers.get(cluster_idx)
    }

    /// Count members in each cluster.
    pub fn cluster_sizes(&self) -> Vec<usize> {
        let mut sizes = vec![0usize; self.k];
        for &label in &self.labels {
            if label < self.k {
                sizes[label] += 1;
            }
        }
        sizes
    }

    /// Get indices of all members in a specific cluster.
    pub fn cluster_members(&self, cluster_idx: usize) -> Vec<usize> {
        self.labels
            .iter()
            .enumerate()
            .filter(|(_, &label)| label == cluster_idx)
            .map(|(idx, _)| idx)
            .collect()
    }
}

/// Configuration for K-means clustering.
#[derive(Debug, Clone)]
pub struct KMeansConfig {
    /// Number of clusters (k)
    pub k: usize,
    /// Maximum iterations
    pub max_iterations: u64,
    /// Random seed for reproducibility
    pub seed: u64,
    /// Number of random initializations to try
    pub n_init: usize,
}

impl Default for KMeansConfig {
    fn default() -> Self {
        Self {
            k: 3,
            max_iterations: 300,
            seed: 42,
            n_init: 10,
        }
    }
}

impl KMeansConfig {
    /// Create a new config with the specified k.
    pub fn with_k(k: usize) -> Self {
        Self {
            k,
            ..Default::default()
        }
    }
}

/// Default feature columns for strategy clustering.
pub const DEFAULT_CLUSTER_FEATURES: &[&str] = &[
    "sharpe",
    "cagr",
    "max_drawdown",
    "sortino",
    "calmar",
    "win_rate",
    "profit_factor",
];

/// Cluster strategy configurations based on performance metrics.
///
/// Groups similar strategies together using K-means clustering
/// on normalized feature vectors.
///
/// # Arguments
/// * `df` - DataFrame with metric columns
/// * `features` - Column names to use as clustering features
/// * `config` - K-means configuration
///
/// # Returns
/// ClusteringResult with cluster assignments and centers
pub fn cluster_strategies(
    df: &DataFrame,
    features: &[&str],
    config: &KMeansConfig,
) -> Result<ClusteringResult, ClusteringError> {
    let n_samples = df.height();
    if n_samples == 0 {
        return Err(ClusteringError::EmptyData);
    }

    if config.k < 2 || config.k > n_samples / 2 {
        return Err(ClusteringError::InvalidK);
    }

    // Extract feature matrix
    let (data_matrix, feature_names) = extract_feature_matrix(df, features)?;

    // Normalize features (z-score normalization)
    let normalized = normalize_features(&data_matrix);

    // Create linfa dataset
    let dataset = DatasetBase::from(normalized.clone());

    // Run K-means with best of n_init runs
    let mut best_result: Option<(Vec<usize>, Array2<f64>, f64)> = None;

    for init in 0..config.n_init {
        let seed = config.seed + init as u64;
        let rng = SmallRng::seed_from_u64(seed);
        let model = KMeans::params_with_rng(config.k, rng)
            .max_n_iterations(config.max_iterations)
            .fit(&dataset)
            .map_err(|e| ClusteringError::ClusteringFailed(e.to_string()))?;

        let predictions = model.predict(&dataset);
        let labels: Vec<usize> = predictions.iter().copied().collect();
        let centers = model.centroids().to_owned();
        let inertia = compute_inertia(&normalized, &labels, &centers);

        match &best_result {
            None => best_result = Some((labels, centers, inertia)),
            Some((_, _, best_inertia)) if inertia < *best_inertia => {
                best_result = Some((labels, centers, inertia));
            }
            _ => {}
        }
    }

    let (labels, centers, inertia) = best_result.ok_or_else(|| {
        ClusteringError::ClusteringFailed("No valid clustering result".to_string())
    })?;

    // Convert centers to Vec<Vec<f64>>
    let centers_vec: Vec<Vec<f64>> = centers.outer_iter().map(|row| row.to_vec()).collect();

    Ok(ClusteringResult {
        k: config.k,
        labels,
        centers: centers_vec,
        feature_names,
        inertia,
    })
}

/// Extract feature matrix from DataFrame.
fn extract_feature_matrix(
    df: &DataFrame,
    features: &[&str],
) -> Result<(Array2<f64>, Vec<String>), ClusteringError> {
    let n_samples = df.height();
    let n_features = features.len();

    let mut data = Array2::<f64>::zeros((n_samples, n_features));
    let mut feature_names = Vec::with_capacity(n_features);

    for (j, &feature) in features.iter().enumerate() {
        let col = df
            .column(feature)
            .map_err(|_| ClusteringError::MissingColumn(feature.to_string()))?;

        let values = col
            .f64()
            .map_err(|_| ClusteringError::MissingColumn(format!("{} is not f64", feature)))?;

        for (i, value) in values.iter().enumerate() {
            data[[i, j]] = value.unwrap_or(0.0);
        }

        feature_names.push(feature.to_string());
    }

    Ok((data, feature_names))
}

/// Normalize features using z-score normalization.
fn normalize_features(data: &Array2<f64>) -> Array2<f64> {
    let n_features = data.ncols();
    let mut normalized = data.clone();

    for j in 0..n_features {
        let col = data.column(j);
        let mean = col.mean().unwrap_or(0.0);
        let std = col.std(0.0);

        // Avoid division by zero
        let std = if std < 1e-10 { 1.0 } else { std };

        for i in 0..data.nrows() {
            normalized[[i, j]] = (data[[i, j]] - mean) / std;
        }
    }

    normalized
}

/// Compute inertia (sum of squared distances to cluster centers).
fn compute_inertia(data: &Array2<f64>, labels: &[usize], centers: &Array2<f64>) -> f64 {
    let mut inertia = 0.0;

    for (i, &label) in labels.iter().enumerate() {
        let point = data.row(i);
        let center = centers.row(label);

        let distance_sq: f64 = point
            .iter()
            .zip(center.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum();

        inertia += distance_sq;
    }

    inertia
}

/// Find optimal k using the elbow method.
///
/// Runs clustering for a range of k values and returns inertia values.
/// The optimal k is typically where the inertia curve has an "elbow".
///
/// # Arguments
/// * `df` - DataFrame with metric columns
/// * `features` - Column names to use as clustering features
/// * `k_range` - Range of k values to try (e.g., 2..=10)
///
/// # Returns
/// Vec of (k, inertia) tuples
pub fn elbow_analysis(
    df: &DataFrame,
    features: &[&str],
    k_range: std::ops::RangeInclusive<usize>,
) -> Result<Vec<(usize, f64)>, ClusteringError> {
    let mut results = Vec::new();

    for k in k_range {
        let config = KMeansConfig::with_k(k);
        match cluster_strategies(df, features, &config) {
            Ok(result) => results.push((k, result.inertia)),
            Err(ClusteringError::InvalidK) => continue, // Skip invalid k
            Err(e) => return Err(e),
        }
    }

    Ok(results)
}

/// Add cluster labels to a DataFrame.
///
/// Returns a new DataFrame with an additional "cluster" column.
pub fn add_cluster_column(
    mut df: DataFrame,
    result: &ClusteringResult,
) -> Result<DataFrame, ClusteringError> {
    if df.height() != result.labels.len() {
        return Err(ClusteringError::ClusteringFailed(
            "Label count doesn't match DataFrame height".to_string(),
        ));
    }

    let cluster_col: Vec<u32> = result.labels.iter().map(|&l| l as u32).collect();
    let series = Series::new("cluster".into(), cluster_col);

    df.with_column(series)
        .map(|df| df.clone())
        .map_err(ClusteringError::PolarsError)
}

/// Summarize clusters with aggregated metrics.
///
/// Returns a DataFrame with one row per cluster containing
/// average metrics for that cluster.
pub fn cluster_summary(
    df: &DataFrame,
    result: &ClusteringResult,
    metrics: &[&str],
) -> Result<DataFrame, ClusteringError> {
    // First add cluster column
    let df_with_clusters = add_cluster_column(df.clone(), result)?;

    // Group by cluster and aggregate
    let mut agg_exprs: Vec<Expr> = vec![len().alias("n_members")];

    for metric in metrics {
        agg_exprs.push(col(*metric).mean().alias(format!("avg_{}", metric)));
        agg_exprs.push(col(*metric).std(0).alias(format!("std_{}", metric)));
    }

    df_with_clusters
        .lazy()
        .group_by([col("cluster")])
        .agg(agg_exprs)
        .sort(["cluster"], SortMultipleOptions::new())
        .collect()
        .map_err(ClusteringError::PolarsError)
}

/// Find the most representative config in each cluster.
///
/// The representative is the config closest to the cluster center.
pub fn cluster_representatives(
    df: &DataFrame,
    result: &ClusteringResult,
    id_column: &str,
) -> Result<Vec<(usize, String, f64)>, ClusteringError> {
    // Extract feature matrix for distance calculation
    let features: Vec<&str> = result.feature_names.iter().map(|s| s.as_str()).collect();
    let (data_matrix, _) = extract_feature_matrix(df, &features)?;
    let normalized = normalize_features(&data_matrix);

    // Get ID column
    let ids = df
        .column(id_column)
        .map_err(|_| ClusteringError::MissingColumn(id_column.to_string()))?;

    let mut representatives = Vec::new();

    for cluster_idx in 0..result.k {
        let members = result.cluster_members(cluster_idx);
        if members.is_empty() {
            continue;
        }

        let center: Array1<f64> = Array1::from(result.centers[cluster_idx].clone());

        // Find member closest to center
        let mut min_distance = f64::MAX;
        let mut best_idx = members[0];

        for &idx in &members {
            let point = normalized.row(idx);
            let distance: f64 = point
                .iter()
                .zip(center.iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum();

            if distance < min_distance {
                min_distance = distance;
                best_idx = idx;
            }
        }

        let id = ids
            .str()
            .map_err(|_| ClusteringError::MissingColumn(format!("{} is not string", id_column)))?
            .get(best_idx)
            .unwrap_or("unknown")
            .to_string();

        representatives.push((cluster_idx, id, min_distance.sqrt()));
    }

    Ok(representatives)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_df() -> DataFrame {
        DataFrame::new(vec![
            Series::new("config_id".into(), vec!["c1", "c2", "c3", "c4", "c5", "c6"]).into(),
            // Two clusters: high performers (c1, c2, c3) and low performers (c4, c5, c6)
            Series::new("sharpe".into(), vec![1.5, 1.4, 1.6, 0.3, 0.4, 0.2]).into(),
            Series::new("cagr".into(), vec![0.15, 0.14, 0.16, 0.03, 0.04, 0.02]).into(),
            Series::new(
                "max_drawdown".into(),
                vec![0.10, 0.11, 0.09, 0.30, 0.28, 0.32],
            )
            .into(),
            Series::new("sortino".into(), vec![2.0, 1.9, 2.1, 0.4, 0.5, 0.3]).into(),
            Series::new("calmar".into(), vec![1.5, 1.3, 1.8, 0.1, 0.14, 0.06]).into(),
            Series::new("win_rate".into(), vec![0.55, 0.54, 0.56, 0.42, 0.43, 0.41]).into(),
            Series::new("profit_factor".into(), vec![1.8, 1.7, 1.9, 1.0, 1.1, 0.9]).into(),
        ])
        .unwrap()
    }

    #[test]
    fn test_cluster_strategies() {
        let df = create_test_df();
        let config = KMeansConfig::with_k(2);

        let result = cluster_strategies(&df, DEFAULT_CLUSTER_FEATURES, &config).unwrap();

        assert_eq!(result.k, 2);
        assert_eq!(result.labels.len(), 6);
        assert_eq!(result.centers.len(), 2);

        // Check that all labels are valid (0 or 1)
        assert!(result.labels.iter().all(|&l| l < 2));

        // Check cluster sizes sum to total
        let sizes = result.cluster_sizes();
        assert_eq!(sizes.iter().sum::<usize>(), 6);
    }

    #[test]
    fn test_cluster_separation() {
        let df = create_test_df();
        let config = KMeansConfig::with_k(2);

        let result = cluster_strategies(&df, DEFAULT_CLUSTER_FEATURES, &config).unwrap();

        // High performers (c1, c2, c3) should be in one cluster
        // Low performers (c4, c5, c6) should be in another
        let high_cluster = result.labels[0];
        assert_eq!(result.labels[1], high_cluster);
        assert_eq!(result.labels[2], high_cluster);

        let low_cluster = result.labels[3];
        assert_eq!(result.labels[4], low_cluster);
        assert_eq!(result.labels[5], low_cluster);

        // Should be different clusters
        assert_ne!(high_cluster, low_cluster);
    }

    #[test]
    fn test_add_cluster_column() {
        let df = create_test_df();
        let config = KMeansConfig::with_k(2);

        let result = cluster_strategies(&df, DEFAULT_CLUSTER_FEATURES, &config).unwrap();
        let df_with_clusters = add_cluster_column(df, &result).unwrap();

        assert!(df_with_clusters.column("cluster").is_ok());
        assert_eq!(df_with_clusters.height(), 6);
    }

    #[test]
    fn test_cluster_summary() {
        let df = create_test_df();
        let config = KMeansConfig::with_k(2);

        let result = cluster_strategies(&df, DEFAULT_CLUSTER_FEATURES, &config).unwrap();
        let summary = cluster_summary(&df, &result, &["sharpe", "cagr"]).unwrap();

        assert_eq!(summary.height(), 2); // 2 clusters
        assert!(summary.column("n_members").is_ok());
        assert!(summary.column("avg_sharpe").is_ok());
        assert!(summary.column("avg_cagr").is_ok());
    }

    #[test]
    fn test_cluster_representatives() {
        let df = create_test_df();
        let config = KMeansConfig::with_k(2);

        let result = cluster_strategies(&df, DEFAULT_CLUSTER_FEATURES, &config).unwrap();
        let reps = cluster_representatives(&df, &result, "config_id").unwrap();

        assert_eq!(reps.len(), 2); // One representative per cluster

        // Representatives should be valid config IDs
        for (_, id, _) in &reps {
            assert!(["c1", "c2", "c3", "c4", "c5", "c6"].contains(&id.as_str()));
        }
    }

    #[test]
    fn test_elbow_analysis() {
        let df = create_test_df();
        let results = elbow_analysis(&df, DEFAULT_CLUSTER_FEATURES, 2..=3).unwrap();

        assert_eq!(results.len(), 2); // k=2 and k=3

        // Inertia should decrease with more clusters
        assert!(results[1].1 <= results[0].1);
    }

    #[test]
    fn test_empty_df_error() {
        let df = DataFrame::new(vec![
            Series::new("sharpe".into(), Vec::<f64>::new()).into(),
            Series::new("cagr".into(), Vec::<f64>::new()).into(),
        ])
        .unwrap();

        let config = KMeansConfig::with_k(2);
        let result = cluster_strategies(&df, &["sharpe", "cagr"], &config);

        assert!(matches!(result, Err(ClusteringError::EmptyData)));
    }

    #[test]
    fn test_missing_column_error() {
        let df = create_test_df();
        let config = KMeansConfig::with_k(2);

        let result = cluster_strategies(&df, &["sharpe", "nonexistent"], &config);

        assert!(matches!(result, Err(ClusteringError::MissingColumn(_))));
    }
}
