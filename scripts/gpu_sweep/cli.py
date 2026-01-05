"""
CLI entry point for GPU Mega-Sweep.

Usage:
    python -m gpu_sweep --config configs/mega_sweep.yaml
    python -m gpu_sweep --data-dir data/parquet --symbols SPY,QQQ --strategy ma_crossover
"""

import sys
from datetime import date
from pathlib import Path

import click
from rich.console import Console
from rich.progress import Progress, SpinnerColumn, TextColumn
from rich.table import Table

from .config import SweepConfig

console = Console()


@click.group()
@click.version_option(version="0.1.0")
def cli():
    """GPU Mega-Sweep: GPU-accelerated parameter sweeps for TrendLab."""
    pass


@cli.command()
@click.option(
    "--config",
    "-c",
    type=click.Path(exists=True, path_type=Path),
    help="YAML configuration file",
)
@click.option(
    "--data-dir",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Data directory (overrides config)",
)
@click.option(
    "--symbols",
    "-s",
    type=str,
    default=None,
    help="Comma-separated symbols (overrides config)",
)
@click.option(
    "--strategy",
    type=str,
    default=None,
    help="Single strategy to run (overrides config)",
)
@click.option(
    "--output",
    "-o",
    type=click.Path(path_type=Path),
    default=None,
    help="Output file path",
)
@click.option(
    "--dry-run",
    is_flag=True,
    help="Show what would be run without executing",
)
def run(
    config: Path | None,
    data_dir: Path | None,
    symbols: str | None,
    strategy: str | None,
    output: Path | None,
    dry_run: bool,
):
    """Run a GPU-accelerated parameter sweep."""
    # Check CUDA availability
    if not dry_run:
        if not _check_cuda():
            console.print("[red]Error: CUDA not available. GPU sweep requires CUDA.[/red]")
            sys.exit(1)

    # Load or build configuration
    if config:
        sweep_config = SweepConfig.from_yaml(config)
        console.print(f"[green]Loaded config from {config}[/green]")
    else:
        sweep_config = SweepConfig()

    # Apply CLI overrides
    if data_dir:
        sweep_config.data.base_dir = data_dir
    if symbols:
        sweep_config.symbols = [s.strip() for s in symbols.split(",")]
    if output:
        sweep_config.output_dir = output.parent
    if strategy:
        # Filter to single strategy
        if strategy in sweep_config.strategies:
            sweep_config.strategies = {strategy: sweep_config.strategies[strategy]}
        else:
            console.print(f"[red]Unknown strategy: {strategy}[/red]")
            sys.exit(1)

    # Validate configuration
    if not sweep_config.symbols:
        console.print("[red]Error: No symbols specified[/red]")
        sys.exit(1)
    if not sweep_config.strategies:
        console.print("[red]Error: No strategies enabled[/red]")
        sys.exit(1)

    # Show summary
    console.print()
    console.print(sweep_config.summary())
    console.print()

    if dry_run:
        console.print("[yellow]Dry run - no execution[/yellow]")
        return

    # Run the sweep
    _run_sweep(sweep_config, output)


@cli.command()
@click.option(
    "--config",
    "-c",
    type=click.Path(exists=True, path_type=Path),
    required=True,
    help="YAML configuration file",
)
def info(config: Path):
    """Show information about a sweep configuration."""
    sweep_config = SweepConfig.from_yaml(config)
    console.print(sweep_config.summary())

    # Show detailed strategy breakdown
    console.print()
    table = Table(title="Strategy Details")
    table.add_column("Strategy", style="cyan")
    table.add_column("Configs", justify="right")
    table.add_column("Parameters")

    for name, grid in sweep_config.strategies.items():
        configs = grid.generate_configs()
        if configs:
            # Show first config as example
            example = configs[0]
            params_str = ", ".join(f"{k}={v}" for k, v in example.items())
            table.add_row(name, str(len(configs)), params_str[:50])

    console.print(table)


