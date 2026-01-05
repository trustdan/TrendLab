"""
GPU Mega-Sweep engine - orchestrates the full backtest pipeline.

Coordinates data loading, indicator computation, signal generation,
position scanning, and metrics calculation across batches of configs.

V2: Uses batched indicator cache and vectorized signal generation
to achieve true GPU parallelization (20-100x speedup).
"""

from datetime import date
from pathlib import Path
from typing import Any, Callable

import cupy as cp
import numpy as np
import polars as pl

from .batched_cache import BatchedIndicatorCache
from .config import SweepConfig
from .data import load_symbol_to_arrays
from .indicator_graph import collect_strategy_indicators
from .indicators import IndicatorCache
from .metrics import compute_all_metrics_gpu, metrics_to_numpy
from .position_scan import position_scan_with_fills_gpu
from .signal_batched import compute_signals_batched
from .strategies.base import IndicatorKey, IndicatorReq


# Memory budget: estimate ~54 MB per config layer (from plan)
# With 12 GB usable, can fit ~220 configs at once
DEFAULT_BATCH_SIZE = 200


def estimate_batch_size() -> int:
    """
    Estimate optimal batch size based on available GPU memory.

    Returns a conservative batch size to avoid OOM errors.
    """
    try:
        device = cp.cuda.Device()
        free_mem, total_mem = device.mem_info
        # Use 70% of free memory for headroom
        usable_mem = free_mem * 0.7
        # Rough estimate: 54 MB per config layer
        batch_size = int(usable_mem / (54 * 1024 * 1024))
        return max(10, min(batch_size, 1000))  # Clamp to reasonable range
    except Exception:
        return DEFAULT_BATCH_SIZE


def run_sweep(
    config: SweepConfig,
    progress_callback: Callable[[str], None] | None = None,
) -> pl.DataFrame:
    """
    Run a complete GPU-accelerated parameter sweep.

    This is the main entry point for sweep execution.

    Args:
        config: Complete sweep configuration
        progress_callback: Optional callback for progress updates

    Returns:
        DataFrame with results for all configs and symbols
    """

    def update_progress(msg: str):
        if progress_callback:
            progress_callback(msg)

    update_progress("Initializing GPU...")

    # Estimate batch size
    batch_size = estimate_batch_size()
    update_progress(f"Using batch size: {batch_size}")

    # Calculate date range for years calculation
    if config.data.start_date and config.data.end_date:
        days = (config.data.end_date - config.data.start_date).days
        years = days / 365.25
    else:
        years = 10.0  # Default assumption

    all_results = []

    # Process each symbol
    for symbol_idx, symbol in enumerate(config.symbols):
        update_progress(f"Processing {symbol} ({symbol_idx + 1}/{len(config.symbols)})")

        # Load symbol data
        try:
            ohlcv = load_symbol_to_arrays(
                config.data.base_dir,
                symbol,
                config.data.timeframe,
                config.data.start_date,
                config.data.end_date,
            )
        except Exception as e:
            update_progress(f"Skipping {symbol}: {e}")
            continue

        # Extract price arrays
        open_prices = ohlcv["open"]
        high_prices = ohlcv["high"]
        low_prices = ohlcv["low"]
        close_prices = ohlcv["close"]
        num_bars = len(close_prices)

        if num_bars < 50:
            update_progress(f"Skipping {symbol}: too few bars ({num_bars})")
            continue

        # Process each strategy
        for strategy_name, grid in config.strategies.items():
            configs = grid.generate_configs()
            if not configs:
                continue

            update_progress(f"  {strategy_name}: {len(configs)} configs")

            # Run in batches
            for batch_start in range(0, len(configs), batch_size):
                batch_end = min(batch_start + batch_size, len(configs))
                batch_configs = configs[batch_start:batch_end]

                # Run batch
                batch_results = _run_strategy_batch(
                    strategy_name=strategy_name,
                    configs=batch_configs,
                    open_prices=open_prices,
                    high_prices=high_prices,
                    low_prices=low_prices,
                    close_prices=close_prices,
                    initial_cash=config.backtest.initial_cash,
                    fees_bps=config.backtest.fees_bps,
                    qty_per_trade=config.backtest.qty_per_trade,
                    years=years,
                )

                # Add metadata columns
                for i, result in enumerate(batch_results):
                    result["symbol"] = symbol
                    result["strategy_type"] = strategy_name
                    result["config_id"] = batch_start + i
                    result.update(batch_configs[i])  # Add strategy params

                all_results.extend(batch_results)

    # Build final DataFrame
    if not all_results:
        return _empty_results_df()

    update_progress("Building results DataFrame...")
    return pl.DataFrame(all_results)


