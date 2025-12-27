import { useState, useCallback } from 'react';
import { VscRocket, VscDebugStop } from 'react-icons/vsc';
import { useAppStore } from '../../../store';

export function YoloControls() {
  const {
    yoloEnabled,
    yoloPhase,
    yoloIteration,
    yoloRandomizationPct,
    yoloTotalConfigsTested,
    yoloCompletedConfigs,
    yoloTotalConfigs,
    yoloLoading,
    yoloError,
    startYolo,
    stopYolo,
    setRandomizationPct,
  } = useAppStore();

  const [localPct, setLocalPct] = useState(yoloRandomizationPct * 100);

  const handlePctChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const value = parseFloat(e.target.value);
      setLocalPct(value);
      setRandomizationPct(value / 100);
    },
    [setRandomizationPct]
  );

  const handleStart = useCallback(() => {
    startYolo(yoloRandomizationPct);
  }, [startYolo, yoloRandomizationPct]);

  const isRunning = yoloEnabled && yoloPhase === 'sweeping';
  const progressPct =
    yoloTotalConfigs > 0 ? (yoloCompletedConfigs / yoloTotalConfigs) * 100 : 0;

  return (
    <div className="yolo-controls">
      <div className="yolo-header">
        <span className="yolo-title">YOLO Mode</span>
        <span className="yolo-subtitle">Continuous auto-optimization</span>
      </div>

      {yoloError && <div className="yolo-error">{yoloError}</div>}

      {isRunning && (
        <div className="yolo-progress">
          <div className="progress-bar">
            <div
              className="progress-fill"
              style={{ width: `${progressPct}%` }}
            />
          </div>
          <div className="progress-text">
            Iteration {yoloIteration} - {yoloCompletedConfigs}/{yoloTotalConfigs}{' '}
            configs
          </div>
        </div>
      )}

      <div className="yolo-slider-row">
        <label htmlFor="yolo-pct">Randomization:</label>
        <input
          id="yolo-pct"
          type="range"
          min="5"
          max="50"
          step="5"
          value={localPct}
          onChange={handlePctChange}
          disabled={isRunning}
        />
        <span className="pct-value">{localPct}%</span>
      </div>

      <div className="yolo-stats">
        <span>Total tested: {yoloTotalConfigsTested.toLocaleString()}</span>
        {yoloIteration > 0 && <span>Iterations: {yoloIteration}</span>}
      </div>

      <div className="yolo-button-row">
        {isRunning ? (
          <button
            className="yolo-btn stop"
            onClick={stopYolo}
            disabled={yoloLoading}
          >
            <VscDebugStop />
            <span>Stop YOLO</span>
          </button>
        ) : (
          <button
            className="yolo-btn start"
            onClick={handleStart}
            disabled={yoloLoading}
          >
            <VscRocket />
            <span>Start YOLO</span>
          </button>
        )}
      </div>

      <div className="yolo-hint">
        {isRunning
          ? 'Press Y or Escape to stop'
          : 'Press Y to toggle YOLO mode'}
      </div>

      <style>{`
        .yolo-controls {
          padding: var(--space-md);
          background: var(--surface);
          border-radius: var(--radius-md);
          border: 1px solid var(--border);
          margin-top: var(--space-lg);
        }
        .yolo-header {
          display: flex;
          flex-direction: column;
          margin-bottom: var(--space-md);
        }
        .yolo-title {
          font-size: var(--font-size-lg);
          font-weight: 600;
          color: var(--yellow);
        }
        .yolo-subtitle {
          font-size: var(--font-size-sm);
          color: var(--muted);
        }
        .yolo-error {
          padding: var(--space-sm);
          background: rgba(255, 100, 100, 0.1);
          border: 1px solid var(--red);
          border-radius: var(--radius-sm);
          color: var(--red);
          margin-bottom: var(--space-md);
          font-size: var(--font-size-sm);
        }
        .yolo-progress {
          margin-bottom: var(--space-md);
        }
        .progress-bar {
          height: 6px;
          background: var(--muted);
          border-radius: 3px;
          overflow: hidden;
          margin-bottom: var(--space-xs);
        }
        .progress-fill {
          height: 100%;
          background: var(--yellow);
          transition: width 0.2s ease;
        }
        .progress-text {
          font-size: var(--font-size-sm);
          color: var(--fg);
          text-align: center;
        }
        .yolo-slider-row {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          margin-bottom: var(--space-md);
        }
        .yolo-slider-row label {
          font-size: var(--font-size-sm);
          color: var(--muted);
          min-width: 100px;
        }
        .yolo-slider-row input[type="range"] {
          flex: 1;
          accent-color: var(--yellow);
        }
        .pct-value {
          min-width: 40px;
          text-align: right;
          font-size: var(--font-size-sm);
          color: var(--fg);
        }
        .yolo-stats {
          display: flex;
          gap: var(--space-lg);
          font-size: var(--font-size-sm);
          color: var(--muted);
          margin-bottom: var(--space-md);
        }
        .yolo-button-row {
          display: flex;
          justify-content: center;
          margin-bottom: var(--space-sm);
        }
        .yolo-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: var(--space-sm);
          padding: var(--space-sm) var(--space-lg);
          font-size: var(--font-size-md);
          font-weight: 600;
          border: none;
          border-radius: var(--radius-md);
          cursor: pointer;
          transition: all 0.15s ease;
        }
        .yolo-btn.start {
          background: var(--yellow);
          color: var(--bg);
        }
        .yolo-btn.start:hover:not(:disabled) {
          opacity: 0.9;
          transform: translateY(-1px);
        }
        .yolo-btn.start:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
        .yolo-btn.stop {
          background: var(--red);
          color: var(--bg);
        }
        .yolo-btn.stop:hover:not(:disabled) {
          opacity: 0.9;
        }
        .yolo-hint {
          text-align: center;
          font-size: var(--font-size-xs);
          color: var(--muted);
        }
      `}</style>
    </div>
  );
}
