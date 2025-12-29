//! Background worker thread for async operations.
//!
//! Handles:
//! - Yahoo Finance data fetching (async HTTP)
//! - Parameter sweeps (parallel via Rayon)
//! - Cancellation via atomic flag
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use chrono::{NaiveDate, Utc};
use std::collections::HashMap;
use tracing::{debug, info, trace};
use trendlab_core::{
    bars_to_dataframe, combine_equity_curves_simple, compute_analysis, one_sided_mean_pvalue,
    run_donchian_sweep_polars, run_strategy_sweep_polars_parallel, scan_symbol_parquet_lazy,
    AggregatedConfigResult, AggregatedMetrics, AggregatedPortfolioResult, AnalysisConfig,
    BacktestConfig, BacktestResult, Bar, CostModel, CrossSymbolLeaderboard, CrossSymbolRankMetric,
    DataQualityChecker, DataQualityReport, DonchianBacktestConfig, IntoLazy, Leaderboard,
    LeaderboardEntry, Metrics, MultiStrategyGrid, MultiStrategySweepResult, MultiSweepResult,
    OpeningPeriod, PolarsBacktestConfig, RankMetric, StatisticalAnalysis, StrategyBestResult,
    StrategyConfigId, StrategyGridConfig, StrategyParams, StrategyTypeId, SweepConfigResult,
    SweepGrid, SweepResult, VotingMethod, WalkForwardConfig, WalkForwardResult,
};

/// Commands sent from TUI thread to worker thread.
#[derive(Debug)]
pub enum WorkerCommand {
    /// Search for symbols matching a query (autocomplete).
    SearchSymbols { query: String },

    /// Fetch data from Yahoo Finance for given symbols.
    FetchData {
        symbols: Vec<String>,
        start: NaiveDate,
        end: NaiveDate,
        force: bool,
    },

    /// Load cached data from local Parquet store for given symbols (no network).
    LoadCachedData { symbols: Vec<String> },

    /// Start a parameter sweep.
    StartSweep {
        bars: Arc<Vec<Bar>>,
        grid: SweepGrid,
        backtest_config: BacktestConfig,
        /// Use Polars-native backtest (vectorized, faster for large sweeps)
        use_polars: bool,
    },

    /// Start a multi-ticker parameter sweep.
    StartMultiSweep {
        /// Map of symbol -> bars
        symbol_bars: HashMap<String, Arc<Vec<Bar>>>,
        grid: SweepGrid,
        backtest_config: BacktestConfig,
    },

    /// Start a multi-strategy sweep (all strategies across all symbols).
    StartMultiStrategySweep {
        /// Map of symbol -> bars
        symbol_bars: HashMap<String, Arc<Vec<Bar>>>,
        /// Strategy grid with configs for each strategy type
        strategy_grid: MultiStrategyGrid,
        backtest_config: BacktestConfig,
    },

    /// Start a single-symbol sweep directly from Parquet (Phase 4 direct pipeline).
    /// This skips the Vec<Bar> intermediate for maximum performance.
    StartSweepFromParquet {
        symbol: String,
        start: NaiveDate,
        end: NaiveDate,
        grid: SweepGrid,
        backtest_config: BacktestConfig,
        /// Use Polars-native backtest (always true for this command)
        use_polars: bool,
    },

    /// Start a multi-strategy sweep directly from Parquet (Phase 4 direct pipeline).
    /// This scans Parquet files directly into LazyFrames for maximum performance.
    StartMultiStrategySweepFromParquet {
        /// List of symbols to sweep
        symbols: Vec<String>,
        /// Date range
        start: NaiveDate,
        end: NaiveDate,
        /// Strategy grid with configs for each strategy type
        strategy_grid: MultiStrategyGrid,
        backtest_config: BacktestConfig,
    },

    /// Cancel the current operation.
    Cancel,

    /// Shutdown the worker thread.
    Shutdown,

    /// Compute statistical analysis for a backtest result.
    ComputeAnalysis {
        /// Identifier for this analysis (e.g., config_id string)
        analysis_id: String,
        /// The backtest result to analyze
        backtest_result: BacktestResult,
        /// Bar data for MAE/MFE and regime analysis
        bars: Arc<Vec<Bar>>,
        /// Analysis configuration
        config: AnalysisConfig,
    },

    /// Start YOLO mode - continuous auto-optimization loop.
    /// Runs multi-strategy sweeps with randomized parameters until cancelled.
    StartYoloMode {
        /// Symbols to sweep
        symbols: Vec<String>,
        /// Optional mapping of symbol -> sector_id (e.g., "AAPL" -> "technology")
        ///
        /// If provided, YOLO results can be enriched with sector info for sector-aware validation.
        symbol_sector_ids: HashMap<String, String>,
        /// Date range start
        start: NaiveDate,
        /// Date range end
        end: NaiveDate,
        /// Base strategy grid (will be jittered each iteration)
        strategy_grid: MultiStrategyGrid,
        /// Backtest configuration
        backtest_config: BacktestConfig,
        /// Randomization percentage (e.g., 0.15 = +/-15%)
        randomization_pct: f64,
        /// Optional: existing per-symbol leaderboard to continue from
        existing_per_symbol_leaderboard: Option<Leaderboard>,
        /// Optional: existing cross-symbol leaderboard to continue from
        existing_cross_symbol_leaderboard: Option<CrossSymbolLeaderboard>,
    },
}

/// A symbol search result from Yahoo.
#[derive(Debug, Clone)]
pub struct SymbolSearchResult {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub type_disp: String,
}

/// Updates sent from worker thread back to TUI thread.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum WorkerUpdate {
    // Symbol search updates
    SearchResults {
        query: String,
        results: Vec<SymbolSearchResult>,
    },
    SearchError {
        query: String,
        error: String,
    },

    // Data fetch updates
    FetchStarted {
        symbol: String,
        index: usize,
        total: usize,
    },
    FetchComplete {
        symbol: String,
        bars: Vec<Bar>,
        quality: DataQualityReport,
    },
    FetchError {
        symbol: String,
        error: String,
    },
    FetchAllComplete {
        symbols_fetched: usize,
    },

    // Cache load updates
    CacheLoadStarted {
        symbol: String,
        index: usize,
        total: usize,
    },
    CacheLoadComplete {
        symbol: String,
        bars: Vec<Bar>,
    },
    CacheLoadError {
        symbol: String,
        error: String,
    },
    CacheLoadAllComplete {
        symbols_loaded: usize,
        symbols_missing: usize,
    },

    // Sweep updates
    SweepStarted {
        total_configs: usize,
    },
    SweepProgress {
        completed: usize,
        total: usize,
    },
    SweepComplete {
        result: SweepResult,
    },
    SweepCancelled {
        completed: usize,
    },

    // Multi-sweep updates
    MultiSweepStarted {
        total_symbols: usize,
        configs_per_symbol: usize,
    },
    MultiSweepSymbolStarted {
        symbol: String,
        symbol_index: usize,
        total_symbols: usize,
    },
    MultiSweepSymbolComplete {
        symbol: String,
        result: SweepResult,
    },
    MultiSweepComplete {
        result: MultiSweepResult,
    },
    MultiSweepCancelled {
        completed_symbols: usize,
    },

    // Multi-strategy sweep updates
    MultiStrategySweepStarted {
        total_symbols: usize,
        total_strategies: usize,
        total_configs: usize,
    },
    MultiStrategySweepStrategyStarted {
        symbol: String,
        strategy_type: StrategyTypeId,
    },
    MultiStrategySweepProgress {
        completed_configs: usize,
        total_configs: usize,
        current_strategy: StrategyTypeId,
        current_symbol: String,
    },
    MultiStrategySweepComplete {
        result: MultiStrategySweepResult,
    },
    MultiStrategySweepCancelled {
        completed_configs: usize,
    },

    // Statistical analysis updates
    AnalysisStarted {
        analysis_id: String,
    },
    AnalysisComplete {
        analysis_id: String,
        analysis: StatisticalAnalysis,
    },
    AnalysisError {
        analysis_id: String,
        error: String,
    },

    // YOLO mode updates
    YoloModeStarted {
        total_symbols: usize,
        total_strategies: usize,
    },
    YoloIterationComplete {
        iteration: u32,
        /// Best cross-symbol aggregated result this round (primary)
        best_aggregate: Option<AggregatedConfigResult>,
        /// Best per-symbol result this round (secondary)
        best_per_symbol: Option<StrategyBestResult>,
        /// Cross-symbol leaderboard (primary - ranked by avg sharpe across symbols)
        cross_symbol_leaderboard: CrossSymbolLeaderboard,
        /// Per-symbol leaderboard (secondary)
        per_symbol_leaderboard: Leaderboard,
        configs_tested_this_round: usize,
    },
    YoloProgress {
        iteration: u32,
        phase: String,
        completed_configs: usize,
        total_configs: usize,
    },
    YoloStopped {
        /// Cross-symbol leaderboard (primary)
        cross_symbol_leaderboard: CrossSymbolLeaderboard,
        /// Per-symbol leaderboard (secondary)
        per_symbol_leaderboard: Leaderboard,
        total_iterations: u32,
        total_configs_tested: u64,
    },

    // General
    Ready,
    Idle,
}

/// Channels for communicating with the worker thread.
pub struct WorkerChannels {
    pub command_tx: Sender<WorkerCommand>,
    pub update_rx: Receiver<WorkerUpdate>,
    pub cancel_flag: Arc<AtomicBool>,
}

/// Spawn the background worker thread.
///
/// Returns channels for communication and the thread handle.
pub fn spawn_worker() -> (WorkerChannels, JoinHandle<()>) {
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let (update_tx, update_rx) = std::sync::mpsc::channel();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_clone = cancel_flag.clone();

    let handle = thread::spawn(move || {
        worker_loop(command_rx, update_tx, cancel_flag_clone);
    });

    let channels = WorkerChannels {
        command_tx,
        update_rx,
        cancel_flag,
    };

    (channels, handle)
}

