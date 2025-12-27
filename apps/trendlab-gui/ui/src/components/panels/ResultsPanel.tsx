import { VscTable } from 'react-icons/vsc';

export function ResultsPanel() {
  return (
    <div className="panel">
      <h1 className="panel-title">Results</h1>

      <div className="panel-placeholder">
        <VscTable size={48} />
        <h2>Results Panel</h2>
        <p>Analyze backtest results</p>
        <ul>
          <li>View results table with metrics</li>
          <li>Sort by CAGR, Sharpe, drawdown, etc.</li>
          <li>Filter by strategy or ticker</li>
          <li>Select configurations for charting</li>
          <li>Export strategy artifacts</li>
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
