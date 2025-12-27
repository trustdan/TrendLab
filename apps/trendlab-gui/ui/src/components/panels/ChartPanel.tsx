import { VscGraphLine } from 'react-icons/vsc';

export function ChartPanel() {
  return (
    <div className="panel">
      <h1 className="panel-title">Chart</h1>

      <div className="panel-placeholder">
        <VscGraphLine size={48} />
        <h2>Chart Panel</h2>
        <p>Visualize performance</p>
        <ul>
          <li>Equity curves with drawdown overlay</li>
          <li>Candlestick charts with indicators</li>
          <li>Multi-ticker comparisons</li>
          <li>Trade markers and annotations</li>
          <li>Interactive zoom and pan</li>
        </ul>
      </div>

      <style>{`
        .panel-placeholder {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: var(--space-md);
          padding: var(--space-xl);
          text-align: center;
          color: var(--muted);
          border: 2px dashed var(--border);
          border-radius: var(--radius-lg);
          margin-top: var(--space-lg);
        }
        .panel-placeholder h2 {
          font-size: var(--font-size-lg);
          color: var(--fg);
          margin: 0;
        }
        .panel-placeholder p {
          margin: 0;
          font-size: var(--font-size-sm);
        }
        .panel-placeholder ul {
          list-style: none;
          padding: 0;
          margin: var(--space-md) 0 0;
          font-size: var(--font-size-sm);
          text-align: left;
        }
        .panel-placeholder li {
          padding: var(--space-xs) 0;
        }
        .panel-placeholder li::before {
          content: "• ";
          color: var(--blue);
        }
      `}</style>
    </div>
  );
}
