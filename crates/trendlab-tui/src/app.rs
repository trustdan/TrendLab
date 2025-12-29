//! Application state for TrendLab TUI
#![allow(dead_code)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::upper_case_acronyms)]

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, NaiveDate, Utc};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use ratatui::layout::Rect;
use trendlab_core::{
    generate_session_id, BacktestConfig, Bar, CostModel, CrossSymbolLeaderboard, FillModel,
    Leaderboard, LeaderboardScope, Metrics, MultiStrategyGrid, MultiStrategySweepResult,
    MultiSweepResult, PyramidConfig, RiskProfile, Sector, StrategyTypeId, SweepConfigResult,
    SweepDepth, SweepGrid, Universe,
};

use crate::worker::{WorkerChannels, WorkerCommand};

/// Seeded, opt-in randomization of initial UI defaults.
///
/// Intended for "Monte Carlo-ish" exploration: each launch can open near defaults
/// while remaining reproducible via the seed.
#[derive(Debug, Clone)]
pub struct RandomDefaults {
    pub enabled: bool,
    pub seed: u64,
}

impl Default for RandomDefaults {
    fn default() -> Self {
        Self {
            enabled: false,
            seed: 0,
        }
    }
}

fn env_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "y" | "on")
        }
        Err(_) => false,
    }
}

fn env_optional_bool(name: &str) -> Option<bool> {
    std::env::var(name).ok().map(|v| {
        let v = v.trim().to_ascii_lowercase();
        // Accept common falsy values explicitly.
        if matches!(v.as_str(), "0" | "false" | "no" | "n" | "off") {
            false
        } else {
            matches!(v.as_str(), "1" | "true" | "yes" | "y" | "on")
        }
    })
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

fn generate_seed() -> u64 {
    // Stable enough on all platforms; not crypto-grade (doesn't need to be).
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn read_and_increment_launch_count() -> u64 {
    // Persist a simple counter so the "random defaults" change each time the TUI is opened,
    // even if you open it multiple times in the same second.
    let path = std::path::Path::new("configs").join("tui_launch_count.txt");
    let mut count: u64 = 0;
    if let Ok(s) = std::fs::read_to_string(&path) {
        count = s.trim().parse::<u64>().unwrap_or(0);
    }
    let next = count.saturating_add(1);
    let _ = std::fs::create_dir_all("configs");
    let _ = std::fs::write(&path, next.to_string());
    next
}

fn derive_nonrepeatable_seed() -> u64 {
    // Not crypto; just "looks random" and won't repeat in normal usage.
    let t = generate_seed();
    let launch = read_and_increment_launch_count();
    let pid = std::process::id() as u64;
    // Mix bits a bit (xorshift-ish).
    let mut x = t ^ (launch.rotate_left(17)) ^ (pid.rotate_left(7));
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    x.wrapping_mul(0x2545F4914F6CDD1D)
}

fn jitter_pct_delta(rng: &mut impl Rng, min_abs: f64, max_abs: f64) -> f64 {
    // delta in [-max_abs, -min_abs] U [min_abs, max_abs]
    let mag = rng.gen_range(min_abs..=max_abs);
    let sign = if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
    sign * mag
}

fn round_to_step_f64(value: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return value;
    }
    (value / step).round() * step
}

fn jitter_usize_percent(
    rng: &mut impl Rng,
    base: usize,
    min_pct: f64,
    max_pct: f64,
    step: usize,
    min: usize,
    max: usize,
) -> usize {
    let delta = jitter_pct_delta(rng, min_pct, max_pct);
    let mut candidate = (base as f64) * (1.0 + delta);
    if step > 1 {
        candidate = round_to_step_f64(candidate, step as f64);
    }
    let candidate = candidate.round().max(0.0) as usize;
    candidate.clamp(min, max)
}

fn jitter_f64_percent(
    rng: &mut impl Rng,
    base: f64,
    min_pct: f64,
    max_pct: f64,
    step: f64,
    min: f64,
    max: f64,
) -> f64 {
    let delta = jitter_pct_delta(rng, min_pct, max_pct);
    let candidate = base * (1.0 + delta);
    round_to_step_f64(candidate, step).clamp(min, max)
}

fn jitter_date_range_percent(
    rng: &mut impl Rng,
    start: NaiveDate,
    end: NaiveDate,
    min_pct: f64,
    max_pct: f64,
    min_date: NaiveDate,
    max_date: NaiveDate,
    min_span_days: i64,
) -> (NaiveDate, NaiveDate) {
    let span = (end - start).num_days().abs().max(1);
    // Only jitter the start date; end date stays fixed to maximize data coverage
    let mag_start = rng.gen_range(min_pct..=max_pct);
    // Start date can shift either direction
    let shift_start =
        (span as f64 * mag_start).round() as i64 * if rng.gen_bool(0.5) { 1 } else { -1 };
    // End date only shifts forward (never backward) to ensure we always include recent data.
    // This prevents YOLO mode from accidentally cutting off years of recent market data.
    // The shift is clamped to max_date anyway, so forward shifts have no effect when end == max_date.

    let mut s = start
        .checked_add_signed(chrono::Duration::days(shift_start))
        .unwrap_or(start)
        .clamp(min_date, max_date);
    // Keep end date at the original value (no jitter) to maximize data coverage
    let mut e = end.clamp(min_date, max_date);

    if e < s {
        std::mem::swap(&mut s, &mut e);
    }
    if (e - s).num_days() < min_span_days {
        e = s
            .checked_add_signed(chrono::Duration::days(min_span_days))
            .unwrap_or(e)
            .clamp(min_date, max_date);
    }
    (s, e)
}

/// Strategy category for grouped checkboxes in the TUI.
/// Each category groups related strategy types for easier navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrategyCategory {
    /// Channel-based breakout strategies (Donchian, Keltner, STARC, Bollinger, Supertrend)
    ChannelBreakouts,
    /// Momentum and direction strategies (MA Cross, TSMOM, DMI/ADX, Aroon, Heikin-Ashi)
    MomentumDirection,
    /// Price structure breakouts (Darvas, 52-Week High, Larry Williams, ORB)
    PriceBreakouts,
    /// Classic preset strategies (Turtle S1, Turtle S2, Parabolic SAR)
    ClassicPresets,
}

