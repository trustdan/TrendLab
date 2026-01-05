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
5. Combo strategy analysis (if combo data present):
   - Component frequency (which singles appear in winning combos)
   - Synergy analysis (combo performance vs component averages)
   - Voting method comparison
   - Combo vs single head-to-head comparison
   - Robustness: combo vs single

Usage:
    python scripts/analyze_yolo.py [--export-csv] [--charts]

Output Files (with --export-csv):
    strategy_comparison.csv       - Strategy rankings by median Sharpe
    sector_performance.csv        - Sector rankings
    strategy_sector_combos.csv    - Best strategy+sector pairs
    robustness_scores.csv         - Configs ranked by consistency
    universal_winners.csv         - Configs that rank well universally
    params_*.csv                  - Parameter analysis per strategy type

    Combo-specific (if combo data present):
    combo_component_frequency.csv - How often each single strategy appears in combos
    combo_synergy_analysis.csv    - Synergy scores, best/worst strategy pairs
    combo_voting_analysis.csv     - Performance by voting method
    combo_vs_single_comparison.csv - Head-to-head performance comparison
    combo_robustness.csv          - Robustness comparison

Charts (with --charts):
    strategy_comparison.png       - Strategy type bar chart
    best_strategy_per_sector.png  - Best strategy for each sector
    sector_performance.png        - Sector rankings
    robust_configs.png            - Top 15 most robust configurations

    Combo-specific (if combo data present):
    combo_vs_single.png           - Combo vs Single comparison (3 metrics)
    combo_component_frequency.png - Component frequency bar chart
    combo_synergy.png             - Synergy score bar chart
    combo_voting_methods.png      - Voting method performance
    combo_robustness.png          - Robustness comparison
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

# Try to import matplotlib (with protection against segfaults on some systems)
MATPLOTLIB_AVAILABLE = False
try:
    import matplotlib
    matplotlib.use('Agg')  # Use non-interactive backend to avoid display issues
    import matplotlib.pyplot as plt
    MATPLOTLIB_AVAILABLE = True
except (ImportError, RuntimeError):
    pass


# =============================================================================
# Combo Strategy Helpers
# =============================================================================

def is_combo_strategy(strategy_type: str) -> bool:
    """Check if strategy is a combo type."""
    return strategy_type in ["Combo2", "Combo3"]


def parse_combo_config(config: dict) -> dict | None:
    """
    Extract components and voting from combo config.

    Returns dict with:
      - components: List of [type, config] pairs
      - voting: Voting method string
      - component_types: Sorted list of component strategy types
    """
    if "Combo" not in config:
        return None
    combo = config["Combo"]
    return {
        "components": combo.get("components", []),
        "voting": combo.get("voting", "Unknown"),
        "component_types": sorted([c[0] for c in combo.get("components", [])])
    }


def get_combo_component_key(components: list) -> str:
    """Create sortable key like 'Donchian+Supertrend'."""
    types = sorted([c[0] for c in components])
    return "+".join(types)


