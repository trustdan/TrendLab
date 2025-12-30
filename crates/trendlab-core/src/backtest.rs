//! Minimal backtest kernel (Phase 1).
//!
//! Assumptions (Phase 1):
//! - Signals are computed on bar close.
//! - Fills occur on the next bar open.
//! - Long-only (flat or long).
//! - Supports both fixed and dynamic (volatility-based) position sizing.

use crate::bar::Bar;
use crate::error::{Result, TrendLabError};
use crate::sizing::{PositionSizer, SizeResult};
use crate::strategy::{Position, Signal, Strategy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillModel {
    /// Signal on close of bar `t` fills at open of bar `t+1`.
    NextOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CostModel {
    /// Fees in basis points (bps) per side (entry and exit).
    pub fees_bps_per_side: f64,
    /// Slippage in basis points (bps) applied in the "worse" direction.
    pub slippage_bps: f64,
}

impl Default for CostModel {
    fn default() -> Self {
        Self {
            fees_bps_per_side: 0.0,
            slippage_bps: 0.0,
        }
    }
}

/// Configuration for pyramiding (adding to winning positions).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PyramidConfig {
    /// Whether pyramiding is enabled.
    pub enabled: bool,
    /// Maximum number of units (including initial entry).
    pub max_units: usize,
    /// Threshold for adding as a multiple of ATR (e.g., 0.5 = half ATR).
    pub threshold_atr_multiple: f64,
    /// ATR period for calculating pyramid threshold.
    pub atr_period: usize,
}

impl Default for PyramidConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_units: 1,
            threshold_atr_multiple: 0.5,
            atr_period: 20,
        }
    }
}

impl PyramidConfig {
    /// Turtle System 1 pyramid config: 4 units, 0.5 ATR threshold, 20-day ATR.
    pub fn turtle_system_1() -> Self {
        Self {
            enabled: true,
            max_units: 4,
            threshold_atr_multiple: 0.5,
            atr_period: 20,
        }
    }

    /// Disabled pyramiding (single unit).
    pub fn disabled() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub initial_cash: f64,
    pub fill_model: FillModel,
    pub cost_model: CostModel,
    /// Fixed position quantity per unit.
    pub qty: f64,
    /// Pyramiding configuration.
    pub pyramid_config: PyramidConfig,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_cash: 100_000.0,
            fill_model: FillModel::NextOpen,
            cost_model: CostModel::default(),
            qty: 1.0,
            pyramid_config: PyramidConfig::default(),
        }
    }
}

impl BacktestConfig {
    /// Create config with pyramiding enabled.
    pub fn with_pyramid(mut self, pyramid_config: PyramidConfig) -> Self {
        self.pyramid_config = pyramid_config;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub side: Side,
    pub qty: f64,
    pub price: f64,
    pub fees: f64,
    pub raw_price: f64,
    /// ATR value at time of fill (for volatility sizing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub atr_at_fill: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

/// Direction of a trade (long or short).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TradeDirection {
    /// Long trade: buy to open, sell to close.
    #[default]
    Long,
    /// Short trade: sell to open, buy to close.
    Short,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub entry: Fill,
    pub exit: Fill,
    pub gross_pnl: f64,
    pub net_pnl: f64,
    /// Direction of the trade.
    #[serde(default)]
    pub direction: TradeDirection,
}

/// A pyramided trade consisting of multiple entry fills and a single exit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PyramidTrade {
    /// All entry fills (initial + pyramid adds).
    pub entries: Vec<Fill>,
    /// Single exit fill that closes all units.
    pub exit: Fill,
    /// Gross PnL (sum of per-unit PnL).
    pub gross_pnl: f64,
    /// Net PnL (after all fees).
    pub net_pnl: f64,
    /// Average entry price across all units.
    pub avg_entry_price: f64,
    /// Total units traded.
    pub total_units: f64,
    /// Direction of the trade.
    #[serde(default)]
    pub direction: TradeDirection,
}

