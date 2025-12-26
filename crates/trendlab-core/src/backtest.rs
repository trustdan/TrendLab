//! Minimal backtest kernel (Phase 1).
//!
//! Assumptions (Phase 1):
//! - Signals are computed on bar close.
//! - Fills occur on the next bar open.
//! - Long-only (flat or long).
//! - Fixed quantity sizing (1.0 unit) for now.

use crate::bar::Bar;
use crate::error::{Result, TrendLabError};
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub initial_cash: f64,
    pub fill_model: FillModel,
    pub cost_model: CostModel,
    /// Fixed position quantity (Phase 1).
    pub qty: f64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_cash: 100_000.0,
            fill_model: FillModel::NextOpen,
            cost_model: CostModel::default(),
            qty: 1.0,
        }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub entry: Fill,
    pub exit: Fill,
    pub gross_pnl: f64,
    pub net_pnl: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquityPoint {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub cash: f64,
    pub position_qty: f64,
    pub close: f64,
    pub equity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestResult {
    pub fills: Vec<Fill>,
    pub trades: Vec<Trade>,
    pub equity: Vec<EquityPoint>,
}

impl BacktestResult {
    pub fn last_equity(&self) -> Option<f64> {
        self.equity.last().map(|p| p.equity)
    }
}

/// Run a backtest over `bars` with a stateful strategy.
pub fn run_backtest<S: Strategy>(
    bars: &[Bar],
    strategy: &mut S,
    config: BacktestConfig,
) -> Result<BacktestResult> {
    if bars.is_empty() {
        return Ok(BacktestResult {
            fills: vec![],
            trades: vec![],
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
                        });
                        fills.push(fill);
                    }
                    _ => {
                        // Ignore impossible or redundant signals.
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
        equity,
    })
}

fn execute_fill(
    ts: chrono::DateTime<chrono::Utc>,
    side: Side,
    qty: f64,
    raw_price: f64,
    costs: &CostModel,
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
}
