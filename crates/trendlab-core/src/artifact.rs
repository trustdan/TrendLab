//! Strategy artifact export for Pine Script parity validation.
//!
//! A StrategyArtifact captures everything needed to:
//! 1. Generate an equivalent Pine Script
//! 2. Validate that the Pine Script produces identical signals

use crate::backtest::{BacktestResult, CostModel, FillModel};
use crate::bar::Bar;
use crate::indicators::{donchian_channel, parabolic_sar, supertrend, DonchianChannel};
use crate::strategy::{
    DonchianBreakoutStrategy, FiftyTwoWeekHighStrategy, ParabolicSARStrategy, Position, Signal,
    Strategy, SupertrendStrategy,
};
use crate::sweep::StrategyConfigId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Current schema version for StrategyArtifact.
pub const SCHEMA_VERSION: &str = "1.0.0";

/// Complete strategy artifact for Pine Script parity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyArtifact {
    pub schema_version: String,
    pub strategy_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_version: Option<String>,
    pub symbol: String,
    pub timeframe: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_range: Option<DataRange>,
    pub indicators: Vec<IndicatorDef>,
    pub rules: Rules,
    pub fill_model: String,
    pub cost_model: ArtifactCostModel,
    pub parameters: HashMap<String, ParamValue>,
    pub parity_vectors: ParityVectors,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ArtifactMetadata>,
}

/// Date range of backtest data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Indicator definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorDef {
    pub id: String,
    #[serde(rename = "type")]
    pub indicator_type: String,
    pub params: HashMap<String, ParamValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pine_expr: Option<String>,
}

/// Parameter value (number, string, or boolean).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

impl From<i64> for ParamValue {
    fn from(v: i64) -> Self {
        ParamValue::Integer(v)
    }
}

impl From<usize> for ParamValue {
    fn from(v: usize) -> Self {
        ParamValue::Integer(v as i64)
    }
}

impl From<f64> for ParamValue {
    fn from(v: f64) -> Self {
        ParamValue::Float(v)
    }
}

impl From<&str> for ParamValue {
    fn from(v: &str) -> Self {
        ParamValue::String(v.to_string())
    }
}

impl From<bool> for ParamValue {
    fn from(v: bool) -> Self {
        ParamValue::Bool(v)
    }
}

/// Entry and exit rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rules {
    pub entry: Rule,
    pub exit: Rule,
}

/// Single trading rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub condition: String,
    pub pine_condition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_required: Option<String>,
}

/// Cost model for artifact (matches JSON schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCostModel {
    pub fees_bps_per_side: f64,
    pub slippage_bps: f64,
}

impl From<CostModel> for ArtifactCostModel {
    fn from(cm: CostModel) -> Self {
        ArtifactCostModel {
            fees_bps_per_side: cm.fees_bps_per_side,
            slippage_bps: cm.slippage_bps,
        }
    }
}

/// Parity test vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityVectors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub vectors: Vec<ParityVector>,
}

/// Single parity test vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityVector {
    pub ts: DateTime<Utc>,
    pub ohlcv: OhlcvData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicators: Option<HashMap<String, IndicatorValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_after: Option<String>,
}

/// OHLCV data for a bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OhlcvData {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
}

impl From<&Bar> for OhlcvData {
    fn from(bar: &Bar) -> Self {
        OhlcvData {
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: if bar.volume > 0.0 {
                Some(bar.volume)
            } else {
                None
            },
        }
    }
}

/// Indicator value (single number or multi-value like Donchian upper/lower).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IndicatorValue {
    Single(f64),
    Null,
    Multi(HashMap<String, f64>),
}

impl From<f64> for IndicatorValue {
    fn from(v: f64) -> Self {
        IndicatorValue::Single(v)
    }
}

impl From<Option<f64>> for IndicatorValue {
    fn from(v: Option<f64>) -> Self {
        match v {
            Some(val) => IndicatorValue::Single(val),
            None => IndicatorValue::Null,
        }
    }
}

impl From<DonchianChannel> for IndicatorValue {
    fn from(ch: DonchianChannel) -> Self {
        let mut map = HashMap::new();
        map.insert("upper".to_string(), ch.upper);
        map.insert("lower".to_string(), ch.lower);
        IndicatorValue::Multi(map)
    }
}

/// Optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Builder for constructing StrategyArtifact.
#[derive(Debug, Default)]
pub struct ArtifactBuilder {
    strategy_id: Option<String>,
    strategy_version: Option<String>,
    symbol: Option<String>,
    timeframe: Option<String>,
    indicators: Vec<IndicatorDef>,
    rules: Option<Rules>,
    fill_model: Option<String>,
    cost_model: Option<ArtifactCostModel>,
    parameters: HashMap<String, ParamValue>,
    bars: Option<Vec<Bar>>,
    backtest_result: Option<BacktestResult>,
    metadata: Option<ArtifactMetadata>,
}

impl ArtifactBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn strategy_id(mut self, id: impl Into<String>) -> Self {
        self.strategy_id = Some(id.into());
        self
    }

    pub fn strategy_version(mut self, version: impl Into<String>) -> Self {
        self.strategy_version = Some(version.into());
        self
    }

    pub fn symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    pub fn timeframe(mut self, tf: impl Into<String>) -> Self {
        self.timeframe = Some(tf.into());
        self
    }

    pub fn indicator(mut self, def: IndicatorDef) -> Self {
        self.indicators.push(def);
        self
    }

    pub fn rules(mut self, rules: Rules) -> Self {
        self.rules = Some(rules);
        self
    }

    pub fn fill_model(mut self, fm: FillModel) -> Self {
        self.fill_model = Some(match fm {
            FillModel::NextOpen => "NextOpen".to_string(),
        });
        self
    }

    pub fn cost_model(mut self, cm: CostModel) -> Self {
        self.cost_model = Some(cm.into());
        self
    }

    pub fn parameter(mut self, key: impl Into<String>, value: impl Into<ParamValue>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    pub fn bars(mut self, bars: Vec<Bar>) -> Self {
        self.bars = Some(bars);
        self
    }

    pub fn backtest_result(mut self, result: BacktestResult) -> Self {
        self.backtest_result = Some(result);
        self
    }

    pub fn metadata(mut self, meta: ArtifactMetadata) -> Self {
        self.metadata = Some(meta);
        self
    }

    /// Build the artifact, generating parity vectors from bars and backtest result.
    pub fn build(self) -> Result<StrategyArtifact, ArtifactError> {
        let strategy_id = self
            .strategy_id
            .ok_or(ArtifactError::MissingField("strategy_id"))?;
        let symbol = self.symbol.ok_or(ArtifactError::MissingField("symbol"))?;
        let timeframe = self
            .timeframe
            .ok_or(ArtifactError::MissingField("timeframe"))?;
        let rules = self.rules.ok_or(ArtifactError::MissingField("rules"))?;
        let fill_model = self
            .fill_model
            .ok_or(ArtifactError::MissingField("fill_model"))?;
        let cost_model = self
            .cost_model
            .ok_or(ArtifactError::MissingField("cost_model"))?;
        let bars = self.bars.ok_or(ArtifactError::MissingField("bars"))?;

        // Build data range from bars
        let data_range = if !bars.is_empty() {
            Some(DataRange {
                start: bars.first().unwrap().ts,
                end: bars.last().unwrap().ts,
            })
        } else {
            None
        };

        // Build parity vectors
        let vectors = build_basic_parity_vectors(&bars);

        Ok(StrategyArtifact {
            schema_version: SCHEMA_VERSION.to_string(),
            strategy_id,
            strategy_version: self.strategy_version,
            symbol,
            timeframe,
            generated_at: Some(Utc::now()),
            data_range,
            indicators: self.indicators,
            rules,
            fill_model,
            cost_model,
            parameters: self.parameters,
            parity_vectors: ParityVectors {
                description: Some("Parity test vectors for Pine Script validation".to_string()),
                vectors,
            },
            metadata: self.metadata,
        })
    }
}

/// Build basic parity vectors from bars (without indicator values).
fn build_basic_parity_vectors(bars: &[Bar]) -> Vec<ParityVector> {
    bars.iter()
        .map(|bar| ParityVector {
            ts: bar.ts,
            ohlcv: OhlcvData::from(bar),
            indicators: None,
            signal: None,
            position_after: None,
        })
        .collect()
}