@cli.command()
@click.option(
    "--rust-results",
    type=click.Path(exists=True, path_type=Path),
    required=True,
    help="Rust sweep results (Parquet) to compare against",
)
@click.option(
    "--gpu-results",
    type=click.Path(exists=True, path_type=Path),
    required=True,
    help="GPU sweep results (Parquet) to validate",
)
@click.option(
    "--tolerance",
    type=float,
    default=1e-6,
    help="Tolerance for metric comparison",
)
def validate(rust_results: Path, gpu_results: Path, tolerance: float):
    """Validate GPU results against Rust implementation."""
    import polars as pl

    console.print(f"[cyan]Validating GPU results against Rust...[/cyan]")
    console.print(f"  Rust: {rust_results}")
    console.print(f"  GPU:  {gpu_results}")
    console.print(f"  Tolerance: {tolerance}")
    console.print()

    # Load both result sets
    rust_df = pl.read_parquet(rust_results)
    gpu_df = pl.read_parquet(gpu_results)

    # Compare metrics
    metrics = ["sharpe", "cagr", "max_drawdown", "total_return"]
    all_passed = True

    for metric in metrics:
        if metric not in rust_df.columns or metric not in gpu_df.columns:
            console.print(f"[yellow]Skipping {metric} - not in both results[/yellow]")
            continue

        rust_vals = rust_df[metric].to_numpy()
        gpu_vals = gpu_df[metric].to_numpy()

        if len(rust_vals) != len(gpu_vals):
            console.print(f"[red]{metric}: Length mismatch ({len(rust_vals)} vs {len(gpu_vals)})[/red]")
            all_passed = False
            continue

        max_diff = abs(rust_vals - gpu_vals).max()
        if max_diff <= tolerance:
            console.print(f"[green]{metric}: PASS (max diff: {max_diff:.2e})[/green]")
        else:
            console.print(f"[red]{metric}: FAIL (max diff: {max_diff:.2e})[/red]")
            all_passed = False

    console.print()
    if all_passed:
        console.print("[green]All metrics within tolerance![/green]")
    else:
        console.print("[red]Some metrics exceeded tolerance[/red]")
        sys.exit(1)


@cli.command()
def gpu_info():
    """Show GPU information and CUDA status."""
    if not _check_cuda():
        console.print("[red]CUDA not available[/red]")
        return

    import cupy as cp

    device = cp.cuda.Device()
    mem_info = device.mem_info

    console.print("[cyan]GPU Information[/cyan]")
    console.print(f"  Device: {device.id}")
    console.print(f"  Name: {cp.cuda.runtime.getDeviceProperties(device.id)['name'].decode()}")
    console.print(f"  Memory: {mem_info[1] / 1e9:.1f} GB total, {mem_info[0] / 1e9:.1f} GB free")
    console.print(f"  CUDA Version: {cp.cuda.runtime.runtimeGetVersion()}")


def _check_cuda() -> bool:
    """Check if CUDA is available."""
    try:
        import cupy as cp
        cp.cuda.Device()
        return True
    except Exception:
        return False


def _run_sweep(config: SweepConfig, output: Path | None):
    """Execute the GPU sweep."""
    # Import here to avoid slow startup
    from .engine import run_sweep

    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        console=console,
    ) as progress:
        task = progress.add_task("Running GPU sweep...", total=None)

        try:
            results = run_sweep(config, progress_callback=lambda msg: progress.update(task, description=msg))
        except Exception as e:
            console.print(f"[red]Sweep failed: {e}[/red]")
            raise

    # Save results
    output_path = output or (config.output_dir / "sweep_results.parquet")
    output_path.parent.mkdir(parents=True, exist_ok=True)

    results.write_parquet(output_path)
    console.print(f"[green]Results saved to {output_path}[/green]")

    # Show summary
    console.print()
    console.print(f"[cyan]Sweep Complete[/cyan]")
    console.print(f"  Total results: {len(results):,}")


def main():
    """Main entry point."""
    cli()


if __name__ == "__main__":
    main()
