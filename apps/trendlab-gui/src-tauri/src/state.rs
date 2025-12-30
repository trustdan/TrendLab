//! Application state for the GUI - wraps trendlab-engine's App.
//!
//! The engine's App struct is the single source of truth for all application state.
//! This module adds GUI-specific state (jobs, companion IPC) while delegating
//! core state management to the engine.

use crate::jobs::Jobs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use trendlab_core::{Sector, SweepDepth, Universe};
use trendlab_engine::app::{App, StrategyType};
use trendlab_engine::worker::WorkerCommand;

/// Parse a strategy ID string to StrategyType enum.
fn parse_strategy_type(id: &str) -> Option<StrategyType> {
    match id.to_lowercase().as_str() {
        "donchian" => Some(StrategyType::Donchian),
        "keltner" => Some(StrategyType::Keltner),
        "starc" => Some(StrategyType::STARC),
        "supertrend" => Some(StrategyType::Supertrend),
        "ma_crossover" | "macrossover" => Some(StrategyType::MACrossover),
        "tsmom" => Some(StrategyType::Tsmom),
        "turtle_s1" | "turtles1" => Some(StrategyType::TurtleS1),
        "turtle_s2" | "turtles2" => Some(StrategyType::TurtleS2),
        "parabolic_sar" | "parabolicsar" => Some(StrategyType::ParabolicSar),
        "opening_range" | "openingrange" => Some(StrategyType::OpeningRange),
        _ => None,
    }
}

/// Convert StrategyType to lowercase string ID.
fn strategy_type_to_id(st: &StrategyType) -> String {
    match st {
        StrategyType::Donchian => "donchian".to_string(),
        StrategyType::TurtleS1 => "turtle_s1".to_string(),
        StrategyType::TurtleS2 => "turtle_s2".to_string(),
        StrategyType::MACrossover => "ma_crossover".to_string(),
        StrategyType::Tsmom => "tsmom".to_string(),
        StrategyType::Keltner => "keltner".to_string(),
        StrategyType::STARC => "starc".to_string(),
        StrategyType::Supertrend => "supertrend".to_string(),
        StrategyType::ParabolicSar => "parabolic_sar".to_string(),
        StrategyType::OpeningRange => "opening_range".to_string(),
    }
}
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

/// Main application state for the GUI.
///
/// Contains the engine's App struct as the single source of truth for core state,
/// plus GUI-specific state like jobs and companion IPC.
///
/// Note: WorkerChannels is NOT included here because mpsc::Receiver is not Sync.
/// Instead, the worker is managed separately via the command_tx sender.
/// The update_rx is processed in a separate async task that forwards to React.
pub struct AppState {
    /// The engine's App struct - single source of truth for all core state
    pub engine: RwLock<App>,
    /// Command sender for background operations (std::sync::mpsc::Sender is Sync)
    pub command_tx: Sender<WorkerCommand>,
    /// Cancellation flag for in-progress operations
    pub cancel_flag: Arc<AtomicBool>,
    /// Job lifecycle tracking (GUI-specific)
    pub jobs: Jobs,
    /// Data directory configuration
    pub data_config: DataConfig,
    /// Companion client for IPC (if launched from unified launcher)
    pub companion: RwLock<CompanionClient>,
    /// Cached symbols (GUI-specific - tracks what parquet data exists locally)
    pub cached_symbols: RwLock<std::collections::HashSet<String>>,
    /// Cost model configuration (GUI-specific - stored separately from engine)
    pub cost_model: RwLock<trendlab_core::CostModel>,
    /// Sweep depth configuration (GUI-specific - not stored in engine)
    pub sweep_depth: RwLock<SweepDepth>,
}

