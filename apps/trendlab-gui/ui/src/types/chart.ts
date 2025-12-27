// Chart types for TradingView Lightweight Charts integration
// These types use Unix timestamps (seconds) for chart rendering

/** Chart display mode */
export type ChartMode =
  | 'candlestick'
  | 'equity'
  | 'multi_ticker'
  | 'portfolio'
  | 'strategy_comparison';

/** Display labels for chart modes */
export const CHART_MODE_LABELS: Record<ChartMode, string> = {
  candlestick: 'Candlestick',
  equity: 'Equity Curve',
  multi_ticker: 'Multi-Ticker',
  portfolio: 'Portfolio',
  strategy_comparison: 'Strategy Comparison',
};

/** OHLCV candlestick bar for chart rendering (Unix timestamp) */
export interface ChartCandleData {
  /** Unix timestamp in seconds */
  time: number;
  /** Open price */
  open: number;
  /** High price */
  high: number;
  /** Low price */
  low: number;
  /** Close price */
  close: number;
  /** Volume */
  volume: number;
}

/** Single point on an equity curve (Unix timestamp) */
export interface ChartEquityPoint {
  /** Unix timestamp in seconds */
  time: number;
  /** Equity value */
  value: number;
}

/** Drawdown point for overlay */
export interface DrawdownPoint {
  /** Unix timestamp in seconds */
  time: number;
  /** Drawdown as decimal (e.g., -0.15 for -15%) */
  drawdown: number;
}

/** Trade marker for chart display */
export interface TradeMarker {
  /** Unix timestamp in seconds */
  time: number;
  /** Price at entry/exit */
  price: number;
  /** "entry" or "exit" */
  markerType: 'entry' | 'exit';
  /** "long" or "short" */
  direction: 'long' | 'short';
  /** PnL for exit markers */
  pnl?: number;
  /** Tooltip text */
  text: string;
}

/** Named equity curve for multi-series charts */
export interface NamedEquityCurve {
  /** Series identifier (ticker symbol or strategy name) */
  name: string;
  /** Display color (hex) */
  color: string;
  /** Equity points */
  data: ChartEquityPoint[];
}

/** Complete chart data response */
export interface ChartData {
  /** Candlestick data (for Candlestick mode) */
  candles?: ChartCandleData[];
  /** Primary equity curve */
  equity?: ChartEquityPoint[];
  /** Multiple equity curves (for MultiTicker/StrategyComparison) */
  curves?: NamedEquityCurve[];
  /** Drawdown overlay */
  drawdown?: DrawdownPoint[];
  /** Trade markers */
  trades?: TradeMarker[];
}

/** Chart overlay options */
export interface ChartOverlays {
  /** Show drawdown overlay */
  drawdown: boolean;
  /** Show volume subplot */
  volume: boolean;
  /** Show trade markers */
  trades: boolean;
  /** Show crosshair */
  crosshair: boolean;
}

/** Default overlay settings */
export const DEFAULT_OVERLAYS: ChartOverlays = {
  drawdown: false,
  volume: true,
  trades: true,
  crosshair: true,
};

/** Chart state */
export interface ChartState {
  /** Current chart mode */
  mode: ChartMode;
  /** Current symbol (for Candlestick/Equity modes) */
  symbol?: string;
  /** Current strategy (for Equity mode) */
  strategy?: string;
  /** Current config ID (for Equity mode) */
  configId?: string;
  /** Overlay visibility */
  overlays: ChartOverlays;
}

/** Default chart state */
export const DEFAULT_CHART_STATE: ChartState = {
  mode: 'candlestick',
  overlays: DEFAULT_OVERLAYS,
};

/** Tokyo Night color palette for charts */
export const CHART_COLORS = {
  // Series colors
  blue: '#7aa2f7',
  green: '#9ece6a',
  red: '#f7768e',
  orange: '#e0af68',
  purple: '#bb9af7',
  cyan: '#73daca',
  peach: '#ff9e64',
  teal: '#2ac3de',

  // UI colors
  background: '#1a1b26',
  foreground: '#c0caf5',
  gridLines: '#292e42',
  borderColor: '#3b4261',
  textColor: '#a9b1d6',

  // Candlestick colors
  upColor: '#9ece6a',
  downColor: '#f7768e',
  wickUpColor: '#9ece6a',
  wickDownColor: '#f7768e',

  // Volume colors
  volumeUp: 'rgba(158, 206, 106, 0.3)',
  volumeDown: 'rgba(247, 118, 142, 0.3)',

  // Drawdown
  drawdownLine: '#f7768e',
  drawdownFill: 'rgba(247, 118, 142, 0.1)',

  // Crosshair
  crosshairLine: '#7aa2f7',
  crosshairLabel: '#1a1b26',
  crosshairLabelBg: '#7aa2f7',
} as const;

/** Series color palette for multi-series charts */
export const SERIES_COLORS = [
  CHART_COLORS.blue,
  CHART_COLORS.green,
  CHART_COLORS.red,
  CHART_COLORS.orange,
  CHART_COLORS.purple,
  CHART_COLORS.cyan,
  CHART_COLORS.peach,
  CHART_COLORS.teal,
];
