import { VscPlay, VscDebugStop } from 'react-icons/vsc';

interface SweepControlsProps {
  isRunning: boolean;
  canStart: boolean;
  isActive: boolean;
  estimatedConfigs: number;
  onStart: () => void;
  onCancel: () => void;
}

export function SweepControls({
  isRunning,
  canStart,
  isActive,
  estimatedConfigs,
  onStart,
  onCancel,
}: SweepControlsProps) {
  return (
    <div className={`sweep-section controls ${isActive ? 'active' : ''}`}>
      {isRunning ? (
        <button
          className="sweep-btn cancel"
          onClick={onCancel}
        >
          <VscDebugStop />
          <span>Cancel Sweep</span>
        </button>
      ) : (
        <button
          className="sweep-btn start"
          onClick={onStart}
          disabled={!canStart}
        >
          <VscPlay />
          <span>Start Sweep</span>
        </button>
      )}

      <div className="controls-info">
        {isRunning ? (
          <span className="info-running">Sweep in progress... Press Esc to cancel</span>
        ) : canStart ? (
          <span className="info-ready">
            Press Enter to sweep ~{estimatedConfigs.toLocaleString()} configurations
          </span>
        ) : (
          <span className="info-blocked">
            Select symbols and strategies to start
          </span>
        )}
      </div>

      <style>{`
        .sweep-section.controls {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: var(--space-md);
        }
        .sweep-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: var(--space-sm);
          padding: var(--space-md) var(--space-xl);
          font-size: var(--font-size-md);
          font-weight: 600;
          border: none;
          border-radius: var(--radius-md);
          cursor: pointer;
          transition: all 0.15s ease;
        }
        .sweep-btn.start {
          background: var(--green);
          color: var(--bg);
        }
        .sweep-btn.start:hover:not(:disabled) {
          background: var(--green);
          opacity: 0.9;
          transform: translateY(-1px);
        }
        .sweep-btn.start:disabled {
          background: var(--muted);
          cursor: not-allowed;
          opacity: 0.5;
        }
        .sweep-btn.cancel {
          background: var(--red);
          color: var(--bg);
        }
        .sweep-btn.cancel:hover {
          opacity: 0.9;
        }
        .controls-info {
          text-align: center;
          font-size: var(--font-size-sm);
        }
        .info-ready {
          color: var(--muted);
        }
        .info-running {
          color: var(--yellow);
        }
        .info-blocked {
          color: var(--red);
        }
      `}</style>
    </div>
  );
}
