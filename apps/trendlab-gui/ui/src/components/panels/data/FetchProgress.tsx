import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../../store';
import type { FetchProgress as FetchProgressType, FetchComplete } from '../../../types';

export function FetchProgress() {
  const fetchProgress = useAppStore((s) => s.fetchProgress);
  const isFetching = useAppStore((s) => s.isFetching);
  const fetchJobId = useAppStore((s) => s.fetchJobId);
  const setFetchProgress = useAppStore((s) => s.setFetchProgress);
  const setIsFetching = useAppStore((s) => s.setIsFetching);
  const setFetchJobId = useAppStore((s) => s.setFetchJobId);
  const setCachedSymbols = useAppStore((s) => s.setCachedSymbols);

  // Listen to fetch events
  useEffect(() => {
    const unsubscribers: Array<() => void> = [];

    // Progress events - extract payload from EventEnvelope wrapper
    listen<{ payload: FetchProgressType }>('data:progress', (event) => {
      setFetchProgress(event.payload.payload);
    }).then((unsub) => unsubscribers.push(unsub));

    // Complete event - refresh cached symbols list
    listen<{ payload: FetchComplete }>('data:complete', async (event) => {
      setIsFetching(false);
      setFetchProgress(null);
      setFetchJobId(null);
      console.log('Fetch complete:', event.payload.payload);

      // Refresh cached symbols from backend
      try {
        const cachedData = await invoke<string[]>('get_cached_symbols');
        setCachedSymbols(cachedData);
      } catch (err) {
        console.error('Failed to refresh cached symbols:', err);
      }
    }).then((unsub) => unsubscribers.push(unsub));

    // Cancelled event
    listen('data:cancelled', () => {
      setIsFetching(false);
      setFetchProgress(null);
      setFetchJobId(null);
    }).then((unsub) => unsubscribers.push(unsub));

    return () => {
      unsubscribers.forEach((unsub) => unsub());
    };
  }, [setFetchProgress, setIsFetching, setFetchJobId, setCachedSymbols]);

  const handleCancel = async () => {
    if (!fetchJobId) return;
    try {
      await invoke('cancel_job', { jobId: fetchJobId });
    } catch (err) {
      console.error('Failed to cancel fetch:', err);
    }
  };

  if (!isFetching || !fetchProgress) {
    return null;
  }

  const percent = Math.round((fetchProgress.current / fetchProgress.total) * 100);

  return (
    <div className="fetch-progress">
      <div className="progress-header">
        <span className="progress-title">Fetching Data</span>
        <button className="progress-cancel" onClick={handleCancel}>
          Cancel (Esc)
        </button>
      </div>

      <div className="progress-bar-container">
        <div className="progress-bar" style={{ width: `${percent}%` }} />
      </div>

      <div className="progress-info">
        <span className="progress-symbol">{fetchProgress.symbol}</span>
        <span className="progress-count">
          {fetchProgress.current} / {fetchProgress.total}
        </span>
        <span className="progress-percent">{percent}%</span>
      </div>

      {fetchProgress.message && (
        <div className="progress-message">{fetchProgress.message}</div>
      )}

      <style>{`
        .fetch-progress {
          padding: var(--space-md);
          background: var(--bg-secondary);
          border: 1px solid var(--blue);
          border-radius: var(--radius-md);
          margin-top: var(--space-sm);
        }
        .progress-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: var(--space-sm);
        }
        .progress-title {
          color: var(--blue);
          font-weight: 600;
          font-size: var(--font-size-sm);
        }
        .progress-cancel {
          padding: var(--space-xs) var(--space-sm);
          background: none;
          border: 1px solid var(--red);
          border-radius: var(--radius-sm);
          color: var(--red);
          cursor: pointer;
          font-family: var(--font-mono);
          font-size: var(--font-size-xs);
        }
        .progress-cancel:hover {
          background: var(--red-bg);
        }
        .progress-bar-container {
          height: 4px;
          background: var(--border);
          border-radius: 2px;
          overflow: hidden;
          margin-bottom: var(--space-sm);
        }
        .progress-bar {
          height: 100%;
          background: var(--blue);
          transition: width 0.2s ease;
        }
        .progress-info {
          display: flex;
          gap: var(--space-md);
          font-family: var(--font-mono);
          font-size: var(--font-size-sm);
        }
        .progress-symbol {
          color: var(--cyan);
          font-weight: 600;
        }
        .progress-count {
          color: var(--muted);
        }
        .progress-percent {
          color: var(--green);
          margin-left: auto;
        }
        .progress-message {
          margin-top: var(--space-xs);
          color: var(--muted);
          font-size: var(--font-size-xs);
        }
      `}</style>
    </div>
  );
}