/// Error type for artifact building.
#[derive(Debug, Clone)]
pub enum ArtifactError {
    MissingField(&'static str),
    InvalidData(String),
    UnsupportedStrategy(String),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactError::MissingField(field) => write!(f, "Missing required field: {}", field),
            ArtifactError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
            ArtifactError::UnsupportedStrategy(s) => {
                write!(f, "Strategy does not support artifact export yet: {}", s)
            }
        }
    }
}

impl std::error::Error for ArtifactError {}

/// Create artifact for Donchian breakout strategy.
pub fn create_donchian_artifact(
    bars: &[Bar],
    entry_lookback: usize,
    exit_lookback: usize,
    cost_model: CostModel,
    _backtest_result: &BacktestResult,
) -> Result<StrategyArtifact, ArtifactError> {
    if bars.is_empty() {
        return Err(ArtifactError::InvalidData(
            "bars cannot be empty".to_string(),
        ));
    }

    let symbol = &bars[0].symbol;
    let timeframe = &bars[0].timeframe;

    // Compute indicator values for all bars
    let entry_channel = donchian_channel(bars, entry_lookback);
    let exit_channel = donchian_channel(bars, exit_lookback);

    // Build strategy to compute signals
    let mut strategy = DonchianBreakoutStrategy::new(entry_lookback, exit_lookback);
    strategy.reset();

    // Build parity vectors with indicators and signals
    let mut position = Position::Flat;
    let mut vectors = Vec::with_capacity(bars.len());

    for (i, bar) in bars.iter().enumerate() {
        let hist = &bars[..=i];
        let signal = strategy.signal(hist, position);

        // Update position based on signal
        let signal_str = match signal {
            Signal::Hold => None,
            Signal::EnterLong => {
                position = Position::Long;
                Some("enter_long".to_string())
            }
            Signal::ExitLong => {
                position = Position::Flat;
                Some("exit_long".to_string())
            }
            Signal::AddLong => {
                // AddLong keeps position as Long (for pyramiding)
                Some("add_long".to_string())
            }
            // Short signals not yet implemented (long-only first)
            Signal::EnterShort => {
                position = Position::Short;
                Some("enter_short".to_string())
            }
            Signal::ExitShort => {
                position = Position::Flat;
                Some("exit_short".to_string())
            }
            Signal::AddShort => Some("add_short".to_string()),
        };

        let position_str = match position {
            Position::Flat => "flat",
            Position::Long => "long",
            Position::Short => "short",
        };

        // Build indicator values
        let mut indicators = HashMap::new();
        if let Some(ch) = entry_channel[i] {
            indicators.insert("donchian_entry".to_string(), IndicatorValue::from(ch));
        }
        if let Some(ch) = exit_channel[i] {
            indicators.insert("donchian_exit".to_string(), IndicatorValue::from(ch));
        }

        vectors.push(ParityVector {
            ts: bar.ts,
            ohlcv: OhlcvData::from(bar),
            indicators: if indicators.is_empty() {
                None
            } else {
                Some(indicators)
            },
            signal: signal_str,
            position_after: Some(position_str.to_string()),
        });
    }

    // Build indicator definitions
    let indicators = vec![
        IndicatorDef {
            id: "donchian_entry".to_string(),
            indicator_type: "donchian".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert("lookback".to_string(), ParamValue::from(entry_lookback));
                p
            },
            pine_expr: Some(format!(
                "ta.highest(high[1], {}) / ta.lowest(low[1], {})",
                entry_lookback, entry_lookback
            )),
        },
        IndicatorDef {
            id: "donchian_exit".to_string(),
            indicator_type: "donchian".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert("lookback".to_string(), ParamValue::from(exit_lookback));
                p
            },
            pine_expr: Some(format!(
                "ta.highest(high[1], {}) / ta.lowest(low[1], {})",
                exit_lookback, exit_lookback
            )),
        },
    ];

    // Build rules
    let rules = Rules {
        entry: Rule {
            condition: "close > donchian_entry.upper".to_string(),
            pine_condition: format!("close > ta.highest(high[1], {})", entry_lookback),
            position_required: Some("flat".to_string()),
        },
        exit: Rule {
            condition: "close < donchian_exit.lower".to_string(),
            pine_condition: format!("close < ta.lowest(low[1], {})", exit_lookback),
            position_required: Some("long".to_string()),
        },
    };

    // Build parameters
    let mut parameters = HashMap::new();
    parameters.insert(
        "entry_lookback".to_string(),
        ParamValue::from(entry_lookback),
    );
    parameters.insert("exit_lookback".to_string(), ParamValue::from(exit_lookback));

    Ok(StrategyArtifact {
        schema_version: SCHEMA_VERSION.to_string(),
        strategy_id: "donchian_breakout".to_string(),
        strategy_version: Some("1.0.0".to_string()),
        symbol: symbol.to_string(),
        timeframe: timeframe.to_string(),
        generated_at: Some(Utc::now()),
        data_range: Some(DataRange {
            start: bars.first().unwrap().ts,
            end: bars.last().unwrap().ts,
        }),
        indicators,
        rules,
        fill_model: "NextOpen".to_string(),
        cost_model: cost_model.into(),
        parameters,
        parity_vectors: ParityVectors {
            description: Some(format!(
                "Parity vectors for Donchian({}/{}) on {} bars",
                entry_lookback,
                exit_lookback,
                bars.len()
            )),
            vectors,
        },
        metadata: None,
    })
}

