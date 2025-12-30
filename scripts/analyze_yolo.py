#!/usr/bin/env python3
# IMPORTANT: Warnings must be filtered BEFORE any imports to avoid numpy crashes
import warnings
warnings.filterwarnings('ignore')
import os
os.environ['PYTHONWARNINGS'] = 'ignore'

"""
TrendLab YOLO Run Analysis Script

Analyzes sweep results to extract insights about winning strategies:
1. Strategy type comparison
2. Parameter patterns
3. Sector analysis
4. Robustness metrics

Usage:
    python scripts/analyze_yolo.py [--export-csv] [--charts]
"""

import json
import argparse
import sys
from pathlib import Path
from typing import Any

# Force UTF-8 output on Windows
if sys.platform == 'win32':
    sys.stdout.reconfigure(encoding='utf-8')
    sys.stderr.reconfigure(encoding='utf-8')

import polars as pl
pl.Config.set_fmt_str_lengths(50)

# Try to import matplotlib
try:
    import matplotlib.pyplot as plt
    MATPLOTLIB_AVAILABLE = True
except ImportError:
    MATPLOTLIB_AVAILABLE = False


# =============================================================================
# Data Loading
# =============================================================================

def load_leaderboard(path: Path) -> pl.DataFrame:
    """Load individual symbol/config leaderboard and flatten to DataFrame."""
    with open(path, 'r') as f:
        data = json.load(f)

    rows = []
    for entry in data.get('entries', []):
        config = entry.get('config', {})
        strategy_type = entry.get('strategy_type', 'Unknown')

        # Extract config parameters
        params = config.get(strategy_type, {})

        # Extract period info from equity curve and dates
        equity_curve = entry.get('equity_curve', [])
        dates = entry.get('dates', [])
        num_bars = len(equity_curve)

        # Parse dates to get period
        start_date = None
        end_date = None
        period_days = None
        if dates and len(dates) >= 2:
            try:
                # Parse ISO format dates
                start_date = dates[0][:10] if dates[0] else None
                end_date = dates[-1][:10] if dates[-1] else None
                if start_date and end_date:
                    from datetime import datetime
                    d1 = datetime.fromisoformat(start_date)
                    d2 = datetime.fromisoformat(end_date)
                    period_days = (d2 - d1).days
            except Exception:
                pass

        row = {
            'rank': entry.get('rank'),
            'strategy_type': strategy_type,
            'symbol': entry.get('symbol'),
            'sector': entry.get('sector'),
            'num_bars': num_bars,
            'start_date': start_date,
            'end_date': end_date,
            'period_days': period_days,
            **{f'param_{k}': v for k, v in params.items()},
            **entry.get('metrics', {})
        }
        rows.append(row)

    return pl.DataFrame(rows)


def load_cross_symbol_leaderboard(path: Path) -> tuple[pl.DataFrame, pl.DataFrame]:
    """
    Load cross-symbol leaderboard and flatten to:
    - config_df: One row per config with aggregate stats
    - per_symbol_df: One row per config+symbol combination
    """
    with open(path, 'r') as f:
        data = json.load(f)

    config_rows = []
    per_symbol_rows = []

    for entry in data.get('entries', []):
        config_id = entry.get('config_id', {})
        strategy_type = entry.get('strategy_type', 'Unknown')
        # Handle both dict and string config_id (e.g., TurtleS1/S2 have no params)
        if isinstance(config_id, dict):
            params = config_id.get(strategy_type, {})
        else:
            params = {}

        # Create config identifier string
        config_str = '_'.join(f"{k}={v}" for k, v in sorted(params.items()))

        # Extract period info from combined equity curve and dates
        equity_curve = entry.get('combined_equity_curve', [])
        dates = entry.get('dates', [])
        num_bars = len(equity_curve)

        # Parse dates to get period
        start_date = None
        end_date = None
        period_days = None
        if dates and len(dates) >= 2:
            try:
                start_date = dates[0][:10] if dates[0] else None
                end_date = dates[-1][:10] if dates[-1] else None
                if start_date and end_date:
                    from datetime import datetime
                    d1 = datetime.fromisoformat(start_date)
                    d2 = datetime.fromisoformat(end_date)
                    period_days = (d2 - d1).days
            except Exception:
                pass

        config_row = {
            'rank': entry.get('rank'),
            'strategy_type': strategy_type,
            'config_str': config_str,
            'num_symbols': len(entry.get('symbols', [])),
            'num_bars': num_bars,
            'start_date': start_date,
            'end_date': end_date,
            'period_days': period_days,
            **{f'param_{k}': v for k, v in params.items()},
        }
        config_rows.append(config_row)

        # Per-symbol metrics
        per_symbol_metrics = entry.get('per_symbol_metrics', {})
        per_symbol_sectors = entry.get('per_symbol_sectors', {})

        for symbol, metrics in per_symbol_metrics.items():
            ps_row = {
                'config_rank': entry.get('rank'),
                'strategy_type': strategy_type,
                'config_str': config_str,
                'symbol': symbol,
                'sector': per_symbol_sectors.get(symbol, 'unknown'),
                'num_bars': num_bars,
                'start_date': start_date,
                'end_date': end_date,
                'period_days': period_days,
                **{f'param_{k}': v for k, v in params.items()},
                **metrics
            }
            per_symbol_rows.append(ps_row)

    return pl.DataFrame(config_rows), pl.DataFrame(per_symbol_rows)


