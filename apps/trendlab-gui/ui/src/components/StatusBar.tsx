import { useAppStore } from '../store';
import { getKeyboardHints } from '../hooks';

export function StatusBar() {
  const { activePanel, operationState, statusMessage, activeJobId, jobs } = useAppStore();

  const activeJob = activeJobId ? jobs[activeJobId] : null;
  const hints = getKeyboardHints(activePanel);

  return (
    <footer className="app-statusbar">
      {/* Operation state indicator */}
      <div className="status-indicator">
        <span className={`status-dot ${operationState}`} />
        <span className="status-label">
          {operationState === 'idle' && 'Ready'}
          {operationState === 'loading' && 'Working...'}
          {operationState === 'success' && 'Done'}
          {operationState === 'error' && 'Error'}
        </span>
      </div>

      {/* Active job progress */}
      {activeJob && activeJob.status === 'running' && activeJob.progress && (
        <div className="status-job">
          <span className="status-job-type">{activeJob.type}</span>
          <div className="status-progress-bar">
            <div
              className="status-progress-fill"
              style={{
                width: `${(activeJob.progress.current / activeJob.progress.total) * 100}%`,
              }}
            />
          </div>
          <span className="status-progress-text">
            {activeJob.progress.current}/{activeJob.progress.total}
          </span>
        </div>
      )}

      {/* Status message */}
      {statusMessage && (
        <div className={`status-message text-${statusMessage.type === 'info' ? 'muted' : statusMessage.type}`}>
          {statusMessage.text}
        </div>
      )}

      {/* Spacer */}
      <div style={{ flex: 1 }} />

      {/* Context-aware keyboard hints */}
      <div className="status-hints">
        {hints.slice(0, 4).map((hint, i) => (
          <span key={i} className="status-hint-item">
            {hint}
          </span>
        ))}
        <span className="status-hint-item">
          <kbd className="kbd">?</kbd> help
        </span>
      </div>

      <style>{`
        .status-indicator {
          display: flex;
          align-items: center;
          gap: var(--space-xs);
        }
        .status-label {
          font-size: var(--font-size-xs);
        }
        .status-job {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          padding: 0 var(--space-sm);
          border-left: 1px solid var(--border);
        }
        .status-job-type {
          font-size: var(--font-size-xs);
          color: var(--blue);
          text-transform: uppercase;
          letter-spacing: 0.05em;
        }
        .status-progress-bar {
          width: 80px;
          height: 4px;
          background: var(--bg-dark);
          border-radius: 2px;
          overflow: hidden;
        }
        .status-progress-fill {
          height: 100%;
          background: var(--blue);
          transition: width 100ms ease;
        }
        .status-progress-text {
          font-family: var(--font-mono);
          font-size: var(--font-size-xs);
          color: var(--muted);
          min-width: 40px;
        }
        .status-message {
          font-size: var(--font-size-xs);
          padding: 0 var(--space-sm);
          border-left: 1px solid var(--border);
        }
        .status-hints {
          display: flex;
          align-items: center;
          gap: var(--space-md);
        }
        .status-hint-item {
          font-size: var(--font-size-xs);
          color: var(--muted);
        }
        .status-hint-item .kbd {
          margin-right: 4px;
        }
      `}</style>
    </footer>
  );
}