impl StrategyCategory {
    /// All categories in display order.
    pub fn all() -> &'static [StrategyCategory] {
        &[
            StrategyCategory::ChannelBreakouts,
            StrategyCategory::MomentumDirection,
            StrategyCategory::PriceBreakouts,
            StrategyCategory::ClassicPresets,
        ]
    }

    /// Display name for the category.
    pub fn name(&self) -> &'static str {
        match self {
            StrategyCategory::ChannelBreakouts => "Channel Breakouts",
            StrategyCategory::MomentumDirection => "Momentum/Direction",
            StrategyCategory::PriceBreakouts => "Price Breakouts",
            StrategyCategory::ClassicPresets => "Classic Presets",
        }
    }

    /// Get all strategy types in this category.
    pub fn strategies(&self) -> &'static [StrategyType] {
        match self {
            StrategyCategory::ChannelBreakouts => &[
                StrategyType::Donchian,
                StrategyType::Keltner,
                StrategyType::STARC,
                StrategyType::Supertrend,
            ],
            StrategyCategory::MomentumDirection => {
                &[StrategyType::MACrossover, StrategyType::Tsmom]
            }
            StrategyCategory::PriceBreakouts => &[StrategyType::OpeningRange],
            StrategyCategory::ClassicPresets => &[
                StrategyType::TurtleS1,
                StrategyType::TurtleS2,
                StrategyType::ParabolicSar,
            ],
        }
    }

    /// Check if category has any strategies (for display purposes).
    pub fn is_empty(&self) -> bool {
        self.strategies().is_empty()
    }

    /// Count of strategies in this category.
    pub fn len(&self) -> usize {
        self.strategies().len()
    }
}

/// Voting method for ensemble strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VotingMethod {
    /// Simple majority: > 50% agreement takes that signal
    #[default]
    Majority,
    /// Longer horizons weighted more heavily
    WeightedByHorizon,
    /// Only enter if ALL horizons agree; exit on ANY exit signal
    UnanimousEntry,
}

impl VotingMethod {
    /// All voting methods.
    pub fn all() -> &'static [VotingMethod] {
        &[
            VotingMethod::Majority,
            VotingMethod::WeightedByHorizon,
            VotingMethod::UnanimousEntry,
        ]
    }

    /// Display name.
    pub fn name(&self) -> &'static str {
        match self {
            VotingMethod::Majority => "Majority",
            VotingMethod::WeightedByHorizon => "Weighted by Horizon",
            VotingMethod::UnanimousEntry => "Unanimous Entry",
        }
    }
}

/// Ensemble configuration for multi-horizon voting.
#[derive(Debug, Clone)]
pub struct EnsembleConfig {
    /// Whether ensemble mode is enabled
    pub enabled: bool,
    /// Base strategy type for the ensemble
    pub base_strategy: StrategyType,
    /// Lookback horizons to use (e.g., [10, 20, 55] for Donchian)
    pub horizons: Vec<usize>,
    /// Voting method for combining signals
    pub voting: VotingMethod,
}

impl Default for EnsembleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_strategy: StrategyType::Donchian,
            horizons: vec![10, 20, 55],
            voting: VotingMethod::Majority,
        }
    }
}

/// Startup flow mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupMode {
    Manual,
    FullAuto,
}

impl StartupMode {
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            StartupMode::Manual => "Manual",
            StartupMode::FullAuto => "Full-Auto",
        }
    }
}

/// Strategy selection for Full-Auto mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategySelection {
    /// Run all strategies and compare
    #[default]
    AllStrategies,
    /// Run a single strategy type
    Single(StrategyType),
}

impl StrategySelection {
    /// Get all options for the startup modal
    pub fn all_options() -> Vec<StrategySelection> {
        let mut options = vec![StrategySelection::AllStrategies];
        for st in StrategyType::all() {
            options.push(StrategySelection::Single(*st));
        }
        options
    }

    pub fn name(&self) -> &'static str {
        match self {
            StrategySelection::AllStrategies => "All Strategies",
            StrategySelection::Single(st) => st.name(),
        }
    }
}

/// Startup modal state (shown on app launch).
#[derive(Debug, Clone)]
pub struct StartupState {
    pub active: bool,
    pub mode: StartupMode,
    pub selected_strategy_index: usize,
    /// Strategy selection for Full-Auto mode (All Strategies = index 0)
    pub strategy_selection: StrategySelection,
    /// Sweep depth for parameter range coverage
    pub sweep_depth: SweepDepth,
}

impl Default for StartupState {
    fn default() -> Self {
        Self {
            active: true,
            mode: StartupMode::Manual,
            selected_strategy_index: 0,
            strategy_selection: StrategySelection::AllStrategies,
            sweep_depth: SweepDepth::Standard,
        }
    }
}

/// Full-auto pipeline stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoStage {
    #[default]
    Idle,
    LoadingCache,
    FetchingMissing,
    Sweeping,
}

/// Full-auto run state.
#[derive(Debug, Clone, Default)]
pub struct AutoRunState {
    pub enabled: bool,
    pub stage: AutoStage,
    pub desired_symbols: Vec<String>,
    pub pending_missing: Vec<String>,
    pub jump_to_chart_on_complete: bool,
}

/// Panel identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Data,
    Strategy,
    Sweep,
    Results,
    Chart,
    Help,
}

impl Panel {
    pub fn all() -> &'static [Panel] {
        &[
            Panel::Data,
            Panel::Strategy,
            Panel::Sweep,
            Panel::Results,
            Panel::Chart,
            Panel::Help,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            Panel::Data => "Data",
            Panel::Strategy => "Strategy",
            Panel::Sweep => "Sweep",
            Panel::Results => "Results",
            Panel::Chart => "Chart",
            Panel::Help => "Help",
        }
    }

    pub fn hotkey(&self) -> char {
        match self {
            Panel::Data => '1',
            Panel::Strategy => '2',
            Panel::Sweep => '3',
            Panel::Results => '4',
            Panel::Chart => '5',
            Panel::Help => '6',
        }
    }
}

/// Help panel section identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpSection {
    #[default]
    Global,
    Data,
    Strategy,
    Sweep,
    Results,
    Chart,
    Features,
}

impl HelpSection {
    pub fn all() -> &'static [HelpSection] {
        &[
            HelpSection::Global,
            HelpSection::Data,
            HelpSection::Strategy,
            HelpSection::Sweep,
            HelpSection::Results,
            HelpSection::Chart,
            HelpSection::Features,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            HelpSection::Global => "Global",
            HelpSection::Data => "Data",
            HelpSection::Strategy => "Strategy",
            HelpSection::Sweep => "Sweep",
            HelpSection::Results => "Results",
            HelpSection::Chart => "Chart",
            HelpSection::Features => "Features",
        }
    }

    /// Map a Panel to its corresponding HelpSection
    pub fn from_panel(panel: Panel) -> Self {
        match panel {
            Panel::Data => HelpSection::Data,
            Panel::Strategy => HelpSection::Strategy,
            Panel::Sweep => HelpSection::Sweep,
            Panel::Results => HelpSection::Results,
            Panel::Chart => HelpSection::Chart,
            Panel::Help => HelpSection::Global,
        }
    }

    pub fn next(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|s| s == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|s| s == self).unwrap_or(0);
        if idx == 0 {
            all[all.len() - 1]
        } else {
            all[idx - 1]
        }
    }
}

