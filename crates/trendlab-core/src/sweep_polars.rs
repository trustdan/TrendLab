//! Polars-based sweep result analysis.
//!
//! Provides DataFrame conversion and analysis for sweep results,
//! enabling fast filtering, sorting, and aggregation of large parameter sweeps.

use crate::sweep::{MultiStrategySweepResult, RankMetric, SweepResult};
use polars::prelude::*;
use std::collections::HashMap;

/// Convert a SweepResult to a Polars DataFrame.
///
/// Creates a DataFrame with columns for all configuration parameters and metrics.
/// This enables fast filtering and sorting with Polars expressions.
pub fn sweep_to_dataframe(result: &SweepResult) -> PolarsResult<DataFrame> {
    let n = result.config_results.len();

    // Config parameters
    let entry_lookback: Vec<u32> = result
        .config_results
        .iter()
        .map(|r| r.config_id.entry_lookback as u32)
        .collect();
    let exit_lookback: Vec<u32> = result
        .config_results
        .iter()
        .map(|r| r.config_id.exit_lookback as u32)
        .collect();

    // Metrics
    let total_return: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.total_return)
        .collect();
    let cagr: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.cagr)
        .collect();
    let sharpe: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.sharpe)
        .collect();
    let sortino: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.sortino)
        .collect();
    let max_drawdown: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.max_drawdown)
        .collect();
    let calmar: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.calmar)
        .collect();
    let win_rate: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.win_rate)
        .collect();
    let profit_factor: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.profit_factor)
        .collect();
    let num_trades: Vec<u32> = result
        .config_results
        .iter()
        .map(|r| r.metrics.num_trades)
        .collect();
    let turnover: Vec<f64> = result
        .config_results
        .iter()
        .map(|r| r.metrics.turnover)
        .collect();

    // Config ID string for easy identification
    let config_id: Vec<String> = result
        .config_results
        .iter()
        .map(|r| r.config_id.id())
        .collect();

    // Sweep metadata
    let sweep_id: Vec<String> = vec![result.sweep_id.clone(); n];

    DataFrame::new(vec![
        Series::new("sweep_id".into(), sweep_id).into(),
        Series::new("config_id".into(), config_id).into(),
        Series::new("entry_lookback".into(), entry_lookback).into(),
        Series::new("exit_lookback".into(), exit_lookback).into(),
        Series::new("total_return".into(), total_return).into(),
        Series::new("cagr".into(), cagr).into(),
        Series::new("sharpe".into(), sharpe).into(),
        Series::new("sortino".into(), sortino).into(),
        Series::new("max_drawdown".into(), max_drawdown).into(),
        Series::new("calmar".into(), calmar).into(),
        Series::new("win_rate".into(), win_rate).into(),
        Series::new("profit_factor".into(), profit_factor).into(),
        Series::new("num_trades".into(), num_trades).into(),
        Series::new("turnover".into(), turnover).into(),
    ])
}

/// Convert a MultiStrategySweepResult to a Polars DataFrame.
///
/// Creates a DataFrame with all results across symbols and strategies.
pub fn multi_sweep_to_dataframe(result: &MultiStrategySweepResult) -> PolarsResult<DataFrame> {
    let mut all_dfs: Vec<DataFrame> = Vec::new();

    for ((symbol, strategy_type), sweep_result) in &result.results {
        let mut df = sweep_to_dataframe(sweep_result)?;

        // Add symbol and strategy columns
        let n = df.height();
        let symbol_col = Series::new("symbol".into(), vec![symbol.clone(); n]);
        let strategy_col = Series::new("strategy_type".into(), vec![strategy_type.id(); n]);

        df = df.with_column(symbol_col)?.clone();
        df = df.with_column(strategy_col)?.clone();

        all_dfs.push(df);
    }

    if all_dfs.is_empty() {
        // Return empty DataFrame with correct schema
        return DataFrame::new(vec![
            Series::new("sweep_id".into(), Vec::<String>::new()).into(),
            Series::new("config_id".into(), Vec::<String>::new()).into(),
            Series::new("entry_lookback".into(), Vec::<u32>::new()).into(),
            Series::new("exit_lookback".into(), Vec::<u32>::new()).into(),
            Series::new("total_return".into(), Vec::<f64>::new()).into(),
            Series::new("cagr".into(), Vec::<f64>::new()).into(),
            Series::new("sharpe".into(), Vec::<f64>::new()).into(),
            Series::new("sortino".into(), Vec::<f64>::new()).into(),
            Series::new("max_drawdown".into(), Vec::<f64>::new()).into(),
            Series::new("calmar".into(), Vec::<f64>::new()).into(),
            Series::new("win_rate".into(), Vec::<f64>::new()).into(),
            Series::new("profit_factor".into(), Vec::<f64>::new()).into(),
            Series::new("num_trades".into(), Vec::<u32>::new()).into(),
            Series::new("turnover".into(), Vec::<f64>::new()).into(),
            Series::new("symbol".into(), Vec::<String>::new()).into(),
            Series::new("strategy_type".into(), Vec::<String>::new()).into(),
        ]);
    }

    // Vertically concatenate all DataFrames
    let lazy_frames: Vec<LazyFrame> = all_dfs.into_iter().map(|df| df.lazy()).collect();
    concat(lazy_frames, UnionArgs::default())?.collect()
}

