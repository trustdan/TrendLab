import { useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../store';
import { SectorList, TickerList, TickerDetail, SearchOverlay, FetchProgress } from './data';
import { useKeyboardNavigation, type KeyboardAction } from '../../hooks/useKeyboardNavigation';
import type { Universe, StartJobResponse } from '../../types';

export function DataPanel() {
  const viewMode = useAppStore((s) => s.viewMode);
  const searchMode = useAppStore((s) => s.searchMode);
  const setUniverse = useAppStore((s) => s.setUniverse);
  const setCachedSymbols = useAppStore((s) => s.setCachedSymbols);
  const universe = useAppStore((s) => s.universe);
  const activePanel = useAppStore((s) => s.activePanel);
  const selectedTickers = useAppStore((s) => s.selectedTickers);
  const isFetching = useAppStore((s) => s.isFetching);
  const setIsFetching = useAppStore((s) => s.setIsFetching);
  const fetchJobId = useAppStore((s) => s.fetchJobId);
  const setFetchJobId = useAppStore((s) => s.setFetchJobId);

  // Data navigation actions
  const navigateSector = useAppStore((s) => s.navigateSector);
  const navigateTicker = useAppStore((s) => s.navigateTicker);
  const expandToTickers = useAppStore((s) => s.expandToTickers);
  const collapseToSectors = useAppStore((s) => s.collapseToSectors);
  const toggleTicker = useAppStore((s) => s.toggleTicker);
  const selectAll = useAppStore((s) => s.selectAll);
  const selectAllInSector = useAppStore((s) => s.selectAllInSector);
  const selectNone = useAppStore((s) => s.selectNone);
  const selectNoneGlobal = useAppStore((s) => s.selectNoneGlobal);
  const enterSearchMode = useAppStore((s) => s.enterSearchMode);
  const exitSearchMode = useAppStore((s) => s.exitSearchMode);
  const getTickersForCurrentSector = useAppStore((s) => s.getTickersForCurrentSector);
  const selectedTickerIndex = useAppStore((s) => s.selectedTickerIndex);

  // Handle keyboard actions for Data panel
  const handleAction = useCallback(
    async (action: KeyboardAction) => {
      if (activePanel !== 'data') return;

      // Handle search mode separately
      if (searchMode) {
        if (action.type === 'cancel') {
          exitSearchMode();
        }
        return;
      }

      switch (action.type) {
        case 'move_down':
          if (viewMode === 'sectors') {
            navigateSector(1);
          } else {
            navigateTicker(1);
          }
          break;

        case 'move_up':
          if (viewMode === 'sectors') {
            navigateSector(-1);
          } else {
            navigateTicker(-1);
          }
          break;

        case 'move_right':
        case 'confirm':
          if (viewMode === 'sectors') {
            expandToTickers();
          }
          break;

        case 'move_left':
          if (viewMode === 'tickers') {
            collapseToSectors();
          }
          break;

        case 'toggle_selection':
          if (viewMode === 'tickers') {
            const tickers = getTickersForCurrentSector();
            const currentTicker = tickers[selectedTickerIndex];
            if (currentTicker) {
              toggleTicker(currentTicker);
            }
          }
          break;

        case 'select_all':
          // In sectors view, select ALL tickers globally; in tickers view, select all in current sector
          if (viewMode === 'sectors') {
            selectAll();
          } else {
            selectAllInSector();
          }
          break;

        case 'select_none':
          // In sectors view, deselect ALL; in tickers view, deselect current sector only
          if (viewMode === 'sectors') {
            selectNoneGlobal();
          } else {
            selectNone();
          }
          break;

        case 'search':
          enterSearchMode();
          break;

        case 'fetch':
          if (selectedTickers.size > 0 && !isFetching) {
            try {
              setIsFetching(true);
              const symbols = Array.from(selectedTickers);
              const today = new Date();
              const tenYearsAgo = new Date(today);
              tenYearsAgo.setFullYear(today.getFullYear() - 10);

              const response = await invoke<StartJobResponse>('fetch_data', {
                symbols,
                start: tenYearsAgo.toISOString().split('T')[0],
                end: today.toISOString().split('T')[0],
                force: false,
              });
              setFetchJobId(response.job_id);
            } catch (err) {
              console.error('Failed to start fetch:', err);
              setIsFetching(false);
              setFetchJobId(null);
            }
          }
          break;

        case 'cancel':
          if (isFetching && fetchJobId) {
            try {
              await invoke('cancel_job', { jobId: fetchJobId });
            } catch (err) {
              console.error('Failed to cancel fetch:', err);
            }
          }
          break;
      }
    },
    [
      activePanel,
      searchMode,
      viewMode,
      selectedTickers,
      isFetching,
      fetchJobId,
      navigateSector,
      navigateTicker,
      expandToTickers,
      collapseToSectors,
      toggleTicker,
      selectAll,
      selectAllInSector,
      selectNone,
      selectNoneGlobal,
      enterSearchMode,
      exitSearchMode,
      getTickersForCurrentSector,
      selectedTickerIndex,
      setIsFetching,
      setFetchJobId,
    ]
  );

  useKeyboardNavigation(handleAction);

  // Load universe and cached symbols on mount, then select all tickers by default
  useEffect(() => {
    const loadData = async () => {
      try {
        const [universeData, cachedData] = await Promise.all([
          invoke<Universe>('get_universe'),
          invoke<string[]>('get_cached_symbols'),
        ]);
        setUniverse(universeData);
        setCachedSymbols(cachedData);
        // Auto-select all tickers by default for YOLO mode
        // Use setTimeout to ensure universe is set in store first
        setTimeout(() => selectAll(), 0);
      } catch (err) {
        console.error('Failed to load data:', err);
      }
    };
    loadData();
  }, [setUniverse, setCachedSymbols, selectAll]);

  // Sync selected tickers to backend whenever selection changes
  useEffect(() => {
    const syncSelection = async () => {
      try {
        const tickers = Array.from(selectedTickers);
        await invoke('update_selection', { tickers });
      } catch (err) {
        console.error('Failed to sync selection to backend:', err);
      }
    };
    syncSelection();
  }, [selectedTickers]);

  if (!universe) {
    return (
      <div className="panel data-panel">
        <h1 className="panel-title">Data</h1>
        <div className="data-loading">Loading universe...</div>

        <style>{`
          .data-loading {
            display: flex;
            align-items: center;
            justify-content: center;
            flex: 1;
            color: var(--muted);
          }
        `}</style>
      </div>
    );
  }

  return (
    <div className="panel data-panel">
      <h1 className="panel-title">Data</h1>

      <div className="data-content">
        <div className="data-left">
          <SectorList />
        </div>
        <div className="data-right">
          {searchMode ? (
            <SearchOverlay />
          ) : viewMode === 'sectors' ? (
            <div className="sector-summary">
              <SectorSummary />
            </div>
          ) : (
            <TickerList />
          )}
        </div>
      </div>

      <FetchProgress />
      <TickerDetail />

      <style>{`
        .data-panel {
          display: flex;
          flex-direction: column;
          height: 100%;
        }
        .data-content {
          display: flex;
          flex: 1;
          min-height: 0;
          border: 1px solid var(--border);
          border-radius: var(--radius-md);
          overflow: hidden;
        }
        .data-left {
          width: 35%;
          min-width: 200px;
        }
        .data-right {
          flex: 1;
          display: flex;
          flex-direction: column;
        }
        .sector-summary {
          flex: 1;
          overflow-y: auto;
        }
      `}</style>
    </div>
  );
}

function SectorSummary() {
  const universe = useAppStore((s) => s.universe);
  const selectedTickers = useAppStore((s) => s.selectedTickers);
  const cachedSymbols = useAppStore((s) => s.cachedSymbols);
  const selectAll = useAppStore((s) => s.selectAll);
  const selectNoneGlobal = useAppStore((s) => s.selectNoneGlobal);

  if (!universe) return null;

  const sectorStats = universe.sectors.map((sector) => {
    const selected = sector.tickers.filter((t) => selectedTickers.has(t)).length;
    const cached = sector.tickers.filter((t) => cachedSymbols.has(t)).length;
    return {
      id: sector.id,
      name: sector.name,
      total: sector.tickers.length,
      selected,
      cached,
    };
  });

  const totalTickers = universe.sectors.reduce((sum, s) => sum + s.tickers.length, 0);
  const totalSelected = selectedTickers.size;
  const totalCached = [...selectedTickers].filter((t) =>
    cachedSymbols.has(t)
  ).length;
  const allSelected = totalSelected === totalTickers;

  return (
    <div className="sector-summary-content">
      <div className="summary-header">
        <div className="summary-title-row">
          <h3>Selection Summary</h3>
          <div className="selection-buttons">
            <button
              className={`selection-btn ${allSelected ? 'active' : ''}`}
              onClick={selectAll}
              title="Select all tickers (A)"
            >
              All ({totalTickers})
            </button>
            <button
              className={`selection-btn ${totalSelected === 0 ? 'active' : ''}`}
              onClick={selectNoneGlobal}
              title="Deselect all (N)"
            >
              None
            </button>
          </div>
        </div>
        <div className="summary-totals">
          <span className="total-selected">{totalSelected} selected</span>
          <span className="total-cached">{totalCached} cached</span>
        </div>
      </div>

      <div className="summary-sectors">
        {sectorStats.map((stat) => (
          <div key={stat.id} className="summary-sector">
            <span className="sector-name">{stat.name}</span>
            <span className="sector-stats">
              <span className={`stat-selected ${stat.selected > 0 ? 'has-selection' : ''}`}>
                {stat.selected}/{stat.total}
              </span>
              {stat.cached > 0 && (
                <span className="stat-cached">({stat.cached} cached)</span>
              )}
            </span>
          </div>
        ))}
      </div>

      <style>{`
        .sector-summary-content {
          padding: var(--space-md);
        }
        .summary-header {
          margin-bottom: var(--space-md);
          padding-bottom: var(--space-sm);
          border-bottom: 1px solid var(--border);
        }
        .summary-title-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-bottom: var(--space-xs);
        }
        .summary-header h3 {
          margin: 0;
          color: var(--fg);
          font-size: var(--font-size-md);
        }
        .selection-buttons {
          display: flex;
          gap: var(--space-xs);
        }
        .selection-btn {
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          color: var(--muted);
          padding: var(--space-xs) var(--space-sm);
          border-radius: var(--radius-sm);
          cursor: pointer;
          font-family: var(--font-mono);
          font-size: var(--font-size-xs);
          transition: all 0.15s ease;
        }
        .selection-btn:hover {
          background: var(--bg);
          color: var(--fg);
          border-color: var(--blue);
        }
        .selection-btn.active {
          background: var(--blue-bg);
          color: var(--blue);
          border-color: var(--blue);
        }
        .summary-totals {
          display: flex;
          gap: var(--space-md);
          font-size: var(--font-size-sm);
        }
        .total-selected {
          color: var(--blue);
        }
        .total-cached {
          color: var(--green);
        }
        .summary-sectors {
          display: flex;
          flex-direction: column;
          gap: var(--space-xs);
        }
        .summary-sector {
          display: flex;
          justify-content: space-between;
          padding: var(--space-xs) 0;
          font-size: var(--font-size-sm);
        }
        .sector-name {
          color: var(--muted);
        }
        .sector-stats {
          display: flex;
          gap: var(--space-sm);
          font-family: var(--font-mono);
        }
        .stat-selected {
          color: var(--muted);
        }
        .stat-selected.has-selection {
          color: var(--green);
        }
        .stat-cached {
          color: var(--muted);
          font-size: var(--font-size-xs);
        }
      `}</style>
    </div>
  );
}
