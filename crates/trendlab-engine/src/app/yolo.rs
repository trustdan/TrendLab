//! YOLO Mode state - continuous auto-optimization.

use chrono::{DateTime, NaiveDate, Utc};
use tracing::debug;
use trendlab_core::{
    generate_session_id, AggregatedConfigResult, CrossSymbolLeaderboard, Leaderboard,
    LeaderboardScope, RiskProfile, SweepDepth,
};

/// Combo strategy mode for YOLO iterations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboMode {
    /// No combo strategies - only single strategies
    None,
    /// 2-way combos only (every other iteration)
    #[default]
    TwoWay,
    /// 2-way and 3-way combos (every other + every 6th)
    TwoAndThreeWay,
    /// All combos: 2-way, 3-way, and 4-way
    All,
}

impl ComboMode {
    /// Cycle to the next combo mode
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::TwoWay,
            Self::TwoWay => Self::TwoAndThreeWay,
            Self::TwoAndThreeWay => Self::All,
            Self::All => Self::None,
        }
    }

    /// Cycle to the previous combo mode
    pub fn prev(self) -> Self {
        match self {
            Self::None => Self::All,
            Self::TwoWay => Self::None,
            Self::TwoAndThreeWay => Self::TwoWay,
            Self::All => Self::TwoAndThreeWay,
        }
    }

    /// Display name for the combo mode
    pub fn display_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::TwoWay => "2-Way",
            Self::TwoAndThreeWay => "2+3-Way",
            Self::All => "2+3+4-Way",
        }
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
    /// Walk-forward Sharpe threshold (min avg Sharpe to trigger WF validation)
    pub wf_sharpe_threshold: f64,
    /// Optional cap for Polars threads (per backtest)
    pub polars_max_threads: Option<usize>,
    /// Optional cap for outer Rayon pool (symbol-level parallelism)
    pub outer_threads: Option<usize>,
    /// Number of warmup iterations before winner exploitation begins
    pub warmup_iterations: u32,
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
    /// Walk-forward Sharpe threshold (0.15 to 0.50)
    pub wf_sharpe_threshold: f64,
    /// Sweep depth for parameter coverage
    pub sweep_depth: SweepDepth,
    /// Optional cap for Polars threads (per backtest)
    pub polars_max_threads: Option<usize>,
    /// Optional cap for outer Rayon pool (symbol-level parallelism)
    pub outer_threads: Option<usize>,
    /// Number of warmup iterations before winner exploitation begins (single strategies)
    pub warmup_iterations: u32,
    /// Number of warmup iterations before combo strategies are tested (default 10)
    /// Combos start after this many iterations to let singles establish a baseline
    pub combo_warmup_iterations: u32,
    /// Combo strategy mode (None, 2-Way, 2+3-Way, All)
    pub combo_mode: ComboMode,
}

/// Fields in the YOLO config modal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoloConfigField {
    StartDate,
    EndDate,
    Randomization,
    WfSharpeThreshold,
    SweepDepth,
    PolarsThreads,
    OuterThreads,
    WarmupIterations,
    ComboMode,
    ComboWarmup,
}

impl Default for YoloConfigState {
    fn default() -> Self {
        let today = chrono::Local::now().date_naive();
        Self {
            focused_field: YoloConfigField::StartDate,
            start_date: today - chrono::Duration::days(5 * 365),
            end_date: today,
            randomization_pct: 0.30,
            wf_sharpe_threshold: 0.25,
            sweep_depth: SweepDepth::Quick,
            polars_max_threads: None,
            outer_threads: None,
            warmup_iterations: 50,
            combo_warmup_iterations: 10,
            combo_mode: ComboMode::TwoWay,
        }
    }
}

