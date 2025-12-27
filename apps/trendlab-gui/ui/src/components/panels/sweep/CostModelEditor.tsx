import { useState } from 'react';
import type { CostModel } from '../../../types/sweep';

interface CostModelEditorProps {
  costModel: CostModel;
  isActive: boolean;
  onChange: (costModel: CostModel) => void;
}

export function CostModelEditor({
  costModel,
  isActive,
  onChange,
}: CostModelEditorProps) {
  const [localFees, setLocalFees] = useState(costModel.fees_bps.toString());
  const [localSlippage, setLocalSlippage] = useState(costModel.slippage_bps.toString());

  const handleFeesChange = (value: string) => {
    setLocalFees(value);
    const parsed = parseFloat(value);
    if (!isNaN(parsed) && parsed >= 0) {
      onChange({ ...costModel, fees_bps: parsed });
    }
  };

  const handleSlippageChange = (value: string) => {
    setLocalSlippage(value);
    const parsed = parseFloat(value);
    if (!isNaN(parsed) && parsed >= 0) {
      onChange({ ...costModel, slippage_bps: parsed });
    }
  };

  const totalCostBps = costModel.fees_bps + costModel.slippage_bps;
  const totalCostPct = (totalCostBps / 100).toFixed(3);

  return (
    <div className={`sweep-section ${isActive ? 'active' : ''}`}>
      <h3 className="sweep-section-title">Cost Model</h3>

      <div className="cost-inputs">
        <div className="cost-row">
          <label htmlFor="fees">Trading Fees</label>
          <div className="cost-input-group">
            <input
              id="fees"
              type="number"
              min="0"
              step="0.5"
              value={localFees}
              onChange={(e) => handleFeesChange(e.target.value)}
              className="cost-input"
            />
            <span className="cost-unit">bps</span>
          </div>
        </div>

        <div className="cost-row">
          <label htmlFor="slippage">Slippage</label>
          <div className="cost-input-group">
            <input
              id="slippage"
              type="number"
              min="0"
              step="0.5"
              value={localSlippage}
              onChange={(e) => handleSlippageChange(e.target.value)}
              className="cost-input"
            />
            <span className="cost-unit">bps</span>
          </div>
        </div>
      </div>

      <div className="cost-summary">
        <span className="cost-label">Total Round-Trip Cost:</span>
        <span className="cost-value">
          {totalCostBps.toFixed(1)} bps ({totalCostPct}%)
        </span>
      </div>

      <div className="cost-hint">
        1 bp = 0.01% per trade. Costs applied per round-trip (entry + exit).
      </div>

      <style>{`
        .cost-inputs {
          display: flex;
          flex-direction: column;
          gap: var(--space-sm);
        }
        .cost-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }
        .cost-row label {
          color: var(--fg);
        }
        .cost-input-group {
          display: flex;
          align-items: center;
          gap: var(--space-xs);
        }
        .cost-input {
          width: 80px;
          padding: var(--space-xs) var(--space-sm);
          background: var(--bg);
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          color: var(--fg);
          font-size: var(--font-size-sm);
          text-align: right;
        }
        .cost-input:focus {
          outline: none;
          border-color: var(--blue);
        }
        .cost-unit {
          color: var(--muted);
          font-size: var(--font-size-sm);
          width: 30px;
        }
        .cost-summary {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-top: var(--space-sm);
          padding-top: var(--space-sm);
          border-top: 1px solid var(--border);
        }
        .cost-label {
          color: var(--muted);
        }
        .cost-value {
          color: var(--cyan);
          font-weight: 500;
        }
        .cost-hint {
          margin-top: var(--space-sm);
          font-size: var(--font-size-xs);
          color: var(--muted);
        }
      `}</style>
    </div>
  );
}
