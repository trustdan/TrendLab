import { useAppStore } from '../../../store';
import type { ViewMode } from '../../../types';
import { VIEW_MODE_LABELS } from '../../../types';

interface ViewModeToggleProps {
  isActive: boolean;
}

const VIEW_MODES: ViewMode[] = ['all_configs', 'per_ticker', 'by_strategy'];

export function ViewModeToggle({ isActive }: ViewModeToggleProps) {
  const { resultsViewMode, setResultsViewMode } = useAppStore();

  return (
    <div className={`view-mode-toggle ${isActive ? 'active' : ''}`}>
      {VIEW_MODES.map((mode) => (
        <button
          key={mode}
          className={`mode-btn ${resultsViewMode === mode ? 'selected' : ''}`}
          onClick={() => setResultsViewMode(mode)}
        >
          {VIEW_MODE_LABELS[mode]}
        </button>
      ))}

      <style>{`
        .view-mode-toggle {
          display: flex;
          gap: 2px;
          padding: 2px;
          background: var(--bg-secondary);
          border-radius: var(--radius-sm);
          border: 1px solid var(--border);
        }

        .view-mode-toggle.active {
          border-color: var(--cyan);
        }

        .mode-btn {
          padding: var(--space-xs) var(--space-sm);
          font-size: var(--font-size-xs);
          background: transparent;
          border: none;
          color: var(--muted);
          cursor: pointer;
          border-radius: var(--radius-xs);
          transition: all 0.15s;
        }

        .mode-btn:hover {
          color: var(--fg);
          background: var(--bg-hover);
        }

        .mode-btn.selected {
          background: var(--cyan);
          color: var(--bg);
          font-weight: 600;
        }
      `}</style>
    </div>
  );
}