/// Help panel state
#[derive(Debug, Clone)]
pub struct HelpState {
    pub active_section: HelpSection,
    pub scroll_offset: usize,
    pub max_scroll: usize,
    pub previous_panel: Panel,
    pub search_mode: bool,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_index: usize,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            active_section: HelpSection::Global,
            scroll_offset: 0,
            max_scroll: 0,
            previous_panel: Panel::Data,
            search_mode: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: 0,
        }
    }
}

/// Current operation state
#[derive(Debug, Clone, Default)]
pub enum OperationState {
    #[default]
    Idle,
    FetchingData {
        current_symbol: String,
        completed: usize,
        total: usize,
    },
    RunningSweep {
        completed: usize,
        total: usize,
    },
}

/// Type of status message for appropriate color styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageType {
    /// Informational message (cyan/default)
    #[default]
    Info,
    /// Success message (green)
    Success,
    /// Warning message (yellow)
    Warning,
    /// Error message (red)
    Error,
}

/// Search suggestion from Yahoo.
#[derive(Debug, Clone)]
pub struct SearchSuggestion {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub type_disp: String,
}

/// View mode for the Data panel (sector list vs ticker list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataViewMode {
    /// Viewing the list of sectors
    #[default]
    Sectors,
    /// Viewing tickers within the selected sector
    Tickers,
}

/// Data panel state
#[derive(Debug)]
pub struct DataState {
    pub symbols: Vec<String>,
    pub selected_index: usize,
    pub bar_count: usize,
    pub date_range: Option<(String, String)>,
    pub bars_cache: HashMap<String, Vec<Bar>>,
    // Search mode state
    pub search_mode: bool,
    pub search_input: String,
    pub search_suggestions: Vec<SearchSuggestion>,
    pub search_selected: usize,
    pub search_loading: bool,
    // Universe/sector state
    pub universe: Universe,
    pub view_mode: DataViewMode,
    pub selected_sector_index: usize,
    pub selected_ticker_index: usize,
    pub selected_tickers: HashSet<String>,
}

impl Default for DataState {
    fn default() -> Self {
        Self {
            symbols: Vec::new(),
            selected_index: 0,
            bar_count: 0,
            date_range: None,
            bars_cache: HashMap::new(),
            search_mode: false,
            search_input: String::new(),
            search_suggestions: Vec::new(),
            search_selected: 0,
            search_loading: false,
            universe: Universe::default_universe(),
            view_mode: DataViewMode::Sectors,
            selected_sector_index: 0,
            selected_ticker_index: 0,
            selected_tickers: HashSet::new(),
        }
    }
}

impl DataState {
    /// Get the currently selected symbol
    pub fn selected_symbol(&self) -> Option<&String> {
        self.symbols.get(self.selected_index)
    }

    /// Get bars for the selected symbol
    pub fn selected_bars(&self) -> Option<&Vec<Bar>> {
        self.selected_symbol().and_then(|s| self.bars_cache.get(s))
    }

    /// Get the currently selected sector.
    pub fn selected_sector(&self) -> Option<&Sector> {
        self.universe
            .get_sector_by_index(self.selected_sector_index)
    }

    /// Get tickers in the currently selected sector.
    pub fn current_sector_tickers(&self) -> &[String] {
        self.selected_sector()
            .map(|s| s.tickers.as_slice())
            .unwrap_or(&[])
    }

    /// Get the currently focused ticker (in ticker view mode).
    pub fn focused_ticker(&self) -> Option<&String> {
        self.current_sector_tickers()
            .get(self.selected_ticker_index)
    }

    /// Check if a ticker is selected for multi-ticker sweep.
    pub fn is_ticker_selected(&self, ticker: &str) -> bool {
        self.selected_tickers.contains(ticker)
    }

    /// Toggle ticker selection for multi-ticker sweep.
    pub fn toggle_ticker_selection(&mut self, ticker: &str) {
        if self.selected_tickers.contains(ticker) {
            self.selected_tickers.remove(ticker);
        } else {
            self.selected_tickers.insert(ticker.to_string());
        }
    }

    /// Select all tickers in the current sector.
    pub fn select_all_in_sector(&mut self) {
        // Collect tickers first to avoid borrow conflict
        let tickers: Vec<String> = self
            .selected_sector()
            .map(|s| s.tickers.clone())
            .unwrap_or_default();

        for ticker in tickers {
            self.selected_tickers.insert(ticker);
        }
    }

    /// Deselect all tickers in the current sector.
    pub fn deselect_all_in_sector(&mut self) {
        // Collect tickers first to avoid borrow conflict
        let tickers: Vec<String> = self
            .selected_sector()
            .map(|s| s.tickers.clone())
            .unwrap_or_default();

        for ticker in &tickers {
            self.selected_tickers.remove(ticker);
        }
    }

    /// Select all tickers across all sectors (for YOLO mode).
    pub fn select_all(&mut self) {
        for sector in &self.universe.sectors {
            for ticker in &sector.tickers {
                self.selected_tickers.insert(ticker.clone());
            }
        }
    }

    /// Deselect all tickers globally.
    pub fn deselect_all(&mut self) {
        self.selected_tickers.clear();
    }