/// Enrich a DataFrame with a sector column based on symbol lookups.
///
/// Given a DataFrame with a "symbol" column and a sector lookup table,
/// adds a new "sector" column mapping each symbol to its sector.
/// Symbols not found in the lookup will have "Unknown" as their sector.
///
/// # Arguments
/// * `df` - DataFrame with a "symbol" column
/// * `sector_lookup` - HashMap mapping ticker symbols to sector names
///
/// # Returns
/// A new DataFrame with the "sector" column added
///
/// # Example
/// ```ignore
/// use trendlab_core::universe::Universe;
/// use trendlab_core::sweep_polars::enrich_with_sector;
///
/// let universe = Universe::default_universe();
/// let lookup = universe.build_sector_lookup();
/// let enriched_df = enrich_with_sector(df, &lookup)?;
/// ```
pub fn enrich_with_sector(
    mut df: DataFrame,
    sector_lookup: &HashMap<String, String>,
) -> PolarsResult<DataFrame> {
    // Extract the symbol column
    let symbol_col = df.column("symbol")?.str()?;

    // Map each symbol to its sector
    let sectors: Vec<String> = symbol_col
        .into_iter()
        .map(|opt_symbol| {
            opt_symbol
                .and_then(|s| sector_lookup.get(s))
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string())
        })
        .collect();

    // Add the sector column to the DataFrame
    let sector_series = Series::new("sector".into(), sectors);
    df.with_column(sector_series).map(|df| df.clone())
}

/// Enrich a MultiStrategySweepResult DataFrame with sectors.
///
/// Convenience function that converts a MultiStrategySweepResult to a DataFrame
/// and enriches it with sector information in one step.
pub fn multi_sweep_with_sectors(
    result: &MultiStrategySweepResult,
    sector_lookup: &HashMap<String, String>,
) -> PolarsResult<DataFrame> {
    let df = multi_sweep_to_dataframe(result)?;
    enrich_with_sector(df, sector_lookup)
}

/// Analysis result from sweep queries.
#[derive(Debug, Clone)]
pub struct SweepAnalysis {
    /// The filtered/sorted DataFrame
    pub df: DataFrame,
    /// Number of configs that passed filters
    pub count: usize,
}

impl SweepAnalysis {
    /// Get the top N configurations by a metric.
    pub fn top(&self, _n: usize) -> &DataFrame {
        &self.df // Already sorted, just take head in actual usage
    }

    /// Get summary statistics.
    pub fn summary(&self) -> PolarsResult<DataFrame> {
        self.df
            .clone()
            .lazy()
            .select([
                col("sharpe").mean().alias("mean_sharpe"),
                col("sharpe").max().alias("max_sharpe"),
                col("sharpe").min().alias("min_sharpe"),
                col("cagr").mean().alias("mean_cagr"),
                col("max_drawdown").mean().alias("mean_drawdown"),
                col("num_trades").mean().alias("mean_trades"),
            ])
            .collect()
    }
}

