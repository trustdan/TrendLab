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

/// Default feature columns for strategy clustering (basic metrics).
pub const DEFAULT_CLUSTER_FEATURES: &[&str] = &[
    "sharpe",
    "cagr",
    "max_drawdown",
    "sortino",
    "calmar",
    "win_rate",
    "profit_factor",
];

/// Extended feature columns including tail risk and regime analysis.
/// Use when statistical analysis has been computed.
pub const EXTENDED_CLUSTER_FEATURES: &[&str] = &[
    "sharpe",
    "cagr",
    "max_drawdown",
    "sortino",
    "calmar",
    "win_rate",
    "profit_factor",
    "cvar_95",           // Tail risk
    "skewness",          // Return asymmetry
    "kurtosis",          // Fat tails
    "sharpe_stability",  // Consistency across folds
    "regime_concentration", // Regime dependency
];

/// Robustness-focused features for identifying stable strategies.
/// Emphasizes consistency and risk metrics over raw returns.
pub const ROBUSTNESS_CLUSTER_FEATURES: &[&str] = &[
    "sharpe",
    "min_sharpe",        // Worst-case performance
    "sharpe_stability",  // Consistency
    "max_drawdown",
    "cvar_95",
    "hit_rate",          // Symbol consistency
    "regime_concentration",
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

// =============================================================================
// DIVERSE SELECTION FOR YOLO MODE
// =============================================================================

/// Configuration for diverse strategy selection.
#[derive(Debug, Clone)]
pub struct DiverseSelectionConfig {
    /// Number of strategies to select
    pub n_select: usize,
    /// Column to rank strategies by (e.g., "sharpe", "robust_score")
    pub rank_column: String,
    /// Whether higher values are better
    pub descending: bool,
    /// Minimum number of clusters to use (auto-scales with n_select)
    pub min_clusters: usize,
    /// Maximum number of clusters to use
    pub max_clusters: usize,
    /// Features to use for clustering
    pub features: Vec<String>,
}

impl Default for DiverseSelectionConfig {
    fn default() -> Self {
        Self {
            n_select: 10,
            rank_column: "sharpe".to_string(),
            descending: true,
            min_clusters: 3,
            max_clusters: 10,
            features: DEFAULT_CLUSTER_FEATURES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl DiverseSelectionConfig {
    /// Create a config for selecting n diverse strategies.
    pub fn with_n(n_select: usize) -> Self {
        Self {
            n_select,
            // Auto-scale clusters to roughly n_select/2 (at least 2 per cluster)
            min_clusters: (n_select / 4).max(2),
            max_clusters: (n_select / 2).max(3),
            ..Default::default()
        }
    }

    /// Use robustness-focused features.
    pub fn with_robustness_features(mut self) -> Self {
        self.features = ROBUSTNESS_CLUSTER_FEATURES.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Rank by a specific column.
    pub fn rank_by(mut self, column: &str, descending: bool) -> Self {
        self.rank_column = column.to_string();
        self.descending = descending;
        self
    }
}

/// Result of diverse selection.
#[derive(Debug, Clone)]
pub struct DiverseSelectionResult {
    /// Indices of selected rows in the original DataFrame
    pub selected_indices: Vec<usize>,
    /// Cluster assignments for selected rows
    pub cluster_assignments: Vec<usize>,
    /// Cluster ID -> count of selected strategies from that cluster
    pub cluster_distribution: Vec<(usize, usize)>,
    /// Clustering result used for selection
    pub clustering_result: ClusteringResult,
}

impl DiverseSelectionResult {
    /// Get IDs of selected strategies from the DataFrame.
    pub fn get_selected_ids(&self, df: &DataFrame, id_column: &str) -> Result<Vec<String>, ClusteringError> {
        let ids = df
            .column(id_column)
            .map_err(|_| ClusteringError::MissingColumn(id_column.to_string()))?
            .str()
            .map_err(|_| ClusteringError::MissingColumn(format!("{} is not string", id_column)))?;

        Ok(self.selected_indices
            .iter()
            .filter_map(|&idx| ids.get(idx).map(|s| s.to_string()))
            .collect())
    }

    /// Extract selected rows from the original DataFrame.
    ///
    /// Returns a new DataFrame containing only the selected rows in their
    /// original order (sorted by rank as determined during selection).
    pub fn get_selected_rows(&self, df: &DataFrame) -> Result<DataFrame, ClusteringError> {
        if self.selected_indices.is_empty() {
            // Return empty DataFrame with same schema
            return Ok(df.clone().slice(0, 0));
        }

        // Build mask for take operation
        let indices: Vec<u32> = self.selected_indices.iter().map(|&i| i as u32).collect();
        let idx_arr = polars::prelude::IdxCa::new("idx".into(), &indices);

        df.take(&idx_arr).map_err(ClusteringError::PolarsError)
    }
}

/// Select diverse top strategies using clustering.
///
/// Instead of just selecting the top N strategies (which may be very similar),
/// this function clusters strategies first and selects the best from each cluster.
///
/// Algorithm:
/// 1. Pre-filter to top N*2 by rank (performance threshold)
/// 2. Cluster the filtered strategies
/// 3. From each cluster, select the top-ranked strategies proportionally
/// 4. Ensure we get exactly N strategies
///
/// # Arguments
/// * `df` - DataFrame with strategy results
/// * `config` - Selection configuration
///
/// # Returns
/// DiverseSelectionResult with selected indices and cluster info
pub fn select_diverse_strategies(
    df: &DataFrame,
    config: &DiverseSelectionConfig,
) -> Result<DiverseSelectionResult, ClusteringError> {
    let n_rows = df.height();
    if n_rows == 0 {
        return Err(ClusteringError::EmptyData);
    }

    // Clamp n_select to available rows
    let n_select = config.n_select.min(n_rows);

    // Pre-filter to top N*2 candidates (ensures quality threshold)
    let pre_filter_n = (n_select * 2).min(n_rows);

    // Sort by rank column and take top N
    let sort_options = SortMultipleOptions::new().with_order_descending(config.descending);
    let sorted_df = df
        .clone()
        .sort([&config.rank_column], sort_options)
        .map_err(ClusteringError::PolarsError)?
        .head(Some(pre_filter_n));

    // Track original indices by sorting the indices alongside the data
    let mut indices_with_ranks: Vec<(usize, f64)> = df
        .column(&config.rank_column)
        .map_err(|_| ClusteringError::MissingColumn(config.rank_column.clone()))?
        .f64()
        .map_err(|_| ClusteringError::MissingColumn(format!("{} is not f64", config.rank_column)))?
        .iter()
        .enumerate()
        .map(|(i, v)| (i, v.unwrap_or(f64::NEG_INFINITY)))
        .collect();

    // Sort by rank (descending if descending=true)
    if config.descending {
        indices_with_ranks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        indices_with_ranks.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    }

    // Take top N original indices
    let original_indices: Vec<usize> = indices_with_ranks
        .into_iter()
        .take(pre_filter_n)
        .map(|(i, _)| i)
        .collect();

    // Determine optimal k
    let k = determine_optimal_k(pre_filter_n, n_select, config.min_clusters, config.max_clusters);

    // Filter features to only those present in the DataFrame
    let available_features: Vec<&str> = config
        .features
        .iter()
        .filter(|f| sorted_df.column(f.as_str()).is_ok())
        .map(|s| s.as_str())
        .collect();

    if available_features.is_empty() {
        return Err(ClusteringError::MissingColumn("No clustering features available".to_string()));
    }

    // Cluster the filtered candidates
    let cluster_config = KMeansConfig::with_k(k);
    let clustering_result = cluster_strategies(&sorted_df, &available_features, &cluster_config)?;

    // Select from each cluster proportionally
    let selected = select_proportionally_from_clusters(
        &sorted_df,
        &clustering_result,
        &config.rank_column,
        n_select,
    )?;

    // Map back to original indices
    let selected_indices: Vec<usize> = selected.iter().map(|&i| original_indices[i]).collect();
    let cluster_assignments: Vec<usize> = selected
        .iter()
        .map(|&i| clustering_result.labels[i])
        .collect();

    // Compute cluster distribution
    let mut distribution = vec![0usize; k];
    for &cluster in &cluster_assignments {
        distribution[cluster] += 1;
    }
    let cluster_distribution: Vec<(usize, usize)> = distribution
        .into_iter()
        .enumerate()
        .filter(|(_, count)| *count > 0)
        .collect();

    Ok(DiverseSelectionResult {
        selected_indices,
        cluster_assignments,
        cluster_distribution,
        clustering_result,
    })
}

/// Determine optimal k based on data size and selection count.
fn determine_optimal_k(n_candidates: usize, n_select: usize, min_k: usize, max_k: usize) -> usize {
    // Target ~2-3 strategies per cluster
    let target_k = n_select / 2;

    // Clamp to valid range
    let k = target_k.max(min_k).min(max_k);

    // Ensure k is valid for the data
    k.min(n_candidates / 2).max(2)
}

/// Select strategies proportionally from each cluster.
fn select_proportionally_from_clusters(
    df: &DataFrame,
    clustering: &ClusteringResult,
    rank_column: &str,
    n_select: usize,
) -> Result<Vec<usize>, ClusteringError> {
    let k = clustering.k;
    let cluster_sizes = clustering.cluster_sizes();
    let total_size: usize = cluster_sizes.iter().sum();

    // Calculate proportional allocation per cluster
    let mut allocations: Vec<usize> = cluster_sizes
        .iter()
        .map(|&size| {
            // Proportional share, at least 1 if cluster is non-empty
            let share = (size as f64 / total_size as f64) * n_select as f64;
            share.round() as usize
        })
        .collect();

    // Adjust to ensure we get exactly n_select
    let total_allocated: usize = allocations.iter().sum();
    if total_allocated < n_select {
        // Add to largest clusters first
        let mut sorted_clusters: Vec<usize> = (0..k).collect();
        sorted_clusters.sort_by_key(|&i| std::cmp::Reverse(cluster_sizes[i]));

        let mut remaining = n_select - total_allocated;
        for &cluster in &sorted_clusters {
            if remaining == 0 {
                break;
            }
            if allocations[cluster] < cluster_sizes[cluster] {
                allocations[cluster] += 1;
                remaining -= 1;
            }
        }
    } else if total_allocated > n_select {
        // Remove from smallest allocations first (keeping at least 1)
        let mut sorted_clusters: Vec<usize> = (0..k).collect();
        sorted_clusters.sort_by_key(|&i| allocations[i]);

        let mut excess = total_allocated - n_select;
        for &cluster in &sorted_clusters {
            if excess == 0 {
                break;
            }
            if allocations[cluster] > 1 {
                allocations[cluster] -= 1;
                excess -= 1;
            }
        }
    }

    // Get rank values for sorting within clusters
    let ranks: Vec<f64> = df
        .column(rank_column)
        .map_err(|_| ClusteringError::MissingColumn(rank_column.to_string()))?
        .f64()
        .map_err(|_| ClusteringError::MissingColumn(format!("{} is not f64", rank_column)))?
        .iter()
        .map(|v| v.unwrap_or(f64::NEG_INFINITY))
        .collect();

    // Select top-ranked from each cluster
    let mut selected = Vec::with_capacity(n_select);

    for (cluster, &allocation) in allocations.iter().enumerate().take(k) {
        let mut members = clustering.cluster_members(cluster);
        // Sort members by rank (descending - best first)
        members.sort_by(|&a, &b| ranks[b].partial_cmp(&ranks[a]).unwrap_or(std::cmp::Ordering::Equal));

        // Take allocated number from this cluster
        let take = allocation.min(members.len());
        selected.extend(&members[..take]);
    }

    Ok(selected)
}

/// Quick helper: select top N diverse strategies by Sharpe.
///
/// This is a convenience wrapper for the common case of selecting
/// diverse high-Sharpe strategies for YOLO mode.
pub fn select_diverse_by_sharpe(
    df: &DataFrame,
    n: usize,
) -> Result<DiverseSelectionResult, ClusteringError> {
    let config = DiverseSelectionConfig::with_n(n)
        .rank_by("sharpe", true);
    select_diverse_strategies(df, &config)
}

/// Quick helper: select top N diverse strategies by robust score.
pub fn select_diverse_by_robust_score(
    df: &DataFrame,
    n: usize,
) -> Result<DiverseSelectionResult, ClusteringError> {
    let config = DiverseSelectionConfig::with_n(n)
        .rank_by("robust_score", true)
        .with_robustness_features();
    select_diverse_strategies(df, &config)
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

    // =========================================================================
    // Diverse Selection Tests
    // =========================================================================

    fn create_larger_test_df() -> DataFrame {
        // Create a larger dataset for diverse selection tests
        // 12 configs in 3 clusters (performance profiles)
        DataFrame::new(vec![
            Series::new("config_id".into(), vec![
                "high1", "high2", "high3", "high4",      // High performers
                "mid1", "mid2", "mid3", "mid4",          // Medium performers
                "low1", "low2", "low3", "low4",          // Low performers
            ]).into(),
            Series::new("sharpe".into(), vec![
                2.0, 1.9, 1.8, 1.7,   // High
                1.0, 0.9, 0.8, 0.7,   // Mid
                0.2, 0.1, 0.0, -0.1,  // Low
            ]).into(),
            Series::new("cagr".into(), vec![
                0.20, 0.19, 0.18, 0.17,
                0.10, 0.09, 0.08, 0.07,
                0.02, 0.01, 0.00, -0.01,
            ]).into(),
            Series::new("max_drawdown".into(), vec![
                0.10, 0.11, 0.12, 0.13,
                0.20, 0.21, 0.22, 0.23,
                0.35, 0.36, 0.37, 0.38,
            ]).into(),
            Series::new("sortino".into(), vec![
                2.5, 2.4, 2.3, 2.2,
                1.3, 1.2, 1.1, 1.0,
                0.3, 0.2, 0.1, 0.0,
            ]).into(),
            Series::new("calmar".into(), vec![
                2.0, 1.7, 1.5, 1.3,
                0.5, 0.4, 0.4, 0.3,
                0.06, 0.03, 0.0, -0.03,
            ]).into(),
            Series::new("win_rate".into(), vec![
                0.60, 0.59, 0.58, 0.57,
                0.52, 0.51, 0.50, 0.49,
                0.42, 0.41, 0.40, 0.39,
            ]).into(),
            Series::new("profit_factor".into(), vec![
                2.0, 1.9, 1.8, 1.7,
                1.3, 1.2, 1.1, 1.0,
                0.9, 0.8, 0.7, 0.6,
            ]).into(),
        ])
        .unwrap()
    }

    #[test]
    fn test_diverse_selection_config() {
        let config = DiverseSelectionConfig::default();
        assert_eq!(config.n_select, 10);
        assert_eq!(config.rank_column, "sharpe");
        assert!(config.descending);

        let config = DiverseSelectionConfig::with_n(6);
        assert_eq!(config.n_select, 6);
        assert!(config.min_clusters >= 2);
    }

    #[test]
    fn test_diverse_selection_basic() {
        let df = create_larger_test_df();
        let config = DiverseSelectionConfig::with_n(6)
            .rank_by("sharpe", true);

        let result = select_diverse_strategies(&df, &config).unwrap();

        // Should select exactly 6 strategies
        assert_eq!(result.selected_indices.len(), 6);
        assert_eq!(result.cluster_assignments.len(), 6);

        // All selected indices should be valid
        for &idx in &result.selected_indices {
            assert!(idx < df.height());
        }

        // Should have strategies from multiple clusters
        let unique_clusters: std::collections::HashSet<_> =
            result.cluster_assignments.iter().collect();
        assert!(unique_clusters.len() >= 2, "Should select from multiple clusters");
    }

    #[test]
    fn test_diverse_selection_selects_top_performers() {
        let df = create_larger_test_df();
        let result = select_diverse_by_sharpe(&df, 4).unwrap();

        // Get the config_ids of selected strategies
        let ids = result.get_selected_ids(&df, "config_id").unwrap();

        // Should include some high performers
        let high_performers: Vec<_> = ids.iter()
            .filter(|id| id.starts_with("high"))
            .collect();
        assert!(!high_performers.is_empty(), "Should include high performers");
    }

    #[test]
    fn test_diverse_selection_cluster_distribution() {
        let df = create_larger_test_df();
        let config = DiverseSelectionConfig::with_n(6);

        let result = select_diverse_strategies(&df, &config).unwrap();

        // Check distribution is non-empty
        assert!(!result.cluster_distribution.is_empty());

        // Total from distribution should match selected count
        let total: usize = result.cluster_distribution.iter().map(|(_, count)| count).sum();
        assert_eq!(total, 6);
    }

    #[test]
    fn test_diverse_selection_respects_available_features() {
        // Create a minimal DataFrame with fewer features
        let df = DataFrame::new(vec![
            Series::new("config_id".into(), vec!["a", "b", "c", "d", "e", "f"]).into(),
            Series::new("sharpe".into(), vec![1.5, 1.4, 1.3, 0.5, 0.4, 0.3]).into(),
            Series::new("cagr".into(), vec![0.15, 0.14, 0.13, 0.05, 0.04, 0.03]).into(),
        ])
        .unwrap();

        let config = DiverseSelectionConfig::with_n(4);
        let result = select_diverse_strategies(&df, &config).unwrap();

        // Should still work with limited features
        assert_eq!(result.selected_indices.len(), 4);
    }

    #[test]
    fn test_select_diverse_by_sharpe_helper() {
        let df = create_larger_test_df();
        let result = select_diverse_by_sharpe(&df, 4).unwrap();

        assert_eq!(result.selected_indices.len(), 4);
    }

    #[test]
    fn test_determine_optimal_k() {
        // n_select = 10, should target k = 5 (10/2)
        assert_eq!(determine_optimal_k(20, 10, 2, 10), 5);

        // Small dataset: target_k = 6/2 = 3, clamped to min(3, 6/2) = 3
        assert_eq!(determine_optimal_k(6, 6, 2, 10), 3);

        // Very small: target_k = 4/2 = 2, clamped by n_candidates/2 = 2
        assert_eq!(determine_optimal_k(4, 4, 2, 10), 2);

        // Large selection, k should be clamped to max
        assert_eq!(determine_optimal_k(100, 30, 3, 8), 8);
    }
}