/// Create artifact for 52-week high breakout strategy.
pub fn create_52wk_high_artifact(
    bars: &[Bar],
    period: usize,
    entry_pct: f64,
    exit_pct: f64,
    cost_model: CostModel,
) -> Result<StrategyArtifact, ArtifactError> {
    if bars.is_empty() {
        return Err(ArtifactError::InvalidData(
            "bars cannot be empty".to_string(),
        ));
    }

    let symbol = &bars[0].symbol;
    let timeframe = &bars[0].timeframe;

    // Build strategy to compute signals
    let mut strategy = FiftyTwoWeekHighStrategy::new(period, entry_pct, exit_pct);
    strategy.reset();

    // Build parity vectors with indicators and signals
    let mut position = Position::Flat;
    let mut vectors = Vec::with_capacity(bars.len());

    for (i, bar) in bars.iter().enumerate() {
        let hist = &bars[..=i];
        let signal = strategy.signal(hist, position);

        // Update position based on signal
        let signal_str = match signal {
            Signal::Hold => None,
            Signal::EnterLong => {
                position = Position::Long;
                Some("enter_long".to_string())
            }
            Signal::ExitLong => {
                position = Position::Flat;
                Some("exit_long".to_string())
            }
            _ => None,
        };

        let position_str = match position {
            Position::Flat => "flat",
            Position::Long => "long",
            Position::Short => "short",
        };

        // Build indicator values - 52wk high uses highest high over period
        let mut indicators = HashMap::new();
        if i >= period {
            let highest = bars[i.saturating_sub(period)..i]
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let lowest = bars[i.saturating_sub(period)..i]
                .iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min);
            let mut ch = HashMap::new();
            ch.insert("upper".to_string(), highest);
            ch.insert("lower".to_string(), lowest);
            indicators.insert("period_channel".to_string(), IndicatorValue::Multi(ch));
        }

        vectors.push(ParityVector {
            ts: bar.ts,
            ohlcv: OhlcvData::from(bar),
            indicators: if indicators.is_empty() {
                None
            } else {
                Some(indicators)
            },
            signal: signal_str,
            position_after: Some(position_str.to_string()),
        });
    }

    // Build indicator definitions
    let indicators = vec![IndicatorDef {
        id: "period_channel".to_string(),
        indicator_type: "highest_lowest".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert("period".to_string(), ParamValue::from(period));
            p
        },
        pine_expr: Some(format!(
            "ta.highest(high[1], {}) / ta.lowest(low[1], {})",
            period, period
        )),
    }];

    // Build rules
    let rules = Rules {
        entry: Rule {
            condition: format!(
                "close > highest_high * {:.2} (within {}% of {}-day high)",
                entry_pct / 100.0,
                100.0 - entry_pct,
                period
            ),
            pine_condition: format!(
                "close >= ta.highest(high[1], {}) * {}",
                period,
                entry_pct / 100.0
            ),
            position_required: Some("flat".to_string()),
        },
        exit: Rule {
            condition: format!(
                "close < highest_high * {:.2} (dropped {}% from {}-day high)",
                exit_pct / 100.0,
                100.0 - exit_pct,
                period
            ),
            pine_condition: format!(
                "close < ta.highest(high[1], {}) * {}",
                period,
                exit_pct / 100.0
            ),
            position_required: Some("long".to_string()),
        },
    };

    // Build parameters
    let mut parameters = HashMap::new();
    parameters.insert("period".to_string(), ParamValue::from(period));
    parameters.insert("entry_pct".to_string(), ParamValue::from(entry_pct));
    parameters.insert("exit_pct".to_string(), ParamValue::from(exit_pct));

    Ok(StrategyArtifact {
        schema_version: SCHEMA_VERSION.to_string(),
        strategy_id: "52wk_high".to_string(),
        strategy_version: Some("1.0.0".to_string()),
        symbol: symbol.to_string(),
        timeframe: timeframe.to_string(),
        generated_at: Some(Utc::now()),
        data_range: Some(DataRange {
            start: bars.first().unwrap().ts,
            end: bars.last().unwrap().ts,
        }),
        indicators,
        rules,
        fill_model: "NextOpen".to_string(),
        cost_model: cost_model.into(),
        parameters,
        parity_vectors: ParityVectors {
            description: Some(format!(
                "Parity vectors for 52WkHigh({}/{:.0}/{:.0}) on {} bars",
                period,
                entry_pct,
                exit_pct,
                bars.len()
            )),
            vectors,
        },
        metadata: None,
    })
}

