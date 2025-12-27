import { useEffect, useRef, useCallback } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useAppStore } from '../../../store';
import { MetricHeader } from './MetricHeader';
import type { ResultRow, SortMetric } from '../../../types';

interface ResultsTableProps {
  isActive: boolean;
}

/** Row height in pixels for virtualization */
const ROW_HEIGHT = 32;

/** Threshold for enabling virtualization (in row count) */
const VIRTUALIZATION_THRESHOLD = 100;

/** Format a number as percentage */
function formatPct(value: number): string {
  return `${(value * 100).toFixed(1)}%`;
}

/** Format a number with fixed decimals */
function formatNum(value: number, decimals: number = 2): string {
  return value.toFixed(decimals);
}

/** ResultRow component for a single row */
function ResultRowItem({
  result,
  isSelected,
  isFocused,
  rowIndex,
  style,
  onSelect,
}: {
  result: ResultRow;
  isSelected: boolean;
  isFocused: boolean;
  rowIndex: number;
  style?: React.CSSProperties;
  onSelect: () => void;
}) {
  const m = result.metrics;
  const rowRef = useRef<HTMLTableRowElement>(null);

  // Scroll into view when focused
  useEffect(() => {
    if (isFocused && rowRef.current) {
      rowRef.current.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  }, [isFocused]);

  return (
    <tr
      ref={rowRef}
      id={`result-row-${rowIndex}`}
      tabIndex={isFocused ? 0 : -1}
      aria-selected={isSelected}
      aria-rowindex={rowIndex + 2} // +2 for 1-based index + header row
      className={`result-row ${isSelected ? 'selected' : ''} ${isFocused ? 'focused' : ''}`}
      style={style}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect();
        }
      }}
    >
      <td className="col-symbol">{result.symbol}</td>
      <td className="col-strategy">{result.strategy}</td>
      <td className="col-config">{result.config_id}</td>
      <td className="col-metric num">{formatNum(m.sharpe)}</td>
      <td className="col-metric num">{formatPct(m.cagr)}</td>
      <td className="col-metric num">{formatPct(m.max_drawdown)}</td>
      <td className="col-metric num">{formatNum(m.calmar)}</td>
      <td className="col-metric num">{formatPct(m.win_rate)}</td>
      <td className="col-metric num">{m.num_trades}</td>
    </tr>
  );
}

