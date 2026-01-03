//! Application state for TrendLab TUI
//!
//! This module is organized into focused submodules:
//! - `randomization` - Seeding and jitter helpers for Monte Carlo exploration
//! - `strategies` - Strategy types, configs, and selection state
//! - `navigation` - Panel navigation, startup mode, and UI state enums
//! - `data` - Data panel state and related types
//! - `sweep` - Sweep panel state
//! - `results` - Results panel state and view modes
//! - `yolo` - YOLO mode continuous auto-optimization state
//! - `chart_state` - Chart panel state and visualization types
//! - `utils` - Utility functions

#![allow(dead_code)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::upper_case_acronyms)]

pub mod chart_state;
pub mod data;
pub mod navigation;
pub mod randomization;
pub mod results;
pub mod strategies;
pub mod sweep;
pub mod utils;
pub mod yolo;

// Re-export all public types for external use
pub use chart_state::{
    AnimationState, CandleData, ChartRect, ChartState, ChartViewMode, CursorState, StrategyCurve,
    TickerBestStrategy, TickerCurve, WinningConfig,
};
pub use data::{DataState, DataViewMode, SearchSuggestion};
pub use navigation::{
    AutoRunState, AutoStage, HelpSection, HelpState, MessageType, OperationState, Panel,
    StartupMode, StartupState, StrategySelection,
};
pub use randomization::{
    derive_nonrepeatable_seed, env_optional_bool, env_truthy, env_u64, generate_seed,
    jitter_date_range_percent, jitter_f64_percent, jitter_pct_delta, jitter_usize_percent,
    RandomDefaults,
};
pub use results::{ResultsState, ResultsViewMode, TickerSummary};
pub use strategies::{
    DonchianConfig, EnsembleConfig, KeltnerConfig, MACrossoverConfig, OpeningRangeConfig,
    ParabolicSarConfig, STARCConfig, StrategyCategory, StrategyFocus, StrategyState, StrategyType,
    SupertrendConfig, TsmomConfig, VotingMethod,
};
pub use sweep::SweepState;
pub use utils::{calculate_drawdown, scan_parquet_directory};
pub use yolo::{YoloConfigField, YoloConfigState, YoloState};

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use rand::rngs::StdRng;
use rand::SeedableRng;
use trendlab_core::{
    BacktestConfig, Bar, CostModel, CrossSymbolLeaderboard, FillModel, Leaderboard,
    LeaderboardScope, MultiStrategyGrid, PyramidConfig, SweepDepth,
};

use crate::worker::{WorkerChannels, WorkerCommand};

/// Main application state
pub struct App {
    pub active_panel: Panel,
    pub data: DataState,
    pub strategy: StrategyState,
    pub sweep: SweepState,
    pub results: ResultsState,
    pub chart: ChartState,
    pub help: HelpState,
    pub status_message: String,
    pub status_message_type: MessageType,
    pub operation: OperationState,
    pub startup: StartupState,
    pub auto: AutoRunState,
    pub yolo: YoloState,
    pub random_defaults: RandomDefaults,
    /// Default date range used for data fetches in the TUI.
    pub fetch_range: (NaiveDate, NaiveDate),
    /// Canonical default (un-jittered) date range used for reference/reset.
    pub fetch_range_default: (NaiveDate, NaiveDate),
}

impl App {
    /// Set a status message with the given type.
    pub fn set_status(&mut self, message: impl Into<String>, msg_type: MessageType) {
        self.status_message = message.into();
        self.status_message_type = msg_type;
    }

    /// Set an info status message (cyan).
    pub fn set_status_info(&mut self, message: impl Into<String>) {
        self.set_status(message, MessageType::Info);
    }

    /// Set a success status message (green).
    pub fn set_status_success(&mut self, message: impl Into<String>) {
        self.set_status(message, MessageType::Success);
    }

    /// Set a warning status message (yellow).
    pub fn set_status_warning(&mut self, message: impl Into<String>) {
        self.set_status(message, MessageType::Warning);
    }

    /// Set an error status message (red).
    pub fn set_status_error(&mut self, message: impl Into<String>) {
        self.set_status(message, MessageType::Error);
    }