/// Main worker loop - runs in background thread.
fn worker_loop(
    command_rx: Receiver<WorkerCommand>,
    update_tx: Sender<WorkerUpdate>,
    cancel_flag: Arc<AtomicBool>,
) {
    // Create a Tokio runtime for async operations
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    // Signal ready
    let _ = update_tx.send(WorkerUpdate::Ready);

    while let Ok(cmd) = command_rx.recv() {
        // Reset cancel flag for new operation
        cancel_flag.store(false, Ordering::SeqCst);

        match cmd {
            WorkerCommand::SearchSymbols { query } => {
                rt.block_on(handle_search(&query, &update_tx));
            }

            WorkerCommand::FetchData {
                symbols,
                start,
                end,
                force,
            } => {
                rt.block_on(handle_fetch(
                    &symbols,
                    start,
                    end,
                    force,
                    &update_tx,
                    &cancel_flag,
                ));
            }

            WorkerCommand::LoadCachedData { symbols } => {
                handle_load_cached(&symbols, &update_tx, &cancel_flag);
            }

            WorkerCommand::StartSweep {
                bars,
                grid,
                backtest_config,
                use_polars,
            } => {
                if use_polars {
                    handle_sweep_polars(&bars, &grid, backtest_config, &update_tx, &cancel_flag);
                } else {
                    handle_sweep(&bars, &grid, backtest_config, &update_tx, &cancel_flag);
                }
            }

            WorkerCommand::StartMultiSweep {
                symbol_bars,
                grid,
                backtest_config,
            } => {
                handle_multi_sweep(
                    symbol_bars,
                    &grid,
                    backtest_config,
                    &update_tx,
                    &cancel_flag,
                );
            }

            WorkerCommand::StartMultiStrategySweep {
                symbol_bars,
                strategy_grid,
                backtest_config,
            } => {
                handle_multi_strategy_sweep(
                    symbol_bars,
                    &strategy_grid,
                    backtest_config,
                    &update_tx,
                    &cancel_flag,
                );
            }

            WorkerCommand::StartSweepFromParquet {
                symbol,
                start,
                end,
                grid,
                backtest_config,
                use_polars: _,
            } => {
                handle_sweep_from_parquet(
                    &symbol,
                    start,
                    end,
                    &grid,
                    backtest_config,
                    &update_tx,
                    &cancel_flag,
                );
            }

            WorkerCommand::StartMultiStrategySweepFromParquet {
                symbols,
                start,
                end,
                strategy_grid,
                backtest_config,
            } => {
                handle_multi_strategy_sweep_from_parquet(
                    &symbols,
                    start,
                    end,
                    &strategy_grid,
                    backtest_config,
                    &update_tx,
                    &cancel_flag,
                );
            }

            WorkerCommand::ComputeAnalysis {
                analysis_id,
                backtest_result,
                bars,
                config,
            } => {
                handle_compute_analysis(&analysis_id, &backtest_result, &bars, &config, &update_tx);
            }

            WorkerCommand::StartYoloMode {
                symbols,
                symbol_sector_ids,
                start,
                end,
                strategy_grid,
                backtest_config,
                randomization_pct,
                existing_per_symbol_leaderboard,
                existing_cross_symbol_leaderboard,
            } => {
                handle_yolo_mode(
                    &symbols,
                    &symbol_sector_ids,
                    start,
                    end,
                    &strategy_grid,
                    backtest_config,
                    randomization_pct,
                    existing_per_symbol_leaderboard,
                    existing_cross_symbol_leaderboard,
                    &update_tx,
                    &cancel_flag,
                );
            }

            WorkerCommand::Cancel => {
                // Set the flag - the running operation will check it
                cancel_flag.store(true, Ordering::SeqCst);
            }

            WorkerCommand::Shutdown => {
                break;
            }
        }

        // Signal idle after each operation
        let _ = update_tx.send(WorkerUpdate::Idle);
    }
}

/// Load cached bars for a list of symbols from local Parquet store.
fn handle_load_cached(
    symbols: &[String],
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    use std::path::Path;
    use trendlab_core::read_parquet;

    let total = symbols.len();
    let mut loaded = 0usize;
    let mut missing = 0usize;

    let parquet_dir = Path::new("data/parquet/1d");

    for (index, symbol) in symbols.iter().enumerate() {
        if cancel_flag.load(Ordering::SeqCst) {
            return;
        }

        let _ = update_tx.send(WorkerUpdate::CacheLoadStarted {
            symbol: symbol.clone(),
            index,
            total,
        });

        let symbol_dir = parquet_dir.join(format!("symbol={}", symbol));
        if !symbol_dir.exists() {
            missing += 1;
            let _ = update_tx.send(WorkerUpdate::CacheLoadError {
                symbol: symbol.clone(),
                error: "No cached data (missing parquet directory)".to_string(),
            });
            continue;
        }

        let mut all_bars: Vec<Bar> = Vec::new();
        match std::fs::read_dir(&symbol_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let data_file = path.join("data.parquet");
                        if data_file.exists() {
                            match read_parquet(&data_file) {
                                Ok(mut bars) => all_bars.append(&mut bars),
                                Err(e) => {
                                    let _ = update_tx.send(WorkerUpdate::CacheLoadError {
                                        symbol: symbol.clone(),
                                        error: format!("Parquet read error: {}", e),
                                    });
                                    all_bars.clear();
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                let _ = update_tx.send(WorkerUpdate::CacheLoadError {
                    symbol: symbol.clone(),
                    error: format!("IO error: {}", e),
                });
                continue;
            }
        }

        if all_bars.is_empty() {
            missing += 1;
            let _ = update_tx.send(WorkerUpdate::CacheLoadError {
                symbol: symbol.clone(),
                error: "No bars found in cache".to_string(),
            });
            continue;
        }

        all_bars.sort_by_key(|b| b.ts);
        loaded += 1;
        let _ = update_tx.send(WorkerUpdate::CacheLoadComplete {
            symbol: symbol.clone(),
            bars: all_bars,
        });
    }

    let _ = update_tx.send(WorkerUpdate::CacheLoadAllComplete {
        symbols_loaded: loaded,
        symbols_missing: missing,
    });
}

/// Handle data fetch operation (async).
async fn handle_fetch(
    symbols: &[String],
    start: NaiveDate,
    end: NaiveDate,
    _force: bool,
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    use std::path::Path;
    use trendlab_core::{build_yahoo_chart_url, parse_yahoo_chart_json, write_partitioned_parquet};

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .unwrap_or_default();

    let total = symbols.len();
    let mut fetched = 0;

    // Base directory for Parquet cache
    let parquet_dir = Path::new("data/parquet");

    for (index, symbol) in symbols.iter().enumerate() {
        // Check cancellation
        if cancel_flag.load(Ordering::SeqCst) {
            return;
        }

        let _ = update_tx.send(WorkerUpdate::FetchStarted {
            symbol: symbol.clone(),
            index,
            total,
        });

        // Use the chart API (v8) which doesn't require authentication
        let url = build_yahoo_chart_url(symbol, start, end);

        match client.get(&url).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    let _ = update_tx.send(WorkerUpdate::FetchError {
                        symbol: symbol.clone(),
                        error: format!("HTTP {}", response.status()),
                    });
                    continue;
                }

                match response.text().await {
                    Ok(json_text) => match parse_yahoo_chart_json(&json_text, symbol, "1d") {
                        Ok(bars) => {
                            let checker = DataQualityChecker::new();
                            let quality = checker.check(&bars);

                            // Persist to Parquet cache
                            if let Err(e) = write_partitioned_parquet(&bars, parquet_dir) {
                                let _ = update_tx.send(WorkerUpdate::FetchError {
                                    symbol: symbol.clone(),
                                    error: format!("Parquet write error: {}", e),
                                });
                                continue;
                            }

                            let _ = update_tx.send(WorkerUpdate::FetchComplete {
                                symbol: symbol.clone(),
                                bars,
                                quality,
                            });
                            fetched += 1;
                        }
                        Err(e) => {
                            let _ = update_tx.send(WorkerUpdate::FetchError {
                                symbol: symbol.clone(),
                                error: format!("Parse error: {}", e),
                            });
                        }
                    },
                    Err(e) => {
                        let _ = update_tx.send(WorkerUpdate::FetchError {
                            symbol: symbol.clone(),
                            error: format!("Read error: {}", e),
                        });
                    }
                }
            }
            Err(e) => {
                let _ = update_tx.send(WorkerUpdate::FetchError {
                    symbol: symbol.clone(),
                    error: format!("Network error: {}", e),
                });
            }
        }
    }

    let _ = update_tx.send(WorkerUpdate::FetchAllComplete {
        symbols_fetched: fetched,
    });
}

/// Handle sweep operation (parallel via Rayon).
fn handle_sweep(
    bars: &[Bar],
    grid: &SweepGrid,
    config: BacktestConfig,
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    use rayon::prelude::*;
    use trendlab_core::{compute_metrics, run_backtest, ConfigId, DonchianBreakoutStrategy};

    let combinations = grid.combinations();
    let total = combinations.len();

    let _ = update_tx.send(WorkerUpdate::SweepStarted {
        total_configs: total,
    });

    // Atomic counter for progress
    let completed = Arc::new(AtomicUsize::new(0));
    let report_interval = (total / 100).max(1); // Report ~100 times

    // Clone values needed for parallel closure
    let completed_clone = completed.clone();
    let cancel_flag_clone = cancel_flag.clone();
    let update_tx_clone = update_tx.clone();

    let results: Vec<SweepConfigResult> = combinations
        .par_iter()
        .filter_map(|&(entry, exit)| {
            // Check cancellation
            if cancel_flag_clone.load(Ordering::SeqCst) {
                return None;
            }

            let config_id = ConfigId::new(entry, exit);
            let mut strategy = DonchianBreakoutStrategy::new(entry, exit);

            let backtest_result = match run_backtest(bars, &mut strategy, config) {
                Ok(r) => r,
                Err(_) => return None,
            };

            let metrics = compute_metrics(&backtest_result, config.initial_cash);

            let result = SweepConfigResult {
                config_id,
                backtest_result,
                metrics,
            };

            // Update progress
            let count = completed_clone.fetch_add(1, Ordering::SeqCst) + 1;

            // Report periodically
            if count.is_multiple_of(report_interval) || count == total {
                let _ = update_tx_clone.send(WorkerUpdate::SweepProgress {
                    completed: count,
                    total,
                });
            }

            Some(result)
        })
        .collect();

    // Check if cancelled
    let final_completed = completed.load(Ordering::SeqCst);
    if cancel_flag.load(Ordering::SeqCst) {
        let _ = update_tx.send(WorkerUpdate::SweepCancelled {
            completed: final_completed,
        });
        return;
    }

    let sweep_result = SweepResult {
        sweep_id: format!("tui_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S")),
        config_results: results,
        started_at: chrono::Utc::now(), // Approximate - we could track actual start
        completed_at: chrono::Utc::now(),
    };

    let _ = update_tx.send(WorkerUpdate::SweepComplete {
        result: sweep_result,
    });
}

/// Handle sweep operation using Polars-native backtest (vectorized).
fn handle_sweep_polars(
    bars: &[Bar],
    grid: &SweepGrid,
    config: BacktestConfig,
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    use trendlab_core::{compute_metrics, ConfigId};

    let combinations = grid.combinations();
    let total = combinations.len();

    let _ = update_tx.send(WorkerUpdate::SweepStarted {
        total_configs: total,
    });

    // Convert bars to DataFrame once
    let df = match bars_to_dataframe(bars) {
        Ok(df) => df,
        Err(e) => {
            let _ = update_tx.send(WorkerUpdate::SweepCancelled { completed: 0 });
            eprintln!("Failed to convert bars to DataFrame: {}", e);
            return;
        }
    };

    // Build configs for Polars sweep
    let polars_configs: Vec<DonchianBacktestConfig> = combinations
        .iter()
        .map(|&(entry, exit)| {
            DonchianBacktestConfig::new(entry, exit)
                .with_initial_cash(config.initial_cash)
                .with_qty(config.qty)
                .with_cost_model(config.cost_model)
        })
        .collect();

    // Check for cancellation before running sweep
    if cancel_flag.load(Ordering::SeqCst) {
        let _ = update_tx.send(WorkerUpdate::SweepCancelled { completed: 0 });
        return;
    }

    // Run Polars sweep (uses indicator reuse optimization)
    let polars_results = match run_donchian_sweep_polars(df.lazy(), &polars_configs) {
        Ok(results) => results,
        Err(e) => {
            let _ = update_tx.send(WorkerUpdate::SweepCancelled { completed: 0 });
            eprintln!("Polars sweep failed: {}", e);
            return;
        }
    };

    // Convert Polars results to SweepConfigResult format
    let results: Vec<SweepConfigResult> = combinations
        .iter()
        .zip(polars_results.iter())
        .filter_map(|(&(entry, exit), polars_result)| {
            // Convert PolarsBacktestResult to BacktestResult
            let backtest_result = match polars_result.to_backtest_result() {
                Ok(r) => r,
                Err(_) => return None,
            };

            let metrics = compute_metrics(&backtest_result, config.initial_cash);

            Some(SweepConfigResult {
                config_id: ConfigId::new(entry, exit),
                backtest_result,
                metrics,
            })
        })
        .collect();

    // Send progress update (all at once for Polars)
    let _ = update_tx.send(WorkerUpdate::SweepProgress {
        completed: results.len(),
        total,
    });

    let sweep_result = SweepResult {
        sweep_id: format!("tui_polars_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S")),
        config_results: results,
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
    };

    let _ = update_tx.send(WorkerUpdate::SweepComplete {
        result: sweep_result,
    });
}