def _run_strategy_batch(
    strategy_name: str,
    configs: list[dict[str, Any]],
    open_prices: cp.ndarray,
    high_prices: cp.ndarray,
    low_prices: cp.ndarray,
    close_prices: cp.ndarray,
    initial_cash: float,
    fees_bps: float,
    qty_per_trade: float,
    years: float,
) -> list[dict[str, Any]]:
    """
    Run a batch of configs for a single strategy on a single symbol.

    V2: Uses batched indicator cache and vectorized signal generation.
    Indicators are computed once per unique (type, params) combination,
    then reused across all configs that need them.
    """
    num_configs = len(configs)
    num_bars = len(close_prices)

    # Create batched indicator cache (V2 - with deduplication)
    cache = BatchedIndicatorCache(
        high=high_prices,
        low=low_prices,
        close=close_prices,
        open_prices=open_prices,
    )

    # Collect and pre-compute all unique indicators for this strategy
    specs = collect_strategy_indicators(strategy_name, configs)
    cache.ensure_computed(specs)

    # Generate signals for ALL configs at once (V2 - vectorized)
    entry_signals, exit_signals = compute_signals_batched(
        strategy_type=strategy_name,
        configs=configs,
        cache=cache,
        open_prices=open_prices,
        high_prices=high_prices,
        low_prices=low_prices,
        close_prices=close_prices,
    )

    # Run position scan with fills (already batched)
    position, equity, entry_bars, exit_bars = position_scan_with_fills_gpu(
        entry_signals=entry_signals,
        exit_signals=exit_signals,
        open_prices=open_prices,
        close_prices=close_prices,
        fees_bps=fees_bps,
        qty_per_trade=qty_per_trade,
    )

    # Compute metrics (already batched)
    metrics = compute_all_metrics_gpu(
        equity=equity,
        entry_bars=entry_bars,
        exit_bars=exit_bars,
        open_prices=open_prices,
        initial_cash=initial_cash,
        years=years,
        fees_bps=fees_bps,
        qty_per_trade=qty_per_trade,
    )

    # Convert to numpy for results
    metrics_np = metrics_to_numpy(metrics)

    # Build result dicts
    results = []
    for i in range(num_configs):
        result = {
            "total_return": float(metrics_np["total_return"][i]),
            "cagr": float(metrics_np["cagr"][i]),
            "sharpe": float(metrics_np["sharpe"][i]),
            "sortino": float(metrics_np["sortino"][i]),
            "max_drawdown": float(metrics_np["max_drawdown"][i]),
            "calmar": float(metrics_np["calmar"][i]),
            "win_rate": float(metrics_np["win_rate"][i]),
            "profit_factor": float(metrics_np["profit_factor"][i]),
            "num_trades": int(metrics_np["num_trades"][i]),
            "turnover": float(metrics_np["turnover"][i]),
        }
        results.append(result)

    return results


