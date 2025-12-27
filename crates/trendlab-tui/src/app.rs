//! Application state for TrendLab TUI

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::NaiveDate;
use trendlab_core::{
    BacktestConfig, Bar, CostModel, FillModel, MultiSweepResult, PyramidConfig, Sector,
    SweepConfigResult, SweepGrid, Universe,
};

use crate::worker::{WorkerChannels, WorkerCommand};

/// Panel identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Data,
    Strategy,
    Sweep,
    Results,
    Chart,
}

impl Panel {
    pub fn all() -> &'static [Panel] {
        &[
            Panel::Data,
            Panel::Strategy,
            Panel::Sweep,
            Panel::Results,
            Panel::Chart,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            Panel::Data => "Data",
            Panel::Strategy => "Strategy",
            Panel::Sweep => "Sweep",
            Panel::Results => "Results",
            Panel::Chart => "Chart",
        }
    }

    pub fn hotkey(&self) -> char {
        match self {
            Panel::Data => '1',
            Panel::Strategy => '2',
            Panel::Sweep => '3',
            Panel::Results => '4',
            Panel::Chart => '5',
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
        self.universe.get_sector_by_index(self.selected_sector_index)
    }

    /// Get tickers in the currently selected sector.
    pub fn current_sector_tickers(&self) -> &[String] {
        self.selected_sector()
            .map(|s| s.tickers.as_slice())
            .unwrap_or(&[])
    }

    /// Get the currently focused ticker (in ticker view mode).
    pub fn focused_ticker(&self) -> Option<&String> {
        self.current_sector_tickers().get(self.selected_ticker_index)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategyType {
    #[default]
    Donchian,
    TurtleS1,
    TurtleS2,
    MACrossover,
    Tsmom,
}

impl StrategyType {
    pub fn all() -> &'static [StrategyType] {
        &[
            StrategyType::Donchian,
            StrategyType::TurtleS1,
            StrategyType::TurtleS2,
            StrategyType::MACrossover,
            StrategyType::Tsmom,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            StrategyType::Donchian => "Donchian Breakout",
            StrategyType::TurtleS1 => "Turtle System 1",
            StrategyType::TurtleS2 => "Turtle System 2",
            StrategyType::MACrossover => "MA Crossover",
            StrategyType::Tsmom => "TSMOM",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            StrategyType::Donchian => "Breakout above N-day high, exit below M-day low",
            StrategyType::TurtleS1 => "Classic Turtle: 20-day entry, 10-day exit",
            StrategyType::TurtleS2 => "Turtle variant: 55-day entry, 20-day exit",
            StrategyType::MACrossover => "Enter on golden cross, exit on death cross",
            StrategyType::Tsmom => "Time-series momentum: long when return > 0",
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

/// Strategy panel state
#[derive(Debug)]
pub struct StrategyState {
    pub selected_type: StrategyType,
    pub selected_type_index: usize,
    pub donchian_config: DonchianConfig,
    pub ma_config: MACrossoverConfig,
    pub tsmom_config: TsmomConfig,
    pub selected_field: usize,
    pub editing_strategy: bool, // true = selecting strategy (left), false = editing params (right)
}

impl Default for StrategyState {
    fn default() -> Self {
        Self {
            selected_type: StrategyType::default(),
            selected_type_index: 0,
            donchian_config: DonchianConfig::default(),
            ma_config: MACrossoverConfig::default(),
            tsmom_config: TsmomConfig::default(),
            selected_field: 0,
            editing_strategy: true, // Start on strategy selection (left panel)
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
    pub fn to_sweep_grid(&self) -> SweepGrid {
        let entry_lookbacks: Vec<usize> = self
            .param_ranges
            .iter()
            .find(|(name, _)| name == "entry_lookback")
            .map(|(_, values)| values.iter().filter_map(|v| v.parse().ok()).collect())
            .unwrap_or_else(|| vec![10, 20, 30, 40, 50]);

        let exit_lookbacks: Vec<usize> = self
            .param_ranges
            .iter()
            .find(|(name, _)| name == "exit_lookback")
            .map(|(_, values)| values.iter().filter_map(|v| v.parse().ok()).collect())
            .unwrap_or_else(|| vec![5, 10, 15, 20, 25]);

        SweepGrid::new(entry_lookbacks, exit_lookbacks)
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
    /// Per-ticker summaries (derived from multi_sweep_result)
    pub ticker_summaries: Vec<TickerSummary>,
    /// Selected ticker index (in PerTicker view)
    pub selected_ticker_index: usize,
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
                    .max_by(|a, b| a.metrics.cagr.partial_cmp(&b.metrics.cagr).unwrap())
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
            self.ticker_summaries.sort_by(|a, b| a.symbol.cmp(&b.symbol));
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
                    ResultsViewMode::SingleSymbol
                }
            }
            ResultsViewMode::PerTicker => ResultsViewMode::Aggregated,
            ResultsViewMode::Aggregated => ResultsViewMode::PerTicker,
        };
        self.selected_ticker_index = 0;
    }

    /// Get the current view mode name
    pub fn view_mode_name(&self) -> &'static str {
        match self.view_mode {
            ResultsViewMode::SingleSymbol => "Single Symbol",
            ResultsViewMode::PerTicker => "Per-Ticker",
            ResultsViewMode::Aggregated => "Portfolio",
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
}

/// Per-ticker equity curve for multi-curve display
#[derive(Debug, Clone)]
pub struct TickerCurve {
    pub symbol: String,
    pub equity: Vec<f64>,
}

/// Chart panel state
#[derive(Debug, Default)]
pub struct ChartState {
    pub equity_curve: Vec<f64>,
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
}

impl ChartState {
    /// Check if we have multi-curve data
    pub fn has_multi_curves(&self) -> bool {
        !self.ticker_curves.is_empty()
    }

    /// Cycle through chart view modes
    pub fn cycle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ChartViewMode::Single => {
                if self.has_multi_curves() {
                    ChartViewMode::MultiTicker
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::MultiTicker => ChartViewMode::Portfolio,
            ChartViewMode::Portfolio => ChartViewMode::Single,
        };
    }

    /// Get view mode name
    pub fn view_mode_name(&self) -> &'static str {
        match self.view_mode {
            ChartViewMode::Single => "Single",
            ChartViewMode::MultiTicker => "Multi-Ticker",
            ChartViewMode::Portfolio => "Portfolio",
        }
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
    pub status_message: String,
    pub operation: OperationState,
}

impl App {
    pub fn new() -> Self {
        // Scan for existing symbols on startup
        let symbols = scan_parquet_directory();

        // Load universe from config (or use default)
        let mut data_state = DataState {
            symbols,
            ..Default::default()
        };
        data_state.load_universe_from_config();

        Self {
            active_panel: Panel::Data,
            data: data_state,
            strategy: StrategyState::default(),
            sweep: SweepState {
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
            },
            results: ResultsState::default(),
            chart: ChartState {
                zoom_level: 1.0,
                ..Default::default()
            },
            status_message: "Welcome to TrendLab TUI. Press Tab to switch panels, q to quit."
                .to_string(),
            operation: OperationState::Idle,
        }
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
            self.active_panel = panels[index];
        }
    }

    /// Toggle between sub-panels within Strategy panel
    /// Returns true if toggle was handled (i.e., we're in Strategy panel)
    pub fn toggle_strategy_focus(&mut self) -> bool {
        if self.active_panel == Panel::Strategy {
            // Don't toggle to params if strategy has no configurable params
            let param_count = self.strategy.selected_type.param_count();
            if self.strategy.editing_strategy && param_count == 0 {
                self.status_message = "This strategy has fixed parameters.".to_string();
                return true;
            }
            self.strategy.editing_strategy = !self.strategy.editing_strategy;
            true
        } else {
            false
        }
    }

    pub fn handle_up(&mut self) {
        match self.active_panel {
            Panel::Data => {
                match self.data.view_mode {
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
                }
            }
            Panel::Strategy => {
                if self.strategy.editing_strategy {
                    // Up/Down cycles through strategy types (left panel)
                    if self.strategy.selected_type_index > 0 {
                        self.strategy.selected_type_index -= 1;
                        self.strategy.selected_type =
                            StrategyType::all()[self.strategy.selected_type_index];
                        self.strategy.selected_field = 0;
                    }
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
                if self.results.selected_index > 0 {
                    self.results.selected_index -= 1;
                }
            }
            Panel::Chart => {
                self.chart.zoom_level = (self.chart.zoom_level * 1.1).min(4.0);
            }
        }
    }

    pub fn handle_down(&mut self) {
        match self.active_panel {
            Panel::Data => {
                match self.data.view_mode {
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
                }
            }
            Panel::Strategy => {
                if self.strategy.editing_strategy {
                    // Up/Down cycles through strategy types (left panel)
                    let max_idx = StrategyType::all().len() - 1;
                    if self.strategy.selected_type_index < max_idx {
                        self.strategy.selected_type_index += 1;
                        self.strategy.selected_type =
                            StrategyType::all()[self.strategy.selected_type_index];
                        self.strategy.selected_field = 0;
                    }
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
                if !self.results.results.is_empty()
                    && self.results.selected_index < self.results.results.len() - 1
                {
                    self.results.selected_index += 1;
                }
            }
            Panel::Chart => {
                self.chart.zoom_level = (self.chart.zoom_level / 1.1).max(0.25);
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
            _ => {}
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
                    // Check if we have data loaded
                    if let Some(bars) = self.data.selected_bars() {
                        if bars.is_empty() {
                            self.status_message =
                                "No bars loaded. Select a symbol and press Enter in Data panel."
                                    .to_string();
                            return;
                        }

                        let grid = self.sweep.to_sweep_grid();
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
                        let cmd = WorkerCommand::StartSweep {
                            bars: Arc::new(bars.clone()),
                            grid,
                            backtest_config,
                        };

                        if channels.command_tx.send(cmd).is_ok() {
                            self.sweep.is_running = true;
                            self.sweep.progress = 0.0;
                            self.status_message = "Starting sweep...".to_string();
                        } else {
                            self.status_message = "Failed to start sweep.".to_string();
                        }
                    } else {
                        self.status_message =
                            "Load data first! Go to Data panel and press Enter.".to_string();
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
                        // Calculate drawdown curve
                        self.chart.drawdown_curve = calculate_drawdown(&self.chart.equity_curve);
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

        let cmd = WorkerCommand::FetchData {
            symbols: symbols.clone(),
            start: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            force: false,
        };

        if channels.command_tx.send(cmd).is_ok() {
            self.status_message = format!("Fetching {} symbols...", symbols.len());
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

        if !self.results.has_multi_sweep() {
            self.status_message = "Run a multi-symbol sweep first.".to_string();
            return;
        }

        self.results.cycle_view_mode();
        self.status_message = format!("View: {}", self.results.view_mode_name());
    }

    /// Handle 'm' key to toggle chart view mode (single vs multi-ticker vs portfolio)
    pub fn handle_toggle_chart_mode(&mut self) {
        if self.active_panel != Panel::Chart {
            return;
        }

        if !self.chart.has_multi_curves() {
            self.status_message = "Run a multi-symbol sweep first.".to_string();
            return;
        }

        self.chart.cycle_view_mode();
        self.status_message = format!("Chart: {}", self.chart.view_mode_name());
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

    /// Handle 'a' key to select all tickers in current sector.
    pub fn handle_select_all(&mut self) {
        if self.active_panel != Panel::Data {
            return;
        }

        if self.data.view_mode == DataViewMode::Tickers {
            self.data.select_all_in_sector();
            if let Some(sector) = self.data.selected_sector() {
                let count = sector.len();
                self.status_message = format!("Selected all {} tickers in {}", count, sector.name);
            }
        }
    }

    /// Handle 'n' key to deselect all tickers in current sector.
    pub fn handle_select_none(&mut self) {
        if self.active_panel != Panel::Data {
            return;
        }

        if self.data.view_mode == DataViewMode::Tickers {
            self.data.deselect_all_in_sector();
            if let Some(sector) = self.data.selected_sector() {
                self.status_message = format!("Deselected all tickers in {}", sector.name);
            }
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
        self.data.bars_cache.insert(symbol.to_string(), all_bars);

        if let Some((start, end)) = date_range {
            self.status_message = format!("{}: {} bars ({} to {})", symbol, bar_count, start, end);
        } else {
            self.status_message = format!("{}: {} bars loaded", symbol, bar_count);
        }
    }

    /// Update data info for selected symbol
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
