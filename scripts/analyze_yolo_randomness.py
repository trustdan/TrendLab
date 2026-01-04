#!/usr/bin/env python3
"""
TrendLab YOLO Randomness Analysis Script

Analyzes YOLO mode exploration patterns to identify:
1. Discovery timeline - when were top configs found?
2. Parameter distribution - are we exploring the full range?
3. Config uniqueness - how many duplicates?
4. Parameter space coverage - are there dead zones?
5. Convergence speed - how fast do we stop exploring?

Usage:
    python scripts/analyze_yolo_randomness.py [--charts] [--export-csv]
    python scripts/analyze_yolo_randomness.py --history-dir target/release/artifacts/yolo_history
"""

import warnings
warnings.filterwarnings('ignore')

import json
import argparse
import sys
from pathlib import Path
from typing import Any
from collections import defaultdict
import math

# Force UTF-8 output on Windows
if sys.platform == 'win32':
    sys.stdout.reconfigure(encoding='utf-8')
    sys.stderr.reconfigure(encoding='utf-8')

import polars as pl
pl.Config.set_fmt_str_lengths(50)

# Try to import matplotlib
try:
    import matplotlib.pyplot as plt
    import matplotlib.patches as mpatches
    MATPLOTLIB_AVAILABLE = True
except ImportError:
    MATPLOTLIB_AVAILABLE = False


# =============================================================================
# Data Loading
# =============================================================================

def load_yolo_history(history_dir: Path) -> pl.DataFrame:
    """Load all YOLO history JSONL files into a single DataFrame."""
    rows = []

    for jsonl_file in sorted(history_dir.glob("*.jsonl")):
        session_id = jsonl_file.stem
        with open(jsonl_file, 'r') as f:
            for line_num, line in enumerate(f, 1):
                try:
                    entry = json.loads(line.strip())
                    config_id = entry.get('config_id', {})
                    strategy_type = entry.get('strategy_type', 'Unknown')

                    # Extract parameters from config_id
                    params = config_id.get(strategy_type, {})
                    if isinstance(params, str):
                        params = {}  # Handle fixed strategies like TurtleS1

                    row = {
                        'session_id': session_id,
                        'tested_at': entry.get('tested_at'),
                        'iteration': entry.get('iteration', 0),
                        'strategy_type': strategy_type,
                        'config_hash': entry.get('config_hash', 0),
                        'symbol_count': entry.get('symbol_count', 0),
                        'avg_sharpe': entry.get('avg_sharpe'),
                        'min_sharpe': entry.get('min_sharpe'),
                        'max_sharpe': entry.get('max_sharpe'),
                        'hit_rate': entry.get('hit_rate', 0),
                        'avg_cagr': entry.get('avg_cagr'),
                        'avg_max_drawdown': entry.get('avg_max_drawdown'),
                    }

                    # Add flattened parameters
                    for k, v in params.items():
                        if isinstance(v, (int, float)):
                            row[f'param_{k}'] = v

                    rows.append(row)
                except json.JSONDecodeError:
                    continue

    if not rows:
        print(f"No YOLO history found in {history_dir}")
        return pl.DataFrame()

    return pl.DataFrame(rows)


# =============================================================================
# Analysis Functions
# =============================================================================

def analyze_discovery_timeline(df: pl.DataFrame) -> dict:
    """Analyze when top configs were discovered."""
    if len(df) == 0 or 'avg_sharpe' not in df.columns:
        return {}

    # Filter valid sharpe values
    valid_df = df.filter(pl.col('avg_sharpe').is_not_null())
    if len(valid_df) == 0:
        return {}

    # Rank all configs by avg_sharpe
    ranked = valid_df.with_columns([
        pl.col('avg_sharpe').rank(descending=True).alias('sharpe_rank')
    ]).sort('sharpe_rank')

    # Get top 10 configs
    top_10 = ranked.filter(pl.col('sharpe_rank') <= 10)

    # When was each discovered?
    discovery_iterations = top_10['iteration'].to_list()

    # Calculate stagnation index (iterations since last top-10 improvement)
    max_iteration = df['iteration'].max()
    last_top10_discovery = max(discovery_iterations) if discovery_iterations else 0
    stagnation_index = max_iteration - last_top10_discovery if max_iteration else 0

    # First discovery of current best
    best_config = ranked.row(0)

    return {
        'top_10_discovery_iterations': sorted(discovery_iterations),
        'first_top10_discovery': min(discovery_iterations) if discovery_iterations else None,
        'last_top10_discovery': last_top10_discovery,
        'max_iteration': max_iteration,
        'stagnation_index': stagnation_index,
        'best_config_iteration': best_config[ranked.columns.index('iteration')],
        'best_config_sharpe': best_config[ranked.columns.index('avg_sharpe')],
        'best_config_strategy': best_config[ranked.columns.index('strategy_type')],
    }


