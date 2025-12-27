import { VscCheck } from 'react-icons/vsc';
import { useAppStore } from '../../../store';

export function EnsembleToggle() {
  const { ensembleEnabled, toggleEnsemble } = useAppStore();

  return (
    <div
      className={`ensemble-toggle ${ensembleEnabled ? 'enabled' : ''}`}
      onClick={toggleEnsemble}
      role="checkbox"
      aria-checked={ensembleEnabled}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          toggleEnsemble();
        }
      }}
    >
      <span className={`checkbox-box ${ensembleEnabled ? 'checked' : ''}`}>
        {ensembleEnabled && <VscCheck />}
      </span>
      <span className="ensemble-label">Ensemble Mode</span>
      <span className="keyboard-hint">(e)</span>

      <style>{`
        .ensemble-toggle {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          padding: var(--space-xs) var(--space-sm);
          cursor: pointer;
          border-radius: var(--radius-sm);
          user-select: none;
        }

        .ensemble-toggle:hover {
          background: var(--bg-hover);
        }

        .checkbox-box {
          width: 16px;
          height: 16px;
          border: 1px solid var(--border);
          border-radius: var(--radius-xs);
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 12px;
          color: var(--bg);
          background: var(--bg);
        }

        .checkbox-box.checked {
          background: var(--purple);
          border-color: var(--purple);
        }

        .ensemble-label {
          color: var(--fg);
        }

        .ensemble-toggle.enabled .ensemble-label {
          color: var(--purple);
        }

        .keyboard-hint {
          font-size: var(--font-size-xs);
          color: var(--muted);
          margin-left: auto;
        }
      `}</style>
    </div>
  );
}