impl AppState {
    /// Create new AppState with command sender and cancel flag from worker channels.
    /// The update_rx should be processed separately in an async task.
    pub fn new(command_tx: Sender<WorkerCommand>, cancel_flag: Arc<AtomicBool>) -> Self {
        let engine = App::new();

        Self {
            engine: RwLock::new(engine),
            command_tx,
            cancel_flag,
            jobs: Jobs::new(),
            data_config: DataConfig::default(),
            companion: RwLock::new(CompanionClient::noop()),
            cached_symbols: RwLock::new(std::collections::HashSet::new()),
            cost_model: RwLock::new(trendlab_core::CostModel {
                fees_bps_per_side: 5.0,
                slippage_bps: 5.0,
            }),
            sweep_depth: RwLock::new(SweepDepth::Standard),
        }
    }

    /// Send a command to the worker
    #[allow(clippy::result_large_err)]
    pub fn send_command(
        &self,
        cmd: WorkerCommand,
    ) -> Result<(), std::sync::mpsc::SendError<WorkerCommand>> {
        self.command_tx.send(cmd)
    }

    /// Request cancellation of the current operation
    pub fn request_cancel(&self) {
        self.cancel_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Clear the cancellation flag (call before starting a new operation)
    pub fn clear_cancel(&self) {
        self.cancel_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    // ========================================================================
    // Engine Access
    // ========================================================================

    /// Get read access to the engine state
    pub fn engine_read(&self) -> std::sync::RwLockReadGuard<'_, App> {
        self.engine.read().unwrap()
    }

    /// Get write access to the engine state
    pub fn engine_write(&self) -> std::sync::RwLockWriteGuard<'_, App> {
        self.engine.write().unwrap()
    }

    // ========================================================================
    // Companion/IPC Methods (GUI-specific)
    // ========================================================================

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