/// Create artifact for Supertrend strategy.
pub fn create_supertrend_artifact(
    bars: &[Bar],
    atr_period: usize,
    multiplier: f64,
    cost_model: CostModel,
) -> Result<StrategyArtifact, ArtifactError> {
    if bars.is_empty() {
        return Err(ArtifactError::InvalidData(
            "bars cannot be empty".to_string(),
        ));
    }

    let symbol = &bars[0].symbol;
    let timeframe = &bars[0].timeframe;

    // Compute indicator values
    let st_values = supertrend(bars, atr_period, multiplier);

    // Build strategy to compute signals
    let mut strategy = SupertrendStrategy::new(atr_period, multiplier);
    strategy.reset();

    // Build parity vectors with indicators and signals
    let mut position = Position::Flat;
    let mut vectors = Vec::with_capacity(bars.len());

    for (i, bar) in bars.iter().enumerate() {
        let hist = &bars[..=i];
        let signal = strategy.signal(hist, position);

        // Update position based on signal
        let signal_str = match signal {
            Signal::Hold => None,
            Signal::EnterLong => {
                position = Position::Long;
                Some("enter_long".to_string())
            }
            Signal::ExitLong => {
                position = Position::Flat;
                Some("exit_long".to_string())
            }
            _ => None,
        };

        let position_str = match position {
            Position::Flat => "flat",
            Position::Long => "long",
            Position::Short => "short",
        };

        // Build indicator values
        let mut indicators = HashMap::new();
        if let Some(st) = &st_values[i] {
            let mut st_map = HashMap::new();
            st_map.insert("supertrend".to_string(), st.supertrend);
            st_map.insert("upper_band".to_string(), st.upper_band);
            st_map.insert("lower_band".to_string(), st.lower_band);
            st_map.insert(
                "is_uptrend".to_string(),
                if st.is_uptrend { 1.0 } else { 0.0 },
            );
            indicators.insert("supertrend".to_string(), IndicatorValue::Multi(st_map));
        }

        vectors.push(ParityVector {
            ts: bar.ts,
            ohlcv: OhlcvData::from(bar),
            indicators: if indicators.is_empty() {
                None
            } else {
                Some(indicators)
            },
            signal: signal_str,
            position_after: Some(position_str.to_string()),
        });
    }

    // Build indicator definitions
    let indicators = vec![IndicatorDef {
        id: "supertrend".to_string(),
        indicator_type: "supertrend".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert("atr_period".to_string(), ParamValue::from(atr_period));
            p.insert("multiplier".to_string(), ParamValue::from(multiplier));
            p
        },
        pine_expr: Some(format!("ta.supertrend({}, {})", multiplier, atr_period)),
    }];

    // Build rules
    let rules = Rules {
        entry: Rule {
            condition: "Supertrend flips to uptrend (close crosses above supertrend line)".to_string(),
            pine_condition: format!(
                "[supertrend, direction] = ta.supertrend({}, {})\nta.crossover(close, supertrend)",
                multiplier, atr_period
            ),
            position_required: Some("flat".to_string()),
        },
        exit: Rule {
            condition: "Supertrend flips to downtrend (close crosses below supertrend line)"
                .to_string(),
            pine_condition: format!(
                "[supertrend, direction] = ta.supertrend({}, {})\nta.crossunder(close, supertrend)",
                multiplier, atr_period
            ),
            position_required: Some("long".to_string()),
        },
    };

    // Build parameters
    let mut parameters = HashMap::new();
    parameters.insert("atr_period".to_string(), ParamValue::from(atr_period));
    parameters.insert("multiplier".to_string(), ParamValue::from(multiplier));

    Ok(StrategyArtifact {
        schema_version: SCHEMA_VERSION.to_string(),
        strategy_id: "supertrend".to_string(),
        strategy_version: Some("1.0.0".to_string()),
        symbol: symbol.to_string(),
        timeframe: timeframe.to_string(),
        generated_at: Some(Utc::now()),
        data_range: Some(DataRange {
            start: bars.first().unwrap().ts,
            end: bars.last().unwrap().ts,
        }),
        indicators,
        rules,
        fill_model: "NextOpen".to_string(),
        cost_model: cost_model.into(),
        parameters,
        parity_vectors: ParityVectors {
            description: Some(format!(
                "Parity vectors for Supertrend({}/{:.1}) on {} bars",
                atr_period,
                multiplier,
                bars.len()
            )),
            vectors,
        },
        metadata: None,
    })
}