def flatten_combo_params(combo_info: dict) -> dict:
    """
    Flatten all component parameters with prefixed column names.

    Returns dict like:
      comp1_Supertrend_period: 10
      comp1_Supertrend_multiplier: 3.0
      comp2_Donchian_entry_lookback: 20
      ...

    Handles both:
      - Nested dict configs: {"Supertrend": {"period": 10}}
      - Unit struct configs (no params): "TurtleS1" (string)
    """
    params = {}
    for i, (comp_type, comp_config) in enumerate(combo_info["components"], start=1):
        # comp_config can be:
        # - dict like {"Supertrend": {"period": 10, "multiplier": 3.0}}
        # - string like "TurtleS1" for unit structs with no params
        if isinstance(comp_config, dict):
            inner_params = comp_config.get(comp_type, {})
            if isinstance(inner_params, dict):
                for k, v in inner_params.items():
                    params[f"comp{i}_{comp_type}_{k}"] = v
        # For string configs (unit structs), no params to add
    return params


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

        # Build base row
        row = {
            'rank': entry.get('rank'),
            'strategy_type': strategy_type,
            'symbol': entry.get('symbol'),
            'sector': entry.get('sector'),
            'num_bars': num_bars,
            'start_date': start_date,
            'end_date': end_date,
            'period_days': period_days,
            **entry.get('metrics', {})
        }

        # Add combo-specific columns
        row['is_combo'] = is_combo_strategy(strategy_type)
        if row['is_combo']:
            combo_info = parse_combo_config(config)
            if combo_info:
                row['combo_size'] = len(combo_info['components'])
                row['voting_method'] = combo_info['voting']
                row['component_types'] = get_combo_component_key(combo_info['components'])
                # Flatten all component parameters
                row.update(flatten_combo_params(combo_info))
            else:
                row['combo_size'] = None
                row['voting_method'] = None
                row['component_types'] = None
        else:
            row['combo_size'] = None
            row['voting_method'] = None
            row['component_types'] = None
            # Add regular params for single strategies
            row.update({f'param_{k}': v for k, v in params.items()})

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

        # Build config row with combo detection
        config_row = {
            'rank': entry.get('rank'),
            'strategy_type': strategy_type,
            'config_str': config_str,
            'num_symbols': len(entry.get('symbols', [])),
            'num_bars': num_bars,
            'start_date': start_date,
            'end_date': end_date,
            'period_days': period_days,
        }

        # Add combo-specific columns
        config_row['is_combo'] = is_combo_strategy(strategy_type)
        combo_info = None
        if config_row['is_combo']:
            combo_info = parse_combo_config(config_id)
            if combo_info:
                config_row['combo_size'] = len(combo_info['components'])
                config_row['voting_method'] = combo_info['voting']
                config_row['component_types'] = get_combo_component_key(combo_info['components'])
                # Store component types list for later analysis (as JSON string)
                config_row['component_types_list'] = json.dumps(combo_info['component_types'])
                # Flatten all component parameters
                config_row.update(flatten_combo_params(combo_info))
            else:
                config_row['combo_size'] = None
                config_row['voting_method'] = None
                config_row['component_types'] = None
                config_row['component_types_list'] = None
        else:
            config_row['combo_size'] = None
            config_row['voting_method'] = None
            config_row['component_types'] = None
            config_row['component_types_list'] = None
            # Add regular params for single strategies
            config_row.update({f'param_{k}': v for k, v in params.items()})

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
                **metrics
            }

            # Add combo-specific columns (same as config_row)
            ps_row['is_combo'] = is_combo_strategy(strategy_type)
            if ps_row['is_combo'] and combo_info:
                ps_row['combo_size'] = len(combo_info['components'])
                ps_row['voting_method'] = combo_info['voting']
                ps_row['component_types'] = get_combo_component_key(combo_info['components'])
                ps_row['component_types_list'] = json.dumps(combo_info['component_types'])
                ps_row.update(flatten_combo_params(combo_info))
            else:
                ps_row['combo_size'] = None
                ps_row['voting_method'] = None
                ps_row['component_types'] = None
                ps_row['component_types_list'] = None
                if not ps_row['is_combo']:
                    ps_row.update({f'param_{k}': v for k, v in params.items()})

            per_symbol_rows.append(ps_row)

    # Create DataFrames with explicit schema for mixed-type columns
    config_df = pl.DataFrame(config_rows, infer_schema_length=None)
    per_symbol_df = pl.DataFrame(per_symbol_rows, infer_schema_length=None)
    return config_df, per_symbol_df


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
# Combo-Specific Analysis Functions
# =============================================================================

