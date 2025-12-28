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
pub mod statistics;
pub mod validation;
pub mod indicators;
pub mod indicators_polars;
pub mod leaderboard;
pub mod metrics;
pub mod sector_analysis;
pub mod sizing;
pub mod strategy;
pub mod strategy_v2;
pub mod sweep;
pub mod sweep_polars;
pub mod universe;

pub use artifact::{
    create_donchian_artifact, ArtifactBuilder, ArtifactCostModel, ArtifactError, ArtifactMetadata,
    DataRange, IndicatorDef, IndicatorValue, OhlcvData, ParamValue, ParityVector, ParityVectors,
    Rule, Rules, StrategyArtifact, SCHEMA_VERSION,
};
pub use backtest::{
    run_backtest, run_backtest_with_pyramid, run_backtest_with_sizer, BacktestConfig,
    BacktestResult, BacktestSizingConfig, CostModel, EquityPoint, Fill, FillModel, PyramidConfig,
    PyramidTrade, Side, Trade, TradeDirection,
};
pub use backtest_polars::{
    run_backtest_polars, run_donchian_backtest_polars, run_donchian_sweep_polars,
    run_strategy_sweep_polars, run_strategy_sweep_polars_parallel, DonchianBacktestConfig,
    PolarsBacktestConfig, PolarsBacktestResult,
};
// Re-export IntoLazy trait for DataFrame.lazy() calls
pub use bar::Bar;
pub use data::{
    bars_to_dataframe, build_yahoo_chart_url, build_yahoo_url, dataframe_to_bars, parquet_path,
    parse_yahoo_chart_json, parse_yahoo_csv, partition_by_year, read_parquet,
    scan_multiple_parquet_lazy, scan_parquet_lazy, scan_symbol_parquet_lazy, write_parquet,
    write_partitioned_parquet, CacheMetadata, DataQualityChecker, DataQualityReport, DataSource,
    FetchRequest, FetchResult, ProviderError, QualityIssue,
};
pub use error::TrendLabError;
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
pub use leaderboard::{
    compute_confidence_from_equity, generate_session_id, AggregatedConfigResult,
    AggregatedMetrics, CrossSymbolLeaderboard, CrossSymbolRankMetric, Leaderboard,
    LeaderboardEntry, LeaderboardScope,
};
pub use metrics::{compute_metrics, Metrics};
pub use polars::prelude::IntoLazy;
pub use analysis::{
    AnalysisConfig, EdgeRatioStats, ExcursionStats, HoldingBucket, HoldingPeriodStats,
    RegimeAnalysis, RegimeMetrics, ReturnDistribution, StatisticalAnalysis, TradeAnalysis,
    TradeExcursion, VolAtEntryStats, VolRegime,
};
pub use analysis_polars::{
    compute_analysis, compute_regime_analysis, compute_return_distribution, compute_trade_analysis,
};
pub use sizing::{
    turtle_sizer, FixedSizer, PositionSizer, SizeResult, SizingConfig, VolatilitySizer,
};
pub use strategy::{
    AroonCrossStrategy, BollingerSqueezeStrategy, CCIStrategy, DarvasBoxStrategy, DmiAdxStrategy,
    DonchianBreakoutStrategy, EnsembleStrategy, FiftyTwoWeekHighStrategy, HeikinAshiRegimeStrategy,
    IchimokuStrategy, KeltnerBreakoutStrategy, LarryWilliamsStrategy, MACDAdxStrategy,
    MACDStrategy, MACrossoverStrategy, NullStrategy, OpeningRangeBreakoutStrategy,
    OscillatorConfluenceStrategy, ParabolicSARStrategy, Position, ROCStrategy, RSIBollingerStrategy,
    RSIStrategy, STARCBreakoutStrategy, Signal, StochasticStrategy, Strategy, SupertrendStrategy,
    TradingMode, TsmomStrategy, VotingMethod, WilliamsRStrategy,
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
    sweep_to_dataframe, top_configs_by_sharpe, write_sweep_parquet, SweepAnalysis, SweepQuery,
};
pub use sector_analysis::{
    best_strategy_per_sector, filter_sectors, sector_concentration, sector_dispersion,
    sector_performance, sector_summary_ranked, sector_vs_universe, top_per_sector,
};
pub use clustering::{
    add_cluster_column, cluster_representatives, cluster_strategies, cluster_summary,
    elbow_analysis, ClusteringError, ClusteringResult, KMeansConfig, DEFAULT_CLUSTER_FEATURES,
};
pub use validation::{
    generate_ts_cv_splits, generate_walk_forward_folds, slice_by_index, train_test_split_by_date,
    CVSplit, FoldResult, TimeSeriesCVConfig, ValidationError, WalkForwardConfig, WalkForwardFold,
    WalkForwardResult,
};
pub use statistics::{
    benjamini_hochberg, bonferroni, bootstrap_ci, bootstrap_sharpe, holm_bonferroni,
    permutation_test, sample_statistics, BootstrapConfig, BootstrapResult, ConfidenceGrade,
    MultipleComparisonMethod, MultipleComparisonResult, PermutationResult, SampleStatistics,
    StatisticsError, StrategyStatistics,
};
pub use universe::{Sector, Universe, UniverseError};

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
        AroonCrossStrategy, BollingerSqueezeStrategy, CCIStrategy, DarvasBoxStrategy, DmiAdxStrategy,
        DonchianBreakoutStrategy, FiftyTwoWeekHighStrategy, HeikinAshiRegimeStrategy,
        IchimokuStrategy, KeltnerBreakoutStrategy, LarryWilliamsStrategy, MACDAdxStrategy,
        MACDStrategy, MACrossoverStrategy, OscillatorConfluenceStrategy, Position, ROCStrategy,
        RSIBollingerStrategy, RSIStrategy, STARCBreakoutStrategy, Signal, StochasticStrategy,
        Strategy, SupertrendStrategy, TsmomStrategy, WilliamsRStrategy,
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