/// Query configuration for sweep analysis.
#[derive(Debug, Clone)]
pub struct SweepQuery {
    /// Minimum Sharpe ratio filter
    pub min_sharpe: Option<f64>,
    /// Maximum drawdown filter (as positive number, e.g., 0.20 for 20%)
    pub max_drawdown: Option<f64>,
    /// Minimum number of trades
    pub min_trades: Option<u32>,
    /// Maximum number of trades
    pub max_trades: Option<u32>,
    /// Minimum CAGR
    pub min_cagr: Option<f64>,
    /// Minimum win rate
    pub min_win_rate: Option<f64>,
    /// Metric to sort by
    pub sort_by: RankMetric,
    /// Sort ascending (default: false = descending for most metrics)
    pub ascending: bool,
    /// Maximum results to return
    pub limit: Option<usize>,
}

impl Default for SweepQuery {
    fn default() -> Self {
        Self {
            min_sharpe: None,
            max_drawdown: None,
            min_trades: None,
            max_trades: None,
            min_cagr: None,
            min_win_rate: None,
            sort_by: RankMetric::Sharpe,
            ascending: false,
            limit: None,
        }
    }
}

impl SweepQuery {
    /// Create a new query with defaults (sort by Sharpe descending).
    pub fn new() -> Self {
        Self {
            sort_by: RankMetric::Sharpe,
            ascending: false,
            ..Default::default()
        }
    }

    /// Set minimum Sharpe filter.
    pub fn min_sharpe(mut self, min: f64) -> Self {
        self.min_sharpe = Some(min);
        self
    }

    /// Set maximum drawdown filter.
    pub fn max_drawdown(mut self, max: f64) -> Self {
        self.max_drawdown = Some(max);
        self
    }

    /// Set minimum trades filter.
    pub fn min_trades(mut self, min: u32) -> Self {
        self.min_trades = Some(min);
        self
    }

    /// Set maximum trades filter.
    pub fn max_trades(mut self, max: u32) -> Self {
        self.max_trades = Some(max);
        self
    }

    /// Set minimum CAGR filter.
    pub fn min_cagr(mut self, min: f64) -> Self {
        self.min_cagr = Some(min);
        self
    }

    /// Set minimum win rate filter.
    pub fn min_win_rate(mut self, min: f64) -> Self {
        self.min_win_rate = Some(min);
        self
    }

    /// Set sort metric and direction.
    pub fn sort(mut self, metric: RankMetric, ascending: bool) -> Self {
        self.sort_by = metric;
        self.ascending = ascending;
        self
    }

    /// Set maximum results limit.
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// Execute query on a DataFrame.
    pub fn execute(&self, df: DataFrame) -> PolarsResult<SweepAnalysis> {
        let mut lf = df.lazy();

        // Apply filters
        if let Some(min) = self.min_sharpe {
            lf = lf.filter(col("sharpe").gt_eq(lit(min)));
        }
        if let Some(max) = self.max_drawdown {
            lf = lf.filter(col("max_drawdown").lt_eq(lit(max)));
        }
        if let Some(min) = self.min_trades {
            lf = lf.filter(col("num_trades").gt_eq(lit(min)));
        }
        if let Some(max) = self.max_trades {
            lf = lf.filter(col("num_trades").lt_eq(lit(max)));
        }
        if let Some(min) = self.min_cagr {
            lf = lf.filter(col("cagr").gt_eq(lit(min)));
        }
        if let Some(min) = self.min_win_rate {
            lf = lf.filter(col("win_rate").gt_eq(lit(min)));
        }

        // Sort by specified metric
        let sort_col = metric_to_col_name(self.sort_by);
        let sort_opts = SortMultipleOptions::new().with_order_descending(!self.ascending);
        lf = lf.sort([sort_col], sort_opts);

        // Apply limit
        if let Some(n) = self.limit {
            lf = lf.limit(n as u32);
        }

        let result_df = lf.collect()?;
        let count = result_df.height();

        Ok(SweepAnalysis {
            df: result_df,
            count,
        })
    }
}

/// Analyze a sweep result with a query.
pub fn analyze_sweep(result: &SweepResult, query: &SweepQuery) -> PolarsResult<SweepAnalysis> {
    let df = sweep_to_dataframe(result)?;
    query.execute(df)
}

