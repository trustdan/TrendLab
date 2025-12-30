//! Results panel state and related types.

use std::collections::HashMap;

use trendlab_core::{
    MultiStrategySweepResult, MultiSweepResult, StatisticalAnalysis, SweepConfigResult,
};

/// View mode for the Results panel (per-ticker vs aggregated portfolio)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultsViewMode {
    /// Viewing single-symbol sweep results (current behavior)
    #[default]
    SingleSymbol,
    /// Viewing per-ticker results from a multi-symbol sweep
    PerTicker,
    /// Viewing aggregated portfolio results
    Aggregated,
    /// Viewing YOLO leaderboard (cross-symbol aggregated results)
    Leaderboard,
}

/// Per-ticker summary for multi-sweep display
#[derive(Debug, Clone)]
pub struct TickerSummary {
    pub symbol: String,
    pub best_config_entry: usize,
    pub best_config_exit: usize,
    pub cagr: f64,
    pub sharpe: f64,
    pub max_drawdown: f64,
    pub num_trades: u32,
}

/// Results panel state
#[derive(Debug, Default)]
pub struct ResultsState {
    pub results: Vec<SweepConfigResult>,
    pub selected_index: usize,
    pub sort_column: usize,
    /// View mode for multi-sweep results
    pub view_mode: ResultsViewMode,
    /// Multi-sweep results (when running across multiple symbols)
    pub multi_sweep_result: Option<MultiSweepResult>,
    /// Multi-strategy sweep results (when running all strategies across all symbols)
    pub multi_strategy_result: Option<MultiStrategySweepResult>,
    /// Per-ticker summaries (derived from multi_sweep_result)
    pub ticker_summaries: Vec<TickerSummary>,
    /// Selected ticker index (in PerTicker view)
    pub selected_ticker_index: usize,
    /// Selected leaderboard index (in Leaderboard view)
    pub selected_leaderboard_index: usize,
    /// Expanded leaderboard index (for in-place drill-down, None = collapsed)
    pub expanded_leaderboard_index: Option<usize>,
    /// Statistical analysis for the currently selected config
    pub selected_analysis: Option<StatisticalAnalysis>,
    /// ID of the config for which analysis is being shown
    pub selected_analysis_id: Option<String>,
    /// Cache of computed analyses (config_id -> analysis)
    pub analysis_cache: HashMap<String, StatisticalAnalysis>,
    /// Whether analysis panel is visible
    pub show_analysis: bool,
}

impl ResultsState {
    /// Derive ticker summaries from multi-sweep result
    pub fn update_ticker_summaries(&mut self) {
        self.ticker_summaries.clear();

        if let Some(ref multi) = self.multi_sweep_result {
            for (symbol, sweep_result) in &multi.symbol_results {
                // Find best config by CAGR
                if let Some(best) = sweep_result.config_results.iter().max_by(|a, b| {
                    a.metrics
                        .cagr
                        .partial_cmp(&b.metrics.cagr)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }) {
                    self.ticker_summaries.push(TickerSummary {
                        symbol: symbol.clone(),
                        best_config_entry: best.config_id.entry_lookback,
                        best_config_exit: best.config_id.exit_lookback,
                        cagr: best.metrics.cagr,
                        sharpe: best.metrics.sharpe,
                        max_drawdown: best.metrics.max_drawdown,
                        num_trades: best.metrics.num_trades,
                    });
                }
            }
            // Sort by symbol name for consistent display
            self.ticker_summaries
                .sort_by(|a, b| a.symbol.cmp(&b.symbol));
        }
    }

    /// Check if we have multi-sweep results
    pub fn has_multi_sweep(&self) -> bool {
        self.multi_sweep_result.is_some()
    }

    /// Cycle through available view modes
    pub fn cycle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ResultsViewMode::SingleSymbol => {
                if self.has_multi_sweep() {
                    ResultsViewMode::PerTicker
                } else {
                    ResultsViewMode::Leaderboard
                }
            }
            ResultsViewMode::PerTicker => ResultsViewMode::Aggregated,
            ResultsViewMode::Aggregated => ResultsViewMode::Leaderboard,
            ResultsViewMode::Leaderboard => ResultsViewMode::SingleSymbol,
        };
        self.selected_ticker_index = 0;
        self.selected_leaderboard_index = 0;
    }

    /// Get the current view mode name
    pub fn view_mode_name(&self) -> &'static str {
        match self.view_mode {
            ResultsViewMode::SingleSymbol => "Single Symbol",
            ResultsViewMode::PerTicker => "Per-Ticker",
            ResultsViewMode::Aggregated => "Portfolio",
            ResultsViewMode::Leaderboard => "YOLO Leaderboard",
        }
    }
}