# =============================================================================
# Analysis Functions
# =============================================================================

def analyze_strategy_types(df: pl.DataFrame) -> pl.DataFrame:
    """Compare performance across strategy types."""
    return df.group_by('strategy_type').agg([
        pl.count().alias('count'),
        pl.col('cagr').median().alias('median_cagr'),
        pl.col('cagr').mean().alias('mean_cagr'),
        pl.col('sharpe').median().alias('median_sharpe'),
        pl.col('sharpe').mean().alias('mean_sharpe'),
        pl.col('sharpe').std().alias('std_sharpe'),
        pl.col('sortino').median().alias('median_sortino'),
        pl.col('max_drawdown').mean().alias('avg_max_dd'),
        pl.col('calmar').median().alias('median_calmar'),
        pl.col('num_trades').mean().alias('avg_trades'),
        pl.col('win_rate').mean().alias('avg_win_rate'),
    ]).sort('median_sharpe', descending=True)


def analyze_top_performers(df: pl.DataFrame, top_pct: float = 0.1) -> pl.DataFrame:
    """Analyze what percentage of each strategy's configs land in top X%."""
    total = len(df)
    top_n = int(total * top_pct)

    df_ranked = df.with_columns(
        pl.col('sharpe').rank(descending=True).alias('sharpe_rank')
    )

    top_configs = df_ranked.filter(pl.col('sharpe_rank') <= top_n)

    return top_configs.group_by('strategy_type').agg([
        pl.count().alias('in_top_10pct'),
    ]).join(
        df.group_by('strategy_type').count().rename({'count': 'total'}),
        on='strategy_type'
    ).with_columns(
        (pl.col('in_top_10pct') / pl.col('total') * 100).alias('pct_in_top10')
    ).sort('pct_in_top10', descending=True)


def analyze_parameters(df: pl.DataFrame) -> dict[str, pl.DataFrame]:
    """Analyze parameter patterns for each strategy type."""
    results = {}

    for strategy_type in df['strategy_type'].unique().to_list():
        strat_df = df.filter(pl.col('strategy_type') == strategy_type)

        # Get parameter columns
        param_cols = [c for c in strat_df.columns if c.startswith('param_')]

        if not param_cols:
            continue

        # For top 25% by Sharpe, what are the parameter distributions?
        top_25_pct = int(len(strat_df) * 0.25)
        top_df = strat_df.sort('sharpe', descending=True).head(top_25_pct)

        param_stats = []
        for param in param_cols:
            if strat_df[param].dtype in [pl.Float64, pl.Float32, pl.Int64, pl.Int32]:
                stats = {
                    'param': param.replace('param_', ''),
                    'all_median': strat_df[param].median(),
                    'all_min': strat_df[param].min(),
                    'all_max': strat_df[param].max(),
                    'top25_median': top_df[param].median(),
                    'top25_min': top_df[param].min(),
                    'top25_max': top_df[param].max(),
                }
                # Simple correlation with Sharpe (using Polars native correlation)
                try:
                    # Filter out nulls for correlation
                    valid = strat_df.filter(
                        pl.col(param).is_not_null() & pl.col('sharpe').is_not_null()
                    )
                    if len(valid) > 2:
                        corr = valid.select(
                            pl.corr(param, 'sharpe').alias('corr')
                        ).item()
                        stats['corr_with_sharpe'] = corr if corr is not None else 0.0
                    else:
                        stats['corr_with_sharpe'] = 0.0
                except Exception:
                    stats['corr_with_sharpe'] = 0.0

                param_stats.append(stats)

        if param_stats:
            results[strategy_type] = pl.DataFrame(param_stats)

    return results


