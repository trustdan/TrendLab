import { useState, useEffect, useCallback } from 'react';
import { VscTable, VscRefresh, VscTrash, VscLoading, VscStarFull } from 'react-icons/vsc';
import { useAppStore } from '../../store';
import {
  ResultsTable,
  ViewModeToggle,
  ResultDetail,
  ExportButton,
  Leaderboard,
} from './results';
import { useKeyboardNavigation, type KeyboardAction } from '../../hooks';

type ResultsView = 'results' | 'leaderboard';

export function ResultsPanel() {
  const {
    results,
    isLoading,
    resultsError,
    hasResults,
    loadResults,
    refreshResults,
    clearResults,
    activePanel,
    navigateResultUp,
    navigateResultDown,
    selectFocusedResult,
    cycleResultsViewMode,
    cycleSortMetric,
    setActivePanel,
    setChartMode,
    // YOLO leaderboard
    crossSymbolLeaderboard,
    leaderboard,
  } = useAppStore();

  const [currentView, setCurrentView] = useState<ResultsView>('results');

  // Check if we have leaderboard data
  const hasLeaderboardData =
    (crossSymbolLeaderboard?.entries?.length ?? 0) > 0 ||
    (leaderboard?.entries?.length ?? 0) > 0;

  // Handle keyboard actions for Results panel
  const handleAction = useCallback(
    (action: KeyboardAction) => {
      if (activePanel !== 'results') return;

      switch (action.type) {
        case 'move_down':
          navigateResultDown();
          break;
        case 'move_up':
          navigateResultUp();
          break;
        case 'confirm':
          // Select focused result and go to chart
          selectFocusedResult();
          setChartMode('equity');
          setActivePanel('chart');
          break;
        case 'toggle_selection':
          selectFocusedResult();
          break;
        case 'toggle_view':
          cycleResultsViewMode();
          break;
        case 'sort':
          cycleSortMetric();
          break;
      }
    },
    [
      activePanel,
      navigateResultUp,
      navigateResultDown,
      selectFocusedResult,
      cycleResultsViewMode,
      cycleSortMetric,
      setActivePanel,
      setChartMode,
    ]
  );

  useKeyboardNavigation(handleAction);

  // Load results on mount
  useEffect(() => {
    loadResults();
  }, [loadResults]);

  const showResults = hasResults() && !isLoading && currentView === 'results';
  const showLeaderboard = currentView === 'leaderboard' && !isLoading;
  const showEmpty = !hasResults() && !isLoading && !resultsError && currentView === 'results';
  const showError = resultsError !== null && !isLoading;

  return (
    <div className="panel results-panel">
      <div className="panel-header">
        <div className="header-left">
          <h1 className="panel-title">Results</h1>

          {/* View toggle: Results vs Leaderboard */}
          {hasLeaderboardData && (
            <div className="view-toggle">
              <button
                className={`toggle-btn ${currentView === 'results' ? 'active' : ''}`}
                onClick={() => setCurrentView('results')}
                title="View sweep results"
              >
                <VscTable size={14} />
                <span>Results</span>
              </button>
              <button
                className={`toggle-btn ${currentView === 'leaderboard' ? 'active' : ''}`}
                onClick={() => setCurrentView('leaderboard')}
                title="View YOLO leaderboard"
              >
                <VscStarFull size={14} />
                <span>Leaderboard</span>
              </button>
            </div>
          )}
        </div>

        <div className="panel-actions">
          {currentView === 'results' && <ViewModeToggle isActive={false} />}
          {currentView === 'results' && <ExportButton />}
          <button
            className="icon-button"
            onClick={() => refreshResults()}
            disabled={isLoading}
            title="Refresh results"
          >
            <VscRefresh className={isLoading ? 'spin' : ''} size={16} />
          </button>
          {currentView === 'results' && (
            <button
              className="icon-button danger"
              onClick={() => clearResults()}
              disabled={isLoading || !hasResults()}
              title="Clear all results"
            >
              <VscTrash size={16} />
            </button>
          )}
        </div>
      </div>

      {isLoading && (
        <div className="panel-loading">
          <VscLoading className="spin" size={32} />
          <span>Loading results...</span>
        </div>
      )}

      {showError && (
        <div className="panel-error">
          <span className="error-label">Error:</span>
          <span className="error-message">{resultsError}</span>
          <button className="retry-button" onClick={() => loadResults()}>
            Retry
          </button>
        </div>
      )}

      {showEmpty && (
        <div className="panel-empty">
          <VscTable size={48} />
          <h2>No Results</h2>
          <p>Run a parameter sweep to generate backtest results</p>
          <ul>
            <li>Select symbols in the Data panel</li>
            <li>Configure strategies in the Strategy panel</li>
            <li>Set sweep parameters and run in the Sweep panel</li>
          </ul>
        </div>
      )}

      {showResults && (
        <div className="results-content">
          <div className="results-main">
            <ResultsTable isActive={true} />
          </div>
          <div className="results-sidebar">
            <ResultDetail isActive={true} />
          </div>
        </div>
      )}

      {showLeaderboard && (
        <div className="leaderboard-content">
          <Leaderboard isActive={activePanel === 'results'} />
        </div>
      )}

      <style>{`
        .results-panel {
          display: flex;
          flex-direction: column;
          height: 100%;
        }

        .panel-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-bottom: var(--space-md);
          padding-bottom: var(--space-sm);
          border-bottom: 1px solid var(--border);
        }

        .header-left {
          display: flex;
          align-items: center;
          gap: var(--space-md);
        }

        .view-toggle {
          display: flex;
          gap: 2px;
          padding: 2px;
          background: var(--bg-secondary);
          border-radius: var(--radius-sm);
          border: 1px solid var(--border);
        }

        .toggle-btn {
          display: flex;
          align-items: center;
          gap: var(--space-xs);
          padding: var(--space-xs) var(--space-sm);
          font-size: var(--font-size-xs);
          background: transparent;
          border: none;
          color: var(--muted);
          cursor: pointer;
          border-radius: var(--radius-xs);
          transition: all 0.15s;
        }

        .toggle-btn:hover {
          color: var(--fg);
          background: var(--bg-hover);
        }

        .toggle-btn.active {
          background: var(--cyan);
          color: var(--bg);
          font-weight: 600;
        }

        .panel-actions {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
        }

        .icon-button {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 32px;
          height: 32px;
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          background: var(--bg-secondary);
          color: var(--fg);
          cursor: pointer;
          transition: all 0.15s ease;
        }

        .icon-button:hover:not(:disabled) {
          border-color: var(--cyan);
          color: var(--cyan);
        }

        .icon-button:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .icon-button.danger:hover:not(:disabled) {
          border-color: var(--red);
          color: var(--red);
        }

        .panel-loading {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: var(--space-md);
          padding: var(--space-xl);
          color: var(--muted);
        }

        .panel-error {
          display: flex;
          align-items: center;
          gap: var(--space-md);
          padding: var(--space-md);
          background: rgba(255, 85, 85, 0.1);
          border: 1px solid var(--red);
          border-radius: var(--radius-md);
        }

        .error-label {
          color: var(--red);
          font-weight: 600;
        }

        .error-message {
          flex: 1;
          color: var(--fg);
        }

        .retry-button {
          padding: var(--space-xs) var(--space-sm);
          border: 1px solid var(--red);
          border-radius: var(--radius-sm);
          background: transparent;
          color: var(--red);
          cursor: pointer;
          transition: all 0.15s ease;
        }

        .retry-button:hover {
          background: var(--red);
          color: var(--bg);
        }

        .panel-empty {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: var(--space-md);
          padding: var(--space-xl);
          text-align: center;
          color: var(--muted);
          border: 2px dashed var(--border);
          border-radius: var(--radius-lg);
          margin-top: var(--space-lg);
        }

        .panel-empty h2 {
          font-size: var(--font-size-lg);
          color: var(--fg);
          margin: 0;
        }

        .panel-empty p {
          margin: 0;
          font-size: var(--font-size-sm);
        }

        .panel-empty ul {
          list-style: none;
          padding: 0;
          margin: var(--space-md) 0 0;
          font-size: var(--font-size-sm);
          text-align: left;
        }

        .panel-empty li {
          padding: var(--space-xs) 0;
        }

        .panel-empty li::before {
          content: "→ ";
          color: var(--blue);
        }

        .results-content {
          display: grid;
          grid-template-columns: 1fr 350px;
          gap: var(--space-md);
          flex: 1;
          min-height: 0;
        }

        .results-main {
          overflow: auto;
          min-height: 0;
        }

        .results-sidebar {
          overflow: auto;
          min-height: 0;
        }

        .leaderboard-content {
          flex: 1;
          min-height: 0;
          overflow: hidden;
        }

        @keyframes spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }

        .spin {
          animation: spin 1s linear infinite;
        }

        /* Responsive: stack on narrow screens */
        @media (max-width: 900px) {
          .results-content {
            grid-template-columns: 1fr;
          }

          .results-sidebar {
            order: -1;
            max-height: 200px;
          }
        }
      `}</style>
    </div>
  );
}
