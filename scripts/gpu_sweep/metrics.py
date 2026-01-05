"""
GPU-accelerated performance metrics with exact Rust parity.

All metrics match the implementations in crates/trendlab-core/src/metrics.rs.
"""

import cupy as cp
import numpy as np


def compute_returns_gpu(equity: cp.ndarray) -> cp.ndarray:
    """
    Compute daily returns from equity curve.

    Args:
        equity: Equity curve, shape [num_configs, num_bars]

    Returns:
        Daily returns, shape [num_configs, num_bars - 1]
        r[t] = (equity[t] - equity[t-1]) / equity[t-1]
    """
    # Avoid division by zero
    prev_equity = equity[:, :-1]
    prev_equity = cp.where(cp.abs(prev_equity) < 1e-10, 1e-10, prev_equity)

    returns = (equity[:, 1:] - equity[:, :-1]) / prev_equity
    return cp.nan_to_num(returns, nan=0.0, posinf=0.0, neginf=0.0)


def sharpe_gpu(equity: cp.ndarray) -> cp.ndarray:
    """
    Compute annualized Sharpe ratio for multiple equity curves.

    Matches Rust calculate_sharpe() exactly:
    - Assumes 252 trading days per year
    - Risk-free rate of 0
    - Uses population standard deviation (n, not n-1)

    Args:
        equity: Equity curve, shape [num_configs, num_bars]

    Returns:
        Sharpe ratios, shape [num_configs]
    """
    if equity.shape[1] < 2:
        return cp.zeros(equity.shape[0], dtype=cp.float32)

    # Daily returns
    returns = compute_returns_gpu(equity)

    # Mean and standard deviation
    n = returns.shape[1]
    mean_ret = cp.sum(returns, axis=1) / n

    # Population variance (matching Rust)
    variance = cp.sum((returns - mean_ret[:, None]) ** 2, axis=1) / n
    std_ret = cp.sqrt(variance)

    # Annualize: (mean * 252) / (std * sqrt(252))
    # This simplifies to: mean * sqrt(252) / std
    sharpe = (mean_ret * 252.0) / (std_ret * cp.sqrt(252.0) + 1e-10)

    return cp.where(std_ret == 0, 0.0, sharpe).astype(cp.float32)


def sortino_gpu(equity: cp.ndarray) -> cp.ndarray:
    """
    Compute annualized Sortino ratio for multiple equity curves.

    Like Sharpe but only penalizes downside volatility.
    Matches Rust calculate_sortino() exactly.

    Args:
        equity: Equity curve, shape [num_configs, num_bars]

    Returns:
        Sortino ratios, shape [num_configs]
    """
    if equity.shape[1] < 2:
        return cp.zeros(equity.shape[0], dtype=cp.float32)

    # Daily returns
    returns = compute_returns_gpu(equity)

    # Mean return
    n = returns.shape[1]
    mean_ret = cp.sum(returns, axis=1) / n

    # Downside deviation: only consider returns below zero
    negative_returns = cp.where(returns < 0, returns, 0.0)
    downside_variance = cp.sum(negative_returns**2, axis=1) / n
    downside_dev = cp.sqrt(downside_variance)

    # Annualize
    sortino = (mean_ret * 252.0) / (downside_dev * cp.sqrt(252.0) + 1e-10)

    return cp.where(downside_dev == 0, 0.0, sortino).astype(cp.float32)


def cagr_gpu(
    equity: cp.ndarray,
    initial_cash: float,
    years: float,
) -> cp.ndarray:
    """
    Compute CAGR for multiple equity curves.

    Matches Rust calculate_cagr() exactly.

    Args:
        equity: Equity curve, shape [num_configs, num_bars]
        initial_cash: Starting capital
        years: Number of years in backtest period

    Returns:
        CAGR values, shape [num_configs]
    """
    if initial_cash <= 0.0 or years <= 0.0:
        return cp.zeros(equity.shape[0], dtype=cp.float32)

    final_equity = equity[:, -1]
    cagr = (final_equity / initial_cash) ** (1.0 / years) - 1.0

    return cagr.astype(cp.float32)