def analyze_combo_components(df: pl.DataFrame) -> pl.DataFrame:
    """
    Analyze frequency of single strategies appearing in winning combos.

    Returns DataFrame with columns:
      - component_strategy: The single strategy type
      - appearance_count: How many combos include this strategy
      - avg_combo_sharpe: Average Sharpe of combos containing this strategy
      - pct_of_combos: What % of all combos include this strategy
    """
    # Filter to combos only
    combo_df = df.filter(pl.col('is_combo') == True)

    if len(combo_df) == 0:
        return pl.DataFrame({
            'component_strategy': [],
            'appearance_count': [],
            'avg_combo_sharpe': [],
            'pct_of_combos': []
        })

    # Parse component_types_list and explode
    total_combos = len(combo_df)

    # Explode the component types
    rows = []
    for row in combo_df.iter_rows(named=True):
        comp_list_str = row.get('component_types_list')
        if comp_list_str:
            try:
                comp_list = json.loads(comp_list_str)
                for comp in comp_list:
                    rows.append({
                        'component_strategy': comp,
                        'sharpe': row.get('sharpe', 0),
                        'config_str': row.get('config_str', '')
                    })
            except json.JSONDecodeError:
                pass

    if not rows:
        return pl.DataFrame({
            'component_strategy': [],
            'appearance_count': [],
            'avg_combo_sharpe': [],
            'pct_of_combos': []
        })

    exploded = pl.DataFrame(rows)

    return exploded.group_by('component_strategy').agg([
        pl.count().alias('appearance_count'),
        pl.col('sharpe').mean().alias('avg_combo_sharpe'),
        pl.col('sharpe').median().alias('median_combo_sharpe'),
        pl.col('config_str').n_unique().alias('unique_combos'),
    ]).with_columns([
        (pl.col('unique_combos') / total_combos * 100).alias('pct_of_combos')
    ]).sort('appearance_count', descending=True)


def analyze_synergy(per_symbol_df: pl.DataFrame) -> pl.DataFrame:
    """
    Calculate synergy scores: combo performance vs average of component singles.

    Synergy > 0 means combo outperforms the average of its components.

    Returns DataFrame with:
      - component_types: The combo pair/triple (e.g., "Donchian+Supertrend")
      - voting_method: Voting method used
      - combo_sharpe: Average Sharpe of this combo type
      - avg_component_sharpe: Average Sharpe of component singles
      - synergy_score: combo_sharpe - avg_component_sharpe
    """
    if 'is_combo' not in per_symbol_df.columns:
        return pl.DataFrame()

    # Get combo results
    combo_df = per_symbol_df.filter(pl.col('is_combo') == True)
    single_df = per_symbol_df.filter(pl.col('is_combo') == False)

    if len(combo_df) == 0 or len(single_df) == 0:
        return pl.DataFrame()

    # Calculate average Sharpe per single strategy type
    single_avg = single_df.group_by('strategy_type').agg([
        pl.col('sharpe').mean().alias('single_avg_sharpe')
    ])

    # For each combo, calculate average Sharpe of its components
    combo_results = []
    for row in combo_df.iter_rows(named=True):
        comp_list_str = row.get('component_types_list')
        if not comp_list_str:
            continue

        try:
            comp_list = json.loads(comp_list_str)
        except json.JSONDecodeError:
            continue

        # Look up average Sharpe for each component
        component_sharpes = []
        for comp in comp_list:
            avg_row = single_avg.filter(pl.col('strategy_type') == comp)
            if len(avg_row) > 0:
                component_sharpes.append(avg_row['single_avg_sharpe'][0])

        if component_sharpes:
            avg_comp_sharpe = sum(component_sharpes) / len(component_sharpes)
        else:
            avg_comp_sharpe = 0.0

        combo_results.append({
            'component_types': row.get('component_types', ''),
            'voting_method': row.get('voting_method', ''),
            'symbol': row.get('symbol', ''),
            'combo_sharpe': row.get('sharpe', 0),
            'avg_component_sharpe': avg_comp_sharpe,
        })

    if not combo_results:
        return pl.DataFrame()

    result_df = pl.DataFrame(combo_results).with_columns([
        (pl.col('combo_sharpe') - pl.col('avg_component_sharpe')).alias('synergy_score')
    ])

    # Aggregate by combo type and voting method
    return result_df.group_by(['component_types', 'voting_method']).agg([
        pl.count().alias('count'),
        pl.col('combo_sharpe').mean().alias('avg_combo_sharpe'),
        pl.col('combo_sharpe').median().alias('median_combo_sharpe'),
        pl.col('avg_component_sharpe').mean().alias('avg_component_sharpe'),
        pl.col('synergy_score').mean().alias('avg_synergy'),
        pl.col('synergy_score').median().alias('median_synergy'),
        pl.col('synergy_score').std().alias('std_synergy'),
        # Count positive synergy occurrences
        (pl.col('synergy_score') > 0).sum().alias('positive_synergy_count'),
    ]).with_columns([
        (pl.col('positive_synergy_count') / pl.col('count') * 100).alias('pct_positive_synergy')
    ]).sort('avg_synergy', descending=True)


