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

use chrono::NaiveDate;
use std::collections::HashMap;
use trendlab_core::{
    bars_to_dataframe, run_donchian_sweep_polars, run_strategy_sweep_polars_parallel,
    scan_symbol_parquet_lazy, AggregatedPortfolioResult, BacktestConfig, Bar, CostModel,
    DataQualityChecker, DataQualityReport, DonchianBacktestConfig, IntoLazy, MultiStrategyGrid,
    MultiStrategySweepResult, MultiSweepResult, PolarsBacktestConfig, RankMetric, StrategyTypeId,
    SweepConfigResult, SweepGrid, SweepResult,
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