def total_return_gpu(equity: cp.ndarray, initial_cash: float) -> cp.ndarray:
    """
    Compute total return for multiple equity curves.

    Args:
        equity: Equity curve, shape [num_configs, num_bars]
        initial_cash: Starting capital

    Returns:
        Total returns (as decimals), shape [num_configs]
    """
    if initial_cash <= 0.0:
        return cp.zeros(equity.shape[0], dtype=cp.float32)

    final_equity = equity[:, -1]
    return ((final_equity - initial_cash) / initial_cash).astype(cp.float32)


def _running_max_gpu(arr: cp.ndarray) -> cp.ndarray:
    """
    Compute running maximum along axis=1 without cp.maximum.accumulate.

    Uses a parallel prefix-sum style algorithm with log(n) iterations.
    This works by repeatedly computing max with shifted versions.

    Args:
        arr: Input array, shape [num_configs, num_bars]

    Returns:
        Running maximum, shape [num_configs, num_bars]
    """
    n_bars = arr.shape[1]
    if n_bars == 0:
        return arr.copy()

    result = arr.copy()

    # Parallel prefix max using doubling technique
    # Each iteration i computes max with offset 2^i
    offset = 1
    while offset < n_bars:
        # Create shifted version (shift right by offset, pad with -inf)
        shifted = cp.full_like(result, -cp.inf)
        shifted[:, offset:] = result[:, :-offset]
        result = cp.maximum(result, shifted)
        offset *= 2

    return result


def max_drawdown_gpu(equity: cp.ndarray) -> cp.ndarray:
    """
    Compute max drawdown for multiple equity curves.

    Matches Rust calculate_max_drawdown() exactly.
    Uses running peak tracking.

    Args:
        equity: Equity curve, shape [num_configs, num_bars]

    Returns:
        Max drawdowns (as positive decimals), shape [num_configs]
    """
    if equity.shape[1] == 0:
        return cp.zeros(equity.shape[0], dtype=cp.float32)

    # Running maximum (cumulative max along bar axis)
    running_peak = _running_max_gpu(equity)

    # Drawdown at each point: (peak - equity) / peak
    # Guard against division by zero
    safe_peak = cp.where(cp.abs(running_peak) < 1e-10, 1e-10, running_peak)
    drawdown = (running_peak - equity) / safe_peak

    # Max drawdown per config
    return cp.max(drawdown, axis=1).astype(cp.float32)


def calmar_gpu(
    equity: cp.ndarray,
    initial_cash: float,
    years: float,
) -> cp.ndarray:
    """
    Compute Calmar ratio for multiple equity curves.

    Calmar = CAGR / Max Drawdown
    Matches Rust implementation.

    Args:
        equity: Equity curve, shape [num_configs, num_bars]
        initial_cash: Starting capital
        years: Number of years in backtest period

    Returns:
        Calmar ratios, shape [num_configs]
    """
    cagr_vals = cagr_gpu(equity, initial_cash, years)
    max_dd = max_drawdown_gpu(equity)

    # Calmar = CAGR / max_drawdown (where max_drawdown > 0)
    calmar = cp.where(max_dd > 0, cagr_vals / max_dd, 0.0)

    return calmar.astype(cp.float32)


