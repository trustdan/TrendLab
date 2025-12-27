import { useCallback } from 'react';
import { Navigation, StatusBar } from './components';
import {
  DataPanel,
  StrategyPanel,
  SweepPanel,
  ResultsPanel,
  ChartPanel,
} from './components/panels';
import { useKeyboardNavigation, type KeyboardAction } from './hooks';
import { useAppStore, type PanelId } from './store';

/** Panel component map */
const PANEL_COMPONENTS: Record<PanelId, React.ComponentType> = {
  data: DataPanel,
  strategy: StrategyPanel,
  sweep: SweepPanel,
  results: ResultsPanel,
  chart: ChartPanel,
};

export function App() {
  const { activePanel, setStatus } = useAppStore();

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

  const ActivePanel = PANEL_COMPONENTS[activePanel];

  return (
    <div className="app-container">
      <header className="app-header">
        <span className="app-title">TrendLab</span>
        <span className="text-muted" style={{ fontSize: 'var(--font-size-sm)' }}>
          Trend-Following Backtest Lab
        </span>
      </header>

      <main className="app-main">
        <Navigation />
        <div className="app-content">
          <ActivePanel />
        </div>
      </main>

      <StatusBar />
    </div>
  );
}