    pub fn new() -> Self {
        // Scan for existing symbols on startup
        let symbols = scan_parquet_directory();

        // Load universe from config (or use default)
        let mut data_state = DataState {
            symbols,
            ..Default::default()
        };
        data_state.load_universe_from_config();
        // Auto-select all tickers by default for YOLO mode
        data_state.select_all();

        // Randomize initial UI defaults.
        // Default: enabled (can be disabled).
        //
        // Env:
        // - TRENDLAB_RANDOM_DEFAULTS=0/false/off (disable)
        // - TRENDLAB_RANDOM_DEFAULTS=1/true/on  (enable)
        // - TRENDLAB_RANDOM_SEED=12345 (optional; overrides the non-repeatable seed)
        let random_enabled = env_optional_bool("TRENDLAB_RANDOM_DEFAULTS").unwrap_or(true);
        let seed = if random_enabled {
            env_u64("TRENDLAB_RANDOM_SEED").unwrap_or_else(derive_nonrepeatable_seed)
        } else {
            0
        };
        let mut rng = StdRng::seed_from_u64(seed);

        let mut strategy_state = StrategyState::default();
        let mut sweep_state = SweepState {
            param_ranges: vec![
                (
                    "entry_lookback".to_string(),
                    vec!["10", "20", "30", "40", "50"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                (
                    "exit_lookback".to_string(),
                    vec!["5", "10", "15", "20", "25"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
            ],
            ..Default::default()
        };

        // Fetch date defaults: 5 years back from today to today.
        // Dynamic calculation ensures we always use recent data.
        let today = chrono::Local::now().date_naive();
        let fetch_default_start = today - chrono::Duration::days(5 * 365);
        let fetch_default_end = today;

        let mut fetch_range = (fetch_default_start, fetch_default_end);

        if random_enabled {
            // Percent-based jitter: 2%..40% (signed), rounded to the same step sizes
            // the UI uses, so "R to reset" + a few arrows gets you back quickly.
            let min_pct = 0.02;
            let max_pct = 0.40;

            strategy_state.donchian_config.entry_lookback = jitter_usize_percent(
                &mut rng,
                strategy_state.donchian_config.entry_lookback,
                min_pct,
                max_pct,
                5,
                5,
                500,
            );
            strategy_state.donchian_config.exit_lookback = jitter_usize_percent(
                &mut rng,
                strategy_state.donchian_config.exit_lookback,
                min_pct,
                max_pct,
                5,
                5,
                500,
            );

            strategy_state.ma_config.fast_period = jitter_usize_percent(
                &mut rng,
                strategy_state.ma_config.fast_period,
                min_pct,
                max_pct,
                5,
                5,
                500,
            );
            // Keep slow > fast by ensuring it's at least fast+5 after jitter.
            let slow = jitter_usize_percent(
                &mut rng,
                strategy_state.ma_config.slow_period,
                min_pct,
                max_pct,
                10,
                10,
                1000,
            );
            strategy_state.ma_config.slow_period =
                slow.max(strategy_state.ma_config.fast_period + 5);

            strategy_state.tsmom_config.lookback = jitter_usize_percent(
                &mut rng,
                strategy_state.tsmom_config.lookback,
                min_pct,
                max_pct,
                21,
                21,
                2000,
            );

            strategy_state.keltner_config.ema_period = jitter_usize_percent(
                &mut rng,
                strategy_state.keltner_config.ema_period,
                min_pct,
                max_pct,
                5,
                5,
                500,
            );
            strategy_state.keltner_config.atr_period = jitter_usize_percent(
                &mut rng,
                strategy_state.keltner_config.atr_period,
                min_pct,
                max_pct,
                5,
                5,
                500,
            );
            strategy_state.keltner_config.multiplier = jitter_f64_percent(
                &mut rng,
                strategy_state.keltner_config.multiplier,
                min_pct,
                max_pct,
                0.1,
                0.5,
                10.0,
            );

            strategy_state.starc_config.sma_period = jitter_usize_percent(
                &mut rng,
                strategy_state.starc_config.sma_period,
                min_pct,
                max_pct,
                5,
                5,
                500,
            );
            strategy_state.starc_config.atr_period = jitter_usize_percent(
                &mut rng,
                strategy_state.starc_config.atr_period,
                min_pct,
                max_pct,
                5,
                5,
                500,
            );
            strategy_state.starc_config.multiplier = jitter_f64_percent(
                &mut rng,
                strategy_state.starc_config.multiplier,
                min_pct,
                max_pct,
                0.1,
                0.5,
                10.0,
            );

            strategy_state.supertrend_config.atr_period = jitter_usize_percent(
                &mut rng,
                strategy_state.supertrend_config.atr_period,
                min_pct,
                max_pct,
                5,
                5,
                500,
            );
            strategy_state.supertrend_config.multiplier = jitter_f64_percent(
                &mut rng,
                strategy_state.supertrend_config.multiplier,
                min_pct,
                max_pct,
                0.1,
                0.5,
                10.0,
            );

            strategy_state.parabolic_sar_config.af_start = jitter_f64_percent(
                &mut rng,
                strategy_state.parabolic_sar_config.af_start,
                min_pct,
                max_pct,
                0.01,
                0.01,
                1.0,
            );
            strategy_state.parabolic_sar_config.af_step = jitter_f64_percent(
                &mut rng,
                strategy_state.parabolic_sar_config.af_step,
                min_pct,
                max_pct,
                0.01,
                0.01,
                1.0,
            );
            strategy_state.parabolic_sar_config.af_max = jitter_f64_percent(
                &mut rng,
                strategy_state.parabolic_sar_config.af_max,
                min_pct,
                max_pct,
                0.01,
                0.05,
                2.0,
            );

            strategy_state.opening_range_config.range_bars = jitter_usize_percent(
                &mut rng,
                strategy_state.opening_range_config.range_bars,
                min_pct,
                max_pct,
                1,
                1,
                50,
            );

            // Ensemble horizons: jitter each horizon by percent, rounded to 5-day steps.
            strategy_state.ensemble.horizons = strategy_state
                .ensemble
                .horizons
                .iter()
                .map(|h| jitter_usize_percent(&mut rng, *h, min_pct, max_pct, 5, 5, 2000))
                .collect();

            // Sweep grid: apply one percent factor per axis so it stays coherent.
            let entry_factor = 1.0 + jitter_pct_delta(&mut rng, min_pct, max_pct);
            let exit_factor = 1.0 + jitter_pct_delta(&mut rng, min_pct, max_pct);
            for (name, values) in &mut sweep_state.param_ranges {
                if name == "entry_lookback" {
                    *values = values
                        .iter()
                        .filter_map(|v| v.parse::<i32>().ok())
                        .map(|n| {
                            randomization::round_to_step_f64((n as f64) * entry_factor, 5.0)
                                .max(5.0) as i32
                        })
                        .map(|n| n.to_string())
                        .collect();
                } else if name == "exit_lookback" {
                    *values = values
                        .iter()
                        .filter_map(|v| v.parse::<i32>().ok())
                        .map(|n| {
                            randomization::round_to_step_f64((n as f64) * exit_factor, 5.0).max(5.0)
                                as i32
                        })
                        .map(|n| n.to_string())
                        .collect();
                }
            }

            // Fetch date range: jitter start/end by 2%..40% of the default span.
            let min_date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
            fetch_range = jitter_date_range_percent(
                &mut rng,
                fetch_default_start,
                fetch_default_end,
                min_pct,
                max_pct,
                min_date,
                today,
                30,
            );
        }

        Self {
            active_panel: Panel::Data,
            data: data_state,
            strategy: strategy_state,
            sweep: sweep_state,
            results: ResultsState::default(),
            chart: ChartState {
                zoom_level: 1.0,
                ..Default::default()
            },
            help: HelpState::default(),
            status_message: "Welcome to TrendLab TUI. Press Tab to switch panels, ? for help."
                .to_string(),
            status_message_type: MessageType::Info,
            operation: OperationState::Idle,
            startup: StartupState::default(),
            auto: AutoRunState::default(),
            yolo: {
                // Load existing leaderboards on startup for persistence
                let all_time_per_symbol =
                    Leaderboard::load(std::path::Path::new("artifacts/leaderboard.json"))
                        .ok()
                        .unwrap_or_else(|| Leaderboard::new(4));
                let all_time_cross_symbol = CrossSymbolLeaderboard::load(std::path::Path::new(
                    "artifacts/cross_symbol_leaderboard.json",
                ))
                .ok();
                YoloState {
                    // Session leaderboards start fresh each launch
                    session_leaderboard: Leaderboard::new(10),
                    session_cross_symbol_leaderboard: None,
                    // All-time leaderboards loaded from disk
                    all_time_leaderboard: all_time_per_symbol,
                    all_time_cross_symbol_leaderboard: all_time_cross_symbol,
                    // Start viewing session by default
                    view_scope: LeaderboardScope::Session,
                    session_id: trendlab_core::generate_session_id(),
                    ..Default::default()
                }
            },
            random_defaults: RandomDefaults {
                enabled: random_enabled,
                seed,
            },
            fetch_range,
            fetch_range_default: (fetch_default_start, fetch_default_end),
        }
    }

    pub fn fetch_range(&self) -> (NaiveDate, NaiveDate) {
        self.fetch_range
    }

    /// Reset configurable lookbacks/params and fetch date range back to their canonical defaults.
    ///
    /// This does not change which strategies are selected/focused; it only resets values.
    pub fn reset_ui_defaults(&mut self) {
        self.strategy.donchian_config = DonchianConfig::default();
        self.strategy.ma_config = MACrossoverConfig::default();
        self.strategy.tsmom_config = TsmomConfig::default();
        self.strategy.keltner_config = KeltnerConfig::default();
        self.strategy.starc_config = STARCConfig::default();
        self.strategy.supertrend_config = SupertrendConfig::default();
        self.strategy.parabolic_sar_config = ParabolicSarConfig::default();
        self.strategy.opening_range_config = OpeningRangeConfig::default();

        // Reset ensemble horizons while preserving toggle + voting/base strategy.
        self.strategy.ensemble.horizons = EnsembleConfig::default().horizons;

        self.sweep.param_ranges = vec![
            (
                "entry_lookback".to_string(),
                vec!["10", "20", "30", "40", "50"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            ),
            (
                "exit_lookback".to_string(),
                vec!["5", "10", "15", "20", "25"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            ),
        ];

        self.fetch_range = self.fetch_range_default;
        self.status_message = "Reset defaults.".to_string();
    }

    pub fn next_panel(&mut self) {
        let panels = Panel::all();
        let current = panels
            .iter()
            .position(|&p| p == self.active_panel)
            .unwrap_or(0);
        let next = (current + 1) % panels.len();
        self.active_panel = panels[next];
    }

    pub fn prev_panel(&mut self) {
        let panels = Panel::all();
        let current = panels
            .iter()
            .position(|&p| p == self.active_panel)
            .unwrap_or(0);
        let prev = if current == 0 {
            panels.len() - 1
        } else {
            current - 1
        };
        self.active_panel = panels[prev];
    }

    pub fn select_panel(&mut self, index: usize) {
        let panels = Panel::all();
        if index < panels.len() {
            let target_panel = panels[index];
            // Context-sensitive Help: remember where we came from and set section
            if target_panel == Panel::Help && self.active_panel != Panel::Help {
                self.help.previous_panel = self.active_panel;
                self.help.active_section = HelpSection::from_panel(self.active_panel);
                self.help.scroll_offset = 0;
            }
            self.active_panel = target_panel;
        }
    }

    /// Toggle between sub-panels within Strategy panel
    /// Returns true if toggle was handled (i.e., we're in Strategy panel)
    pub fn toggle_strategy_focus(&mut self) -> bool {
        if self.active_panel == Panel::Strategy {
            match self.strategy.focus {
                StrategyFocus::Selection => {
                    // Don't toggle to params if strategy has no configurable params
                    let param_count = self.strategy.selected_type.param_count();
                    if param_count == 0 {
                        self.status_message = "This strategy has fixed parameters.".to_string();
                        return true;
                    }
                    self.strategy.focus = StrategyFocus::Parameters;
                    self.strategy.editing_strategy = false;
                }
                StrategyFocus::Parameters => {
                    self.strategy.focus = StrategyFocus::Selection;
                    self.strategy.editing_strategy = true;
                }
            }
            true
        } else {
            false
        }
    }

    /// Navigate up in the grouped strategy list.
    fn navigate_strategy_up(&mut self) {
        if self.strategy.focus_on_category {
            // Move to previous category
            if self.strategy.focused_category_index > 0 {
                self.strategy.focused_category_index -= 1;
                // If the new category is expanded and has strategies, go to last strategy
                if let Some(cat) = self.strategy.focused_category() {
                    if self.strategy.is_expanded(cat) && !cat.is_empty() {
                        self.strategy.focus_on_category = false;
                        self.strategy.focused_strategy_in_category = cat.len().saturating_sub(1);
                    }
                }
            }
        } else {
            // Currently on a strategy
            if self.strategy.focused_strategy_in_category > 0 {
                self.strategy.focused_strategy_in_category -= 1;
            } else {
                // Move to category header
                self.strategy.focus_on_category = true;
            }
        }
        // Update selected_type to match focused strategy (for parameter panel)
        self.sync_selected_type_to_focus();
    }

    /// Navigate down in the grouped strategy list.
    fn navigate_strategy_down(&mut self) {
        let num_categories = StrategyCategory::all().len();

        if self.strategy.focus_on_category {
            // Check if current category is expanded
            if let Some(cat) = self.strategy.focused_category() {
                if self.strategy.is_expanded(cat) && !cat.is_empty() {
                    // Move into the category's strategies
                    self.strategy.focus_on_category = false;
                    self.strategy.focused_strategy_in_category = 0;
                    // Update selected_type to match focused strategy (for parameter panel)
                    self.sync_selected_type_to_focus();
                    return;
                }
            }
            // Move to next category
            if self.strategy.focused_category_index < num_categories.saturating_sub(1) {
                self.strategy.focused_category_index += 1;
                self.strategy.focus_on_category = true;
            }
        } else {
            // Currently on a strategy
            if let Some(cat) = self.strategy.focused_category() {
                if self.strategy.focused_strategy_in_category < cat.len().saturating_sub(1) {
                    self.strategy.focused_strategy_in_category += 1;
                } else {
                    // Move to next category
                    if self.strategy.focused_category_index < num_categories.saturating_sub(1) {
                        self.strategy.focused_category_index += 1;
                        self.strategy.focus_on_category = true;
                    }
                }
            }
        }
        // Update selected_type to match focused strategy (for parameter panel)
        self.sync_selected_type_to_focus();
    }

    /// Sync selected_type to match the currently focused strategy.
    /// This keeps the parameter panel in sync as the user navigates.
    fn sync_selected_type_to_focus(&mut self) {
        if let Some(strat) = self.strategy.focused_strategy() {
            self.strategy.selected_type = strat;
            self.strategy.selected_type_index = StrategyType::all()
                .iter()
                .position(|s| *s == strat)
                .unwrap_or(0);
            self.strategy.selected_field = 0;
        }
    }

    /// Handle Space key in Strategy panel (toggle strategy checkbox).
    pub fn handle_strategy_space(&mut self) {
        if self.active_panel != Panel::Strategy {
            return;
        }
        if self.strategy.focus != StrategyFocus::Selection {
            return;
        }

        if self.strategy.focus_on_category {
            // Toggle all strategies in category
            if let Some(cat) = self.strategy.focused_category() {
                let all_selected = cat
                    .strategies()
                    .iter()
                    .all(|s| self.strategy.is_selected(*s));
                if all_selected {
                    self.strategy.deselect_all_in_category(cat);
                    self.status_message = format!("Deselected all in {}", cat.name());
                } else {
                    self.strategy.select_all_in_category(cat);
                    self.status_message = format!("Selected all in {}", cat.name());
                }
            }
        } else {
            // Toggle single strategy
            if let Some(strat) = self.strategy.focused_strategy() {
                self.strategy.toggle_strategy(strat);
                let selected = self.strategy.is_selected(strat);
                self.status_message = format!(
                    "{} {}",
                    if selected { "Selected" } else { "Deselected" },
                    strat.name()
                );
            }
        }
    }

    /// Handle Enter key in Strategy panel (expand/collapse category or select strategy for param editing).
    pub fn handle_strategy_enter(&mut self) {
        if self.active_panel != Panel::Strategy {
            return;
        }
        if self.strategy.focus != StrategyFocus::Selection {
            return;
        }

        if self.strategy.focus_on_category {
            // Toggle category expansion
            if let Some(cat) = self.strategy.focused_category() {
                self.strategy.toggle_category(cat);
                let expanded = self.strategy.is_expanded(cat);
                self.status_message = format!(
                    "{} {}",
                    if expanded { "Expanded" } else { "Collapsed" },
                    cat.name()
                );
            }
        } else {
            // Select strategy for parameter editing
            if let Some(strat) = self.strategy.focused_strategy() {
                self.strategy.selected_type = strat;
                self.strategy.selected_type_index = StrategyType::all()
                    .iter()
                    .position(|s| *s == strat)
                    .unwrap_or(0);
                self.strategy.selected_field = 0;

                // If strategy has params, switch to param editing
                if strat.param_count() > 0 {
                    self.strategy.focus = StrategyFocus::Parameters;
                    self.strategy.editing_strategy = false;
                    self.status_message = format!("Editing {} parameters", strat.name());
                } else {
                    self.status_message = format!("{} has fixed parameters", strat.name());
                }
            }
        }
    }

    /// Handle 'a' key in Strategy panel (select all in category).
    pub fn handle_strategy_select_all(&mut self) {
        if self.active_panel != Panel::Strategy {
            return;
        }
        if self.strategy.focus != StrategyFocus::Selection {
            return;
        }

        if let Some(cat) = self.strategy.focused_category() {
            self.strategy.select_all_in_category(cat);
            let count = cat.len();
            self.status_message = format!("Selected all {} strategies in {}", count, cat.name());
        }
    }

    /// Handle 'n' key in Strategy panel (deselect all in category).
    pub fn handle_strategy_select_none(&mut self) {
        if self.active_panel != Panel::Strategy {
            return;
        }
        if self.strategy.focus != StrategyFocus::Selection {
            return;
        }

        if let Some(cat) = self.strategy.focused_category() {
            self.strategy.deselect_all_in_category(cat);
            self.status_message = format!("Deselected all in {}", cat.name());
        }
    }

    /// Handle 'e' key in Strategy panel (toggle ensemble mode).
    pub fn handle_toggle_ensemble(&mut self) {
        if self.active_panel != Panel::Strategy {
            return;
        }

        self.strategy.ensemble.enabled = !self.strategy.ensemble.enabled;
        if self.strategy.ensemble.enabled {
            self.status_message = format!(
                "Ensemble mode ON ({} voting)",
                self.strategy.ensemble.voting.name()
            );
        } else {
            self.status_message = "Ensemble mode OFF".to_string();
        }
    }

    pub fn handle_up(&mut self) {
        match self.active_panel {
            Panel::Data => match self.data.view_mode {
                DataViewMode::Sectors => {
                    if self.data.selected_sector_index > 0 {
                        self.data.selected_sector_index -= 1;
                    }
                }
                DataViewMode::Tickers => {
                    if self.data.selected_ticker_index > 0 {
                        self.data.selected_ticker_index -= 1;
                    }
                }
            },
            Panel::Strategy => {
                if self.strategy.focus == StrategyFocus::Selection {
                    // Navigate grouped checkboxes
                    self.navigate_strategy_up();
                } else {
                    // Up/Down cycles through parameters (right panel)
                    let param_count = self.strategy.selected_type.param_count();
                    if param_count > 0 && self.strategy.selected_field > 0 {
                        self.strategy.selected_field -= 1;
                    }
                }
            }
            Panel::Sweep => {
                if self.sweep.selected_param > 0 {
                    self.sweep.selected_param -= 1;
                }
            }
            Panel::Results => {
                // Collapse expanded row when navigating
                self.results.expanded_leaderboard_index = None;

                match self.results.view_mode {
                    ResultsViewMode::Leaderboard => {
                        if self.results.selected_leaderboard_index > 0 {
                            self.results.selected_leaderboard_index -= 1;
                        }
                    }
                    _ => {
                        if self.results.selected_index > 0 {
                            self.results.selected_index -= 1;
                        }
                    }
                }
            }
            Panel::Chart => {
                self.chart.zoom_level = (self.chart.zoom_level * 1.1).min(4.0);
            }
            Panel::Help => {
                // Scroll up in help content
                if self.help.scroll_offset > 0 {
                    self.help.scroll_offset -= 1;
                }
            }
        }
    }

    pub fn handle_down(&mut self) {
        match self.active_panel {
            Panel::Data => match self.data.view_mode {
                DataViewMode::Sectors => {
                    let max_idx = self.data.universe.sector_count().saturating_sub(1);
                    if self.data.selected_sector_index < max_idx {
                        self.data.selected_sector_index += 1;
                    }
                }
                DataViewMode::Tickers => {
                    let max_idx = self.data.current_sector_tickers().len().saturating_sub(1);
                    if self.data.selected_ticker_index < max_idx {
                        self.data.selected_ticker_index += 1;
                    }
                }
            },
            Panel::Strategy => {
                if self.strategy.focus == StrategyFocus::Selection {
                    // Navigate grouped checkboxes
                    self.navigate_strategy_down();
                } else {
                    // Up/Down cycles through parameters (right panel)
                    let param_count = self.strategy.selected_type.param_count();
                    if param_count > 0 && self.strategy.selected_field < param_count - 1 {
                        self.strategy.selected_field += 1;
                    }
                }
            }
            Panel::Sweep => {
                if !self.sweep.param_ranges.is_empty()
                    && self.sweep.selected_param < self.sweep.param_ranges.len() - 1
                {
                    self.sweep.selected_param += 1;
                }
            }
            Panel::Results => {
                // Collapse expanded row when navigating
                self.results.expanded_leaderboard_index = None;

                match self.results.view_mode {
                    ResultsViewMode::Leaderboard => {
                        let max_idx = self
                            .yolo
                            .cross_symbol_leaderboard()
                            .map(|lb| lb.entries.len().saturating_sub(1))
                            .unwrap_or(0);
                        if self.results.selected_leaderboard_index < max_idx {
                            self.results.selected_leaderboard_index += 1;
                        }
                    }
                    _ => {
                        if !self.results.results.is_empty()
                            && self.results.selected_index < self.results.results.len() - 1
                        {
                            self.results.selected_index += 1;
                        }
                    }
                }
            }
            Panel::Chart => {
                self.chart.zoom_level = (self.chart.zoom_level / 1.1).max(0.25);
            }
            Panel::Help => {
                // Scroll down in help content
                self.help.scroll_offset += 1;
                // Max scroll will be clamped in draw function
            }
        }
    }

    pub fn handle_left(&mut self) {
        match self.active_panel {
            Panel::Data => {
                // Left arrow goes back from tickers to sectors
                if self.data.view_mode == DataViewMode::Tickers {
                    self.data.view_mode = DataViewMode::Sectors;
                    self.status_message = "Sector view".to_string();
                }
            }
            Panel::Strategy => {
                self.adjust_strategy_param(-1);
            }
            Panel::Chart => {
                if self.chart.scroll_offset > 0 {
                    self.chart.scroll_offset -= 1;
                }
            }
            Panel::Results => {
                // Collapse leaderboard row in Leaderboard view
                if self.results.view_mode == ResultsViewMode::Leaderboard {
                    self.collapse_leaderboard_row();
                }
            }
            Panel::Help => {
                // Previous section
                self.help.active_section = self.help.active_section.prev();
                self.help.scroll_offset = 0;
            }
            _ => {}
        }
    }

    pub fn handle_right(&mut self) {
        match self.active_panel {
            Panel::Data => {
                // Right arrow goes into ticker view for selected sector
                if self.data.view_mode == DataViewMode::Sectors {
                    self.data.view_mode = DataViewMode::Tickers;
                    self.data.selected_ticker_index = 0;
                    if let Some(sector) = self.data.selected_sector() {
                        self.status_message = format!("{} tickers", sector.name);
                    }
                }
            }
            Panel::Strategy => {
                self.adjust_strategy_param(1);
            }
            Panel::Chart => {
                self.chart.scroll_offset += 1;
            }
            Panel::Results => {
                // Expand leaderboard row in Leaderboard view
                if self.results.view_mode == ResultsViewMode::Leaderboard {
                    self.expand_leaderboard_row();
                }
            }
            Panel::Help => {
                // Next section
                self.help.active_section = self.help.active_section.next();
                self.help.scroll_offset = 0;
            }
            _ => {}
        }
    }

    /// Expand the currently selected leaderboard row to show details.
    pub fn expand_leaderboard_row(&mut self) {
        if self.results.view_mode == ResultsViewMode::Leaderboard {
            self.results.expanded_leaderboard_index = Some(self.results.selected_leaderboard_index);
        }
    }

    /// Collapse any expanded leaderboard row.
    pub fn collapse_leaderboard_row(&mut self) {
        self.results.expanded_leaderboard_index = None;
    }

    /// Check if a leaderboard row is expanded.
    pub fn is_leaderboard_expanded(&self) -> bool {
        self.results.expanded_leaderboard_index.is_some()
    }

    // =========================================================================
    // Help Panel Navigation
    // =========================================================================

    /// Page down in Help panel (half screen)
    pub fn help_page_down(&mut self) {
        self.help.scroll_offset += 10;
    }

    /// Page up in Help panel (half screen)
    pub fn help_page_up(&mut self) {
        self.help.scroll_offset = self.help.scroll_offset.saturating_sub(10);
    }

    /// Jump to bottom of Help content
    pub fn help_jump_to_bottom(&mut self) {
        // Set a high value; draw function will clamp
        self.help.scroll_offset = usize::MAX / 2;
    }

    /// Move to next search match in Help
    pub fn help_next_match(&mut self) {
        if !self.help.search_matches.is_empty() {
            self.help.search_index = (self.help.search_index + 1) % self.help.search_matches.len();
            if let Some(&line) = self.help.search_matches.get(self.help.search_index) {
                self.help.scroll_offset = line;
            }
        }
    }

    /// Move to previous search match in Help
    pub fn help_prev_match(&mut self) {
        if !self.help.search_matches.is_empty() {
            if self.help.search_index == 0 {
                self.help.search_index = self.help.search_matches.len() - 1;
            } else {
                self.help.search_index -= 1;
            }
            if let Some(&line) = self.help.search_matches.get(self.help.search_index) {
                self.help.scroll_offset = line;
            }
        }
    }

    /// Update search matches for Help panel (called on each keystroke)
    pub fn update_help_search_matches(&mut self) {
        // For simplicity, we just track that there might be matches
        // The actual match highlighting is done in the draw function
        self.help.search_matches.clear();
        self.help.search_index = 0;

        // In a full implementation, we'd scan the help content here
        // For now, just indicate search is active if query is non-empty
        if !self.help.search_query.is_empty() {
            // Placeholder - matches will be found during rendering
            // The draw function handles highlighting
        }
    }

    fn adjust_strategy_param(&mut self, delta: i32) {
        let field = self.strategy.selected_field;

        match self.strategy.selected_type {
            StrategyType::Donchian => match field {
                0 => {
                    let new_val = (self.strategy.donchian_config.entry_lookback as i32 + delta * 5)
                        .max(5) as usize;
                    self.strategy.donchian_config.entry_lookback = new_val;
                }
                1 => {
                    let new_val = (self.strategy.donchian_config.exit_lookback as i32 + delta * 5)
                        .max(5) as usize;
                    self.strategy.donchian_config.exit_lookback = new_val;
                }
                _ => {}
            },
            StrategyType::TurtleS1 | StrategyType::TurtleS2 => {
                // Fixed params, no adjustment
            }
            StrategyType::MACrossover => {
                match field {
                    0 => {
                        let new_val = (self.strategy.ma_config.fast_period as i32 + delta * 5)
                            .max(5) as usize;
                        // Ensure fast < slow
                        if new_val < self.strategy.ma_config.slow_period {
                            self.strategy.ma_config.fast_period = new_val;
                        }
                    }
                    1 => {
                        let new_val = (self.strategy.ma_config.slow_period as i32 + delta * 10)
                            .max(10) as usize;
                        // Ensure slow > fast
                        if new_val > self.strategy.ma_config.fast_period {
                            self.strategy.ma_config.slow_period = new_val;
                        }
                    }
                    2 => {
                        // Toggle MA type
                        self.strategy.ma_config.ma_type = if self.strategy.ma_config.ma_type == 0 {
                            1
                        } else {
                            0
                        };
                    }
                    _ => {}
                }
            }
            StrategyType::Tsmom => {
                if field == 0 {
                    let new_val =
                        (self.strategy.tsmom_config.lookback as i32 + delta * 21).max(21) as usize;
                    self.strategy.tsmom_config.lookback = new_val;
                }
            }
            StrategyType::Keltner => match field {
                0 => {
                    let new_val = (self.strategy.keltner_config.ema_period as i32 + delta * 5)
                        .max(5) as usize;
                    self.strategy.keltner_config.ema_period = new_val;
                }
                1 => {
                    let new_val = (self.strategy.keltner_config.atr_period as i32 + delta * 5)
                        .max(5) as usize;
                    self.strategy.keltner_config.atr_period = new_val;
                }
                2 => {
                    let new_val =
                        (self.strategy.keltner_config.multiplier + delta as f64 * 0.5).max(0.5);
                    self.strategy.keltner_config.multiplier = new_val;
                }
                _ => {}
            },
            StrategyType::STARC => match field {
                0 => {
                    let new_val =
                        (self.strategy.starc_config.sma_period as i32 + delta * 5).max(5) as usize;
                    self.strategy.starc_config.sma_period = new_val;
                }
                1 => {
                    let new_val =
                        (self.strategy.starc_config.atr_period as i32 + delta * 5).max(5) as usize;
                    self.strategy.starc_config.atr_period = new_val;
                }
                2 => {
                    let new_val =
                        (self.strategy.starc_config.multiplier + delta as f64 * 0.5).max(0.5);
                    self.strategy.starc_config.multiplier = new_val;
                }
                _ => {}
            },
            StrategyType::Supertrend => match field {
                0 => {
                    let new_val = (self.strategy.supertrend_config.atr_period as i32 + delta * 5)
                        .max(5) as usize;
                    self.strategy.supertrend_config.atr_period = new_val;
                }
                1 => {
                    let new_val =
                        (self.strategy.supertrend_config.multiplier + delta as f64 * 0.5).max(0.5);
                    self.strategy.supertrend_config.multiplier = new_val;
                }
                _ => {}
            },
            StrategyType::ParabolicSar => match field {
                0 => {
                    let new_val = (self.strategy.parabolic_sar_config.af_start
                        + delta as f64 * 0.01)
                        .max(0.01);
                    self.strategy.parabolic_sar_config.af_start = new_val;
                }
                1 => {
                    let new_val = (self.strategy.parabolic_sar_config.af_step
                        + delta as f64 * 0.01)
                        .max(0.01);
                    self.strategy.parabolic_sar_config.af_step = new_val;
                }
                2 => {
                    let new_val =
                        (self.strategy.parabolic_sar_config.af_max + delta as f64 * 0.05).max(0.05);
                    self.strategy.parabolic_sar_config.af_max = new_val;
                }
                _ => {}
            },
            StrategyType::OpeningRange => match field {
                0 => {
                    let new_val = (self.strategy.opening_range_config.range_bars as i32 + delta)
                        .max(1) as usize;
                    self.strategy.opening_range_config.range_bars = new_val;
                }
                1 => {
                    // Cycle through periods: 0=Weekly, 1=Monthly, 2=Rolling
                    let current = self.strategy.opening_range_config.period;
                    self.strategy.opening_range_config.period = if delta > 0 {
                        (current + 1) % 3
                    } else {
                        (current + 2) % 3
                    };
                }
                _ => {}
            },
        }
    }

    /// Handle Enter key with access to worker channels
    pub fn handle_enter_with_channels(&mut self, channels: &WorkerChannels) {
        match self.active_panel {
            Panel::Data => {
                // Load bars for selected symbol from Parquet
                if let Some(symbol) = self.data.selected_symbol().cloned() {
                    self.load_bars_for_symbol(&symbol);
                }
            }
            Panel::Sweep => {
                if !self.sweep.is_running {
                    // If user selected multiple tickers, run multi-sweep; otherwise run single.
                    if self.data.selected_tickers.len() >= 2 {
                        self.start_multi_sweep(channels);
                    } else {
                        self.start_single_sweep(channels);
                    }
                }
            }
            Panel::Results => {
                if !self.results.results.is_empty() {
                    self.chart.selected_result_index = Some(self.results.selected_index);
                    if let Some(result) = self.results.results.get(self.results.selected_index) {
                        // Extract equity curve from backtest result
                        self.chart.equity_curve = result
                            .backtest_result
                            .equity
                            .iter()
                            .map(|p| p.equity)
                            .collect();
                        // Extract dates from backtest result
                        self.chart.equity_dates =
                            result.backtest_result.equity.iter().map(|p| p.ts).collect();
                        // Calculate drawdown curve
                        self.chart.drawdown_curve = calculate_drawdown(&self.chart.equity_curve);
                        // Set winning config for Pine export display
                        self.chart.winning_config = Some(WinningConfig {
                            strategy_name: self.strategy.selected_type.name().to_string(),
                            config_display: self.strategy.config_display_string(),
                            symbol: self.data.selected_symbol().cloned(),
                        });
                    }
                    self.active_panel = Panel::Chart;
                }
            }
            _ => {}
        }
    }

    pub fn handle_escape(&mut self) {
        match self.active_panel {
            Panel::Sweep => {
                if self.sweep.is_running {
                    self.sweep.is_running = false;
                    self.status_message = "Sweep cancelled.".to_string();
                }
            }
            Panel::Chart => {
                self.chart.zoom_level = 1.0;
                self.chart.scroll_offset = 0;
            }
            _ => {}
        }
    }

    /// Handle 'f' key to fetch data
    pub fn handle_fetch(&mut self, channels: &WorkerChannels) {
        if self.active_panel != Panel::Data {
            return;
        }

        // Determine which symbols to fetch
        let symbols = if !self.data.selected_tickers.is_empty() {
            // Fetch all selected tickers
            self.data.selected_tickers_sorted()
        } else if self.data.view_mode == DataViewMode::Tickers {
            // Fetch the focused ticker
            if let Some(ticker) = self.data.focused_ticker() {
                vec![ticker.clone()]
            } else {
                return;
            }
        } else if let Some(sector) = self.data.selected_sector() {
            // Fetch all tickers in selected sector
            sector.tickers.clone()
        } else {
            return;
        };

        if symbols.is_empty() {
            self.status_message = "No symbols to fetch.".to_string();
            return;
        }

        let (start, end) = self.fetch_range();
        let cmd = WorkerCommand::FetchData {
            symbols: symbols.clone(),
            start,
            end,
            force: false,
        };

        if channels.command_tx.send(cmd).is_ok() {
            self.status_message = format!(
                "Fetching {} symbols ({} to {})...",
                symbols.len(),
                start.format("%Y-%m-%d"),
                end.format("%Y-%m-%d")
            );
        }
    }

    /// Handle 's' key to cycle sort column in results panel
    pub fn handle_sort(&mut self) {
        if self.active_panel != Panel::Results {
            return;
        }

        // Cycle through sort columns: CAGR, Sharpe, MaxDD, Trades
        self.results.sort_column = (self.results.sort_column + 1) % 4;

        // Sort the results
        self.sort_results();

        let column_names = ["CAGR", "Sharpe", "Max DD", "Trades"];
        self.status_message = format!("Sorted by {}", column_names[self.results.sort_column]);
    }

    /// Sort results by current sort column
    fn sort_results(&mut self) {
        match self.results.sort_column {
            0 => {
                // CAGR (descending)
                self.results.results.sort_by(|a, b| {
                    b.metrics
                        .cagr
                        .partial_cmp(&a.metrics.cagr)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            1 => {
                // Sharpe (descending)
                self.results.results.sort_by(|a, b| {
                    b.metrics
                        .sharpe
                        .partial_cmp(&a.metrics.sharpe)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            2 => {
                // Max DD (ascending - less negative is better)
                self.results.results.sort_by(|a, b| {
                    b.metrics
                        .max_drawdown
                        .partial_cmp(&a.metrics.max_drawdown)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            3 => {
                // Trades (descending)
                self.results
                    .results
                    .sort_by(|a, b| b.metrics.num_trades.cmp(&a.metrics.num_trades));
            }
            _ => {}
        }

        // Reset selection to first item
        self.results.selected_index = 0;
    }

    /// Handle 'd' key to toggle drawdown overlay in chart panel
    pub fn handle_toggle_drawdown(&mut self) {
        if self.active_panel != Panel::Chart {
            return;
        }

        self.chart.show_drawdown = !self.chart.show_drawdown;
        self.status_message = if self.chart.show_drawdown {
            "Drawdown overlay ON".to_string()
        } else {
            "Drawdown overlay OFF".to_string()
        };
    }

    /// Handle 'v' key to toggle results view mode (per-ticker vs aggregated)
    pub fn handle_toggle_view(&mut self) {
        if self.active_panel != Panel::Results {
            return;
        }

        self.results.cycle_view_mode();
        self.status_message = format!("View: {}", self.results.view_mode_name());
    }

    /// Handle 'm' key to toggle chart view mode (single vs multi-ticker vs portfolio vs candlestick)
    pub fn handle_toggle_chart_mode(&mut self) {
        if self.active_panel != Panel::Chart {
            return;
        }

        // Allow cycling if we have any data (multi-curves, candle data, or strategy curves)
        if !self.chart.has_multi_curves()
            && !self.chart.has_candle_data()
            && !self.chart.has_strategy_curves()
        {
            self.status_message = "Load data first.".to_string();
            return;
        }

        self.chart.cycle_view_mode();
        self.status_message = format!("Chart: {}", self.chart.view_mode_name());
    }

    /// Handle 'v' key in Chart panel to toggle volume subplot
    pub fn handle_toggle_volume(&mut self) {
        if self.active_panel != Panel::Chart {
            return;
        }

        self.chart.show_volume = !self.chart.show_volume;
        self.status_message = if self.chart.show_volume {
            "Volume subplot ON".to_string()
        } else {
            "Volume subplot OFF".to_string()
        };
    }

    /// Handle 'c' key in Chart panel to toggle crosshair
    pub fn handle_toggle_crosshair(&mut self) {
        if self.active_panel != Panel::Chart {
            return;
        }

        self.chart.show_crosshair = !self.chart.show_crosshair;
        self.status_message = if self.chart.show_crosshair {
            "Crosshair ON".to_string()
        } else {
            "Crosshair OFF".to_string()
        };
    }

    /// Update cursor position from mouse event
    pub fn update_cursor_position(&mut self, col: u16, row: u16) {
        self.chart.cursor.terminal_pos = Some((col, row));

        // Check if cursor is within chart area
        // Using lock() for Mutex-based chart_area (thread-safe for GUI)
        let chart_area_opt = self.chart.chart_area.lock().ok().and_then(|g| *g);
        if let Some(chart_area) = chart_area_opt {
            if col >= chart_area.x
                && col < chart_area.x + chart_area.width
                && row >= chart_area.y
                && row < chart_area.y + chart_area.height
            {
                let rel_x = col - chart_area.x;
                let rel_y = row - chart_area.y;
                self.chart.cursor.chart_pos = Some((rel_x, rel_y));
                self.chart.cursor.in_chart = true;

                // Calculate data index from x position
                self.chart.cursor.data_index = self.calculate_data_index(rel_x, chart_area.width);
            } else {
                self.chart.cursor.in_chart = false;
                self.chart.cursor.chart_pos = None;
                self.chart.cursor.data_index = None;
            }
        }
    }

    /// Calculate which data point is under the cursor
    fn calculate_data_index(&self, rel_x: u16, chart_width: u16) -> Option<usize> {
        let data_len = match self.chart.view_mode {
            ChartViewMode::Single => self.chart.equity_curve.len(),
            ChartViewMode::Portfolio => self.chart.portfolio_curve.len(),
            ChartViewMode::Candlestick => self.chart.candle_data.len(),
            ChartViewMode::MultiTicker => self
                .chart
                .ticker_curves
                .first()
                .map(|c| c.equity.len())
                .unwrap_or(0),
            _ => self.chart.equity_curve.len(),
        };

        if data_len == 0 || chart_width == 0 {
            return None;
        }

        // Account for zoom and scroll
        let visible_range = (data_len as f64 / self.chart.zoom_level) as usize;
        let start_idx = self.chart.scroll_offset;
        let ratio = rel_x as f64 / chart_width as f64;
        let idx = start_idx + (ratio * visible_range as f64) as usize;

        if idx < data_len {
            Some(idx)
        } else {
            None
        }
    }

    /// Tick animations (called every frame)
    pub fn tick_animations(&mut self) {
        if self.chart.animation.animating {
            // Ease-out interpolation (smooth deceleration)
            let ease = 0.15;

            let zoom_diff = self.chart.animation.target_zoom - self.chart.zoom_level;
            let scroll_diff = self.chart.animation.target_scroll - self.chart.scroll_offset as f64;

            if zoom_diff.abs() < 0.001 && scroll_diff.abs() < 0.5 {
                // Close enough, snap to target
                self.chart.zoom_level = self.chart.animation.target_zoom;
                self.chart.scroll_offset = self.chart.animation.target_scroll as usize;
                self.chart.animation.animating = false;
            } else {
                // Interpolate
                self.chart.zoom_level += zoom_diff * ease;
                self.chart.scroll_offset =
                    (self.chart.scroll_offset as f64 + scroll_diff * ease) as usize;
            }
        }
    }

    /// Handle Space key to toggle ticker selection in Data panel.
    pub fn handle_space(&mut self) {
        if self.active_panel != Panel::Data {
            return;
        }

        if self.data.view_mode == DataViewMode::Tickers {
            if let Some(ticker) = self.data.focused_ticker().cloned() {
                self.data.toggle_ticker_selection(&ticker);
                let selected_count = self.data.selected_tickers.len();
                self.status_message = format!(
                    "{} {} ({} selected)",
                    if self.data.is_ticker_selected(&ticker) {
                        "Selected"
                    } else {
                        "Deselected"
                    },
                    ticker,
                    selected_count
                );
            }
        }
    }

    /// Handle 'a' key to select all tickers.
    /// In Sectors view: selects ALL tickers globally (YOLO mode).
    /// In Tickers view: selects all tickers in current sector.
    pub fn handle_select_all(&mut self) {
        if self.active_panel != Panel::Data {
            return;
        }

        if self.data.view_mode == DataViewMode::Sectors {
            // Global select all for YOLO mode
            self.data.select_all();
            let total: usize = self
                .data
                .universe
                .sectors
                .iter()
                .map(|s| s.tickers.len())
                .sum();
            self.status_message = format!("Selected all {} tickers (YOLO mode)", total);
        } else {
            // Select all in current sector
            self.data.select_all_in_sector();
            if let Some(sector) = self.data.selected_sector() {
                let count = sector.len();
                self.status_message = format!("Selected all {} tickers in {}", count, sector.name);
            }
        }
    }

    /// Handle 'n' key to deselect all tickers.
    /// In Sectors view: deselects ALL tickers globally.
    /// In Tickers view: deselects all tickers in current sector.
    pub fn handle_select_none(&mut self) {
        if self.active_panel != Panel::Data {
            return;
        }

        if self.data.view_mode == DataViewMode::Sectors {
            // Global deselect
            self.data.deselect_all();
            self.status_message = "Deselected all tickers".to_string();
        } else {
            // Deselect in current sector
            self.data.deselect_all_in_sector();
            if let Some(sector) = self.data.selected_sector() {
                self.status_message = format!("Deselected all tickers in {}", sector.name);
            }
        }
    }

    /// Handle 'a' key in Results panel to toggle analysis view.
    pub fn handle_toggle_analysis(&mut self, channels: &WorkerChannels) {
        if self.active_panel != Panel::Results {
            return;
        }

        // Toggle show_analysis
        self.results.show_analysis = !self.results.show_analysis;

        if self.results.show_analysis {
            // Check if we have a selected result to analyze
            if let Some(result) = self.results.results.get(self.results.selected_index) {
                // Create an analysis_id based on the config
                let analysis_id = format!(
                    "entry{}exit{}",
                    result.config_id.entry_lookback, result.config_id.exit_lookback
                );

                self.results.selected_analysis_id = Some(analysis_id.clone());

                // Check if analysis is already cached
                if let Some(cached) = self.results.analysis_cache.get(&analysis_id) {
                    self.results.selected_analysis = Some(cached.clone());
                    self.status_message = format!("Showing analysis for {}", analysis_id);
                } else {
                    // Need to compute analysis - find the bars for the symbol
                    if let Some(symbol) = self.data.selected_symbol() {
                        if let Some(bars) = self.data.bars_cache.get(symbol) {
                            // Send compute command to worker
                            let _ = channels.command_tx.send(WorkerCommand::ComputeAnalysis {
                                analysis_id: analysis_id.clone(),
                                backtest_result: result.backtest_result.clone(),
                                bars: Arc::new(bars.clone()),
                                config: trendlab_core::AnalysisConfig::default(),
                            });
                            self.status_message =
                                format!("Computing analysis for {}...", analysis_id);
                        } else {
                            self.status_message =
                                "Cannot compute analysis: no bars loaded for symbol".to_string();
                            self.results.show_analysis = false;
                        }
                    } else {
                        self.status_message =
                            "Cannot compute analysis: no symbol selected".to_string();
                        self.results.show_analysis = false;
                    }
                }
            } else {
                self.status_message = "No result selected for analysis".to_string();
                self.results.show_analysis = false;
            }
        } else {
            self.results.selected_analysis = None;
            self.results.selected_analysis_id = None;
            self.status_message = "Analysis view closed".to_string();
        }
    }

    /// Handle 'P' key in Results panel to export Pine Script for selected config.
    ///
    /// This generates a Pine Script v6 file for the currently selected strategy
    /// configuration and saves it to `pine-scripts/strategies/<strategy>/<config>.pine`.
    pub fn handle_pine_export(&mut self) {
        if self.active_panel != Panel::Results {
            return;
        }

        // Get selected leaderboard entry in Leaderboard view
        if self.results.view_mode == ResultsViewMode::Leaderboard {
            if let Some(lb) = self.yolo.cross_symbol_leaderboard() {
                if let Some(entry) = lb.entries.get(self.results.selected_leaderboard_index) {
                    // Extract strategy info from leaderboard entry
                    let strategy_id = entry.config_id.strategy_type().id();
                    let config_display = entry.config_id.display();

                    // Build config filename from display (e.g., "52wk High 80/70%/59%" -> "80_70_59")
                    // Include Sharpe in filename for uniqueness across different runs
                    let config_params = config_display
                        .split_whitespace()
                        .last()
                        .unwrap_or("default")
                        .replace('/', "_")
                        .replace('%', "")
                        .replace('.', "_");

                    // Add Sharpe to filename (e.g., "8_3_0_s0.42")
                    // Same params + same Sharpe = truly identical config, safe to overwrite
                    let sharpe = entry.aggregate_metrics.avg_sharpe;
                    let config_filename = format!("{}_s{:.2}", config_params, sharpe);

                    let output_dir =
                        std::path::PathBuf::from("pine-scripts/strategies").join(strategy_id);
                    let output_file = output_dir.join(format!("{}.pine", config_filename));

                    // Create directory if needed
                    if let Err(e) = std::fs::create_dir_all(&output_dir) {
                        self.status_message = format!("Failed to create directory: {}", e);
                        return;
                    }

                    // Generate Pine script content with performance metrics
                    let pine_content = entry.config_id.to_pine_script(
                        Some(entry.aggregate_metrics.avg_sharpe),
                        Some(entry.aggregate_metrics.hit_rate),
                        Some(entry.symbols.len()),
                    );

                    // Check if this is a stub (strategy not yet implemented)
                    if pine_content.contains("not yet implemented") {
                        self.status_message = format!(
                            "Pine generation not implemented for {} - use /pine:generate",
                            entry.config_id.strategy_type().name()
                        );
                        return;
                    }

                    // Write the file
                    match std::fs::write(&output_file, &pine_content) {
                        Ok(_) => {
                            self.status_message =
                                format!("Pine script saved: {}", output_file.display());
                        }
                        Err(e) => {
                            self.status_message = format!("Failed to write Pine script: {}", e);
                        }
                    }
                    return;
                }
            }
            self.status_message = "No leaderboard entry selected".to_string();
        } else {
            self.status_message =
                "Pine export available in Leaderboard view (press 'v' to switch)".to_string();
        }
    }

    /// Load bars for a symbol from Parquet cache
    fn load_bars_for_symbol(&mut self, symbol: &str) {
        use trendlab_core::read_parquet;

        let parquet_dir = std::path::Path::new("data/parquet/1d");
        let symbol_dir = parquet_dir.join(format!("symbol={}", symbol));

        if !symbol_dir.exists() {
            self.status_message = format!("No data for {}. Press 'f' to fetch.", symbol);
            return;
        }

        let mut all_bars = Vec::new();

        // Load all year partitions
        if let Ok(entries) = std::fs::read_dir(&symbol_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let data_file = path.join("data.parquet");
                    if data_file.exists() {
                        match read_parquet(&data_file) {
                            Ok(bars) => all_bars.extend(bars),
                            Err(e) => {
                                self.status_message =
                                    format!("Error reading {}: {}", data_file.display(), e);
                                return;
                            }
                        }
                    }
                }
            }
        }

        if all_bars.is_empty() {
            self.status_message = format!("No bars found for {}", symbol);
            return;
        }

        // Sort by timestamp
        all_bars.sort_by_key(|b| b.ts);

        // Update state
        let bar_count = all_bars.len();
        let date_range = if !all_bars.is_empty() {
            let start = all_bars.first().unwrap().ts.format("%Y-%m-%d").to_string();
            let end = all_bars.last().unwrap().ts.format("%Y-%m-%d").to_string();
            Some((start, end))
        } else {
            None
        };

        self.data.bar_count = bar_count;
        self.data.date_range = date_range.clone();

        // Also update candle data for chart candlestick view
        self.chart.update_candle_data(&all_bars, symbol);

        self.data.bars_cache.insert(symbol.to_string(), all_bars);

        if let Some((start, end)) = date_range {
            self.status_message = format!("{}: {} bars ({} to {})", symbol, bar_count, start, end);
        } else {
            self.status_message = format!("{}: {} bars loaded", symbol, bar_count);
        }
    }

    /// Update data info for selected symbol
    #[allow(dead_code)]
    fn update_data_info(&mut self) {
        if let Some(symbol) = self.data.selected_symbol() {
            if let Some(bars) = self.data.bars_cache.get(symbol) {
                self.data.bar_count = bars.len();
                if !bars.is_empty() {
                    let start = bars.first().unwrap().ts.format("%Y-%m-%d").to_string();
                    let end = bars.last().unwrap().ts.format("%Y-%m-%d").to_string();
                    self.data.date_range = Some((start, end));
                }
            } else {
                self.data.bar_count = 0;
                self.data.date_range = None;
            }
        }
    }

    /// Start a single-symbol sweep using the currently selected symbol's bars.
    pub fn start_single_sweep(&mut self, channels: &WorkerChannels) {
        // Check if we have data loaded
        if let Some(bars) = self.data.selected_bars() {
            if bars.is_empty() {
                self.status_message =
                    "No bars loaded. Select a symbol and press Enter in Data panel.".to_string();
                return;
            }

            let grid = match self.sweep.to_sweep_grid() {
                Ok(g) => g,
                Err(msg) => {
                    self.status_message = msg;
                    return;
                }
            };
            let backtest_config = BacktestConfig {
                initial_cash: 100_000.0,
                fill_model: FillModel::NextOpen,
                cost_model: CostModel {
                    fees_bps_per_side: 10.0,
                    slippage_bps: 5.0,
                },
                qty: 100.0,
                pyramid_config: PyramidConfig::default(),
            };

            // Send sweep command to worker
            // Use Polars by default for Donchian sweeps (vectorized, faster)
            let cmd = WorkerCommand::StartSweep {
                bars: Arc::new(bars.clone()),
                grid,
                backtest_config,
                use_polars: true,
            };

            if channels.command_tx.send(cmd).is_ok() {
                self.sweep.is_running = true;
                self.sweep.progress = 0.0;
                self.status_message = "Starting sweep...".to_string();
            } else {
                self.status_message = "Failed to start sweep.".to_string();
            }
        } else {
            self.status_message = "Load data first! Go to Data panel and press Enter.".to_string();
        }
    }

    /// Start a multi-symbol sweep using selected tickers' cached bars.
    pub fn start_multi_sweep(&mut self, channels: &WorkerChannels) {
        let selected = self.data.selected_tickers_sorted();
        if selected.len() < 2 {
            self.status_message = "Select at least 2 tickers first (Data panel).".to_string();
            return;
        }

        // Build symbol->bars map from cache
        let mut symbol_bars: HashMap<String, Arc<Vec<Bar>>> = HashMap::new();
        let mut missing: Vec<String> = Vec::new();
        for sym in &selected {
            if let Some(bars) = self.data.bars_cache.get(sym) {
                if !bars.is_empty() {
                    symbol_bars.insert(sym.clone(), Arc::new(bars.clone()));
                } else {
                    missing.push(sym.clone());
                }
            } else {
                missing.push(sym.clone());
            }
        }

        if symbol_bars.len() < 2 {
            self.status_message = format!(
                "Need bars for >= 2 selected tickers. Missing: {}",
                missing.join(", ")
            );
            return;
        }

        let grid = match self.sweep.to_sweep_grid() {
            Ok(g) => g,
            Err(msg) => {
                self.status_message = msg;
                return;
            }
        };
        let backtest_config = BacktestConfig {
            initial_cash: 100_000.0,
            fill_model: FillModel::NextOpen,
            cost_model: CostModel {
                fees_bps_per_side: 10.0,
                slippage_bps: 5.0,
            },
            qty: 100.0,
            pyramid_config: PyramidConfig::default(),
        };

        let cmd = WorkerCommand::StartMultiSweep {
            symbol_bars,
            grid,
            backtest_config,
        };

        if channels.command_tx.send(cmd).is_ok() {
            self.sweep.is_running = true;
            self.sweep.progress = 0.0;
            self.status_message = format!("Starting multi-sweep ({} symbols)...", selected.len());
        } else {
            self.status_message = "Failed to start multi-sweep.".to_string();
        }
    }

    /// Start a multi-strategy sweep (all strategies) using selected tickers' cached bars.
    pub fn start_multi_strategy_sweep(&mut self, channels: &WorkerChannels) {
        let selected = self.data.selected_tickers_sorted();
        if selected.is_empty() {
            self.status_message = "No tickers selected.".to_string();
            return;
        }

        // Build symbol->bars map from cache
        let mut symbol_bars: HashMap<String, Arc<Vec<Bar>>> = HashMap::new();
        let mut missing: Vec<String> = Vec::new();
        for sym in &selected {
            if let Some(bars) = self.data.bars_cache.get(sym) {
                if !bars.is_empty() {
                    symbol_bars.insert(sym.clone(), Arc::new(bars.clone()));
                } else {
                    missing.push(sym.clone());
                }
            } else {
                missing.push(sym.clone());
            }
        }

        if symbol_bars.is_empty() {
            self.status_message = format!(
                "No bars loaded for selected tickers. Missing: {}",
                missing.join(", ")
            );
            return;
        }

        // Use selected sweep depth from startup modal
        let strategy_grid = MultiStrategyGrid::with_depth(self.startup.sweep_depth);
        let backtest_config = BacktestConfig {
            initial_cash: 100_000.0,
            fill_model: FillModel::NextOpen,
            cost_model: CostModel {
                fees_bps_per_side: 10.0,
                slippage_bps: 5.0,
            },
            qty: 100.0,
            pyramid_config: PyramidConfig::default(),
        };

        let cmd = WorkerCommand::StartMultiStrategySweep {
            symbol_bars,
            strategy_grid,
            backtest_config,
        };

        if channels.command_tx.send(cmd).is_ok() {
            self.sweep.is_running = true;
            self.sweep.progress = 0.0;
            self.status_message = format!(
                "Starting multi-strategy sweep ({} symbols, all strategies)...",
                selected.len()
            );
        } else {
            self.status_message = "Failed to start multi-strategy sweep.".to_string();
        }
    }

    /// Kick off full-auto mode: select all universe tickers, load cached data, fetch missing,
    /// then run a multi-sweep and jump to the combined chart.
    pub fn start_full_auto(&mut self, channels: &WorkerChannels) {
        // Select all tickers across the universe
        let mut all: Vec<String> = Vec::new();
        for sector in self.data.universe.sectors.iter() {
            all.extend(sector.tickers.iter().cloned());
        }
        all.sort();
        all.dedup();

        if all.is_empty() {
            self.status_message = "Universe has no tickers to run.".to_string();
            return;
        }

        self.data.selected_tickers.clear();
        for sym in &all {
            self.data.selected_tickers.insert(sym.clone());
        }

        self.auto.enabled = true;
        self.auto.stage = AutoStage::LoadingCache;
        self.auto.desired_symbols = all.clone();
        self.auto.pending_missing.clear();
        self.auto.jump_to_chart_on_complete = true;

        // Ask worker to load cached data for everything we selected.
        let cmd = WorkerCommand::LoadCachedData {
            symbols: all.clone(),
        };
        if channels.command_tx.send(cmd).is_ok() {
            self.status_message = format!(
                "Full-Auto: loading cached data for {} tickers...",
                all.len()
            );
        } else {
            self.status_message = "Full-Auto: failed to start cache load.".to_string();
            self.auto.stage = AutoStage::Idle;
        }
    }

    /// Start YOLO Mode: continuous auto-optimization loop
    ///
    /// Runs multi-strategy sweeps in a loop, applying parameter randomization each iteration,
    /// maintaining a top-4 leaderboard ranked by Sharpe, and persisting to JSON.
    /// Press Escape to stop.
    ///
    /// Always uses all tickers from the universe for comprehensive exploration.
    pub fn start_yolo_mode(&mut self, channels: &WorkerChannels) {
        // Always use all tickers from universe for YOLO mode
        let mut selected: Vec<String> = Vec::new();
        for sector in self.data.universe.sectors.iter() {
            selected.extend(sector.tickers.iter().cloned());
        }
        selected.sort();
        selected.dedup();

        // Debug: Log universe size and verify count
        let universe_total: usize = self
            .data
            .universe
            .sectors
            .iter()
            .map(|s| s.tickers.len())
            .sum();
        tracing::info!(
            universe_sectors = self.data.universe.sectors.len(),
            universe_total_tickers = universe_total,
            selected_after_dedup = selected.len(),
            "YOLO: Collected tickers from universe"
        );

        // Warn if count doesn't match expected
        if selected.len() < 400 {
            tracing::warn!(
                selected_count = selected.len(),
                expected_min = 450,
                "YOLO: Unexpectedly low ticker count! Universe may be filtered."
            );
        }

        // Update selected_tickers to match
        self.data.selected_tickers.clear();
        for sym in &selected {
            self.data.selected_tickers.insert(sym.clone());
        }

        if selected.is_empty() {
            self.status_message = "YOLO Mode: Universe has no tickers.".to_string();
            return;
        }

        // Build symbol->bars map from cache
        let mut symbol_bars: HashMap<String, Arc<Vec<Bar>>> = HashMap::new();
        let mut missing: Vec<String> = Vec::new();
        for sym in &selected {
            if let Some(bars) = self.data.bars_cache.get(sym) {
                if !bars.is_empty() {
                    symbol_bars.insert(sym.clone(), Arc::new(bars.clone()));
                } else {
                    missing.push(sym.clone());
                }
            } else {
                missing.push(sym.clone());
            }
        }

        if symbol_bars.is_empty() {
            self.status_message = format!(
                "YOLO Mode: No bars loaded for selected tickers. Missing: {}",
                missing.join(", ")
            );
            return;
        }

        // Build strategy grid (use Quick depth for faster iterations)
        let strategy_grid = MultiStrategyGrid::with_depth(SweepDepth::Quick);

        let backtest_config = BacktestConfig {
            initial_cash: 100_000.0,
            fill_model: FillModel::NextOpen,
            cost_model: CostModel {
                fees_bps_per_side: 10.0,
                slippage_bps: 5.0,
            },
            qty: 100.0,
            pyramid_config: PyramidConfig::default(),
        };

        // Try to load existing leaderboards
        let existing_per_symbol_leaderboard =
            Leaderboard::load(std::path::Path::new("artifacts/leaderboard.json")).ok();
        let existing_cross_symbol_leaderboard = CrossSymbolLeaderboard::load(std::path::Path::new(
            "artifacts/cross_symbol_leaderboard.json",
        ))
        .ok();

        // Provide symbol -> sector_id mapping so YOLO can do sector-aware validation.
        let sector_lookup = self.data.universe.build_sector_id_lookup();
        let symbol_sector_ids: HashMap<String, String> = selected
            .iter()
            .filter_map(|sym| {
                sector_lookup
                    .get(sym)
                    .map(|sector| (sym.clone(), sector.clone()))
            })
            .collect();

        let cmd = WorkerCommand::StartYoloMode {
            symbols: selected.clone(),
            symbol_sector_ids,
            start: self.yolo.config.start_date,
            end: self.yolo.config.end_date,
            strategy_grid,
            backtest_config,
            randomization_pct: self.yolo.randomization_pct,
            wf_sharpe_threshold: self.yolo.wf_sharpe_threshold,
            existing_per_symbol_leaderboard,
            existing_cross_symbol_leaderboard,
            session_id: Some(self.yolo.session_id.clone()),
            polars_max_threads: self.yolo.polars_max_threads,
            outer_threads: self.yolo.outer_threads,
        };

        let symbols_count = selected.len();
        if channels.command_tx.send(cmd).is_ok() {
            self.yolo.enabled = true;
            self.yolo.started_at = Some(Utc::now());
            self.sweep.is_running = true;
            self.status_message = format!(
                "YOLO Mode starting: {} symbols (press ESC to stop)...",
                symbols_count
            );
            tracing::info!(
                symbols_sent_to_worker = symbols_count,
                start_date = %self.yolo.config.start_date,
                end_date = %self.yolo.config.end_date,
                "YOLO: Starting with symbol count and date range"
            );
        } else {
            self.status_message = "Failed to start YOLO Mode.".to_string();
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
