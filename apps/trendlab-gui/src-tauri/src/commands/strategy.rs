//! Strategy panel commands for the GUI.
//!
//! Provides commands for strategy selection, parameter editing, and ensemble configuration.

use crate::error::GuiError;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Types
// ============================================================================

/// Strategy information for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct StrategyInfo {
    pub id: String,
    pub name: String,
    pub has_params: bool,
}

/// Strategy category grouping.
#[derive(Debug, Clone, Serialize)]
pub struct StrategyCategory {
    pub id: String,
    pub name: String,
    pub strategies: Vec<StrategyInfo>,
}

/// Parameter value (can be int, float, or string for enums).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ParamValue {
    Int(i64),
    Float(f64),
    String(String),
}

impl ParamValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ParamValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            ParamValue::Float(v) => Some(*v),
            ParamValue::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            ParamValue::String(v) => Some(v),
            _ => None,
        }
    }
}

/// Strategy parameter values (generic key-value map).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StrategyParamValues {
    pub values: HashMap<String, ParamValue>,
}

/// Parameter definition with constraints.
#[derive(Debug, Clone, Serialize)]
pub struct ParamDef {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub param_type: String, // "int", "float", "enum"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    pub default: ParamValue,
}

/// Strategy defaults including parameter definitions.
#[derive(Debug, Clone, Serialize)]
pub struct StrategyDefaults {
    pub strategy_id: String,
    pub params: Vec<ParamDef>,
    pub values: StrategyParamValues,
}

/// Ensemble configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleConfig {
    pub enabled: bool,
    pub voting_method: String,
}

impl Default for EnsembleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            voting_method: "majority".to_string(),
        }
    }
}

// ============================================================================
// Static Data - Strategy Categories (matching TUI exactly)
// ============================================================================

/// Get the static list of strategy categories.
fn get_categories() -> Vec<StrategyCategory> {
    vec![
        StrategyCategory {
            id: "channel".to_string(),
            name: "Channel Breakouts".to_string(),
            strategies: vec![
                StrategyInfo {
                    id: "donchian".to_string(),
                    name: "Donchian Breakout".to_string(),
                    has_params: true,
                },
                StrategyInfo {
                    id: "keltner".to_string(),
                    name: "Keltner Channel".to_string(),
                    has_params: true,
                },
                StrategyInfo {
                    id: "starc".to_string(),
                    name: "STARC Bands".to_string(),
                    has_params: true,
                },
                StrategyInfo {
                    id: "supertrend".to_string(),
                    name: "Supertrend".to_string(),
                    has_params: true,
                },
            ],
        },
        StrategyCategory {
            id: "momentum".to_string(),
            name: "Momentum/Direction".to_string(),
            strategies: vec![
                StrategyInfo {
                    id: "ma_crossover".to_string(),
                    name: "MA Crossover".to_string(),
                    has_params: true,
                },
                StrategyInfo {
                    id: "tsmom".to_string(),
                    name: "TSMOM".to_string(),
                    has_params: true,
                },
            ],
        },
        StrategyCategory {
            id: "price".to_string(),
            name: "Price Breakouts".to_string(),
            strategies: vec![StrategyInfo {
                id: "opening_range".to_string(),
                name: "Opening Range Breakout".to_string(),
                has_params: true,
            }],
        },
        StrategyCategory {
            id: "presets".to_string(),
            name: "Classic Presets".to_string(),
            strategies: vec![
                StrategyInfo {
                    id: "turtle_s1".to_string(),
                    name: "Turtle System 1".to_string(),
                    has_params: false,
                },
                StrategyInfo {
                    id: "turtle_s2".to_string(),
                    name: "Turtle System 2".to_string(),
                    has_params: false,
                },
                StrategyInfo {
                    id: "parabolic_sar".to_string(),
                    name: "Parabolic SAR".to_string(),
                    has_params: true,
                },
            ],
        },
    ]
}