def win_rate_gpu(
    entry_bars: cp.ndarray,
    exit_bars: cp.ndarray,
    open_prices: cp.ndarray,
    fees_bps: float = 10.0,
    qty_per_trade: float = 100.0,
) -> cp.ndarray:
    """
    Compute win rate for multiple configs.

    Win rate = winning_trades / total_trades

    Args:
        entry_bars: Boolean array where entries occurred [num_configs, num_bars]
        exit_bars: Boolean array where exits occurred
        open_prices: Open prices, shape [num_bars]
        fees_bps: Fees in basis points per side
        qty_per_trade: Shares per trade

    Returns:
        Win rates, shape [num_configs]
    """
    num_configs, num_bars = entry_bars.shape
    fee_mult = fees_bps / 10000.0

    # Broadcast prices if needed
    if open_prices.ndim == 1:
        open_prices = cp.broadcast_to(open_prices, (num_configs, num_bars))

    # Count trades and winning trades
    entry_price = cp.zeros((num_configs,), dtype=cp.float32)
    num_trades = cp.zeros((num_configs,), dtype=cp.int32)
    num_wins = cp.zeros((num_configs,), dtype=cp.int32)

    for t in range(num_bars):
        # Record entry prices
        entering = entry_bars[:, t]
        entry_price = cp.where(entering, open_prices[:, t], entry_price)

        # Check wins on exits
        exiting = exit_bars[:, t]
        if cp.any(exiting):
            exit_price = open_prices[:, t]
            gross_pnl = (exit_price - entry_price) * qty_per_trade
            fees = (entry_price + exit_price) * qty_per_trade * fee_mult
            net_pnl = gross_pnl - fees

            is_win = net_pnl > 0
            num_wins = cp.where(exiting & is_win, num_wins + 1, num_wins)
            num_trades = cp.where(exiting, num_trades + 1, num_trades)

    # Win rate
    win_rate = cp.where(num_trades > 0, num_wins.astype(cp.float32) / num_trades, 0.0)

    return win_rate.astype(cp.float32)


def profit_factor_gpu(
    entry_bars: cp.ndarray,
    exit_bars: cp.ndarray,
    open_prices: cp.ndarray,
    fees_bps: float = 10.0,
    qty_per_trade: float = 100.0,
) -> tuple[cp.ndarray, cp.ndarray, cp.ndarray]:
    """
    Compute profit factor for multiple configs.

    Profit factor = gross_profit / gross_loss
    Matches Rust implementation.

    Args:
        entry_bars: Boolean array where entries occurred [num_configs, num_bars]
        exit_bars: Boolean array where exits occurred
        open_prices: Open prices, shape [num_bars]
        fees_bps: Fees in basis points per side
        qty_per_trade: Shares per trade

    Returns:
        Tuple of (profit_factor, gross_profit, gross_loss)
    """
    num_configs, num_bars = entry_bars.shape
    fee_mult = fees_bps / 10000.0

    # Broadcast prices if needed
    if open_prices.ndim == 1:
        open_prices = cp.broadcast_to(open_prices, (num_configs, num_bars))

    entry_price = cp.zeros((num_configs,), dtype=cp.float32)
    gross_profit = cp.zeros((num_configs,), dtype=cp.float32)
    gross_loss = cp.zeros((num_configs,), dtype=cp.float32)

    for t in range(num_bars):
        # Record entry prices
        entering = entry_bars[:, t]
        entry_price = cp.where(entering, open_prices[:, t], entry_price)

        # Compute PnL on exits
        exiting = exit_bars[:, t]
        if cp.any(exiting):
            exit_price = open_prices[:, t]
            gross_pnl = (exit_price - entry_price) * qty_per_trade
            fees = (entry_price + exit_price) * qty_per_trade * fee_mult
            net_pnl = gross_pnl - fees

            # Accumulate profits and losses
            is_profit = net_pnl > 0
            gross_profit = cp.where(
                exiting & is_profit,
                gross_profit + net_pnl,
                gross_profit,
            )
            gross_loss = cp.where(
                exiting & ~is_profit,
                gross_loss + cp.abs(net_pnl),
                gross_loss,
            )

    # Profit factor: gross_profit / gross_loss
    # Handle edge cases matching Rust
    pf = cp.where(
        gross_loss > 0,
        gross_profit / gross_loss,
        cp.where(gross_profit > 0, cp.inf, 0.0),
    )

    return pf.astype(cp.float32), gross_profit, gross_loss


def num_trades_gpu(exit_bars: cp.ndarray) -> cp.ndarray:
    """
    Count completed trades for multiple configs.

    A completed trade is an exit (matching Rust implementation).

    Args:
        exit_bars: Boolean array where exits occurred [num_configs, num_bars]

    Returns:
        Number of trades per config, shape [num_configs]
    """
    return cp.sum(exit_bars, axis=1).astype(cp.int32)


