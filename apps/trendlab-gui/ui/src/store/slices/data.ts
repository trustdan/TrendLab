import type { SliceCreator } from '../types';
import type {
  Universe,
  Sector,
  SearchResult,
  FetchProgress,
  DataViewMode,
} from '../../types';

/** Data panel state */
export interface DataSlice {
  // Universe and selection
  universe: Universe | null;
  cachedSymbols: Set<string>;
  selectedTickers: Set<string>;

  // Navigation within data panel
  viewMode: DataViewMode;
  selectedSectorIndex: number;
  selectedTickerIndex: number;

  // Search state
  searchMode: boolean;
  searchQuery: string;
  searchResults: SearchResult[];
  searchLoading: boolean;
  searchSelectedIndex: number;

  // Fetch state
  fetchProgress: FetchProgress | null;
  isFetching: boolean;
  fetchJobId: string | null;

  // Actions
  setUniverse: (universe: Universe) => void;
  setCachedSymbols: (symbols: string[]) => void;
  addCachedSymbol: (symbol: string) => void;

  // Selection actions
  toggleTicker: (ticker: string) => void;
  selectAllInSector: () => void;
  selectNone: () => void;
  setSelectedTickers: (tickers: string[]) => void;

  // Navigation actions
  setViewMode: (mode: DataViewMode) => void;
  navigateSector: (delta: number) => void;
  navigateTicker: (delta: number) => void;
  expandToTickers: () => void;
  collapseToSectors: () => void;

  // Search actions
  enterSearchMode: () => void;
  exitSearchMode: () => void;
  setSearchQuery: (query: string) => void;
  setSearchResults: (results: SearchResult[]) => void;
  setSearchLoading: (loading: boolean) => void;
  navigateSearchResult: (delta: number) => void;
  selectSearchResult: () => void;

  // Fetch actions
  setFetchProgress: (progress: FetchProgress | null) => void;
  setIsFetching: (fetching: boolean) => void;
  setFetchJobId: (jobId: string | null) => void;

  // Helpers
  getCurrentSector: () => Sector | null;
  getTickersForCurrentSector: () => string[];
  getSelectedCountForSector: (sectorId: string) => number;
}

/** Create data slice */
export const createDataSlice: SliceCreator<DataSlice> = (set, get) => ({
  // Initial state
  universe: null,
  cachedSymbols: new Set(),
  selectedTickers: new Set(),
  viewMode: 'sectors',
  selectedSectorIndex: 0,
  selectedTickerIndex: 0,
  searchMode: false,
  searchQuery: '',
  searchResults: [],
  searchLoading: false,
  searchSelectedIndex: 0,
  fetchProgress: null,
  isFetching: false,
  fetchJobId: null,

  // Universe and cache actions
  setUniverse: (universe) => set({ universe }),

  setCachedSymbols: (symbols) => set({ cachedSymbols: new Set(symbols) }),

  addCachedSymbol: (symbol) =>
    set((state) => ({
      cachedSymbols: new Set([...state.cachedSymbols, symbol]),
    })),

  // Selection actions
  toggleTicker: (ticker) =>
    set((state) => {
      const newSelected = new Set(state.selectedTickers);
      if (newSelected.has(ticker)) {
        newSelected.delete(ticker);
      } else {
        newSelected.add(ticker);
      }
      return { selectedTickers: newSelected };
    }),

  selectAllInSector: () =>
    set((state) => {
      const sector = get().getCurrentSector();
      if (!sector) return state;
      const newSelected = new Set(state.selectedTickers);
      sector.tickers.forEach((t) => newSelected.add(t));
      return { selectedTickers: newSelected };
    }),

  selectNone: () =>
    set((state) => {
      const sector = get().getCurrentSector();
      if (!sector) return { selectedTickers: new Set() };
      const newSelected = new Set(state.selectedTickers);
      sector.tickers.forEach((t) => newSelected.delete(t));
      return { selectedTickers: newSelected };
    }),

  setSelectedTickers: (tickers) => set({ selectedTickers: new Set(tickers) }),

  // Navigation actions
  setViewMode: (mode) => set({ viewMode: mode, selectedTickerIndex: 0 }),

  navigateSector: (delta) =>
    set((state) => {
      if (!state.universe) return state;
      const count = state.universe.sectors.length;
      const newIndex = (state.selectedSectorIndex + delta + count) % count;
      return { selectedSectorIndex: newIndex, selectedTickerIndex: 0 };
    }),

  navigateTicker: (delta) =>
    set((state) => {
      const tickers = get().getTickersForCurrentSector();
      if (tickers.length === 0) return state;
      const newIndex =
        (state.selectedTickerIndex + delta + tickers.length) % tickers.length;
      return { selectedTickerIndex: newIndex };
    }),

  expandToTickers: () => set({ viewMode: 'tickers', selectedTickerIndex: 0 }),

  collapseToSectors: () => set({ viewMode: 'sectors' }),

  // Search actions
  enterSearchMode: () =>
    set({
      searchMode: true,
      searchQuery: '',
      searchResults: [],
      searchSelectedIndex: 0,
    }),

  exitSearchMode: () =>
    set({
      searchMode: false,
      searchQuery: '',
      searchResults: [],
      searchSelectedIndex: 0,
    }),

  setSearchQuery: (query) => set({ searchQuery: query, searchSelectedIndex: 0 }),

  setSearchResults: (results) => set({ searchResults: results, searchLoading: false }),

  setSearchLoading: (loading) => set({ searchLoading: loading }),

  navigateSearchResult: (delta) =>
    set((state) => {
      if (state.searchResults.length === 0) return state;
      const newIndex =
        (state.searchSelectedIndex + delta + state.searchResults.length) %
        state.searchResults.length;
      return { searchSelectedIndex: newIndex };
    }),

  selectSearchResult: () => {
    const state = get();
    if (state.searchResults.length === 0) return;
    const result = state.searchResults[state.searchSelectedIndex];
    if (result) {
      // Add to selected tickers
      const newSelected = new Set(state.selectedTickers);
      newSelected.add(result.symbol);
      set({
        selectedTickers: newSelected,
        searchMode: false,
        searchQuery: '',
        searchResults: [],
      });
    }
  },

  // Fetch actions
  setFetchProgress: (progress) => set({ fetchProgress: progress }),
  setIsFetching: (fetching) => set({ isFetching: fetching }),
  setFetchJobId: (jobId) => set({ fetchJobId: jobId }),

  // Helper methods
  getCurrentSector: () => {
    const state = get();
    if (!state.universe) return null;
    return state.universe.sectors[state.selectedSectorIndex] ?? null;
  },

  getTickersForCurrentSector: () => {
    const sector = get().getCurrentSector();
    return sector?.tickers ?? [];
  },

  getSelectedCountForSector: (sectorId: string) => {
    const state = get();
    if (!state.universe) return 0;
    const sector = state.universe.sectors.find((s) => s.id === sectorId);
    if (!sector) return 0;
    return sector.tickers.filter((t) => state.selectedTickers.has(t)).length;
  },
});
