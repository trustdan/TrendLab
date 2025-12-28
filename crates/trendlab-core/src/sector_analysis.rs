//! Sector-based analysis using Polars.
//!
//! Provides analytical functions for understanding strategy performance
//! across market sectors, enabling:
//! - Sector performance aggregation
//! - Intra-sector dispersion analysis
//! - Top performers per sector
//! - Cross-sector comparisons

use polars::prelude::*;

/// Aggregate performance metrics by sector.
///
/// Groups the DataFrame by sector and computes summary statistics
/// for key performance metrics (Sharpe, CAGR, drawdown, etc.).
///
/// # Arguments
/// * `df` - DataFrame with columns: sector, sharpe, cagr, max_drawdown, num_trades, sortino, calmar
///
/// # Returns
/// DataFrame with one row per sector containing aggregated metrics:
/// - sector, n_configs, avg_sharpe, best_sharpe, avg_cagr, best_cagr,
///   avg_drawdown, worst_drawdown, avg_trades, avg_sortino, avg_calmar
pub fn sector_performance(df: DataFrame) -> PolarsResult<DataFrame> {
    df.lazy()
        .group_by([col("sector")])
        .agg([
            len().alias("n_configs"),
            // Sharpe statistics
            col("sharpe").mean().alias("avg_sharpe"),
            col("sharpe").max().alias("best_sharpe"),
            col("sharpe").min().alias("worst_sharpe"),
            col("sharpe").std(0).alias("sharpe_std"),
            // CAGR statistics
            col("cagr").mean().alias("avg_cagr"),
            col("cagr").max().alias("best_cagr"),
            col("cagr").min().alias("worst_cagr"),
            // Drawdown statistics (note: higher is worse)
            col("max_drawdown").mean().alias("avg_drawdown"),
            col("max_drawdown").max().alias("worst_drawdown"),
            col("max_drawdown").min().alias("best_drawdown"),
            // Trade statistics
            col("num_trades").mean().alias("avg_trades"),
            col("num_trades").sum().alias("total_trades"),
            // Additional risk metrics
            col("sortino").mean().alias("avg_sortino"),
            col("calmar").mean().alias("avg_calmar"),
            col("win_rate").mean().alias("avg_win_rate"),
            col("profit_factor").mean().alias("avg_profit_factor"),
        ])
        .sort(
            ["avg_sharpe"],
            SortMultipleOptions::new().with_order_descending(true),
        )
        .collect()
}

/// Analyze dispersion of performance within each sector.
///
/// Measures how much variation exists within each sector,
/// useful for identifying sectors with consistent vs. variable performance.
///
/// # Arguments
/// * `df` - DataFrame with sector and metric columns
/// * `metric` - Column name to analyze dispersion for (e.g., "sharpe", "cagr")
///
/// # Returns
/// DataFrame with dispersion statistics per sector:
/// - sector, n_configs, mean, std, min, max, range, cv (coefficient of variation)
pub fn sector_dispersion(df: DataFrame, metric: &str) -> PolarsResult<DataFrame> {
    df.lazy()
        .group_by([col("sector")])
        .agg([
            len().alias("n_configs"),
            col(metric).mean().alias("mean"),
            col(metric).std(0).alias("std"),
            col(metric).min().alias("min"),
            col(metric).max().alias("max"),
            (col(metric).max() - col(metric).min()).alias("range"),
            col(metric).median().alias("median"),
            col(metric)
                .quantile(lit(0.25), QuantileMethod::Linear)
                .alias("q25"),
            col(metric)
                .quantile(lit(0.75), QuantileMethod::Linear)
                .alias("q75"),
        ])
        .with_column(
            // Coefficient of variation = std / |mean| (handle zero mean)
            when(col("mean").abs().gt(lit(1e-10)))
                .then(col("std") / col("mean").abs())
                .otherwise(lit(f64::NAN))
                .alias("cv"),
        )
        .with_column(
            // IQR = Q75 - Q25
            (col("q75") - col("q25")).alias("iqr"),
        )
        .sort(
            ["cv"],
            SortMultipleOptions::new().with_order_descending(false), // Lower CV = more consistent
        )
        .collect()
}

