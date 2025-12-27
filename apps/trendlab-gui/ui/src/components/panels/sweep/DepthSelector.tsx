import { VscCheck } from 'react-icons/vsc';
import type { SweepDepth, DepthOption } from '../../../types/sweep';

interface DepthSelectorProps {
  options: DepthOption[];
  selected: SweepDepth;
  focusedIndex: number;
  isActive: boolean;
  onSelect: (depth: SweepDepth) => void;
}

export function DepthSelector({
  options,
  selected,
  focusedIndex,
  isActive,
  onSelect,
}: DepthSelectorProps) {
  return (
    <div className={`sweep-section ${isActive ? 'active' : ''}`}>
      <h3 className="sweep-section-title">Sweep Depth</h3>

      <div className="depth-options">
        {options.map((option, index) => {
          const isSelected = option.id === selected;
          const isFocused = isActive && index === focusedIndex;

          return (
            <button
              key={option.id}
              className={`depth-option ${isSelected ? 'selected' : ''} ${isFocused ? 'focused' : ''}`}
              onClick={() => onSelect(option.id)}
            >
              <div className="depth-header">
                <span className="depth-name">{option.name}</span>
                {isSelected && <VscCheck className="depth-check" />}
              </div>
              <div className="depth-description">{option.description}</div>
              <div className="depth-configs">
                ~{option.estimated_configs.toLocaleString()} configs
              </div>
            </button>
          );
        })}
      </div>

      <div className="depth-hint">
        <span className="key">j/k</span> navigate
        <span className="key">Space</span> select
      </div>

      <style>{`
        .depth-options {
          display: flex;
          flex-direction: column;
          gap: var(--space-xs);
        }
        .depth-option {
          display: flex;
          flex-direction: column;
          align-items: flex-start;
          padding: var(--space-sm) var(--space-md);
          background: var(--bg);
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          cursor: pointer;
          text-align: left;
          transition: all 0.15s ease;
        }
        .depth-option:hover {
          border-color: var(--blue);
          background: var(--bg-hover);
        }
        .depth-option.focused {
          border-color: var(--blue);
          background: var(--bg-hover);
          outline: 2px solid var(--blue);
          outline-offset: 2px;
        }
        .depth-option.selected {
          border-color: var(--green);
          background: rgba(100, 255, 100, 0.05);
        }
        .depth-option.selected.focused {
          outline-color: var(--green);
        }
        .depth-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          width: 100%;
        }
        .depth-name {
          font-weight: 600;
          color: var(--fg);
        }
        .depth-check {
          color: var(--green);
        }
        .depth-description {
          font-size: var(--font-size-sm);
          color: var(--muted);
          margin-top: var(--space-xs);
        }
        .depth-configs {
          font-size: var(--font-size-sm);
          color: var(--yellow);
          margin-top: var(--space-xs);
        }
        .depth-hint {
          display: flex;
          gap: var(--space-md);
          margin-top: var(--space-sm);
          font-size: var(--font-size-xs);
          color: var(--muted);
        }
        .depth-hint .key {
          display: inline-block;
          padding: 0 var(--space-xs);
          background: var(--bg);
          border: 1px solid var(--border);
          border-radius: var(--radius-xs);
          font-family: monospace;
          margin-right: var(--space-xs);
        }
      `}</style>
    </div>
  );
}
