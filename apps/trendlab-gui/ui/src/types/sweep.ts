// ============================================================================
// Sweep Types - Mirrors Rust backend types (commands/sweep.rs)
// ============================================================================

import type { MetricsSummary } from './metrics';
import type { StrategyConfigId, StrategyTypeId } from './strategy';

// Import DateRange from bar.ts (canonical source)
import type { DateRange } from './bar';

// ============================================================================
// Sweep Depth
// ============================================================================

/** Sweep depth level (controls parameter grid density) */
export type SweepDepth = 'quick' | 'normal' | 'deep' | 'exhaustive';

/** Depth option for selector (from backend) */
export interface DepthOption {
  id: SweepDepth;
  name: string;
  description: string;
  estimated_configs: number;
}

// ============================================================================
// Cost Model
// ============================================================================

/** Cost model configuration */
export interface CostModel {
  fees_bps: number;
  slippage_bps: number;
}

/** Default cost model */
export const DEFAULT_COST_MODEL: CostModel = {
  fees_bps: 5.0,
  slippage_bps: 5.0,
};

// ============================================================================
// Sweep State & Config
// ============================================================================

/** Sweep state from backend */
export interface SweepState {
  depth: SweepDepth;
  cost_model: CostModel;
  date_range: DateRange;
  is_running: boolean;
  current_job_id?: string | null;
}

/** Selection summary for display */
export interface SelectionSummary {
  symbols: string[];
  strategies: string[];
  symbol_count: number;
  strategy_count: number;
  estimated_configs: number;
  has_cached_data: boolean;
}

/** Start sweep response */
export interface StartSweepResponse {
  job_id: string;
  total_configs: number;
}

// ============================================================================
// Sweep Events
// ============================================================================

/** Sweep progress event payload (worker events may omit job_id/config_id) */
export interface SweepProgressPayload {
  job_id?: string;
  current: number;
  total: number;
  symbol: string;
  strategy: string;
  config_id?: string;
  message: string;
}

/** Sweep complete event payload (worker events may omit details) */
export interface SweepCompletePayload {
  job_id?: string;
  total_configs?: number;
  successful?: number;
  failed?: number;
  elapsed_ms?: number;
}

/** Sweep cancelled event payload */
export interface SweepCancelledPayload {
  job_id: string;
  completed: number;
}

// ============================================================================
// Sweep Results (for Results panel - kept from original)
// ============================================================================

/** Sweep progress event payload (legacy format) */
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
  dateRange: DateRange;
  costModel: CostModel;
}
