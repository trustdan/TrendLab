// Data panel types

export interface Sector {
  id: string;
  name: string;
  tickers: string[];
}

export interface Universe {
  name: string;
  description: string;
  sectors: Sector[];
}

export interface SearchResult {
  symbol: string;
  name: string;
  exchange: string;
  type_disp: string;
}

/** Progress update for data fetch operations */
export interface FetchProgress {
  symbol: string;
  current: number;
  total: number;
  message: string;
}

export interface FetchComplete {
  symbols_fetched: number;
  symbols_failed: number;
  message: string;
}

export interface StartJobResponse {
  job_id: string;
}

export type DataViewMode = 'sectors' | 'tickers';
