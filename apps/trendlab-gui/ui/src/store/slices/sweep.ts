import type { SliceCreator } from '../types';
import { invoke } from '@tauri-apps/api/core';
import type {
  SweepDepth,
  CostModel,
  SelectionSummary,
  DepthOption,
  SweepState,
  StartSweepResponse,
  SweepProgressPayload,
  SweepCompletePayload,
} from '../../types/sweep';
import type { DateRange } from '../../types';

// ============================================================================
// Types
// ============================================================================

/** Focus area within the sweep panel */
export type SweepFocus = 'summary' | 'depth' | 'cost' | 'dates' | 'controls';

/** Sweep slice state */
export interface SweepSlice {
  // State from backend
  selectionSummary: SelectionSummary | null;
  depthOptions: DepthOption[];
  depth: SweepDepth;
  costModel: CostModel;
  dateRange: DateRange;
  isRunning: boolean;
  currentJobId: string | null;

  // Progress state
  progress: {
    current: number;
    total: number;
    message: string;
    symbol: string;
    strategy: string;
  } | null;

  // UI state
  sweepFocus: SweepFocus;
  focusedDepthIndex: number;

  // Loading state
  loading: boolean;
  error: string | null;

  // Actions - Data loading
  loadSweepState: () => Promise<void>;
  loadSelectionSummary: () => Promise<void>;
  loadDepthOptions: () => Promise<void>;

  // Actions - Settings
  setDepth: (depth: SweepDepth) => Promise<void>;
  setCostModel: (costModel: CostModel) => Promise<void>;
  setDateRange: (dateRange: DateRange) => Promise<void>;

  // Actions - Sweep control
  startSweep: () => Promise<StartSweepResponse | null>;
  cancelSweep: () => Promise<void>;

  // Actions - Progress updates (from events)
  updateProgress: (payload: SweepProgressPayload) => void;
  handleComplete: (payload: SweepCompletePayload) => void;
  handleCancelled: () => void;

  // Actions - UI
  setSweepFocus: (focus: SweepFocus) => void;
  setFocusedDepthIndex: (index: number) => void;
  moveFocus: (direction: 'up' | 'down') => void;
  selectFocusedDepth: () => void;
}

// ============================================================================
// Default State
// ============================================================================

const getDefaultDateRange = (): DateRange => {
  const end = new Date().toISOString().split('T')[0];
  const start = new Date(Date.now() - 365 * 10 * 24 * 60 * 60 * 1000)
    .toISOString()
    .split('T')[0];
  return { start, end };
};

// ============================================================================
// Slice Creator
// ============================================================================

export const createSweepSlice: SliceCreator<SweepSlice> = (set, get) => ({
  // Initial state
  selectionSummary: null,
  depthOptions: [],
  depth: 'normal',
  costModel: { fees_bps: 5.0, slippage_bps: 5.0 },
  dateRange: getDefaultDateRange(),
  isRunning: false,
  currentJobId: null,
  progress: null,
  sweepFocus: 'summary',
  focusedDepthIndex: 1, // Normal depth
  loading: false,
  error: null,

  // Data loading
  loadSweepState: async () => {
    set({ loading: true, error: null });
    try {
      const state = await invoke<SweepState>('get_sweep_state');
      set({
        depth: state.depth,
        costModel: state.cost_model,
        dateRange: state.date_range,
        isRunning: state.is_running,
        currentJobId: state.current_job_id,
        loading: false,
      });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  loadSelectionSummary: async () => {
    try {
      const summary = await invoke<SelectionSummary>('get_selection_summary');
      set({ selectionSummary: summary });
    } catch (e) {
      console.error('Failed to load selection summary:', e);
    }
  },

  loadDepthOptions: async () => {
    try {
      const options = await invoke<DepthOption[]>('get_depth_options');
      set({ depthOptions: options });
      // Update focused index based on current depth
      const { depth } = get();
      const idx = options.findIndex((o) => o.id === depth);
      if (idx >= 0) {
        set({ focusedDepthIndex: idx });
      }
    } catch (e) {
      console.error('Failed to load depth options:', e);
    }
  },

  // Settings
  setDepth: async (depth) => {
    try {
      await invoke('set_sweep_depth', { depth });
      set({ depth });
      // Reload options to update estimated configs
      get().loadDepthOptions();
      get().loadSelectionSummary();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setCostModel: async (costModel) => {
    try {
      await invoke('set_cost_model', { costModel });
      set({ costModel });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setDateRange: async (dateRange) => {
    try {
      await invoke('set_date_range', { dateRange });
      set({ dateRange });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Sweep control
  startSweep: async () => {
    set({ error: null });
    try {
      const response = await invoke<StartSweepResponse>('start_sweep');
      set({
        isRunning: true,
        currentJobId: response.job_id,
        progress: {
          current: 0,
          total: response.total_configs,
          message: 'Starting sweep...',
          symbol: '',
          strategy: '',
        },
      });
      return response;
    } catch (e) {
      set({ error: String(e) });
      return null;
    }
  },

  cancelSweep: async () => {
    try {
      await invoke('cancel_sweep');
    } catch (e) {
      console.error('Failed to cancel sweep:', e);
    }
  },

  // Progress updates
  updateProgress: (payload) => {
    set({
      progress: {
        current: payload.current,
        total: payload.total,
        message: payload.message,
        symbol: payload.symbol,
        strategy: payload.strategy,
      },
    });
  },

  handleComplete: (_payload) => {
    set({
      isRunning: false,
      currentJobId: null,
      progress: null,
    });
  },

  handleCancelled: () => {
    set({
      isRunning: false,
      currentJobId: null,
      progress: null,
    });
  },

  // UI actions
  setSweepFocus: (focus) => set({ sweepFocus: focus }),

  setFocusedDepthIndex: (index) => set({ focusedDepthIndex: index }),

  moveFocus: (direction) => {
    const { sweepFocus, depthOptions, focusedDepthIndex } = get();
    const focusOrder: SweepFocus[] = ['summary', 'depth', 'cost', 'dates', 'controls'];
    const currentIndex = focusOrder.indexOf(sweepFocus);

    if (sweepFocus === 'depth') {
      // Navigate within depth options
      if (direction === 'up' && focusedDepthIndex > 0) {
        set({ focusedDepthIndex: focusedDepthIndex - 1 });
      } else if (direction === 'down' && focusedDepthIndex < depthOptions.length - 1) {
        set({ focusedDepthIndex: focusedDepthIndex + 1 });
      } else if (direction === 'up' && focusedDepthIndex === 0) {
        // Move to previous section
        set({ sweepFocus: 'summary' });
      } else if (direction === 'down' && focusedDepthIndex === depthOptions.length - 1) {
        // Move to next section
        set({ sweepFocus: 'cost' });
      }
    } else {
      // Navigate between sections
      if (direction === 'up' && currentIndex > 0) {
        const newFocus = focusOrder[currentIndex - 1];
        set({ sweepFocus: newFocus });
        if (newFocus === 'depth') {
          set({ focusedDepthIndex: depthOptions.length - 1 });
        }
      } else if (direction === 'down' && currentIndex < focusOrder.length - 1) {
        const newFocus = focusOrder[currentIndex + 1];
        set({ sweepFocus: newFocus });
        if (newFocus === 'depth') {
          set({ focusedDepthIndex: 0 });
        }
      }
    }
  },

  selectFocusedDepth: () => {
    const { depthOptions, focusedDepthIndex } = get();
    if (depthOptions[focusedDepthIndex]) {
      get().setDepth(depthOptions[focusedDepthIndex].id);
    }
  },
});