/// Handle multi-ticker sweep operation.
fn handle_multi_sweep(
    symbol_bars: HashMap<String, Arc<Vec<Bar>>>,
    grid: &SweepGrid,
    config: BacktestConfig,
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    use trendlab_core::{compute_metrics, run_backtest, ConfigId, DonchianBreakoutStrategy};

    let total_symbols = symbol_bars.len();
    let configs_per_symbol = grid.len();

    let _ = update_tx.send(WorkerUpdate::MultiSweepStarted {
        total_symbols,
        configs_per_symbol,
    });

    let sweep_id = format!("multi_tui_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let mut multi_result = MultiSweepResult::new(sweep_id);
    let started_at = chrono::Utc::now();

    let mut symbols_completed = 0;

    // Sort symbols for deterministic ordering
    let mut symbols: Vec<String> = symbol_bars.keys().cloned().collect();
    symbols.sort();

    for (symbol_index, symbol) in symbols.iter().enumerate() {
        // Check cancellation
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = update_tx.send(WorkerUpdate::MultiSweepCancelled {
                completed_symbols: symbols_completed,
            });
            return;
        }

        let _ = update_tx.send(WorkerUpdate::MultiSweepSymbolStarted {
            symbol: symbol.clone(),
            symbol_index,
            total_symbols,
        });

        let bars = match symbol_bars.get(symbol) {
            Some(b) => b,
            None => continue,
        };

        // Run sweep for this symbol
        let combinations = grid.combinations();
        let symbol_sweep_id = format!("{}_{}", multi_result.sweep_id, symbol);
        let symbol_started = chrono::Utc::now();

        let config_results: Vec<SweepConfigResult> = combinations
            .iter()
            .filter_map(|&(entry, exit)| {
                if cancel_flag.load(Ordering::SeqCst) {
                    return None;
                }

                let config_id = ConfigId::new(entry, exit);
                let mut strategy = DonchianBreakoutStrategy::new(entry, exit);

                let backtest_result = match run_backtest(bars, &mut strategy, config) {
                    Ok(r) => r,
                    Err(_) => return None,
                };

                let metrics = compute_metrics(&backtest_result, config.initial_cash);

                Some(SweepConfigResult {
                    config_id,
                    backtest_result,
                    metrics,
                })
            })
            .collect();

        let symbol_result = SweepResult {
            sweep_id: symbol_sweep_id,
            config_results,
            started_at: symbol_started,
            completed_at: chrono::Utc::now(),
        };

        // Send symbol complete update
        let _ = update_tx.send(WorkerUpdate::MultiSweepSymbolComplete {
            symbol: symbol.clone(),
            result: symbol_result.clone(),
        });

        multi_result.add_symbol_result(symbol.clone(), symbol_result);
        symbols_completed += 1;
    }

    // Compute aggregated portfolio results
    multi_result.aggregated = AggregatedPortfolioResult::from_symbol_results(
        &multi_result.symbol_results,
        RankMetric::Sharpe,
    );
    multi_result.started_at = started_at;
    multi_result.completed_at = chrono::Utc::now();

    let _ = update_tx.send(WorkerUpdate::MultiSweepComplete {
        result: multi_result,
    });
}

/// Handle sweep from Parquet directly (Phase 4 - no Vec<Bar> intermediate).
///
/// This loads data directly from Parquet files into a LazyFrame,
/// avoiding the Vec<Bar> conversion overhead.
fn handle_sweep_from_parquet(
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
    grid: &SweepGrid,
    config: BacktestConfig,
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    use std::path::Path;
    use trendlab_core::compute_metrics;

    let parquet_dir = Path::new("data/parquet");

    // Scan Parquet directly into LazyFrame with date filtering
    let lf = match scan_symbol_parquet_lazy(parquet_dir, symbol, "1d", Some(start), Some(end)) {
        Ok(lf) => lf,
        Err(e) => {
            let _ = update_tx.send(WorkerUpdate::SweepCancelled { completed: 0 });
            eprintln!("Failed to scan Parquet for {}: {}", symbol, e);
            return;
        }
    };

    // Collect to DataFrame for the sweep
    let df = match lf.collect() {
        Ok(df) => df,
        Err(e) => {
            let _ = update_tx.send(WorkerUpdate::SweepCancelled { completed: 0 });
            eprintln!("Failed to collect Parquet data: {}", e);
            return;
        }
    };

    let total = grid.len();
    let _ = update_tx.send(WorkerUpdate::SweepStarted {
        total_configs: total,
    });

    // Check for cancellation
    if cancel_flag.load(Ordering::SeqCst) {
        let _ = update_tx.send(WorkerUpdate::SweepCancelled { completed: 0 });
        return;
    }

    // Build Polars configs for sweep
    let combinations = grid.combinations();
    let polars_configs: Vec<DonchianBacktestConfig> = combinations
        .iter()
        .map(|&(entry, exit)| {
            DonchianBacktestConfig::new(entry, exit)
                .with_initial_cash(config.initial_cash)
                .with_qty(config.qty)
                .with_cost_model(config.cost_model)
        })
        .collect();

    // Run Polars sweep (uses indicator reuse optimization)
    let polars_results = match run_donchian_sweep_polars(df.lazy(), &polars_configs) {
        Ok(results) => results,
        Err(e) => {
            let _ = update_tx.send(WorkerUpdate::SweepCancelled { completed: 0 });
            eprintln!("Polars sweep failed: {}", e);
            return;
        }
    };

    // Convert Polars results to SweepConfigResult format
    use trendlab_core::ConfigId;
    let results: Vec<SweepConfigResult> = combinations
        .iter()
        .zip(polars_results.iter())
        .filter_map(|(&(entry, exit), polars_result)| {
            let backtest_result = match polars_result.to_backtest_result() {
                Ok(r) => r,
                Err(_) => return None,
            };

            let metrics = compute_metrics(&backtest_result, config.initial_cash);

            Some(SweepConfigResult {
                config_id: ConfigId::new(entry, exit),
                backtest_result,
                metrics,
            })
        })
        .collect();

    // Send progress update
    let _ = update_tx.send(WorkerUpdate::SweepProgress {
        completed: results.len(),
        total,
    });

    let sweep_result = SweepResult {
        sweep_id: format!("tui_parquet_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S")),
        config_results: results,
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
    };

    let _ = update_tx.send(WorkerUpdate::SweepComplete {
        result: sweep_result,
    });
}

/// Handle multi-strategy sweep from Parquet directly (Phase 4 - no Vec<Bar> intermediate).
///
/// This scans Parquet files directly into LazyFrames for each symbol,
/// avoiding the Vec<Bar> conversion overhead.
fn handle_multi_strategy_sweep_from_parquet(
    symbols: &[String],
    start: NaiveDate,
    end: NaiveDate,
    grid: &MultiStrategyGrid,
    config: BacktestConfig,
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    use std::path::Path;

    let parquet_dir = Path::new("data/parquet");

    let total_symbols = symbols.len();
    let enabled_strategies = grid.enabled_strategies();
    let total_strategies = enabled_strategies.len();
    let total_configs = grid.total_configs() * total_symbols;

    let _ = update_tx.send(WorkerUpdate::MultiStrategySweepStarted {
        total_symbols,
        total_strategies,
        total_configs,
    });

    let sweep_id = format!(
        "multi_strategy_parquet_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let mut result = MultiStrategySweepResult::new(sweep_id);
    let started_at = chrono::Utc::now();
    let completed_configs = Arc::new(AtomicUsize::new(0));

    // Polars backtest config
    let polars_config =
        PolarsBacktestConfig::new(config.initial_cash, config.qty).with_cost_model(CostModel {
            fees_bps_per_side: config.cost_model.fees_bps_per_side,
            slippage_bps: config.cost_model.slippage_bps,
        });

    // For each symbol
    for symbol in symbols {
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = update_tx.send(WorkerUpdate::MultiStrategySweepCancelled {
                completed_configs: completed_configs.load(Ordering::SeqCst),
            });
            return;
        }

        // Scan Parquet directly into LazyFrame
        let lf = match scan_symbol_parquet_lazy(parquet_dir, symbol, "1d", Some(start), Some(end)) {
            Ok(lf) => lf,
            Err(e) => {
                eprintln!("Failed to scan Parquet for {}: {}", symbol, e);
                continue;
            }
        };

        // Collect to DataFrame
        let df = match lf.collect() {
            Ok(df) => df,
            Err(e) => {
                eprintln!("Failed to collect Parquet data for {}: {}", symbol, e);
                continue;
            }
        };

        // For each strategy
        for strategy_config in &grid.strategies {
            if !strategy_config.enabled {
                continue;
            }
            if cancel_flag.load(Ordering::SeqCst) {
                break;
            }

            let _ = update_tx.send(WorkerUpdate::MultiStrategySweepStrategyStarted {
                symbol: symbol.clone(),
                strategy_type: strategy_config.strategy_type,
            });

            // Run Polars-native sweep
            let sweep_result =
                match run_strategy_sweep_polars_parallel(&df, strategy_config, &polars_config) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!(
                            "Polars sweep failed for {} / {:?}: {}",
                            symbol, strategy_config.strategy_type, e
                        );
                        continue;
                    }
                };

            // Update progress
            let num_configs = sweep_result.config_results.len();
            let new_count =
                completed_configs.fetch_add(num_configs, Ordering::SeqCst) + num_configs;

            let _ = update_tx.send(WorkerUpdate::MultiStrategySweepProgress {
                completed_configs: new_count,
                total_configs,
                current_strategy: strategy_config.strategy_type,
                current_symbol: symbol.clone(),
            });

            result.add_result(symbol.clone(), strategy_config.strategy_type, sweep_result);
        }
    }

    // Compute aggregations
    result.started_at = started_at;
    result.compute_aggregations();

    let _ = update_tx.send(WorkerUpdate::MultiStrategySweepComplete { result });
}