    /// Get count of selected tickers in a sector by sector ID.
    pub fn selected_count_in_sector(&self, sector_id: &str) -> usize {
        self.universe
            .get_sector(sector_id)
            .map(|sector| {
                sector
                    .tickers
                    .iter()
                    .filter(|t| self.selected_tickers.contains(*t))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Get all selected tickers as a sorted vector.
    pub fn selected_tickers_sorted(&self) -> Vec<String> {
        let mut tickers: Vec<String> = self.selected_tickers.iter().cloned().collect();
        tickers.sort();
        tickers
    }

    /// Load universe from config file, falling back to default.
    pub fn load_universe_from_config(&mut self) {
        let config_path = std::path::Path::new("configs/universe.toml");
        if config_path.exists() {
            match Universe::load(config_path) {
                Ok(universe) => {
                    self.universe = universe;
                }
                Err(e) => {
                    eprintln!("Failed to load universe config: {}", e);
                    // Keep default universe
                }
            }
        }
        // Otherwise keep the default universe
    }
}

/// Strategy type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StrategyType {
    #[default]
    Donchian,
    TurtleS1,
    TurtleS2,
    MACrossover,
    Tsmom,
    // Phase 1: ATR-Based Channels
    Keltner,
    STARC,
    Supertrend,
    // Phase 4: Complex Stateful + Ensemble
    ParabolicSar,
    OpeningRange,
}

impl StrategyType {
    pub fn all() -> &'static [StrategyType] {
        &[
            StrategyType::Donchian,
            StrategyType::TurtleS1,
            StrategyType::TurtleS2,
            StrategyType::MACrossover,
            StrategyType::Tsmom,
            StrategyType::Keltner,
            StrategyType::STARC,
            StrategyType::Supertrend,
            StrategyType::ParabolicSar,
            StrategyType::OpeningRange,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            StrategyType::Donchian => "Donchian Breakout",
            StrategyType::TurtleS1 => "Turtle System 1",
            StrategyType::TurtleS2 => "Turtle System 2",
            StrategyType::MACrossover => "MA Crossover",
            StrategyType::Tsmom => "TSMOM",
            StrategyType::Keltner => "Keltner Channel",
            StrategyType::STARC => "STARC Bands",
            StrategyType::Supertrend => "Supertrend",
            StrategyType::ParabolicSar => "Parabolic SAR",
            StrategyType::OpeningRange => "Opening Range Breakout",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            StrategyType::Donchian => "Breakout above N-day high, exit below M-day low",
            StrategyType::TurtleS1 => "Classic Turtle: 20-day entry, 10-day exit",
            StrategyType::TurtleS2 => "Turtle variant: 55-day entry, 20-day exit",
            StrategyType::MACrossover => "Enter on golden cross, exit on death cross",
            StrategyType::Tsmom => "Time-series momentum: long when return > 0",
            StrategyType::Keltner => "Breakout above EMA + k*ATR channel",
            StrategyType::STARC => "Breakout above SMA + k*ATR bands",
            StrategyType::Supertrend => "Follow ATR-based trailing trend line",
            StrategyType::ParabolicSar => "Wilder's SAR: trailing stop with acceleration",
            StrategyType::OpeningRange => "Entry on breakout above opening range high",
        }
    }

    /// Number of configurable parameters for this strategy
    pub fn param_count(&self) -> usize {
        match self {
            StrategyType::Donchian => 2,
            StrategyType::TurtleS1 => 0, // Fixed params
            StrategyType::TurtleS2 => 0, // Fixed params
            StrategyType::MACrossover => 3,
            StrategyType::Tsmom => 1,
            StrategyType::Keltner => 3, // ema_period, atr_period, multiplier
            StrategyType::STARC => 3,   // sma_period, atr_period, multiplier
            StrategyType::Supertrend => 2, // atr_period, multiplier
            StrategyType::ParabolicSar => 3, // af_start, af_step, af_max
            StrategyType::OpeningRange => 2, // range_bars, period
        }
    }

    /// Get the category this strategy belongs to.
    pub fn category(&self) -> StrategyCategory {
        match self {
            StrategyType::Donchian
            | StrategyType::Keltner
            | StrategyType::STARC
            | StrategyType::Supertrend => StrategyCategory::ChannelBreakouts,
            StrategyType::MACrossover | StrategyType::Tsmom => StrategyCategory::MomentumDirection,
            StrategyType::OpeningRange => StrategyCategory::PriceBreakouts,
            StrategyType::TurtleS1 | StrategyType::TurtleS2 | StrategyType::ParabolicSar => {
                StrategyCategory::ClassicPresets
            }
        }
    }
}

/// Donchian strategy configuration
#[derive(Debug, Clone)]
pub struct DonchianConfig {
    pub entry_lookback: usize,
    pub exit_lookback: usize,
}

impl Default for DonchianConfig {
    fn default() -> Self {
        Self {
            entry_lookback: 20,
            exit_lookback: 10,
        }
    }
}

/// MA Crossover strategy configuration
#[derive(Debug, Clone)]
pub struct MACrossoverConfig {
    pub fast_period: usize,
    pub slow_period: usize,
    pub ma_type: usize, // 0 = SMA, 1 = EMA
}

impl Default for MACrossoverConfig {
    fn default() -> Self {
        Self {
            fast_period: 10,
            slow_period: 50,
            ma_type: 0, // SMA
        }
    }
}

impl MACrossoverConfig {
    pub fn ma_type_name(&self) -> &'static str {
        if self.ma_type == 0 {
            "SMA"
        } else {
            "EMA"
        }
    }

    // Convenience alias used by some UI helpers.
    pub fn ma_type_label(&self) -> &'static str {
        self.ma_type_name()
    }
}

/// TSMOM strategy configuration
#[derive(Debug, Clone)]
pub struct TsmomConfig {
    pub lookback: usize,
}

impl Default for TsmomConfig {
    fn default() -> Self {
        Self { lookback: 252 }
    }
}

/// Keltner Channel strategy configuration
#[derive(Debug, Clone)]
pub struct KeltnerConfig {
    pub ema_period: usize,
    pub atr_period: usize,
    pub multiplier: f64,
}

impl Default for KeltnerConfig {
    fn default() -> Self {
        Self {
            ema_period: 20,
            atr_period: 10,
            multiplier: 2.0,
        }
    }
}

/// STARC Bands strategy configuration
#[derive(Debug, Clone)]
pub struct STARCConfig {
    pub sma_period: usize,
    pub atr_period: usize,
    pub multiplier: f64,
}

impl Default for STARCConfig {
    fn default() -> Self {
        Self {
            sma_period: 20,
            atr_period: 15,
            multiplier: 2.0,
        }
    }
}

/// Supertrend strategy configuration
#[derive(Debug, Clone)]
pub struct SupertrendConfig {
    pub atr_period: usize,
    pub multiplier: f64,
}

impl Default for SupertrendConfig {
    fn default() -> Self {
        Self {
            atr_period: 10,
            multiplier: 3.0,
        }
    }
}

/// Parabolic SAR strategy configuration
#[derive(Debug, Clone)]
pub struct ParabolicSarConfig {
    pub af_start: f64,
    pub af_step: f64,
    pub af_max: f64,
}

impl Default for ParabolicSarConfig {
    fn default() -> Self {
        Self {
            af_start: 0.02,
            af_step: 0.02,
            af_max: 0.20,
        }
    }
}

/// Opening Range Breakout strategy configuration
#[derive(Debug, Clone)]
pub struct OpeningRangeConfig {
    pub range_bars: usize,
    pub period: usize, // 0 = Weekly, 1 = Monthly, 2 = Rolling
}

impl Default for OpeningRangeConfig {
    fn default() -> Self {
        Self {
            range_bars: 5,
            period: 0, // Weekly
        }
    }
}