impl YoloConfigField {
    pub fn next(self) -> Self {
        match self {
            Self::StartDate => Self::EndDate,
            Self::EndDate => Self::Randomization,
            Self::Randomization => Self::WfSharpeThreshold,
            Self::WfSharpeThreshold => Self::SweepDepth,
            Self::SweepDepth => Self::PolarsThreads,
            Self::PolarsThreads => Self::OuterThreads,
            Self::OuterThreads => Self::WarmupIterations,
            Self::WarmupIterations => Self::ComboMode,
            Self::ComboMode => Self::ComboWarmup,
            Self::ComboWarmup => Self::StartDate,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::StartDate => Self::ComboWarmup,
            Self::EndDate => Self::StartDate,
            Self::Randomization => Self::EndDate,
            Self::WfSharpeThreshold => Self::Randomization,
            Self::SweepDepth => Self::WfSharpeThreshold,
            Self::PolarsThreads => Self::SweepDepth,
            Self::OuterThreads => Self::PolarsThreads,
            Self::WarmupIterations => Self::OuterThreads,
            Self::ComboMode => Self::WarmupIterations,
            Self::ComboWarmup => Self::ComboMode,
        }
    }
}

impl Default for YoloState {
    fn default() -> Self {
        Self {
            enabled: false,
            iteration: 0,
            // Session leaderboards (fresh each app launch)
            session_leaderboard: Leaderboard::new(500), // Reasonable limit for session display
            session_cross_symbol_leaderboard: None,
            // All-time leaderboards (will be loaded from disk in App::new)
            all_time_leaderboard: Leaderboard::new(500), // Larger capacity for historical data
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
            // Default walk-forward threshold (lowered from 0.30 to capture more configs)
            wf_sharpe_threshold: 0.25,
            polars_max_threads: None,
            outer_threads: None,
            warmup_iterations: 50,
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
        // DEBUG: Log incoming data
        debug!(
            incoming_per_symbol = per_symbol.entries.len(),
            incoming_cross_symbol = cross_symbol.entries.len(),
            my_session_id = %self.session_id,
            configs_tested = configs_tested_this_round,
            "YOLO DEBUG: update_leaderboards called"
        );

        // ACCUMULATE session entries: add new entries from this session (by config hash)
        // This preserves entries even after they're displaced from all-time top-N
        let existing_per_symbol_hashes: std::collections::HashSet<u64> = self
            .session_leaderboard
            .entries
            .iter()
            .map(|e| e.config_hash())
            .collect();

        let mut per_symbol_added = 0;
        let mut per_symbol_session_match = 0;
        for entry in per_symbol.entries.iter() {
            if entry.session_id.as_ref() == Some(&self.session_id) {
                per_symbol_session_match += 1;
                let hash = entry.config_hash();
                if !existing_per_symbol_hashes.contains(&hash) {
                    self.session_leaderboard.entries.push(entry.clone());
                    per_symbol_added += 1;
                }
            }
        }
        debug!(
            session_match = per_symbol_session_match,
            new_added = per_symbol_added,
            total_session_entries = self.session_leaderboard.entries.len(),
            "YOLO DEBUG: per-symbol session filtering"
        );
        self.session_leaderboard.total_iterations = per_symbol.total_iterations;
        self.session_leaderboard.last_updated = per_symbol.last_updated;
        // Re-rank session entries by Sharpe
        self.session_leaderboard.sort_and_rerank();

        // ACCUMULATE cross-symbol session entries similarly
        // Initialize with EMPTY leaderboard (same settings, no entries) - NOT a clone of all-time!
        // Use reasonable limits to avoid O(n log n) sort overhead on huge entry lists
        let session_cross = self
            .session_cross_symbol_leaderboard
            .get_or_insert_with(|| {
                CrossSymbolLeaderboard::with_max_per_strategy(
                    1000, // Reasonable limit for session display
                    cross_symbol.rank_by,
                    100, // Per-strategy limit for session
                )
            });

        let existing_cross_hashes: std::collections::HashSet<u64> = session_cross
            .entries
            .iter()
            .map(|e| e.config_hash())
            .collect();

        let mut cross_symbol_added = 0;
        let mut cross_symbol_session_match = 0;
        for entry in cross_symbol.entries.iter() {
            // DEBUG: Log each entry's session_id for comparison
            if cross_symbol_added == 0 && cross_symbol_session_match == 0 {
                // Only log first entry to avoid spam
                debug!(
                    entry_session_id = ?entry.session_id,
                    expected_session_id = %self.session_id,
                    "YOLO DEBUG: First cross-symbol entry session_id comparison"
                );
            }
            if entry.session_id.as_ref() == Some(&self.session_id) {
                cross_symbol_session_match += 1;
                let hash = entry.config_hash();
                if !existing_cross_hashes.contains(&hash) {
                    session_cross.entries.push(entry.clone());
                    cross_symbol_added += 1;
                }
            }
        }
        debug!(
            session_match = cross_symbol_session_match,
            new_added = cross_symbol_added,
            total_session_entries = session_cross.entries.len(),
            "YOLO DEBUG: cross-symbol session filtering"
        );
        // Update metadata
        session_cross.last_updated = cross_symbol.last_updated;
        if let (Some(start), Some(end)) = (cross_symbol.requested_start, cross_symbol.requested_end)
        {
            session_cross.set_requested_range(start, end);
        }
        // Re-rank session cross-symbol entries
        session_cross.sort_and_rerank();

        self.session_configs_tested += configs_tested_this_round as u64;

        // Merge into all-time leaderboards (unchanged logic)
        for entry in per_symbol.entries.iter() {
            self.all_time_leaderboard.try_insert(entry.clone());
        }
        if let Some(ref mut all_time_cross) = self.all_time_cross_symbol_leaderboard {
            for entry in cross_symbol.entries.iter() {
                all_time_cross.try_insert(entry.clone());
            }
            if let (Some(start), Some(end)) =
                (cross_symbol.requested_start, cross_symbol.requested_end)
            {
                all_time_cross.set_requested_range(start, end);
            }
        } else {
            self.all_time_cross_symbol_leaderboard = Some(cross_symbol);
        }
        self.total_configs_tested += configs_tested_this_round as u64;
    }

    /// Update leaderboards with direct session results from worker.
    ///
    /// This method receives session_results directly from the worker, bypassing
    /// the session_id filtering that was causing entries to be lost when they
    /// were displaced from the all-time top-N leaderboard.
    pub fn update_leaderboards_with_session(
        &mut self,
        per_symbol: Leaderboard,
        cross_symbol: CrossSymbolLeaderboard,
        configs_tested_this_round: usize,
        session_results: Vec<AggregatedConfigResult>,
    ) {
        debug!(
            incoming_per_symbol = per_symbol.entries.len(),
            incoming_cross_symbol = cross_symbol.entries.len(),
            session_results_count = session_results.len(),
            my_session_id = %self.session_id,
            configs_tested = configs_tested_this_round,
            "YOLO DEBUG: update_leaderboards_with_session called"
        );

        // Initialize session cross-symbol leaderboard if needed
        let session_cross = self
            .session_cross_symbol_leaderboard
            .get_or_insert_with(|| {
                CrossSymbolLeaderboard::with_max_per_strategy(
                    1000, // Reasonable limit for session display
                    cross_symbol.rank_by,
                    100, // Per-strategy limit for session
                )
            });

        // Directly add session_results to session leaderboard (no session_id filtering needed)
        let existing_hashes: std::collections::HashSet<u64> = session_cross
            .entries
            .iter()
            .map(|e| e.config_hash())
            .collect();

        let mut added = 0;
        for entry in session_results {
            let hash = entry.config_hash();
            if !existing_hashes.contains(&hash) {
                session_cross.entries.push(entry);
                added += 1;
            }
        }

        debug!(
            new_added = added,
            total_session_entries = session_cross.entries.len(),
            "YOLO DEBUG: Direct session results added"
        );

        // Update metadata
        session_cross.last_updated = cross_symbol.last_updated;
        if let (Some(start), Some(end)) = (cross_symbol.requested_start, cross_symbol.requested_end)
        {
            session_cross.set_requested_range(start, end);
        }
        // Re-rank session cross-symbol entries
        session_cross.sort_and_rerank();

        self.session_configs_tested += configs_tested_this_round as u64;

        // Merge into all-time leaderboards (unchanged logic)
        for entry in per_symbol.entries.iter() {
            self.all_time_leaderboard.try_insert(entry.clone());
        }
        if let Some(ref mut all_time_cross) = self.all_time_cross_symbol_leaderboard {
            for entry in cross_symbol.entries.iter() {
                all_time_cross.try_insert(entry.clone());
            }
            if let (Some(start), Some(end)) =
                (cross_symbol.requested_start, cross_symbol.requested_end)
            {
                all_time_cross.set_requested_range(start, end);
            }
        } else {
            self.all_time_cross_symbol_leaderboard = Some(cross_symbol);
        }
        self.total_configs_tested += configs_tested_this_round as u64;
    }
}