/// Handle multi-strategy sweep operation (all strategies across all symbols).
///
/// Uses Polars-native backtest for vectorized performance.
fn handle_multi_strategy_sweep(
    symbol_bars: HashMap<String, Arc<Vec<Bar>>>,
    grid: &MultiStrategyGrid,
    config: BacktestConfig,
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    let total_symbols = symbol_bars.len();
    let enabled_strategies = grid.enabled_strategies();
    let total_strategies = enabled_strategies.len();
    let total_configs = grid.total_configs() * total_symbols;

    let _ = update_tx.send(WorkerUpdate::MultiStrategySweepStarted {
        total_symbols,
        total_strategies,
        total_configs,
    });

    let sweep_id = format!(
        "multi_strategy_polars_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let mut result = MultiStrategySweepResult::new(sweep_id);
    let started_at = chrono::Utc::now();
    let completed_configs = Arc::new(AtomicUsize::new(0));

    // Polars backtest config (mirrors the BacktestConfig)
    let polars_config =
        PolarsBacktestConfig::new(config.initial_cash, config.qty).with_cost_model(CostModel {
            fees_bps_per_side: config.cost_model.fees_bps_per_side,
            slippage_bps: config.cost_model.slippage_bps,
        });

    // Sort symbols for deterministic ordering
    let mut symbols: Vec<String> = symbol_bars.keys().cloned().collect();
    symbols.sort();

    // For each symbol
    for symbol in &symbols {
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = update_tx.send(WorkerUpdate::MultiStrategySweepCancelled {
                completed_configs: completed_configs.load(Ordering::SeqCst),
            });
            return;
        }

        let bars = match symbol_bars.get(symbol) {
            Some(b) => b,
            None => continue,
        };

        // Convert bars to DataFrame for Polars backtest
        let df = match bars_to_dataframe(bars) {
            Ok(df) => df,
            Err(e) => {
                eprintln!("Failed to convert bars for {}: {}", symbol, e);
                continue;
            }
        };

        // For each strategy
        for strategy_config in &grid.strategies {
            if !strategy_config.enabled {
                continue;
            }
            if cancel_flag.load(Ordering::SeqCst) {
                break;
            }

            let _ = update_tx.send(WorkerUpdate::MultiStrategySweepStrategyStarted {
                symbol: symbol.clone(),
                strategy_type: strategy_config.strategy_type,
            });

            // Run Polars-native sweep for this strategy/symbol
            let sweep_result =
                match run_strategy_sweep_polars_parallel(&df, strategy_config, &polars_config) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!(
                            "Polars sweep failed for {} / {:?}: {}",
                            symbol, strategy_config.strategy_type, e
                        );
                        continue;
                    }
                };

            // Update progress
            let num_configs = sweep_result.config_results.len();
            let new_count =
                completed_configs.fetch_add(num_configs, Ordering::SeqCst) + num_configs;

            let _ = update_tx.send(WorkerUpdate::MultiStrategySweepProgress {
                completed_configs: new_count,
                total_configs,
                current_strategy: strategy_config.strategy_type,
                current_symbol: symbol.clone(),
            });

            // Store result - add_result expects SweepResult (which contains config_results)
            result.add_result(symbol.clone(), strategy_config.strategy_type, sweep_result);
        }
    }

    // Compute aggregations
    result.started_at = started_at;
    result.compute_aggregations();

    let _ = update_tx.send(WorkerUpdate::MultiStrategySweepComplete { result });
}

/// Handle symbol search operation (async).
async fn handle_search(query: &str, update_tx: &Sender<WorkerUpdate>) {
    // Yahoo Finance search API
    let url = format!(
        "https://query1.finance.yahoo.com/v1/finance/search?q={}&quotesCount=5&newsCount=0",
        urlencoding::encode(query)
    );

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .unwrap_or_default();

    match client.get(&url).send().await {
        Ok(response) => {
            if !response.status().is_success() {
                let _ = update_tx.send(WorkerUpdate::SearchError {
                    query: query.to_string(),
                    error: format!("HTTP {}", response.status()),
                });
                return;
            }

            match response.text().await {
                Ok(json_text) => match parse_search_response(&json_text) {
                    Ok(results) => {
                        let _ = update_tx.send(WorkerUpdate::SearchResults {
                            query: query.to_string(),
                            results,
                        });
                    }
                    Err(e) => {
                        let _ = update_tx.send(WorkerUpdate::SearchError {
                            query: query.to_string(),
                            error: e,
                        });
                    }
                },
                Err(e) => {
                    let _ = update_tx.send(WorkerUpdate::SearchError {
                        query: query.to_string(),
                        error: format!("Read error: {}", e),
                    });
                }
            }
        }
        Err(e) => {
            let _ = update_tx.send(WorkerUpdate::SearchError {
                query: query.to_string(),
                error: format!("Network error: {}", e),
            });
        }
    }
}

/// Parse Yahoo Finance search API response.
fn parse_search_response(json_text: &str) -> Result<Vec<SymbolSearchResult>, String> {
    // Parse JSON manually to avoid serde dependency
    // Response format: {"quotes":[{"symbol":"SPY","shortname":"SPDR S&P 500","exchange":"PCX","quoteType":"ETF"},...]}

    let mut results = Vec::new();

    // Find the quotes array
    let quotes_start = json_text
        .find("\"quotes\":[")
        .ok_or("No quotes array in response")?;

    let array_start = quotes_start + 10; // len of "quotes":[ (10 chars)
    let text_from_array = &json_text[array_start..];

    // Find matching ]
    let mut depth = 1;
    let mut array_end = 0;
    for (i, c) in text_from_array.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    array_end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    let array_content = &text_from_array[..array_end];

    // Parse each quote object
    let mut pos = 0;
    while let Some(obj_start) = array_content[pos..].find('{') {
        let obj_start = pos + obj_start;

        // Find matching }
        let mut depth = 1;
        let mut obj_end = obj_start + 1;
        for (i, c) in array_content[obj_start + 1..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        obj_end = obj_start + 1 + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        let obj_content = &array_content[obj_start..obj_end];

        // Extract fields
        let symbol = extract_json_string(obj_content, "symbol").unwrap_or_default();
        let name = extract_json_string(obj_content, "shortname")
            .or_else(|| extract_json_string(obj_content, "longname"))
            .unwrap_or_default();
        let exchange = extract_json_string(obj_content, "exchange").unwrap_or_default();
        let type_disp = extract_json_string(obj_content, "quoteType")
            .or_else(|| extract_json_string(obj_content, "typeDisp"))
            .unwrap_or_default();

        if !symbol.is_empty() {
            results.push(SymbolSearchResult {
                symbol,
                name,
                exchange,
                type_disp,
            });
        }

        pos = obj_end;

        // Limit to 5 results
        if results.len() >= 5 {
            break;
        }
    }

    Ok(results)
}

/// Extract a string field from a JSON object.
fn extract_json_string(json: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", field);
    let start = json.find(&pattern)? + pattern.len();
    let remaining = &json[start..];

    // Find closing quote, handling escapes
    let mut end = 0;
    let mut escaped = false;
    for (i, c) in remaining.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => {
                end = i;
                break;
            }
            _ => {}
        }
    }

    Some(remaining[..end].to_string())
}

/// Handle statistical analysis computation.
fn handle_compute_analysis(
    analysis_id: &str,
    backtest_result: &BacktestResult,
    bars: &[Bar],
    config: &AnalysisConfig,
    update_tx: &Sender<WorkerUpdate>,
) {
    let _ = update_tx.send(WorkerUpdate::AnalysisStarted {
        analysis_id: analysis_id.to_string(),
    });

    match compute_analysis(backtest_result, bars, config) {
        Ok(analysis) => {
            let _ = update_tx.send(WorkerUpdate::AnalysisComplete {
                analysis_id: analysis_id.to_string(),
                analysis,
            });
        }
        Err(e) => {
            let _ = update_tx.send(WorkerUpdate::AnalysisError {
                analysis_id: analysis_id.to_string(),
                error: format!("Analysis computation failed: {}", e),
            });
        }
    }
}

// =============================================================================
// Walk-Forward Helpers for YOLO Mode
// =============================================================================

/// Minimum in-sample Sharpe ratio required to run walk-forward validation.
/// Configs below this threshold are unlikely to generalize well.
const WF_SHARPE_THRESHOLD: f64 = 0.3;

/// Walk-forward configuration for YOLO mode - fast but still statistically valid.
fn get_yolo_wf_config() -> WalkForwardConfig {
    WalkForwardConfig::yolo_quick()
}

/// Aggregate walk-forward results across multiple symbols.
///
/// Returns:
/// - Mean OOS Sharpe across all symbols and folds
/// - Std of OOS Sharpe across folds
/// - Percentage of folds with positive OOS Sharpe
/// - Walk-forward grade (A-F)
/// - Sharpe degradation (OOS/IS ratio)
#[allow(dead_code)]
fn aggregate_wf_results(
    wf_results: &[(String, WalkForwardResult)],
) -> Option<(f64, f64, f64, char, f64)> {
    if wf_results.is_empty() {
        return None;
    }

    // Collect all fold OOS Sharpes across all symbols
    let mut all_oos_sharpes: Vec<f64> = Vec::new();
    let mut all_is_sharpes: Vec<f64> = Vec::new();
    let mut total_profitable_folds = 0usize;
    let mut total_folds = 0usize;

    for (_symbol, wf_result) in wf_results {
        for fold in &wf_result.folds {
            all_oos_sharpes.push(fold.oos_sharpe);
            all_is_sharpes.push(fold.is_sharpe);
            if fold.is_oos_profitable() {
                total_profitable_folds += 1;
            }
            total_folds += 1;
        }
    }

    if all_oos_sharpes.is_empty() {
        return None;
    }

    // Compute statistics
    let n = all_oos_sharpes.len() as f64;
    let mean_oos = all_oos_sharpes.iter().sum::<f64>() / n;
    let mean_is = all_is_sharpes.iter().sum::<f64>() / n;

    let variance_oos = all_oos_sharpes
        .iter()
        .map(|x| (x - mean_oos).powi(2))
        .sum::<f64>()
        / (n - 1.0).max(1.0);
    let std_oos = variance_oos.sqrt();

    let pct_profitable = total_profitable_folds as f64 / total_folds.max(1) as f64;
    let sharpe_degradation = if mean_is.abs() > 1e-6 {
        mean_oos / mean_is
    } else {
        0.0
    };

    // Compute grade based on aggregate metrics
    // A: OOS > 0.5, degradation > 0.7, profitable > 80%
    // B: OOS > 0.3, degradation > 0.5, profitable > 70%
    // C: OOS > 0.1, degradation > 0.3, profitable > 60%
    // D: OOS > 0.0, degradation > 0.2, profitable > 50%
    // F: Otherwise
    let grade = if mean_oos > 0.5 && sharpe_degradation > 0.7 && pct_profitable > 0.8 {
        'A'
    } else if mean_oos > 0.3 && sharpe_degradation > 0.5 && pct_profitable > 0.7 {
        'B'
    } else if mean_oos > 0.1 && sharpe_degradation > 0.3 && pct_profitable > 0.6 {
        'C'
    } else if mean_oos > 0.0 && sharpe_degradation > 0.2 && pct_profitable > 0.5 {
        'D'
    } else {
        'F'
    };

    Some((mean_oos, std_oos, pct_profitable, grade, sharpe_degradation))
}

/// Compute p-value for walk-forward results.
///
/// Tests H0: mean OOS Sharpe <= 0 vs H1: mean OOS Sharpe > 0
/// Uses one-sided t-test via `one_sided_mean_pvalue`.
#[allow(dead_code)]
fn compute_wf_pvalue(wf_results: &[(String, WalkForwardResult)]) -> Option<f64> {
    // Collect all OOS Sharpes across all symbols and folds
    let oos_sharpes: Vec<f64> = wf_results
        .iter()
        .flat_map(|(_, wf)| wf.folds.iter().map(|f| f.oos_sharpe))
        .collect();

    if oos_sharpes.len() < 3 {
        return None; // Need at least 3 samples for meaningful p-value
    }

    one_sided_mean_pvalue(&oos_sharpes).ok()
}

