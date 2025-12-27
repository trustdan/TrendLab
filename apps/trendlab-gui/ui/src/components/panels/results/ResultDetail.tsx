import { useAppStore } from '../../../store';

interface ResultDetailProps {
  isActive: boolean;
}

/** Format a number as percentage */
function formatPct(value: number): string {
  return `${(value * 100).toFixed(2)}%`;
}

/** Format a number with fixed decimals */
function formatNum(value: number, decimals: number = 2): string {
  return value.toFixed(decimals);
}

export function ResultDetail({ isActive }: ResultDetailProps) {
  const { getSelectedResult } = useAppStore();
  const result = getSelectedResult();

  if (!result) {
    return (
      <div className={`result-detail empty ${isActive ? 'active' : ''}`}>
        <p>Select a result to view details</p>

        <style>{`
          .result-detail {
            padding: var(--space-md);
            border: 1px solid var(--border);
            border-radius: var(--radius-md);
            background: var(--bg-secondary);
          }

          .result-detail.active {
            border-color: var(--cyan);
          }

          .result-detail.empty {
            display: flex;
            align-items: center;
            justify-content: center;
            color: var(--muted);
            min-height: 150px;
          }
        `}</style>
      </div>
    );
  }

  const m = result.metrics;

  return (
    <div className={`result-detail ${isActive ? 'active' : ''}`}>
      <div className="detail-header">
        <span className="symbol">{result.symbol}</span>
        <span className="strategy">{result.strategy}</span>
        <span className="config">{result.config_id}</span>
      </div>

      <div className="metrics-grid">
        <div className="metric-item">
          <span className="label">Sharpe</span>
          <span className="value">{formatNum(m.sharpe)}</span>
        </div>
        <div className="metric-item">
          <span className="label">CAGR</span>
          <span className="value">{formatPct(m.cagr)}</span>
        </div>
        <div className="metric-item">
          <span className="label">Sortino</span>
          <span className="value">{formatNum(m.sortino)}</span>
        </div>
        <div className="metric-item">
          <span className="label">Calmar</span>
          <span className="value">{formatNum(m.calmar)}</span>
        </div>
        <div className="metric-item">
          <span className="label">Max DD</span>
          <span className="value negative">{formatPct(m.max_drawdown)}</span>
        </div>
        <div className="metric-item">
          <span className="label">Win Rate</span>
          <span className="value">{formatPct(m.win_rate)}</span>
        </div>
        <div className="metric-item">
          <span className="label">Profit Factor</span>
          <span className="value">{formatNum(m.profit_factor)}</span>
        </div>
        <div className="metric-item">
          <span className="label">Trades</span>
          <span className="value">{m.num_trades}</span>
        </div>
        <div className="metric-item">
          <span className="label">Total Return</span>
          <span className={`value ${m.total_return >= 0 ? 'positive' : 'negative'}`}>
            {formatPct(m.total_return)}
          </span>
        </div>
        <div className="metric-item">
          <span className="label">Turnover</span>
          <span className="value">{formatNum(m.turnover)}x</span>
        </div>
      </div>

      {result.equity_curve.length > 0 && (
        <div className="equity-sparkline">
          <span className="label">Equity Curve</span>
          <div className="sparkline-placeholder">
            {/* TODO: Add actual sparkline chart */}
            <span className="mini-chart">[chart]</span>
          </div>
        </div>
      )}

      <style>{`
        .result-detail {
          padding: var(--space-md);
          border: 1px solid var(--border);
          border-radius: var(--radius-md);
          background: var(--bg-secondary);
        }

        .result-detail.active {
          border-color: var(--cyan);
        }

        .detail-header {
          display: flex;
          gap: var(--space-md);
          margin-bottom: var(--space-md);
          padding-bottom: var(--space-sm);
          border-bottom: 1px solid var(--border);
        }

        .detail-header .symbol {
          font-weight: 700;
          font-size: var(--font-size-lg);
          color: var(--cyan);
        }

        .detail-header .strategy {
          color: var(--purple);
          font-weight: 500;
        }

        .detail-header .config {
          color: var(--muted);
          font-size: var(--font-size-sm);
        }

        .metrics-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
          gap: var(--space-md);
        }

        .metric-item {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .metric-item .label {
          font-size: var(--font-size-xs);
          color: var(--muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .metric-item .value {
          font-family: var(--font-mono);
          font-size: var(--font-size-md);
          font-weight: 600;
        }

        .metric-item .value.positive {
          color: var(--green);
        }

        .metric-item .value.negative {
          color: var(--red);
        }

        .equity-sparkline {
          margin-top: var(--space-md);
          padding-top: var(--space-md);
          border-top: 1px solid var(--border);
        }

        .equity-sparkline .label {
          font-size: var(--font-size-xs);
          color: var(--muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .sparkline-placeholder {
          height: 60px;
          margin-top: var(--space-xs);
          display: flex;
          align-items: center;
          justify-content: center;
          background: var(--bg);
          border-radius: var(--radius-sm);
          color: var(--muted);
        }
      `}</style>
    </div>
  );
}
