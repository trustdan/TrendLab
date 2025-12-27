# TrendLab

```text
  _____ ____  _____ _   _ ____  _        _    ____
 |_   _|  _ \| ____| \ | |  _ \| |      / \  | __ )
   | | | |_) |  _| |  \| | | | | |     / _ \ |  _ \
   | | |  _ <| |___| |\  | |_| | |___ / ___ \| |_) |
   |_| |_| \_\_____|_| \_|____/|_____/_/   \_\____/
                                                ╱╲
            Rust • Polars • BDD           ╱╲___╱  ╲___
                                         ╱
```

[![CI](https://github.com/trustdan/TrendLab/actions/workflows/ci.yml/badge.svg)](https://github.com/trustdan/TrendLab/actions/workflows/ci.yml)

TrendLab is a **research-grade trend-following backtesting lab** (not a live-trading engine).
It exists to help you answer, repeatedly and defensibly:

> Which trend-following approach works, under what conditions, and how do I know I’m not fooling myself?

The project optimizes for:

- **Correctness and reproducibility** (invariants + deterministic outputs)
- **Fast experimentation** (sweep-first workflows across strategy families)
- **Verifiable results** via **StrategyArtifact → Pine parity** (TradingView as an external reference implementation)

## Quick Links

[Quick Start](#quick-start) | [Zero to Backtest](#zero-to-first-backtest-5-minutes) | [TUI Guide](#tui-features) | [Desktop GUI](#desktop-gui) | [Strategies](#strategies) | [CLI Commands](#cli-commands) | [Contributing](#contributing) | [Project Map](#project-map)

## Ethos (what we care about)

- **Correctness over cleverness**: no “probably right” backtests.
- **BDD-first**: a `.feature` file is the contract; implementation follows.
- **Explicit assumptions**: fill model, costs, data adjustments, missing bars policy are spelled out and tested.
- **Parity is correctness, not polish**: Pine parity is part of validation, not a “nice to have.”

## How it works (high level)

TrendLab's "happy path" is:

1. **Ingest & cache market data** ✅ Yahoo daily OHLCV → Parquet cache
2. **Compute indicators and signals** ✅ Strict time alignment; no lookahead
3. **Simulate fills and accounting** ✅ Signal-on-close → fill-next-open; explicit fees/slippage
4. **Run sweeps** ✅ Parallel parameter sweeps across universes via Polars + Rayon
5. **Rank + report** ✅ Full metrics suite with HTML/CSV export
6. **Export StrategyArtifact** ✅ JSON schema for Pine generation
7. **Verify in TradingView** via Pine parity vectors (in progress)

## Prerequisites

### System Requirements

| Requirement       | Minimum                       | Recommended              |
| ----------------- | ----------------------------- | ------------------------ |
| **Rust**          | 1.75+ (stable)                | Latest stable            |
| **Platform**      | Windows 10, macOS 12+, Linux  | Any modern OS            |
| **Memory**        | 4 GB                          | 8+ GB (for large sweeps) |
| **Terminal Size** | 80×24                         | 120×40+ (for TUI)        |

### Dependencies

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version  # Should be 1.75 or higher
cargo --version
```

### Optional Dependencies

- **Git**: For version control and cloning the repository
- **Internet**: Required for Yahoo Finance data fetching (not needed for cached data)

### TUI Requirements

The TUI works best in terminals that support:

- 256 colors or true color
- Unicode characters (for chart rendering)
- Mouse input (optional, for interactive charts)

**Recommended terminals**: Windows Terminal, iTerm2, Alacritty, Kitty, WezTerm, Warp

## Architecture (workspace crates)

- **`trendlab-core`**: pure domain logic (bars, indicators, strategies, backtest kernel, metrics). **No IO.**
- **`trendlab-cli`**: orchestration + IO (data refresh, sweep runs, report export, artifact export).
- **`trendlab-bdd`**: cucumber runner, step definitions, and fixtures that lock invariants early.

For more detail, see `docs/PROJECT_OVERVIEW.md` and `docs/architecture.md`.

## Current status (what's real today)

**Milestone 0 ("Foundation")** ✅ Complete:

- Cucumber BDD suite wired and running under `cargo test`
- Deterministic fixtures in `fixtures/synth/`
- Minimal vertical slice: SMA indicator, backtest kernel with next-open fill + fees/slippage

**Milestone 1 ("Data Layer")** ✅ Complete:

- Yahoo Finance data provider with automatic caching
- Parquet storage for normalized bar data
- Data quality validation and missing bar handling

**Milestone 2 ("Polars Integration")** ✅ Complete:

- Dual-backend architecture: Polars-native (vectorized) and sequential
- 3 strategy families fully ported to Polars: Donchian/Turtle, MA Crossover, TSMOM
- Unified sweep runner with parallel execution via Rayon
- Full TUI integration with Polars-native sweeps

**Milestone 3 ("Short Selling")** ✅ Complete:

- Full short selling support with position states: -1 (Short), 0 (Flat), 1 (Long)
- Three trading modes: `LongOnly`, `ShortOnly`, `LongShort`
- Short entry on breakdown (close < channel lower), exit on breakout (close > channel upper)
- Correct short P&L calculation (profit when exit price < entry price)
- Negative position quantities for short positions
- All StrategyV2 implementations support trading mode selection

**Milestone 4 ("Phase 4 Strategies")** ✅ Complete:

- **Parabolic SAR Strategy**: Wilder's stop-and-reverse with AF/EP tracking
- **Opening Range Breakout Strategy**: Weekly/Monthly/Rolling period detection
- **Ensemble Strategy**: Multi-horizon voting with three methods (Majority, WeightedByHorizon, UnanimousEntry)
- New indicators: `parabolic_sar()`, `opening_range()`
- Extended sweep infrastructure with depth-aware grid configs
- Full TUI integration with parameter editing support
- BDD test coverage with 3 feature files and synthetic fixtures

See [Backtest Architecture](#backtest-architecture) below for details on the dual-backend design.

**Milestone 5 ("V2 Strategy Expansion")** 🚧 In Progress:

- **10 of 15 V2 strategies implemented** with Polars-native backends
- Phase 1 (ATR-Based): Keltner ✅, STARC (pending), Supertrend (pending)
- Phase 2 (Momentum/Direction): DMI/ADX ✅, Aroon ✅, Heikin-Ashi ✅
- Phases 3-5: 5 strategies pending (Darvas Box, 52-Week High, Larry Williams, Bollinger Squeeze, Short Selling enhancements)
- 124 library tests, 241 BDD scenarios (210 passed, 31 skipped)

See [Strategy Roadmap](#strategy-roadmap) and [docs/roadmap-v2-strategies.md](docs/roadmap-v2-strategies.md) for details.

**Milestone 6 ("Desktop GUI")** 🚧 In Progress:

- Tauri v2 desktop application with React + TypeScript frontend
- TradingView Lightweight Charts for professional candlestick visualization
- Same 5-panel workflow as TUI with polished web aesthetics
- Full keyboard navigation matching TUI shortcuts
- Phases 1-7 substantially complete (Foundation through Polish)

See [Desktop GUI](#desktop-gui) and [docs/roadmap-tauri-gui.md](docs/roadmap-tauri-gui.md) for details.

## Quick start

```bash
# Build
cargo build

# Run tests (includes BDD)
cargo test

# CLI help
cargo run -p trendlab-cli -- --help

# TUI
cargo run -p trendlab-tui --bin trendlab-tui
```

## Zero to First Backtest (5 minutes)

A complete walkthrough from clone to seeing your first backtest results:

### Step 1: Clone and Build

```bash
git clone https://github.com/trustdan/TrendLab.git
cd TrendLab
cargo build --release
```

### Step 2: Fetch Market Data

```bash
# Fetch 5 years of SPY data
cargo run -p trendlab-cli -- data refresh-yahoo --tickers SPY --start 2020-01-01 --end 2024-12-31

# Verify data was cached
cargo run -p trendlab-cli -- data status --ticker SPY
```

### Step 3: Run Your First Sweep

```bash
# Run a Donchian breakout parameter sweep
cargo run -p trendlab-cli -- sweep --strategy donchian --ticker SPY \
    --start 2020-01-01 --end 2024-12-31 --top-n 5
```

### Step 4: Explore with TUI (Optional)

```bash
# Launch the interactive terminal UI
cargo run -p trendlab-tui --bin trendlab-tui

# Choose "Full-Auto" mode at startup for guided exploration
```

### Step 5: Export Results

```bash
# Generate HTML report
cargo run -p trendlab-cli -- report html --run-id sweep_001 --open

# Export to CSV for further analysis
cargo run -p trendlab-cli -- report export --run-id sweep_001 --output results.csv
```

**What you just did:**

1. Downloaded 5 years of daily OHLCV data from Yahoo Finance
2. Ran 100+ parameter combinations of the Donchian breakout strategy
3. Ranked results by Sharpe ratio and other metrics
4. Generated a report you can share or analyze further

## TUI Features

The TUI provides an interactive terminal interface for backtesting exploration.

### Panels

| Panel | Hotkey | Description |
|-------|--------|-------------|
| Data | `1` | Browse universe sectors, select tickers, fetch market data |
| Strategy | `2` | Choose strategy type and configure parameters |
| Sweep | `3` | Run parameter sweeps across selected tickers |
| Results | `4` | View sweep results ranked by metrics |
| Chart | `5` | Visualize equity curves with legends |

### Full-Auto Mode

On startup, choose between **Manual** and **Full-Auto** modes:

- **Manual**: Use panels to pick tickers, configure strategy, then run sweeps
- **Full-Auto**: Automatically selects all universe tickers, runs sweeps, and displays results

### Sweep Depth Presets

Full-Auto mode includes research-backed parameter sweep depths:

| Depth | Configs | Description |
|-------|---------|-------------|
| **Quick** | ~11 | Core classics only (Turtle 20/55, Golden Cross 50/200) |
| **Standard** | ~40 | Balanced coverage - good for initial exploration |
| **Comprehensive** | ~88 | Extended ranges + fine granularity for thorough analysis |

Parameter ranges are based on:
- **Donchian/Turtle**: Classic Turtle Trading rules (20/10 for S1, 55/20 for S2)
- **MA Crossover**: Golden Cross variants (50/200 SMA, with EMA options)
- **TSMOM**: Academic momentum research (63, 126, 252 day lookbacks)

### Startup Modal Controls

| Key | Action |
|-----|--------|
| `Left`/`Right` | Switch between Manual and Full-Auto modes |
| `Up`/`Down` | Select strategy (in Full-Auto mode) |
| `[`/`]` | Cycle sweep depth (Quick/Standard/Comprehensive) |
| `Enter` | Start with selected options |
| `Esc` | Dismiss modal (enters Manual mode) |

### Chart Views

The Chart panel supports multiple view modes (press `m` to cycle):

- **Single**: Individual equity curve
- **Candlestick**: OHLC candlestick chart with wicks and bodies
- **Multi-Ticker**: Overlay best configs per ticker
- **Portfolio**: Aggregated portfolio equity
- **Strategy Comparison**: Compare strategies with color-coded legend
- **Per-Ticker Best**: Each ticker's best strategy result

### Chart Enhancements

The chart panel includes professional-grade features:

| Feature | Toggle | Description |
|---------|--------|-------------|
| **Volume Bars** | `v` | Subplot showing volume with muted green/red coloring |
| **Crosshair** | `c` | Vertical and horizontal lines tracking cursor position |
| **Tooltips** | (auto) | Hover over chart to see data point details |
| **Grid Lines** | (always) | Subtle grid for easier value reading |
| **Smooth Zoom** | `↑/↓` or scroll | Animated zoom with ease-out interpolation |

### Chart Controls

| Key | Action |
|-----|--------|
| `m` | Cycle view mode |
| `v` | Toggle volume subplot |
| `c` | Toggle crosshair |
| `d` | Toggle drawdown overlay |
| `←/→` | Scroll chart left/right |
| `↑/↓` | Zoom in/out |
| Mouse scroll | Zoom in/out |
| Mouse move | Update crosshair + tooltip |

### Master Keyboard Shortcuts

Complete reference of all keyboard shortcuts in the TUI:

#### Global Navigation

| Key | Action |
|-----|--------|
| `1`-`5` | Jump to panel (Data, Strategy, Sweep, Results, Chart) |
| `Tab` | Next panel |
| `Shift+Tab` | Previous panel |
| `q` / `Ctrl+C` | Quit application |
| `Esc` | Cancel current operation / dismiss modal |

#### Data Panel (`1`)

| Key | Action |
|-----|--------|
| `↑/↓` | Navigate sectors or tickers |
| `←/→` | Switch between Sectors and Tickers views |
| `Enter` | Expand sector / Load ticker data |
| `Space` | Toggle ticker selection (for multi-ticker sweeps) |
| `a` | Select all tickers in current sector |
| `n` | Deselect all tickers |

#### Strategy Panel (`2`)

| Key | Action |
|-----|--------|
| `↑/↓` | Navigate strategy categories/types (left) or parameters (right) |
| `←/→` | Adjust parameter values / Switch focus between selection and parameters |
| `Enter` | Expand/collapse category or toggle strategy selection |
| `Space` | Toggle strategy checkbox |
| `Tab` | Switch between Selection (left) and Parameters (right) panels |

#### Sweep Panel (`3`)

| Key | Action |
|-----|--------|
| `Enter` | Start parameter sweep |
| `Esc` | Cancel running sweep |
| `↑/↓` | Navigate sweep parameters |

#### Results Panel (`4`)

| Key | Action |
|-----|--------|
| `↑/↓` | Navigate results list |
| `Enter` | View selected result in Chart panel |
| `a` | Toggle statistical analysis view (VaR, MAE/MFE, regime analysis) |
| `v` | Cycle results view mode (Single/Multi-Ticker/Best-Per-Ticker) |
| `s` | Sort by next metric |
| `r` | Reverse sort order |

#### Chart Panel (`5`)

| Key | Action |
|-----|--------|
| `m` | Cycle view mode (Single → Candlestick → Multi-Ticker → Portfolio → Strategy Comparison) |
| `v` | Toggle volume subplot |
| `c` | Toggle crosshair cursor |
| `d` | Toggle drawdown overlay |
| `←/→` | Scroll chart left/right |
| `↑/↓` | Zoom in/out |
| Mouse scroll | Zoom in/out |
| Mouse hover | Show tooltip with data point details |

### Configuring Strategies

The Strategy panel (`2`) allows you to select and configure strategy parameters.

#### Strategy Selection (Left Panel)

Strategies are organized into categories:

| Category | Strategies |
|----------|------------|
| **Channel Breakouts** | Donchian |
| **Momentum/Direction** | MA Crossover, TSMOM |
| **Classic Presets** | Turtle S1, Turtle S2 |
| **Complex/SAR** | Parabolic SAR, Opening Range Breakout |
| **Ensemble** | Multi-Horizon Ensemble |

- Use `↑/↓` to navigate categories and strategies
- Press `Enter` or `Space` to toggle selection
- Multiple strategies can be selected for comparison sweeps

#### Parameter Editing (Right Panel)

Press `Tab` or `→` to switch to parameter editing:

**Donchian Breakout:**
| Parameter | Range | Description |
|-----------|-------|-------------|
| Entry Lookback | 5-100 | Days for highest high (entry signal) |
| Exit Lookback | 5-100 | Days for lowest low (exit signal) |

**MA Crossover:**
| Parameter | Range | Description |
|-----------|-------|-------------|
| Fast Period | 5-100 | Short-term moving average period |
| Slow Period | 20-300 | Long-term moving average period |
| MA Type | SMA/EMA | Simple or Exponential moving average |

**TSMOM:**
| Parameter | Range | Description |
|-----------|-------|-------------|
| Lookback | 21-252 | Momentum calculation period (days) |

**Turtle S1/S2:** Fixed parameters (20/10 and 55/20 respectively)

**Parabolic SAR:**
| Parameter | Range | Description |
|-----------|-------|-------------|
| AF Start | 0.01-0.05 | Initial acceleration factor |
| AF Step | 0.01-0.05 | AF increment on new extreme |
| AF Max | 0.1-0.3 | Maximum acceleration factor |

**Opening Range Breakout:**
| Parameter | Range | Description |
|-----------|-------|-------------|
| Range Bars | 1-10 | Number of bars to establish range |
| Period Type | Weekly/Monthly/Rolling | Range reset period |

**Ensemble:**
| Parameter | Options | Description |
|-----------|---------|-------------|
| Base Strategy | Donchian/MA/TSMOM | Strategy family to use |
| Horizons | List of periods | Lookback periods to combine |
| Voting Method | Majority/Weighted/Unanimous | How signals are aggregated |

#### Workflow Example

1. Press `2` to open Strategy panel
2. Navigate to desired strategy with `↑/↓`
3. Press `Space` to select it
4. Press `Tab` to switch to parameters
5. Use `↑/↓` to select parameter, `←/→` to adjust
6. Press `Tab` to return to selection (or proceed to Sweep panel)

## Desktop GUI

A polished desktop GUI built with **Tauri v2** + **React** + **TypeScript**, featuring **TradingView Lightweight Charts** for professional financial visualization.

### Why Tauri?

| Feature            | Benefit                                              |
| ------------------ | ---------------------------------------------------- |
| **Rust backend**   | Reuses existing trendlab-core with zero FFI overhead |
| **Web frontend**   | Modern React ecosystem with professional charting    |
| **Small bundle**   | ~5MB binaries using system WebView                   |
| **Cross-platform** | Windows, macOS, Linux from single codebase           |

### GUI Features (Planned)

The GUI mirrors the TUI experience with enhanced visuals:

| Panel        | TUI                           | GUI Enhancement                    |
| ------------ | ----------------------------- | ---------------------------------- |
| **Data**     | Sector list, ticker selection | Autocomplete search, drag-select   |
| **Strategy** | Category accordion, params    | Interactive parameter sliders      |
| **Sweep**    | Progress bar                  | Real-time streaming progress       |
| **Results**  | ASCII table                   | Sortable data grid with filtering  |
| **Chart**    | Unicode candlesticks          | TradingView professional charts    |

### Chart Capabilities

Using TradingView Lightweight Charts:

- **Candlestick charts** with proper wicks, bodies, colors
- **Indicator overlays** (MA, Bollinger, channels)
- **Volume subplot** with color coding
- **Trade markers** showing entry/exit points
- **Crosshair** with OHLCV tooltips
- **Zoom/pan** with smooth animations
- **Multi-ticker overlay** for comparison
- **Equity curve** and drawdown visualization

### Architecture

```text
┌─────────────────────────────────────────────────────────┐
│                    Tauri Desktop App                    │
├─────────────────────────────────────────────────────────┤
│  React Frontend (TypeScript)                            │
│  ├── Zustand state management                           │
│  ├── TradingView Lightweight Charts                     │
│  └── Tokyo Night theme                                  │
├─────────────────────────────────────────────────────────┤
│  Tauri Commands (IPC Bridge)                            │
│  ├── invoke() for request/response                      │
│  └── events for streaming (sweep progress)              │
├─────────────────────────────────────────────────────────┤
│  Rust Backend                                           │
│  ├── trendlab-core (domain logic, strategies, metrics)  │
│  ├── AppState (RwLock-wrapped shared state)             │
│  └── Worker thread (async operations)                   │
└─────────────────────────────────────────────────────────┘
```

### Roadmap

See [docs/roadmap-tauri-gui.md](docs/roadmap-tauri-gui.md) for the detailed implementation plan with checkboxes:

- **Phase 1**: Foundation (crate setup, React scaffolding, navigation)
- **Phase 2**: Data Panel + BDD tests
- **Phase 3**: Strategy Panel + BDD tests
- **Phase 4**: Sweep Panel + BDD tests
- **Phase 5**: Results Panel + BDD tests
- **Phase 6**: Chart Panel + TradingView integration
- **Phase 7**: Polish (keyboard nav, accessibility, performance)

## Backtest Architecture

TrendLab uses a **dual-backend architecture** for backtesting:

### Polars Backend (Default)

The Polars backend uses vectorized DataFrame operations for high performance:

- **Vectorized indicators**: Rolling windows, exponential moving averages computed column-wise
- **Parallel sweeps**: Parameter combinations processed in parallel via Rayon
- **Memory efficient**: Lazy evaluation with predicate pushdown
- **StrategyV2 trait**: Polars-native strategy implementations

```rust
// StrategyV2 trait for Polars-native strategies
pub trait StrategyV2: Send + Sync {
    fn name(&self) -> &str;
    fn compute_signals(&self, df: &DataFrame) -> Result<DataFrame>;
    fn trading_mode(&self) -> TradingMode;  // LongOnly, ShortOnly, or LongShort
}
```

### Trading Modes

All StrategyV2 implementations support three trading modes:

| Mode | Position States | Entry Signal | Exit Signal |
|------|-----------------|--------------|-------------|
| `LongOnly` | 0 (Flat), 1 (Long) | Breakout above channel | Breakdown below exit channel |
| `ShortOnly` | 0 (Flat), -1 (Short) | Breakdown below channel | Breakout above exit channel |
| `LongShort` | -1, 0, 1 | Both directions | Opposite signal or exit channel |

**Short position P&L**: Profit = (entry_price - exit_price) × shares (profit when exit < entry)

Implemented strategies:

| Strategy | Description | Key Parameters |
|----------|-------------|----------------|
| `DonchianBreakoutV2` | Channel breakout (Turtle-style) | entry_lookback, exit_lookback |
| `MACrossoverV2` | Moving average crossover | fast_period, slow_period, ma_type |
| `TsmomV2` | Time-series momentum | lookback_days |
| `ParabolicSARStrategy` | Wilder's SAR with AF/EP tracking | af_start, af_step, af_max |
| `OpeningRangeBreakoutStrategy` | Period range breakout | range_bars, period_type |
| `EnsembleStrategy` | Multi-horizon voting | base_strategy, horizons, voting_method |
| `KeltnerV2` | EMA ± ATR channel breakout | ema_period, atr_period, multiplier |
| `DmiAdxV2` | Directional movement + trend strength | di_period, adx_threshold |
| `AroonV2` | High/low recency crossover | period |
| `HeikinAshiV2` | Smoothed candle regime detection | confirmation_bars |

### Sequential Backend (Fallback)

The sequential backend processes bars one-at-a-time for debugging and validation:

- **Bar-by-bar simulation**: Explicit order of operations
- **Easier debugging**: Step through each bar's logic
- **Reference implementation**: Used to validate Polars results

Use `--sequential` flag with CLI commands to use this backend.

### CLI Usage

```bash
# Run sweep with Polars (default, faster)
cargo run -p trendlab-cli -- sweep --strategy donchian --universe SPY,QQQ

# Run sweep with sequential backend (for debugging)
cargo run -p trendlab-cli -- sweep --strategy donchian --universe SPY,QQQ --sequential
```

### Performance Characteristics

| Backend | Sweep Speed | Memory | Use Case |
|---------|-------------|--------|----------|
| Polars | ~10-50x faster | Lower (lazy eval) | Production sweeps |
| Sequential | Baseline | Higher | Debugging, validation |

## Strategies

TrendLab implements ten trend-following strategy families:

### Donchian Breakout / Turtle System

Channel breakout strategy based on the Turtle Trading rules:

- **Long Entry**: Price breaks above N-day high (upper channel)
- **Long Exit**: Price breaks below M-day low (lower exit channel)
- **Short Entry**: Price breaks below N-day low (lower channel)
- **Short Exit**: Price breaks above M-day high (upper exit channel)
- **Presets**: Turtle S1 (20/10), Turtle S2 (55/20)
- **Trading Modes**: LongOnly, ShortOnly, LongShort

### MA Crossover

Moving average crossover with configurable MA types:

- **Long Entry**: Fast MA crosses above slow MA
- **Long Exit**: Fast MA crosses below slow MA
- **Short Entry**: Fast MA crosses below slow MA
- **Short Exit**: Fast MA crosses above slow MA
- **MA Types**: SMA (Simple), EMA (Exponential)
- **Presets**: Golden Cross (50/200 SMA)
- **Trading Modes**: LongOnly, ShortOnly, LongShort

### TSMOM (Time-Series Momentum)

Academic momentum strategy based on Moskowitz et al. research:

- **Long Signal**: Return over lookback period is positive
- **Short Signal**: Return over lookback period is negative
- **Lookbacks**: 63 (quarterly), 126 (semi-annual), 252 (annual)
- **Trading Modes**: LongOnly, ShortOnly, LongShort

### Parabolic SAR

Wilder's stop-and-reverse trailing stop system:

- **Entry**: SAR flips from above to below price (bullish) or below to above (bearish)
- **Exit**: SAR flips in opposite direction
- **Acceleration Factor**: Starts at 0.02, increments by 0.02, max 0.20 (configurable)
- **Extreme Point**: Tracks highest high (uptrend) or lowest low (downtrend)

### Opening Range Breakout

Breakout strategy based on the opening range of a period:

- **Entry**: Price breaks above/below the high/low of the opening range
- **Period Types**: Weekly (first N bars of week), Monthly, or Rolling window
- **Range Bars**: Number of bars to establish the opening range (default: 5)

### Multi-Horizon Ensemble

Combines signals from multiple lookback periods using voting:

- **Voting Methods**: Majority (>50%), Weighted by Horizon, Unanimous Entry
- **Base Strategies**: Works with Donchian, MA Crossover, or TSMOM
- **Presets**: Donchian Triple (10/20/55), MA Triple, TSMOM Multi (21/63/126/252)

### Keltner Channel

ATR-based volatility channel breakout:

- **Entry**: Price breaks above upper band (EMA + ATR × multiplier)
- **Exit**: Price falls below lower band (EMA - ATR × multiplier)
- **Parameters**: ema_period (20), atr_period (10), multiplier (2.0)

### DMI/ADX

Directional Movement Index with trend strength filter:

- **Entry**: +DI crosses above -DI AND ADX > threshold (trending market)
- **Exit**: +DI crosses below -DI
- **Parameters**: di_period (14), adx_threshold (25)

### Aroon

High/low recency oscillator for trend detection:

- **Entry**: Aroon-Up crosses above Aroon-Down
- **Exit**: Aroon-Up crosses below Aroon-Down
- **Calculation**: 100 × (period - bars_since_high) / period
- **Parameters**: period (25)

### Heikin-Ashi

Smoothed candlestick regime detection:

- **Entry**: First bullish HA candle after N consecutive bearish candles
- **Exit**: First bearish HA candle after bullish regime
- **HA Transform**: Smoothed OHLC using EWM (alpha=0.5) for open
- **Parameters**: confirmation_bars (3)

## Strategy Roadmap

Upcoming strategies organized in implementation phases:

### Phase 1: ATR-Based Channels (Partial ✅)

| Strategy           | Description                                | Key Parameters                      | Status |
| ------------------ | ------------------------------------------ | ----------------------------------- | ------ |
| **Keltner Channel**| Breakout on EMA ± ATR bands                | ema_period, atr_period, multiplier  | ✅ |
| **STARC Bands**    | Breakout on SMA ± ATR bands                | sma_period, atr_period, multiplier  | Pending |
| **Supertrend**     | ATR-based trailing stop with regime flips  | atr_period, multiplier              | Pending |

### Phase 2: Momentum & Direction ✅ Complete

| Strategy              | Description                               | Key Parameters                          | Status |
| --------------------- | ----------------------------------------- | --------------------------------------- | ------ |
| **DMI/ADX**           | Directional movement with trend strength  | di_period, adx_threshold                | ✅ |
| **Aroon**             | High/low recency oscillator crossover     | period                                  | ✅ |
| **Heikin-Ashi**       | Smoothed candle regime detection          | confirmation_bars                       | ✅ |

**Phase 2 Indicators Implemented:**
- +DM/-DM (directional movement), +DI/-DI (directional indicators), ADX
- Aroon Up/Down with bars-since-high/low calculation
- Heikin-Ashi OHLC transform with EWM smoothing

### Phase 3: Price Structure (Pending)

| Strategy           | Description                                 | Key Parameters              | Status |
| ------------------ | ------------------------------------------- | --------------------------- | ------ |
| **52-Week High**   | Proximity to annual high (George & Hwang)   | period, entry_pct, exit_pct | Pending |
| **Darvas Box**     | Classic box breakout (Nicolas Darvas)       | confirmation_bars           | Pending |
| **Larry Williams** | Range expansion volatility breakout         | range_mult, atr_stop_mult   | Pending |
| **Bollinger Squeeze** | Volatility contraction breakout          | period, std_mult, squeeze_threshold | Pending |

New indicators needed: Rolling max/min, Darvas box detection, Bollinger bandwidth

### Phase 4: Complex & Ensemble ✅ Complete

| Strategy                  | Description                                  | Key Parameters                  | Status |
| ------------------------- | -------------------------------------------- | ------------------------------- | ------ |
| **Parabolic SAR**         | Wilder's stop-and-reverse trailing stop      | af_start, af_step, af_max       | ✅ |
| **Opening Range Breakout**| Weekly/monthly range breakout adaptation     | range_bars, period              | ✅ |
| **Multi-Horizon Ensemble**| Vote across multiple lookback periods        | base_strategy, horizons, voting | ✅ |

**Ensemble Voting Methods:**

- **Majority**: >50% agreement triggers signal
- **Weighted by Horizon**: Longer horizons weighted more heavily
- **Unanimous Entry**: All must agree to enter; any can trigger exit

**Built-in Presets:**

- `Donchian Triple` (10/20/55 lookbacks)
- `MA Triple` (5-20/10-50/20-100 fast-slow pairs)
- `TSMOM Multi` (21/63/126/252 day momentum)

## Indicators

All indicators are pure functions with no lookahead:

| Indicator | Description | Module |
|-----------|-------------|--------|
| Donchian Channel | Highest high / lowest low over N bars | `indicators.rs` |
| SMA | Simple moving average | `indicators.rs` |
| EMA | Exponential moving average | `indicators.rs` |
| ATR | Average true range (standard) | `indicators.rs` |
| ATR Wilder | Wilder's smoothed ATR | `indicators.rs` |
| Parabolic SAR | Wilder's stop-and-reverse with AF/EP | `indicators.rs` |
| Opening Range | Weekly/Monthly/Rolling period detection | `indicators.rs` |
| Keltner Channel | EMA ± ATR bands | `indicators_polars.rs` |
| DMI/ADX | +DI, -DI, ADX directional movement | `indicators_polars.rs` |
| Aroon Up/Down | Bars since high/low oscillator | `indicators_polars.rs` |
| Heikin-Ashi | Smoothed OHLC candle transform | `indicators_polars.rs` |

## Metrics

Full performance metrics computed for every backtest:

| Metric | Description |
|--------|-------------|
| Total Return | End equity / initial equity - 1 |
| CAGR | Compound annual growth rate |
| Sharpe | Annualized risk-adjusted return (252 days) |
| Sortino | Downside-only Sharpe variant |
| Max Drawdown | Largest peak-to-trough decline |
| Calmar | CAGR / Max Drawdown |
| Win Rate | Winning trades / total trades |
| Profit Factor | Gross profit / gross loss |
| Turnover | Annual trading volume as multiple of capital |

## Statistical Analysis

Press `a` in the Results panel to compute and view detailed statistical analysis for the selected backtest configuration. Analysis is computed asynchronously and cached for fast switching between configs.

### Return Distribution

Risk metrics from the daily return distribution:

| Metric | Description |
|--------|-------------|
| VaR 95% / 99% | Value at Risk - worst expected daily loss at 95th/99th percentile |
| CVaR 95% / 99% | Conditional VaR (Expected Shortfall) - average loss beyond VaR |
| Skewness | Return asymmetry (negative = fat left tail, worse than Sharpe implies) |
| Kurtosis | Tail fatness (high = more extreme moves than normal distribution) |
| Daily Mean/Std | Mean and standard deviation of daily returns |
| Min/Max | Best and worst single-day returns |

### Trade Analysis (Swing Trading Focus)

Trade-level statistics optimized for 2-10 week holding periods:

| Metric | Description |
|--------|-------------|
| MAE (Max Adverse Excursion) | Worst drawdown during each trade - critical for stop placement |
| MFE (Max Favorable Excursion) | Best unrealized gain during each trade |
| Edge Ratio | MFE/MAE - quality of trade execution (>1.0 = favorable) |
| Holding Period | Mean, median, and histogram of trade durations |

Holding period buckets: 1-5 days, 6-10 days, 11-20 days, 21-50 days, 50+ days

### Regime Analysis

Performance breakdown by volatility regime (based on ATR):

| Regime   | Classification         |
|----------|------------------------|
| High Vol | ATR > 1.5× median ATR  |
| Neutral  | Normal volatility      |
| Low Vol  | ATR < 0.75× median ATR |

For each regime, shows:

- Percentage of trading days in that regime
- Number of trades entered
- Win rate and average return per trade
- Sharpe ratio during that regime

This helps identify when the strategy works vs fails (e.g., trend-following often struggles in low-vol, range-bound markets).

## Position Sizing

TrendLab supports multiple position sizing approaches:

| Sizer | Description |
|-------|-------------|
| Fixed | Constant number of units per trade |
| Volatility | Units = risk budget / (ATR × price) — Turtle-style |

**Pyramiding** is also supported: add to winning positions up to a configurable maximum.

## Universe Configuration

Tickers are organized by sector in `configs/universe.toml`:

- **11 equity sectors**: Technology, Healthcare, Financials, etc.
- **12 ETF categories**: Broad market, sector ETFs, commodities
- **150+ tickers** ready to use

Load universe in code:

```rust
let universe = Universe::load("configs/universe.toml")?;
for sector in &universe.sectors {
    println!("{}: {} tickers", sector.name, sector.len());
}
```

## Visualization & Reporting

TrendLab includes comprehensive visualization and reporting capabilities:

### HTML Reports

Self-contained HTML reports with no external dependencies:

```bash
# Generate HTML report for a sweep run
cargo run -p trendlab-cli -- report html --run-id sweep_001

# Generate and auto-open in browser
cargo run -p trendlab-cli -- report html --run-id sweep_001 --open
```

Report features:

- **Summary section**: Key metrics and configuration overview
- **Equity chart**: Interactive visualization of portfolio performance
- **Trades table**: Complete list of trades with entry/exit details and P&L
- **Metrics summary**: Sharpe, CAGR, max drawdown, and more
- **Inline CSS/JS**: No external dependencies, works offline

### Terminal Output

Formatted terminal output for quick analysis:

| Feature           | Description                                           |
| ----------------- | ----------------------------------------------------- |
| **Sweep tables**  | Colored, aligned tables ranking configurations        |
| **Sparklines**    | Unicode block character mini-charts (`▁▂▃▄▅▆▇█`)     |
| **Equity charts** | ASCII art charts that fit in 80 columns               |
| **Color coding**  | Green for profits, red for losses, yellow for neutral |

### Sparklines

Compact equity curve visualization in a single line:

```text
▁▂▃▄▅▆▇█▇▆▅▆▇█  (shows trend at a glance)
```

## BDD Test Coverage

24 Gherkin feature files covering:

| Feature | Tests |
|---------|-------|
| `strategy_donchian.feature` | Donchian breakout entry/exit logic |
| `strategy_turtle_s1.feature` | Turtle System 1 rules |
| `strategy_turtle_s2.feature` | Turtle System 2 rules |
| `strategy_ma_crossover.feature` | MA crossover logic |
| `strategy_tsmom.feature` | Time-series momentum |
| `strategy_short_selling.feature` | Short selling and LongShort modes |
| `strategy_parabolic_sar.feature` | Parabolic SAR entry/exit on flips |
| `strategy_opening_range.feature` | Opening range breakout logic |
| `strategy_ensemble.feature` | Multi-horizon voting strategies |
| `strategy_keltner.feature` | Keltner channel breakout logic |
| `strategy_dmi_adx.feature` | DMI/ADX directional movement |
| `strategy_aroon.feature` | Aroon oscillator crossovers |
| `strategy_heikin_ashi.feature` | Heikin-Ashi regime detection |
| `indicators.feature` | SMA, EMA, Donchian, ATR, DMI, Aroon, HA |
| `invariants.feature` | No-lookahead, accounting identity |
| `costs.feature` | Fees and slippage handling |
| `volatility_sizing.feature` | ATR-based sizing |
| `pyramiding.feature` | Position pyramiding |
| `sweep.feature` | Parameter sweep infrastructure |
| `data_quality.feature` | Data validation |
| `artifact_parity.feature` | Pine export format |
| `visualization.feature` | HTML reports, terminal output, sparklines |

## One-command quality gate ("press start")

**Windows:**

```powershell
.\scripts\verify.ps1
```

**macOS/Linux:**

```bash
bash scripts/verify.sh
```

## Development workflow (the loop)

1. Add/extend a `.feature` file in `crates/trendlab-bdd/tests/features/`
2. Add/extend a tiny deterministic fixture in `fixtures/synth/` (20–200 bars)
3. Run `scripts/verify.*` and watch it fail (red)
4. Implement the minimum change in `trendlab-core` / `trendlab-cli`
5. Re-run `scripts/verify.*` until green

## CLI Commands

All major commands are implemented and working:

```bash
# Data management
trendlab data refresh-yahoo --tickers SPY,QQQ,IWM --start 2020-01-01 --end 2024-12-31
trendlab data refresh-yahoo --tickers SPY --start 2020-01-01 --end 2024-12-31 --force
trendlab data status --ticker SPY

# Run a single backtest
trendlab run --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31

# Run parameter sweep (Polars by default)
trendlab sweep --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31
trendlab sweep --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31 --sequential
trendlab sweep --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31 \
    --grid "entry:10,20,30,40;exit:5,10,15" --top-n 10

# Reporting
trendlab report list
trendlab report summary --run-id sweep_001 --top-n 10
trendlab report html --run-id sweep_001 --open
trendlab report export --run-id sweep_001 --output results.csv

# Strategy artifacts (for Pine parity)
trendlab artifact export --run-id sweep_001 --config-id best_sharpe
trendlab artifact validate --path artifact.json
```

## Project structure

```text
TrendLab/
├── apps/
│   └── trendlab-gui/     # Desktop GUI (Tauri v2 + React)
│       ├── src-tauri/    # Rust backend (commands, state, events)
│       └── ui/           # React frontend (TypeScript)
├── crates/
│   ├── trendlab-core/    # Domain types, indicators, sim kernel, metrics (no IO)
│   ├── trendlab-cli/     # CLI + orchestration + IO
│   ├── trendlab-tui/     # Terminal UI (ratatui)
│   └── trendlab-bdd/     # BDD runner + step defs + feature files
├── docs/                 # Assumptions, schemas, architecture, style
├── fixtures/             # Deterministic test datasets (tiny)
├── data/                 # Market data cache (gitignored)
├── schemas/              # JSON schemas for artifacts/configs
├── configs/              # Sweep grids and templates
├── reports/              # Generated reports and run outputs
└── artifacts/            # Exported StrategyArtifact JSON
```

## Documentation

- **Assumptions**: `docs/assumptions.md` (fill conventions, timezone, missing bars)
- **Canonical schema**: `docs/schema.md` (bars, partitions, units)
- **BDD style**: `docs/bdd-style.md`
- **Architecture**: `docs/architecture.md`
- **Roadmap**: `.claude/plans/development-plan.md`

## Performance Tuning

Tips for running large parameter sweeps efficiently:

### Memory Management

| Scenario                    | Recommendation                                  |
| --------------------------- | ----------------------------------------------- |
| Large universe (100+ tickers) | Process in batches of 20-30 tickers           |
| Long history (10+ years)    | Use date ranges to limit data loaded            |
| Many parameters (1000+ configs) | Use `--top-n` to limit results in memory     |

### Parallelism Settings

```bash
# Control thread count via environment variable
RAYON_NUM_THREADS=4 cargo run -p trendlab-cli -- sweep ...

# For memory-constrained systems, reduce parallelism
RAYON_NUM_THREADS=2 cargo run -p trendlab-cli -- sweep ...
```

### Backend Selection

| Use Case                  | Recommended Backend    |
| ------------------------- | ---------------------- |
| Production sweeps         | Polars (default)       |
| Debugging strategy logic  | `--sequential`         |
| Validating new indicators | Both (compare results) |

### Data Caching

- First fetch from Yahoo is slow; subsequent runs use Parquet cache
- Use `--force` only when you need fresh data
- Data is partitioned by symbol and year for efficient access

### Profiling Sweeps

```bash
# Time a sweep
time cargo run --release -p trendlab-cli -- sweep ...

# For detailed profiling, use cargo-flamegraph
cargo install flamegraph
cargo flamegraph -p trendlab-cli -- sweep ...
```

## Troubleshooting

### Common Issues

**Data not loading:**

```bash
# Check if data exists
cargo run -p trendlab-cli -- data status --ticker SPY

# Re-fetch with force flag
cargo run -p trendlab-cli -- data refresh-yahoo --tickers SPY --start 2020-01-01 --end 2024-12-31 --force
```

**Sweep produces no results:**

- Verify date range overlaps with available data
- Check that warmup period doesn't exceed data length
- Ensure strategy parameters are valid (e.g., fast MA < slow MA)

**TUI display issues:**

- Ensure terminal is at least 80×24 characters
- Try a different terminal emulator (Windows Terminal, iTerm2)
- Check terminal supports Unicode and 256 colors

**Build errors:**

```bash
# Clean and rebuild
cargo clean
cargo build --release

# Update Rust toolchain
rustup update stable
```

## Contributing

Contributions are welcome! Please follow these guidelines:

### Before Contributing

1. Check existing issues and PRs to avoid duplicates
2. For major changes, open an issue first to discuss the approach
3. Read `docs/bdd-style.md` for testing conventions

### Development Process

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature`
3. Write tests first (BDD scenarios in `crates/trendlab-bdd/tests/features/`)
4. Implement the minimum change to pass tests
5. Run the quality gate: `.\scripts\verify.ps1` (Windows) or `bash scripts/verify.sh`
6. Commit with clear messages
7. Open a pull request

### Code Standards

- `cargo fmt` before committing
- `cargo clippy --all-targets --all-features -D warnings` must pass
- All tests must pass
- New features require BDD scenarios
- Document public APIs

### What We're Looking For

- New strategy implementations (see Strategy Roadmap)
- Additional indicators
- Performance improvements
- Documentation improvements
- Bug fixes with regression tests

## Project Map

```text
TrendLab
│
├─ Getting Started
│  ├── Quick Start .............. Build and run tests
│  ├── Prerequisites ............ Rust 1.75+, terminal setup
│  └── Zero to First Backtest ... 5-minute walkthrough
│
├─ Using TrendLab
│  ├── TUI Features ............. Interactive terminal UI
│  ├── Desktop GUI .............. Tauri + React (coming soon)
│  ├── CLI Commands ............. Data, sweep, report, artifact
│  ├── Strategies ............... Donchian, MA Cross, TSMOM, SAR, ORB, Ensemble
│  └── Strategy Roadmap ......... Keltner, ADX, Darvas, etc.
│
├─ Architecture
│  ├── Workspace Crates ......... core, cli, bdd, tui, gui
│  ├── Backtest Architecture .... Polars vs sequential backends
│  ├── Indicators ............... SMA, EMA, ATR, Donchian
│  └── Metrics .................. Sharpe, CAGR, drawdown, etc.
│
├─ Configuration
│  ├── Universe ................. 150+ tickers by sector
│  ├── Position Sizing .......... Fixed, volatility-based
│  └── Performance Tuning ....... Memory, parallelism tips
│
└─ Development
   ├── BDD Test Coverage ........ 20 feature files
   ├── Development Workflow ..... Red-green-refactor loop
   ├── Contributing ............. Fork, test, PR process
   └── Troubleshooting .......... Common issues & fixes
```

## License

MIT