/// Compute simplified walk-forward metrics from equity curves.
///
/// This is a fast approximation that doesn't re-run backtests.
/// Instead, it:
/// 1. Splits each equity curve into 3 folds
/// 2. For each fold, treats first 85% as "in-sample" and last 15% as "out-of-sample"
/// 3. Computes Sharpe ratio for each portion
/// 4. Aggregates across symbols and folds
///
/// Returns (grade, mean_oos_sharpe, std_oos_sharpe, pct_profitable, sharpe_degradation, oos_p_value)
#[allow(clippy::type_complexity)]
fn compute_equity_based_wf(
    per_symbol_equity: &HashMap<String, Vec<f64>>,
) -> (
    Option<char>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
) {
    const MIN_BARS_PER_FOLD: usize = 42; // ~2 months of trading days
    const NUM_FOLDS: usize = 3;
    const IS_RATIO: f64 = 0.85; // In-sample portion of each fold

    let mut all_is_sharpes: Vec<f64> = Vec::new();
    let mut all_oos_sharpes: Vec<f64> = Vec::new();
    let mut profitable_folds = 0usize;
    let mut total_folds = 0usize;

    for equity in per_symbol_equity.values() {
        if equity.len() < MIN_BARS_PER_FOLD * NUM_FOLDS {
            continue; // Not enough data for meaningful WF
        }

        // Compute daily returns
        let returns: Vec<f64> = equity
            .windows(2)
            .map(|w| {
                if w[0] > 1e-10 {
                    (w[1] - w[0]) / w[0]
                } else {
                    0.0
                }
            })
            .collect();

        if returns.len() < MIN_BARS_PER_FOLD * NUM_FOLDS {
            continue;
        }

        // Split returns into folds
        let fold_size = returns.len() / NUM_FOLDS;
        for fold_idx in 0..NUM_FOLDS {
            let fold_start = fold_idx * fold_size;
            let fold_end = if fold_idx == NUM_FOLDS - 1 {
                returns.len()
            } else {
                fold_start + fold_size
            };

            let fold_returns = &returns[fold_start..fold_end];
            let is_end = (fold_returns.len() as f64 * IS_RATIO) as usize;

            if is_end < 20 || fold_returns.len() - is_end < 5 {
                continue; // Not enough data in split
            }

            let is_returns = &fold_returns[..is_end];
            let oos_returns = &fold_returns[is_end..];

            // Compute Sharpe for each portion (annualized)
            let is_sharpe = compute_sharpe_from_returns(is_returns);
            let oos_sharpe = compute_sharpe_from_returns(oos_returns);

            all_is_sharpes.push(is_sharpe);
            all_oos_sharpes.push(oos_sharpe);

            if oos_sharpe > 0.0 {
                profitable_folds += 1;
            }
            total_folds += 1;
        }
    }

    if all_oos_sharpes.len() < 3 {
        return (None, None, None, None, None, None);
    }

    // Compute aggregate statistics
    let n = all_oos_sharpes.len() as f64;
    let mean_oos = all_oos_sharpes.iter().sum::<f64>() / n;
    let mean_is = all_is_sharpes.iter().sum::<f64>() / n;

    let variance_oos = all_oos_sharpes
        .iter()
        .map(|x| (x - mean_oos).powi(2))
        .sum::<f64>()
        / (n - 1.0).max(1.0);
    let std_oos = variance_oos.sqrt();

    let pct_profitable = profitable_folds as f64 / total_folds.max(1) as f64;
    let sharpe_degradation = if mean_is.abs() > 1e-6 {
        mean_oos / mean_is
    } else {
        0.0
    };

    // Compute p-value
    let oos_pval = one_sided_mean_pvalue(&all_oos_sharpes).ok();

    // Compute grade
    let grade = if mean_oos > 0.5 && sharpe_degradation > 0.7 && pct_profitable > 0.8 {
        'A'
    } else if mean_oos > 0.3 && sharpe_degradation > 0.5 && pct_profitable > 0.7 {
        'B'
    } else if mean_oos > 0.1 && sharpe_degradation > 0.3 && pct_profitable > 0.6 {
        'C'
    } else if mean_oos > 0.0 && sharpe_degradation > 0.2 && pct_profitable > 0.5 {
        'D'
    } else {
        'F'
    };

    (
        Some(grade),
        Some(mean_oos),
        Some(std_oos),
        Some(pct_profitable),
        Some(sharpe_degradation),
        oos_pval,
    )
}

/// Compute annualized Sharpe ratio from daily returns.
fn compute_sharpe_from_returns(returns: &[f64]) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let std = variance.sqrt();

    if std < 1e-10 {
        return 0.0;
    }

    // Annualize (assuming 252 trading days)
    (mean / std) * (252.0_f64).sqrt()
}

// =============================================================================
// YOLO Mode Handler
// =============================================================================