impl OpeningRangeConfig {
    pub fn period_name(&self) -> &'static str {
        match self.period {
            0 => "Weekly",
            1 => "Monthly",
            _ => "Rolling",
        }
    }

    // Convenience alias used by some UI helpers.
    pub fn timeframe_label(&self) -> &'static str {
        self.period_name()
    }
}

/// Focus mode within the strategy panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategyFocus {
    /// Navigating categories and strategy checkboxes (left panel)
    #[default]
    Selection,
    /// Editing parameters for a specific strategy (right panel)
    Parameters,
}

/// Strategy panel state with grouped checkbox support
#[derive(Debug)]
pub struct StrategyState {
    // === Grouped checkbox state ===
    /// Categories that are currently expanded (show their strategies)
    pub expanded_categories: HashSet<StrategyCategory>,
    /// Strategies that are currently selected (checked)
    pub selected_strategies: HashSet<StrategyType>,
    /// Currently focused category index
    pub focused_category_index: usize,
    /// Currently focused strategy index within the focused category
    pub focused_strategy_in_category: usize,
    /// Whether we're focused on a category header or a strategy within it
    pub focus_on_category: bool,

    // === Ensemble configuration ===
    pub ensemble: EnsembleConfig,

    // === Legacy fields for parameter editing ===
    pub selected_type: StrategyType,
    pub selected_type_index: usize,
    pub donchian_config: DonchianConfig,
    pub ma_config: MACrossoverConfig,
    pub tsmom_config: TsmomConfig,
    pub keltner_config: KeltnerConfig,
    pub starc_config: STARCConfig,
    pub supertrend_config: SupertrendConfig,
    pub parabolic_sar_config: ParabolicSarConfig,
    pub opening_range_config: OpeningRangeConfig,
    pub selected_field: usize,
    /// Current focus mode: Selection (left) or Parameters (right)
    pub focus: StrategyFocus,

    // Legacy alias for backwards compatibility
    pub editing_strategy: bool,
}

impl Default for StrategyState {
    fn default() -> Self {
        // Start with non-empty categories expanded
        let mut expanded = HashSet::new();
        for cat in StrategyCategory::all() {
            if !cat.is_empty() {
                expanded.insert(*cat);
            }
        }

        // Start with Donchian selected by default
        let mut selected = HashSet::new();
        selected.insert(StrategyType::Donchian);

        Self {
            expanded_categories: expanded,
            selected_strategies: selected,
            focused_category_index: 0,
            focused_strategy_in_category: 0,
            focus_on_category: true,
            ensemble: EnsembleConfig::default(),
            selected_type: StrategyType::default(),
            selected_type_index: 0,
            donchian_config: DonchianConfig::default(),
            ma_config: MACrossoverConfig::default(),
            tsmom_config: TsmomConfig::default(),
            keltner_config: KeltnerConfig::default(),
            starc_config: STARCConfig::default(),
            supertrend_config: SupertrendConfig::default(),
            parabolic_sar_config: ParabolicSarConfig::default(),
            opening_range_config: OpeningRangeConfig::default(),
            selected_field: 0,
            focus: StrategyFocus::Selection,
            editing_strategy: true, // Legacy: true = selection mode
        }
    }
}

impl StrategyState {
    /// Get the currently focused category.
    pub fn focused_category(&self) -> Option<StrategyCategory> {
        StrategyCategory::all()
            .get(self.focused_category_index)
            .copied()
    }

    /// Check if a category is expanded.
    pub fn is_expanded(&self, cat: StrategyCategory) -> bool {
        self.expanded_categories.contains(&cat)
    }

    /// Toggle category expansion.
    pub fn toggle_category(&mut self, cat: StrategyCategory) {
        if self.expanded_categories.contains(&cat) {
            self.expanded_categories.remove(&cat);
        } else {
            self.expanded_categories.insert(cat);
        }
    }

    /// Check if a strategy is selected.
    pub fn is_selected(&self, strat: StrategyType) -> bool {
        self.selected_strategies.contains(&strat)
    }

    /// Toggle strategy selection.
    pub fn toggle_strategy(&mut self, strat: StrategyType) {
        if self.selected_strategies.contains(&strat) {
            self.selected_strategies.remove(&strat);
        } else {
            self.selected_strategies.insert(strat);
        }
    }

    /// Select all strategies in a category.
    pub fn select_all_in_category(&mut self, cat: StrategyCategory) {
        for strat in cat.strategies() {
            self.selected_strategies.insert(*strat);
        }
    }

    /// Deselect all strategies in a category.
    pub fn deselect_all_in_category(&mut self, cat: StrategyCategory) {
        for strat in cat.strategies() {
            self.selected_strategies.remove(strat);
        }
    }

    /// Count selected strategies in a category.
    pub fn selected_count_in_category(&self, cat: StrategyCategory) -> usize {
        cat.strategies()
            .iter()
            .filter(|s| self.selected_strategies.contains(s))
            .count()
    }

    /// Get the currently focused strategy (if focus is on a strategy, not category header).
    pub fn focused_strategy(&self) -> Option<StrategyType> {
        if self.focus_on_category {
            return None;
        }
        let cat = self.focused_category()?;
        cat.strategies()
            .get(self.focused_strategy_in_category)
            .copied()
    }

    /// Get all selected strategies as a sorted vector.
    pub fn selected_strategies_sorted(&self) -> Vec<StrategyType> {
        let mut strats: Vec<StrategyType> = self.selected_strategies.iter().copied().collect();
        strats.sort_by_key(|s| s.name());
        strats
    }

    /// Format the current strategy config as a display string for Pine export
    pub fn config_display_string(&self) -> String {
        match self.selected_type {
            StrategyType::Donchian => {
                format!(
                    "Entry={}, Exit={}",
                    self.donchian_config.entry_lookback, self.donchian_config.exit_lookback
                )
            }
            StrategyType::MACrossover => {
                format!(
                    "Fast={}, Slow={}, Type={}",
                    self.ma_config.fast_period,
                    self.ma_config.slow_period,
                    self.ma_config.ma_type_name()
                )
            }
            StrategyType::Tsmom => {
                format!("Lookback={}", self.tsmom_config.lookback)
            }
            StrategyType::Keltner => {
                format!(
                    "EMA={}, ATR={}, Mult={:.1}",
                    self.keltner_config.ema_period,
                    self.keltner_config.atr_period,
                    self.keltner_config.multiplier
                )
            }
            StrategyType::STARC => {
                format!(
                    "SMA={}, ATR={}, Mult={:.1}",
                    self.starc_config.sma_period,
                    self.starc_config.atr_period,
                    self.starc_config.multiplier
                )
            }
            StrategyType::Supertrend => {
                format!(
                    "ATR={}, Mult={:.1}",
                    self.supertrend_config.atr_period, self.supertrend_config.multiplier
                )
            }
            StrategyType::ParabolicSar => {
                format!(
                    "Start={:.2}, Step={:.2}, Max={:.2}",
                    self.parabolic_sar_config.af_start,
                    self.parabolic_sar_config.af_step,
                    self.parabolic_sar_config.af_max
                )
            }
            StrategyType::OpeningRange => {
                format!(
                    "Bars={}, Period={}",
                    self.opening_range_config.range_bars,
                    self.opening_range_config.period_name()
                )
            }
            StrategyType::TurtleS1 => "System 1 (20/10)".to_string(),
            StrategyType::TurtleS2 => "System 2 (55/20)".to_string(),
        }
    }
}