/// Get top N configurations per sector.
///
/// Returns the best-performing configurations within each sector,
/// enabling sector-specific strategy selection.
///
/// # Arguments
/// * `df` - DataFrame with sector and metric columns
/// * `n` - Number of top configs to return per sector
/// * `metric` - Column name to rank by (e.g., "sharpe")
/// * `descending` - If true, higher values are better
///
/// # Returns
/// DataFrame with top N configs per sector, sorted by sector then metric
pub fn top_per_sector(
    df: DataFrame,
    n: usize,
    metric: &str,
    descending: bool,
) -> PolarsResult<DataFrame> {
    let sort_opts = SortMultipleOptions::new().with_order_descending(descending);

    df.lazy()
        .sort([metric], sort_opts.clone())
        .group_by([col("sector")])
        .head(Some(n))
        .sort(
            ["sector", metric],
            SortMultipleOptions::new().with_order_descending_multi([false, descending]),
        )
        .collect()
}

/// Compare sector averages against overall universe averages.
///
/// Identifies sectors that outperform or underperform relative to the universe,
/// helping to identify favorable or unfavorable market segments.
///
/// # Arguments
/// * `df` - DataFrame with sector and metric columns
///
/// # Returns
/// DataFrame with sector metrics and relative performance (diff from universe mean)
pub fn sector_vs_universe(df: DataFrame) -> PolarsResult<DataFrame> {
    // First compute universe-wide averages
    let universe_avg = df
        .clone()
        .lazy()
        .select([
            col("sharpe").mean().alias("universe_sharpe"),
            col("cagr").mean().alias("universe_cagr"),
            col("max_drawdown").mean().alias("universe_drawdown"),
            col("sortino").mean().alias("universe_sortino"),
        ])
        .collect()?;

    // Extract universe values
    let universe_sharpe = universe_avg
        .column("universe_sharpe")?
        .f64()?
        .get(0)
        .unwrap_or(0.0);
    let universe_cagr = universe_avg
        .column("universe_cagr")?
        .f64()?
        .get(0)
        .unwrap_or(0.0);
    let universe_drawdown = universe_avg
        .column("universe_drawdown")?
        .f64()?
        .get(0)
        .unwrap_or(0.0);
    let universe_sortino = universe_avg
        .column("universe_sortino")?
        .f64()?
        .get(0)
        .unwrap_or(0.0);

    // Compute sector averages with relative differences
    df.lazy()
        .group_by([col("sector")])
        .agg([
            len().alias("n_configs"),
            col("sharpe").mean().alias("avg_sharpe"),
            col("cagr").mean().alias("avg_cagr"),
            col("max_drawdown").mean().alias("avg_drawdown"),
            col("sortino").mean().alias("avg_sortino"),
        ])
        .with_columns([
            (col("avg_sharpe") - lit(universe_sharpe)).alias("sharpe_vs_universe"),
            (col("avg_cagr") - lit(universe_cagr)).alias("cagr_vs_universe"),
            (col("avg_drawdown") - lit(universe_drawdown)).alias("drawdown_vs_universe"),
            (col("avg_sortino") - lit(universe_sortino)).alias("sortino_vs_universe"),
        ])
        .with_column(
            // Composite score: sum of normalized differences (Sharpe matters most)
            (col("sharpe_vs_universe") * lit(2.0)
                + col("cagr_vs_universe")
                - col("drawdown_vs_universe") // Lower drawdown is better
                + col("sortino_vs_universe"))
            .alias("relative_score"),
        )
        .sort(
            ["relative_score"],
            SortMultipleOptions::new().with_order_descending(true),
        )
        .collect()
}

