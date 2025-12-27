import { invoke } from '@tauri-apps/api/core';
import type { SliceCreator } from '../types';
import type {
  ResultRow,
  ResultMetrics,
  ViewMode,
  SortMetric,
  TickerSummary,
  StrategySummary,
  ResultsQuery,
} from '../../types';

/** Results panel state */
export interface ResultsSlice {
  // State
  results: ResultRow[];
  tickerSummaries: TickerSummary[];
  strategySummaries: StrategySummary[];
  selectedResultId: string | null;
  focusedResultIndex: number;
  resultsViewMode: ViewMode;
  sortBy: SortMetric;
  ascending: boolean;
  isLoading: boolean;
  resultsError: string | null;

  // Actions - Data Loading
  loadResults: (query?: ResultsQuery) => Promise<void>;
  loadTickerSummaries: () => Promise<void>;
  loadStrategySummaries: () => Promise<void>;
  refreshResults: () => Promise<void>;
  clearResults: () => Promise<void>;

  // Actions - Selection & Navigation
  selectResult: (id: string | null) => void;
  getSelectedResult: () => ResultRow | null;
  navigateResultUp: () => void;
  navigateResultDown: () => void;
  selectFocusedResult: () => void;
  getFocusedResultId: () => string | null;

  // Actions - View/Sort
  setResultsViewMode: (mode: ViewMode) => void;
  setSortBy: (metric: SortMetric) => void;
  toggleSortOrder: () => void;
  cycleResultsViewMode: () => void;
  cycleSortMetric: () => void;

  // Actions - Export
  exportArtifact: (resultId: string) => Promise<string>;

  // Helpers
  hasResults: () => boolean;
  getResultCount: () => number;
  getResultById: (id: string) => ResultRow | null;
}

const VIEW_MODES: ViewMode[] = ['all_configs', 'per_ticker', 'by_strategy'];
const SORT_METRICS: SortMetric[] = [
  'sharpe',
  'cagr',
  'sortino',
  'calmar',
  'max_drawdown',
  'win_rate',
  'profit_factor',
  'total_return',
  'num_trades',
];

/** Create results slice */
export const createResultsSlice: SliceCreator<ResultsSlice> = (set, get) => ({
  // Initial state
  results: [],
  tickerSummaries: [],
  strategySummaries: [],
  selectedResultId: null,
  focusedResultIndex: 0,
  resultsViewMode: 'all_configs',
  sortBy: 'sharpe',
  ascending: false,
  isLoading: false,
  resultsError: null,

  // Data loading
  loadResults: async (query?: ResultsQuery) => {
    set({ isLoading: true, resultsError: null });
    try {
      const state = get();
      const effectiveQuery: ResultsQuery = {
        view_mode: query?.view_mode ?? state.resultsViewMode,
        sort_by: query?.sort_by ?? state.sortBy,
        ascending: query?.ascending ?? state.ascending,
        limit: query?.limit,
        symbol_filter: query?.symbol_filter,
        strategy_filter: query?.strategy_filter,
      };

      const results = await invoke<ResultRow[]>('get_results', { query: effectiveQuery });
      set({ results, isLoading: false });
    } catch (err) {
      set({ resultsError: String(err), isLoading: false });
    }
  },

  loadTickerSummaries: async () => {
    try {
      const summaries = await invoke<TickerSummary[]>('get_ticker_summaries');
      set({ tickerSummaries: summaries });
    } catch (err) {
      console.error('Failed to load ticker summaries:', err);
    }
  },

  loadStrategySummaries: async () => {
    try {
      const summaries = await invoke<StrategySummary[]>('get_strategy_summaries');
      set({ strategySummaries: summaries });
    } catch (err) {
      console.error('Failed to load strategy summaries:', err);
    }
  },

  refreshResults: async () => {
    const state = get();
    await state.loadResults();
    await state.loadTickerSummaries();
    await state.loadStrategySummaries();
  },

  clearResults: async () => {
    try {
      await invoke('clear_results');
      set({
        results: [],
        tickerSummaries: [],
        strategySummaries: [],
        selectedResultId: null,
        resultsError: null,
      });
    } catch (err) {
      console.error('Failed to clear results:', err);
    }
  },

  // Selection
  selectResult: (id) => {
    set({ selectedResultId: id });
    // Sync to backend
    invoke('select_result', { resultId: id }).catch(console.error);
  },

  getSelectedResult: () => {
    const state = get();
    if (!state.selectedResultId) return null;
    return state.results.find((r) => r.id === state.selectedResultId) ?? null;
  },

  navigateResultUp: () => {
    const { results, focusedResultIndex } = get();
    if (results.length === 0) return;
    const newIndex = Math.max(0, focusedResultIndex - 1);
    set({ focusedResultIndex: newIndex });
  },

  navigateResultDown: () => {
    const { results, focusedResultIndex } = get();
    if (results.length === 0) return;
    const newIndex = Math.min(results.length - 1, focusedResultIndex + 1);
    set({ focusedResultIndex: newIndex });
  },

  selectFocusedResult: () => {
    const { results, focusedResultIndex, selectResult } = get();
    if (results.length === 0) return;
    const focusedResult = results[focusedResultIndex];
    if (focusedResult) {
      selectResult(focusedResult.id);
    }
  },

  getFocusedResultId: () => {
    const { results, focusedResultIndex } = get();
    if (results.length === 0) return null;
    return results[focusedResultIndex]?.id ?? null;
  },

  // View/Sort
  setResultsViewMode: (mode) => {
    set({ resultsViewMode: mode });
    invoke('set_view_mode', { viewMode: mode }).catch(console.error);
    get().loadResults();
  },

  setSortBy: (metric) => {
    set({ sortBy: metric });
    invoke('set_sort_config', { sortBy: metric, ascending: get().ascending }).catch(console.error);
    get().loadResults();
  },

  toggleSortOrder: () => {
    const newAscending = !get().ascending;
    set({ ascending: newAscending });
    invoke('set_sort_config', { sortBy: get().sortBy, ascending: newAscending }).catch(console.error);
    get().loadResults();
  },

  cycleResultsViewMode: () => {
    const current = get().resultsViewMode;
    const idx = VIEW_MODES.indexOf(current);
    const next = VIEW_MODES[(idx + 1) % VIEW_MODES.length];
    get().setResultsViewMode(next);
  },

  cycleSortMetric: () => {
    const current = get().sortBy;
    const idx = SORT_METRICS.indexOf(current);
    const next = SORT_METRICS[(idx + 1) % SORT_METRICS.length];
    get().setSortBy(next);
  },

  // Export
  exportArtifact: async (resultId) => {
    const path = await invoke<string>('export_artifact', { resultId });
    return path;
  },

  // Helpers
  hasResults: () => get().results.length > 0,
  getResultCount: () => get().results.length,
  getResultById: (id) => get().results.find((r) => r.id === id) ?? null,
});
