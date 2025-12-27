//! TrendLab Core - Domain types, strategies, and metrics for trend-following research.
//!
//! This crate provides:
//! - Bar and OHLCV data types
//! - Strategy trait and implementations
//! - Performance metrics calculations
//! - Data provider traits
//! - Parameter sweep infrastructure
//! - Strategy artifact export for Pine Script parity

pub mod artifact;
pub mod backtest;
pub mod backtest_polars;
pub mod bar;
pub mod data;
pub mod error;
pub mod indicators;
pub mod indicators_polars;
pub mod metrics;
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
    PyramidTrade, Side, Trade,
};
pub use backtest_polars::{
    run_backtest_polars, run_donchian_backtest_polars, run_donchian_sweep_polars,
    run_strategy_sweep_polars, run_strategy_sweep_polars_parallel, DonchianBacktestConfig,
    PolarsBacktestConfig, PolarsBacktestResult,
};
// Re-export IntoLazy trait for DataFrame.lazy() calls
pub use polars::prelude::IntoLazy;
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
    atr, atr_wilder, donchian_channel, ema_close, sma_close, true_range, DonchianChannel, MAType,
};
pub use indicators_polars::{
    apply_indicators, atr_sma_expr, atr_wilder_expr, donchian_channel_exprs, ema_close_expr,
    sma_close_expr, true_range_expr, IndicatorSet, IndicatorSpec,
};
pub use metrics::{compute_metrics, Metrics};
pub use sizing::{
    turtle_sizer, FixedSizer, PositionSizer, SizeResult, SizingConfig, VolatilitySizer,
};
pub use strategy::{
    DonchianBreakoutStrategy, MACrossoverStrategy, NullStrategy, Position, Signal, Strategy,
    TsmomStrategy,
};
pub use strategy_v2::{
    create_strategy_v2, create_strategy_v2_from_config, DonchianBreakoutV2, MACrossoverV2,
    StrategySpec, StrategyV2, TsmomV2,
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
    analyze_sweep, compare_strategies, multi_sweep_to_dataframe, parameter_heatmap,
    parameter_sensitivity, read_sweep_parquet, sweep_to_dataframe, top_configs_by_sharpe,
    write_sweep_parquet, SweepAnalysis, SweepQuery,
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
        atr, atr_wilder, donchian_channel, ema_close, sma_close, true_range, DonchianChannel,
        MAType,
    };
    pub use crate::sizing::{FixedSizer, PositionSizer, VolatilitySizer};
    pub use crate::strategy::{
        DonchianBreakoutStrategy, MACrossoverStrategy, Position, Signal, Strategy, TsmomStrategy,
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
