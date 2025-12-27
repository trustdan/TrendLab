/** Strategy type identifier */
export type StrategyTypeId =
  | 'donchian'
  | 'ma_crossover'
  | 'momentum'
  | 'atr_breakout'
  | 'bollinger_breakout'
  | 'dual_ma'
  | 'keltner'
  | 'supertrend'
  | 'parabolic_sar'
  | 'aroon'
  | 'dmi_adx'
  | '52wk_high'
  | 'darvas_box'
  | 'heikin_ashi'
  | 'opening_range'
  | 'starc'
  | 'larry_williams'
  | 'bollinger_squeeze'
  | 'ensemble';

/** Strategy category for grouping */
export type StrategyCategory =
  | 'breakout'
  | 'momentum'
  | 'trend_following'
  | 'mean_reversion'
  | 'volatility'
  | 'composite';

/** Parameter definition */
export interface ParameterDef {
  name: string;
  type: 'int' | 'float' | 'bool';
  default: number | boolean;
  min?: number;
  max?: number;
  step?: number;
  description?: string;
}

/** Strategy type definition */
export interface StrategyType {
  id: StrategyTypeId;
  name: string;
  category: StrategyCategory;
  description: string;
  parameters: ParameterDef[];
}

/** Parameter values for a strategy instance */
export type StrategyParams = Record<string, number | boolean>;

/** Unique config identifier (strategy + params) */
export type StrategyConfigId = string;

/** Strategy configuration (type + specific params) */
export interface StrategyConfig {
  id: StrategyConfigId;
  strategyType: StrategyTypeId;
  params: StrategyParams;
  displayName: string;
}

/** Selected strategies for sweep */
export interface StrategySelection {
  strategyType: StrategyTypeId;
  enabled: boolean;
  paramOverrides?: Partial<StrategyParams>;
}

/** Strategy grid depth for sweeps */
export type SweepDepth = 'quick' | 'normal' | 'deep' | 'exhaustive';

/** Strategy grid configuration */
export interface StrategyGrid {
  strategyType: StrategyTypeId;
  depth: SweepDepth;
  configs: StrategyConfig[];
}

/** Category with strategies */
export interface CategoryGroup {
  category: StrategyCategory;
  label: string;
  strategies: StrategyType[];
}

/** Strategy category labels */
export const CATEGORY_LABELS: Record<StrategyCategory, string> = {
  breakout: 'Breakout',
  momentum: 'Momentum',
  trend_following: 'Trend Following',
  mean_reversion: 'Mean Reversion',
  volatility: 'Volatility',
  composite: 'Composite',
};
