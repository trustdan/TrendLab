import type { SliceCreator } from '../types';
import { invoke } from '@tauri-apps/api/core';
import type {
  YoloPhase,
  Leaderboard,
  CrossSymbolLeaderboard,
  LeaderboardScope,
} from '../../types/yolo';
import type {
  YoloStartedPayload,
  YoloProgressPayload,
  YoloIterationCompletePayload,
  YoloStoppedPayload,
} from '../../types/events';

// ============================================================================
// Types
// ============================================================================

/** Response from start_yolo command */
export interface StartYoloResponse {
  job_id: string;
  total_symbols: number;
  total_strategies: number;
}

/** Response from get_yolo_state command */
export interface YoloStateResponse {
  enabled: boolean;
  phase: YoloPhase;
  iteration: number;
  randomization_pct: number;
  total_configs_tested: number;
  started_at: string | null;
  current_job_id: string | null;
}

/** Response from get_leaderboard command */
export interface LeaderboardResponse {
  per_symbol: Leaderboard | null;
  cross_symbol: CrossSymbolLeaderboard | null;
}

/** YOLO slice state and actions */
export interface YoloSlice {
  // State
  yoloEnabled: boolean;
  yoloPhase: YoloPhase;
  yoloIteration: number;
  yoloRandomizationPct: number;
  yoloTotalConfigsTested: number;
  yoloSessionConfigsTested: number;
  yoloStartedAt: string | null;
  yoloCurrentJobId: string | null;
  yoloCompletedConfigs: number;
  yoloTotalConfigs: number;

  // Leaderboards - Session (current app launch)
  sessionLeaderboard: Leaderboard | null;
  sessionCrossSymbolLeaderboard: CrossSymbolLeaderboard | null;

  // Leaderboards - All-Time (persisted across sessions)
  allTimeLeaderboard: Leaderboard | null;
  allTimeCrossSymbolLeaderboard: CrossSymbolLeaderboard | null;

  // View scope toggle
  leaderboardScope: LeaderboardScope;

  // UI state
  yoloLoading: boolean;
  yoloError: string | null;

  // Computed accessors
  currentLeaderboard: () => Leaderboard | null;
  currentCrossSymbolLeaderboard: () => CrossSymbolLeaderboard | null;
  currentConfigsTested: () => number;

  // Actions - YOLO control
  startYolo: (randomizationPct?: number) => Promise<StartYoloResponse | null>;
  stopYolo: () => Promise<void>;
  setRandomizationPct: (pct: number) => void;

  // Actions - View scope
  toggleLeaderboardScope: () => void;
  setLeaderboardScope: (scope: LeaderboardScope) => void;

  // Actions - Data loading
  loadYoloState: () => Promise<void>;
  loadLeaderboards: () => Promise<void>;

  // Actions - Event handlers
  handleYoloStarted: (payload: YoloStartedPayload) => void;
  handleYoloProgress: (payload: YoloProgressPayload) => void;
  handleYoloIterationComplete: (payload: YoloIterationCompletePayload) => void;
  handleYoloStopped: (payload: YoloStoppedPayload) => void;

  // Actions - Utility
  resetYolo: () => void;
}

// ============================================================================
// Slice Creator
// ============================================================================

