import type { TradeMarker } from '../../types';
import { CHART_COLORS } from '../../types';

/**
 * Note: In Lightweight Charts v5, setMarkers() was removed from series.
 * Markers require the markers plugin. For now, we display trades in a table only.
 * Chart markers can be added later using the v5 markers primitive API.
 */
interface UseTradeMarkersProps {
  /** Trade marker data */
  trades: TradeMarker[];
  /** Whether markers are enabled */
  enabled?: boolean;
}

/**
 * Stub hook for trade markers.
 * In v5, chart markers require the markers plugin.
 * For now, trades are displayed in TradesTable component only.
 */
export function useTradeMarkers(_props: UseTradeMarkersProps): void {
  // No-op: markers plugin not yet implemented for v5
  // TODO: Implement using lightweight-charts markers primitive when needed
}

/**
 * Format marker tooltip text.
 */
function formatMarkerText(trade: TradeMarker): string {
  const action = trade.markerType === 'entry' ? 'Enter' : 'Exit';
  const direction = trade.direction === 'long' ? 'Long' : 'Short';
  const price = `$${trade.price.toFixed(2)}`;

  if (trade.markerType === 'exit' && trade.pnl !== undefined) {
    const pnlStr = trade.pnl >= 0 ? `+$${trade.pnl.toFixed(2)}` : `-$${Math.abs(trade.pnl).toFixed(2)}`;
    return `${action} ${direction} @ ${price} (${pnlStr})`;
  }

  return `${action} ${direction} @ ${price}`;
}

/**
 * Component to display trade markers in a trades table below the chart.
 */
interface TradesTableProps {
  trades: TradeMarker[];
  onTradeClick?: (trade: TradeMarker) => void;
}

export function TradesTable({ trades, onTradeClick }: TradesTableProps) {
  if (trades.length === 0) {
    return (
      <div style={{ padding: '12px', color: 'var(--text-tertiary)', fontSize: '12px' }}>
        No trades to display
      </div>
    );
  }

  // Group trades into pairs (entry + exit)
  const tradePairs: Array<{ entry: TradeMarker; exit?: TradeMarker }> = [];
  let currentEntry: TradeMarker | null = null;

  for (const trade of trades) {
    if (trade.markerType === 'entry') {
      if (currentEntry) {
        // Orphan entry without exit
        tradePairs.push({ entry: currentEntry });
      }
      currentEntry = trade;
    } else if (trade.markerType === 'exit' && currentEntry) {
      tradePairs.push({ entry: currentEntry, exit: trade });
      currentEntry = null;
    }
  }

  // Add any remaining entry
  if (currentEntry) {
    tradePairs.push({ entry: currentEntry });
  }

  return (
    <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '11px' }}>
      <thead>
        <tr style={{ borderBottom: '1px solid var(--border)' }}>
          <th style={thStyle}>Dir</th>
          <th style={thStyle}>Entry Date</th>
          <th style={thStyle}>Entry Price</th>
          <th style={thStyle}>Exit Date</th>
          <th style={thStyle}>Exit Price</th>
          <th style={thStyle}>P&L</th>
        </tr>
      </thead>
      <tbody>
        {tradePairs.map((pair, i) => {
          const pnl = pair.exit?.pnl ?? 0;
          const pnlColor = pnl >= 0 ? CHART_COLORS.green : CHART_COLORS.red;

          return (
            <tr
              key={i}
              style={{ borderBottom: '1px solid var(--border)', cursor: 'pointer' }}
              onClick={() => onTradeClick?.(pair.entry)}
            >
              <td style={tdStyle}>
                <span
                  style={{
                    color: pair.entry.direction === 'long' ? CHART_COLORS.green : CHART_COLORS.red,
                  }}
                >
                  {pair.entry.direction === 'long' ? '▲' : '▼'}
                </span>
              </td>
              <td style={tdStyle}>{formatDate(pair.entry.time)}</td>
              <td style={tdStyle}>${pair.entry.price.toFixed(2)}</td>
              <td style={tdStyle}>{pair.exit ? formatDate(pair.exit.time) : '—'}</td>
              <td style={tdStyle}>{pair.exit ? `$${pair.exit.price.toFixed(2)}` : '—'}</td>
              <td style={{ ...tdStyle, color: pnlColor }}>
                {pair.exit ? (pnl >= 0 ? '+' : '') + `$${pnl.toFixed(2)}` : '—'}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

const thStyle: React.CSSProperties = {
  textAlign: 'left',
  padding: '8px',
  color: 'var(--text-tertiary)',
  fontWeight: 500,
};

const tdStyle: React.CSSProperties = {
  padding: '8px',
  color: 'var(--text-secondary)',
};

function formatDate(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: '2-digit',
  });
}

// Export formatMarkerText for potential future use
export { formatMarkerText };