impl PyramidTrade {
    /// Calculate average entry price from fills.
    pub fn compute_avg_entry_price(entries: &[Fill]) -> f64 {
        if entries.is_empty() {
            return 0.0;
        }
        let total_cost: f64 = entries.iter().map(|f| f.price * f.qty).sum();
        let total_qty: f64 = entries.iter().map(|f| f.qty).sum();
        if total_qty == 0.0 {
            0.0
        } else {
            total_cost / total_qty
        }
    }

    /// Create a PyramidTrade from entries and exit.
    pub fn from_fills(entries: Vec<Fill>, exit: Fill) -> Self {
        let avg_entry_price = Self::compute_avg_entry_price(&entries);
        let total_units: f64 = entries.iter().map(|f| f.qty).sum();
        let total_entry_fees: f64 = entries.iter().map(|f| f.fees).sum();

        let gross_pnl = (exit.price - avg_entry_price) * total_units;
        let net_pnl = gross_pnl - total_entry_fees - exit.fees;

        Self {
            entries,
            exit,
            gross_pnl,
            net_pnl,
            avg_entry_price,
            total_units,
            direction: TradeDirection::Long, // Default to Long for now
        }
    }

    /// Create a PyramidTrade from entries, exit, and direction.
    pub fn from_fills_with_direction(
        entries: Vec<Fill>,
        exit: Fill,
        direction: TradeDirection,
    ) -> Self {
        let avg_entry_price = Self::compute_avg_entry_price(&entries);
        let total_units: f64 = entries.iter().map(|f| f.qty).sum();
        let total_entry_fees: f64 = entries.iter().map(|f| f.fees).sum();

        // For short trades, PnL is reversed: profit when exit < entry
        let gross_pnl = match direction {
            TradeDirection::Long => (exit.price - avg_entry_price) * total_units,
            TradeDirection::Short => (avg_entry_price - exit.price) * total_units,
        };
        let net_pnl = gross_pnl - total_entry_fees - exit.fees;

        Self {
            entries,
            exit,
            gross_pnl,
            net_pnl,
            avg_entry_price,
            total_units,
            direction,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquityPoint {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub cash: f64,
    pub position_qty: f64,
    pub close: f64,
    pub equity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BacktestResult {
    pub fills: Vec<Fill>,
    pub trades: Vec<Trade>,
    /// Pyramid trades (when pyramiding is enabled).
    pub pyramid_trades: Vec<PyramidTrade>,
    pub equity: Vec<EquityPoint>,
}

impl BacktestResult {
    pub fn last_equity(&self) -> Option<f64> {
        self.equity.last().map(|p| p.equity)
    }

    /// Get total number of units currently in position.
    pub fn current_units(&self) -> f64 {
        self.equity.last().map(|p| p.position_qty).unwrap_or(0.0)
    }
}

/// Run a backtest over `bars` with a stateful strategy.
pub fn run_backtest<S: Strategy + ?Sized>(
    bars: &[Bar],
    strategy: &mut S,
    config: BacktestConfig,
) -> Result<BacktestResult> {
    if bars.is_empty() {
        return Ok(BacktestResult {
            fills: vec![],
            trades: vec![],
            pyramid_trades: vec![],
            equity: vec![],
        });
    }

    if config.initial_cash <= 0.0 {
        return Err(TrendLabError::Config("initial_cash must be > 0".into()));
    }
    if config.qty <= 0.0 {
        return Err(TrendLabError::Config("qty must be > 0".into()));
    }

    strategy.reset();

    let mut cash = config.initial_cash;
    let mut position_qty = 0.0;
    let mut position = Position::Flat;

    let mut pending_signal: Option<Signal> = None;
    let mut fills: Vec<Fill> = vec![];
    let mut trades: Vec<Trade> = vec![];
    let mut current_entry: Option<Fill> = None;
    let mut equity: Vec<EquityPoint> = Vec::with_capacity(bars.len());

    for i in 0..bars.len() {
        // 1) Execute fills on open (from prior close).
        if let Some(sig) = pending_signal.take() {
            if i == 0 {
                // Should not happen, but keep it safe.
            } else {
                match (sig, position) {
                    (Signal::EnterLong, Position::Flat) => {
                        let raw_price = bars[i].open;
                        let fill = execute_fill(
                            bars[i].ts,
                            Side::Buy,
                            config.qty,
                            raw_price,
                            &config.cost_model,
                            None,
                        );
                        cash -= fill.qty * fill.price;
                        cash -= fill.fees;
                        position_qty += fill.qty;
                        position = Position::Long;
                        current_entry = Some(fill.clone());
                        fills.push(fill);
                    }
                    (Signal::ExitLong, Position::Long) => {
                        let raw_price = bars[i].open;
                        let fill = execute_fill(
                            bars[i].ts,
                            Side::Sell,
                            config.qty,
                            raw_price,
                            &config.cost_model,
                            None,
                        );
                        cash += fill.qty * fill.price;
                        cash -= fill.fees;
                        position_qty -= fill.qty;
                        position = Position::Flat;

                        let entry = current_entry.take().ok_or_else(|| {
                            TrendLabError::Strategy("exit fill without an entry fill".into())
                        })?;

                        let gross_pnl = (fill.price - entry.price) * entry.qty;
                        let net_pnl = gross_pnl - entry.fees - fill.fees;

                        trades.push(Trade {
                            entry,
                            exit: fill.clone(),
                            gross_pnl,
                            net_pnl,
                            direction: TradeDirection::Long,
                        });
                        fills.push(fill);
                    }
                    // Short entry: Flat -> Short
                    (Signal::EnterShort, Position::Flat) => {
                        let raw_price = bars[i].open;
                        let fill = execute_fill(
                            bars[i].ts,
                            Side::Sell, // Sell to open short
                            config.qty,
                            raw_price,
                            &config.cost_model,
                            None,
                        );
                        // Short sale: receive cash (we're selling borrowed shares)
                        cash += fill.qty * fill.price;
                        cash -= fill.fees;
                        // Position qty is negative for shorts
                        position_qty -= fill.qty;
                        position = Position::Short;
                        current_entry = Some(fill.clone());
                        fills.push(fill);
                    }
                    // Short exit (cover): Short -> Flat
                    (Signal::ExitShort, Position::Short) => {
                        let raw_price = bars[i].open;
                        let fill = execute_fill(
                            bars[i].ts,
                            Side::Buy, // Buy to close short
                            config.qty,
                            raw_price,
                            &config.cost_model,
                            None,
                        );
                        // Cover: pay cash to buy back shares
                        cash -= fill.qty * fill.price;
                        cash -= fill.fees;
                        // Close the short position
                        position_qty += fill.qty;
                        position = Position::Flat;

                        let entry = current_entry.take().ok_or_else(|| {
                            TrendLabError::Strategy("exit fill without an entry fill".into())
                        })?;

                        // For shorts: profit when exit < entry (bought back cheaper)
                        let gross_pnl = (entry.price - fill.price) * entry.qty;
                        let net_pnl = gross_pnl - entry.fees - fill.fees;

                        trades.push(Trade {
                            entry,
                            exit: fill.clone(),
                            gross_pnl,
                            net_pnl,
                            direction: TradeDirection::Short,
                        });
                        fills.push(fill);
                    }
                    _ => {
                        // Ignore impossible or redundant signals (e.g., AddLong, AddShort for now).
                    }
                }
            }
        }

        // 2) Mark-to-market equity at close (after any open fills).
        let close = bars[i].close;
        let eq = cash + position_qty * close;
        equity.push(EquityPoint {
            ts: bars[i].ts,
            cash,
            position_qty,
            close,
            equity: eq,
        });

        // 3) Compute signal on close to be filled next bar open.
        let hist = &bars[..=i];
        let sig = if i + 1 >= strategy.warmup_period() {
            strategy.signal(hist, position)
        } else {
            Signal::Hold
        };

        pending_signal = match config.fill_model {
            FillModel::NextOpen => Some(sig),
        };
    }

    Ok(BacktestResult {
        fills,
        trades,
        pyramid_trades: vec![],
        equity,
    })
}

/// Configuration for backtest with dynamic position sizing.
#[derive(Debug, Clone)]
pub struct BacktestSizingConfig {
    pub initial_cash: f64,
    pub fill_model: FillModel,
    pub cost_model: CostModel,
}

impl Default for BacktestSizingConfig {
    fn default() -> Self {
        Self {
            initial_cash: 100_000.0,
            fill_model: FillModel::NextOpen,
            cost_model: CostModel::default(),
        }
    }
}

/// Pending signal with computed size information.
#[derive(Debug, Clone)]
struct PendingEntry {
    signal: Signal,
    size_result: SizeResult,
}

/// Run a backtest with dynamic position sizing.
///
/// Unlike `run_backtest`, this version uses a `PositionSizer` to determine
/// position size at each entry signal. The sizer computes position size based
/// on the bars available at signal time.
pub fn run_backtest_with_sizer<S: Strategy, P: PositionSizer>(
    bars: &[Bar],
    strategy: &mut S,
    sizer: &P,
    config: BacktestSizingConfig,
) -> Result<BacktestResult> {
    if bars.is_empty() {
        return Ok(BacktestResult {
            fills: vec![],
            trades: vec![],
            pyramid_trades: vec![],
            equity: vec![],
        });
    }

    if config.initial_cash <= 0.0 {
        return Err(TrendLabError::Config("initial_cash must be > 0".into()));
    }

    strategy.reset();

    let mut cash = config.initial_cash;
    let mut position_qty = 0.0;
    let mut position = Position::Flat;

    let mut pending_entry: Option<PendingEntry> = None;
    let mut pending_exit: Option<Signal> = None;
    let mut entry_qty: f64 = 0.0; // Track entry qty to use same qty on exit
    let mut entry_atr: Option<f64> = None;

    let mut fills: Vec<Fill> = vec![];
    let mut trades: Vec<Trade> = vec![];
    let mut current_entry: Option<Fill> = None;
    let mut equity: Vec<EquityPoint> = Vec::with_capacity(bars.len());

    for i in 0..bars.len() {
        // 1) Execute fills on open (from prior close signal).
        if let Some(entry) = pending_entry.take() {
            if i > 0 && matches!(entry.signal, Signal::EnterLong) && position == Position::Flat {
                let raw_price = bars[i].open;
                let qty = entry.size_result.units;

                let fill = execute_fill(
                    bars[i].ts,
                    Side::Buy,
                    qty,
                    raw_price,
                    &config.cost_model,
                    entry.size_result.atr,
                );
                cash -= fill.qty * fill.price;
                cash -= fill.fees;
                position_qty += fill.qty;
                entry_qty = fill.qty;
                entry_atr = entry.size_result.atr;
                position = Position::Long;
                current_entry = Some(fill.clone());
                fills.push(fill);
            }
        }

        if let Some(sig) = pending_exit.take() {
            if i > 0 && matches!(sig, Signal::ExitLong) && position == Position::Long {
                let raw_price = bars[i].open;
                let qty = entry_qty; // Use same qty as entry

                let fill = execute_fill(
                    bars[i].ts,
                    Side::Sell,
                    qty,
                    raw_price,
                    &config.cost_model,
                    entry_atr,
                );
                cash += fill.qty * fill.price;
                cash -= fill.fees;
                position_qty -= fill.qty;
                position = Position::Flat;
                entry_qty = 0.0;
                entry_atr = None;

                let entry = current_entry.take().ok_or_else(|| {
                    TrendLabError::Strategy("exit fill without an entry fill".into())
                })?;

                let gross_pnl = (fill.price - entry.price) * entry.qty;
                let net_pnl = gross_pnl - entry.fees - fill.fees;

                trades.push(Trade {
                    entry,
                    exit: fill.clone(),
                    gross_pnl,
                    net_pnl,
                    direction: TradeDirection::Long,
                });
                fills.push(fill);
            }
        }

        // 2) Mark-to-market equity at close.
        let close = bars[i].close;
        let eq = cash + position_qty * close;
        equity.push(EquityPoint {
            ts: bars[i].ts,
            cash,
            position_qty,
            close,
            equity: eq,
        });

        // 3) Compute signal on close.
        let hist = &bars[..=i];
        let warmup = strategy.warmup_period().max(sizer.warmup_period());

        let sig = if i + 1 >= warmup {
            strategy.signal(hist, position)
        } else {
            Signal::Hold
        };

        // 4) Prepare pending fill for next bar.
        match (sig, position) {
            (Signal::EnterLong, Position::Flat) => {
                // Compute position size now (at signal time)
                if let Some(size_result) = sizer.size(hist, close) {
                    pending_entry = Some(PendingEntry {
                        signal: sig,
                        size_result,
                    });
                }
                // If sizer returns None, skip the entry
            }
            (Signal::ExitLong, Position::Long) => {
                pending_exit = Some(sig);
            }
            _ => {}
        }
    }

    Ok(BacktestResult {
        fills,
        trades,
        pyramid_trades: vec![],
        equity,
    })
}

/// State for pyramiding during a backtest.
#[derive(Debug, Clone, Default)]
struct PyramidState {
    /// Current number of units (0 when flat).
    units: usize,
    /// Price of last pyramid entry (used for threshold calculation).
    last_add_price: f64,
    /// ATR at initial entry (used for threshold calculation).
    entry_atr: f64,
    /// All entry fills for the current position.
    entries: Vec<Fill>,
}

impl PyramidState {
    fn reset(&mut self) {
        self.units = 0;
        self.last_add_price = 0.0;
        self.entry_atr = 0.0;
        self.entries.clear();
    }

    fn is_flat(&self) -> bool {
        self.units == 0
    }

    fn can_add(&self, max_units: usize) -> bool {
        self.units < max_units
    }

    fn should_pyramid(&self, current_price: f64, threshold_atr_multiple: f64) -> bool {
        if self.is_flat() || self.entry_atr <= 0.0 {
            return false;
        }
        let threshold = self.entry_atr * threshold_atr_multiple;
        current_price >= self.last_add_price + threshold
    }
}

/// Run a backtest with pyramiding support.
///
/// This function supports adding to winning positions at fixed ATR intervals.
/// - Initial entry creates unit 1
/// - Additional units added when price moves by `threshold_atr_multiple * ATR`
/// - Maximum of `max_units` total units
/// - All units exit together on exit signal
///
/// Returns `BacktestResult` with `pyramid_trades` populated.
pub fn run_backtest_with_pyramid<S: Strategy>(
    bars: &[Bar],
    strategy: &mut S,
    config: BacktestConfig,
) -> Result<BacktestResult> {
    use crate::indicators::atr_wilder;

    if bars.is_empty() {
        return Ok(BacktestResult {
            fills: vec![],
            trades: vec![],
            pyramid_trades: vec![],
            equity: vec![],
        });
    }

    if config.initial_cash <= 0.0 {
        return Err(TrendLabError::Config("initial_cash must be > 0".into()));
    }
    if config.qty <= 0.0 {
        return Err(TrendLabError::Config("qty must be > 0".into()));
    }

    // If pyramiding disabled, delegate to standard backtest
    if !config.pyramid_config.enabled {
        return run_backtest(bars, strategy, config);
    }

    strategy.reset();

    let pyramid_cfg = &config.pyramid_config;
    let atr_values = atr_wilder(bars, pyramid_cfg.atr_period);

    let mut cash = config.initial_cash;
    let mut position_qty = 0.0;
    let mut position = Position::Flat;

    let mut pending_signal: Option<Signal> = None;
    let mut pending_pyramid_price: Option<f64> = None; // Price at which pyramid was triggered
    let mut fills: Vec<Fill> = vec![];
    let mut pyramid_trades: Vec<PyramidTrade> = vec![];
    let mut equity: Vec<EquityPoint> = Vec::with_capacity(bars.len());
    let mut pyr_state = PyramidState::default();

    for i in 0..bars.len() {
        let current_bar = &bars[i];

        // 1) Execute pending entry/exit/pyramid fills on open
        if let Some(sig) = pending_signal.take() {
            match (sig, position) {
                (Signal::EnterLong, Position::Flat) => {
                    let raw_price = current_bar.open;
                    let entry_atr = if i > 0 {
                        atr_values[i - 1].unwrap_or(4.0) // Use prior bar's ATR, default if unavailable
                    } else {
                        4.0 // Default ATR for warmup period
                    };

                    let fill = execute_fill(
                        current_bar.ts,
                        Side::Buy,
                        config.qty,
                        raw_price,
                        &config.cost_model,
                        Some(entry_atr),
                    );
                    cash -= fill.qty * fill.price;
                    cash -= fill.fees;
                    position_qty += fill.qty;
                    position = Position::Long;

                    pyr_state.units = 1;
                    pyr_state.last_add_price = fill.price;
                    pyr_state.entry_atr = entry_atr;
                    pyr_state.entries.push(fill.clone());
                    fills.push(fill);
                }
                (Signal::ExitLong, Position::Long) => {
                    // Exit all units together
                    let raw_price = current_bar.open;
                    let total_qty = position_qty;
                    let fill = execute_fill(
                        current_bar.ts,
                        Side::Sell,
                        total_qty,
                        raw_price,
                        &config.cost_model,
                        Some(pyr_state.entry_atr),
                    );
                    cash += fill.qty * fill.price;
                    cash -= fill.fees;
                    position_qty = 0.0;
                    position = Position::Flat;

                    // Create pyramid trade from accumulated entries
                    if !pyr_state.entries.is_empty() {
                        let pyr_trade =
                            PyramidTrade::from_fills(pyr_state.entries.clone(), fill.clone());
                        pyramid_trades.push(pyr_trade);
                    }

                    fills.push(fill);
                    pyr_state.reset();
                }
                _ => {}
            }
        }

        // Execute pending pyramid add
        if pending_pyramid_price.take().is_some()
            && position == Position::Long
            && pyr_state.can_add(pyramid_cfg.max_units)
        {
            let raw_price = current_bar.open;
            let fill = execute_fill(
                current_bar.ts,
                Side::Buy,
                config.qty,
                raw_price,
                &config.cost_model,
                Some(pyr_state.entry_atr),
            );
            cash -= fill.qty * fill.price;
            cash -= fill.fees;
            position_qty += fill.qty;

            pyr_state.units += 1;
            pyr_state.last_add_price = fill.price;
            pyr_state.entries.push(fill.clone());
            fills.push(fill);
        }

        // 2) Mark-to-market equity at close
        let close = current_bar.close;
        let eq = cash + position_qty * close;
        equity.push(EquityPoint {
            ts: current_bar.ts,
            cash,
            position_qty,
            close,
            equity: eq,
        });

        // 3) Compute signals on close for next bar
        let hist = &bars[..=i];
        let warmup = strategy.warmup_period().max(pyramid_cfg.atr_period);

        let sig = if i + 1 >= warmup {
            strategy.signal(hist, position)
        } else {
            Signal::Hold
        };

        // Set pending signal for entry/exit
        pending_signal = Some(sig);

        // 4) Check for pyramid opportunity (on close, execute on next open)
        if position == Position::Long
            && pyr_state.can_add(pyramid_cfg.max_units)
            && pyr_state.should_pyramid(close, pyramid_cfg.threshold_atr_multiple)
        {
            pending_pyramid_price = Some(close);
        }
    }

    Ok(BacktestResult {
        fills,
        trades: vec![], // Standard trades not populated for pyramid backtest
        pyramid_trades,
        equity,
    })
}

fn execute_fill(
    ts: chrono::DateTime<chrono::Utc>,
    side: Side,
    qty: f64,
    raw_price: f64,
    costs: &CostModel,
    atr_at_fill: Option<f64>,
) -> Fill {
    let slip_rate = costs.slippage_bps / 10_000.0;
    let fee_rate = costs.fees_bps_per_side / 10_000.0;

    let slipped_price = match side {
        Side::Buy => raw_price * (1.0 + slip_rate),
        Side::Sell => raw_price * (1.0 - slip_rate),
    };

    let notional = qty * slipped_price;
    let fees = notional.abs() * fee_rate;

    Fill {
        ts,
        side,
        qty,
        price: slipped_price,
        fees,
        raw_price,
        atr_at_fill,
    }
}

/// A deterministic, test-only strategy: enter at a fixed bar index, exit at a fixed bar index.
///
/// The strategy generates signals on bar **close** at index `entry_idx` / `exit_idx`.
#[derive(Debug, Clone)]
pub struct FixedEntryExitStrategy {
    entry_idx: usize,
    exit_idx: usize,
}

impl FixedEntryExitStrategy {
    pub fn new(entry_idx: usize, exit_idx: usize) -> Self {
        Self {
            entry_idx,
            exit_idx,
        }
    }
}

impl Strategy for FixedEntryExitStrategy {
    fn id(&self) -> &str {
        "fixed_entry_exit"
    }

    fn warmup_period(&self) -> usize {
        0
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        let idx = bars.len().saturating_sub(1);
        match current_position {
            Position::Flat if idx == self.entry_idx => Signal::EnterLong,
            Position::Long if idx == self.exit_idx => Signal::ExitLong,
            _ => Signal::Hold,
        }
    }

    fn reset(&mut self) {}
}

/// A deterministic, test-only strategy for short positions.
///
/// The strategy generates short entry/exit signals at fixed bar indices.
#[derive(Debug, Clone)]
pub struct FixedShortStrategy {
    entry_idx: usize,
    exit_idx: usize,
}

impl FixedShortStrategy {
    pub fn new(entry_idx: usize, exit_idx: usize) -> Self {
        Self {
            entry_idx,
            exit_idx,
        }
    }
}

impl Strategy for FixedShortStrategy {
    fn id(&self) -> &str {
        "fixed_short"
    }

    fn warmup_period(&self) -> usize {
        0
    }

    fn signal(&self, bars: &[Bar], current_position: Position) -> Signal {
        let idx = bars.len().saturating_sub(1);
        match current_position {
            Position::Flat if idx == self.entry_idx => Signal::EnterShort,
            Position::Short if idx == self.exit_idx => Signal::ExitShort,
            _ => Signal::Hold,
        }
    }

    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn mk_bar(day: u32, open: f64, close: f64) -> Bar {
        let ts = chrono::Utc.with_ymd_and_hms(2024, 1, day, 0, 0, 0).unwrap();
        Bar::new(ts, open, open, open, close, 0.0, "TEST", "1d")
    }

    #[test]
    fn fill_is_next_open() {
        let bars = vec![
            mk_bar(1, 10.0, 10.0),
            mk_bar(2, 20.0, 20.0),
            mk_bar(3, 30.0, 30.0),
            mk_bar(4, 40.0, 40.0),
        ];

        let mut strat = FixedEntryExitStrategy::new(1, 2);
        let res = run_backtest(&bars, &mut strat, BacktestConfig::default()).unwrap();

        // Signal at idx=1 fills at idx=2 open => 30.0
        assert_eq!(res.fills[0].price, 30.0);
    }

    #[test]
    fn short_trade_profitable_on_price_decline() {
        // Price declines from 100 to 80 -> short should profit
        let bars = vec![
            mk_bar(1, 100.0, 100.0),
            mk_bar(2, 100.0, 95.0), // Signal short at close (idx=1)
            mk_bar(3, 95.0, 90.0),  // Short entry fills at open (95.0)
            mk_bar(4, 90.0, 85.0),  // Signal exit at close (idx=3)
            mk_bar(5, 85.0, 80.0),  // Short exit fills at open (85.0)
        ];

        let mut strat = FixedShortStrategy::new(1, 3);
        let config = BacktestConfig {
            initial_cash: 100_000.0,
            qty: 10.0,
            ..Default::default()
        };
        let res = run_backtest(&bars, &mut strat, config).unwrap();

        // Verify we have 2 fills
        assert_eq!(res.fills.len(), 2, "Should have 2 fills (entry and exit)");

        // Short entry: sell at 95.0
        assert_eq!(res.fills[0].side, Side::Sell);
        assert_eq!(res.fills[0].price, 95.0);

        // Short exit (cover): buy at 85.0
        assert_eq!(res.fills[1].side, Side::Buy);
        assert_eq!(res.fills[1].price, 85.0);

        // Verify trade
        assert_eq!(res.trades.len(), 1);
        let trade = &res.trades[0];
        assert_eq!(trade.direction, TradeDirection::Short);

        // Gross PnL: (entry_price - exit_price) * qty = (95 - 85) * 10 = 100
        assert_eq!(trade.gross_pnl, 100.0);
    }

    #[test]
    fn short_trade_loss_on_price_increase() {
        // Price increases from 100 to 120 -> short should lose
        let bars = vec![
            mk_bar(1, 100.0, 100.0),
            mk_bar(2, 100.0, 105.0), // Signal short at close (idx=1)
            mk_bar(3, 105.0, 110.0), // Short entry fills at open (105.0)
            mk_bar(4, 110.0, 115.0), // Signal exit at close (idx=3)
            mk_bar(5, 115.0, 120.0), // Short exit fills at open (115.0)
        ];

        let mut strat = FixedShortStrategy::new(1, 3);
        let config = BacktestConfig {
            initial_cash: 100_000.0,
            qty: 10.0,
            ..Default::default()
        };
        let res = run_backtest(&bars, &mut strat, config).unwrap();

        // Verify trade
        assert_eq!(res.trades.len(), 1);
        let trade = &res.trades[0];
        assert_eq!(trade.direction, TradeDirection::Short);

        // Gross PnL: (entry_price - exit_price) * qty = (105 - 115) * 10 = -100
        assert_eq!(trade.gross_pnl, -100.0);
    }

    #[test]
    fn short_position_has_negative_qty_in_equity() {
        let bars = vec![
            mk_bar(1, 100.0, 100.0),
            mk_bar(2, 100.0, 100.0), // Signal short at close (idx=1)
            mk_bar(3, 100.0, 100.0), // Short entry fills at open
            mk_bar(4, 100.0, 100.0),
        ];

        let mut strat = FixedShortStrategy::new(1, 10); // Exit never triggers
        let config = BacktestConfig {
            initial_cash: 100_000.0,
            qty: 10.0,
            ..Default::default()
        };
        let res = run_backtest(&bars, &mut strat, config).unwrap();

        // After short entry (bar index 2), position_qty should be -10
        assert_eq!(res.equity[2].position_qty, -10.0);
        assert_eq!(res.equity[3].position_qty, -10.0);
    }

    #[test]
    fn short_equity_accounting_identity() {
        // Verify: equity = cash + position_qty * close at every bar
        let bars = vec![
            mk_bar(1, 100.0, 100.0),
            mk_bar(2, 100.0, 100.0), // Signal short
            mk_bar(3, 100.0, 95.0),  // Entry at 100, close at 95 -> unrealized profit
            mk_bar(4, 95.0, 90.0),   // Signal exit
            mk_bar(5, 90.0, 85.0),   // Exit at 90
        ];

        let mut strat = FixedShortStrategy::new(1, 3);
        let config = BacktestConfig {
            initial_cash: 100_000.0,
            qty: 10.0,
            ..Default::default()
        };
        let res = run_backtest(&bars, &mut strat, config).unwrap();

        for eq in &res.equity {
            let calculated_equity = eq.cash + eq.position_qty * eq.close;
            assert!(
                (eq.equity - calculated_equity).abs() < 0.0001,
                "Equity identity violated: {} != {} (cash={}, pos={}, close={})",
                eq.equity,
                calculated_equity,
                eq.cash,
                eq.position_qty,
                eq.close
            );
        }
    }
}
