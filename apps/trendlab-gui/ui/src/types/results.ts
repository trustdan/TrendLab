/** Metrics for a single backtest result */
export interface ResultMetrics {
  total_return: number;
  cagr: number;
  sharpe: number;
  sortino: number;
  max_drawdown: number;
  calmar: number;
  win_rate: number;
  profit_factor: number;
  num_trades: number;
  turnover: number;
}

/** A single backtest result row for display */
export interface ResultRow {
  /** Unique identifier for this result */
  id: string;
  /** Symbol tested */
  symbol: string;
  /** Strategy type (donchian, ma_crossover, etc.) */
  strategy: string;
  /** Configuration ID string */
  config_id: string;
  /** Performance metrics */
  metrics: ResultMetrics;
  /** Equity curve (for sparkline or detail view) */
  equity_curve: number[];
}

/** View mode for results display */
export type ViewMode = 'per_ticker' | 'by_strategy' | 'all_configs';

export const VIEW_MODE_LABELS: Record<ViewMode, string> = {
  per_ticker: 'Per Ticker',
  by_strategy: 'By Strategy',
  all_configs: 'All Configs',
};

/** Metric to sort by */
export type SortMetric =
  | 'sharpe'
  | 'cagr'
  | 'sortino'
  | 'max_drawdown'
  | 'calmar'
  | 'win_rate'
  | 'profit_factor'
  | 'total_return'
  | 'num_trades';

export const SORT_METRIC_LABELS: Record<SortMetric, string> = {
  sharpe: 'Sharpe',
  cagr: 'CAGR',
  sortino: 'Sortino',
  max_drawdown: 'Max DD',
  calmar: 'Calmar',
  win_rate: 'Win Rate',
  profit_factor: 'Profit Factor',
  total_return: 'Total Return',
  num_trades: 'Trades',
};

/** Summary for a single ticker across all strategies */
export interface TickerSummary {
  symbol: string;
  configs_tested: number;
  best_strategy: string;
  best_sharpe: number;
  avg_sharpe: number;
  best_cagr: number;
  worst_drawdown: number;
}

/** Summary for a single strategy across all tickers */
export interface StrategySummary {
  strategy: string;
  tickers_tested: number;
  configs_tested: number;
  avg_sharpe: number;
  best_sharpe: number;
  avg_cagr: number;
  best_cagr: number;
  worst_drawdown: number;
}

/** Query parameters for getting results */
export interface ResultsQuery {
  view_mode?: ViewMode;
  sort_by?: SortMetric;
  ascending?: boolean;
  limit?: number;
  symbol_filter?: string;
  strategy_filter?: string;
}

/** Results state */
export interface ResultsState {
  results: ResultRow[];
  sweep_id: string | null;
  selected_id: string | null;
  view_mode: ViewMode;
  sort_by: SortMetric;
  ascending: boolean;
}