/// Sweep panel state
#[derive(Debug, Default)]
pub struct SweepState {
    pub is_running: bool,
    pub progress: f64,
    pub total_configs: usize,
    pub completed_configs: usize,
    pub selected_param: usize,
    pub param_ranges: Vec<(String, Vec<String>)>,
}

impl SweepState {
    /// Generate a SweepGrid from current param ranges
    /// Returns Ok(grid) or Err(message) if parameter parsing fails
    pub fn to_sweep_grid(&self) -> Result<SweepGrid, String> {
        let entry_result = self
            .param_ranges
            .iter()
            .find(|(name, _)| name == "entry_lookback");

        let exit_result = self
            .param_ranges
            .iter()
            .find(|(name, _)| name == "exit_lookback");

        // Parse entry lookbacks with error tracking
        let entry_lookbacks: Vec<usize> = if let Some((_, values)) = entry_result {
            let parsed: Vec<usize> = values.iter().filter_map(|v| v.parse().ok()).collect();
            if parsed.is_empty() && !values.is_empty() {
                return Err(format!(
                    "Invalid entry_lookback values: {:?}. Use comma-separated integers like '10,20,30'",
                    values
                ));
            }
            if parsed.is_empty() {
                vec![10, 20, 30, 40, 50] // Default if no values provided
            } else {
                parsed
            }
        } else {
            vec![10, 20, 30, 40, 50]
        };

        // Parse exit lookbacks with error tracking
        let exit_lookbacks: Vec<usize> = if let Some((_, values)) = exit_result {
            let parsed: Vec<usize> = values.iter().filter_map(|v| v.parse().ok()).collect();
            if parsed.is_empty() && !values.is_empty() {
                return Err(format!(
                    "Invalid exit_lookback values: {:?}. Use comma-separated integers like '5,10,15'",
                    values
                ));
            }
            if parsed.is_empty() {
                vec![5, 10, 15, 20, 25] // Default if no values provided
            } else {
                parsed
            }
        } else {
            vec![5, 10, 15, 20, 25]
        };

        Ok(SweepGrid::new(entry_lookbacks, exit_lookbacks))
    }
}

/// YOLO Mode state - continuous auto-optimization
#[derive(Debug, Clone)]
pub struct YoloState {
    /// Whether YOLO mode is currently running
    pub enabled: bool,
    /// Current iteration number
    pub iteration: u32,

    // Session leaderboards (reset each app launch)
    /// Session per-symbol top performers leaderboard
    pub session_leaderboard: Leaderboard,
    /// Session cross-symbol aggregated leaderboard
    pub session_cross_symbol_leaderboard: Option<CrossSymbolLeaderboard>,

    // All-time leaderboards (persistent across sessions)
    /// All-time per-symbol top performers leaderboard
    pub all_time_leaderboard: Leaderboard,
    /// All-time cross-symbol aggregated leaderboard
    pub all_time_cross_symbol_leaderboard: Option<CrossSymbolLeaderboard>,

    /// Which scope is currently being displayed (toggle with 't')
    pub view_scope: LeaderboardScope,

    /// Unique session ID for tracking which session discovered entries
    pub session_id: String,

    /// Risk profile for weighted ranking (cycle with 'p')
    pub risk_profile: RiskProfile,

    /// Randomization percentage (e.g., 0.15 = ±15%)
    pub randomization_pct: f64,
    /// Total configs tested this session
    pub session_configs_tested: u64,
    /// Total configs tested all-time (loaded from all_time_leaderboard)
    pub total_configs_tested: u64,
    /// When YOLO mode was started this session
    pub started_at: Option<DateTime<Utc>>,
    /// Whether the YOLO config modal is shown
    pub show_config: bool,
    /// The config modal state
    pub config: YoloConfigState,
}

/// YOLO mode configuration modal state
#[derive(Debug, Clone)]
pub struct YoloConfigState {
    /// Which field is currently focused
    pub focused_field: YoloConfigField,
    /// Start date for the backtest period
    pub start_date: NaiveDate,
    /// End date for the backtest period
    pub end_date: NaiveDate,
    /// Randomization percentage (0.0 to 1.0)
    pub randomization_pct: f64,
    /// Sweep depth for parameter coverage
    pub sweep_depth: SweepDepth,
}

/// Fields in the YOLO config modal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoloConfigField {
    StartDate,
    EndDate,
    Randomization,
    SweepDepth,
}

impl Default for YoloConfigState {
    fn default() -> Self {
        let today = chrono::Local::now().date_naive();
        Self {
            focused_field: YoloConfigField::StartDate,
            start_date: today - chrono::Duration::days(5 * 365),
            end_date: today,
            randomization_pct: 0.30,
            sweep_depth: SweepDepth::Quick,
        }
    }
}

impl YoloConfigField {
    pub fn next(self) -> Self {
        match self {
            Self::StartDate => Self::EndDate,
            Self::EndDate => Self::Randomization,
            Self::Randomization => Self::SweepDepth,
            Self::SweepDepth => Self::StartDate,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::StartDate => Self::SweepDepth,
            Self::EndDate => Self::StartDate,
            Self::Randomization => Self::EndDate,
            Self::SweepDepth => Self::Randomization,
        }
    }
}

impl Default for YoloState {
    fn default() -> Self {
        Self {
            enabled: false,
            iteration: 0,
            // Session leaderboards (fresh each app launch)
            session_leaderboard: Leaderboard::new(4),
            session_cross_symbol_leaderboard: None,
            // All-time leaderboards (will be loaded from disk in App::new)
            all_time_leaderboard: Leaderboard::new(16), // Larger capacity for historical data
            all_time_cross_symbol_leaderboard: None,
            // Default to showing session results
            view_scope: LeaderboardScope::Session,
            // Generate unique session ID
            session_id: generate_session_id(),
            // Default risk profile for weighted ranking
            risk_profile: RiskProfile::default(),
            // Default exploration strength for YOLO mode. Kept moderate so it explores meaningfully
            // without completely thrashing parameter space each iteration.
            randomization_pct: 0.30,
            session_configs_tested: 0,
            total_configs_tested: 0,
            started_at: None,
            show_config: false,
            config: YoloConfigState::default(),
        }
    }
}