/// Get parameter definitions for a strategy.
fn get_strategy_param_defs(strategy_id: &str) -> Vec<ParamDef> {
    match strategy_id {
        "donchian" => vec![
            ParamDef {
                key: "entry_lookback".to_string(),
                label: "Entry Lookback".to_string(),
                param_type: "int".to_string(),
                min: Some(5.0),
                max: Some(100.0),
                step: Some(5.0),
                options: None,
                default: ParamValue::Int(20),
            },
            ParamDef {
                key: "exit_lookback".to_string(),
                label: "Exit Lookback".to_string(),
                param_type: "int".to_string(),
                min: Some(5.0),
                max: Some(50.0),
                step: Some(5.0),
                options: None,
                default: ParamValue::Int(10),
            },
        ],
        "keltner" => vec![
            ParamDef {
                key: "ema_period".to_string(),
                label: "EMA Period".to_string(),
                param_type: "int".to_string(),
                min: Some(10.0),
                max: Some(50.0),
                step: Some(5.0),
                options: None,
                default: ParamValue::Int(20),
            },
            ParamDef {
                key: "atr_period".to_string(),
                label: "ATR Period".to_string(),
                param_type: "int".to_string(),
                min: Some(5.0),
                max: Some(20.0),
                step: Some(5.0),
                options: None,
                default: ParamValue::Int(10),
            },
            ParamDef {
                key: "multiplier".to_string(),
                label: "Multiplier".to_string(),
                param_type: "float".to_string(),
                min: Some(1.0),
                max: Some(4.0),
                step: Some(0.5),
                options: None,
                default: ParamValue::Float(2.0),
            },
        ],
        "starc" => vec![
            ParamDef {
                key: "sma_period".to_string(),
                label: "SMA Period".to_string(),
                param_type: "int".to_string(),
                min: Some(10.0),
                max: Some(50.0),
                step: Some(5.0),
                options: None,
                default: ParamValue::Int(20),
            },
            ParamDef {
                key: "atr_period".to_string(),
                label: "ATR Period".to_string(),
                param_type: "int".to_string(),
                min: Some(5.0),
                max: Some(20.0),
                step: Some(5.0),
                options: None,
                default: ParamValue::Int(15),
            },
            ParamDef {
                key: "multiplier".to_string(),
                label: "Multiplier".to_string(),
                param_type: "float".to_string(),
                min: Some(1.0),
                max: Some(4.0),
                step: Some(0.5),
                options: None,
                default: ParamValue::Float(2.0),
            },
        ],
        "supertrend" => vec![
            ParamDef {
                key: "atr_period".to_string(),
                label: "ATR Period".to_string(),
                param_type: "int".to_string(),
                min: Some(5.0),
                max: Some(20.0),
                step: Some(5.0),
                options: None,
                default: ParamValue::Int(10),
            },
            ParamDef {
                key: "multiplier".to_string(),
                label: "Multiplier".to_string(),
                param_type: "float".to_string(),
                min: Some(1.0),
                max: Some(5.0),
                step: Some(0.5),
                options: None,
                default: ParamValue::Float(3.0),
            },
        ],
        "ma_crossover" => vec![
            ParamDef {
                key: "fast_period".to_string(),
                label: "Fast Period".to_string(),
                param_type: "int".to_string(),
                min: Some(5.0),
                max: Some(50.0),
                step: Some(5.0),
                options: None,
                default: ParamValue::Int(10),
            },
            ParamDef {
                key: "slow_period".to_string(),
                label: "Slow Period".to_string(),
                param_type: "int".to_string(),
                min: Some(20.0),
                max: Some(200.0),
                step: Some(10.0),
                options: None,
                default: ParamValue::Int(50),
            },
            ParamDef {
                key: "ma_type".to_string(),
                label: "MA Type".to_string(),
                param_type: "enum".to_string(),
                min: None,
                max: None,
                step: None,
                options: Some(vec!["SMA".to_string(), "EMA".to_string()]),
                default: ParamValue::String("SMA".to_string()),
            },
        ],
        "tsmom" => vec![ParamDef {
            key: "lookback".to_string(),
            label: "Lookback".to_string(),
            param_type: "int".to_string(),
            min: Some(21.0),
            max: Some(252.0),
            step: Some(21.0),
            options: None,
            default: ParamValue::Int(252),
        }],
        "opening_range" => vec![
            ParamDef {
                key: "range_bars".to_string(),
                label: "Range Bars".to_string(),
                param_type: "int".to_string(),
                min: Some(3.0),
                max: Some(10.0),
                step: Some(1.0),
                options: None,
                default: ParamValue::Int(5),
            },
            ParamDef {
                key: "period".to_string(),
                label: "Period".to_string(),
                param_type: "enum".to_string(),
                min: None,
                max: None,
                step: None,
                options: Some(vec![
                    "Weekly".to_string(),
                    "Monthly".to_string(),
                    "Rolling".to_string(),
                ]),
                default: ParamValue::String("Weekly".to_string()),
            },
        ],
        "parabolic_sar" => vec![
            ParamDef {
                key: "af_start".to_string(),
                label: "AF Start".to_string(),
                param_type: "float".to_string(),
                min: Some(0.01),
                max: Some(0.05),
                step: Some(0.01),
                options: None,
                default: ParamValue::Float(0.02),
            },
            ParamDef {
                key: "af_step".to_string(),
                label: "AF Step".to_string(),
                param_type: "float".to_string(),
                min: Some(0.01),
                max: Some(0.05),
                step: Some(0.01),
                options: None,
                default: ParamValue::Float(0.02),
            },
            ParamDef {
                key: "af_max".to_string(),
                label: "AF Max".to_string(),
                param_type: "float".to_string(),
                min: Some(0.1),
                max: Some(0.3),
                step: Some(0.05),
                options: None,
                default: ParamValue::Float(0.20),
            },
        ],
        // Turtle presets have fixed params (not editable)
        "turtle_s1" => vec![
            ParamDef {
                key: "entry_lookback".to_string(),
                label: "Entry Lookback".to_string(),
                param_type: "int".to_string(),
                min: None,
                max: None,
                step: None,
                options: None,
                default: ParamValue::Int(20),
            },
            ParamDef {
                key: "exit_lookback".to_string(),
                label: "Exit Lookback".to_string(),
                param_type: "int".to_string(),
                min: None,
                max: None,
                step: None,
                options: None,
                default: ParamValue::Int(10),
            },
        ],
        "turtle_s2" => vec![
            ParamDef {
                key: "entry_lookback".to_string(),
                label: "Entry Lookback".to_string(),
                param_type: "int".to_string(),
                min: None,
                max: None,
                step: None,
                options: None,
                default: ParamValue::Int(55),
            },
            ParamDef {
                key: "exit_lookback".to_string(),
                label: "Exit Lookback".to_string(),
                param_type: "int".to_string(),
                min: None,
                max: None,
                step: None,
                options: None,
                default: ParamValue::Int(20),
            },
        ],
        _ => vec![],
    }
}

