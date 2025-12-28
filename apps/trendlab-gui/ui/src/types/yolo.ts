/**
 * YOLO Mode Types - Auto-optimization with leaderboard
 */

/**
 * Phase of YOLO mode operation
 */
export type YoloPhase = 'idle' | 'sweeping' | 'stopped';

/**
 * Leaderboard scope for viewing session vs all-time results
 */
export type LeaderboardScope = 'session' | 'all_time';

/**
 * Entry in the per-symbol leaderboard
 */
export interface LeaderboardEntry {
  rank: number;
  strategyType: string;
  configId: string;
  symbol: string | null;
  sharpe: number;
  cagr: number;
  maxDrawdown: number;
  discoveredAt: string;
  iteration: number;
  equityCurve?: number[];
  dates?: string[];
  confidenceGrade?: ConfidenceGrade | null;

  // Walk-Forward Validation Fields
  /** Walk-forward grade (A-F) for robustness assessment */
  walkForwardGrade?: string | null;
  /** Mean out-of-sample Sharpe ratio across folds */
  meanOosSharpe?: number | null;
  /** Sharpe degradation ratio: OOS/IS (1.0 = good generalization) */
  sharpeDegradation?: number | null;
  /** Percentage of folds with positive OOS Sharpe */
  pctProfitableFolds?: number | null;
  /** P-value from one-sided test of OOS Sharpe > 0 */
  oosPValue?: number | null;
  /** FDR-adjusted p-value after Benjamini-Hochberg correction */
  fdrAdjustedPValue?: number | null;
}

/**
 * Per-symbol leaderboard (top N strategies for individual symbols)
 */
export interface Leaderboard {
  entries: LeaderboardEntry[];
  maxEntries: number;
  totalIterations: number;
  startedAt: string;
  lastUpdated: string;
  totalConfigsTested: number;
}

/**
 * Aggregated metrics across multiple symbols
 */
export interface AggregatedMetrics {
  avgSharpe: number;
  minSharpe: number;
  maxSharpe: number;
  geoMeanCagr: number;
  avgCagr: number;
  worstMaxDrawdown: number;
  avgMaxDrawdown: number;
  hitRate: number;
  profitableCount: number;
  totalSymbols: number;
  avgTrades: number;
}

/**
 * Entry in the cross-symbol leaderboard (aggregated across all symbols)
 */
export interface AggregatedConfigResult {
  rank: number;
  strategyType: string;
  configId: string;
  symbols: string[];
  perSymbolMetrics: Record<string, { sharpe: number; cagr: number; maxDrawdown: number }>;
  aggregateMetrics: AggregatedMetrics;
  combinedEquityCurve?: number[];
  dates?: string[];
  discoveredAt: string;
  iteration: number;
  confidenceGrade?: ConfidenceGrade | null;

  // Walk-Forward Validation Fields
  /** Walk-forward grade (A-F) aggregated across symbols */
  walkForwardGrade?: string | null;
  /** Mean out-of-sample Sharpe ratio across all symbols and folds */
  meanOosSharpe?: number | null;
  /** Standard deviation of OOS Sharpe across folds */
  stdOosSharpe?: number | null;
  /** Sharpe degradation ratio: mean_oos / mean_is */
  sharpeDegradation?: number | null;
  /** Percentage of folds with positive OOS Sharpe */
  pctProfitableFolds?: number | null;
  /** P-value from one-sided test of mean OOS Sharpe > 0 */
  oosPValue?: number | null;
  /** FDR-adjusted p-value after Benjamini-Hochberg correction */
  fdrAdjustedPValue?: number | null;
}

/**
 * Cross-symbol leaderboard (primary for multi-symbol runs)
 */
export interface CrossSymbolLeaderboard {
  entries: AggregatedConfigResult[];
  maxEntries: number;
  rankBy: CrossSymbolRankMetric;
  totalIterations: number;
  startedAt: string;
  lastUpdated: string;
  totalConfigsTested: number;
}

/**
 * Ranking metric for cross-symbol leaderboard
 */
export type CrossSymbolRankMetric = 'avgSharpe' | 'minSharpe' | 'geoMeanCagr' | 'hitRate' | 'meanOosSharpe';

/**
 * Statistical confidence grade for strategy results
 */
export type ConfidenceGrade = 'High' | 'Medium' | 'Low' | 'Insufficient';

/**
 * YOLO mode state
 */
export interface YoloState {
  enabled: boolean;
  phase: YoloPhase;
  iteration: number;
  randomizationPct: number;
  totalConfigsTested: number;
  startedAt: string | null;
  leaderboard: Leaderboard | null;
  crossSymbolLeaderboard: CrossSymbolLeaderboard | null;
  currentJobId: string | null;
  completedConfigs: number;
  totalConfigs: number;
}

/**
 * Default YOLO state
 */
export const DEFAULT_YOLO_STATE: YoloState = {
  enabled: false,
  phase: 'idle',
  iteration: 0,
  randomizationPct: 0.15,
  totalConfigsTested: 0,
  startedAt: null,
  leaderboard: null,
  crossSymbolLeaderboard: null,
  currentJobId: null,
  completedConfigs: 0,
  totalConfigs: 0,
};