impl YoloState {
    /// Get the per-symbol leaderboard for the current view scope.
    pub fn leaderboard(&self) -> &Leaderboard {
        match self.view_scope {
            LeaderboardScope::Session => &self.session_leaderboard,
            LeaderboardScope::AllTime => &self.all_time_leaderboard,
        }
    }

    /// Get the cross-symbol leaderboard for the current view scope.
    pub fn cross_symbol_leaderboard(&self) -> Option<&CrossSymbolLeaderboard> {
        match self.view_scope {
            LeaderboardScope::Session => self.session_cross_symbol_leaderboard.as_ref(),
            LeaderboardScope::AllTime => self.all_time_cross_symbol_leaderboard.as_ref(),
        }
    }

    /// Get configs tested count for the current view scope.
    pub fn configs_tested(&self) -> u64 {
        match self.view_scope {
            LeaderboardScope::Session => self.session_configs_tested,
            LeaderboardScope::AllTime => self.total_configs_tested,
        }
    }

    /// Toggle the view scope between Session and AllTime.
    pub fn toggle_scope(&mut self) {
        self.view_scope = self.view_scope.toggle();
    }

    /// Update both session and all-time leaderboards with new results from worker.
    pub fn update_leaderboards(
        &mut self,
        per_symbol: Leaderboard,
        cross_symbol: CrossSymbolLeaderboard,
        configs_tested_this_round: usize,
    ) {
        // Update session leaderboards
        self.session_leaderboard = per_symbol.clone();
        self.session_cross_symbol_leaderboard = Some(cross_symbol.clone());
        self.session_configs_tested += configs_tested_this_round as u64;

        // Merge into all-time leaderboards
        for entry in per_symbol.entries.iter() {
            self.all_time_leaderboard.try_insert(entry.clone());
        }
        if let Some(ref mut all_time_cross) = self.all_time_cross_symbol_leaderboard {
            for entry in cross_symbol.entries.iter() {
                all_time_cross.try_insert(entry.clone());
            }
        } else {
            self.all_time_cross_symbol_leaderboard = Some(cross_symbol);
        }
        self.total_configs_tested += configs_tested_this_round as u64;
    }
}

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
    pub selected_analysis: Option<trendlab_core::StatisticalAnalysis>,
    /// ID of the config for which analysis is being shown
    pub selected_analysis_id: Option<String>,
    /// Cache of computed analyses (config_id -> analysis)
    pub analysis_cache: std::collections::HashMap<String, trendlab_core::StatisticalAnalysis>,
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
                if let Some(best) = sweep_result
                    .config_results
                    .iter()
                    .max_by(|a, b| a.metrics.cagr.partial_cmp(&b.metrics.cagr).unwrap_or(std::cmp::Ordering::Equal))
                {
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

/// Chart view mode for multi-curve display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartViewMode {
    /// Single equity curve (original behavior)
    #[default]
    Single,
    /// Multiple per-ticker equity curves overlaid
    MultiTicker,
    /// Aggregated portfolio equity curve
    Portfolio,
    /// Strategy comparison: overlay best configs per strategy
    StrategyComparison,
    /// Per-ticker best strategy: each ticker's best strategy
    PerTickerBestStrategy,
    /// OHLC candlestick chart
    Candlestick,
}

/// A single candlestick for rendering
#[derive(Debug, Clone)]
pub struct CandleData {
    /// Index in the data series (for X position)
    #[allow(dead_code)]
    pub index: usize,
    /// Open price
    pub open: f64,
    /// High price
    pub high: f64,
    /// Low price
    pub low: f64,
    /// Close price
    pub close: f64,
    /// Volume
    pub volume: f64,
    /// Date for label display
    pub date: String,
}

#[allow(dead_code)]
impl CandleData {
    /// Returns true if this is a bullish (up) candle
    pub fn is_bullish(&self) -> bool {
        self.close >= self.open
    }

    /// Returns the body top (higher of open/close)
    pub fn body_top(&self) -> f64 {
        self.open.max(self.close)
    }

    /// Returns the body bottom (lower of open/close)
    pub fn body_bottom(&self) -> f64 {
        self.open.min(self.close)
    }
}

/// Cursor state for crosshair and tooltip
#[derive(Debug, Clone, Default)]
pub struct CursorState {
    /// Raw terminal coordinates (column, row)
    pub terminal_pos: Option<(u16, u16)>,
    /// Chart-relative coordinates (x, y within chart area)
    pub chart_pos: Option<(u16, u16)>,
    /// Data point index under cursor (for tooltip)
    pub data_index: Option<usize>,
    /// Whether cursor is within chart bounds
    pub in_chart: bool,
}

/// Animation state for smooth transitions
#[derive(Debug, Clone)]
pub struct AnimationState {
    /// Target zoom level (animate toward this)
    pub target_zoom: f64,
    /// Target scroll offset
    pub target_scroll: f64,
    /// Is animation active
    pub animating: bool,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            target_zoom: 1.0,
            target_scroll: 0.0,
            animating: false,
        }
    }
}

/// Per-ticker equity curve for multi-curve display
#[derive(Debug, Clone)]
pub struct TickerCurve {
    pub symbol: String,
    pub equity: Vec<f64>,
    pub dates: Vec<DateTime<Utc>>,
}

/// Per-strategy equity curve for strategy comparison view
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StrategyCurve {
    pub strategy_type: StrategyTypeId,
    pub config_display: String,
    pub equity: Vec<f64>,
    pub dates: Vec<DateTime<Utc>>,
    pub metrics: Metrics,
}

/// Per-ticker best strategy result for best strategy view
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TickerBestStrategy {
    pub symbol: String,
    pub strategy_type: StrategyTypeId,
    pub config_display: String,
    pub equity: Vec<f64>,
    pub dates: Vec<DateTime<Utc>>,
    pub metrics: Metrics,
}