def analyze_parameter_distributions(df: pl.DataFrame) -> dict[str, dict]:
    """Analyze parameter distributions per strategy, comparing early vs late iterations."""
    results = {}

    # Define iteration thresholds
    early_max = 20
    late_min = df['iteration'].max() - 50 if df['iteration'].max() > 50 else df['iteration'].max() // 2

    for strategy_type in df['strategy_type'].unique().to_list():
        strat_df = df.filter(pl.col('strategy_type') == strategy_type)
        param_cols = [c for c in strat_df.columns if c.startswith('param_')]

        if not param_cols:
            continue

        strat_results = {}
        for param in param_cols:
            param_name = param.replace('param_', '')
            param_values = strat_df[param].drop_nulls()

            if len(param_values) == 0:
                continue

            early_values = strat_df.filter(pl.col('iteration') <= early_max)[param].drop_nulls()
            late_values = strat_df.filter(pl.col('iteration') >= late_min)[param].drop_nulls()

            # Calculate entropy (measure of spread)
            def calc_entropy(values: pl.Series, bins: int = 20) -> float:
                if len(values) < 2:
                    return 0.0
                hist, _ = pl.Series(values).to_numpy().astype(float), None
                import numpy as np
                counts, _ = np.histogram(values.to_numpy(), bins=bins)
                probs = counts / counts.sum()
                probs = probs[probs > 0]
                return -np.sum(probs * np.log2(probs)) if len(probs) > 0 else 0.0

            strat_results[param_name] = {
                'all_min': param_values.min(),
                'all_max': param_values.max(),
                'all_mean': param_values.mean(),
                'all_std': param_values.std(),
                'all_count': len(param_values),
                'early_mean': early_values.mean() if len(early_values) > 0 else None,
                'early_std': early_values.std() if len(early_values) > 0 else None,
                'late_mean': late_values.mean() if len(late_values) > 0 else None,
                'late_std': late_values.std() if len(late_values) > 0 else None,
                'entropy': calc_entropy(param_values),
            }

        if strat_results:
            results[strategy_type] = strat_results

    return results


def analyze_config_uniqueness(df: pl.DataFrame) -> dict:
    """Analyze how many unique configs vs duplicates."""
    if len(df) == 0:
        return {}

    total_tests = len(df)
    unique_configs = df['config_hash'].n_unique()
    duplicate_ratio = 1.0 - (unique_configs / total_tests)

    # Cumulative unique configs over iterations
    sorted_df = df.sort('iteration')
    seen_hashes = set()
    cumulative_unique = []
    iterations = []

    for row in sorted_df.iter_rows(named=True):
        seen_hashes.add(row['config_hash'])
        if row['iteration'] not in iterations or len(cumulative_unique) == 0:
            iterations.append(row['iteration'])
            cumulative_unique.append(len(seen_hashes))

    # Calculate uniqueness rate (new configs per iteration)
    if len(cumulative_unique) > 1:
        uniqueness_rate = (cumulative_unique[-1] - cumulative_unique[0]) / (iterations[-1] - iterations[0] + 1)
    else:
        uniqueness_rate = 0

    return {
        'total_tests': total_tests,
        'unique_configs': unique_configs,
        'duplicate_ratio': duplicate_ratio,
        'uniqueness_rate': uniqueness_rate,
        'cumulative_unique': list(zip(iterations, cumulative_unique)),
    }