/// Create artifact for Parabolic SAR strategy.
pub fn create_parabolic_sar_artifact(
    bars: &[Bar],
    af_start: f64,
    af_step: f64,
    af_max: f64,
    cost_model: CostModel,
) -> Result<StrategyArtifact, ArtifactError> {
    if bars.is_empty() {
        return Err(ArtifactError::InvalidData(
            "bars cannot be empty".to_string(),
        ));
    }

    let symbol = &bars[0].symbol;
    let timeframe = &bars[0].timeframe;

    // Compute indicator values
    let sar_values = parabolic_sar(bars, af_start, af_step, af_max);

    // Build strategy to compute signals
    let mut strategy = ParabolicSARStrategy::new(af_start, af_step, af_max);
    strategy.reset();

    // Build parity vectors with indicators and signals
    let mut position = Position::Flat;
    let mut vectors = Vec::with_capacity(bars.len());

    for (i, bar) in bars.iter().enumerate() {
        let hist = &bars[..=i];
        let signal = strategy.signal(hist, position);

        // Update position based on signal
        let signal_str = match signal {
            Signal::Hold => None,
            Signal::EnterLong => {
                position = Position::Long;
                Some("enter_long".to_string())
            }
            Signal::ExitLong => {
                position = Position::Flat;
                Some("exit_long".to_string())
            }
            _ => None,
        };

        let position_str = match position {
            Position::Flat => "flat",
            Position::Long => "long",
            Position::Short => "short",
        };

        // Build indicator values
        let mut indicators = HashMap::new();
        if let Some(sar) = &sar_values[i] {
            let mut sar_map = HashMap::new();
            sar_map.insert("sar".to_string(), sar.sar);
            sar_map.insert("af".to_string(), sar.af);
            sar_map.insert("ep".to_string(), sar.ep);
            sar_map.insert(
                "is_uptrend".to_string(),
                if sar.is_uptrend { 1.0 } else { 0.0 },
            );
            indicators.insert("parabolic_sar".to_string(), IndicatorValue::Multi(sar_map));
        }

        vectors.push(ParityVector {
            ts: bar.ts,
            ohlcv: OhlcvData::from(bar),
            indicators: if indicators.is_empty() {
                None
            } else {
                Some(indicators)
            },
            signal: signal_str,
            position_after: Some(position_str.to_string()),
        });
    }

    // Build indicator definitions
    let indicators = vec![IndicatorDef {
        id: "parabolic_sar".to_string(),
        indicator_type: "parabolic_sar".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert("af_start".to_string(), ParamValue::from(af_start));
            p.insert("af_step".to_string(), ParamValue::from(af_step));
            p.insert("af_max".to_string(), ParamValue::from(af_max));
            p
        },
        pine_expr: Some(format!("ta.sar({}, {}, {})", af_start, af_step, af_max)),
    }];

    // Build rules
    let rules = Rules {
        entry: Rule {
            condition: "SAR flips below price (uptrend begins)".to_string(),
            pine_condition: format!(
                "sar = ta.sar({}, {}, {})\nta.crossover(close, sar)",
                af_start, af_step, af_max
            ),
            position_required: Some("flat".to_string()),
        },
        exit: Rule {
            condition: "SAR flips above price (downtrend begins)".to_string(),
            pine_condition: format!(
                "sar = ta.sar({}, {}, {})\nta.crossunder(close, sar)",
                af_start, af_step, af_max
            ),
            position_required: Some("long".to_string()),
        },
    };

    // Build parameters
    let mut parameters = HashMap::new();
    parameters.insert("af_start".to_string(), ParamValue::from(af_start));
    parameters.insert("af_step".to_string(), ParamValue::from(af_step));
    parameters.insert("af_max".to_string(), ParamValue::from(af_max));

    Ok(StrategyArtifact {
        schema_version: SCHEMA_VERSION.to_string(),
        strategy_id: "parabolic_sar".to_string(),
        strategy_version: Some("1.0.0".to_string()),
        symbol: symbol.to_string(),
        timeframe: timeframe.to_string(),
        generated_at: Some(Utc::now()),
        data_range: Some(DataRange {
            start: bars.first().unwrap().ts,
            end: bars.last().unwrap().ts,
        }),
        indicators,
        rules,
        fill_model: "NextOpen".to_string(),
        cost_model: cost_model.into(),
        parameters,
        parity_vectors: ParityVectors {
            description: Some(format!(
                "Parity vectors for Parabolic SAR({:.2}/{:.2}/{:.2}) on {} bars",
                af_start,
                af_step,
                af_max,
                bars.len()
            )),
            vectors,
        },
        metadata: None,
    })
}

