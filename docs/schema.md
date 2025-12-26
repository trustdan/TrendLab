# TrendLab Data Schema

This document defines the canonical data schemas used throughout TrendLab.

---

## Bar Schema (OHLCV)

The fundamental unit of market data is a bar (candlestick).

| Field | Type | Units | Description |
|-------|------|-------|-------------|
| `ts` | i64 | milliseconds since Unix epoch (UTC) | Bar timestamp (start of period) |
| `open` | f64 | price units | Opening price |
| `high` | f64 | price units | Highest price during period |
| `low` | f64 | price units | Lowest price during period |
| `close` | f64 | price units | Closing price (adjusted) |
| `volume` | f64 | shares | Trading volume |
| `symbol` | String | — | Ticker symbol (e.g., "SPY") |
| `timeframe` | String | — | Bar duration (e.g., "1d", "1h") |

**Notes:**
- `close` is always split-adjusted and dividend-adjusted
- `volume` is raw (not adjusted for splits)
- `ts` represents the *start* of the bar period

---

## Parquet Storage Layout

```
data/parquet/{timeframe}/symbol={SYMBOL}/year={YYYY}/*.parquet
```

**Example:**
```
data/parquet/1d/symbol=SPY/year=2023/data.parquet
data/parquet/1d/symbol=SPY/year=2024/data.parquet
data/parquet/1d/symbol=AAPL/year=2023/data.parquet
```

**Partitioning rationale:**
- Partition by `symbol` for efficient single-symbol queries
- Partition by `year` for efficient date range pruning
- Timeframe in path allows multiple timeframes without schema collision

---

## Raw Cache Layout

```
data/raw/{provider}/{symbol}/{start_date}_{end_date}.json
```

**Example:**
```
data/raw/yahoo/SPY/2020-01-01_2024-12-31.json
```

**Metadata sidecar:**
```
data/raw/yahoo/SPY/2020-01-01_2024-12-31.meta.json
```

Contains:
- `fetched_at`: ISO timestamp of fetch
- `provider_version`: API version used
- `row_count`: Number of bars
- `checksum`: SHA256 of data file

---

## Trade Schema

| Field | Type | Description |
|-------|------|-------------|
| `trade_id` | u64 | Unique trade identifier |
| `symbol` | String | Ticker symbol |
| `entry_ts` | i64 | Entry bar timestamp |
| `entry_price` | f64 | Execution price at entry |
| `exit_ts` | i64 | Exit bar timestamp |
| `exit_price` | f64 | Execution price at exit |
| `quantity` | f64 | Position size (shares) |
| `direction` | String | "long" or "short" |
| `pnl` | f64 | Profit/loss (after costs) |
| `pnl_pct` | f64 | Percent return |

---

## Equity Curve Schema

| Field | Type | Description |
|-------|------|-------------|
| `ts` | i64 | Bar timestamp |
| `equity` | f64 | Account equity (NAV) |
| `drawdown` | f64 | Current drawdown from peak |
| `drawdown_pct` | f64 | Drawdown as percentage |

---

## Metrics Schema

| Field | Type | Description |
|-------|------|-------------|
| `config_id` | String | Configuration identifier |
| `cagr` | f64 | Compound annual growth rate |
| `sharpe` | f64 | Sharpe ratio (annualized) |
| `sortino` | f64 | Sortino ratio (annualized) |
| `max_drawdown` | f64 | Maximum drawdown (percentage) |
| `calmar` | f64 | CAGR / Max Drawdown |
| `win_rate` | f64 | Winning trades / Total trades |
| `profit_factor` | f64 | Gross profit / Gross loss |
| `num_trades` | u32 | Total number of trades |
| `turnover` | f64 | Annual turnover (as multiple of capital) |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2024-12-26 | Initial schema document |