def analyze_parameter_space_coverage(df: pl.DataFrame) -> dict[str, dict]:
    """Analyze coverage of parameter space for 2-param strategies."""
    results = {}

    # Focus on strategies with exactly 2 main params
    two_param_strategies = {
        'Supertrend': ['param_atr_period', 'param_multiplier'],
        'LarryWilliams': ['param_range_multiplier', 'param_atr_stop_mult'],
        'Donchian': ['param_entry_lookback', 'param_exit_lookback'],
    }

    for strategy_type, param_names in two_param_strategies.items():
        strat_df = df.filter(pl.col('strategy_type') == strategy_type)

        if len(strat_df) == 0:
            continue

        # Check if params exist
        if not all(p in strat_df.columns for p in param_names):
            continue

        # Get param values
        p1_values = strat_df[param_names[0]].drop_nulls().to_list()
        p2_values = strat_df[param_names[1]].drop_nulls().to_list()

        if not p1_values or not p2_values:
            continue

        # Calculate coverage using a grid
        grid_size = 10
        p1_min, p1_max = min(p1_values), max(p1_values)
        p2_min, p2_max = min(p2_values), max(p2_values)

        if p1_max == p1_min or p2_max == p2_min:
            continue

        # Count cells that have been visited
        visited_cells = set()
        for p1, p2 in zip(p1_values, p2_values):
            cell1 = int((p1 - p1_min) / (p1_max - p1_min) * (grid_size - 1))
            cell2 = int((p2 - p2_min) / (p2_max - p2_min) * (grid_size - 1))
            visited_cells.add((cell1, cell2))

        coverage_ratio = len(visited_cells) / (grid_size * grid_size)

        results[strategy_type] = {
            'param1_name': param_names[0].replace('param_', ''),
            'param2_name': param_names[1].replace('param_', ''),
            'param1_range': (p1_min, p1_max),
            'param2_range': (p2_min, p2_max),
            'cells_visited': len(visited_cells),
            'total_cells': grid_size * grid_size,
            'coverage_ratio': coverage_ratio,
            'points': list(zip(p1_values, p2_values)),
        }

    return results


