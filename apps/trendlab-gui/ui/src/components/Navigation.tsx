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
    <nav className="app-nav">
      <div className="nav-header">
        <span className="nav-title">Panels</span>
      </div>
      {PANELS.map((panel) => {
        const Icon = PANEL_ICONS[panel.id];
        const isActive = activePanel === panel.id;

        return (
          <button
            key={panel.id}
            className={`nav-item ${isActive ? 'active' : ''}`}
            onClick={() => setActivePanel(panel.id)}
            aria-current={isActive ? 'page' : undefined}
          >
            <Icon size={18} />
            <span className="nav-label">{panel.label}</span>
            <span className="nav-key">{panel.shortcut}</span>
          </button>
        );
      })}
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
