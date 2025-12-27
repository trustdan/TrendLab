import type { MetricsSummary } from './metrics';
import type { StrategyConfigId, StrategyTypeId, SweepDepth } from './strategy';

/** Sweep progress event payload */
export interface SweepProgress {
  current: number;
  total: number;
  ticker: string;
  strategy: StrategyTypeId;
  configId: StrategyConfigId;
  message: string;
}

/** Single sweep result row */
export interface SweepResultRow {
  ticker: string;
  strategyType: StrategyTypeId;
  configId: StrategyConfigId;
  displayName: string;
  metrics: MetricsSummary;
  rank: number;
}

/** Sweep result for one symbol */
export interface SymbolSweepResult {
  ticker: string;
  results: SweepResultRow[];
  bestConfig: StrategyConfigId;
  timestamp: string;
}

/** Multi-symbol sweep result */
export interface MultiSweepResult {
  symbols: string[];
  strategies: StrategyTypeId[];
  depth: SweepDepth;
  results: SymbolSweepResult[];
  startTime: string;
  endTime: string;
  totalConfigs: number;
}

/** View mode for results display */
export type ResultsViewMode = 'per_ticker' | 'by_strategy' | 'all_configs';

/** Sort configuration */
export interface SortConfig {
  column: string;
  ascending: boolean;
}

/** Filter configuration for results */
export interface ResultsFilter {
  minSharpe?: number;
  minCagr?: number;
  maxDrawdown?: number;
  strategies?: StrategyTypeId[];
  tickers?: string[];
}

/** Sweep job configuration */
export interface SweepJobConfig {
  symbols: string[];
  strategies: StrategyTypeId[];
  depth: SweepDepth;
  dateRange: {
    start: string;
    end: string;
  };
  costModel: {
    feesBps: number;
    slippageBps: number;
  };
}