/// Get top N configs by Sharpe, with optional minimum thresholds.
pub fn top_configs_by_sharpe(
    result: &SweepResult,
    n: usize,
    min_sharpe: Option<f64>,
    max_drawdown: Option<f64>,
) -> PolarsResult<DataFrame> {
    let df = sweep_to_dataframe(result)?;

    let mut lf = df.lazy();

    if let Some(min) = min_sharpe {
        lf = lf.filter(col("sharpe").gt_eq(lit(min)));
    }
    if let Some(max) = max_drawdown {
        lf = lf.filter(col("max_drawdown").lt_eq(lit(max)));
    }

    let sort_opts = SortMultipleOptions::new().with_order_descending(true);
    lf = lf.sort(["sharpe"], sort_opts);
    lf = lf.limit(n as u32);

    lf.collect()
}

/// Get parameter heatmap data for 2D parameter sweeps.
///
/// Returns a DataFrame with entry_lookback, exit_lookback, and the specified metric
/// suitable for plotting as a heatmap.
pub fn parameter_heatmap(result: &SweepResult, metric: RankMetric) -> PolarsResult<DataFrame> {
    let df = sweep_to_dataframe(result)?;
    let metric_col = metric_to_col_name(metric);

    df.lazy()
        .select([
            col("entry_lookback"),
            col("exit_lookback"),
            col(metric_col).alias("value"),
        ])
        .collect()
}

/// Calculate parameter sensitivity: variance of metric across parameter changes.
pub fn parameter_sensitivity(result: &SweepResult, metric: RankMetric) -> PolarsResult<DataFrame> {
    let df = sweep_to_dataframe(result)?;
    let metric_col = metric_to_col_name(metric);

    // Group by entry_lookback and compute variance of metric across exit_lookbacks
    let entry_sensitivity = df
        .clone()
        .lazy()
        .group_by([col("entry_lookback")])
        .agg([
            col(metric_col).mean().alias("mean_value"),
            col(metric_col).var(0).alias("variance"),
            len().alias("n_configs"),
        ])
        .with_column(lit("entry_lookback").alias("parameter"))
        .select([
            col("entry_lookback")
                .cast(DataType::String)
                .alias("param_value"),
            col("parameter"),
            col("mean_value"),
            col("variance"),
            col("n_configs"),
        ])
        .collect()?;

    // Group by exit_lookback
    let exit_sensitivity = df
        .lazy()
        .group_by([col("exit_lookback")])
        .agg([
            col(metric_col).mean().alias("mean_value"),
            col(metric_col).var(0).alias("variance"),
            len().alias("n_configs"),
        ])
        .with_column(lit("exit_lookback").alias("parameter"))
        .select([
            col("exit_lookback")
                .cast(DataType::String)
                .alias("param_value"),
            col("parameter"),
            col("mean_value"),
            col("variance"),
            col("n_configs"),
        ])
        .collect()?;

    // Combine results
    let lazy_frames = vec![entry_sensitivity.lazy(), exit_sensitivity.lazy()];
    concat(lazy_frames, UnionArgs::default())?.collect()
}

/// Compare strategies across a multi-strategy sweep.
pub fn compare_strategies(result: &MultiStrategySweepResult) -> PolarsResult<DataFrame> {
    let df = multi_sweep_to_dataframe(result)?;

    df.lazy()
        .group_by([col("strategy_type")])
        .agg([
            len().alias("n_configs"),
            col("sharpe").mean().alias("avg_sharpe"),
            col("sharpe").max().alias("best_sharpe"),
            col("sharpe").min().alias("worst_sharpe"),
            col("cagr").mean().alias("avg_cagr"),
            col("cagr").max().alias("best_cagr"),
            col("max_drawdown").mean().alias("avg_drawdown"),
            col("max_drawdown").max().alias("worst_drawdown"),
            col("num_trades").mean().alias("avg_trades"),
        ])
        .sort(
            ["best_sharpe"],
            SortMultipleOptions::new().with_order_descending(true),
        )
        .collect()
}