export const createYoloSlice: SliceCreator<YoloSlice> = (set, get) => ({
  // Initial state
  yoloEnabled: false,
  yoloPhase: 'idle',
  yoloIteration: 0,
  yoloRandomizationPct: 0.15, // 15% default
  yoloTotalConfigsTested: 0,
  yoloSessionConfigsTested: 0,
  yoloStartedAt: null,
  yoloCurrentJobId: null,
  yoloCompletedConfigs: 0,
  yoloTotalConfigs: 0,
  // Session leaderboards (reset each app launch)
  sessionLeaderboard: null,
  sessionCrossSymbolLeaderboard: null,
  // All-time leaderboards (persisted)
  allTimeLeaderboard: null,
  allTimeCrossSymbolLeaderboard: null,
  // Default to session view
  leaderboardScope: 'session',
  yoloLoading: false,
  yoloError: null,

  // Computed accessors
  currentLeaderboard: () => {
    const state = get();
    return state.leaderboardScope === 'session'
      ? state.sessionLeaderboard
      : state.allTimeLeaderboard;
  },

  currentCrossSymbolLeaderboard: () => {
    const state = get();
    return state.leaderboardScope === 'session'
      ? state.sessionCrossSymbolLeaderboard
      : state.allTimeCrossSymbolLeaderboard;
  },

  currentConfigsTested: () => {
    const state = get();
    return state.leaderboardScope === 'session'
      ? state.yoloSessionConfigsTested
      : state.yoloTotalConfigsTested;
  },

  // YOLO control
  startYolo: async (randomizationPct) => {
    const pct = randomizationPct ?? get().yoloRandomizationPct;
    set({ yoloLoading: true, yoloError: null });
    try {
      const response = await invoke<StartYoloResponse>('start_yolo_mode', {
        randomizationPct: pct,
      });
      set({
        yoloEnabled: true,
        yoloPhase: 'sweeping',
        yoloCurrentJobId: response.job_id,
        yoloStartedAt: new Date().toISOString(),
        yoloLoading: false,
      });
      return response;
    } catch (e) {
      set({ yoloError: String(e), yoloLoading: false });
      return null;
    }
  },

  stopYolo: async () => {
    const { yoloCurrentJobId } = get();
    if (!yoloCurrentJobId) return;

    set({ yoloLoading: true });
    try {
      await invoke('stop_yolo_mode', { jobId: yoloCurrentJobId });
      // State will be updated by handleYoloStopped event
    } catch (e) {
      set({ yoloError: String(e), yoloLoading: false });
    }
  },

  setRandomizationPct: (pct) => {
    // Clamp between 5% and 50%
    const clamped = Math.max(0.05, Math.min(0.5, pct));
    set({ yoloRandomizationPct: clamped });
  },

  // View scope actions
  toggleLeaderboardScope: () => {
    const current = get().leaderboardScope;
    set({ leaderboardScope: current === 'session' ? 'all_time' : 'session' });
  },

  setLeaderboardScope: (scope) => {
    set({ leaderboardScope: scope });
  },

  // Data loading
  loadYoloState: async () => {
    try {
      const state = await invoke<YoloStateResponse>('get_yolo_state');
      set({
        yoloEnabled: state.enabled,
        yoloPhase: state.phase,
        yoloIteration: state.iteration,
        yoloRandomizationPct: state.randomization_pct,
        yoloTotalConfigsTested: state.total_configs_tested,
        yoloStartedAt: state.started_at,
        yoloCurrentJobId: state.current_job_id,
      });
    } catch (e) {
      console.error('Failed to load YOLO state:', e);
    }
  },

  loadLeaderboards: async () => {
    try {
      // Load all-time leaderboards from backend (persisted on disk)
      const response = await invoke<LeaderboardResponse>('get_leaderboard');
      set({
        allTimeLeaderboard: response.per_symbol,
        allTimeCrossSymbolLeaderboard: response.cross_symbol,
      });
    } catch (e) {
      console.error('Failed to load leaderboards:', e);
    }
  },

  // Event handlers
  handleYoloStarted: (payload) => {
    set({
      yoloEnabled: true,
      yoloPhase: 'sweeping',
      yoloCurrentJobId: payload.jobId,
      yoloStartedAt: new Date().toISOString(),
      yoloIteration: 0,
      yoloCompletedConfigs: 0,
      yoloTotalConfigs: 0,
      yoloSessionConfigsTested: 0,
      yoloLoading: false,
      // Reset session leaderboards on new YOLO start
      sessionLeaderboard: null,
      sessionCrossSymbolLeaderboard: null,
    });
  },

  handleYoloProgress: (payload) => {
    set({
      yoloIteration: payload.iteration,
      yoloPhase: payload.phase as YoloPhase,
      yoloCompletedConfigs: payload.completedConfigs,
      yoloTotalConfigs: payload.totalConfigs,
    });
  },

  handleYoloIterationComplete: (payload) => {
    const configsThisRound = payload.configsTestedThisRound;
    set({
      yoloIteration: payload.iteration,
      // Update both session and all-time leaderboards
      sessionCrossSymbolLeaderboard: payload.crossSymbolLeaderboard,
      sessionLeaderboard: payload.perSymbolLeaderboard,
      allTimeCrossSymbolLeaderboard: payload.crossSymbolLeaderboard,
      allTimeLeaderboard: payload.perSymbolLeaderboard,
      // Update counts for both
      yoloSessionConfigsTested: get().yoloSessionConfigsTested + configsThisRound,
      yoloTotalConfigsTested: get().yoloTotalConfigsTested + configsThisRound,
    });
  },

  handleYoloStopped: (payload) => {
    set({
      yoloEnabled: false,
      yoloPhase: 'stopped',
      yoloCurrentJobId: null,
      // Final results update both session and all-time
      sessionCrossSymbolLeaderboard: payload.crossSymbolLeaderboard,
      sessionLeaderboard: payload.perSymbolLeaderboard,
      allTimeCrossSymbolLeaderboard: payload.crossSymbolLeaderboard,
      allTimeLeaderboard: payload.perSymbolLeaderboard,
      yoloIteration: payload.totalIterations,
      yoloTotalConfigsTested: payload.totalConfigsTested,
      yoloLoading: false,
    });
  },

  // Utility
  resetYolo: () => {
    set({
      yoloEnabled: false,
      yoloPhase: 'idle',
      yoloIteration: 0,
      yoloTotalConfigsTested: 0,
      yoloSessionConfigsTested: 0,
      yoloStartedAt: null,
      yoloCurrentJobId: null,
      yoloCompletedConfigs: 0,
      yoloTotalConfigs: 0,
      sessionLeaderboard: null,
      sessionCrossSymbolLeaderboard: null,
      allTimeLeaderboard: null,
      allTimeCrossSymbolLeaderboard: null,
      leaderboardScope: 'session',
      yoloLoading: false,
      yoloError: null,
    });
  },
});
