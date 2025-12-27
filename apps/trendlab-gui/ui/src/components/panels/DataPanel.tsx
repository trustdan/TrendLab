import { VscDatabase } from 'react-icons/vsc';

export function DataPanel() {
  return (
    <div className="panel">
      <h1 className="panel-title">Data</h1>

      <div className="panel-placeholder">
        <VscDatabase size={48} />
        <h2>Data Panel</h2>
        <p>View and manage market data</p>
        <ul>
          <li>View cached symbols</li>
          <li>Search for new tickers</li>
          <li>Fetch historical data from Yahoo Finance</li>
          <li>Select tickers for backtesting</li>
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