    // ========================================================================
    // Data/Universe Methods (delegated to engine)
    // ========================================================================

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
        let engine = self.engine.read().unwrap();
        UniverseInfo::from(&engine.data.universe)
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
        let engine = self.engine.read().unwrap();
        let mut tickers: Vec<_> = engine.data.selected_tickers.iter().cloned().collect();
        tickers.sort();
        tickers
    }

    /// Set selected tickers.
    pub fn set_selected_tickers(&self, tickers: Vec<String>) {
        let mut engine = self.engine.write().unwrap();
        engine.data.selected_tickers = tickers.into_iter().collect();
    }

    // ========================================================================
    // Strategy Methods (delegated to engine)
    // ========================================================================

    /// Get selected strategies.
    pub fn get_selected_strategies(&self) -> Vec<String> {
        let engine = self.engine.read().unwrap();
        engine
            .strategy
            .selected_strategies
            .iter()
            .map(strategy_type_to_id)
            .collect()
    }

    /// Set strategy selection by ID.
    pub fn set_strategy_selected(&self, strategy_id: &str, selected: bool) {
        let mut engine = self.engine.write().unwrap();
        // Parse strategy_id to StrategyType
        if let Some(strategy_type) = parse_strategy_type(strategy_id) {
            if selected {
                engine.strategy.selected_strategies.insert(strategy_type);
            } else {
                engine.strategy.selected_strategies.remove(&strategy_type);
            }
        }
    }

    /// Set selected strategies by list of IDs.
    pub fn set_selected_strategies(&self, strategy_ids: Vec<String>) {
        let mut engine = self.engine.write().unwrap();
        engine.strategy.selected_strategies.clear();
        for id in strategy_ids {
            if let Some(strategy_type) = parse_strategy_type(&id) {
                engine.strategy.selected_strategies.insert(strategy_type);
            }
        }
    }

    /// Get strategy params (placeholder - returns empty map for now).
    pub fn get_strategy_params(
        &self,
        _strategy_id: &str,
    ) -> crate::commands::strategy::StrategyParamValues {
        // TODO: Store per-strategy params in engine
        crate::commands::strategy::StrategyParamValues::default()
    }

    /// Set strategy params (placeholder - no-op for now).
    pub fn set_strategy_params(
        &self,
        _strategy_id: &str,
        _params: crate::commands::strategy::StrategyParamValues,
    ) {
        // TODO: Store per-strategy params in engine
    }

    /// Get ensemble config.
    pub fn get_ensemble_config(&self) -> crate::commands::strategy::EnsembleConfig {
        let engine = self.engine.read().unwrap();
        crate::commands::strategy::EnsembleConfig {
            enabled: engine.strategy.ensemble.enabled,
            voting_method: format!("{:?}", engine.strategy.ensemble.voting),
        }
    }

    /// Set ensemble enabled state.
    pub fn set_ensemble_enabled(&self, enabled: bool) {
        let mut engine = self.engine.write().unwrap();
        engine.strategy.ensemble.enabled = enabled;
    }

    // ========================================================================
    // Chart Methods (delegated to engine)
    // ========================================================================

    /// Get chart view mode.
    pub fn get_chart_view_mode(&self) -> trendlab_engine::app::ChartViewMode {
        let engine = self.engine.read().unwrap();
        engine.chart.view_mode
    }

    /// Get chart overlays state.
    pub fn get_chart_overlays(&self) -> (bool, bool, bool) {
        let engine = self.engine.read().unwrap();
        (
            engine.chart.show_drawdown,
            engine.chart.show_volume,
            engine.chart.show_crosshair,
        )
    }

    /// Get candle symbol.
    pub fn get_candle_symbol(&self) -> Option<String> {
        let engine = self.engine.read().unwrap();
        engine.chart.candle_symbol.clone()
    }

    /// Get selected result index.
    pub fn get_selected_result_index(&self) -> Option<usize> {
        let engine = self.engine.read().unwrap();
        engine.chart.selected_result_index
    }

    /// Set chart view mode.
    pub fn set_chart_mode(&self, mode: trendlab_engine::app::ChartViewMode) {
        let mut engine = self.engine.write().unwrap();
        engine.chart.view_mode = mode;
    }

    /// Set chart selection (symbol for candle view).
    pub fn set_chart_selection(
        &self,
        symbol: Option<String>,
        _strategy: Option<String>,
        _config_id: Option<String>,
    ) {
        let mut engine = self.engine.write().unwrap();
        // Only symbol is stored in engine's chart state for candlestick view
        engine.chart.candle_symbol = symbol;
        // Note: strategy and config_id are matched via results panel selection
    }

    /// Toggle a chart overlay.
    pub fn toggle_chart_overlay(&self, overlay: &str, enabled: bool) {
        let mut engine = self.engine.write().unwrap();
        match overlay {
            "drawdown" => engine.chart.show_drawdown = enabled,
            "volume" => engine.chart.show_volume = enabled,
            "crosshair" => engine.chart.show_crosshair = enabled,
            _ => {}
        }
    }

    // ========================================================================
    // Results Methods (delegated to engine)
    // ========================================================================

    /// Get results count.
    pub fn get_results_count(&self) -> usize {
        let engine = self.engine.read().unwrap();
        engine.results.results.len()
    }

    /// Check if results exist.
    pub fn has_results(&self) -> bool {
        let engine = self.engine.read().unwrap();
        !engine.results.results.is_empty()
    }

    /// Set results (from sweep completion).
    pub fn set_results(&self, results: Vec<trendlab_core::SweepConfigResult>) {
        let mut engine = self.engine.write().unwrap();
        engine.results.results = results;
    }

    /// Get results view mode.
    pub fn get_view_mode(&self) -> trendlab_engine::app::ResultsViewMode {
        let engine = self.engine.read().unwrap();
        engine.results.view_mode
    }

    /// Set results view mode.
    pub fn set_view_mode(&self, mode: trendlab_engine::app::ResultsViewMode) {
        let mut engine = self.engine.write().unwrap();
        engine.results.view_mode = mode;
    }

    /// Set selected result by ID.
    pub fn set_selected_result(&self, result_id: Option<String>) {
        let mut engine = self.engine.write().unwrap();
        if let Some(id) = result_id {
            // Find the index of the result with this ID
            if let Some(idx) = engine.results.results.iter().position(|r| {
                // Match by config_id string representation
                r.config_id.id() == id
            }) {
                engine.results.selected_index = idx;
            }
        }
    }

    /// Set sort column for results.
    pub fn set_sort_column(&self, column: usize) {
        let mut engine = self.engine.write().unwrap();
        engine.results.sort_column = column;
    }

    /// Clear all results.
    pub fn clear_results(&self) {
        let mut engine = self.engine.write().unwrap();
        engine.results.results.clear();
        engine.results.selected_index = 0;
    }

    // ========================================================================
    // Sweep Methods (GUI-specific state)
    // ========================================================================

    /// Get sweep depth.
    pub fn get_sweep_depth(&self) -> trendlab_core::SweepDepth {
        let depth = self.sweep_depth.read().unwrap();
        *depth
    }

    /// Set sweep depth.
    pub fn set_sweep_depth(&self, depth: trendlab_core::SweepDepth) {
        let mut d = self.sweep_depth.write().unwrap();
        *d = depth;
    }

    // ========================================================================
    // YOLO Methods (delegated to engine)
    // ========================================================================

    /// Get YOLO state.
    pub fn get_yolo_state(&self) -> trendlab_engine::app::YoloState {
        let engine = self.engine.read().unwrap();
        engine.yolo.clone()
    }

    /// Check if YOLO mode is running.
    pub fn is_yolo_running(&self) -> bool {
        let engine = self.engine.read().unwrap();
        engine.yolo.enabled
    }

    /// Set YOLO running state.
    pub fn set_yolo_running(&self, running: bool) {
        let mut engine = self.engine.write().unwrap();
        engine.yolo.enabled = running;
    }

    /// Get YOLO iteration count.
    pub fn get_yolo_iteration(&self) -> u32 {
        let engine = self.engine.read().unwrap();
        engine.yolo.iteration
    }

    /// Get per-symbol leaderboard (session).
    pub fn get_per_symbol_leaderboard(&self) -> Option<trendlab_core::Leaderboard> {
        let engine = self.engine.read().unwrap();
        Some(engine.yolo.session_leaderboard.clone())
    }

    /// Get cross-symbol leaderboard (session).
    pub fn get_cross_symbol_leaderboard(&self) -> Option<trendlab_core::CrossSymbolLeaderboard> {
        let engine = self.engine.read().unwrap();
        engine.yolo.cross_symbol_leaderboard().cloned()
    }

    // ========================================================================
    // Sweep State Access for Commands
    // ========================================================================

    /// Get sweep running state.
    pub fn is_sweep_running(&self) -> bool {
        let engine = self.engine.read().unwrap();
        matches!(
            engine.operation,
            trendlab_engine::app::OperationState::RunningSweep { .. }
        )
    }

    /// Get sweep cost model.
    /// Note: Cost model is stored in GUI state, not engine.
    pub fn get_cost_model(&self) -> trendlab_core::CostModel {
        let cost = self.cost_model.read().unwrap();
        *cost
    }

    /// Set sweep cost model.
    pub fn set_cost_model(&self, cost_model: trendlab_core::CostModel) {
        let mut cost = self.cost_model.write().unwrap();
        *cost = cost_model;
    }

    /// Get sweep date range from engine's fetch_range.
    pub fn get_date_range(&self) -> (chrono::NaiveDate, chrono::NaiveDate) {
        let engine = self.engine.read().unwrap();
        engine.fetch_range
    }

    /// Set sweep date range in engine's fetch_range.
    pub fn set_date_range(&self, start: chrono::NaiveDate, end: chrono::NaiveDate) {
        let mut engine = self.engine.write().unwrap();
        engine.fetch_range = (start, end);
    }
}
