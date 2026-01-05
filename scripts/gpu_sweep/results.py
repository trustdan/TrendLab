"""
Result aggregation and analysis for GPU Mega-Sweep.

Provides utilities for analyzing sweep results, ranking strategies,
and exporting to various formats.
"""

from pathlib import Path

import polars as pl


def write_results(
    df: pl.DataFrame,
    output_path: Path,
    format: str = "parquet",
) -> None:
    """
    Write results to file.

    Args:
        df: Results DataFrame
        output_path: Output file path
        format: Output format ('parquet' or 'csv')
    """
    output_path.parent.mkdir(parents=True, exist_ok=True)

    if format == "parquet":
        df.write_parquet(output_path)
    elif format == "csv":
        df.write_csv(output_path)
    else:
        raise ValueError(f"Unsupported format: {format}")


def load_results(path: Path) -> pl.DataFrame:
    """Load results from Parquet file."""
    return pl.read_parquet(path)


def filter_results(
    df: pl.DataFrame,
    min_sharpe: float | None = None,
    max_drawdown: float | None = None,
    min_trades: int | None = None,
    min_cagr: float | None = None,
    min_win_rate: float | None = None,
) -> pl.DataFrame:
    """
    Filter results by minimum thresholds.

    Args:
        df: Results DataFrame
        min_sharpe: Minimum Sharpe ratio
        max_drawdown: Maximum drawdown (as decimal, e.g., 0.2 = 20%)
        min_trades: Minimum number of trades
        min_cagr: Minimum CAGR
        min_win_rate: Minimum win rate

    Returns:
        Filtered DataFrame
    """
    lf = df.lazy()

    if min_sharpe is not None:
        lf = lf.filter(pl.col("sharpe") >= min_sharpe)
    if max_drawdown is not None:
        lf = lf.filter(pl.col("max_drawdown") <= max_drawdown)
    if min_trades is not None:
        lf = lf.filter(pl.col("num_trades") >= min_trades)
    if min_cagr is not None:
        lf = lf.filter(pl.col("cagr") >= min_cagr)
    if min_win_rate is not None:
        lf = lf.filter(pl.col("win_rate") >= min_win_rate)

    return lf.collect()


def top_n_by_metric(
    df: pl.DataFrame,
    metric: str = "sharpe",
    n: int = 10,
    ascending: bool = False,
) -> pl.DataFrame:
    """
    Get top N results by a specific metric.

    Args:
        df: Results DataFrame
        metric: Column name to sort by
        n: Number of results to return
        ascending: Sort ascending (default descending)

    Returns:
        Top N results
    """
    return (
        df.lazy()
        .sort(metric, descending=not ascending)
        .limit(n)
        .collect()
    )


def strategy_summary(df: pl.DataFrame) -> pl.DataFrame:
    """
    Generate summary statistics by strategy type.

    Args:
        df: Results DataFrame

    Returns:
        Summary DataFrame with stats per strategy
    """
    return (
        df.lazy()
        .group_by("strategy_type")
        .agg([
            pl.len().alias("n_configs"),
            pl.col("sharpe").mean().alias("mean_sharpe"),
            pl.col("sharpe").median().alias("median_sharpe"),
            pl.col("sharpe").max().alias("max_sharpe"),
            pl.col("sharpe").min().alias("min_sharpe"),
            pl.col("sharpe").std().alias("std_sharpe"),
            pl.col("cagr").mean().alias("mean_cagr"),
            pl.col("cagr").max().alias("max_cagr"),
            pl.col("max_drawdown").mean().alias("mean_drawdown"),
            pl.col("max_drawdown").max().alias("worst_drawdown"),
            pl.col("num_trades").mean().alias("avg_trades"),
            pl.col("win_rate").mean().alias("mean_win_rate"),
        ])
        .sort("median_sharpe", descending=True)
        .collect()
    )


def symbol_summary(df: pl.DataFrame) -> pl.DataFrame:
    """
    Generate summary statistics by symbol.

    Args:
        df: Results DataFrame

    Returns:
        Summary DataFrame with stats per symbol
    """
    return (
        df.lazy()
        .group_by("symbol")
        .agg([
            pl.len().alias("n_configs"),
            pl.col("sharpe").mean().alias("mean_sharpe"),
            pl.col("sharpe").median().alias("median_sharpe"),
            pl.col("sharpe").max().alias("max_sharpe"),
            pl.col("cagr").mean().alias("mean_cagr"),
            pl.col("max_drawdown").mean().alias("mean_drawdown"),
        ])
        .sort("median_sharpe", descending=True)
        .collect()
    )


def strategy_symbol_matrix(df: pl.DataFrame) -> pl.DataFrame:
    """
    Generate cross-tabulation of strategy vs symbol performance.

    Args:
        df: Results DataFrame

    Returns:
        Pivot table with mean Sharpe for each strategy-symbol combination
    """
    return (
        df.lazy()
        .group_by(["strategy_type", "symbol"])
        .agg([pl.col("sharpe").mean().alias("mean_sharpe")])
        .collect()
        .pivot(
            on="symbol",
            index="strategy_type",
            values="mean_sharpe",
        )
    )