def analyze_convergence(df: pl.DataFrame) -> dict:
    """Measure how fast exploration converges (entropy over time)."""
    if len(df) == 0:
        return {}

    results = {}
    iterations = sorted(df['iteration'].unique().to_list())

    if len(iterations) < 5:
        return {'insufficient_data': True}

    # Calculate entropy of avg_sharpe distribution over time windows
    window_size = max(1, len(iterations) // 10)
    entropy_over_time = []

    import numpy as np

    for i in range(0, len(iterations), window_size):
        window_iters = iterations[i:i + window_size]
        if not window_iters:
            continue

        window_df = df.filter(pl.col('iteration').is_in(window_iters))
        sharpes = window_df['avg_sharpe'].drop_nulls().to_numpy()

        if len(sharpes) < 5:
            continue

        # Calculate entropy of sharpe distribution
        counts, _ = np.histogram(sharpes, bins=10)
        probs = counts / counts.sum()
        probs = probs[probs > 0]
        entropy = -np.sum(probs * np.log2(probs)) if len(probs) > 0 else 0.0

        entropy_over_time.append({
            'iteration_start': min(window_iters),
            'iteration_end': max(window_iters),
            'entropy': entropy,
            'sample_size': len(sharpes),
        })

    # Calculate convergence rate (entropy decrease)
    if len(entropy_over_time) >= 2:
        initial_entropy = entropy_over_time[0]['entropy']
        final_entropy = entropy_over_time[-1]['entropy']
        convergence_rate = (initial_entropy - final_entropy) / initial_entropy if initial_entropy > 0 else 0
    else:
        convergence_rate = 0

    return {
        'entropy_over_time': entropy_over_time,
        'initial_entropy': entropy_over_time[0]['entropy'] if entropy_over_time else 0,
        'final_entropy': entropy_over_time[-1]['entropy'] if entropy_over_time else 0,
        'convergence_rate': convergence_rate,
    }


# =============================================================================
# Visualization Functions
# =============================================================================

def create_charts(df: pl.DataFrame, discovery: dict, uniqueness: dict,
                  param_dist: dict, coverage: dict, convergence: dict,
                  output_dir: Path):
    """Generate visualization charts."""
    if not MATPLOTLIB_AVAILABLE:
        print("Matplotlib not available - skipping charts")
        return

    output_dir.mkdir(parents=True, exist_ok=True)

    # 1. Discovery Timeline
    if discovery.get('top_10_discovery_iterations'):
        fig, ax = plt.subplots(figsize=(10, 6))
        iters = discovery['top_10_discovery_iterations']
        ax.bar(range(1, len(iters) + 1), iters, color='steelblue')
        ax.set_xlabel('Top Config Rank')
        ax.set_ylabel('Iteration Discovered')
        ax.set_title(f'When Were Top 10 Configs Discovered?\n(Stagnation Index: {discovery["stagnation_index"]} iterations)')
        ax.axhline(y=20, color='red', linestyle='--', alpha=0.5, label='Early threshold (iter 20)')
        ax.legend()
        plt.tight_layout()
        plt.savefig(output_dir / 'randomness_discovery_timeline.png', dpi=150)
        plt.close()

    # 2. Cumulative Unique Configs
    if uniqueness.get('cumulative_unique'):
        fig, ax = plt.subplots(figsize=(10, 6))
        data = uniqueness['cumulative_unique']
        iters = [d[0] for d in data]
        counts = [d[1] for d in data]
        ax.plot(iters, counts, 'b-', linewidth=2)

        # Add ideal line (no duplicates)
        ideal_counts = list(range(1, len(iters) + 1))
        ax.plot(iters[:len(ideal_counts)], ideal_counts, 'g--', alpha=0.5, label='Ideal (no duplicates)')

        ax.set_xlabel('Iteration')
        ax.set_ylabel('Cumulative Unique Configs')
        ax.set_title(f'Config Uniqueness Over Time\n(Duplicate Ratio: {uniqueness["duplicate_ratio"]:.1%})')
        ax.legend()
        plt.tight_layout()
        plt.savefig(output_dir / 'randomness_unique_configs.png', dpi=150)
        plt.close()

    # 3. Parameter Space Coverage Heatmaps
    for strategy, cov_data in coverage.items():
        fig, ax = plt.subplots(figsize=(10, 8))

        points = cov_data['points']
        p1_vals = [p[0] for p in points]
        p2_vals = [p[1] for p in points]

        # Create 2D histogram
        import numpy as np
        h, xedges, yedges = np.histogram2d(p1_vals, p2_vals, bins=20)

        im = ax.imshow(h.T, origin='lower', aspect='auto',
                       extent=[xedges[0], xedges[-1], yedges[0], yedges[-1]],
                       cmap='YlOrRd')
        plt.colorbar(im, ax=ax, label='Test Count')

        ax.set_xlabel(cov_data['param1_name'])
        ax.set_ylabel(cov_data['param2_name'])
        ax.set_title(f'{strategy} Parameter Space Coverage\n(Coverage: {cov_data["coverage_ratio"]:.1%})')

        plt.tight_layout()
        plt.savefig(output_dir / f'randomness_coverage_{strategy.lower()}.png', dpi=150)
        plt.close()

    # 4. Entropy Over Time (Convergence)
    if convergence.get('entropy_over_time'):
        fig, ax = plt.subplots(figsize=(10, 6))

        data = convergence['entropy_over_time']
        iters = [(d['iteration_start'] + d['iteration_end']) / 2 for d in data]
        entropies = [d['entropy'] for d in data]

        ax.plot(iters, entropies, 'b-o', linewidth=2, markersize=6)
        ax.set_xlabel('Iteration')
        ax.set_ylabel('Sharpe Distribution Entropy')
        ax.set_title(f'Exploration Convergence Over Time\n(Convergence Rate: {convergence["convergence_rate"]:.1%})')
        ax.axhline(y=convergence['initial_entropy'], color='green', linestyle='--', alpha=0.5, label='Initial entropy')
        ax.legend()

        plt.tight_layout()
        plt.savefig(output_dir / 'randomness_convergence.png', dpi=150)
        plt.close()

    # 5. Parameter Distribution Comparison (Early vs Late)
    for strategy, params in param_dist.items():
        if len(params) > 4:
            continue  # Skip strategies with too many params

        fig, axes = plt.subplots(1, len(params), figsize=(4 * len(params), 5))
        if len(params) == 1:
            axes = [axes]

        for ax, (param_name, stats) in zip(axes, params.items()):
            # Plot early vs late as box-style comparison
            early_mean = stats.get('early_mean')
            early_std = stats.get('early_std', 0) or 0
            late_mean = stats.get('late_mean')
            late_std = stats.get('late_std', 0) or 0

            if early_mean is not None and late_mean is not None:
                x = [1, 2]
                means = [early_mean, late_mean]
                stds = [early_std, late_std]

                ax.bar(x, means, yerr=stds, capsize=5, color=['green', 'orange'], alpha=0.7)
                ax.set_xticks(x)
                ax.set_xticklabels(['Early\n(iter 1-20)', 'Late\n(last 50)'])
                ax.set_ylabel(param_name)
                ax.set_title(f'{param_name}\n(range: {stats["all_min"]:.2f}-{stats["all_max"]:.2f})')

        fig.suptitle(f'{strategy} Parameter Drift', fontsize=12)
        plt.tight_layout()
        plt.savefig(output_dir / f'randomness_params_{strategy.lower()}.png', dpi=150)
        plt.close()

    print(f"Charts saved to {output_dir}")


# =============================================================================
# Output Functions
# =============================================================================

def print_section(title: str):
    """Print a section header."""
    print(f"\n{'='*60}")
    print(title)
    print('='*60 + "\n")


def print_analysis_summary(discovery: dict, uniqueness: dict, param_dist: dict,
                          coverage: dict, convergence: dict):
    """Print analysis summary to console."""

    print_section("YOLO RANDOMNESS ANALYSIS SUMMARY")

    # Discovery Timeline
    print("1. DISCOVERY TIMELINE")
    print("-" * 40)
    if discovery:
        print(f"   Best config found at iteration: {discovery.get('best_config_iteration', 'N/A')}")
        print(f"   Best config Sharpe: {discovery.get('best_config_sharpe', 0):.4f}")
        print(f"   Best config strategy: {discovery.get('best_config_strategy', 'N/A')}")
        print(f"   Top 10 discovered at iterations: {discovery.get('top_10_discovery_iterations', [])}")
        print(f"   STAGNATION INDEX: {discovery.get('stagnation_index', 0)} iterations")
        if discovery.get('stagnation_index', 0) > 50:
            print("   WARNING: No top-10 improvements in 50+ iterations!")

    # Config Uniqueness
    print("\n2. CONFIG UNIQUENESS")
    print("-" * 40)
    if uniqueness:
        print(f"   Total tests: {uniqueness.get('total_tests', 0):,}")
        print(f"   Unique configs: {uniqueness.get('unique_configs', 0):,}")
        print(f"   DUPLICATE RATIO: {uniqueness.get('duplicate_ratio', 0):.1%}")
        if uniqueness.get('duplicate_ratio', 0) > 0.3:
            print("   WARNING: More than 30% duplicate configs!")

    # Parameter Space Coverage
    print("\n3. PARAMETER SPACE COVERAGE")
    print("-" * 40)
    for strategy, cov_data in coverage.items():
        print(f"   {strategy}:")
        print(f"      {cov_data['param1_name']}: {cov_data['param1_range'][0]:.2f} - {cov_data['param1_range'][1]:.2f}")
        print(f"      {cov_data['param2_name']}: {cov_data['param2_range'][0]:.2f} - {cov_data['param2_range'][1]:.2f}")
        print(f"      COVERAGE: {cov_data['coverage_ratio']:.1%} ({cov_data['cells_visited']}/{cov_data['total_cells']} cells)")
        if cov_data['coverage_ratio'] < 0.5:
            print("      WARNING: Less than 50% parameter space explored!")

    # Convergence
    print("\n4. EXPLORATION CONVERGENCE")
    print("-" * 40)
    if convergence and not convergence.get('insufficient_data'):
        print(f"   Initial entropy: {convergence.get('initial_entropy', 0):.3f}")
        print(f"   Final entropy: {convergence.get('final_entropy', 0):.3f}")
        print(f"   CONVERGENCE RATE: {convergence.get('convergence_rate', 0):.1%}")
        if convergence.get('convergence_rate', 0) > 0.5:
            print("   WARNING: Exploration has converged significantly!")

    # Parameter Drift Summary
    print("\n5. PARAMETER DRIFT (Early vs Late)")
    print("-" * 40)
    for strategy, params in param_dist.items():
        print(f"   {strategy}:")
        for param_name, stats in params.items():
            early_mean = stats.get('early_mean')
            late_mean = stats.get('late_mean')
            if early_mean is not None and late_mean is not None:
                drift = abs(late_mean - early_mean) / (stats['all_max'] - stats['all_min']) * 100 if stats['all_max'] != stats['all_min'] else 0
                direction = "higher" if late_mean > early_mean else "lower"
                print(f"      {param_name}: early={early_mean:.2f}, late={late_mean:.2f} ({drift:.1f}% drift {direction})")

    # Overall Assessment
    print_section("OVERALL RANDOMNESS ASSESSMENT")

    issues = []
    if discovery.get('stagnation_index', 0) > 50:
        issues.append("High stagnation (no new discoveries)")
    if uniqueness.get('duplicate_ratio', 0) > 0.3:
        issues.append("High duplicate ratio (wasted iterations)")
    if any(c.get('coverage_ratio', 1) < 0.5 for c in coverage.values()):
        issues.append("Low parameter space coverage")
    if convergence.get('convergence_rate', 0) > 0.5:
        issues.append("Rapid convergence (not enough exploration)")

    if issues:
        print("ISSUES DETECTED:")
        for issue in issues:
            print(f"   - {issue}")
        print("\nRECOMMENDATIONS:")
        print("   - Increase pure random mode probability")
        print("   - Add non-local parameter jumps")
        print("   - Widen parameter bounds")
        print("   - Consider Latin Hypercube sampling")
    else:
        print("Exploration appears healthy. No major issues detected.")


# =============================================================================
# Main
# =============================================================================

def main():
    parser = argparse.ArgumentParser(description='Analyze YOLO mode randomness patterns')
    parser.add_argument('--charts', action='store_true', help='Generate visualization charts')
    parser.add_argument('--export-csv', action='store_true', help='Export results to CSV')

    # Find history directory
    default_history_dir = Path('target/release/artifacts/yolo_history')
    if not default_history_dir.exists():
        default_history_dir = Path('apps/trendlab-gui/src-tauri/artifacts/yolo_history')

    parser.add_argument('--history-dir', type=Path, default=default_history_dir,
                        help='Directory containing YOLO history JSONL files')
    parser.add_argument('--output-dir', type=Path, default=Path('reports/analysis'),
                        help='Output directory for CSVs and charts')

    args = parser.parse_args()

    # Load data
    print_section("Loading YOLO History")

    if not args.history_dir.exists():
        print(f"Error: History directory not found: {args.history_dir}")
        print("Run YOLO mode first to generate history data.")
        return

    df = load_yolo_history(args.history_dir)

    if len(df) == 0:
        print("No data loaded. Exiting.")
        return

    print(f"Loaded {len(df):,} test entries from {df['session_id'].n_unique()} sessions")
    print(f"Iterations: {df['iteration'].min()} to {df['iteration'].max()}")
    print(f"Strategy types: {df['strategy_type'].n_unique()}")

    # Run analyses
    print_section("Running Analysis")

    discovery = analyze_discovery_timeline(df)
    uniqueness = analyze_config_uniqueness(df)
    param_dist = analyze_parameter_distributions(df)
    coverage = analyze_parameter_space_coverage(df)
    convergence = analyze_convergence(df)

    # Print summary
    print_analysis_summary(discovery, uniqueness, param_dist, coverage, convergence)

    # Export CSV
    if args.export_csv:
        args.output_dir.mkdir(parents=True, exist_ok=True)

        # Summary CSV
        summary_rows = []
        summary_rows.append({
            'metric': 'stagnation_index',
            'value': discovery.get('stagnation_index', 0),
        })
        summary_rows.append({
            'metric': 'duplicate_ratio',
            'value': uniqueness.get('duplicate_ratio', 0),
        })
        summary_rows.append({
            'metric': 'convergence_rate',
            'value': convergence.get('convergence_rate', 0),
        })
        for strategy, cov_data in coverage.items():
            summary_rows.append({
                'metric': f'{strategy}_coverage',
                'value': cov_data['coverage_ratio'],
            })

        pl.DataFrame(summary_rows).write_csv(args.output_dir / 'randomness_summary.csv')
        print(f"\nCSV exported to {args.output_dir / 'randomness_summary.csv'}")

    # Generate charts
    if args.charts:
        print_section("Generating Charts")
        create_charts(df, discovery, uniqueness, param_dist, coverage, convergence, args.output_dir)


if __name__ == '__main__':
    main()
