import { VscDatabase, VscSettings, VscPlay, VscTable, VscGraphLine } from 'react-icons/vsc';
import { useAppStore, PANELS, type PanelId } from '../store';

const PANEL_ICONS: Record<PanelId, React.ComponentType<{ size?: number }>> = {
  data: VscDatabase,
  strategy: VscSettings,
  sweep: VscPlay,
  results: VscTable,
  chart: VscGraphLine,
};

export function Navigation() {
  const { activePanel, setActivePanel } = useAppStore();

  return (
    <nav className="app-nav" aria-label="Main panels">
      <div className="nav-header">
        <span className="nav-title" id="nav-title">Panels</span>
      </div>
      <div role="tablist" aria-labelledby="nav-title" aria-orientation="vertical">
        {PANELS.map((panel) => {
          const Icon = PANEL_ICONS[panel.id];
          const isActive = activePanel === panel.id;

          return (
            <button
              key={panel.id}
              role="tab"
              id={`tab-${panel.id}`}
              aria-selected={isActive}
              aria-controls={`panel-${panel.id}`}
              tabIndex={isActive ? 0 : -1}
              className={`nav-item ${isActive ? 'active' : ''}`}
              onClick={() => setActivePanel(panel.id)}
            >
              <Icon size={18} aria-hidden="true" />
              <span className="nav-label">{panel.label}</span>
              <span className="nav-key" aria-label={`Keyboard shortcut: ${panel.shortcut}`}>
                {panel.shortcut}
              </span>
            </button>
          );
        })}
      </div>
      <style>{`
        .nav-header {
          padding: var(--space-md);
          border-bottom: 1px solid var(--border);
        }
        .nav-title {
          font-size: var(--font-size-xs);
          font-weight: 600;
          text-transform: uppercase;
          letter-spacing: 0.1em;
          color: var(--muted);
        }
        .nav-label {
          flex: 1;
        }
      `}</style>
    </nav>
  );
}