def _compute_signals_for_strategy(
    strategy_name: str,
    config: dict[str, Any],
    open_prices: cp.ndarray,
    high_prices: cp.ndarray,
    low_prices: cp.ndarray,
    close_prices: cp.ndarray,
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """
    Compute entry/exit signals for a specific strategy and config.

    This dispatches to the appropriate signal generator based on strategy type.
    """
    num_bars = len(close_prices)

    # Dispatch based on strategy type
    if strategy_name == "donchian":
        return _signals_donchian(high_prices, low_prices, close_prices, config, cache)
    elif strategy_name == "ma_crossover":
        return _signals_ma_crossover(close_prices, config, cache)
    elif strategy_name == "supertrend":
        return _signals_supertrend(high_prices, low_prices, close_prices, config, cache)
    elif strategy_name == "fifty_two_week":
        return _signals_fifty_two_week(high_prices, close_prices, config, cache)
    elif strategy_name == "parabolic_sar":
        return _signals_parabolic_sar(high_prices, low_prices, close_prices, config, cache)
    elif strategy_name == "bollinger":
        return _signals_bollinger(close_prices, config, cache)
    elif strategy_name == "rsi":
        return _signals_rsi(close_prices, config, cache)
    elif strategy_name == "macd":
        return _signals_macd(close_prices, config, cache)
    elif strategy_name == "aroon":
        return _signals_aroon(high_prices, low_prices, config, cache)
    elif strategy_name == "tsmom":
        return _signals_tsmom(close_prices, config, cache)
    else:
        # Unknown strategy - return no signals
        return cp.zeros(num_bars, dtype=cp.bool_), cp.zeros(num_bars, dtype=cp.bool_)


# ---------------------------------------------------------------------------
# Strategy-specific signal generators
# ---------------------------------------------------------------------------


def _signals_donchian(
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Donchian/Turtle breakout signals."""
    from .indicators import donchian_gpu

    entry_lookback = config["entry_lookback"]
    exit_lookback = config["exit_lookback"]

    # Entry channel (higher highs)
    entry_high, _ = donchian_gpu(high, low, entry_lookback)

    # Exit channel (lower lows)
    _, exit_low = donchian_gpu(high, low, exit_lookback)

    # Entry: close breaks above entry channel high
    entry = close > cp.roll(entry_high, 1)
    entry[:entry_lookback] = False

    # Exit: close breaks below exit channel low
    exit_ = close < cp.roll(exit_low, 1)
    exit_[:exit_lookback] = False

    return entry, exit_


def _signals_ma_crossover(
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Moving average crossover signals."""
    from .indicators import ema_gpu, sma_gpu

    fast_period = config["fast_period"]
    slow_period = config["slow_period"]
    ma_type = config.get("ma_type", "sma")

    if ma_type == "ema":
        fast_ma = ema_gpu(close, fast_period)
        slow_ma = ema_gpu(close, slow_period)
    else:
        fast_ma = sma_gpu(close, fast_period)
        slow_ma = sma_gpu(close, slow_period)

    # Entry: fast crosses above slow
    prev_fast = cp.roll(fast_ma, 1)
    prev_slow = cp.roll(slow_ma, 1)
    entry = (fast_ma > slow_ma) & (prev_fast <= prev_slow)
    entry[:slow_period] = False

    # Exit: fast crosses below slow
    exit_ = (fast_ma < slow_ma) & (prev_fast >= prev_slow)
    exit_[:slow_period] = False

    return entry, exit_


def _signals_supertrend(
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Supertrend signals."""
    from .indicators import supertrend_gpu

    atr_period = config["atr_period"]
    multiplier = config["multiplier"]

    supertrend, direction, _, _ = supertrend_gpu(high, low, close, atr_period, multiplier)

    # Entry: direction flips from -1 to 1 (bullish)
    prev_direction = cp.roll(direction, 1)
    entry = (direction > 0) & (prev_direction <= 0)
    entry[:atr_period + 1] = False

    # Exit: direction flips from 1 to -1 (bearish)
    exit_ = (direction < 0) & (prev_direction >= 0)
    exit_[:atr_period + 1] = False

    return entry, exit_


def _signals_fifty_two_week(
    high: cp.ndarray,
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """52-week high proximity signals."""
    from .indicators import rolling_max_gpu

    period = config["period"]
    entry_pct = config["entry_pct"]
    exit_pct = config["exit_pct"]

    # Rolling high
    rolling_high = rolling_max_gpu(high, period)

    # Proximity to high
    proximity = close / (rolling_high + 1e-10)

    # Entry: close is within entry_pct of rolling high
    entry = proximity >= entry_pct
    entry[:period] = False

    # Exit: close falls below exit_pct of rolling high
    exit_ = proximity < exit_pct
    exit_[:period] = False

    return entry, exit_


def _signals_parabolic_sar(
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Parabolic SAR signals (simplified)."""
    # Full SAR is complex - use a simplified version based on ATR trailing stop
    from .indicators import atr_gpu

    af_start = config.get("af_start", 0.02)
    af_step = config.get("af_step", 0.02)
    af_max = config.get("af_max", 0.2)

    # Use ATR-based approximation
    atr = atr_gpu(high, low, close, 14)
    multiplier = 3.0 / af_start  # Approximate SAR behavior

    # Simple trend following: close above/below ATR bands
    upper_band = close + atr * multiplier
    lower_band = close - atr * multiplier

    # Entry: close above lower band after being below
    prev_close = cp.roll(close, 1)
    entry = close > cp.roll(lower_band, 1)
    entry[:15] = False

    # Exit: close below lower band
    exit_ = close < cp.roll(lower_band, 1)
    exit_[:15] = False

    return entry, exit_


def _signals_bollinger(
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Bollinger Band signals."""
    from .indicators import bollinger_bands_gpu

    period = config["period"]
    std_mult = config["std_mult"]

    middle, upper, lower, bandwidth = bollinger_bands_gpu(close, period, std_mult)

    # Entry: close touches lower band (mean reversion long)
    entry = close <= lower
    entry[:period] = False

    # Exit: close reaches middle band
    exit_ = close >= middle
    exit_[:period] = False

    return entry, exit_


def _signals_rsi(
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """RSI overbought/oversold signals."""
    from .indicators import rsi_gpu

    period = config["period"]
    overbought = config["overbought"]
    oversold = config["oversold"]

    rsi = rsi_gpu(close, period)

    # Entry: RSI crosses above oversold (mean reversion long)
    prev_rsi = cp.roll(rsi, 1)
    entry = (rsi > oversold) & (prev_rsi <= oversold)
    entry[:period + 1] = False

    # Exit: RSI crosses below overbought
    exit_ = (rsi < overbought) & (prev_rsi >= overbought)
    exit_[:period + 1] = False

    return entry, exit_


def _signals_macd(
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """MACD crossover signals."""
    from .indicators import macd_gpu

    fast = config["fast_period"]
    slow = config["slow_period"]
    signal = config["signal_period"]

    macd_line, signal_line, histogram = macd_gpu(close, fast, slow, signal)

    # Entry: MACD crosses above signal
    prev_macd = cp.roll(macd_line, 1)
    prev_signal = cp.roll(signal_line, 1)
    entry = (macd_line > signal_line) & (prev_macd <= prev_signal)
    entry[:slow + signal] = False

    # Exit: MACD crosses below signal
    exit_ = (macd_line < signal_line) & (prev_macd >= prev_signal)
    exit_[:slow + signal] = False

    return entry, exit_


def _signals_aroon(
    high: cp.ndarray,
    low: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Aroon crossover signals."""
    from .indicators import aroon_gpu

    period = config["period"]

    aroon_up, aroon_down, aroon_osc = aroon_gpu(high, low, period)

    # Entry: Aroon Up crosses above Aroon Down
    prev_up = cp.roll(aroon_up, 1)
    prev_down = cp.roll(aroon_down, 1)
    entry = (aroon_up > aroon_down) & (prev_up <= prev_down)
    entry[:period + 1] = False

    # Exit: Aroon Down crosses above Aroon Up
    exit_ = (aroon_down > aroon_up) & (prev_down <= prev_up)
    exit_[:period + 1] = False

    return entry, exit_


def _signals_tsmom(
    close: cp.ndarray,
    config: dict[str, Any],
    cache: IndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Time-series momentum signals."""
    lookback = config["lookback"]
    num_bars = len(close)

    # Momentum: current close vs close lookback periods ago
    lagged_close = cp.roll(close, lookback)
    momentum = (close - lagged_close) / (lagged_close + 1e-10)

    # Entry: positive momentum
    prev_momentum = cp.roll(momentum, 1)
    entry = (momentum > 0) & (prev_momentum <= 0)
    entry[:lookback + 1] = False

    # Exit: negative momentum
    exit_ = (momentum < 0) & (prev_momentum >= 0)
    exit_[:lookback + 1] = False

    return entry, exit_


def _empty_results_df() -> pl.DataFrame:
    """Create an empty results DataFrame with correct schema."""
    return pl.DataFrame({
        "symbol": pl.Series([], dtype=pl.Utf8),
        "strategy_type": pl.Series([], dtype=pl.Utf8),
        "config_id": pl.Series([], dtype=pl.Int64),
        "total_return": pl.Series([], dtype=pl.Float64),
        "cagr": pl.Series([], dtype=pl.Float64),
        "sharpe": pl.Series([], dtype=pl.Float64),
        "sortino": pl.Series([], dtype=pl.Float64),
        "max_drawdown": pl.Series([], dtype=pl.Float64),
        "calmar": pl.Series([], dtype=pl.Float64),
        "win_rate": pl.Series([], dtype=pl.Float64),
        "profit_factor": pl.Series([], dtype=pl.Float64),
        "num_trades": pl.Series([], dtype=pl.Int64),
        "turnover": pl.Series([], dtype=pl.Float64),
    })
