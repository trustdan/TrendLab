//! TrendLab Core - Domain types, strategies, and metrics for trend-following research.
//!
//! This crate provides:
//! - Bar and OHLCV data types
//! - Strategy trait and implementations
//! - Performance metrics calculations
//! - Data provider traits
//! - Parameter sweep infrastructure
//! - Strategy artifact export for Pine Script parity
//! - Post-backtest statistical analysis

pub mod analysis;
pub mod analysis_polars;
pub mod artifact;
pub mod backtest;
pub mod backtest_polars;
pub mod bar;
pub mod clustering;
pub mod data;
pub mod error;
pub mod exploration;
pub mod indicator_cache;
pub mod indicators;
pub mod indicators_polars;
pub mod latin_hypercube;
pub mod leaderboard;
pub mod metrics;
pub mod sector_analysis;
pub mod sizing;
pub mod statistics;
pub mod strategy;
pub mod strategy_v2;
pub mod sweep;
pub mod sweep_polars;
pub mod universe;
pub mod validation;

pub use artifact::{
    create_52wk_high_artifact, create_artifact_from_config, create_donchian_artifact,
    export_artifact_to_file, ArtifactBuilder, ArtifactCostModel, ArtifactError, ArtifactMetadata,
    DataRange, IndicatorDef, IndicatorValue, OhlcvData, ParamValue, ParityVector, ParityVectors,
    Rule, Rules, StrategyArtifact, SCHEMA_VERSION,
};
pub use backtest::{
    run_backtest, run_backtest_with_pyramid, run_backtest_with_sizer, BacktestConfig,
    BacktestResult, BacktestSizingConfig, CostModel, EquityPoint, Fill, FillModel, PyramidConfig,
    PyramidTrade, Side, Trade, TradeDirection,
};
pub use backtest_polars::{
    load_streaming_sweep_results, run_backtest_polars, run_donchian_backtest_polars,
    run_donchian_sweep_polars, run_strategy_sweep_polars, run_strategy_sweep_polars_cached,
    run_strategy_sweep_polars_lazy, run_strategy_sweep_polars_optimized,
    run_strategy_sweep_polars_parallel, run_strategy_sweep_polars_streaming,
    DonchianBacktestConfig, PolarsBacktestConfig, PolarsBacktestResult, StreamingSweepConfig,
    StreamingSweepProgress, StreamingSweepResult, StreamingSweepSummary,
};
// Re-export IntoLazy trait for DataFrame.lazy() calls
pub use analysis::{
    AnalysisConfig, EdgeRatioStats, ExcursionStats, HoldingBucket, HoldingPeriodStats,
    RegimeAnalysis, RegimeMetrics, ReturnDistribution, StatisticalAnalysis, TradeAnalysis,
    TradeExcursion, VolAtEntryStats, VolRegime,
};
pub use analysis_polars::{
    compute_analysis, compute_regime_analysis, compute_return_distribution, compute_trade_analysis,
};
pub use bar::Bar;
pub use clustering::{
    add_cluster_column, cluster_representatives, cluster_strategies, cluster_summary,
    elbow_analysis, select_diverse_by_robust_score, select_diverse_by_sharpe,
    select_diverse_strategies, ClusteringError, ClusteringResult, DiverseSelectionConfig,
    DiverseSelectionResult, KMeansConfig, DEFAULT_CLUSTER_FEATURES, EXTENDED_CLUSTER_FEATURES,
    ROBUSTNESS_CLUSTER_FEATURES,
};
pub use data::{
    bars_to_dataframe, build_yahoo_chart_url, build_yahoo_url, dataframe_to_bars,
    get_parquet_date_range, parquet_path, parse_yahoo_chart_json, parse_yahoo_csv,
    partition_by_year, read_parquet, scan_multiple_parquet_lazy, scan_parquet_lazy,
    scan_symbol_parquet_lazy, write_parquet, write_partitioned_parquet, CacheMetadata,
    DataQualityChecker, DataQualityReport, DataSource, FetchRequest, FetchResult, ProviderError,
    QualityIssue,
};
pub use error::TrendLabError;
pub use exploration::{
    build_exploration_state_from_history, build_tested_configs_index,
    calculate_exploit_probability, denormalize_to_params, generate_lhs_for_strategy,
    generate_random_config, get_param_bounds, lhs_samples_to_normalized, normalize_config,
    record_history_entry, select_exploration_mode, select_exploration_mode_with_config,
    ExplorationConfig, ExplorationMode, ExplorationState, NormalizedConfig, ParamBounds,
    StrategyCoverage, TestedConfigsIndex, DEFAULT_CELL_SIZE,
};
pub use indicator_cache::{
    collect_indicator_requirements, extract_indicator_requirements, CacheStats, IndicatorCache,
    IndicatorKey, LazyIndicatorCache,
};
pub use indicators::{
    aroon, aroon_down, aroon_up, atr, atr_wilder, bollinger_bands, cci, darvas_boxes, dmi,
    donchian_channel, ema_close, heikin_ashi, high_proximity, ichimoku, keltner_channel, macd,
    minus_di, minus_dm, opening_range, parabolic_sar, plus_di, plus_dm, prior_day_range,
    range_breakout_levels, roc, rolling_max_close, rolling_max_high, rolling_min_close,
    rolling_min_low, rolling_std, rsi, sma_close, starc_bands, stochastic, supertrend, true_range,
    williams_r, AroonIndicator, BollingerBands, DarvasBox, DonchianChannel, HABar, HighProximity,
    IchimokuValue, KeltnerChannel, MACDEntryMode, MACDValue, MAType, OpeningPeriod, OpeningRange,
    ParabolicSAR, STARCBands, StochasticValue, SupertrendValue, DMI,
};
pub use indicators_polars::{
    adx_expr, apply_aroon_exprs, apply_bollinger_exprs, apply_dmi_exprs, apply_heikin_ashi_exprs,
    apply_ichimoku_exprs, apply_indicators, apply_keltner_exprs, apply_macd_exprs,
    apply_opening_range_exprs, apply_parabolic_sar_exprs, apply_starc_exprs,
    apply_stochastic_exprs, apply_supertrend_exprs, aroon_down_expr, aroon_oscillator_expr,
    aroon_up_expr, atr_sma_expr, atr_wilder_expr, bollinger_bands_exprs, cci_expr,
    donchian_channel_exprs, dx_expr, ema_close_expr, minus_di_expr, minus_dm_expr,
    minus_dm_smoothed_expr, plus_di_expr, plus_dm_expr, plus_dm_smoothed_expr, roc_expr,
    rolling_std_expr, rsi_expr, sma_close_expr, starc_bands_exprs, supertrend_basic_exprs,
    true_range_expr, williams_r_expr, IndicatorSet, IndicatorSpec,
};
pub use latin_hypercube::{
    generate_lhs_2d, generate_lhs_3d, generate_lhs_samples, LatinHypercubeSampler, LhsConfig,
};
pub use leaderboard::{
    combine_equity_curves_realistic, combine_equity_curves_simple, compute_confidence_from_equity,
    compute_cross_sector_confidence_from_metrics, compute_cross_symbol_confidence_from_metrics,
    generate_session_id, AggregatedConfigResult, AggregatedMetrics, CombinedEquityAggregation,
    CombinedEquityConfig, CombinedEquityResult, CombinedEquityWeighting, CrossSymbolLeaderboard,
    CrossSymbolRankMetric, HistoryEntry, HistoryLogger, Leaderboard, LeaderboardEntry,
    LeaderboardScope, RankingWeights, RiskProfile, RobustScoreConfig,
};
pub use metrics::{compute_metrics, Metrics};
pub use polars::prelude::IntoLazy;
pub use sector_analysis::{
    best_strategy_per_sector, filter_sectors, sector_concentration, sector_dispersion,
    sector_performance, sector_summary_ranked, sector_vs_universe, top_per_sector,
};
pub use sizing::{
    turtle_sizer, FixedSizer, PositionSizer, SizeResult, SizingConfig, VolatilitySizer,
};
pub use statistics::{
    benjamini_hochberg, block_bootstrap_ci, block_bootstrap_sharpe, bonferroni, bootstrap_ci,
    bootstrap_sharpe, holm_bonferroni, one_sided_mean_pvalue, permutation_test, sample_statistics,
    BlockBootstrapConfig, BootstrapConfig, BootstrapMethod, BootstrapResult, ConfidenceGrade,
    MultipleComparisonMethod, MultipleComparisonResult, PermutationResult, SampleStatistics,
    StatisticsError, StrategyStatistics,
};
pub use strategy::{
    AroonCrossStrategy, BollingerSqueezeStrategy, CCIStrategy, DarvasBoxStrategy, DmiAdxStrategy,
    DonchianBreakoutStrategy, EnsembleStrategy, FiftyTwoWeekHighMomentumStrategy,
    FiftyTwoWeekHighStrategy, FiftyTwoWeekHighTrailingStrategy, HeikinAshiRegimeStrategy,
    IchimokuStrategy, KeltnerBreakoutStrategy, LarryWilliamsStrategy, MACDAdxStrategy,
    MACDStrategy, MACrossoverStrategy, NullStrategy, OpeningRangeBreakoutStrategy,
    OscillatorConfluenceStrategy, ParabolicSARStrategy, ParabolicSarDelayedStrategy,
    ParabolicSarFilteredStrategy, Position, ROCStrategy, RSIBollingerStrategy, RSIStrategy,
    STARCBreakoutStrategy, Signal, StochasticStrategy, Strategy, SupertrendAsymmetricStrategy,
    SupertrendConfirmedStrategy, SupertrendCooldownStrategy, SupertrendStrategy,
    SupertrendVolumeStrategy, TradingMode, TsmomStrategy, VotingMethod, WilliamsRStrategy,
};
pub use strategy_v2::{
    create_strategy_v2, create_strategy_v2_from_config, AroonV2, BollingerSqueezeV2, DarvasBoxV2,
    DmiAdxV2, DonchianBreakoutV2, EnsembleV2, FiftyTwoWeekHighV2, HeikinAshiV2, KeltnerV2,
    LarryWilliamsV2, MACrossoverV2, OpeningRangeBreakoutV2, ParabolicSARV2, StarcV2, StrategySpec,
    StrategyV2, SupertrendV2, TsmomV2,
};
pub use sweep::{
    compute_cost_sensitivity, compute_neighbor_sensitivity, create_strategy_from_config,
    generate_summary_markdown, run_single_config_backtest, run_strategy_sweep, run_sweep,
    AggregatedPortfolioResult, ConfigId, CostSensitivity, MultiStrategyGrid,
    MultiStrategySweepResult, MultiSweepResult, NeighborSensitivity, RankMetric, ResultPaths,
    RunManifest, StrategyBestResult, StrategyComparisonEntry, StrategyConfigId, StrategyGridConfig,
    StrategyParams, StrategyTypeId, SweepConfig, SweepConfigResult, SweepDepth, SweepGrid,
    SweepResult,
};
pub use sweep_polars::{
    analyze_sweep, compare_strategies, enrich_with_sector, multi_sweep_to_dataframe,
    multi_sweep_with_sectors, parameter_heatmap, parameter_sensitivity, read_sweep_parquet,
    select_diverse_from_sweep, select_diverse_robust, select_diverse_top_n, sweep_to_dataframe,
    top_configs_by_sharpe, write_sweep_parquet, SweepAnalysis, SweepQuery,
};
pub use universe::{Sector, Universe, UniverseError};
pub use validation::{
    generate_ts_cv_splits, generate_walk_forward_folds, slice_by_index, train_test_split_by_date,
    CVSplit, FoldResult, TimeSeriesCVConfig, ValidationError, WalkForwardConfig, WalkForwardFold,
    WalkForwardResult,
};

