import { VscSymbolClass, VscGraph } from 'react-icons/vsc';
import type { SelectionSummary as SelectionSummaryType } from '../../../types/sweep';

interface SelectionSummaryProps {
  summary: SelectionSummaryType | null;
  isActive: boolean;
}

export function SelectionSummary({ summary, isActive }: SelectionSummaryProps) {
  if (!summary) {
    return (
      <div className={`sweep-section ${isActive ? 'active' : ''}`}>
        <h3 className="sweep-section-title">Selection Summary</h3>
        <div className="sweep-empty">Loading...</div>
      </div>
    );
  }

  const { symbols, strategies, symbol_count, strategy_count, estimated_configs, has_cached_data } = summary;

  return (
    <div className={`sweep-section ${isActive ? 'active' : ''}`}>
      <h3 className="sweep-section-title">Selection Summary</h3>

      <div className="selection-grid">
        <div className="selection-item">
          <VscGraph className="selection-icon" />
          <div className="selection-details">
            <span className="selection-count">{symbol_count}</span>
            <span className="selection-label">Symbol{symbol_count !== 1 ? 's' : ''}</span>
          </div>
          <div className="selection-list">
            {symbols.slice(0, 5).join(', ')}
            {symbols.length > 5 && ` +${symbols.length - 5} more`}
          </div>
        </div>

        <div className="selection-item">
          <VscSymbolClass className="selection-icon" />
          <div className="selection-details">
            <span className="selection-count">{strategy_count}</span>
            <span className="selection-label">Strateg{strategy_count !== 1 ? 'ies' : 'y'}</span>
          </div>
          <div className="selection-list">
            {strategies.slice(0, 3).join(', ')}
            {strategies.length > 3 && ` +${strategies.length - 3} more`}
          </div>
        </div>
      </div>

      <div className="selection-summary-row">
        <span className="summary-label">Estimated Configurations:</span>
        <span className="summary-value configs">{estimated_configs.toLocaleString()}</span>
      </div>

      <div className="selection-summary-row">
        <span className="summary-label">Data Status:</span>
        <span className={`summary-value ${has_cached_data ? 'cached' : 'missing'}`}>
          {has_cached_data ? 'All data cached' : 'Some data missing'}
        </span>
      </div>

      {(symbol_count === 0 || strategy_count === 0) && (
        <div className="selection-warning">
          {symbol_count === 0 && <p>No symbols selected. Go to Data panel to select symbols.</p>}
          {strategy_count === 0 && <p>No strategies selected. Go to Strategy panel to select strategies.</p>}
        </div>
      )}

      <style>{`
        .sweep-section {
          padding: var(--space-md);
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          border-radius: var(--radius-md);
          margin-bottom: var(--space-md);
        }
        .sweep-section.active {
          border-color: var(--blue);
        }
        .sweep-section-title {
          font-size: var(--font-size-sm);
          color: var(--muted);
          margin: 0 0 var(--space-sm) 0;
          text-transform: uppercase;
          letter-spacing: 0.05em;
        }
        .sweep-empty {
          color: var(--muted);
          font-style: italic;
        }
        .selection-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: var(--space-md);
          margin-bottom: var(--space-md);
        }
        .selection-item {
          display: flex;
          flex-direction: column;
          gap: var(--space-xs);
        }
        .selection-icon {
          color: var(--blue);
          font-size: 1.25rem;
        }
        .selection-details {
          display: flex;
          align-items: baseline;
          gap: var(--space-xs);
        }
        .selection-count {
          font-size: var(--font-size-xl);
          font-weight: 600;
          color: var(--fg);
        }
        .selection-label {
          color: var(--muted);
        }
        .selection-list {
          font-size: var(--font-size-sm);
          color: var(--muted);
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }
        .selection-summary-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: var(--space-xs) 0;
          border-top: 1px solid var(--border);
        }
        .summary-label {
          color: var(--muted);
        }
        .summary-value {
          font-weight: 500;
        }
        .summary-value.configs {
          color: var(--yellow);
        }
        .summary-value.cached {
          color: var(--green);
        }
        .summary-value.missing {
          color: var(--red);
        }
        .selection-warning {
          margin-top: var(--space-sm);
          padding: var(--space-sm);
          background: rgba(255, 100, 100, 0.1);
          border: 1px solid var(--red);
          border-radius: var(--radius-sm);
          color: var(--red);
          font-size: var(--font-size-sm);
        }
        .selection-warning p {
          margin: 0;
        }
      `}</style>
    </div>
  );
}