/// Get default parameter values for a strategy.
fn get_default_values(strategy_id: &str) -> StrategyParamValues {
    let defs = get_strategy_param_defs(strategy_id);
    let values = defs.into_iter().map(|d| (d.key, d.default)).collect();
    StrategyParamValues { values }
}

// ============================================================================
// Commands
// ============================================================================

/// Get all strategy categories with their strategies.
#[tauri::command]
pub fn get_strategy_categories() -> Vec<StrategyCategory> {
    get_categories()
}

/// Get parameter definitions and defaults for a strategy.
#[tauri::command]
pub fn get_strategy_defaults(strategy_id: String) -> Result<StrategyDefaults, GuiError> {
    let params = get_strategy_param_defs(&strategy_id);
    if params.is_empty() && !["turtle_s1", "turtle_s2"].contains(&strategy_id.as_str()) {
        return Err(GuiError::InvalidInput {
            message: format!("Unknown strategy: {}", strategy_id),
        });
    }

    let values = get_default_values(&strategy_id);

    Ok(StrategyDefaults {
        strategy_id,
        params,
        values,
    })
}

/// Get currently selected strategies.
#[tauri::command]
pub fn get_strategy_selection(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.get_selected_strategies()
}

/// Update strategy selection.
#[tauri::command]
pub fn update_strategy_selection(
    state: tauri::State<'_, AppState>,
    selected: Vec<String>,
) -> Result<(), GuiError> {
    state.set_selected_strategies(selected);
    Ok(())
}

/// Get parameter values for a strategy.
#[tauri::command]
pub fn get_strategy_params(
    state: tauri::State<'_, AppState>,
    strategy_id: String,
) -> StrategyParamValues {
    state.get_strategy_params(&strategy_id)
}

/// Update parameter values for a strategy.
#[tauri::command]
pub fn update_strategy_params(
    state: tauri::State<'_, AppState>,
    strategy_id: String,
    params: StrategyParamValues,
) -> Result<(), GuiError> {
    state.set_strategy_params(&strategy_id, params);
    Ok(())
}

/// Get ensemble configuration.
#[tauri::command]
pub fn get_ensemble_config(state: tauri::State<'_, AppState>) -> EnsembleConfig {
    state.get_ensemble_config()
}

/// Set ensemble enabled state.
#[tauri::command]
pub fn set_ensemble_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), GuiError> {
    state.set_ensemble_enabled(enabled);
    Ok(())
}