/// Handle YOLO mode - continuous auto-optimization loop.
///
/// Runs multi-strategy sweeps with randomized parameters until cancelled.
/// Uses CONFIG-FIRST iteration: for each config, test across ALL symbols, then aggregate.
/// Maintains two leaderboards:
/// - CrossSymbolLeaderboard: configs ranked by aggregate performance across symbols (primary)
/// - Leaderboard: per-symbol best configs (secondary)
#[allow(clippy::too_many_arguments)]
fn handle_yolo_mode(
    symbols: &[String],
    symbol_sector_ids: &HashMap<String, String>,
    start: NaiveDate,
    end: NaiveDate,
    base_grid: &MultiStrategyGrid,
    config: BacktestConfig,
    randomization_pct: f64,
    existing_per_symbol_leaderboard: Option<Leaderboard>,
    existing_cross_symbol_leaderboard: Option<CrossSymbolLeaderboard>,
    update_tx: &Sender<WorkerUpdate>,
    cancel_flag: &Arc<AtomicBool>,
) {
    use chrono::DateTime;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::path::Path;

    info!(
        symbols = symbols.len(),
        start = %start,
        end = %end,
        randomization_pct = %randomization_pct,
        "Starting YOLO mode"
    );

    let parquet_dir = Path::new("data/parquet");
    let per_symbol_path = Path::new("artifacts/leaderboard.json");
    let cross_symbol_path = Path::new("artifacts/cross_symbol_leaderboard.json");

    // Initialize or continue leaderboards
    let mut per_symbol_leaderboard =
        existing_per_symbol_leaderboard.unwrap_or_else(|| Leaderboard::new(4));
    let mut cross_symbol_leaderboard = existing_cross_symbol_leaderboard
        .unwrap_or_else(|| CrossSymbolLeaderboard::new(4, CrossSymbolRankMetric::AvgSharpe));

    let mut iteration = cross_symbol_leaderboard.total_iterations;

    // Create seeded RNG for reproducibility (but different each run)
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    let total_symbols = symbols.len();
    let enabled_strategies = base_grid.enabled_strategies();
    let total_strategies = enabled_strategies.len();

    // Ensure the cross-symbol leaderboard has enough capacity to represent strategies.
    // In YOLO mode we care about "which strategies are attractive", so we keep the best
    // aggregated config PER strategy (rather than multiple configs of the same strategy).
    if cross_symbol_leaderboard.max_entries < total_strategies.max(4) {
        cross_symbol_leaderboard.max_entries = total_strategies.max(4);
    }
    // If the leaderboard was created by an older build, it may contain multiple entries of the
    // same strategy type. Compact it so we start from a clean "best-per-strategy" state.
    yolo_compact_to_best_per_strategy(&mut cross_symbol_leaderboard);

    // Signal YOLO mode started
    let _ = update_tx.send(WorkerUpdate::YoloModeStarted {
        total_symbols,
        total_strategies,
    });

    // Polars backtest config
    let polars_config =
        PolarsBacktestConfig::new(config.initial_cash, config.qty).with_cost_model(CostModel {
            fees_bps_per_side: config.cost_model.fees_bps_per_side,
            slippage_bps: config.cost_model.slippage_bps,
        });

    // Pre-load all symbol DataFrames to avoid repeated I/O
    let mut symbol_dfs: HashMap<String, polars::prelude::DataFrame> = HashMap::new();
    for symbol in symbols {
        if let Ok(lf) = scan_symbol_parquet_lazy(parquet_dir, symbol, "1d", Some(start), Some(end))
        {
            if let Ok(df) = lf.collect() {
                symbol_dfs.insert(symbol.clone(), df);
            }
        }
    }

    let loaded_symbols: Vec<String> = symbol_dfs.keys().cloned().collect();
    if loaded_symbols.is_empty() {
        let _ = update_tx.send(WorkerUpdate::YoloStopped {
            cross_symbol_leaderboard,
            per_symbol_leaderboard,
            total_iterations: iteration,
            total_configs_tested: 0,
        });
        return;
    }

    // Main YOLO loop - runs until cancelled
    loop {
        // Check cancellation at the start of each iteration
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = per_symbol_leaderboard.save(per_symbol_path);
            let _ = cross_symbol_leaderboard.save(cross_symbol_path);
            let total_configs = cross_symbol_leaderboard.total_configs_tested;
            let _ = update_tx.send(WorkerUpdate::YoloStopped {
                cross_symbol_leaderboard,
                per_symbol_leaderboard,
                total_iterations: iteration,
                total_configs_tested: total_configs,
            });
            return;
        }

        iteration += 1;
        per_symbol_leaderboard.total_iterations = iteration;
        cross_symbol_leaderboard.total_iterations = iteration;

        debug!(iteration = iteration, "Starting YOLO iteration");

        // 1. Jitter the grid parameters (with occasional "wide" exploration iterations)
        let wide = rng.gen_bool(0.20);
        let iter_pct = if wide {
            (randomization_pct * 3.0).min(0.90)
        } else {
            randomization_pct
        };
        let jittered_grid = jitter_multi_strategy_grid(base_grid, iter_pct, &mut rng);

        trace!(
            iteration = iteration,
            jitter_pct = %iter_pct,
            wide = wide,
            configs = jittered_grid.total_configs(),
            "Jittered grid created"
        );

        // Send progress update (include current jitter strength for visibility)
        let phase = if wide {
            format!("sweeping (wide ±{:.0}%)", iter_pct * 100.0)
        } else {
            format!("sweeping (±{:.0}%)", iter_pct * 100.0)
        };
        let _ = update_tx.send(WorkerUpdate::YoloProgress {
            iteration,
            phase,
            completed_configs: 0,
            total_configs: jittered_grid.total_configs() * loaded_symbols.len(),
        });

        let mut configs_tested_this_round = 0usize;
        let mut best_aggregate_this_round: Option<AggregatedConfigResult> = None;
        let mut best_per_symbol_this_round: Option<StrategyBestResult> = None;

        // 2. CONFIG-FIRST iteration: for each strategy config...
        for strategy_config in &jittered_grid.strategies {
            if !strategy_config.enabled {
                continue;
            }
            if cancel_flag.load(Ordering::SeqCst) {
                break;
            }

            // For each config generated by this strategy's param grid...
            // (A strategy param grid can produce multiple configs, e.g., Donchian with multiple lookbacks)
            // We need to enumerate all configs from the grid
            let config_ids = generate_config_ids_for_strategy(strategy_config);

            for strategy_config_id in config_ids {
                if cancel_flag.load(Ordering::SeqCst) {
                    break;
                }

                // Collect results across ALL symbols for this single config
                let mut per_symbol_metrics: HashMap<String, Metrics> = HashMap::new();
                let mut per_symbol_equity: HashMap<String, Vec<f64>> = HashMap::new();
                let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

                for symbol in &loaded_symbols {
                    let df = match symbol_dfs.get(symbol) {
                        Some(df) => df,
                        None => continue,
                    };

                    // Create a single-config grid for this specific config
                    let single_config_grid = create_single_config_grid(
                        strategy_config.strategy_type,
                        &strategy_config_id,
                    );

                    // Run sweep (will just test the single config)
                    let sweep_res = match run_strategy_sweep_polars_parallel(
                        df,
                        &single_config_grid,
                        &polars_config,
                    ) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    configs_tested_this_round += 1;

                    // Get the single result
                    if let Some(result) = sweep_res.config_results.first() {
                        per_symbol_metrics.insert(symbol.clone(), result.metrics.clone());

                        let equity: Vec<f64> = result
                            .backtest_result
                            .equity
                            .iter()
                            .map(|e| e.equity)
                            .collect();
                        let dates: Vec<DateTime<Utc>> =
                            result.backtest_result.equity.iter().map(|e| e.ts).collect();

                        per_symbol_equity.insert(symbol.clone(), equity);
                        per_symbol_dates.insert(symbol.clone(), dates);

                        // Also track per-symbol best
                        let is_per_symbol_best = best_per_symbol_this_round
                            .as_ref()
                            .map(|b| result.metrics.sharpe > b.metrics.sharpe)
                            .unwrap_or(true);

                        if is_per_symbol_best {
                            best_per_symbol_this_round = Some(StrategyBestResult {
                                strategy_type: strategy_config.strategy_type,
                                config_id: strategy_config_id.clone(),
                                symbol: Some(symbol.clone()),
                                metrics: result.metrics.clone(),
                                equity_curve: result
                                    .backtest_result
                                    .equity
                                    .iter()
                                    .map(|e| e.equity)
                                    .collect(),
                                dates: result.backtest_result.equity.iter().map(|e| e.ts).collect(),
                            });
                        }
                    }
                }

                // Skip if no symbols had valid results
                if per_symbol_metrics.is_empty() {
                    continue;
                }

                // 3. Aggregate metrics across all symbols for this config
                // Use tail-risk aware aggregation when equity curves are available
                let aggregate_metrics = AggregatedMetrics::from_per_symbol_with_tail_risk(
                    &per_symbol_metrics,
                    &per_symbol_equity,
                );

                // Create combined equity curve using Phase 3A improvements:
                // - Proper date intersection handling
                // - Equal-weighted (default), with fallback to legacy for backward compat
                let (combined_equity, combined_dates) =
                    combine_equity_curves_simple(&per_symbol_equity, &per_symbol_dates, 100_000.0);

                // Build per-symbol sector mapping for this config (only for symbols that actually produced metrics)
                let mut per_symbol_sectors: HashMap<String, String> = HashMap::new();
                for sym in per_symbol_metrics.keys() {
                    if let Some(sector_id) = symbol_sector_ids.get(sym) {
                        per_symbol_sectors.insert(sym.clone(), sector_id.clone());
                    }
                }

                // 4. Walk-forward validation for promising configs
                // Only run WF for configs with sufficient in-sample performance
                let (wf_grade, mean_oos, std_oos, pct_profitable, degradation, oos_pval) =
                    if aggregate_metrics.avg_sharpe >= WF_SHARPE_THRESHOLD {
                        // Run simplified walk-forward using equity curve returns
                        compute_equity_based_wf(&per_symbol_equity)
                    } else {
                        (None, None, None, None, None, None)
                    };

                // Confidence for YOLO cross-symbol results:
                // Prefer sector-aware robustness when we have enough sector coverage; otherwise fall back
                // to cross-sectional (per-symbol) bootstrap.
                let confidence_grade = Some(
                    trendlab_core::compute_cross_sector_confidence_from_metrics(
                        &per_symbol_metrics,
                        &per_symbol_sectors,
                    )
                    .unwrap_or_else(|| {
                        trendlab_core::compute_cross_symbol_confidence_from_metrics(
                            &per_symbol_metrics,
                        )
                    }),
                );

                let aggregated_result = AggregatedConfigResult {
                    rank: 0, // Will be set by leaderboard
                    strategy_type: strategy_config.strategy_type,
                    config_id: strategy_config_id.clone(),
                    symbols: per_symbol_metrics.keys().cloned().collect(),
                    per_symbol_sectors,
                    per_symbol_metrics,
                    aggregate_metrics: aggregate_metrics.clone(),
                    combined_equity_curve: combined_equity,
                    dates: combined_dates,
                    discovered_at: Utc::now(),
                    iteration,
                    session_id: None, // TODO: Pass session_id from YoloState in Phase 1
                    confidence_grade,
                    // Walk-forward fields (computed from equity-based WF)
                    walk_forward_grade: wf_grade,
                    mean_oos_sharpe: mean_oos,
                    std_oos_sharpe: std_oos,
                    sharpe_degradation: degradation,
                    pct_profitable_folds: pct_profitable,
                    oos_p_value: oos_pval,
                    fdr_adjusted_p_value: None, // Set by FDR correction at end of iteration
                };

                // Track best aggregate this round
                let is_new_best_aggregate = best_aggregate_this_round
                    .as_ref()
                    .map(|b| {
                        aggregate_metrics.avg_sharpe
                            > b.aggregate_metrics
                                .rank_value(CrossSymbolRankMetric::AvgSharpe)
                    })
                    .unwrap_or(true);

                if is_new_best_aggregate {
                    best_aggregate_this_round = Some(aggregated_result.clone());
                }

                // Try to insert into cross-symbol leaderboard (best per strategy)
                yolo_try_insert_best_per_strategy(&mut cross_symbol_leaderboard, aggregated_result);
            }
        }

        // Check if we were cancelled during the sweep
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = per_symbol_leaderboard.save(per_symbol_path);
            let _ = cross_symbol_leaderboard.save(cross_symbol_path);
            let total_configs = cross_symbol_leaderboard.total_configs_tested;
            let _ = update_tx.send(WorkerUpdate::YoloStopped {
                cross_symbol_leaderboard,
                per_symbol_leaderboard,
                total_iterations: iteration,
                total_configs_tested: total_configs,
            });
            return;
        }

        // 4. Update per-symbol leaderboard with best from this round
        if let Some(ref best) = best_per_symbol_this_round {
            // Compute confidence grade from equity curve
            let confidence_grade =
                trendlab_core::compute_confidence_from_equity(&best.equity_curve);

            let entry = LeaderboardEntry {
                rank: 0,
                strategy_type: best.strategy_type,
                config: best.config_id.clone(),
                symbol: best.symbol.clone(),
                sector: None, // TODO: Look up sector from universe in Phase 2
                metrics: best.metrics.clone(),
                equity_curve: best.equity_curve.clone(),
                dates: vec![], // Could extract from backtest result if needed
                discovered_at: Utc::now(),
                iteration,
                session_id: None, // TODO: Pass session_id from YoloState in Phase 1
                confidence_grade,
                // Walk-forward fields (per-symbol, not used in primary YOLO flow)
                walk_forward_grade: None,
                mean_oos_sharpe: None,
                sharpe_degradation: None,
                pct_profitable_folds: None,
                oos_p_value: None,
                fdr_adjusted_p_value: None,
            };
            per_symbol_leaderboard.try_insert(entry);
        }

        per_symbol_leaderboard.add_configs_tested(configs_tested_this_round);
        cross_symbol_leaderboard.add_configs_tested(configs_tested_this_round);
        per_symbol_leaderboard.last_updated = Utc::now();
        cross_symbol_leaderboard.last_updated = Utc::now();

        // 5. Apply FDR correction to cross-symbol leaderboard
        // This adjusts p-values for multiple comparison burden and optionally
        // downgrades confidence grades for non-significant entries
        let n_significant = cross_symbol_leaderboard.apply_fdr_correction(0.05, true);
        trace!(
            n_significant = n_significant,
            total_entries = cross_symbol_leaderboard.entries.len(),
            "Applied FDR correction"
        );

        // 6. Persist leaderboards (every iteration for crash safety)
        let _ = per_symbol_leaderboard.save(per_symbol_path);
        let _ = cross_symbol_leaderboard.save(cross_symbol_path);

        // 7. Send iteration complete update
        let _ = update_tx.send(WorkerUpdate::YoloIterationComplete {
            iteration,
            best_aggregate: best_aggregate_this_round,
            best_per_symbol: best_per_symbol_this_round,
            cross_symbol_leaderboard: cross_symbol_leaderboard.clone(),
            per_symbol_leaderboard: per_symbol_leaderboard.clone(),
            configs_tested_this_round,
        });
    }
}

/// YOLO-specific insertion: keep only the best aggregated config per strategy type.
///
/// This makes the leaderboard answer "which strategies are attractive?" rather than
/// "which configs are attractive?" (which can be dominated by many configs of one strategy).
fn yolo_try_insert_best_per_strategy(
    lb: &mut CrossSymbolLeaderboard,
    entry: AggregatedConfigResult,
) -> bool {
    let rank_by = lb.rank_by;
    let new_value = entry.aggregate_metrics.rank_value(rank_by);

    // If strategy already present, replace only if better.
    if let Some(pos) = lb
        .entries
        .iter()
        .position(|e| e.strategy_type == entry.strategy_type)
    {
        let existing_value = lb.entries[pos].aggregate_metrics.rank_value(rank_by);
        if new_value > existing_value {
            lb.entries[pos] = entry;
            yolo_sort_and_rerank(lb);
            lb.last_updated = Utc::now();
            return true;
        }
        return false;
    }

    // New strategy
    if lb.entries.len() < lb.max_entries {
        lb.entries.push(entry);
        yolo_sort_and_rerank(lb);
        lb.last_updated = Utc::now();
        return true;
    }

    // Full - check if better than worst and replace worst.
    if let Some(worst) = lb.entries.last() {
        let worst_value = worst.aggregate_metrics.rank_value(rank_by);
        if new_value > worst_value {
            lb.entries.pop();
            lb.entries.push(entry);
            yolo_sort_and_rerank(lb);
            lb.last_updated = Utc::now();
            return true;
        }
    }

    false
}

