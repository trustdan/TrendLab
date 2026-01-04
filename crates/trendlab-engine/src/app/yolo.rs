//! YOLO Mode state - continuous auto-optimization.

use chrono::{DateTime, NaiveDate, Utc};
use trendlab_core::{
    generate_session_id, CrossSymbolLeaderboard, Leaderboard, LeaderboardScope, RiskProfile,
    SweepDepth,
};

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
    /// Number of warmup iterations before winner exploitation begins
    pub warmup_iterations: u32,
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
            Self::WarmupIterations => Self::StartDate,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::StartDate => Self::WarmupIterations,
            Self::EndDate => Self::StartDate,
            Self::Randomization => Self::EndDate,
            Self::WfSharpeThreshold => Self::Randomization,
            Self::SweepDepth => Self::WfSharpeThreshold,
            Self::PolarsThreads => Self::SweepDepth,
            Self::OuterThreads => Self::PolarsThreads,
            Self::WarmupIterations => Self::OuterThreads,
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
            // Update requested dates to match the current session (most recent test)
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