/// Identify best sector-strategy combinations.
///
/// For each sector, identifies which strategy types work best,
/// enabling sector-specific strategy allocation.
///
/// # Arguments
/// * `df` - DataFrame with sector, strategy_type, and metric columns
/// * `metric` - Column to rank by
///
/// # Returns
/// DataFrame with best strategy per sector
pub fn best_strategy_per_sector(df: DataFrame, metric: &str) -> PolarsResult<DataFrame> {
    df.lazy()
        .group_by([col("sector"), col("strategy_type")])
        .agg([
            len().alias("n_configs"),
            col(metric).mean().alias("avg_metric"),
            col(metric).max().alias("best_metric"),
        ])
        .sort(
            ["sector", "avg_metric"],
            SortMultipleOptions::new().with_order_descending_multi([false, true]),
        )
        .group_by([col("sector")])
        .head(Some(1)) // Top strategy per sector
        .sort(
            ["avg_metric"],
            SortMultipleOptions::new().with_order_descending(true),
        )
        .collect()
}

/// Compute sector concentration metrics.
///
/// Analyzes how performance is distributed across sectors,
/// identifying whether performance is concentrated in few sectors
/// or distributed evenly.
///
/// # Arguments
/// * `df` - DataFrame with sector column
/// * `metric` - Metric to analyze concentration for
///
/// # Returns
/// DataFrame with concentration statistics
pub fn sector_concentration(df: DataFrame, metric: &str) -> PolarsResult<DataFrame> {
    // First get totals per sector and overall
    let totals_df = df
        .clone()
        .lazy()
        .group_by([col("sector")])
        .agg([
            col(metric).sum().alias("sector_total"),
            len().alias("n_configs"),
        ])
        .collect()?;

    // Get total across all sectors
    let total_df = totals_df
        .clone()
        .lazy()
        .select([col("sector_total").sum().alias("grand_total")])
        .collect()?;

    let total: f64 = total_df.column("grand_total")?.f64()?.get(0).unwrap_or(1.0);

    // Add percentage columns
    totals_df
        .lazy()
        .with_column((col("sector_total") / lit(total) * lit(100.0)).alias("pct_of_total"))
        .sort(
            ["sector_total"],
            SortMultipleOptions::new().with_order_descending(true),
        )
        .with_column(
            // Cumulative percentage for concentration curve
            col("pct_of_total").cum_sum(false).alias("cumulative_pct"),
        )
        .collect()
}

/// Filter DataFrame to specific sectors.
///
/// Convenience function to subset data to sectors of interest.
///
/// # Arguments
/// * `df` - DataFrame with sector column
/// * `sectors` - List of sector names to include
///
/// # Returns
/// Filtered DataFrame containing only the specified sectors
pub fn filter_sectors(df: DataFrame, sectors: &[&str]) -> PolarsResult<DataFrame> {
    // Build OR expression for sector matching
    let mut filter_expr: Option<Expr> = None;
    for sector in sectors {
        let eq_expr = col("sector").eq(lit(*sector));
        filter_expr = Some(match filter_expr {
            Some(expr) => expr.or(eq_expr),
            None => eq_expr,
        });
    }

    match filter_expr {
        Some(expr) => df.lazy().filter(expr).collect(),
        None => Ok(df), // No sectors specified, return original
    }
}