/// Convert RankMetric to DataFrame column name.
fn metric_to_col_name(metric: RankMetric) -> &'static str {
    match metric {
        RankMetric::Sharpe => "sharpe",
        RankMetric::Cagr => "cagr",
        RankMetric::Sortino => "sortino",
        RankMetric::MaxDrawdown => "max_drawdown",
        RankMetric::Calmar => "calmar",
        RankMetric::WinRate => "win_rate",
        RankMetric::ProfitFactor => "profit_factor",
        RankMetric::TotalReturn => "total_return",
    }
}

/// Write sweep results to Parquet file.
pub fn write_sweep_parquet(result: &SweepResult, path: &std::path::Path) -> PolarsResult<()> {
    let mut df = sweep_to_dataframe(result)?;

    // Create parent directories
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            PolarsError::ComputeError(format!("Failed to create directory: {}", e).into())
        })?;
    }

    let file = std::fs::File::create(path)
        .map_err(|e| PolarsError::ComputeError(format!("Failed to create file: {}", e).into()))?;
    ParquetWriter::new(file).finish(&mut df)?;

    Ok(())
}

/// Read sweep results from Parquet file.
pub fn read_sweep_parquet(path: &std::path::Path) -> PolarsResult<DataFrame> {
    LazyFrame::scan_parquet(path, ScanArgsParquet::default())?.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::{BacktestResult, EquityPoint};
    use crate::metrics::Metrics;
    use crate::sweep::{ConfigId, SweepConfigResult, SweepResult};
    use chrono::{TimeZone, Utc};

    fn make_test_sweep_result() -> SweepResult {
        let config_results = vec![
            SweepConfigResult {
                config_id: ConfigId::new(20, 10),
                backtest_result: BacktestResult {
                    equity: vec![EquityPoint {
                        ts: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                        cash: 0.0,
                        position_qty: 100.0,
                        close: 105.0,
                        equity: 10500.0,
                    }],
                    trades: vec![],
                    fills: vec![],
                    pyramid_trades: vec![],
                },
                metrics: Metrics {
                    total_return: 0.05,
                    cagr: 0.05,
                    sharpe: 1.2,
                    sortino: 1.5,
                    max_drawdown: 0.10,
                    calmar: 0.5,
                    win_rate: 0.55,
                    profit_factor: 1.5,
                    num_trades: 10,
                    turnover: 2.0,
                },
            },
            SweepConfigResult {
                config_id: ConfigId::new(30, 15),
                backtest_result: BacktestResult {
                    equity: vec![EquityPoint {
                        ts: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                        cash: 0.0,
                        position_qty: 100.0,
                        close: 110.0,
                        equity: 11000.0,
                    }],
                    trades: vec![],
                    fills: vec![],
                    pyramid_trades: vec![],
                },
                metrics: Metrics {
                    total_return: 0.10,
                    cagr: 0.10,
                    sharpe: 1.5,
                    sortino: 1.8,
                    max_drawdown: 0.15,
                    calmar: 0.67,
                    win_rate: 0.60,
                    profit_factor: 1.8,
                    num_trades: 15,
                    turnover: 2.5,
                },
            },
            SweepConfigResult {
                config_id: ConfigId::new(40, 20),
                backtest_result: BacktestResult {
                    equity: vec![EquityPoint {
                        ts: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                        cash: 0.0,
                        position_qty: 100.0,
                        close: 95.0,
                        equity: 9500.0,
                    }],
                    trades: vec![],
                    fills: vec![],
                    pyramid_trades: vec![],
                },
                metrics: Metrics {
                    total_return: -0.05,
                    cagr: -0.05,
                    sharpe: -0.3,
                    sortino: -0.4,
                    max_drawdown: 0.25,
                    calmar: -0.2,
                    win_rate: 0.40,
                    profit_factor: 0.8,
                    num_trades: 8,
                    turnover: 1.5,
                },
            },
        ];

        SweepResult {
            sweep_id: "test_sweep".to_string(),
            config_results,
            started_at: Utc::now(),
            completed_at: Utc::now(),
        }
    }

    #[test]
    fn test_sweep_to_dataframe() {
        let result = make_test_sweep_result();
        let df = sweep_to_dataframe(&result).unwrap();

        assert_eq!(df.height(), 3);
        assert!(df.column("sharpe").is_ok());
        assert!(df.column("entry_lookback").is_ok());
        assert!(df.column("exit_lookback").is_ok());
    }

    #[test]
    fn test_sweep_query_basic() {
        let result = make_test_sweep_result();
        let df = sweep_to_dataframe(&result).unwrap();

        let query = SweepQuery::new().min_sharpe(1.0).limit(10);
        let analysis = query.execute(df).unwrap();

        assert_eq!(analysis.count, 2); // Only 2 configs have sharpe >= 1.0
    }

    #[test]
    fn test_sweep_query_with_drawdown_filter() {
        let result = make_test_sweep_result();
        let df = sweep_to_dataframe(&result).unwrap();

        let query = SweepQuery::new().max_drawdown(0.12);
        let analysis = query.execute(df).unwrap();

        assert_eq!(analysis.count, 1); // Only config with 10% drawdown passes
    }

    #[test]
    fn test_top_configs_by_sharpe() {
        let result = make_test_sweep_result();
        let df = top_configs_by_sharpe(&result, 2, Some(0.0), None).unwrap();

        assert_eq!(df.height(), 2);

        // First should be highest sharpe (1.5)
        let sharpe_col = df.column("sharpe").unwrap().f64().unwrap();
        assert!((sharpe_col.get(0).unwrap() - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_parameter_heatmap() {
        let result = make_test_sweep_result();
        let df = parameter_heatmap(&result, RankMetric::Sharpe).unwrap();

        assert_eq!(df.height(), 3);
        assert!(df.column("entry_lookback").is_ok());
        assert!(df.column("exit_lookback").is_ok());
        assert!(df.column("value").is_ok());
    }

    #[test]
    fn test_parameter_sensitivity() {
        let result = make_test_sweep_result();
        let df = parameter_sensitivity(&result, RankMetric::Sharpe).unwrap();

        // Should have 2 rows per unique parameter value
        assert!(df.height() >= 2);
        assert!(df.column("parameter").is_ok());
        assert!(df.column("variance").is_ok());
    }

    #[test]
    fn test_analysis_summary() {
        let result = make_test_sweep_result();
        let df = sweep_to_dataframe(&result).unwrap();
        let query = SweepQuery::new();
        let analysis = query.execute(df).unwrap();

        let summary = analysis.summary().unwrap();
        assert!(summary.column("mean_sharpe").is_ok());
        assert!(summary.column("max_sharpe").is_ok());
    }

    #[test]
    fn test_enrich_with_sector() {
        use std::collections::HashMap;

        // Create a simple DataFrame with symbols
        let df = DataFrame::new(vec![
            Series::new("symbol".into(), vec!["AAPL", "JPM", "XOM", "UNKNOWN"])
                .into(),
            Series::new("value".into(), vec![1.0, 2.0, 3.0, 4.0]).into(),
        ])
        .unwrap();

        // Create sector lookup
        let mut sector_lookup = HashMap::new();
        sector_lookup.insert("AAPL".to_string(), "Technology".to_string());
        sector_lookup.insert("JPM".to_string(), "Financial".to_string());
        sector_lookup.insert("XOM".to_string(), "Energy".to_string());

        // Enrich with sectors
        let enriched = super::enrich_with_sector(df, &sector_lookup).unwrap();

        // Check that sector column was added
        assert!(enriched.column("sector").is_ok());

        let sectors = enriched.column("sector").unwrap().str().unwrap();
        assert_eq!(sectors.get(0), Some("Technology"));
        assert_eq!(sectors.get(1), Some("Financial"));
        assert_eq!(sectors.get(2), Some("Energy"));
        assert_eq!(sectors.get(3), Some("Unknown")); // Unknown ticker
    }

    #[test]
    fn test_enrich_with_sector_empty_lookup() {
        use std::collections::HashMap;

        let df = DataFrame::new(vec![
            Series::new("symbol".into(), vec!["AAPL", "MSFT"]).into(),
        ])
        .unwrap();

        let sector_lookup = HashMap::new();

        let enriched = super::enrich_with_sector(df, &sector_lookup).unwrap();

        let sectors = enriched.column("sector").unwrap().str().unwrap();
        assert_eq!(sectors.get(0), Some("Unknown"));
        assert_eq!(sectors.get(1), Some("Unknown"));
    }
}