/// Chart panel state
#[derive(Debug, Default)]
pub struct ChartState {
    pub equity_curve: Vec<f64>,
    pub equity_dates: Vec<DateTime<Utc>>,
    pub drawdown_curve: Vec<f64>,
    pub selected_result_index: Option<usize>,
    pub zoom_level: f64,
    pub scroll_offset: usize,
    pub show_drawdown: bool,
    /// View mode for multi-curve display
    pub view_mode: ChartViewMode,
    /// Per-ticker equity curves (for MultiTicker mode)
    pub ticker_curves: Vec<TickerCurve>,
    /// Portfolio aggregate equity curve
    pub portfolio_curve: Vec<f64>,
    /// Per-strategy equity curves (for StrategyComparison mode)
    pub strategy_curves: Vec<StrategyCurve>,
    /// Per-ticker best strategy results (for PerTickerBestStrategy mode)
    pub ticker_best_strategies: Vec<TickerBestStrategy>,
    /// OHLC candle data for candlestick view
    pub candle_data: Vec<CandleData>,
    /// Currently selected symbol for candlestick view
    pub candle_symbol: Option<String>,
    /// Show volume subplot
    pub show_volume: bool,
    /// Cursor state for crosshair
    pub cursor: CursorState,
    /// Animation state for smooth zoom/pan
    pub animation: AnimationState,
    /// Crosshair visibility
    pub show_crosshair: bool,
    /// Last chart rendering area (for hit-testing)
    pub chart_area: Cell<Option<Rect>>,
    /// Winning config display string (for Pine Script export)
    pub winning_config: Option<WinningConfig>,
}

/// Winning configuration info for display and Pine export
#[derive(Debug, Clone, Default)]
pub struct WinningConfig {
    pub strategy_name: String,
    pub config_display: String,
    pub symbol: Option<String>,
}

impl ChartState {
    /// Check if we have multi-curve data
    pub fn has_multi_curves(&self) -> bool {
        !self.ticker_curves.is_empty()
    }

    /// Check if we have strategy comparison data
    pub fn has_strategy_curves(&self) -> bool {
        !self.strategy_curves.is_empty()
    }

    /// Check if we have candlestick data
    pub fn has_candle_data(&self) -> bool {
        !self.candle_data.is_empty()
    }

    /// Cycle through chart view modes
    pub fn cycle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ChartViewMode::Single => {
                if self.has_candle_data() {
                    ChartViewMode::Candlestick
                } else if self.has_multi_curves() {
                    ChartViewMode::MultiTicker
                } else if self.has_strategy_curves() {
                    ChartViewMode::StrategyComparison
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::Candlestick => {
                if self.has_multi_curves() {
                    ChartViewMode::MultiTicker
                } else if self.has_strategy_curves() {
                    ChartViewMode::StrategyComparison
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::MultiTicker => ChartViewMode::Portfolio,
            ChartViewMode::Portfolio => {
                if self.has_strategy_curves() {
                    ChartViewMode::StrategyComparison
                } else if self.has_candle_data() {
                    ChartViewMode::Candlestick
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::StrategyComparison => {
                if !self.ticker_best_strategies.is_empty() {
                    ChartViewMode::PerTickerBestStrategy
                } else if self.has_candle_data() {
                    ChartViewMode::Candlestick
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::PerTickerBestStrategy => {
                if self.has_candle_data() {
                    ChartViewMode::Candlestick
                } else {
                    ChartViewMode::Single
                }
            }
        };
    }

    /// Get view mode name
    pub fn view_mode_name(&self) -> &'static str {
        match self.view_mode {
            ChartViewMode::Single => "Single",
            ChartViewMode::Candlestick => "Candlestick",
            ChartViewMode::MultiTicker => "Multi-Ticker",
            ChartViewMode::Portfolio => "Portfolio",
            ChartViewMode::StrategyComparison => "Strategy Comparison",
            ChartViewMode::PerTickerBestStrategy => "Per-Ticker Best",
        }
    }

    /// Update candle data from bars
    pub fn update_candle_data(&mut self, bars: &[Bar], symbol: &str) {
        self.candle_data = bars
            .iter()
            .enumerate()
            .map(|(i, bar)| CandleData {
                index: i,
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
                date: bar.ts.format("%Y-%m-%d").to_string(),
            })
            .collect();
        self.candle_symbol = Some(symbol.to_string());
    }

    /// Animated zoom in
    pub fn zoom_in_animated(&mut self) {
        self.animation.target_zoom = (self.animation.target_zoom * 1.2).min(4.0);
        self.animation.animating = true;
    }

    /// Animated zoom out
    pub fn zoom_out_animated(&mut self) {
        self.animation.target_zoom = (self.animation.target_zoom / 1.2).max(0.25);
        self.animation.animating = true;
    }
}

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
                        .map(|n| round_to_step_f64((n as f64) * entry_factor, 5.0).max(5.0) as i32)
                        .map(|n| n.to_string())
                        .collect();
                } else if name == "exit_lookback" {
                    *values = values
                        .iter()
                        .filter_map(|v| v.parse::<i32>().ok())
                        .map(|n| round_to_step_f64((n as f64) * exit_factor, 5.0).max(5.0) as i32)
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
                    session_id: generate_session_id(),
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
        if let Some(chart_area) = self.chart.chart_area.get() {
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
    pub fn start_yolo_mode(&mut self, channels: &WorkerChannels) {
        let selected = self.data.selected_tickers_sorted();
        if selected.is_empty() {
            self.status_message = "YOLO Mode: No tickers selected.".to_string();
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
            start: self.fetch_range.0,
            end: self.fetch_range.1,
            strategy_grid,
            backtest_config,
            randomization_pct: self.yolo.randomization_pct,
            existing_per_symbol_leaderboard,
            existing_cross_symbol_leaderboard,
            session_id: Some(self.yolo.session_id.clone()),
        };

        if channels.command_tx.send(cmd).is_ok() {
            self.yolo.enabled = true;
            self.yolo.started_at = Some(Utc::now());
            self.sweep.is_running = true;
            self.status_message = format!(
                "YOLO Mode starting: {} symbols (press ESC to stop)...",
                selected.len()
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

/// Scan the parquet directory for available symbols
fn scan_parquet_directory() -> Vec<String> {
    let parquet_dir = std::path::Path::new("data/parquet/1d");

    if !parquet_dir.exists() {
        return vec![];
    }

    let mut symbols = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parquet_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("symbol=") {
                if let Some(symbol) = name.strip_prefix("symbol=") {
                    symbols.push(symbol.to_string());
                }
            }
        }
    }

    symbols.sort();
    symbols
}

/// Calculate drawdown curve from equity curve
fn calculate_drawdown(equity: &[f64]) -> Vec<f64> {
    let mut max_equity = 0.0_f64;
    equity
        .iter()
        .map(|&eq| {
            max_equity = max_equity.max(eq);
            if max_equity > 0.0 {
                (eq / max_equity - 1.0) * 100.0
            } else {
                0.0
            }
        })
        .collect()
}
