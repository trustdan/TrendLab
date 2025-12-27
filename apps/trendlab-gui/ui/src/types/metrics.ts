/** Performance metrics for a backtest result */
export interface Metrics {
  // Returns
  totalReturn: number; // Decimal (e.g., 0.25 = 25%)
  cagr: number;
  annualizedReturn: number;

  // Risk
  volatility: number;
  maxDrawdown: number;
  maxDrawdownDuration: number; // Days
  ulcerIndex: number;

  // Risk-adjusted
  sharpeRatio: number;
  sortinoRatio: number;
  calmarRatio: number;
  mar: number; // CAGR / MaxDD

  // Trade stats
  winRate: number;
  profitFactor: number;
  averageWin: number;
  averageLoss: number;
  payoffRatio: number; // avgWin / avgLoss
  expectancy: number;

  // Activity
  tradeCount: number;
  turnover: number; // Annualized
  avgHoldingPeriod: number; // Days
  exposureTime: number; // Fraction of time in market
}

/** Abbreviated metrics for table display */
export interface MetricsSummary {
  cagr: number;
  sharpe: number;
  maxDd: number;
  winRate: number;
  trades: number;
}

/** Metric column definition for sortable tables */
export interface MetricColumn {
  key: keyof Metrics | keyof MetricsSummary;
  label: string;
  format: 'percent' | 'decimal' | 'ratio' | 'integer' | 'days';
  higherIsBetter: boolean;
}

/** Pre-defined metric columns */
export const METRIC_COLUMNS: MetricColumn[] = [
  { key: 'cagr', label: 'CAGR', format: 'percent', higherIsBetter: true },
  { key: 'sharpeRatio', label: 'Sharpe', format: 'ratio', higherIsBetter: true },
  { key: 'sortinoRatio', label: 'Sortino', format: 'ratio', higherIsBetter: true },
  { key: 'maxDrawdown', label: 'Max DD', format: 'percent', higherIsBetter: false },
  { key: 'calmarRatio', label: 'Calmar', format: 'ratio', higherIsBetter: true },
  { key: 'winRate', label: 'Win Rate', format: 'percent', higherIsBetter: true },
  { key: 'profitFactor', label: 'Profit Factor', format: 'ratio', higherIsBetter: true },
  { key: 'tradeCount', label: 'Trades', format: 'integer', higherIsBetter: false },
  { key: 'volatility', label: 'Volatility', format: 'percent', higherIsBetter: false },
  { key: 'turnover', label: 'Turnover', format: 'percent', higherIsBetter: false },
];
