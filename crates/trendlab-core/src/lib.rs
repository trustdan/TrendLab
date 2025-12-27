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
pub mod bar;
pub mod data;
pub mod error;
pub mod indicators;
pub mod metrics;
pub mod sizing;
pub mod strategy;
pub mod sweep;
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
pub use bar::Bar;
pub use data::{
    bars_to_dataframe, build_yahoo_chart_url, build_yahoo_url, dataframe_to_bars, parquet_path,
    parse_yahoo_chart_json, parse_yahoo_csv, partition_by_year, read_parquet, write_parquet,
    write_partitioned_parquet, CacheMetadata, DataQualityChecker, DataQualityReport, DataSource,
    FetchRequest, FetchResult, ProviderError, QualityIssue,
};
pub use error::TrendLabError;
pub use indicators::{
    atr, atr_wilder, donchian_channel, ema_close, sma_close, true_range, DonchianChannel, MAType,
};
pub use metrics::{compute_metrics, Metrics};
pub use sizing::{
    turtle_sizer, FixedSizer, PositionSizer, SizeResult, SizingConfig, VolatilitySizer,
};
pub use strategy::{
    DonchianBreakoutStrategy, MACrossoverStrategy, NullStrategy, Position, Signal, Strategy,
    TsmomStrategy,
};
pub use sweep::{
    compute_cost_sensitivity, compute_neighbor_sensitivity, generate_summary_markdown, run_sweep,
    AggregatedPortfolioResult, ConfigId, CostSensitivity, MultiSweepResult, NeighborSensitivity,
    RankMetric, ResultPaths, RunManifest, SweepConfig, SweepConfigResult, SweepGrid, SweepResult,
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
    pub use crate::sweep::{run_sweep, ConfigId, RankMetric, SweepConfig, SweepGrid, SweepResult};
    pub use crate::universe::{Sector, Universe};
}