export function ResultsTable({ isActive }: ResultsTableProps) {
  const {
    results,
    selectedResultId,
    focusedResultIndex,
    sortBy,
    ascending,
    selectResult,
    setSortBy,
    toggleSortOrder,
  } = useAppStore();

  const parentRef = useRef<HTMLDivElement>(null);

  // Use virtualization for large datasets
  const shouldVirtualize = results.length > VIRTUALIZATION_THRESHOLD;

  const virtualizer = useVirtualizer({
    count: results.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
    enabled: shouldVirtualize,
  });

  // Scroll focused row into view when using virtualization
  useEffect(() => {
    if (shouldVirtualize && focusedResultIndex >= 0) {
      virtualizer.scrollToIndex(focusedResultIndex, { align: 'auto' });
    }
  }, [focusedResultIndex, shouldVirtualize, virtualizer]);

  const handleSort = useCallback((metric: SortMetric) => {
    if (metric === sortBy) {
      toggleSortOrder();
    } else {
      setSortBy(metric);
    }
  }, [sortBy, setSortBy, toggleSortOrder]);

  const columns: Array<{ key: SortMetric; label: string; className: string }> = [
    { key: 'sharpe', label: 'Sharpe', className: 'col-metric' },
    { key: 'cagr', label: 'CAGR', className: 'col-metric' },
    { key: 'max_drawdown', label: 'Max DD', className: 'col-metric' },
    { key: 'calmar', label: 'Calmar', className: 'col-metric' },
    { key: 'win_rate', label: 'Win %', className: 'col-metric' },
    { key: 'num_trades', label: 'Trades', className: 'col-metric' },
  ];

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div
      ref={parentRef}
      className={`results-table-container ${isActive ? 'active' : ''}`}
    >
      {results.length === 0 ? (
        <div className="no-results">
          <p>No results yet.</p>
          <p className="hint">Run a sweep to generate results.</p>
        </div>
      ) : (
        <table
          className="results-table"
          role="grid"
          aria-label="Backtest results"
          aria-rowcount={results.length + 1}
        >
          <thead>
            <tr role="row" aria-rowindex={1}>
              <th className="col-symbol" scope="col">Symbol</th>
              <th className="col-strategy" scope="col">Strategy</th>
              <th className="col-config" scope="col">Config</th>
              {columns.map((col) => (
                <MetricHeader
                  key={col.key}
                  metric={col.key}
                  label={col.label}
                  isActive={sortBy === col.key}
                  ascending={ascending}
                  onClick={() => handleSort(col.key)}
                />
              ))}
            </tr>
          </thead>
          {shouldVirtualize ? (
            // Virtualized tbody for large datasets
            <tbody
              style={{
                display: 'block',
                height: `${virtualizer.getTotalSize()}px`,
                position: 'relative',
              }}
            >
              {virtualItems.map((virtualRow) => {
                const result = results[virtualRow.index];
                return (
                  <ResultRowItem
                    key={result.id}
                    result={result}
                    isSelected={result.id === selectedResultId}
                    isFocused={isActive && virtualRow.index === focusedResultIndex}
                    rowIndex={virtualRow.index}
                    style={{
                      position: 'absolute',
                      top: 0,
                      left: 0,
                      width: '100%',
                      height: `${virtualRow.size}px`,
                      transform: `translateY(${virtualRow.start}px)`,
                      display: 'table-row',
                    }}
                    onSelect={() => selectResult(result.id)}
                  />
                );
              })}
            </tbody>
          ) : (
            // Regular tbody for small datasets
            <tbody>
              {results.map((result, idx) => (
                <ResultRowItem
                  key={result.id}
                  result={result}
                  isSelected={result.id === selectedResultId}
                  isFocused={isActive && idx === focusedResultIndex}
                  rowIndex={idx}
                  onSelect={() => selectResult(result.id)}
                />
              ))}
            </tbody>
          )}
        </table>
      )}

      <style>{`
        .results-table-container {
          flex: 1;
          overflow: auto;
          border: 1px solid var(--border);
          border-radius: var(--radius-md);
        }

        .results-table-container.active {
          border-color: var(--cyan);
        }

        .no-results {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          height: 200px;
          color: var(--muted);
        }

        .no-results .hint {
          font-size: var(--font-size-sm);
          margin-top: var(--space-sm);
        }

        .results-table {
          width: 100%;
          border-collapse: collapse;
          font-size: var(--font-size-sm);
        }

        .results-table thead {
          position: sticky;
          top: 0;
          background: var(--bg-secondary);
          z-index: 1;
        }

        .results-table th {
          padding: var(--space-sm) var(--space-md);
          text-align: left;
          font-weight: 600;
          border-bottom: 2px solid var(--border);
          white-space: nowrap;
        }

        .results-table td {
          padding: var(--space-xs) var(--space-md);
          border-bottom: 1px solid var(--border);
        }

        .results-table .num {
          text-align: right;
          font-family: var(--font-mono);
        }

        .result-row {
          cursor: pointer;
          transition: background 0.1s;
        }

        .result-row:hover {
          background: var(--bg-hover);
        }

        .result-row.selected {
          background: var(--bg-active);
        }

        .result-row.focused {
          outline: 2px solid var(--cyan);
          outline-offset: -2px;
        }

        .col-symbol {
          font-weight: 600;
          color: var(--cyan);
        }

        .col-strategy {
          color: var(--purple);
        }

        .col-config {
          color: var(--muted);
          font-size: var(--font-size-xs);
        }
      `}</style>
    </div>
  );
}
