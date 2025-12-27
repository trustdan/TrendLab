import { useState, useCallback, useEffect } from 'react';
import { Navigation, StatusBar, StartupModal, type StartupMode } from './components';
import {
  DataPanel,
  StrategyPanel,
  SweepPanel,
  ResultsPanel,
  ChartPanel,
} from './components/panels';
import { useKeyboardNavigation, useFocusManagement, type KeyboardAction } from './hooks';
import { useAppStore, type PanelId } from './store';

/** Panel component map */
const PANEL_COMPONENTS: Record<PanelId, React.ComponentType> = {
  data: DataPanel,
  strategy: StrategyPanel,
  sweep: SweepPanel,
  results: ResultsPanel,
  chart: ChartPanel,
};

/** Check if startup modal should be shown based on stored preference */
function shouldShowStartupModal(): boolean {
  try {
    const stored = localStorage.getItem('trendlab-startup-mode');
    // Only skip modal if user has explicitly remembered a choice
    return stored !== 'manual' && stored !== 'full-auto';
  } catch {
    return true;
  }
}

export function App() {
  const { activePanel, setStatus, setActivePanel, loadYoloState, loadLeaderboards } = useAppStore();
  const [showStartupModal, setShowStartupModal] = useState(shouldShowStartupModal);
  const [appMode, setAppMode] = useState<StartupMode>(null);

  // Auto-close modal and set mode if user remembered their choice
  useEffect(() => {
    try {
      const stored = localStorage.getItem('trendlab-startup-mode');
      if (stored === 'manual' || stored === 'full-auto') {
        setAppMode(stored);
        setShowStartupModal(false);
      }
    } catch {
      // localStorage not available
    }
  }, []);

  // Load YOLO state and leaderboards on startup
  useEffect(() => {
    loadYoloState();
    loadLeaderboards();
  }, [loadYoloState, loadLeaderboards]);

  // Handle mode selection from startup modal
  const handleModeSelect = useCallback((mode: 'manual' | 'full-auto') => {
    setAppMode(mode);
    if (mode === 'full-auto') {
      // Navigate to sweep panel for YOLO mode
      setActivePanel('sweep');
      setStatus('Full-Auto (YOLO) mode selected - configure your sweep', 'success');
    } else {
      // Stay on data panel for manual mode
      setActivePanel('data');
      setStatus('Manual mode - select symbols, configure strategies, then sweep', 'info');
    }
  }, [setActivePanel, setStatus]);

  // Handle keyboard actions (panel-specific actions will be wired in later phases)
  const handleAction = useCallback(
    (action: KeyboardAction) => {
      switch (action.type) {
        case 'cancel':
          setStatus('Cancelled', 'info');
          break;

        case 'show_help':
          setStatus('Press 1-5 for panels, j/k to navigate, Space to select', 'info');
          break;

        case 'reset_defaults':
          setStatus('Reset to defaults', 'info');
          break;

        // Navigation actions - will be handled by individual panels
        case 'move_up':
        case 'move_down':
        case 'move_left':
        case 'move_right':
        case 'confirm':
        case 'toggle_selection':
        case 'select_all':
        case 'select_none':
          // These will be passed to active panel in later phases
          break;

        // Panel-specific actions - will be wired in later phases
        case 'fetch':
          setStatus('Fetch: coming in Phase 2', 'info');
          break;
        case 'search':
          setStatus('Search: coming in Phase 2', 'info');
          break;
        case 'sort':
          setStatus('Sort: coming in Phase 5', 'info');
          break;
        case 'toggle_view':
          setStatus('View mode: coming in Phase 5', 'info');
          break;
        case 'toggle_ensemble':
          setStatus('Ensemble: coming in Phase 3', 'info');
          break;
        case 'toggle_drawdown':
          setStatus('Drawdown overlay: coming in Phase 6', 'info');
          break;
        case 'toggle_chart_mode':
          setStatus('Chart mode: coming in Phase 6', 'info');
          break;
        case 'toggle_volume':
          setStatus('Volume: coming in Phase 6', 'info');
          break;
        case 'toggle_crosshair':
          setStatus('Crosshair: coming in Phase 6', 'info');
          break;

        default:
          break;
      }
    },
    [setStatus]
  );

  // Use centralized keyboard navigation (TUI parity)
  useKeyboardNavigation(handleAction);

  // Manage focus when switching panels
  useFocusManagement();

  const ActivePanel = PANEL_COMPONENTS[activePanel];

  return (
    <div className="app-container">
      <header className="app-header">
        <span className="app-title">TrendLab</span>
        <span className="text-muted" style={{ fontSize: 'var(--font-size-sm)' }}>
          Trend-Following Backtest Lab
          {appMode && (
            <span className="mode-badge">
              {appMode === 'full-auto' ? 'YOLO' : 'Manual'}
            </span>
          )}
        </span>
      </header>

      <main className="app-main">
        <Navigation />
        <div className="app-content">
          <ActivePanel />
        </div>
      </main>

      <StatusBar />

      <StartupModal
        isOpen={showStartupModal}
        onClose={() => setShowStartupModal(false)}
        onSelectMode={handleModeSelect}
      />

      {/* Screen reader announcer for dynamic updates */}
      <div
        id="sr-announcer"
        role="status"
        aria-live="polite"
        aria-atomic="true"
        className="sr-only"
      />

      <style>{`
        /* Screen reader only - visually hidden but accessible */
        .sr-only {
          position: absolute;
          width: 1px;
          height: 1px;
          padding: 0;
          margin: -1px;
          overflow: hidden;
          clip: rect(0, 0, 0, 0);
          white-space: nowrap;
          border: 0;
        }
        .mode-badge {
          margin-left: var(--space-sm);
          padding: 2px 6px;
          background: var(--purple);
          color: var(--bg);
          border-radius: var(--radius-xs);
          font-size: var(--font-size-xs);
          font-weight: 600;
          text-transform: uppercase;
        }
      `}</style>
    </div>
  );
}
