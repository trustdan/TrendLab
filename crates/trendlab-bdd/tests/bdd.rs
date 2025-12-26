//! Cucumber BDD test runner for TrendLab.

use cucumber::{given, then, when, World};
use std::path::PathBuf;

/// World state for BDD scenarios.
#[derive(Debug, Default, World)]
pub struct TrendLabWorld {
    /// Loaded bars for testing
    bars: Vec<trendlab_core::Bar>,

    /// Current result or error message
    result: Option<String>,
}

// Step definitions

#[given(regex = r"^a bar series from fixture (.+)$")]
async fn load_fixture(world: &mut TrendLabWorld, fixture: String) {
    let path = PathBuf::from(format!("../../fixtures/{}", fixture));
    // TODO: Implement fixture loading
    world.result = Some(format!("Would load fixture: {:?}", path));
}

#[given("no data loaded")]
async fn no_data(world: &mut TrendLabWorld) {
    world.bars.clear();
}

#[when("the backtest runs")]
async fn run_backtest(world: &mut TrendLabWorld) {
    // TODO: Implement backtest execution
    world.result = Some("Backtest would run here".to_string());
}

#[then(regex = r"^the result should contain (.+)$")]
async fn check_result(world: &mut TrendLabWorld, expected: String) {
    if let Some(ref result) = world.result {
        assert!(
            result.contains(&expected),
            "Expected '{}' in result, got: {}",
            expected,
            result
        );
    } else {
        panic!("No result available");
    }
}

#[then("no trades should be generated")]
async fn no_trades(_world: &mut TrendLabWorld) {
    // TODO: Check trade count
}

fn main() {
    // Run cucumber tests
    futures::executor::block_on(TrendLabWorld::run("tests/features"));
}
