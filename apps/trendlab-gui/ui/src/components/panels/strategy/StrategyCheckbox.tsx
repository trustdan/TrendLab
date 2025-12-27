import { VscCheck } from 'react-icons/vsc';

interface StrategyCheckboxProps {
  name: string;
  isSelected: boolean;
  isFocused: boolean;
  hasParams: boolean;
  onClick: () => void;
}

export function StrategyCheckbox({
  name,
  isSelected,
  isFocused,
  hasParams,
  onClick,
}: StrategyCheckboxProps) {
  return (
    <div
      className={`strategy-checkbox ${isFocused ? 'focused' : ''} ${isSelected ? 'selected' : ''}`}
      onClick={onClick}
      role="checkbox"
      aria-checked={isSelected}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
    >
      <span className={`checkbox-box ${isSelected ? 'checked' : ''}`}>
        {isSelected && <VscCheck />}
      </span>
      <span className="strategy-name">{name}</span>
      {!hasParams && <span className="fixed-badge">fixed</span>}

      <style>{`
        .strategy-checkbox {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          padding: var(--space-xs) var(--space-sm);
          padding-left: calc(var(--space-lg) + var(--space-sm));
          cursor: pointer;
          border-radius: var(--radius-sm);
          user-select: none;
        }

        .strategy-checkbox:hover {
          background: var(--bg-hover);
        }

        .strategy-checkbox.focused {
          background: var(--bg-active);
          outline: 1px solid var(--blue);
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
          background: var(--cyan);
          border-color: var(--cyan);
        }

        .strategy-name {
          color: var(--fg);
        }

        .strategy-checkbox.selected .strategy-name {
          color: var(--cyan);
        }

        .fixed-badge {
          font-size: var(--font-size-xs);
          color: var(--muted);
          background: var(--bg-hover);
          padding: 1px 6px;
          border-radius: var(--radius-xs);
          margin-left: auto;
        }
      `}</style>
    </div>
  );
}