/// Sector summary with rank within universe.
///
/// Provides a comprehensive sector summary with ranking information.
/// Uses row numbers after sorting as a simple ranking mechanism.
///
/// # Arguments
/// * `df` - DataFrame with sector and metric columns
///
/// # Returns
/// DataFrame with sector summary and ranks
pub fn sector_summary_ranked(df: DataFrame) -> PolarsResult<DataFrame> {
    let perf = sector_performance(df)?;
    let n = perf.height();

    // Create rankings by sorting and assigning row numbers
    // For Sharpe: higher is better, so sort descending
    let sharpe_ranked = perf
        .clone()
        .lazy()
        .sort(
            ["avg_sharpe"],
            SortMultipleOptions::new().with_order_descending(true),
        )
        .with_row_index("sharpe_rank", Some(1))
        .collect()?;

    // For CAGR: higher is better
    let cagr_ranked = perf
        .clone()
        .lazy()
        .sort(
            ["avg_cagr"],
            SortMultipleOptions::new().with_order_descending(true),
        )
        .with_row_index("cagr_rank", Some(1))
        .collect()?;

    // For Drawdown: lower is better, so sort ascending
    let dd_ranked = perf
        .clone()
        .lazy()
        .sort(
            ["avg_drawdown"],
            SortMultipleOptions::new().with_order_descending(false),
        )
        .with_row_index("drawdown_rank", Some(1))
        .collect()?;

    // Create lookup maps from sector to rank
    let mut sharpe_ranks = std::collections::HashMap::new();
    let mut cagr_ranks = std::collections::HashMap::new();
    let mut dd_ranks = std::collections::HashMap::new();

    for i in 0..n {
        let sector = sharpe_ranked.column("sector")?.str()?.get(i).unwrap_or("");
        let rank = sharpe_ranked
            .column("sharpe_rank")?
            .idx()?
            .get(i)
            .unwrap_or(0) as f64;
        sharpe_ranks.insert(sector.to_string(), rank);

        let sector = cagr_ranked.column("sector")?.str()?.get(i).unwrap_or("");
        let rank = cagr_ranked.column("cagr_rank")?.idx()?.get(i).unwrap_or(0) as f64;
        cagr_ranks.insert(sector.to_string(), rank);

        let sector = dd_ranked.column("sector")?.str()?.get(i).unwrap_or("");
        let rank = dd_ranked
            .column("drawdown_rank")?
            .idx()?
            .get(i)
            .unwrap_or(0) as f64;
        dd_ranks.insert(sector.to_string(), rank);
    }

    // Add rank columns to perf DataFrame
    let sectors: Vec<String> = perf
        .column("sector")?
        .str()?
        .into_iter()
        .map(|s| s.unwrap_or("").to_string())
        .collect();

    let sharpe_rank_col: Vec<f64> = sectors
        .iter()
        .map(|s| *sharpe_ranks.get(s).unwrap_or(&(n as f64)))
        .collect();
    let cagr_rank_col: Vec<f64> = sectors
        .iter()
        .map(|s| *cagr_ranks.get(s).unwrap_or(&(n as f64)))
        .collect();
    let dd_rank_col: Vec<f64> = sectors
        .iter()
        .map(|s| *dd_ranks.get(s).unwrap_or(&(n as f64)))
        .collect();

    let composite_rank: Vec<f64> = (0..n)
        .map(|i| (sharpe_rank_col[i] + cagr_rank_col[i] + dd_rank_col[i]) / 3.0)
        .collect();

    let mut result = perf;
    result = result
        .with_column(Series::new("sharpe_rank".into(), sharpe_rank_col))?
        .clone();
    result = result
        .with_column(Series::new("cagr_rank".into(), cagr_rank_col))?
        .clone();
    result = result
        .with_column(Series::new("drawdown_rank".into(), dd_rank_col))?
        .clone();
    result = result
        .with_column(Series::new("composite_rank".into(), composite_rank))?
        .clone();

    result
        .lazy()
        .sort(
            ["composite_rank"],
            SortMultipleOptions::new().with_order_descending(false),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_df() -> DataFrame {
        DataFrame::new(vec![
            Series::new(
                "symbol".into(),
                vec!["AAPL", "MSFT", "JPM", "GS", "XOM", "CVX"],
            )
            .into(),
            Series::new(
                "sector".into(),
                vec![
                    "Technology",
                    "Technology",
                    "Financial",
                    "Financial",
                    "Energy",
                    "Energy",
                ],
            )
            .into(),
            Series::new(
                "strategy_type".into(),
                vec![
                    "donchian", "donchian", "donchian", "ma_cross", "donchian", "ma_cross",
                ],
            )
            .into(),
            Series::new("sharpe".into(), vec![1.5, 1.2, 0.8, 0.9, 1.1, 1.0]).into(),
            Series::new("cagr".into(), vec![0.15, 0.12, 0.08, 0.09, 0.11, 0.10]).into(),
            Series::new("sortino".into(), vec![2.0, 1.8, 1.2, 1.3, 1.5, 1.4]).into(),
            Series::new("calmar".into(), vec![1.0, 0.9, 0.6, 0.7, 0.8, 0.75]).into(),
            Series::new(
                "max_drawdown".into(),
                vec![0.15, 0.18, 0.22, 0.20, 0.19, 0.21],
            )
            .into(),
            Series::new("num_trades".into(), vec![20u32, 25, 15, 18, 22, 19]).into(),
            Series::new("win_rate".into(), vec![0.55, 0.52, 0.48, 0.50, 0.53, 0.51]).into(),
            Series::new("profit_factor".into(), vec![1.8, 1.6, 1.3, 1.4, 1.5, 1.45]).into(),
        ])
        .unwrap()
    }

    #[test]
    fn test_sector_performance() {
        let df = create_test_df();
        let result = sector_performance(df).unwrap();

        assert_eq!(result.height(), 3); // 3 sectors
        assert!(result.column("n_configs").is_ok());
        assert!(result.column("avg_sharpe").is_ok());
        assert!(result.column("best_sharpe").is_ok());

        // Technology should be first (highest avg Sharpe)
        let sectors = result.column("sector").unwrap().str().unwrap();
        assert_eq!(sectors.get(0), Some("Technology"));
    }

    #[test]
    fn test_sector_dispersion() {
        let df = create_test_df();
        let result = sector_dispersion(df, "sharpe").unwrap();

        assert_eq!(result.height(), 3);
        assert!(result.column("std").is_ok());
        assert!(result.column("cv").is_ok());
        assert!(result.column("iqr").is_ok());
    }

    #[test]
    fn test_top_per_sector() {
        let df = create_test_df();
        let result = top_per_sector(df, 1, "sharpe", true).unwrap();

        // Should have 1 top config per sector = 3 rows
        assert_eq!(result.height(), 3);

        // Check that we got the best from each sector
        let symbols = result.column("symbol").unwrap().str().unwrap();
        let sharpes = result.column("sharpe").unwrap().f64().unwrap();

        // AAPL should be the top for Technology (Sharpe 1.5)
        // Find the Technology row
        for i in 0..result.height() {
            let sector = result.column("sector").unwrap().str().unwrap().get(i);
            if sector == Some("Technology") {
                assert_eq!(symbols.get(i), Some("AAPL"));
                assert!((sharpes.get(i).unwrap() - 1.5).abs() < 0.001);
            }
        }
    }

    #[test]
    fn test_sector_vs_universe() {
        let df = create_test_df();
        let result = sector_vs_universe(df).unwrap();

        assert_eq!(result.height(), 3);
        assert!(result.column("sharpe_vs_universe").is_ok());
        assert!(result.column("relative_score").is_ok());

        // Technology should have positive vs_universe (above average)
        let tech_row = result
            .clone()
            .lazy()
            .filter(col("sector").eq(lit("Technology")))
            .collect()
            .unwrap();
        let sharpe_diff = tech_row
            .column("sharpe_vs_universe")
            .unwrap()
            .f64()
            .unwrap()
            .get(0)
            .unwrap();
        assert!(sharpe_diff > 0.0);
    }

    #[test]
    fn test_best_strategy_per_sector() {
        let df = create_test_df();
        let result = best_strategy_per_sector(df, "sharpe").unwrap();

        assert_eq!(result.height(), 3);
        assert!(result.column("strategy_type").is_ok());
    }

    #[test]
    fn test_filter_sectors() {
        let df = create_test_df();
        let result = filter_sectors(df, &["Technology", "Energy"]).unwrap();

        assert_eq!(result.height(), 4); // 2 Tech + 2 Energy

        let sectors: Vec<_> = result
            .column("sector")
            .unwrap()
            .str()
            .unwrap()
            .into_iter()
            .flatten()
            .collect();
        assert!(sectors.iter().all(|s| *s == "Technology" || *s == "Energy"));
    }

    #[test]
    fn test_sector_summary_ranked() {
        let df = create_test_df();
        let result = sector_summary_ranked(df).unwrap();

        assert_eq!(result.height(), 3);
        assert!(result.column("sharpe_rank").is_ok());
        assert!(result.column("composite_rank").is_ok());
    }
}