def analyze_sectors(df: pl.DataFrame) -> tuple[pl.DataFrame, pl.DataFrame]:
    """Analyze performance by sector and strategy+sector combinations."""

    # Overall sector performance
    sector_perf = df.group_by('sector').agg([
        pl.count().alias('count'),
        pl.col('cagr').median().alias('median_cagr'),
        pl.col('sharpe').median().alias('median_sharpe'),
        pl.col('sharpe').std().alias('std_sharpe'),
        pl.col('max_drawdown').mean().alias('avg_max_dd'),
    ]).sort('median_sharpe', descending=True)

    # Strategy + Sector combinations
    combo_perf = df.group_by(['strategy_type', 'sector']).agg([
        pl.count().alias('count'),
        pl.col('cagr').median().alias('median_cagr'),
        pl.col('sharpe').median().alias('median_sharpe'),
        pl.col('sharpe').std().alias('std_sharpe'),
    ]).filter(pl.col('count') >= 5).sort('median_sharpe', descending=True)

    return sector_perf, combo_perf


def analyze_robustness(per_symbol_df: pl.DataFrame, config_df: pl.DataFrame) -> pl.DataFrame:
    """
    Find configs that perform consistently across symbols.
    Robustness = high avg Sharpe + low Sharpe variance across symbols.
    """

    robustness = per_symbol_df.group_by(['strategy_type', 'config_str']).agg([
        pl.count().alias('num_symbols'),
        pl.col('sharpe').mean().alias('avg_sharpe'),
        pl.col('sharpe').std().alias('std_sharpe'),
        pl.col('sharpe').min().alias('min_sharpe'),
        pl.col('sharpe').max().alias('max_sharpe'),
        pl.col('cagr').mean().alias('avg_cagr'),
        pl.col('cagr').std().alias('std_cagr'),
        pl.col('max_drawdown').mean().alias('avg_max_dd'),
        pl.col('max_drawdown').std().alias('std_max_dd'),
        # Count symbols with positive Sharpe
        (pl.col('sharpe') > 0).sum().alias('positive_sharpe_count'),
        # Count symbols with positive CAGR
        (pl.col('cagr') > 0).sum().alias('positive_cagr_count'),
    ]).with_columns([
        # Robustness score: avg_sharpe / (1 + std_sharpe) - penalize variance
        (pl.col('avg_sharpe') / (1 + pl.col('std_sharpe').fill_null(0))).alias('robustness_score'),
        # Win ratio across symbols
        (pl.col('positive_sharpe_count') / pl.col('num_symbols')).alias('symbol_win_ratio'),
    ]).filter(
        pl.col('num_symbols') >= 10  # Only configs tested on enough symbols
    ).sort('robustness_score', descending=True)

    return robustness


def find_universal_winners(per_symbol_df: pl.DataFrame, min_symbols: int = 20) -> pl.DataFrame:
    """Find configs that rank in top 50% for most symbols tested."""

    # Rank each config within each symbol
    ranked = per_symbol_df.with_columns([
        pl.col('sharpe').rank(descending=True).over('symbol').alias('rank_in_symbol'),
        pl.count().over('symbol').alias('configs_per_symbol'),
    ]).with_columns([
        (pl.col('rank_in_symbol') / pl.col('configs_per_symbol')).alias('percentile')
    ])

    # For each config, count how often it's in top 50%
    universal = ranked.group_by(['strategy_type', 'config_str']).agg([
        pl.count().alias('num_symbols'),
        (pl.col('percentile') <= 0.5).sum().alias('top_half_count'),
        (pl.col('percentile') <= 0.25).sum().alias('top_quarter_count'),
        (pl.col('percentile') <= 0.1).sum().alias('top_10pct_count'),
        pl.col('sharpe').mean().alias('avg_sharpe'),
    ]).filter(
        pl.col('num_symbols') >= min_symbols
    ).with_columns([
        (pl.col('top_half_count') / pl.col('num_symbols')).alias('top_half_pct'),
        (pl.col('top_quarter_count') / pl.col('num_symbols')).alias('top_quarter_pct'),
    ]).sort('top_half_pct', descending=True)

    return universal