def robustness_analysis(df: pl.DataFrame) -> pl.DataFrame:
    """
    Analyze robustness of configs across symbols.

    A robust config performs well consistently across multiple symbols.

    Args:
        df: Results DataFrame

    Returns:
        DataFrame with robustness metrics per config
    """
    # Group by strategy_type and config parameters to find identical configs
    # Since configs have different param columns, we use config_id as proxy
    return (
        df.lazy()
        .group_by(["strategy_type", "config_id"])
        .agg([
            pl.col("symbol").n_unique().alias("n_symbols"),
            pl.col("sharpe").mean().alias("mean_sharpe"),
            pl.col("sharpe").std().alias("std_sharpe"),
            pl.col("sharpe").min().alias("min_sharpe"),
            (pl.col("sharpe") > 0).mean().alias("win_ratio"),
        ])
        # Robustness score: mean sharpe minus penalty for variance
        .with_columns([
            (pl.col("mean_sharpe") - 0.5 * pl.col("std_sharpe")).alias("robust_score"),
        ])
        .sort("robust_score", descending=True)
        .collect()
    )


def compare_with_rust(
    gpu_df: pl.DataFrame,
    rust_df: pl.DataFrame,
    tolerance: float = 1e-6,
) -> dict[str, dict]:
    """
    Compare GPU results with Rust results for validation.

    Args:
        gpu_df: GPU sweep results
        rust_df: Rust sweep results
        tolerance: Maximum acceptable difference

    Returns:
        Dict with comparison results per metric
    """
    import numpy as np

    metrics = ["sharpe", "cagr", "max_drawdown", "sortino", "calmar", "win_rate"]
    results = {}

    for metric in metrics:
        if metric not in gpu_df.columns or metric not in rust_df.columns:
            results[metric] = {"status": "skipped", "reason": "missing column"}
            continue

        gpu_vals = gpu_df[metric].to_numpy()
        rust_vals = rust_df[metric].to_numpy()

        if len(gpu_vals) != len(rust_vals):
            results[metric] = {
                "status": "failed",
                "reason": f"length mismatch ({len(gpu_vals)} vs {len(rust_vals)})",
            }
            continue

        diff = np.abs(gpu_vals - rust_vals)
        max_diff = float(diff.max())
        mean_diff = float(diff.mean())

        if max_diff <= tolerance:
            results[metric] = {
                "status": "passed",
                "max_diff": max_diff,
                "mean_diff": mean_diff,
            }
        else:
            results[metric] = {
                "status": "failed",
                "max_diff": max_diff,
                "mean_diff": mean_diff,
            }

    return results


def generate_report(df: pl.DataFrame) -> str:
    """
    Generate a text report summarizing sweep results.

    Args:
        df: Results DataFrame

    Returns:
        Formatted text report
    """
    lines = []
    lines.append("=" * 60)
    lines.append("GPU MEGA-SWEEP RESULTS REPORT")
    lines.append("=" * 60)
    lines.append("")

    # Overall stats
    lines.append("OVERALL STATISTICS")
    lines.append("-" * 40)
    lines.append(f"Total results: {len(df):,}")
    lines.append(f"Unique symbols: {df['symbol'].n_unique()}")
    lines.append(f"Unique strategies: {df['strategy_type'].n_unique()}")
    lines.append("")

    # Best overall
    lines.append("TOP 10 CONFIGS BY SHARPE")
    lines.append("-" * 40)
    top = top_n_by_metric(df, "sharpe", 10)
    for row in top.iter_rows(named=True):
        lines.append(
            f"  {row['symbol']}/{row['strategy_type']}: "
            f"Sharpe={row['sharpe']:.3f}, CAGR={row['cagr']:.1%}, "
            f"MaxDD={row['max_drawdown']:.1%}"
        )
    lines.append("")

    # Strategy summary
    lines.append("STRATEGY PERFORMANCE SUMMARY")
    lines.append("-" * 40)
    summary = strategy_summary(df)
    for row in summary.iter_rows(named=True):
        lines.append(
            f"  {row['strategy_type']}: "
            f"Median Sharpe={row['median_sharpe']:.3f} "
            f"(n={row['n_configs']:,})"
        )
    lines.append("")

    # Robustness analysis
    lines.append("MOST ROBUST CONFIGS")
    lines.append("-" * 40)
    robust = robustness_analysis(df).head(5)
    for row in robust.iter_rows(named=True):
        lines.append(
            f"  {row['strategy_type']} (config {row['config_id']}): "
            f"Robust={row['robust_score']:.3f}, "
            f"Win%={row['win_ratio']:.0%} across {row['n_symbols']} symbols"
        )
    lines.append("")

    lines.append("=" * 60)

    return "\n".join(lines)
