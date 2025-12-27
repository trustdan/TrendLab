// ============================================================================
// Strategy Types - Mirrors Rust backend types
// ============================================================================

import type { SweepDepth } from './sweep';

/** Strategy type identifier (matching TUI exactly) */
export type StrategyTypeId =
  | 'donchian'
  | 'keltner'
  | 'starc'
  | 'supertrend'
  | 'ma_crossover'
  | 'tsmom'
  | 'opening_range'
  | 'turtle_s1'
  | 'turtle_s2'
  | 'parabolic_sar';

/** Strategy category ID */
export type StrategyCategoryId = 'channel' | 'momentum' | 'price' | 'presets';

// ============================================================================
// Backend Response Types (from Tauri commands)
// ============================================================================

/** Strategy info from backend */
export interface StrategyInfo {
  id: string;
  name: string;
  has_params: boolean;
}

/** Strategy category from backend */
export interface StrategyCategory {
  id: string;
  name: string;
  strategies: StrategyInfo[];
}

/** Parameter value (int, float, or enum string) */
export type ParamValue = number | string;

/** Strategy parameter values (generic key-value map) */
export interface StrategyParamValues {
  values: Record<string, ParamValue>;
}

/** Parameter definition with constraints */
export interface ParamDef {
  key: string;
  label: string;
  type: 'int' | 'float' | 'enum';
  min?: number;
  max?: number;
  step?: number;
  options?: string[];
  default: ParamValue;
}

/** Strategy defaults response */
export interface StrategyDefaults {
  strategy_id: string;
  params: ParamDef[];
  values: StrategyParamValues;
}

/** Ensemble configuration */
export interface EnsembleConfig {
  enabled: boolean;
  voting_method: 'majority' | 'weighted' | 'unanimous';
}

// ============================================================================
// UI State Types
// ============================================================================

/** Focus mode for strategy panel */
export type StrategyFocus = 'selection' | 'parameters';

/** Navigation item (category or strategy) */
export interface NavigationItem {
  type: 'category' | 'strategy';
  categoryId: string;
  strategyId?: string;
}

// ============================================================================
// Static Data - Strategy Categories (matching TUI exactly)
// ============================================================================

/** Default strategy categories (hardcoded for fast initial render) */
export const STRATEGY_CATEGORIES: StrategyCategory[] = [
  {
    id: 'channel',
    name: 'Channel Breakouts',
    strategies: [
      { id: 'donchian', name: 'Donchian Breakout', has_params: true },
      { id: 'keltner', name: 'Keltner Channel', has_params: true },
      { id: 'starc', name: 'STARC Bands', has_params: true },
      { id: 'supertrend', name: 'Supertrend', has_params: true },
    ],
  },
  {
    id: 'momentum',
    name: 'Momentum/Direction',
    strategies: [
      { id: 'ma_crossover', name: 'MA Crossover', has_params: true },
      { id: 'tsmom', name: 'TSMOM', has_params: true },
    ],
  },
  {
    id: 'price',
    name: 'Price Breakouts',
    strategies: [
      { id: 'opening_range', name: 'Opening Range Breakout', has_params: true },
    ],
  },
  {
    id: 'presets',
    name: 'Classic Presets',
    strategies: [
      { id: 'turtle_s1', name: 'Turtle System 1', has_params: false },
      { id: 'turtle_s2', name: 'Turtle System 2', has_params: false },
      { id: 'parabolic_sar', name: 'Parabolic SAR', has_params: true },
    ],
  },
];

/** Get all strategies as a flat list */
export function getAllStrategies(): StrategyInfo[] {
  return STRATEGY_CATEGORIES.flatMap((c) => c.strategies);
}

/** Get total strategy count */
export function getStrategyCount(): number {
  return STRATEGY_CATEGORIES.reduce((sum, c) => sum + c.strategies.length, 0);
}

/** Find strategy by ID */
export function findStrategy(strategyId: string): StrategyInfo | undefined {
  for (const category of STRATEGY_CATEGORIES) {
    const strategy = category.strategies.find((s) => s.id === strategyId);
    if (strategy) return strategy;
  }
  return undefined;
}

/** Find category by ID */
export function findCategory(categoryId: string): StrategyCategory | undefined {
  return STRATEGY_CATEGORIES.find((c) => c.id === categoryId);
}

/** Get category for a strategy */
export function getCategoryForStrategy(strategyId: string): StrategyCategory | undefined {
  return STRATEGY_CATEGORIES.find((c) =>
    c.strategies.some((s) => s.id === strategyId)
  );
}

// ============================================================================
// Legacy Types (kept for compatibility with other panels)
// ============================================================================

/** Unique config identifier (strategy + params) */
export type StrategyConfigId = string;

/** Strategy configuration (type + specific params) */
export interface StrategyConfig {
  id: string;
  strategyType: StrategyTypeId;
  params: Record<string, ParamValue>;
  displayName: string;
}

/** Strategy grid configuration */
export interface StrategyGrid {
  strategyType: StrategyTypeId;
  depth: SweepDepth;
  configs: StrategyConfig[];
}