fn yolo_sort_and_rerank(lb: &mut CrossSymbolLeaderboard) {
    let rank_by = lb.rank_by;
    lb.entries.sort_by(|a, b| {
        let va = a.aggregate_metrics.rank_value(rank_by);
        let vb = b.aggregate_metrics.rank_value(rank_by);
        vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, entry) in lb.entries.iter_mut().enumerate() {
        entry.rank = i + 1;
    }
}

fn yolo_compact_to_best_per_strategy(lb: &mut CrossSymbolLeaderboard) {
    use std::collections::HashMap;
    let rank_by = lb.rank_by;
    let mut best: HashMap<trendlab_core::StrategyTypeId, AggregatedConfigResult> = HashMap::new();

    for entry in lb.entries.drain(..) {
        best.entry(entry.strategy_type)
            .and_modify(|cur| {
                let cur_v = cur.aggregate_metrics.rank_value(rank_by);
                let new_v = entry.aggregate_metrics.rank_value(rank_by);
                if new_v > cur_v {
                    *cur = entry.clone();
                }
            })
            .or_insert(entry);
    }

    lb.entries = best.into_values().collect();
    if lb.entries.len() > lb.max_entries {
        // If max_entries is smaller than the number of strategies (shouldn't happen after resize),
        // truncate after sorting.
        yolo_sort_and_rerank(lb);
        lb.entries.truncate(lb.max_entries);
    }
    yolo_sort_and_rerank(lb);
}

/// Generate all config IDs for a strategy's parameter grid.
fn generate_config_ids_for_strategy(strategy_config: &StrategyGridConfig) -> Vec<StrategyConfigId> {
    match &strategy_config.params {
        StrategyParams::Donchian {
            entry_lookbacks,
            exit_lookbacks,
        } => {
            let mut configs = Vec::new();
            for &entry in entry_lookbacks {
                for &exit in exit_lookbacks {
                    configs.push(StrategyConfigId::Donchian {
                        entry_lookback: entry,
                        exit_lookback: exit,
                    });
                }
            }
            configs
        }
        StrategyParams::TurtleS1 => vec![StrategyConfigId::TurtleS1],
        StrategyParams::TurtleS2 => vec![StrategyConfigId::TurtleS2],
        StrategyParams::MACrossover {
            fast_periods,
            slow_periods,
            ma_types,
        } => {
            let mut configs = Vec::new();
            for &fast in fast_periods {
                for &slow in slow_periods {
                    for ma_type in ma_types {
                        if fast < slow {
                            configs.push(StrategyConfigId::MACrossover {
                                fast,
                                slow,
                                ma_type: *ma_type,
                            });
                        }
                    }
                }
            }
            configs
        }
        StrategyParams::Tsmom { lookbacks } => lookbacks
            .iter()
            .map(|&lookback| StrategyConfigId::Tsmom { lookback })
            .collect(),
        StrategyParams::DmiAdx {
            di_periods,
            adx_periods,
            adx_thresholds,
        } => {
            let mut configs = Vec::new();
            for &di in di_periods {
                for &adx in adx_periods {
                    for &thresh in adx_thresholds {
                        configs.push(StrategyConfigId::DmiAdx {
                            di_period: di,
                            adx_period: adx,
                            adx_threshold: thresh,
                        });
                    }
                }
            }
            configs
        }
        StrategyParams::Aroon { periods } => periods
            .iter()
            .map(|&period| StrategyConfigId::Aroon { period })
            .collect(),
        StrategyParams::BollingerSqueeze {
            periods,
            std_mults,
            squeeze_thresholds,
        } => {
            let mut configs = Vec::new();
            for &period in periods {
                for &std_mult in std_mults {
                    for &squeeze in squeeze_thresholds {
                        configs.push(StrategyConfigId::BollingerSqueeze {
                            period,
                            std_mult,
                            squeeze_threshold: squeeze,
                        });
                    }
                }
            }
            configs
        }
        StrategyParams::Keltner {
            ema_periods,
            atr_periods,
            multipliers,
        } => {
            let mut configs = Vec::new();
            for &ema in ema_periods {
                for &atr in atr_periods {
                    for &mult in multipliers {
                        configs.push(StrategyConfigId::Keltner {
                            ema_period: ema,
                            atr_period: atr,
                            multiplier: mult,
                        });
                    }
                }
            }
            configs
        }
        StrategyParams::STARC {
            sma_periods,
            atr_periods,
            multipliers,
        } => {
            let mut configs = Vec::new();
            for &sma in sma_periods {
                for &atr in atr_periods {
                    for &mult in multipliers {
                        configs.push(StrategyConfigId::STARC {
                            sma_period: sma,
                            atr_period: atr,
                            multiplier: mult,
                        });
                    }
                }
            }
            configs
        }
        StrategyParams::Supertrend {
            atr_periods,
            multipliers,
        } => {
            let mut configs = Vec::new();
            for &atr in atr_periods {
                for &mult in multipliers {
                    configs.push(StrategyConfigId::Supertrend {
                        atr_period: atr,
                        multiplier: mult,
                    });
                }
            }
            configs
        }
        StrategyParams::FiftyTwoWeekHigh {
            periods,
            entry_pcts,
            exit_pcts,
        } => {
            let mut configs = Vec::new();
            for &period in periods {
                for &entry_pct in entry_pcts {
                    for &exit_pct in exit_pcts {
                        configs.push(StrategyConfigId::FiftyTwoWeekHigh {
                            period,
                            entry_pct,
                            exit_pct,
                        });
                    }
                }
            }
            configs
        }
        StrategyParams::DarvasBox {
            box_confirmation_bars,
        } => box_confirmation_bars
            .iter()
            .map(|&bars| StrategyConfigId::DarvasBox {
                box_confirmation_bars: bars,
            })
            .collect(),
        StrategyParams::LarryWilliams {
            range_multipliers,
            atr_stop_mults,
        } => {
            let mut configs = Vec::new();
            for &range in range_multipliers {
                for &atr in atr_stop_mults {
                    configs.push(StrategyConfigId::LarryWilliams {
                        range_multiplier: range,
                        atr_stop_mult: atr,
                    });
                }
            }
            configs
        }
        StrategyParams::HeikinAshi { confirmation_bars } => confirmation_bars
            .iter()
            .map(|&bars| StrategyConfigId::HeikinAshi {
                confirmation_bars: bars,
            })
            .collect(),
        StrategyParams::ParabolicSar {
            af_starts,
            af_steps,
            af_maxs,
        } => {
            let mut configs = Vec::new();
            for &start in af_starts {
                for &step in af_steps {
                    for &max in af_maxs {
                        configs.push(StrategyConfigId::ParabolicSar {
                            af_start: start,
                            af_step: step,
                            af_max: max,
                        });
                    }
                }
            }
            configs
        }
        StrategyParams::OpeningRangeBreakout {
            range_bars,
            periods,
        } => {
            let mut configs = Vec::new();
            for &bars in range_bars {
                for period in periods {
                    configs.push(StrategyConfigId::OpeningRangeBreakout {
                        range_bars: bars,
                        period: *period,
                    });
                }
            }
            configs
        }
        StrategyParams::Ensemble {
            base_strategies,
            horizon_sets,
            voting_methods,
        } => {
            let mut configs = Vec::new();
            for &base in base_strategies {
                for horizons in horizon_sets {
                    for &voting in voting_methods {
                        configs.push(StrategyConfigId::Ensemble {
                            base_strategy: base,
                            horizons: horizons.clone(),
                            voting,
                        });
                    }
                }
            }
            configs
        }
        // Phase 5 oscillator strategies not yet supported in YOLO mode
        _ => Vec::new(),
    }
}

/// Create a single-config StrategyGridConfig for a specific config ID.
fn create_single_config_grid(
    strategy_type: StrategyTypeId,
    config_id: &StrategyConfigId,
) -> StrategyGridConfig {
    let params = match config_id {
        StrategyConfigId::Donchian {
            entry_lookback,
            exit_lookback,
        } => StrategyParams::Donchian {
            entry_lookbacks: vec![*entry_lookback],
            exit_lookbacks: vec![*exit_lookback],
        },
        StrategyConfigId::TurtleS1 => StrategyParams::TurtleS1,
        StrategyConfigId::TurtleS2 => StrategyParams::TurtleS2,
        StrategyConfigId::MACrossover {
            fast,
            slow,
            ma_type,
        } => StrategyParams::MACrossover {
            fast_periods: vec![*fast],
            slow_periods: vec![*slow],
            ma_types: vec![*ma_type],
        },
        StrategyConfigId::Tsmom { lookback } => StrategyParams::Tsmom {
            lookbacks: vec![*lookback],
        },
        StrategyConfigId::DmiAdx {
            di_period,
            adx_period,
            adx_threshold,
        } => StrategyParams::DmiAdx {
            di_periods: vec![*di_period],
            adx_periods: vec![*adx_period],
            adx_thresholds: vec![*adx_threshold],
        },
        StrategyConfigId::Aroon { period } => StrategyParams::Aroon {
            periods: vec![*period],
        },
        StrategyConfigId::BollingerSqueeze {
            period,
            std_mult,
            squeeze_threshold,
        } => StrategyParams::BollingerSqueeze {
            periods: vec![*period],
            std_mults: vec![*std_mult],
            squeeze_thresholds: vec![*squeeze_threshold],
        },
        StrategyConfigId::Keltner {
            ema_period,
            atr_period,
            multiplier,
        } => StrategyParams::Keltner {
            ema_periods: vec![*ema_period],
            atr_periods: vec![*atr_period],
            multipliers: vec![*multiplier],
        },
        StrategyConfigId::STARC {
            sma_period,
            atr_period,
            multiplier,
        } => StrategyParams::STARC {
            sma_periods: vec![*sma_period],
            atr_periods: vec![*atr_period],
            multipliers: vec![*multiplier],
        },
        StrategyConfigId::Supertrend {
            atr_period,
            multiplier,
        } => StrategyParams::Supertrend {
            atr_periods: vec![*atr_period],
            multipliers: vec![*multiplier],
        },
        StrategyConfigId::FiftyTwoWeekHigh {
            period,
            entry_pct,
            exit_pct,
        } => StrategyParams::FiftyTwoWeekHigh {
            periods: vec![*period],
            entry_pcts: vec![*entry_pct],
            exit_pcts: vec![*exit_pct],
        },
        StrategyConfigId::DarvasBox {
            box_confirmation_bars,
        } => StrategyParams::DarvasBox {
            box_confirmation_bars: vec![*box_confirmation_bars],
        },
        StrategyConfigId::LarryWilliams {
            range_multiplier,
            atr_stop_mult,
        } => StrategyParams::LarryWilliams {
            range_multipliers: vec![*range_multiplier],
            atr_stop_mults: vec![*atr_stop_mult],
        },
        StrategyConfigId::HeikinAshi { confirmation_bars } => StrategyParams::HeikinAshi {
            confirmation_bars: vec![*confirmation_bars],
        },
        StrategyConfigId::ParabolicSar {
            af_start,
            af_step,
            af_max,
        } => StrategyParams::ParabolicSar {
            af_starts: vec![*af_start],
            af_steps: vec![*af_step],
            af_maxs: vec![*af_max],
        },
        StrategyConfigId::OpeningRangeBreakout { range_bars, period } => {
            StrategyParams::OpeningRangeBreakout {
                range_bars: vec![*range_bars],
                periods: vec![*period],
            }
        }
        StrategyConfigId::Ensemble {
            base_strategy,
            horizons,
            voting,
        } => StrategyParams::Ensemble {
            base_strategies: vec![*base_strategy],
            horizon_sets: vec![horizons.clone()],
            voting_methods: vec![*voting],
        },
        // Phase 5 oscillator strategies not yet supported in YOLO mode
        _ => panic!("Strategy config ID not yet supported in YOLO mode"),
    };

    StrategyGridConfig {
        strategy_type,
        enabled: true,
        params,
    }
}

/// Extract a StrategyConfigId from a basic ConfigId based on strategy type.
fn extract_strategy_config_id(
    strategy_type: StrategyTypeId,
    config_id: &trendlab_core::ConfigId,
) -> StrategyConfigId {
    use trendlab_core::MAType;

    match strategy_type {
        StrategyTypeId::Donchian => StrategyConfigId::Donchian {
            entry_lookback: config_id.entry_lookback,
            exit_lookback: config_id.exit_lookback,
        },
        StrategyTypeId::TurtleS1 => StrategyConfigId::TurtleS1,
        StrategyTypeId::TurtleS2 => StrategyConfigId::TurtleS2,
        // For other strategies, we use the entry_lookback as the primary param
        // This is a simplification - in practice each strategy has its own config
        StrategyTypeId::MACrossover => StrategyConfigId::MACrossover {
            fast: config_id.exit_lookback, // exit_lookback used as fast
            slow: config_id.entry_lookback,
            ma_type: MAType::SMA,
        },
        StrategyTypeId::Tsmom => StrategyConfigId::Tsmom {
            lookback: config_id.entry_lookback,
        },
        StrategyTypeId::DmiAdx => StrategyConfigId::DmiAdx {
            di_period: config_id.entry_lookback,
            adx_period: config_id.exit_lookback,
            adx_threshold: 25.0,
        },
        StrategyTypeId::Aroon => StrategyConfigId::Aroon {
            period: config_id.entry_lookback,
        },
        StrategyTypeId::BollingerSqueeze => StrategyConfigId::BollingerSqueeze {
            period: config_id.entry_lookback,
            std_mult: 2.0,
            squeeze_threshold: 0.06,
        },
        StrategyTypeId::Keltner => StrategyConfigId::Keltner {
            ema_period: config_id.entry_lookback,
            atr_period: config_id.exit_lookback,
            multiplier: 2.0,
        },
        StrategyTypeId::STARC => StrategyConfigId::STARC {
            sma_period: config_id.entry_lookback,
            atr_period: config_id.exit_lookback,
            multiplier: 2.0,
        },
        StrategyTypeId::Supertrend => StrategyConfigId::Supertrend {
            atr_period: config_id.entry_lookback,
            multiplier: 3.0,
        },
        StrategyTypeId::FiftyTwoWeekHigh => StrategyConfigId::FiftyTwoWeekHigh {
            period: config_id.entry_lookback,
            entry_pct: 0.95,
            exit_pct: 0.80,
        },
        StrategyTypeId::DarvasBox => StrategyConfigId::DarvasBox {
            box_confirmation_bars: config_id.entry_lookback,
        },
        StrategyTypeId::LarryWilliams => StrategyConfigId::LarryWilliams {
            range_multiplier: 2.0,
            atr_stop_mult: 1.5,
        },
        StrategyTypeId::HeikinAshi => StrategyConfigId::HeikinAshi {
            confirmation_bars: config_id.entry_lookback,
        },
        StrategyTypeId::ParabolicSar => StrategyConfigId::ParabolicSar {
            af_start: 0.02,
            af_step: 0.02,
            af_max: 0.2,
        },
        StrategyTypeId::OpeningRangeBreakout => StrategyConfigId::OpeningRangeBreakout {
            range_bars: config_id.entry_lookback,
            period: OpeningPeriod::Rolling,
        },
        StrategyTypeId::Ensemble => StrategyConfigId::Ensemble {
            base_strategy: StrategyTypeId::Donchian,
            horizons: vec![20, 50],
            voting: VotingMethod::Majority,
        },
        // Phase 5 oscillator strategies not yet supported in simple config conversion
        _ => panic!("Strategy type not yet supported in config_id_from_simple_config"),
    }
}

// =============================================================================
// Grid Jittering Functions
// =============================================================================

use rand::Rng;

/// Jitter all parameters in a MultiStrategyGrid by a percentage.
fn jitter_multi_strategy_grid(
    base: &MultiStrategyGrid,
    pct: f64,
    rng: &mut impl Rng,
) -> MultiStrategyGrid {
    MultiStrategyGrid {
        strategies: base
            .strategies
            .iter()
            .map(|config| StrategyGridConfig {
                strategy_type: config.strategy_type,
                enabled: config.enabled,
                params: jitter_strategy_params(&config.params, pct, rng),
            })
            .collect(),
    }
}

/// Jitter the parameters of a single strategy.
fn jitter_strategy_params(params: &StrategyParams, pct: f64, rng: &mut impl Rng) -> StrategyParams {
    match params {
        StrategyParams::Donchian {
            entry_lookbacks,
            exit_lookbacks,
        } => StrategyParams::Donchian {
            entry_lookbacks: jitter_usize_vec(entry_lookbacks, pct, 5, 5, 200, rng),
            exit_lookbacks: jitter_usize_vec(exit_lookbacks, pct, 5, 2, 100, rng),
        },

        StrategyParams::TurtleS1 => StrategyParams::TurtleS1,
        StrategyParams::TurtleS2 => StrategyParams::TurtleS2,

        StrategyParams::MACrossover {
            fast_periods,
            slow_periods,
            ma_types,
        } => StrategyParams::MACrossover {
            fast_periods: jitter_usize_vec(fast_periods, pct, 5, 5, 100, rng),
            slow_periods: jitter_usize_vec(slow_periods, pct, 10, 20, 500, rng),
            ma_types: ma_types.clone(), // Don't jitter enum types
        },

        StrategyParams::Tsmom { lookbacks } => StrategyParams::Tsmom {
            lookbacks: jitter_usize_vec(lookbacks, pct, 5, 5, 500, rng),
        },

        StrategyParams::DmiAdx {
            di_periods,
            adx_periods,
            adx_thresholds,
        } => StrategyParams::DmiAdx {
            di_periods: jitter_usize_vec(di_periods, pct, 1, 5, 50, rng),
            adx_periods: jitter_usize_vec(adx_periods, pct, 1, 5, 50, rng),
            adx_thresholds: jitter_f64_vec(adx_thresholds, pct, 1.0, 10.0, 50.0, rng),
        },

        StrategyParams::Aroon { periods } => StrategyParams::Aroon {
            periods: jitter_usize_vec(periods, pct, 1, 5, 100, rng),
        },

        StrategyParams::BollingerSqueeze {
            periods,
            std_mults,
            squeeze_thresholds,
        } => StrategyParams::BollingerSqueeze {
            periods: jitter_usize_vec(periods, pct, 1, 5, 100, rng),
            std_mults: jitter_f64_vec(std_mults, pct, 0.1, 1.0, 4.0, rng),
            squeeze_thresholds: jitter_f64_vec(squeeze_thresholds, pct, 0.01, 0.01, 0.5, rng),
        },

        StrategyParams::Keltner {
            ema_periods,
            atr_periods,
            multipliers,
        } => StrategyParams::Keltner {
            ema_periods: jitter_usize_vec(ema_periods, pct, 1, 5, 100, rng),
            atr_periods: jitter_usize_vec(atr_periods, pct, 1, 5, 50, rng),
            multipliers: jitter_f64_vec(multipliers, pct, 0.1, 0.5, 5.0, rng),
        },

        StrategyParams::STARC {
            sma_periods,
            atr_periods,
            multipliers,
        } => StrategyParams::STARC {
            sma_periods: jitter_usize_vec(sma_periods, pct, 1, 5, 100, rng),
            atr_periods: jitter_usize_vec(atr_periods, pct, 1, 5, 50, rng),
            multipliers: jitter_f64_vec(multipliers, pct, 0.1, 0.5, 5.0, rng),
        },

        StrategyParams::Supertrend {
            atr_periods,
            multipliers,
        } => StrategyParams::Supertrend {
            atr_periods: jitter_usize_vec(atr_periods, pct, 1, 5, 50, rng),
            multipliers: jitter_f64_vec(multipliers, pct, 0.1, 1.0, 5.0, rng),
        },

        StrategyParams::FiftyTwoWeekHigh {
            periods,
            entry_pcts,
            exit_pcts,
        } => StrategyParams::FiftyTwoWeekHigh {
            periods: jitter_usize_vec(periods, pct, 5, 50, 500, rng),
            entry_pcts: jitter_f64_vec(entry_pcts, pct, 0.01, 0.80, 1.0, rng),
            exit_pcts: jitter_f64_vec(exit_pcts, pct, 0.01, 0.50, 0.95, rng),
        },

        StrategyParams::DarvasBox {
            box_confirmation_bars,
        } => StrategyParams::DarvasBox {
            box_confirmation_bars: jitter_usize_vec(box_confirmation_bars, pct, 1, 2, 20, rng),
        },

        StrategyParams::LarryWilliams {
            range_multipliers,
            atr_stop_mults,
        } => StrategyParams::LarryWilliams {
            range_multipliers: jitter_f64_vec(range_multipliers, pct, 0.1, 0.5, 5.0, rng),
            atr_stop_mults: jitter_f64_vec(atr_stop_mults, pct, 0.1, 0.5, 5.0, rng),
        },

        StrategyParams::HeikinAshi { confirmation_bars } => StrategyParams::HeikinAshi {
            confirmation_bars: jitter_usize_vec(confirmation_bars, pct, 1, 1, 10, rng),
        },

        StrategyParams::ParabolicSar {
            af_starts,
            af_steps,
            af_maxs,
        } => StrategyParams::ParabolicSar {
            af_starts: jitter_f64_vec(af_starts, pct, 0.005, 0.01, 0.1, rng),
            af_steps: jitter_f64_vec(af_steps, pct, 0.005, 0.01, 0.1, rng),
            af_maxs: jitter_f64_vec(af_maxs, pct, 0.01, 0.1, 0.5, rng),
        },

        StrategyParams::OpeningRangeBreakout {
            range_bars,
            periods,
        } => {
            StrategyParams::OpeningRangeBreakout {
                range_bars: jitter_usize_vec(range_bars, pct, 1, 1, 20, rng),
                periods: periods.clone(), // Don't jitter enum types
            }
        }

        StrategyParams::Ensemble {
            base_strategies,
            horizon_sets,
            voting_methods,
        } => StrategyParams::Ensemble {
            base_strategies: base_strategies.clone(),
            horizon_sets: horizon_sets
                .iter()
                .map(|hs| jitter_usize_vec(hs, pct, 5, 5, 200, rng))
                .collect(),
            voting_methods: voting_methods.clone(),
        },
        // Phase 5 oscillator strategies not yet supported in YOLO mode jittering
        _ => params.clone(),
    }
}

/// Jitter a vector of usize values.
fn jitter_usize_vec(
    values: &[usize],
    pct: f64,
    step: usize,
    min: usize,
    max: usize,
    rng: &mut impl Rng,
) -> Vec<usize> {
    values
        .iter()
        .map(|&v| jitter_usize(v, pct, step, min, max, rng))
        .collect()
}

/// Jitter a single usize value.
fn jitter_usize(
    value: usize,
    pct: f64,
    step: usize,
    min: usize,
    max: usize,
    rng: &mut impl Rng,
) -> usize {
    let delta = rng.gen_range(-pct..=pct);
    let candidate = (value as f64) * (1.0 + delta);
    let candidate = if step > 1 {
        ((candidate / step as f64).round() * step as f64) as usize
    } else {
        candidate.round() as usize
    };
    candidate.clamp(min, max)
}

/// Jitter a vector of f64 values.
fn jitter_f64_vec(
    values: &[f64],
    pct: f64,
    step: f64,
    min: f64,
    max: f64,
    rng: &mut impl Rng,
) -> Vec<f64> {
    values
        .iter()
        .map(|&v| jitter_f64(v, pct, step, min, max, rng))
        .collect()
}

/// Jitter a single f64 value.
fn jitter_f64(value: f64, pct: f64, step: f64, min: f64, max: f64, rng: &mut impl Rng) -> f64 {
    let delta = rng.gen_range(-pct..=pct);
    let candidate = value * (1.0 + delta);
    let candidate = (candidate / step).round() * step;
    candidate.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_search_response_basic() {
        let json = r#"{"quotes":[{"symbol":"GOOG","shortname":"Alphabet Inc.","exchange":"NMS","quoteType":"EQUITY"}]}"#;
        let results = parse_search_response(json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol, "GOOG");
        assert_eq!(results[0].name, "Alphabet Inc.");
        assert_eq!(results[0].exchange, "NMS");
        assert_eq!(results[0].type_disp, "EQUITY");
    }

    #[test]
    fn test_parse_search_response_multiple() {
        let json = r#"{"explains":[],"count":2,"quotes":[{"symbol":"GOOG","shortname":"Alphabet","exchange":"NMS","quoteType":"EQUITY"},{"symbol":"GOOGL","shortname":"Alphabet Class A","exchange":"NMS","quoteType":"EQUITY"}],"news":[]}"#;
        let results = parse_search_response(json).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].symbol, "GOOG");
        assert_eq!(results[1].symbol, "GOOGL");
    }

    #[test]
    fn test_parse_search_response_empty_quotes() {
        let json = r#"{"quotes":[]}"#;
        let results = parse_search_response(json).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_extract_json_string() {
        let json = r#"{"symbol":"SPY","name":"Test"}"#;
        assert_eq!(extract_json_string(json, "symbol"), Some("SPY".to_string()));
        assert_eq!(extract_json_string(json, "name"), Some("Test".to_string()));
        assert_eq!(extract_json_string(json, "missing"), None);
    }
}
