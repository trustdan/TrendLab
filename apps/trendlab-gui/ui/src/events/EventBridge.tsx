/**
 * EventBridge - Listens for worker events from the Rust backend and syncs state.
 *
 * The backend (trendlab-engine worker) emits events when operations complete.
 * This bridge translates those events into store updates by re-fetching
 * data from the engine (single source of truth).
 */

import { useEffect, useRef } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../store';
import type {
  WorkerFetchStartedPayload,
  WorkerFetchCompletePayload,
  WorkerFetchAllCompletePayload,
  WorkerSweepStartedPayload,
  WorkerSweepProgressPayload,
  WorkerSweepCancelledPayload,
  WorkerYoloIterationPayload,
} from '../types/events';

/**
 * Initialize the event bridge for worker → frontend communication.
 * Call this once at app startup.
 */
export function useEventBridge() {
  const store = useAppStore();
  const storeRef = useRef(store);
  storeRef.current = store;

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    // Helper to add a listener
    const addListener = <T,>(
      eventName: string,
      handler: (payload: T) => void
    ) => {
      listen<T>(eventName, (event) => {
        handler(event.payload);
      }).then((fn) => unlisteners.push(fn));
    };

    // Helper to refresh cached symbols from backend
    const refreshCachedSymbols = async () => {
      try {
        const symbols = await invoke<string[]>('get_cached_symbols');
        storeRef.current.setCachedSymbols(symbols);
      } catch (e) {
        console.error('Failed to refresh cached symbols:', e);
      }
    };

    // ========================================================================
    // Search Events
    // ========================================================================

    addListener('worker:search-results', () => {
      // Search results updated in engine - could re-fetch if needed
      // For now, search is handled synchronously via invoke
    });

    // ========================================================================
    // Fetch Events
    // ========================================================================

    addListener<WorkerFetchStartedPayload>('worker:fetch-started', (payload) => {
      const { setStatus, setFetchProgress } = storeRef.current;
      setStatus(`Fetching ${payload.symbol} (${payload.index + 1}/${payload.total})`, 'info');
      setFetchProgress({
        symbol: payload.symbol,
        current: payload.index,
        total: payload.total,
        message: `Fetching ${payload.symbol}`,
      });
    });

    addListener<WorkerFetchCompletePayload>('worker:fetch-complete', (payload) => {
      const { setStatus, addCachedSymbol } = storeRef.current;
      setStatus(`Fetched ${payload.symbol}`, 'success');
      // Add newly fetched symbol to cache
      addCachedSymbol(payload.symbol);
    });

    addListener<WorkerFetchAllCompletePayload>('worker:fetch-all-complete', (payload) => {
      const { setStatus, setIsFetching, setFetchProgress } = storeRef.current;
      setStatus(`Fetched ${payload.symbolsFetched} symbols`, 'success');
      setIsFetching(false);
      setFetchProgress(null);
      // Refresh full cached symbols list
      refreshCachedSymbols();
    });

    // ========================================================================
    // Sweep Events
    // ========================================================================

    addListener<WorkerSweepStartedPayload>('worker:sweep-started', (payload) => {
      const { setStatus, updateProgress } = storeRef.current;
      setStatus(`Sweep started: ${payload.totalConfigs} configs`, 'info');
      updateProgress({
        current: 0,
        total: payload.totalConfigs,
        message: 'Starting sweep...',
        symbol: '',
        strategy: '',
      });
    });

    addListener<WorkerSweepProgressPayload>('worker:sweep-progress', (payload) => {
      const { updateProgress } = storeRef.current;
      updateProgress({
        current: payload.completed,
        total: payload.total,
        message: `${payload.completed}/${payload.total} configs`,
        symbol: '',
        strategy: '',
      });
    });

    addListener('worker:sweep-complete', () => {
      const { setStatus, refreshResults, handleComplete, setActivePanel } = storeRef.current;
      setStatus('Sweep complete!', 'success');
      handleComplete({}); // Clears isRunning state
      // Re-fetch results from engine
      refreshResults();
      // Navigate to results panel
      setActivePanel('results');
    });

    addListener<WorkerSweepCancelledPayload>('worker:sweep-cancelled', (payload) => {
      const { setStatus, handleCancelled, refreshResults } = storeRef.current;
      setStatus(`Sweep cancelled after ${payload.completed} configs`, 'info');
      handleCancelled();
      // Still load partial results
      refreshResults();
    });

    // ========================================================================
    // YOLO Events
    // ========================================================================

    addListener<WorkerYoloIterationPayload>('worker:yolo-iteration', (payload) => {
      const { setStatus, loadLeaderboards } = storeRef.current;
      setStatus(`YOLO iteration ${payload.iteration} complete`, 'info');
      // Re-fetch leaderboards from engine
      loadLeaderboards();
    });

    addListener('worker:yolo-stopped', () => {
      const { setStatus, loadYoloState, loadLeaderboards } = storeRef.current;
      setStatus('YOLO mode stopped', 'info');
      loadYoloState();
      loadLeaderboards();
    });

    // Cleanup all listeners on unmount
    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);
}

/**
 * EventBridge component - renders nothing, just sets up event listeners.
 * Include this in App.tsx to enable worker event handling.
 */
export function EventBridge() {
  useEventBridge();
  return null;
}

export default EventBridge;