// =============================================================================
// Artifact Creation Dispatcher
// =============================================================================

/// Create a strategy artifact from a config ID.
///
/// This dispatcher routes to the appropriate artifact creator based on strategy type.
/// Returns an error for strategies that don't yet have artifact support.
pub fn create_artifact_from_config(
    config: &StrategyConfigId,
    bars: &[Bar],
    cost_model: CostModel,
) -> Result<StrategyArtifact, ArtifactError> {
    // We don't need the backtest result for artifact creation since we recompute signals
    let dummy_result = BacktestResult::default();

    match config {
        StrategyConfigId::Donchian {
            entry_lookback,
            exit_lookback,
        } => create_donchian_artifact(
            bars,
            *entry_lookback,
            *exit_lookback,
            cost_model,
            &dummy_result,
        ),

        StrategyConfigId::TurtleS1 => {
            // Turtle S1: 20-day breakout, 10-day exit
            create_donchian_artifact(bars, 20, 10, cost_model, &dummy_result)
        }

        StrategyConfigId::TurtleS2 => {
            // Turtle S2: 55-day breakout, 20-day exit
            create_donchian_artifact(bars, 55, 20, cost_model, &dummy_result)
        }

        StrategyConfigId::FiftyTwoWeekHigh {
            period,
            entry_pct,
            exit_pct,
        } => create_52wk_high_artifact(bars, *period, *entry_pct, *exit_pct, cost_model),

        StrategyConfigId::Supertrend {
            atr_period,
            multiplier,
        } => create_supertrend_artifact(bars, *atr_period, *multiplier, cost_model),

        StrategyConfigId::ParabolicSar {
            af_start,
            af_step,
            af_max,
        } => create_parabolic_sar_artifact(bars, *af_start, *af_step, *af_max, cost_model),

        // Strategies without artifact support yet
        _ => Err(ArtifactError::UnsupportedStrategy(format!(
            "{:?}",
            config.strategy_type()
        ))),
    }
}

