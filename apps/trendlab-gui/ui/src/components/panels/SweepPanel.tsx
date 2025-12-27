import { VscPlay } from 'react-icons/vsc';

export function SweepPanel() {
  return (
    <div className="panel">
      <h1 className="panel-title">Sweep</h1>

      <div className="panel-placeholder">
        <VscPlay size={48} />
        <h2>Sweep Panel</h2>
        <p>Run parameter sweeps</p>
        <ul>
          <li>Review selected symbols and strategies</li>
          <li>Configure sweep depth</li>
          <li>Set cost model (fees, slippage)</li>
          <li>Start/cancel sweep jobs</li>
          <li>Monitor progress in real-time</li>
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
