interface ProgressBarProps {
  current: number;
  total: number;
  message: string;
  symbol: string;
  strategy: string;
}

export function ProgressBar({
  current,
  total,
  message,
  symbol,
  strategy,
}: ProgressBarProps) {
  const percentage = total > 0 ? Math.round((current / total) * 100) : 0;
  const progressWidth = total > 0 ? (current / total) * 100 : 0;

  return (
    <div className="progress-container">
      <div className="progress-header">
        <span className="progress-label">Progress</span>
        <span className="progress-stats">
          {current.toLocaleString()} / {total.toLocaleString()} ({percentage}%)
        </span>
      </div>

      <div className="progress-bar">
        <div
          className="progress-fill"
          style={{ width: `${progressWidth}%` }}
        />
      </div>

      <div className="progress-details">
        {symbol && strategy ? (
          <span className="progress-current">
            {symbol} × {strategy}
          </span>
        ) : (
          <span className="progress-message">{message || 'Processing...'}</span>
        )}
      </div>

      <style>{`
        .progress-container {
          padding: var(--space-md);
          background: var(--bg-secondary);
          border: 1px solid var(--yellow);
          border-radius: var(--radius-md);
          margin-bottom: var(--space-md);
        }
        .progress-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: var(--space-sm);
        }
        .progress-label {
          font-weight: 600;
          color: var(--fg);
        }
        .progress-stats {
          color: var(--yellow);
          font-family: monospace;
        }
        .progress-bar {
          height: 8px;
          background: var(--bg);
          border-radius: var(--radius-sm);
          overflow: hidden;
        }
        .progress-fill {
          height: 100%;
          background: linear-gradient(90deg, var(--yellow), var(--green));
          transition: width 0.2s ease;
        }
        .progress-details {
          margin-top: var(--space-sm);
          font-size: var(--font-size-sm);
          color: var(--muted);
        }
        .progress-current {
          color: var(--cyan);
        }
        .progress-message {
          font-style: italic;
        }
      `}</style>
    </div>
  );
}
