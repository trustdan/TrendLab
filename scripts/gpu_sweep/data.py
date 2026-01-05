"""
Data loading utilities for GPU Mega-Sweep.

Mirrors Rust parquet loading from crates/trendlab-core/src/data/parquet.rs.
Uses Polars for lazy evaluation and predicate pushdown.
"""

from datetime import date
from pathlib import Path

import polars as pl


def scan_symbol_parquet(
    base_dir: Path | str,
    symbol: str,
    timeframe: str = "1d",
    start_date: date | None = None,
    end_date: date | None = None,
) -> pl.LazyFrame:
    """
    Scan Parquet files for a single symbol.

    Mirrors Rust scan_symbol_parquet_lazy().

    Args:
        base_dir: Base directory containing parquet files (e.g., 'data/parquet')
        symbol: Ticker symbol (e.g., 'SPY')
        timeframe: Timeframe (default '1d')
        start_date: Optional start date filter
        end_date: Optional end date filter

    Returns:
        LazyFrame with columns: ts, open, high, low, close, volume, symbol, timeframe

    File structure expected:
        {base_dir}/{timeframe}/symbol={symbol}/year={YYYY}/data.parquet
    """
    base_dir = Path(base_dir)
    pattern = str(base_dir / timeframe / f"symbol={symbol}" / "year=*" / "*.parquet")

    try:
        lf = pl.scan_parquet(pattern)
    except Exception as e:
        raise FileNotFoundError(
            f"No parquet files found for {symbol} at {pattern}"
        ) from e

    # Apply date filters with predicate pushdown
    if start_date:
        lf = lf.filter(pl.col("ts") >= pl.lit(start_date).cast(pl.Date))
    if end_date:
        lf = lf.filter(pl.col("ts") <= pl.lit(end_date).cast(pl.Date))

    return lf.sort("ts")


def scan_multiple_symbols_parquet(
    base_dir: Path | str,
    symbols: list[str],
    timeframe: str = "1d",
    start_date: date | None = None,
    end_date: date | None = None,
) -> pl.LazyFrame:
    """
    Scan Parquet files for multiple symbols.

    Args:
        base_dir: Base directory containing parquet files
        symbols: List of ticker symbols
        timeframe: Timeframe (default '1d')
        start_date: Optional start date filter
        end_date: Optional end date filter

    Returns:
        LazyFrame with all symbols concatenated, sorted by (symbol, ts)
    """
    base_dir = Path(base_dir)
    frames = []

    for symbol in symbols:
        try:
            lf = scan_symbol_parquet(base_dir, symbol, timeframe, start_date, end_date)
            frames.append(lf)
        except FileNotFoundError:
            # Skip symbols with no data
            continue

    if not frames:
        raise FileNotFoundError(f"No parquet files found for any of {symbols}")

    return pl.concat(frames).sort(["symbol", "ts"])


def get_available_symbols(base_dir: Path | str, timeframe: str = "1d") -> list[str]:
    """
    Get list of available symbols in the data directory.

    Args:
        base_dir: Base directory containing parquet files
        timeframe: Timeframe to check

    Returns:
        List of symbol names
    """
    base_dir = Path(base_dir)
    timeframe_dir = base_dir / timeframe

    if not timeframe_dir.exists():
        return []

    symbols = []
    for path in timeframe_dir.iterdir():
        if path.is_dir() and path.name.startswith("symbol="):
            symbols.append(path.name.replace("symbol=", ""))

    return sorted(symbols)


def get_symbol_date_range(
    base_dir: Path | str,
    symbol: str,
    timeframe: str = "1d",
) -> tuple[date, date] | None:
    """
    Get the date range for a symbol's data.

    Args:
        base_dir: Base directory containing parquet files
        symbol: Ticker symbol
        timeframe: Timeframe

    Returns:
        Tuple of (min_date, max_date) or None if no data
    """
    try:
        lf = scan_symbol_parquet(base_dir, symbol, timeframe)
        result = lf.select([
            pl.col("ts").min().alias("min_date"),
            pl.col("ts").max().alias("max_date"),
        ]).collect()

        if result.height == 0:
            return None

        min_date = result["min_date"][0]
        max_date = result["max_date"][0]

        # Convert to date if datetime
        if hasattr(min_date, "date"):
            min_date = min_date.date()
        if hasattr(max_date, "date"):
            max_date = max_date.date()

        return (min_date, max_date)
    except FileNotFoundError:
        return None


def load_symbol_to_arrays(
    base_dir: Path | str,
    symbol: str,
    timeframe: str = "1d",
    start_date: date | None = None,
    end_date: date | None = None,
) -> dict[str, "cp.ndarray"]:
    """
    Load symbol data directly to cuPy arrays for GPU processing.

    Args:
        base_dir: Base directory containing parquet files
        symbol: Ticker symbol
        timeframe: Timeframe
        start_date: Optional start date filter
        end_date: Optional end date filter

    Returns:
        Dict with keys: 'open', 'high', 'low', 'close', 'volume'
        Each value is a cuPy array of shape [num_bars]
    """
    import cupy as cp

    lf = scan_symbol_parquet(base_dir, symbol, timeframe, start_date, end_date)
    df = lf.select(["open", "high", "low", "close", "volume"]).collect()

    return {
        "open": cp.asarray(df["open"].to_numpy(), dtype=cp.float32),
        "high": cp.asarray(df["high"].to_numpy(), dtype=cp.float32),
        "low": cp.asarray(df["low"].to_numpy(), dtype=cp.float32),
        "close": cp.asarray(df["close"].to_numpy(), dtype=cp.float32),
        "volume": cp.asarray(df["volume"].to_numpy(), dtype=cp.float32),
    }


def load_multiple_symbols_to_arrays(
    base_dir: Path | str,
    symbols: list[str],
    timeframe: str = "1d",
    start_date: date | None = None,
    end_date: date | None = None,
    pad_to_length: int | None = None,
) -> tuple[dict[str, "cp.ndarray"], list[int]]:
    """
    Load multiple symbols as stacked cuPy arrays.

    All symbols are padded to the same length for batched GPU operations.

    Args:
        base_dir: Base directory containing parquet files
        symbols: List of ticker symbols
        timeframe: Timeframe
        start_date: Optional start date filter
        end_date: Optional end date filter
        pad_to_length: Optional fixed length to pad to

    Returns:
        Tuple of:
        - Dict with keys: 'open', 'high', 'low', 'close', 'volume'
          Each value is a cuPy array of shape [num_symbols, max_bars]
        - List of actual lengths for each symbol (for masking)
    """
    import cupy as cp

    all_data = []
    lengths = []

    for symbol in symbols:
        try:
            data = load_symbol_to_arrays(
                base_dir, symbol, timeframe, start_date, end_date
            )
            all_data.append(data)
            lengths.append(len(data["close"]))
        except FileNotFoundError:
            continue

    if not all_data:
        raise FileNotFoundError(f"No data found for any of {symbols}")

    # Determine max length
    max_len = pad_to_length or max(lengths)

    # Stack with padding
    num_symbols = len(all_data)
    result = {
        "open": cp.zeros((num_symbols, max_len), dtype=cp.float32),
        "high": cp.zeros((num_symbols, max_len), dtype=cp.float32),
        "low": cp.zeros((num_symbols, max_len), dtype=cp.float32),
        "close": cp.zeros((num_symbols, max_len), dtype=cp.float32),
        "volume": cp.zeros((num_symbols, max_len), dtype=cp.float32),
    }

    for i, (data, length) in enumerate(zip(all_data, lengths)):
        for key in result:
            result[key][i, :length] = data[key]

    return result, lengths