use std::path::{Path, PathBuf};

/// Returns the absolute path to the artifacts directory.
///
/// Resolves project root based on executable location, handling the case where
/// the binary is in `target/release/` or `target/debug/`.
///
/// Priority:
/// 1. `TRENDLAB_ARTIFACTS` environment variable (if set)
/// 2. Derived from executable location (goes up from target/release to project root)
/// 3. Falls back to `./artifacts` relative to current working directory
pub fn artifacts_dir() -> PathBuf {
    // Check environment variable first
    if let Ok(dir) = std::env::var("TRENDLAB_ARTIFACTS") {
        return PathBuf::from(dir);
    }

    // Derive from executable location
    if let Ok(exe_path) = std::env::current_exe() {
        let dir = exe_path.parent().unwrap_or(Path::new(".")).to_path_buf();

        // If in target/release or target/debug, go up to project root
        if dir.ends_with("release") || dir.ends_with("debug") {
            if let Some(target) = dir.parent() {
                if let Some(project_root) = target.parent() {
                    return project_root.join("artifacts");
                }
            }
        }

        // Not in target directory, use artifacts relative to exe dir
        return dir.join("artifacts");
    }

    // Fallback to relative path
    PathBuf::from("artifacts")
}

/// Re-export commonly used types
pub mod prelude {
    pub use crate::backtest::{
        run_backtest_with_pyramid, BacktestConfig, BacktestResult, CostModel, FillModel,
        PyramidConfig, PyramidTrade,
    };
    pub use crate::bar::Bar;
    pub use crate::data::{DataQualityChecker, DataQualityReport};
    pub use crate::error::TrendLabError;
    pub use crate::indicators::{
        aroon, atr, atr_wilder, bollinger_bands, darvas_boxes, dmi, donchian_channel, ema_close,
        heikin_ashi, high_proximity, keltner_channel, rolling_max_close, rolling_min_close,
        rolling_std, sma_close, starc_bands, supertrend, true_range, AroonIndicator,
        BollingerBands, DarvasBox, DonchianChannel, HABar, HighProximity, KeltnerChannel, MAType,
        STARCBands, SupertrendValue, DMI,
    };
    pub use crate::sizing::{FixedSizer, PositionSizer, VolatilitySizer};
    pub use crate::strategy::{
        AroonCrossStrategy, BollingerSqueezeStrategy, CCIStrategy, DarvasBoxStrategy,
        DmiAdxStrategy, DonchianBreakoutStrategy, FiftyTwoWeekHighStrategy,
        HeikinAshiRegimeStrategy, IchimokuStrategy, KeltnerBreakoutStrategy, LarryWilliamsStrategy,
        MACDAdxStrategy, MACDStrategy, MACrossoverStrategy, OscillatorConfluenceStrategy, Position,
        ROCStrategy, RSIBollingerStrategy, RSIStrategy, STARCBreakoutStrategy, Signal,
        StochasticStrategy, Strategy, SupertrendStrategy, TsmomStrategy, WilliamsRStrategy,
    };
    pub use crate::strategy_v2::{
        create_strategy_v2, DonchianBreakoutV2, MACrossoverV2, StrategySpec, StrategyV2, TsmomV2,
    };
    pub use crate::sweep::{
        create_strategy_from_config, run_strategy_sweep, run_sweep, ConfigId, MultiStrategyGrid,
        MultiStrategySweepResult, RankMetric, StrategyBestResult, StrategyComparisonEntry,
        StrategyConfigId, StrategyGridConfig, StrategyTypeId, SweepConfig, SweepGrid, SweepResult,
    };
    pub use crate::universe::{Sector, Universe};
}