def analyze_voting_methods(df: pl.DataFrame) -> pl.DataFrame:
    """Compare performance by voting method across all combos."""
    combo_df = df.filter(pl.col('is_combo') == True)

    if len(combo_df) == 0:
        return pl.DataFrame()

    return combo_df.group_by('voting_method').agg([
        pl.count().alias('count'),
        pl.col('sharpe').median().alias('median_sharpe'),
        pl.col('sharpe').mean().alias('mean_sharpe'),
        pl.col('sharpe').std().alias('std_sharpe'),
        pl.col('cagr').median().alias('median_cagr'),
        pl.col('max_drawdown').mean().alias('avg_max_dd'),
        pl.col('num_trades').mean().alias('avg_trades'),
    ]).sort('median_sharpe', descending=True)


def compare_combo_vs_single(df: pl.DataFrame) -> pl.DataFrame:
    """
    Head-to-head comparison of combos vs single strategies.

    Returns DataFrame comparing:
      - Sharpe, CAGR, drawdown, trades, win rate
      - Robustness metrics (if available)
    """
    # Group by is_combo flag
    comparison = df.group_by('is_combo').agg([
        pl.count().alias('count'),
        pl.col('sharpe').median().alias('median_sharpe'),
        pl.col('sharpe').mean().alias('mean_sharpe'),
        pl.col('sharpe').std().alias('std_sharpe'),
        pl.col('cagr').median().alias('median_cagr'),
        pl.col('cagr').mean().alias('mean_cagr'),
        pl.col('max_drawdown').mean().alias('avg_max_dd'),
        pl.col('max_drawdown').std().alias('std_max_dd'),
        pl.col('num_trades').mean().alias('avg_trades'),
        pl.col('num_trades').std().alias('std_trades'),
        pl.col('win_rate').mean().alias('avg_win_rate'),
        # Positive performance count
        (pl.col('sharpe') > 0).sum().alias('positive_sharpe_count'),
        (pl.col('cagr') > 0).sum().alias('positive_cagr_count'),
    ]).with_columns([
        pl.when(pl.col('is_combo') == True)
          .then(pl.lit('Combo'))
          .otherwise(pl.lit('Single'))
          .alias('strategy_category'),
        (pl.col('positive_sharpe_count') / pl.col('count') * 100).alias('pct_positive_sharpe'),
    ]).select([
        'strategy_category',
        'count',
        'median_sharpe',
        'mean_sharpe',
        'std_sharpe',
        'median_cagr',
        'avg_max_dd',
        'avg_trades',
        'avg_win_rate',
        'pct_positive_sharpe',
    ]).sort('strategy_category')

    return comparison


def analyze_combo_robustness(per_symbol_df: pl.DataFrame) -> pl.DataFrame:
    """
    Calculate robustness metrics specifically for combo strategies.

    Returns DataFrame comparing robustness of combos vs singles:
      - Cross-symbol consistency
      - Win ratio
      - Sharpe stability
    """
    combo_df = per_symbol_df.filter(pl.col('is_combo') == True)
    single_df = per_symbol_df.filter(pl.col('is_combo') == False)

    def calc_robustness(df: pl.DataFrame, label: str) -> dict:
        if len(df) == 0:
            return {
                'category': label,
                'num_configs': 0,
                'avg_symbol_win_ratio': 0,
                'avg_sharpe': 0,
                'avg_sharpe_std': 0,
                'avg_robustness_score': 0,
            }

        # Calculate per-config stats
        config_stats = df.group_by(['strategy_type', 'config_str']).agg([
            pl.count().alias('num_symbols'),
            pl.col('sharpe').mean().alias('avg_sharpe'),
            pl.col('sharpe').std().alias('std_sharpe'),
            (pl.col('sharpe') > 0).sum().alias('positive_sharpe_count'),
        ]).with_columns([
            (pl.col('positive_sharpe_count') / pl.col('num_symbols')).alias('symbol_win_ratio'),
            (pl.col('avg_sharpe') / (1 + pl.col('std_sharpe').fill_null(0))).alias('robustness_score'),
        ]).filter(pl.col('num_symbols') >= 5)

        return {
            'category': label,
            'num_configs': len(config_stats),
            'avg_symbol_win_ratio': config_stats['symbol_win_ratio'].mean() if len(config_stats) > 0 else 0,
            'avg_sharpe': config_stats['avg_sharpe'].mean() if len(config_stats) > 0 else 0,
            'avg_sharpe_std': config_stats['std_sharpe'].mean() if len(config_stats) > 0 else 0,
            'avg_robustness_score': config_stats['robustness_score'].mean() if len(config_stats) > 0 else 0,
        }

    combo_robust = calc_robustness(combo_df, 'Combo')
    single_robust = calc_robustness(single_df, 'Single')

    return pl.DataFrame([combo_robust, single_robust])


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


