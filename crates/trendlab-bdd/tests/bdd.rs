//! Cucumber BDD test runner for TrendLab.

use cucumber::{given, then, when, World};
use std::path::PathBuf;

/// World state for BDD scenarios.
#[derive(Debug, Default, World)]
pub struct TrendLabWorld {
    /// Loaded bars for testing
    bars: Vec<trendlab_core::Bar>,

    sma_first: Option<Vec<Option<f64>>>,
    sma_second: Option<Vec<Option<f64>>>,

    fees_bps_per_side: f64,
    slippage_bps: f64,

    last_entry_idx: Option<usize>,
    last_exit_idx: Option<usize>,

    backtest_first: Option<trendlab_core::backtest::BacktestResult>,
    backtest_second: Option<trendlab_core::backtest::BacktestResult>,
}

#[derive(Debug, serde::Deserialize)]
struct FixtureRow {
    ts: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    symbol: String,
    timeframe: String,
}

fn load_fixture_csv(fixture: &str) -> Vec<trendlab_core::Bar> {
    let path = PathBuf::from(format!("../../fixtures/{}", fixture));
    let mut reader = csv::Reader::from_path(&path)
        .unwrap_or_else(|e| panic!("Failed to open fixture {:?}: {}", path, e));

    let mut out = Vec::new();
    for row in reader.deserialize::<FixtureRow>() {
        let row =
            row.unwrap_or_else(|e| panic!("Failed to parse fixture row in {:?}: {}", path, e));
        let ts = chrono::DateTime::parse_from_rfc3339(&row.ts)
            .unwrap_or_else(|e| panic!("Failed to parse ts '{}' in {:?}: {}", row.ts, path, e))
            .with_timezone(&chrono::Utc);
        out.push(trendlab_core::Bar::new(
            ts,
            row.open,
            row.high,
            row.low,
            row.close,
            row.volume,
            row.symbol,
            row.timeframe,
        ));
    }
    out
}

fn assert_f64_eq(a: f64, b: f64, eps: f64, msg: &str) {
    let diff = (a - b).abs();
    assert!(
        diff <= eps,
        "{}: expected {} ≈ {} (diff {} > eps {})",
        msg,
        a,
        b,
        diff,
        eps
    );
}

// Step definitions

#[given(regex = r"^a synthetic bar series from fixture (.+)$")]
async fn given_synthetic_fixture(world: &mut TrendLabWorld, fixture: String) {
    world.bars = load_fixture_csv(&fixture);
    world.sma_first = None;
    world.sma_second = None;
    world.backtest_first = None;
    world.backtest_second = None;
    world.fees_bps_per_side = 0.0;
    world.slippage_bps = 0.0;
    world.last_entry_idx = None;
    world.last_exit_idx = None;
}

#[given(regex = r"^fees are set to (\d+(?:\.\d+)?) bps per side$")]
async fn given_fees(world: &mut TrendLabWorld, bps: String) {
    world.fees_bps_per_side = bps.parse::<f64>().unwrap();
}

#[given(regex = r"^slippage is set to (\d+(?:\.\d+)?) bps$")]
async fn given_slippage(world: &mut TrendLabWorld, bps: String) {
    world.slippage_bps = bps.parse::<f64>().unwrap();
}

#[when(regex = r"^I compute SMA with window (\d+)(?: again)?$")]
async fn when_compute_sma(world: &mut TrendLabWorld, window: String) {
    let window = window.parse::<usize>().unwrap();
    let sma = trendlab_core::indicators::sma_close(&world.bars, window);

    if world.sma_first.is_none() {
        world.sma_first = Some(sma);
    } else {
        world.sma_second = Some(sma);
    }
}

#[when(regex = r"^I modify bars after index (\d+)$")]
async fn when_modify_bars_after_index(world: &mut TrendLabWorld, idx: String) {
    let idx = idx.parse::<usize>().unwrap();
    for (i, b) in world.bars.iter_mut().enumerate() {
        if i > idx {
            // Make the change obvious.
            b.open *= 10.0;
            b.high *= 10.0;
            b.low *= 10.0;
            b.close *= 10.0;
        }
    }
}

#[when(regex = r"^I run a backtest with fixed entry at index (\d+) and exit at index (\d+)$")]
async fn when_run_backtest_fixed(world: &mut TrendLabWorld, entry: String, exit: String) {
    let entry = entry.parse::<usize>().unwrap();
    let exit = exit.parse::<usize>().unwrap();
    world.last_entry_idx = Some(entry);
    world.last_exit_idx = Some(exit);

    let mut strat = trendlab_core::backtest::FixedEntryExitStrategy::new(entry, exit);
    let cfg = trendlab_core::backtest::BacktestConfig {
        initial_cash: 100_000.0,
        fill_model: trendlab_core::backtest::FillModel::NextOpen,
        cost_model: trendlab_core::backtest::CostModel {
            fees_bps_per_side: world.fees_bps_per_side,
            slippage_bps: world.slippage_bps,
        },
        qty: 1.0,
    };

    let res = trendlab_core::backtest::run_backtest(&world.bars, &mut strat, cfg)
        .unwrap_or_else(|e| panic!("Backtest failed: {}", e));

    if world.backtest_first.is_none() {
        world.backtest_first = Some(res);
    } else {
        world.backtest_second = Some(res);
    }
}

