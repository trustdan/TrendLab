"""
GPU-accelerated position scan for computing position state from signals.

The position scan converts raw entry/exit signals into position states,
handling the state machine transitions:
- Flat + entry signal → Long (filled at next bar open)
- Long + exit signal → Flat (filled at next bar open)

This is a parallel prefix scan operation, but with state constraints
(can't enter if already in position, can't exit if flat).
"""

import cupy as cp


def position_scan_gpu(
    entry_signals: cp.ndarray,
    exit_signals: cp.ndarray,
) -> cp.ndarray:
    """
    Compute position state from entry/exit signals.

    Signal at bar T is filled at bar T+1 open.
    This function returns position state AT EACH BAR CLOSE.

    Args:
        entry_signals: Boolean array, shape [num_configs, num_bars]
                      True when entry condition is met
        exit_signals: Boolean array, same shape
                     True when exit condition is met

    Returns:
        Position state array, shape [num_configs, num_bars]
        Values: 0 = flat, 1 = long
        Position at bar T reflects fills executed at bar T's open.
    """
    num_configs, num_bars = entry_signals.shape

    # Initialize position (start flat)
    position = cp.zeros((num_configs, num_bars), dtype=cp.int8)

    # Process bar by bar
    # Position at bar T depends on:
    # 1. Position at bar T-1
    # 2. Signal at bar T-1 (which fills at bar T open)
    for t in range(1, num_bars):
        prev_pos = position[:, t - 1]
        prev_entry = entry_signals[:, t - 1]
        prev_exit = exit_signals[:, t - 1]

        # State transitions:
        # Flat (0) + entry → Long (1)
        # Long (1) + exit → Flat (0)
        # Otherwise: maintain position

        # New position = prev_pos + (enter if flat) - (exit if long)
        enter = (prev_pos == 0) & prev_entry
        exit_ = (prev_pos == 1) & prev_exit

        position[:, t] = prev_pos + enter.astype(cp.int8) - exit_.astype(cp.int8)

    return position


def position_scan_with_fills_gpu(
    entry_signals: cp.ndarray,
    exit_signals: cp.ndarray,
    open_prices: cp.ndarray,
    close_prices: cp.ndarray,
    fees_bps: float = 10.0,
    qty_per_trade: float = 100.0,
) -> tuple[cp.ndarray, cp.ndarray, cp.ndarray, cp.ndarray]:
    """
    Compute position state and equity curve from signals.

    This is the full backtest simulation:
    1. Signal at bar T-1 close
    2. Fill at bar T open
    3. Mark-to-market at bar T close

    Args:
        entry_signals: Boolean array, shape [num_configs, num_bars]
        exit_signals: Boolean array, same shape
        open_prices: Open prices, shape [num_bars] (shared across configs)
        close_prices: Close prices, same shape
        fees_bps: Fees in basis points per side
        qty_per_trade: Shares per trade

    Returns:
        Tuple of:
        - position: Position state [num_configs, num_bars]
        - equity: Equity curve [num_configs, num_bars]
        - entry_bars: Boolean array where entries occurred
        - exit_bars: Boolean array where exits occurred
    """
    num_configs, num_bars = entry_signals.shape

    # Broadcast prices to config dimension
    if open_prices.ndim == 1:
        open_prices = cp.broadcast_to(open_prices, (num_configs, num_bars))
        close_prices = cp.broadcast_to(close_prices, (num_configs, num_bars))

    # Fee multiplier
    fee_mult = fees_bps / 10000.0

    # Initialize arrays
    position = cp.zeros((num_configs, num_bars), dtype=cp.int8)
    cash = cp.full((num_configs, num_bars), 100000.0, dtype=cp.float32)  # Initial cash
    position_qty = cp.zeros((num_configs, num_bars), dtype=cp.float32)
    equity = cp.zeros((num_configs, num_bars), dtype=cp.float32)
    entry_bars = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_bars = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    # First bar: no fills possible, just mark-to-market
    equity[:, 0] = cash[:, 0]

    # Process bar by bar
    for t in range(1, num_bars):
        # Carry forward from previous bar
        cash[:, t] = cash[:, t - 1]
        position_qty[:, t] = position_qty[:, t - 1]
        position[:, t] = position[:, t - 1]

        # Check for fills from previous bar's signals
        prev_pos = position[:, t - 1]
        prev_entry = entry_signals[:, t - 1]
        prev_exit = exit_signals[:, t - 1]

        # Entry fills
        enter = (prev_pos == 0) & prev_entry
        if cp.any(enter):
            fill_price = open_prices[:, t]
            fill_cost = qty_per_trade * fill_price
            fill_fees = fill_cost * fee_mult

            # Update cash and position for entries
            cash[:, t] = cp.where(enter, cash[:, t] - fill_cost - fill_fees, cash[:, t])
            position_qty[:, t] = cp.where(enter, qty_per_trade, position_qty[:, t])
            position[:, t] = cp.where(enter, 1, position[:, t])
            entry_bars[:, t] = enter

        # Exit fills
        exit_ = (prev_pos == 1) & prev_exit
        if cp.any(exit_):
            fill_price = open_prices[:, t]
            fill_proceeds = position_qty[:, t] * fill_price
            fill_fees = fill_proceeds * fee_mult

            # Update cash and position for exits
            cash[:, t] = cp.where(exit_, cash[:, t] + fill_proceeds - fill_fees, cash[:, t])
            position_qty[:, t] = cp.where(exit_, 0, position_qty[:, t])
            position[:, t] = cp.where(exit_, 0, position[:, t])
            exit_bars[:, t] = exit_

        # Mark-to-market at close
        equity[:, t] = cash[:, t] + position_qty[:, t] * close_prices[:, t]

    return position, equity, entry_bars, exit_bars


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
    prev_equity = cp.where(prev_equity < 1e-10, 1e-10, prev_equity)

    returns = (equity[:, 1:] - equity[:, :-1]) / prev_equity
    return cp.nan_to_num(returns, nan=0.0, posinf=0.0, neginf=0.0)


def count_trades_gpu(
    entry_bars: cp.ndarray,
    exit_bars: cp.ndarray,
) -> cp.ndarray:
    """
    Count completed trades (entry + exit pairs).

    Args:
        entry_bars: Boolean array where entries occurred
        exit_bars: Boolean array where exits occurred

    Returns:
        Number of completed trades per config, shape [num_configs]
    """
    # A completed trade is an exit
    return cp.sum(exit_bars, axis=1)


def compute_trade_pnl_gpu(
    entry_bars: cp.ndarray,
    exit_bars: cp.ndarray,
    open_prices: cp.ndarray,
    fees_bps: float = 10.0,
    qty_per_trade: float = 100.0,
) -> tuple[cp.ndarray, cp.ndarray]:
    """
    Compute PnL for each trade.

    This is more complex on GPU due to variable number of trades per config.
    Returns gross profit and gross loss aggregates.

    Args:
        entry_bars: Boolean array where entries occurred
        exit_bars: Boolean array where exits occurred
        open_prices: Open prices, shape [num_bars]
        fees_bps: Fees in basis points per side
        qty_per_trade: Shares per trade

    Returns:
        Tuple of (gross_profit, gross_loss) per config
    """
    num_configs, num_bars = entry_bars.shape
    fee_mult = fees_bps / 10000.0

    # Broadcast prices
    if open_prices.ndim == 1:
        open_prices = cp.broadcast_to(open_prices, (num_configs, num_bars))

    # For each config, we need to match entries with exits
    # This is tricky to vectorize - use a simpler approach
    # Track running entry price and compute PnL on exit

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

    return gross_profit, gross_loss