def create_combo_charts(
    combo_vs_single: pl.DataFrame,
    component_freq: pl.DataFrame,
    synergy: pl.DataFrame,
    voting_analysis: pl.DataFrame,
    combo_robustness: pl.DataFrame,
    output_dir: Path
):
    """Create combo-specific analysis charts."""
    if not MATPLOTLIB_AVAILABLE:
        print("Matplotlib not available - skipping combo charts")
        return

    output_dir.mkdir(parents=True, exist_ok=True)

    # 1. Combo vs Single comparison bar chart
    if len(combo_vs_single) > 0:
        fig, axes = plt.subplots(1, 3, figsize=(15, 5))

        categories = combo_vs_single['strategy_category'].to_list()

        # Sharpe comparison
        sharpes = combo_vs_single['median_sharpe'].to_list()
        bars = axes[0].bar(categories, sharpes, color=['#E74C3C', '#3498DB'])
        axes[0].set_ylabel('Median Sharpe')
        axes[0].set_title('Sharpe Ratio')
        axes[0].axhline(y=0, color='gray', linestyle='--', alpha=0.5)
        for bar, val in zip(bars, sharpes):
            axes[0].text(bar.get_x() + bar.get_width()/2, bar.get_height() + 0.01,
                        f'{val:.3f}', ha='center', fontsize=10)

        # CAGR comparison
        cagrs = [v * 100 for v in combo_vs_single['median_cagr'].to_list()]
        bars = axes[1].bar(categories, cagrs, color=['#E74C3C', '#3498DB'])
        axes[1].set_ylabel('Median CAGR (%)')
        axes[1].set_title('Annual Return')
        axes[1].axhline(y=0, color='gray', linestyle='--', alpha=0.5)
        for bar, val in zip(bars, cagrs):
            axes[1].text(bar.get_x() + bar.get_width()/2, bar.get_height() + 0.5,
                        f'{val:.1f}%', ha='center', fontsize=10)

        # Win rate comparison
        pct_positive = combo_vs_single['pct_positive_sharpe'].to_list()
        bars = axes[2].bar(categories, pct_positive, color=['#E74C3C', '#3498DB'])
        axes[2].set_ylabel('% Positive Sharpe')
        axes[2].set_title('Win Rate')
        for bar, val in zip(bars, pct_positive):
            axes[2].text(bar.get_x() + bar.get_width()/2, bar.get_height() + 1,
                        f'{val:.1f}%', ha='center', fontsize=10)

        plt.suptitle('Combo vs Single Strategy Comparison', fontsize=14)
        plt.tight_layout()
        plt.savefig(output_dir / 'combo_vs_single.png', dpi=150)
        plt.close()

    # 2. Component frequency chart
    if len(component_freq) > 0:
        fig, ax = plt.subplots(figsize=(12, 6))
        components = component_freq['component_strategy'].to_list()[:15]
        counts = component_freq['appearance_count'].to_list()[:15]

        bars = ax.barh(components, counts, color='#9B59B6')
        ax.set_xlabel('Appearance Count in Combos')
        ax.set_title('Component Strategy Frequency in Winning Combos')

        for bar, count in zip(bars, counts):
            ax.text(bar.get_width() + 0.5, bar.get_y() + bar.get_height()/2,
                   f'{count}', va='center')

        plt.tight_layout()
        plt.savefig(output_dir / 'combo_component_frequency.png', dpi=150)
        plt.close()

    # 3. Synergy heatmap (strategy pairs)
    if len(synergy) > 0:
        # Get unique component pairs and their synergy scores
        fig, ax = plt.subplots(figsize=(14, 8))

        combo_types = synergy['component_types'].to_list()[:15]
        synergy_scores = synergy['avg_synergy'].to_list()[:15]

        colors = ['#27AE60' if s > 0 else '#E74C3C' for s in synergy_scores]
        bars = ax.barh(combo_types, synergy_scores, color=colors)
        ax.set_xlabel('Average Synergy Score (Combo - Avg Component Sharpe)')
        ax.set_title('Strategy Pair Synergy Analysis')
        ax.axvline(x=0, color='black', linestyle='-', linewidth=1)

        for bar, score in zip(bars, synergy_scores):
            offset = 0.01 if score >= 0 else -0.05
            ax.text(bar.get_width() + offset, bar.get_y() + bar.get_height()/2,
                   f'{score:.3f}', va='center', fontsize=8)

        plt.tight_layout()
        plt.savefig(output_dir / 'combo_synergy.png', dpi=150)
        plt.close()

    # 4. Voting method comparison
    if len(voting_analysis) > 0:
        fig, ax = plt.subplots(figsize=(10, 6))

        methods = voting_analysis['voting_method'].to_list()
        sharpes = voting_analysis['median_sharpe'].to_list()

        bars = ax.bar(methods, sharpes, color='#1ABC9C')
        ax.set_ylabel('Median Sharpe Ratio')
        ax.set_title('Performance by Voting Method')
        ax.axhline(y=0, color='gray', linestyle='--', alpha=0.5)

        for bar, sharpe in zip(bars, sharpes):
            ax.text(bar.get_x() + bar.get_width()/2, bar.get_height() + 0.01,
                   f'{sharpe:.3f}', ha='center')

        plt.xticks(rotation=45, ha='right')
        plt.tight_layout()
        plt.savefig(output_dir / 'combo_voting_methods.png', dpi=150)
        plt.close()

    # 5. Robustness comparison
    if len(combo_robustness) > 0:
        fig, axes = plt.subplots(1, 2, figsize=(12, 5))

        categories = combo_robustness['category'].to_list()

        # Win ratio
        win_ratios = [v * 100 for v in combo_robustness['avg_symbol_win_ratio'].to_list()]
        bars = axes[0].bar(categories, win_ratios, color=['#E74C3C', '#3498DB'])
        axes[0].set_ylabel('Avg Symbol Win Ratio (%)')
        axes[0].set_title('Cross-Symbol Consistency')
        for bar, val in zip(bars, win_ratios):
            axes[0].text(bar.get_x() + bar.get_width()/2, bar.get_height() + 1,
                        f'{val:.1f}%', ha='center')

        # Robustness score
        robustness_scores = combo_robustness['avg_robustness_score'].to_list()
        bars = axes[1].bar(categories, robustness_scores, color=['#E74C3C', '#3498DB'])
        axes[1].set_ylabel('Avg Robustness Score')
        axes[1].set_title('Robustness (Sharpe / (1 + StdDev))')
        for bar, val in zip(bars, robustness_scores):
            axes[1].text(bar.get_x() + bar.get_width()/2, bar.get_height() + 0.01,
                        f'{val:.3f}', ha='center')

        plt.suptitle('Robustness: Combo vs Single Strategies', fontsize=14)
        plt.tight_layout()
        plt.savefig(output_dir / 'combo_robustness.png', dpi=150)
        plt.close()

    print(f"Combo charts saved to {output_dir}")


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
    # 5. COMBO STRATEGY ANALYSIS
    # =================================
    print_section("5. Combo Strategy Analysis")

    # Check if we have combo data
    has_combos = 'is_combo' in analysis_df.columns and analysis_df.filter(pl.col('is_combo') == True).height > 0

    if has_combos:
        print(f"Found {analysis_df.filter(pl.col('is_combo') == True).height} combo entries")

        # Component frequency
        component_freq = analyze_combo_components(analysis_df)
        if len(component_freq) > 0:
            print_dataframe(component_freq, "\nComponent Strategy Frequency in Combos")

        # Synergy analysis
        if per_symbol_df is not None:
            synergy = analyze_synergy(per_symbol_df)
            if len(synergy) > 0:
                print_dataframe(synergy.head(15), "\nStrategy Pair Synergy (top 15)")
        else:
            synergy = pl.DataFrame()

        # Voting method analysis
        voting_analysis = analyze_voting_methods(analysis_df)
        if len(voting_analysis) > 0:
            print_dataframe(voting_analysis, "\nPerformance by Voting Method")

        # Combo vs Single comparison
        combo_vs_single = compare_combo_vs_single(analysis_df)
        if len(combo_vs_single) > 0:
            print_dataframe(combo_vs_single, "\nCombo vs Single Strategy Comparison")

        # Combo robustness
        if per_symbol_df is not None:
            combo_robustness = analyze_combo_robustness(per_symbol_df)
            if len(combo_robustness) > 0:
                print_dataframe(combo_robustness, "\nRobustness: Combo vs Single")
        else:
            combo_robustness = pl.DataFrame()
    else:
        print("No combo strategies found in data.")
        component_freq = pl.DataFrame()
        synergy = pl.DataFrame()
        voting_analysis = pl.DataFrame()
        combo_vs_single = pl.DataFrame()
        combo_robustness = pl.DataFrame()

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

        # Export combo-specific CSVs
        if has_combos:
            if len(component_freq) > 0:
                component_freq.write_csv(args.output_dir / 'combo_component_frequency.csv')
            if len(synergy) > 0:
                synergy.write_csv(args.output_dir / 'combo_synergy_analysis.csv')
            if len(voting_analysis) > 0:
                voting_analysis.write_csv(args.output_dir / 'combo_voting_analysis.csv')
            if len(combo_vs_single) > 0:
                combo_vs_single.write_csv(args.output_dir / 'combo_vs_single_comparison.csv')
            if len(combo_robustness) > 0:
                combo_robustness.write_csv(args.output_dir / 'combo_robustness.csv')

        print(f"CSVs exported to {args.output_dir}")

    if args.charts:
        print_section("Generating Charts")
        if robustness is not None:
            create_charts(strategy_comparison, sector_perf, robustness, combo_perf, args.output_dir)

        # Generate combo-specific charts
        if has_combos:
            create_combo_charts(
                combo_vs_single,
                component_freq,
                synergy,
                voting_analysis,
                combo_robustness,
                args.output_dir
            )

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

    # Combo insights
    if has_combos and len(combo_vs_single) >= 2:
        print("\n" + "="*40)
        print("COMBO STRATEGY INSIGHTS")
        print("="*40)

        # Find combo and single rows
        combo_row = combo_vs_single.filter(pl.col('strategy_category') == 'Combo')
        single_row = combo_vs_single.filter(pl.col('strategy_category') == 'Single')

        if len(combo_row) > 0 and len(single_row) > 0:
            combo_sharpe = combo_row['median_sharpe'][0]
            single_sharpe = single_row['median_sharpe'][0]
            combo_count = combo_row['count'][0]
            single_count = single_row['count'][0]

            print(f"\nCombo vs Single Comparison:")
            print(f"  Combos:  {combo_count} entries, median Sharpe {combo_sharpe:.3f}")
            print(f"  Singles: {single_count} entries, median Sharpe {single_sharpe:.3f}")

            if combo_sharpe > single_sharpe:
                improvement = ((combo_sharpe / single_sharpe) - 1) * 100 if single_sharpe > 0 else 0
                print(f"  -> Combos outperform by {improvement:.1f}%")
            else:
                print(f"  -> Singles outperform combos")

        # Best synergy pair
        if len(synergy) > 0:
            best_synergy = synergy.row(0, named=True)
            print(f"\nBEST SYNERGY PAIR: {best_synergy['component_types']}")
            print(f"  Voting: {best_synergy['voting_method']}")
            print(f"  Synergy Score: {best_synergy['avg_synergy']:.3f}")
            print(f"  Positive Synergy Rate: {best_synergy['pct_positive_synergy']:.1f}%")

        # Best voting method
        if len(voting_analysis) > 0:
            best_voting = voting_analysis.row(0, named=True)
            print(f"\nBEST VOTING METHOD: {best_voting['voting_method']}")
            print(f"  Median Sharpe: {best_voting['median_sharpe']:.3f}")


if __name__ == '__main__':
    main()