/// Export an artifact to a file.
pub fn export_artifact_to_file(
    artifact: &StrategyArtifact,
    output_dir: &Path,
    filename: &str,
) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(output_dir)?;
    let output_path = output_dir.join(format!("{}.json", filename));
    let json = serde_json::to_string_pretty(artifact)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&output_path, json)?;
    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::{run_backtest, BacktestConfig};
    use chrono::TimeZone;

    fn mk_bar(day: u32, open: f64, high: f64, low: f64, close: f64) -> Bar {
        let ts = Utc.with_ymd_and_hms(2024, 1, day, 0, 0, 0).unwrap();
        Bar::new(ts, open, high, low, close, 1000.0, "TEST", "1d")
    }

    fn sample_bars() -> Vec<Bar> {
        vec![
            mk_bar(1, 100.0, 102.0, 99.0, 101.0),
            mk_bar(2, 101.0, 103.0, 100.0, 102.0),
            mk_bar(3, 102.0, 104.0, 101.0, 103.0),
            mk_bar(4, 103.0, 105.0, 102.0, 104.0),
            mk_bar(5, 104.0, 106.0, 103.0, 105.0),
            mk_bar(6, 105.0, 110.0, 104.0, 109.0), // Breakout
            mk_bar(7, 109.0, 111.0, 108.0, 110.0),
            mk_bar(8, 110.0, 112.0, 105.0, 106.0), // Exit signal
            mk_bar(9, 106.0, 107.0, 100.0, 101.0),
            mk_bar(10, 101.0, 103.0, 100.0, 102.0),
        ]
    }

    #[test]
    fn test_create_donchian_artifact() {
        let bars = sample_bars();
        let cost_model = CostModel {
            fees_bps_per_side: 10.0,
            slippage_bps: 5.0,
        };

        let mut strategy = DonchianBreakoutStrategy::new(5, 3);
        let result = run_backtest(&bars, &mut strategy, BacktestConfig::default()).unwrap();

        let artifact = create_donchian_artifact(&bars, 5, 3, cost_model, &result).unwrap();

        assert_eq!(artifact.schema_version, "1.0.0");
        assert_eq!(artifact.strategy_id, "donchian_breakout");
        assert_eq!(artifact.symbol, "TEST");
        assert_eq!(artifact.timeframe, "1d");
        assert_eq!(artifact.fill_model, "NextOpen");
        assert_eq!(artifact.cost_model.fees_bps_per_side, 10.0);
        assert_eq!(artifact.cost_model.slippage_bps, 5.0);
        assert_eq!(artifact.indicators.len(), 2);
        assert_eq!(artifact.parity_vectors.vectors.len(), 10);
    }

    #[test]
    fn test_artifact_serialization() {
        let bars = sample_bars();
        let cost_model = CostModel::default();

        let mut strategy = DonchianBreakoutStrategy::new(5, 3);
        let result = run_backtest(&bars, &mut strategy, BacktestConfig::default()).unwrap();

        let artifact = create_donchian_artifact(&bars, 5, 3, cost_model, &result).unwrap();

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&artifact).unwrap();
        assert!(json.contains("donchian_breakout"));
        assert!(json.contains("schema_version"));

        // Deserialize back
        let parsed: StrategyArtifact = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.strategy_id, artifact.strategy_id);
        assert_eq!(parsed.indicators.len(), artifact.indicators.len());
    }

    #[test]
    fn test_parity_vectors_contain_signals() {
        let bars = sample_bars();
        let cost_model = CostModel::default();

        let mut strategy = DonchianBreakoutStrategy::new(5, 3);
        let result = run_backtest(&bars, &mut strategy, BacktestConfig::default()).unwrap();

        let artifact = create_donchian_artifact(&bars, 5, 3, cost_model, &result).unwrap();

        // Should have at least one entry signal
        let has_entry = artifact
            .parity_vectors
            .vectors
            .iter()
            .any(|v| v.signal.as_deref() == Some("enter_long"));
        assert!(has_entry, "Should have at least one entry signal");
    }

    #[test]
    fn test_builder_pattern() {
        let bars = sample_bars();

        let artifact = ArtifactBuilder::new()
            .strategy_id("test_strategy")
            .symbol("TEST")
            .timeframe("1d")
            .fill_model(FillModel::NextOpen)
            .cost_model(CostModel::default())
            .parameter("lookback", 10_i64)
            .rules(Rules {
                entry: Rule {
                    condition: "test entry".to_string(),
                    pine_condition: "close > sma".to_string(),
                    position_required: None,
                },
                exit: Rule {
                    condition: "test exit".to_string(),
                    pine_condition: "close < sma".to_string(),
                    position_required: None,
                },
            })
            .bars(bars)
            .build()
            .unwrap();

        assert_eq!(artifact.strategy_id, "test_strategy");
    }
}