def turnover_gpu(
    entry_bars: cp.ndarray,
    exit_bars: cp.ndarray,
    open_prices: cp.ndarray,
    equity: cp.ndarray,
    initial_cash: float,
    years: float,
    qty_per_trade: float = 100.0,
) -> cp.ndarray:
    """
    Compute annual turnover for multiple configs.

    Turnover = (total traded notional / average capital) / years
    Matches Rust implementation.

    Args:
        entry_bars: Boolean array where entries occurred [num_configs, num_bars]
        exit_bars: Boolean array where exits occurred
        open_prices: Open prices, shape [num_bars]
        equity: Equity curve [num_configs, num_bars]
        initial_cash: Starting capital
        years: Number of years in backtest period
        qty_per_trade: Shares per trade

    Returns:
        Annual turnover per config, shape [num_configs]
    """
    num_configs, num_bars = entry_bars.shape

    # Broadcast prices if needed
    if open_prices.ndim == 1:
        open_prices = cp.broadcast_to(open_prices, (num_configs, num_bars))

    # Total traded notional
    entry_notional = cp.sum(
        cp.where(entry_bars, open_prices * qty_per_trade, 0.0),
        axis=1,
    )
    exit_notional = cp.sum(
        cp.where(exit_bars, open_prices * qty_per_trade, 0.0),
        axis=1,
    )
    total_traded = entry_notional + exit_notional

    # Average capital
    final_equity = equity[:, -1]
    avg_capital = (initial_cash + final_equity) / 2.0

    # Annual turnover
    turnover = cp.where(
        (years > 0) & (avg_capital > 0),
        (total_traded / avg_capital) / years,
        0.0,
    )

    return turnover.astype(cp.float32)


def compute_all_metrics_gpu(
    equity: cp.ndarray,
    entry_bars: cp.ndarray,
    exit_bars: cp.ndarray,
    open_prices: cp.ndarray,
    initial_cash: float = 100000.0,
    years: float = 10.0,
    fees_bps: float = 10.0,
    qty_per_trade: float = 100.0,
) -> dict[str, cp.ndarray]:
    """
    Compute all performance metrics for multiple configs.

    This is the main entry point for metrics computation.

    Args:
        equity: Equity curve [num_configs, num_bars]
        entry_bars: Boolean array where entries occurred
        exit_bars: Boolean array where exits occurred
        open_prices: Open prices, shape [num_bars]
        initial_cash: Starting capital
        years: Number of years in backtest period
        fees_bps: Fees in basis points per side
        qty_per_trade: Shares per trade

    Returns:
        Dict mapping metric names to arrays of shape [num_configs]
    """
    # Compute equity-based metrics
    total_ret = total_return_gpu(equity, initial_cash)
    cagr_val = cagr_gpu(equity, initial_cash, years)
    sharpe_val = sharpe_gpu(equity)
    sortino_val = sortino_gpu(equity)
    max_dd = max_drawdown_gpu(equity)
    calmar_val = calmar_gpu(equity, initial_cash, years)

    # Compute trade-based metrics
    win_rate_val = win_rate_gpu(entry_bars, exit_bars, open_prices, fees_bps, qty_per_trade)
    pf, gross_profit, gross_loss = profit_factor_gpu(
        entry_bars, exit_bars, open_prices, fees_bps, qty_per_trade
    )
    n_trades = num_trades_gpu(exit_bars)
    turn = turnover_gpu(
        entry_bars, exit_bars, open_prices, equity, initial_cash, years, qty_per_trade
    )

    return {
        "total_return": total_ret,
        "cagr": cagr_val,
        "sharpe": sharpe_val,
        "sortino": sortino_val,
        "max_drawdown": max_dd,
        "calmar": calmar_val,
        "win_rate": win_rate_val,
        "profit_factor": pf,
        "num_trades": n_trades,
        "turnover": turn,
    }


def metrics_to_numpy(metrics: dict[str, cp.ndarray]) -> dict[str, np.ndarray]:
    """
    Convert GPU metrics dict to numpy arrays for Polars/CPU consumption.

    Args:
        metrics: Dict from compute_all_metrics_gpu

    Returns:
        Dict with same keys but numpy arrays
    """
    return {k: cp.asnumpy(v) for k, v in metrics.items()}
