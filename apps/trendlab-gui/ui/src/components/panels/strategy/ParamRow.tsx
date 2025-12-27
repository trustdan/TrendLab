import { VscChevronLeft, VscChevronRight } from 'react-icons/vsc';
import type { ParamDef, ParamValue } from '../../../types';

interface ParamRowProps {
  def: ParamDef;
  value: ParamValue;
  isFocused: boolean;
  isEditable: boolean;
  onAdjust: (direction: 'increment' | 'decrement') => void;
}

export function ParamRow({
  def,
  value,
  isFocused,
  isEditable,
  onAdjust,
}: ParamRowProps) {
  const displayValue = value ?? def.default;

  // Format display value
  const formattedValue =
    typeof displayValue === 'number'
      ? def.type === 'float'
        ? displayValue.toFixed(2)
        : displayValue.toString()
      : displayValue;

  return (
    <div className={`param-row ${isFocused ? 'focused' : ''} ${!isEditable ? 'readonly' : ''}`}>
      <span className="param-label">{def.label}</span>

      <div className="param-value-container">
        {isEditable && (
          <button
            className="param-arrow"
            onClick={() => onAdjust('decrement')}
            tabIndex={-1}
            aria-label="Decrease"
          >
            <VscChevronLeft />
          </button>
        )}

        <span className="param-value">{formattedValue}</span>

        {isEditable && (
          <button
            className="param-arrow"
            onClick={() => onAdjust('increment')}
            tabIndex={-1}
            aria-label="Increase"
          >
            <VscChevronRight />
          </button>
        )}
      </div>

      <style>{`
        .param-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: var(--space-xs) var(--space-sm);
          border-radius: var(--radius-sm);
        }

        .param-row:hover {
          background: var(--bg-hover);
        }

        .param-row.focused {
          background: var(--bg-active);
          outline: 1px solid var(--blue);
        }

        .param-row.readonly {
          opacity: 0.6;
        }

        .param-label {
          color: var(--fg);
          min-width: 120px;
        }

        .param-value-container {
          display: flex;
          align-items: center;
          gap: var(--space-xs);
        }

        .param-value {
          min-width: 60px;
          text-align: right;
          font-family: var(--font-mono);
          color: var(--cyan);
        }

        .param-arrow {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 24px;
          height: 24px;
          border: none;
          background: var(--bg-hover);
          color: var(--muted);
          border-radius: var(--radius-xs);
          cursor: pointer;
        }

        .param-arrow:hover {
          background: var(--bg-active);
          color: var(--fg);
        }

        .param-row.readonly .param-arrow {
          display: none;
        }

        .param-row.focused .param-value {
          color: var(--yellow);
        }
      `}</style>
    </div>
  );
}
