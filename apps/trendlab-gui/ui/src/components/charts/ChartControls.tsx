import { memo } from 'react';
import type { ChartMode, ChartOverlays } from '../../types';
import { CHART_MODE_LABELS } from '../../types';
import styles from './ChartControls.module.css';

interface ChartControlsProps {
  /** Current chart mode */
  mode: ChartMode;
  /** Available modes */
  availableModes?: ChartMode[];
  /** Current overlay settings */
  overlays: ChartOverlays;
  /** Callback when mode changes */
  onModeChange: (mode: ChartMode) => void;
  /** Callback when overlay is toggled */
  onOverlayToggle: (overlay: keyof ChartOverlays) => void;
  /** Whether to show overlay controls */
  showOverlayControls?: boolean;
}

/**
 * Chart controls for mode switching and overlay toggles.
 */
export const ChartControls = memo(function ChartControls({
  mode,
  availableModes = ['candlestick', 'equity', 'multi_ticker', 'portfolio', 'strategy_comparison'],
  overlays,
  onModeChange,
  onOverlayToggle,
  showOverlayControls = true,
}: ChartControlsProps) {
  return (
    <div className={styles.container}>
      {/* Mode selector */}
      <div className={styles.section}>
        <span className={styles.label}>Mode</span>
        <div className={styles.modeButtons}>
          {availableModes.map((m) => (
            <button
              key={m}
              className={`${styles.modeBtn} ${mode === m ? styles.active : ''}`}
              onClick={() => onModeChange(m)}
              title={CHART_MODE_LABELS[m]}
            >
              {getModeIcon(m)}
              <span className={styles.modeLabel}>{getShortLabel(m)}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Overlay toggles */}
      {showOverlayControls && (
        <div className={styles.section}>
          <span className={styles.label}>Overlays</span>
          <div className={styles.toggleButtons}>
            <button
              className={`${styles.toggleBtn} ${overlays.volume ? styles.active : ''}`}
              onClick={() => onOverlayToggle('volume')}
              title="Toggle volume"
            >
              Vol
            </button>
            <button
              className={`${styles.toggleBtn} ${overlays.drawdown ? styles.active : ''}`}
              onClick={() => onOverlayToggle('drawdown')}
              title="Toggle drawdown"
            >
              DD
            </button>
            <button
              className={`${styles.toggleBtn} ${overlays.trades ? styles.active : ''}`}
              onClick={() => onOverlayToggle('trades')}
              title="Toggle trade markers"
            >
              Trades
            </button>
            <button
              className={`${styles.toggleBtn} ${overlays.crosshair ? styles.active : ''}`}
              onClick={() => onOverlayToggle('crosshair')}
              title="Toggle crosshair"
            >
              +
            </button>
          </div>
        </div>
      )}
    </div>
  );
});

/** Get icon for chart mode */
function getModeIcon(mode: ChartMode): string {
  switch (mode) {
    case 'candlestick':
      return '📊';
    case 'equity':
      return '📈';
    case 'multi_ticker':
      return '📉';
    case 'portfolio':
      return '💼';
    case 'strategy_comparison':
      return '⚔️';
    default:
      return '📊';
  }
}

/** Get short label for chart mode */
function getShortLabel(mode: ChartMode): string {
  switch (mode) {
    case 'candlestick':
      return 'OHLC';
    case 'equity':
      return 'Equity';
    case 'multi_ticker':
      return 'Multi';
    case 'portfolio':
      return 'Port';
    case 'strategy_comparison':
      return 'Strat';
    default:
      return mode;
  }
}
