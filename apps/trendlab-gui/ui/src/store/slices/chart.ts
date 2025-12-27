import { invoke } from '@tauri-apps/api/core';
import type { SliceCreator } from '../types';
import type {
  ChartMode,
  ChartState,
  ChartData,
  ChartOverlays,
  ChartCandleData,
  ChartEquityPoint,
  DrawdownPoint,
  NamedEquityCurve,
  TradeMarker,
} from '../../types';
import { DEFAULT_CHART_STATE, DEFAULT_OVERLAYS } from '../../types';

/** Available chart modes in order */
const CHART_MODES: ChartMode[] = [
  'candlestick',
  'equity',
  'multi_ticker',
  'portfolio',
  'strategy_comparison',
];

/** Chart slice state */
export interface ChartSlice {
  // State
  chartMode: ChartMode;
  chartSymbol: string | null;
  chartStrategy: string | null;
  chartConfigId: string | null;
  chartOverlays: ChartOverlays;
  chartData: ChartData | null;
  chartLoading: boolean;
  chartError: string | null;

  // Actions
  setChartMode: (mode: ChartMode) => Promise<void>;
  cycleChartMode: () => Promise<void>;
  setChartSelection: (
    symbol: string | null,
    strategy: string | null,
    configId: string | null
  ) => Promise<void>;
  toggleOverlay: (overlay: keyof ChartOverlays) => Promise<void>;
  loadChartState: () => Promise<void>;
  loadChartData: () => Promise<void>;
  loadCandleData: (symbol: string) => Promise<ChartCandleData[]>;
  loadEquityCurve: (resultId: string) => Promise<ChartEquityPoint[]>;
  loadDrawdownCurve: (resultId: string) => Promise<DrawdownPoint[]>;
  loadMultiTickerCurves: () => Promise<NamedEquityCurve[]>;
  loadPortfolioCurve: () => Promise<ChartEquityPoint[]>;
  loadStrategyCurves: () => Promise<NamedEquityCurve[]>;
  loadTrades: (resultId: string) => Promise<TradeMarker[]>;
  clearChartError: () => void;
}

export const createChartSlice: SliceCreator<ChartSlice> = (set, get) => ({
  // Initial state
  chartMode: DEFAULT_CHART_STATE.mode,
  chartSymbol: null,
  chartStrategy: null,
  chartConfigId: null,
  chartOverlays: { ...DEFAULT_OVERLAYS },
  chartData: null,
  chartLoading: false,
  chartError: null,

  // Actions
  setChartMode: async (mode: ChartMode) => {
    try {
      await invoke('set_chart_mode', { mode });
      set({ chartMode: mode });
      // Reload chart data for new mode
      await get().loadChartData();
    } catch (error) {
      set({ chartError: String(error) });
    }
  },

  cycleChartMode: async () => {
    const current = get().chartMode;
    const idx = CHART_MODES.indexOf(current);
    const next = CHART_MODES[(idx + 1) % CHART_MODES.length];
    await get().setChartMode(next);
  },

  setChartSelection: async (
    symbol: string | null,
    strategy: string | null,
    configId: string | null
  ) => {
    try {
      await invoke('set_chart_selection', { symbol, strategy, configId });
      set({
        chartSymbol: symbol,
        chartStrategy: strategy,
        chartConfigId: configId,
      });
      // Reload chart data for new selection
      await get().loadChartData();
    } catch (error) {
      set({ chartError: String(error) });
    }
  },

  toggleOverlay: async (overlay: keyof ChartOverlays) => {
    const currentOverlays = get().chartOverlays;
    const newValue = !currentOverlays[overlay];

    try {
      await invoke('toggle_overlay', { overlay, enabled: newValue });
      set({
        chartOverlays: {
          ...currentOverlays,
          [overlay]: newValue,
        },
      });
      // Reload chart data if drawdown or trades toggled
      if (overlay === 'drawdown' || overlay === 'trades') {
        await get().loadChartData();
      }
    } catch (error) {
      set({ chartError: String(error) });
    }
  },

  loadChartState: async () => {
    try {
      const state = await invoke<ChartState>('get_chart_state');
      set({
        chartMode: state.mode,
        chartSymbol: state.symbol ?? null,
        chartStrategy: state.strategy ?? null,
        chartConfigId: state.configId ?? null,
        chartOverlays: state.overlays,
      });
    } catch (error) {
      set({ chartError: String(error) });
    }
  },

  loadChartData: async () => {
    set({ chartLoading: true, chartError: null });

    try {
      const data = await invoke<ChartData>('get_chart_data');
      set({ chartData: data, chartLoading: false });
    } catch (error) {
      set({ chartError: String(error), chartLoading: false, chartData: null });
    }
  },

  loadCandleData: async (symbol: string) => {
    try {
      return await invoke<ChartCandleData[]>('get_candle_data', { symbol });
    } catch (error) {
      set({ chartError: String(error) });
      return [];
    }
  },

  loadEquityCurve: async (resultId: string) => {
    try {
      return await invoke<ChartEquityPoint[]>('get_equity_curve', { resultId });
    } catch (error) {
      set({ chartError: String(error) });
      return [];
    }
  },

  loadDrawdownCurve: async (resultId: string) => {
    try {
      return await invoke<DrawdownPoint[]>('get_drawdown_curve', { resultId });
    } catch (error) {
      set({ chartError: String(error) });
      return [];
    }
  },

  loadMultiTickerCurves: async () => {
    try {
      return await invoke<NamedEquityCurve[]>('get_multi_ticker_curves');
    } catch (error) {
      set({ chartError: String(error) });
      return [];
    }
  },

  loadPortfolioCurve: async () => {
    try {
      return await invoke<ChartEquityPoint[]>('get_portfolio_curve');
    } catch (error) {
      set({ chartError: String(error) });
      return [];
    }
  },

  loadStrategyCurves: async () => {
    try {
      return await invoke<NamedEquityCurve[]>('get_strategy_curves');
    } catch (error) {
      set({ chartError: String(error) });
      return [];
    }
  },

  loadTrades: async (resultId: string) => {
    try {
      return await invoke<TradeMarker[]>('get_trades', { resultId });
    } catch (error) {
      set({ chartError: String(error) });
      return [];
    }
  },

  clearChartError: () => {
    set({ chartError: null });
  },
});