#[when("I run the same backtest again")]
async fn when_run_same_backtest_again(world: &mut TrendLabWorld) {
    let entry = world
        .last_entry_idx
        .expect("Expected last entry index to be set before re-running");
    let exit = world
        .last_exit_idx
        .expect("Expected last exit index to be set before re-running");
    when_run_backtest_fixed(world, entry.to_string(), exit.to_string()).await;
}

#[then(regex = r"^SMA values through index (\d+) must be identical$")]
async fn then_sma_values_identical_through(world: &mut TrendLabWorld, idx: String) {
    let idx = idx.parse::<usize>().unwrap();
    let a = world.sma_first.as_ref().expect("Expected SMA first");
    let b = world.sma_second.as_ref().expect("Expected SMA second");

    assert!(idx < a.len() && idx < b.len(), "Index out of bounds");

    for i in 0..=idx {
        match (a[i], b[i]) {
            (None, None) => {}
            (Some(x), Some(y)) => assert_f64_eq(x, y, 1e-10, &format!("SMA mismatch at {}", i)),
            _ => panic!("SMA option mismatch at {}: {:?} vs {:?}", i, a[i], b[i]),
        }
    }
}

#[then("the two backtest results must be identical")]
async fn then_backtests_identical(world: &mut TrendLabWorld) {
    let a = world
        .backtest_first
        .as_ref()
        .expect("Expected backtest first");
    let b = world
        .backtest_second
        .as_ref()
        .expect("Expected backtest second");
    assert_eq!(a, b);
}

#[then("for every bar equity must equal cash plus position_qty times close")]
async fn then_accounting_identity(world: &mut TrendLabWorld) {
    let res = world
        .backtest_first
        .as_ref()
        .expect("Expected backtest result");
    for (i, pt) in res.equity.iter().enumerate() {
        let expected = pt.cash + pt.position_qty * pt.close;
        assert_f64_eq(
            pt.equity,
            expected,
            1e-8,
            &format!("Accounting identity failed at equity index {}", i),
        );
    }
}

#[then(regex = r"^the entry fill price must equal the open price at index (\d+)$")]
async fn then_entry_fill_is_next_open(world: &mut TrendLabWorld, idx: String) {
    let idx = idx.parse::<usize>().unwrap();
    let res = world
        .backtest_first
        .as_ref()
        .expect("Expected backtest result");
    let entry_fill = res.fills.first().expect("Expected at least one fill");
    let expected_open = world.bars[idx].open;
    assert_f64_eq(
        entry_fill.price,
        expected_open,
        1e-10,
        "Entry fill price mismatch",
    );
}

#[then("net PnL must equal gross PnL minus expected fees")]
async fn then_net_pnl_matches(world: &mut TrendLabWorld) {
    let res = world
        .backtest_first
        .as_ref()
        .expect("Expected backtest result");
    let trade = res.trades.first().expect("Expected at least one trade");

    let fee_rate = world.fees_bps_per_side / 10_000.0;
    let expected_fees =
        (trade.entry.qty * trade.entry.price + trade.exit.qty * trade.exit.price) * fee_rate;
    let expected_net = trade.gross_pnl - expected_fees;

    assert_f64_eq(trade.net_pnl, expected_net, 1e-8, "Net PnL mismatch");
}

#[then("entry fill must be worse than the raw price")]
async fn then_entry_slippage_direction(world: &mut TrendLabWorld) {
    let res = world
        .backtest_first
        .as_ref()
        .expect("Expected backtest result");
    let entry_fill = res.fills.first().expect("Expected entry fill");
    assert!(
        entry_fill.price > entry_fill.raw_price,
        "Expected buy fill price ({}) to be worse than raw ({})",
        entry_fill.price,
        entry_fill.raw_price
    );
}

#[then("exit fill must be worse than the raw price")]
async fn then_exit_slippage_direction(world: &mut TrendLabWorld) {
    let res = world
        .backtest_first
        .as_ref()
        .expect("Expected backtest result");
    let exit_fill = res.fills.get(1).expect("Expected exit fill");
    assert!(
        exit_fill.price < exit_fill.raw_price,
        "Expected sell fill price ({}) to be worse than raw ({})",
        exit_fill.price,
        exit_fill.raw_price
    );
}

fn main() {
    // Run cucumber tests
    futures::executor::block_on(TrendLabWorld::run("tests/features"));
}