# =============================================================================
# Output Functions
# =============================================================================

def print_section(title: str):
    """Print a section header."""
    print(f"\n{'='*60}")
    print(title)
    print('='*60 + "\n")


def print_dataframe(df: pl.DataFrame, title: str = "", max_rows: int = 20):
    """Print a DataFrame nicely."""
    if title:
        print(f"\n{title}")
        print("-" * len(title))

    # Use simple ASCII output to avoid encoding issues
    subset = df.head(max_rows)

    # Print header
    cols = df.columns
    header = " | ".join(f"{col:>15}" for col in cols)
    print(header)
    print("-" * len(header))

    # Print rows
    for row in subset.iter_rows():
        formatted = []
        for val in row:
            if val is None:
                formatted.append(f"{'':>15}")
            elif isinstance(val, float):
                formatted.append(f"{val:>15.4f}")
            else:
                formatted.append(f"{str(val):>15}")
        print(" | ".join(formatted))

    if len(df) > max_rows:
        print(f"... showing {max_rows} of {len(df)} rows")


def create_charts(strategy_comparison: pl.DataFrame, sector_perf: pl.DataFrame,
                  robustness: pl.DataFrame, combo_perf: pl.DataFrame, output_dir: Path):
    """Create and save analysis charts."""
    if not MATPLOTLIB_AVAILABLE:
        print("Matplotlib not available - skipping charts")
        return

    output_dir.mkdir(parents=True, exist_ok=True)

    # 1. Strategy comparison bar chart
    fig, ax = plt.subplots(figsize=(12, 6))
    strategies = strategy_comparison['strategy_type'].to_list()
    sharpes = strategy_comparison['median_sharpe'].to_list()

    bars = ax.barh(strategies, sharpes, color='steelblue')
    ax.set_xlabel('Median Sharpe Ratio')
    ax.set_title('Strategy Type Comparison - Median Sharpe')
    ax.axvline(x=0, color='red', linestyle='--', alpha=0.5)

    for bar, sharpe in zip(bars, sharpes):
        ax.text(bar.get_width() + 0.02, bar.get_y() + bar.get_height()/2,
                f'{sharpe:.3f}', va='center')

    plt.tight_layout()
    plt.savefig(output_dir / 'strategy_comparison.png', dpi=150)
    plt.close()

    # 2. Best strategy per sector (actionable chart)
    # Get the best strategy for each sector
    best_per_sector = combo_perf.group_by('sector').agg([
        pl.col('strategy_type').first().alias('best_strategy'),
        pl.col('median_sharpe').first().alias('best_sharpe'),
    ]).sort('best_sharpe', descending=True)

    fig, ax = plt.subplots(figsize=(14, 8))
    sectors = best_per_sector['sector'].to_list()
    sector_sharpes = best_per_sector['best_sharpe'].to_list()
    best_strategies = best_per_sector['best_strategy'].to_list()

    # Color bars by strategy type
    strategy_colors = {
        'Supertrend': '#2E86AB',
        'ParabolicSar': '#A23B72',
        'FiftyTwoWeekHigh': '#F18F01',
        'LarryWilliams': '#C73E1D',
        'STARC': '#3B1F2B',
        'MACrossover': '#95190C',
        'Tsmom': '#610345',
        'Aroon': '#044B7F',
        'Ensemble': '#107E7D',
        'Donchian': '#B4A0E5',
    }
    colors = [strategy_colors.get(s, '#888888') for s in best_strategies]

    bars = ax.barh(sectors, sector_sharpes, color=colors)
    ax.set_xlabel('Median Sharpe Ratio')
    ax.set_title('Best Strategy per Sector')
    ax.axvline(x=0, color='red', linestyle='--', alpha=0.5)

    # Add strategy name labels on bars
    for bar, strategy, sharpe in zip(bars, best_strategies, sector_sharpes):
        ax.text(bar.get_width() + 0.01, bar.get_y() + bar.get_height()/2,
                f'{strategy} ({sharpe:.2f})', va='center', fontsize=8)

    plt.tight_layout()
    plt.savefig(output_dir / 'best_strategy_per_sector.png', dpi=150)
    plt.close()

    # 3. Original sector performance (for reference)
    fig, ax = plt.subplots(figsize=(12, 6))
    sectors = sector_perf['sector'].to_list()
    sector_sharpes = sector_perf['median_sharpe'].to_list()

    bars = ax.barh(sectors, sector_sharpes, color='forestgreen')
    ax.set_xlabel('Median Sharpe Ratio')
    ax.set_title('Sector Performance - Median Sharpe (All Strategies)')
    ax.axvline(x=0, color='red', linestyle='--', alpha=0.5)

    plt.tight_layout()
    plt.savefig(output_dir / 'sector_performance.png', dpi=150)
    plt.close()

    # 3. Top robust configs
    top_robust = robustness.head(15)
    fig, ax = plt.subplots(figsize=(14, 8))

    labels = [f"{row[0]}\n{row[1][:30]}" for row in top_robust.select(['strategy_type', 'config_str']).iter_rows()]
    scores = top_robust['robustness_score'].to_list()

    bars = ax.barh(range(len(labels)), scores, color='darkorange')
    ax.set_yticks(range(len(labels)))
    ax.set_yticklabels(labels, fontsize=8)
    ax.set_xlabel('Robustness Score (Sharpe / (1 + StdDev))')
    ax.set_title('Top 15 Most Robust Configurations')

    plt.tight_layout()
    plt.savefig(output_dir / 'robust_configs.png', dpi=150)
    plt.close()

    print(f"Charts saved to {output_dir}")


