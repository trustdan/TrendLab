use crate::commands::chart::{ChartMode, ChartState};
use crate::commands::results::{ResultRow, ResultsState, SortMetric, ViewMode};
use crate::commands::strategy::{EnsembleConfig, StrategyParamValues};
use crate::commands::sweep::SweepState;
use crate::commands::yolo::YoloState;
use crate::jobs::Jobs;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::RwLock;
use trendlab_core::{Sector, Universe};
use trendlab_launcher::ipc::{CompanionClient, CompanionEvent, JobType};

/// Configuration for the data layer paths.
#[derive(Debug, Clone)]
pub struct DataConfig {
    /// Base directory for all data (typically "data")
    pub data_dir: PathBuf,
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
        }
    }
}

impl DataConfig {
    /// Path to raw cache directory.
    pub fn raw_dir(&self) -> PathBuf {
        self.data_dir.join("raw")
    }

    /// Path to normalized Parquet directory.
    pub fn parquet_dir(&self) -> PathBuf {
        self.data_dir.join("parquet")
    }

    /// Path to quality reports directory.
    pub fn reports_dir(&self) -> PathBuf {
        self.data_dir.join("reports")
    }
}

/// Serializable sector for frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SectorInfo {
    pub id: String,
    pub name: String,
    pub tickers: Vec<String>,
}

impl From<&Sector> for SectorInfo {
    fn from(sector: &Sector) -> Self {
        Self {
            id: sector.id.clone(),
            name: sector.name.clone(),
            tickers: sector.tickers.clone(),
        }
    }
}

/// Serializable universe for frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UniverseInfo {
    pub name: String,
    pub description: String,
    pub sectors: Vec<SectorInfo>,
}

impl From<&Universe> for UniverseInfo {
    fn from(universe: &Universe) -> Self {
        Self {
            name: universe.name.clone(),
            description: universe.description.clone(),
            sectors: universe.sectors.iter().map(SectorInfo::from).collect(),
        }
    }
}

