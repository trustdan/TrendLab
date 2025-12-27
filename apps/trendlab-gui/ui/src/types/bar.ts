/** OHLCV bar data */
export interface Bar {
  date: string; // ISO date string (YYYY-MM-DD)
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
  adj_close?: number;
}

/** Candlestick data for TradingView Lightweight Charts */
export interface CandleData {
  time: string; // YYYY-MM-DD format required by lightweight-charts
  open: number;
  high: number;
  low: number;
  close: number;
}

/** Volume bar data for TradingView Lightweight Charts */
export interface VolumeData {
  time: string;
  value: number;
  color: string;
}

/** Line chart data point */
export interface LineData {
  time: string;
  value: number;
}

/** Symbol metadata */
export interface SymbolInfo {
  ticker: string;
  name?: string;
  sector?: string;
  industry?: string;
  exchange?: string;
}

/** Date range for data operations */
export interface DateRange {
  start: string;
  end: string;
}

/** Data availability info */
export interface DataAvailability {
  ticker: string;
  startDate: string;
  endDate: string;
  barCount: number;
  lastUpdated: string;
}