# =============================================================================
# Main
# =============================================================================

def main():
    parser = argparse.ArgumentParser(description='Analyze TrendLab YOLO sweep results')
    parser.add_argument('--export-csv', action='store_true', help='Export results to CSV')
    parser.add_argument('--charts', action='store_true', help='Generate visualization charts')
    # Check multiple possible artifact locations
    default_data_dir = Path('apps/trendlab-gui/src-tauri/artifacts')
    if not default_data_dir.exists():
        default_data_dir = Path('target/release/artifacts')
    parser.add_argument('--data-dir', type=Path, default=default_data_dir,
                        help='Directory containing leaderboard JSON files')
    parser.add_argument('--output-dir', type=Path, default=Path('reports/analysis'),
                        help='Output directory for CSVs and charts')
    args = parser.parse_args()

    # Paths
    leaderboard_path = args.data_dir / 'leaderboard.json'
    cross_symbol_path = args.data_dir / 'cross_symbol_leaderboard.json'

    # Load data
    print_section("Loading Data")

    if leaderboard_path.exists():
        df = load_leaderboard(leaderboard_path)
        print(f"Loaded leaderboard: {len(df)} entries")
    else:
        print(f"Warning: {leaderboard_path} not found")
        df = None

    if cross_symbol_path.exists():
        config_df, per_symbol_df = load_cross_symbol_leaderboard(cross_symbol_path)
        print(f"Loaded cross-symbol leaderboard: {len(config_df)} configs, {len(per_symbol_df)} symbol results")
    else:
        print(f"Warning: {cross_symbol_path} not found")
        config_df, per_symbol_df = None, None

    if df is None and per_symbol_df is None:
        print("No data files found!")
        return

    # Use per_symbol_df for most analysis (richer data)
    analysis_df = per_symbol_df if per_symbol_df is not None else df

    # =================================
    # 0. PERIOD / SAMPLE SIZE SUMMARY
    # =================================
    print_section("0. Data Period Summary")

    if config_df is not None and 'period_days' in config_df.columns:
        period_summary = config_df.select([
            'strategy_type', 'config_str', 'num_bars', 'start_date', 'end_date', 'period_days'
        ]).filter(pl.col('period_days').is_not_null())

        if len(period_summary) > 0:
            print(f"Backtest period: {period_summary['start_date'][0]} to {period_summary['end_date'][0]}")
            print(f"Trading days: {period_summary['num_bars'][0]}")
            print(f"Calendar days: {period_summary['period_days'][0]}")
            print(f"\nNote: All configs tested on the same date range.")
        else:
            print("Period information not available in data.")
    elif 'num_bars' in analysis_df.columns:
        # Fallback - just show bar counts
        bar_stats = analysis_df.group_by('strategy_type').agg([
            pl.col('num_bars').first().alias('bars'),
        ])
        print("Bars per strategy (equity curve length):")
        for row in bar_stats.iter_rows():
            print(f"  {row[0]}: {row[1]} bars")
    else:
        print("Period information not available in data.")
        print("Tip: Re-run YOLO sweep to generate data with date timestamps.")

    # =================================
    # 1. STRATEGY TYPE COMPARISON
    # =================================
    print_section("1. Strategy Type Comparison")

    strategy_comparison = analyze_strategy_types(analysis_df)
    print_dataframe(strategy_comparison, "Performance by Strategy Type")

    top_perf = analyze_top_performers(analysis_df)
    print_dataframe(top_perf, "\nStrategies in Top 10% by Sharpe")

    # =================================
    # 2. PARAMETER PATTERNS
    # =================================
    print_section("2. Parameter Patterns")

    param_analysis = analyze_parameters(analysis_df)
    for strategy_type, param_df in param_analysis.items():
        print_dataframe(param_df, f"\n{strategy_type} Parameter Analysis")

    # =================================
    # 3. SECTOR ANALYSIS
    # =================================
    print_section("3. Sector Analysis")

    sector_perf, combo_perf = analyze_sectors(analysis_df)
    print_dataframe(sector_perf, "Performance by Sector")
    print_dataframe(combo_perf.head(20), "\nBest Strategy+Sector Combinations (top 20)")

    # =================================
    # 4. ROBUSTNESS METRICS
    # =================================
    print_section("4. Robustness Metrics")

    if per_symbol_df is not None:
        robustness = analyze_robustness(per_symbol_df, config_df)
        print_dataframe(robustness.head(20), "Most Robust Configs (consistent across symbols)")

        universal = find_universal_winners(per_symbol_df)
        print_dataframe(universal.head(20), "\nUniversal Winners (rank well on most symbols)")
    else:
        print("Cross-symbol data required for robustness analysis")
        robustness = None

    # =================================
    # EXPORT & CHARTS
    # =================================
    if args.export_csv:
        print_section("Exporting CSVs")
        args.output_dir.mkdir(parents=True, exist_ok=True)

        strategy_comparison.write_csv(args.output_dir / 'strategy_comparison.csv')
        sector_perf.write_csv(args.output_dir / 'sector_performance.csv')
        combo_perf.write_csv(args.output_dir / 'strategy_sector_combos.csv')

        if robustness is not None:
            robustness.write_csv(args.output_dir / 'robustness_scores.csv')
            universal.write_csv(args.output_dir / 'universal_winners.csv')

        for strategy_type, param_df in param_analysis.items():
            param_df.write_csv(args.output_dir / f'params_{strategy_type}.csv')

        print(f"CSVs exported to {args.output_dir}")

    if args.charts:
        print_section("Generating Charts")
        if robustness is not None:
            create_charts(strategy_comparison, sector_perf, robustness, combo_perf, args.output_dir)

    # =================================
    # SUMMARY INSIGHTS
    # =================================
    print_section("Key Insights Summary")

    # Best overall strategy
    best_strat = strategy_comparison.row(0)
    print(f"BEST STRATEGY TYPE: {best_strat[0]}")
    print(f"  Median Sharpe: {best_strat[4]:.3f}, Median CAGR: {best_strat[2]*100:.1f}%")

    # Best sector
    best_sector = sector_perf.row(0)
    print(f"\nBEST PERFORMING SECTOR: {best_sector[0]}")
    print(f"  Median Sharpe: {best_sector[3]:.3f}")

    # Most robust config
    if robustness is not None and len(robustness) > 0:
        best_robust = robustness.row(0)
        print(f"\nMOST ROBUST CONFIG: {best_robust[0]} - {best_robust[1]}")
        print(f"  Avg Sharpe: {best_robust[3]:.3f}, Std: {best_robust[4]:.3f}")
        print(f"  Symbols with positive Sharpe: {int(best_robust[10])}/{best_robust[2]}")


if __name__ == '__main__':
    main()