pub struct AppState {
    /// Job lifecycle tracking
    pub jobs: Jobs,
    /// Universe of sectors and tickers
    pub universe: RwLock<Universe>,
    /// Currently selected tickers for operations
    pub selected_tickers: RwLock<HashSet<String>>,
    /// Symbols that have cached parquet data
    pub cached_symbols: RwLock<HashSet<String>>,
    /// Data directory configuration
    pub data_config: DataConfig,
    /// Currently selected strategies
    pub selected_strategies: RwLock<HashSet<String>>,
    /// Strategy parameter values (strategy_id -> params)
    pub strategy_params: RwLock<HashMap<String, StrategyParamValues>>,
    /// Ensemble configuration
    pub ensemble_config: RwLock<EnsembleConfig>,
    /// Sweep panel state
    pub sweep: RwLock<SweepState>,
    /// Results panel state
    pub results: RwLock<ResultsState>,
    /// Chart panel state
    pub chart: RwLock<ChartState>,
    /// YOLO mode state
    pub yolo: RwLock<YoloState>,
    /// Companion client for IPC (if launched from unified launcher)
    pub companion: RwLock<CompanionClient>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            jobs: Jobs::new(),
            universe: RwLock::new(Universe::default_universe()),
            selected_tickers: RwLock::new(HashSet::new()),
            cached_symbols: RwLock::new(HashSet::new()),
            data_config: DataConfig::default(),
            selected_strategies: RwLock::new(HashSet::new()),
            strategy_params: RwLock::new(HashMap::new()),
            ensemble_config: RwLock::new(EnsembleConfig::default()),
            sweep: RwLock::new(SweepState::default()),
            results: RwLock::new(ResultsState::default()),
            chart: RwLock::new(ChartState::default()),
            yolo: RwLock::new(YoloState::default()),
            companion: RwLock::new(CompanionClient::noop()),
        }
    }

    /// Initialize companion client if launched from unified launcher.
    /// This should be called from an async context.
    pub async fn init_companion(&self) {
        if let Some(client) = CompanionClient::try_connect().await {
            client.send_started().await;
            let mut companion = self.companion.write().unwrap();
            *companion = client;
        }
    }

    /// Emit an event to the companion terminal.
    pub async fn emit_to_companion(&self, event: CompanionEvent) {
        let client = self.companion.read().unwrap().clone();
        client.emit(event).await;
    }

    /// Send shutdown signal to companion.
    pub async fn shutdown_companion(&self) {
        let client = self.companion.read().unwrap().clone();
        client.send_shutdown().await;
    }

    // ========================================================================
    // Companion Event Helpers
    // ========================================================================

    /// Emit job started event to companion.
    pub async fn companion_job_started(&self, job_id: &str, job_type: JobType, description: &str) {
        self.emit_to_companion(CompanionEvent::JobStarted {
            job_id: job_id.to_string(),
            job_type,
            description: description.to_string(),
        })
        .await;
    }

    /// Emit job progress event to companion.
    pub async fn companion_job_progress(
        &self,
        job_id: &str,
        current: u64,
        total: u64,
        message: &str,
    ) {
        self.emit_to_companion(CompanionEvent::JobProgress {
            job_id: job_id.to_string(),
            current,
            total,
            message: message.to_string(),
        })
        .await;
    }

    /// Emit job complete event to companion.
    pub async fn companion_job_complete(&self, job_id: &str, summary: &str, elapsed_ms: u64) {
        self.emit_to_companion(CompanionEvent::JobComplete {
            job_id: job_id.to_string(),
            summary: summary.to_string(),
            elapsed_ms,
        })
        .await;
    }

    /// Emit job failed event to companion.
    pub async fn companion_job_failed(&self, job_id: &str, error: &str) {
        self.emit_to_companion(CompanionEvent::JobFailed {
            job_id: job_id.to_string(),
            error: error.to_string(),
        })
        .await;
    }

    /// Emit sweep result event to companion.
    pub async fn companion_sweep_result(
        &self,
        ticker: &str,
        strategy: &str,
        config_id: &str,
        sharpe: f64,
        cagr: f64,
        max_dd: f64,
    ) {
        self.emit_to_companion(CompanionEvent::SweepResult {
            ticker: ticker.to_string(),
            strategy: strategy.to_string(),
            config_id: config_id.to_string(),
            sharpe,
            cagr,
            max_dd,
        })
        .await;
    }

    /// Initialize cached symbols by scanning the parquet directory.
    pub fn init_cached_symbols(&self) {
        let parquet_dir = self.data_config.parquet_dir().join("1d");
        if let Ok(entries) = std::fs::read_dir(&parquet_dir) {
            let mut cached = self.cached_symbols.write().unwrap();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Directory name format: "symbol=AAPL"
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if let Some(symbol) = name.strip_prefix("symbol=") {
                            cached.insert(symbol.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Get universe as serializable struct.
    pub fn get_universe_info(&self) -> UniverseInfo {
        let universe = self.universe.read().unwrap();
        UniverseInfo::from(&*universe)
    }

    /// Get list of cached symbols.
    pub fn get_cached_symbols(&self) -> Vec<String> {
        let cached = self.cached_symbols.read().unwrap();
        let mut symbols: Vec<_> = cached.iter().cloned().collect();
        symbols.sort();
        symbols
    }

    /// Add a symbol to the cached set.
    pub fn add_cached_symbol(&self, symbol: &str) {
        let mut cached = self.cached_symbols.write().unwrap();
        cached.insert(symbol.to_string());
    }

    /// Get selected tickers.
    pub fn get_selected_tickers(&self) -> Vec<String> {
        let selected = self.selected_tickers.read().unwrap();
        let mut tickers: Vec<_> = selected.iter().cloned().collect();
        tickers.sort();
        tickers
    }

    /// Set selected tickers.
    pub fn set_selected_tickers(&self, tickers: Vec<String>) {
        let mut selected = self.selected_tickers.write().unwrap();
        *selected = tickers.into_iter().collect();
    }

    // ========================================================================
    // Strategy State Methods
    // ========================================================================

    /// Get selected strategies.
    pub fn get_selected_strategies(&self) -> Vec<String> {
        let selected = self.selected_strategies.read().unwrap();
        let mut strategies: Vec<_> = selected.iter().cloned().collect();
        strategies.sort();
        strategies
    }

    /// Set selected strategies.
    pub fn set_selected_strategies(&self, strategies: Vec<String>) {
        let mut selected = self.selected_strategies.write().unwrap();
        *selected = strategies.into_iter().collect();
    }

    /// Get parameter values for a strategy.
    pub fn get_strategy_params(&self, strategy_id: &str) -> StrategyParamValues {
        let params = self.strategy_params.read().unwrap();
        params.get(strategy_id).cloned().unwrap_or_default()
    }

    /// Set parameter values for a strategy.
    pub fn set_strategy_params(&self, strategy_id: &str, values: StrategyParamValues) {
        let mut params = self.strategy_params.write().unwrap();
        params.insert(strategy_id.to_string(), values);
    }

    /// Get ensemble configuration.
    pub fn get_ensemble_config(&self) -> EnsembleConfig {
        let config = self.ensemble_config.read().unwrap();
        config.clone()
    }

    /// Set ensemble enabled state.
    pub fn set_ensemble_enabled(&self, enabled: bool) {
        let mut config = self.ensemble_config.write().unwrap();
        config.enabled = enabled;
    }

    // ========================================================================
    // Results State Methods
    // ========================================================================

    /// Get results state.
    pub fn get_results_state(&self) -> ResultsState {
        self.results.read().unwrap().clone()
    }

    /// Set results from a sweep.
    pub fn set_results(&self, sweep_id: String, results: Vec<ResultRow>) {
        let mut state = self.results.write().unwrap();
        state.sweep_id = Some(sweep_id);
        state.results = results;
        state.selected_id = None;
    }

    /// Add a single result (for streaming during sweep).
    pub fn add_result(&self, result: ResultRow) {
        let mut state = self.results.write().unwrap();
        state.results.push(result);
    }

    /// Set selected result ID.
    pub fn set_selected_result(&self, result_id: Option<String>) {
        let mut state = self.results.write().unwrap();
        state.selected_id = result_id;
    }

    /// Set view mode.
    pub fn set_view_mode(&self, view_mode: ViewMode) {
        let mut state = self.results.write().unwrap();
        state.view_mode = view_mode;
    }

    /// Set sort configuration.
    pub fn set_sort_config(&self, sort_by: SortMetric, ascending: bool) {
        let mut state = self.results.write().unwrap();
        state.sort_by = sort_by;
        state.ascending = ascending;
    }

    /// Clear results.
    pub fn clear_results(&self) {
        let mut state = self.results.write().unwrap();
        *state = ResultsState::default();
    }

    // ========================================================================
    // Chart State Methods
    // ========================================================================

    /// Get chart state.
    pub fn get_chart_state(&self) -> ChartState {
        self.chart.read().unwrap().clone()
    }

    /// Set chart mode.
    pub fn set_chart_mode(&self, mode: ChartMode) {
        let mut state = self.chart.write().unwrap();
        state.mode = mode;
    }

    /// Set chart selection (symbol/strategy/config).
    pub fn set_chart_selection(
        &self,
        symbol: Option<String>,
        strategy: Option<String>,
        config_id: Option<String>,
    ) {
        let mut state = self.chart.write().unwrap();
        state.symbol = symbol;
        state.strategy = strategy;
        state.config_id = config_id;
    }

    /// Toggle chart overlay.
    pub fn toggle_chart_overlay(&self, overlay: &str, enabled: bool) {
        let mut state = self.chart.write().unwrap();
        match overlay {
            "drawdown" => state.overlays.drawdown = enabled,
            "volume" => state.overlays.volume = enabled,
            "trades" => state.overlays.trades = enabled,
            "crosshair" => state.overlays.crosshair = enabled,
            _ => {}
        }
    }
}
