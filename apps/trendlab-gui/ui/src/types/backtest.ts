import type { Metrics } from './metrics';
import type { StrategyConfigId, StrategyTypeId } from './strategy';

/** Trade direction */
export type Direction = 'long' | 'short';

/** Trade entry/exit */
export interface Trade {
  entryDate: string;
  entryPrice: number;
  exitDate: string;
  exitPrice: number;
  direction: Direction;
  pnl: number;
  pnlPercent: number;
  holdingDays: number;
}

/** Fill for trade execution */
export interface Fill {
  date: string;
  price: number;
  quantity: number;
  side: 'buy' | 'sell';
  fees: number;
}

/** Point on equity curve */
export interface EquityPoint {
  date: string;
  equity: number;
  drawdown: number;
  benchmark?: number;
}

/** Backtest configuration */
export interface BacktestConfig {
  ticker: string;
  strategyType: StrategyTypeId;
  configId: StrategyConfigId;
  startDate: string;
  endDate: string;
  initialCapital: number;
  feesBps: number;
  slippageBps: number;
  fillModel: 'next_open' | 'same_close';
}

/** Complete backtest result */
export interface BacktestResult {
  config: BacktestConfig;
  metrics: Metrics;
  trades: Trade[];
  equityCurve: EquityPoint[];
}

/** Cost model configuration */
export interface CostModel {
  feesBps: number;
  slippageBps: number;
}

/** Default cost model */
export const DEFAULT_COST_MODEL: CostModel = {
  feesBps: 10,
  slippageBps: 5,
};
