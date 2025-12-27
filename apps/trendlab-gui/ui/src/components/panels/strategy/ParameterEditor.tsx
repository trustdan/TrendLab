import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../../store';
import { ParamRow } from './ParamRow';
import type { StrategyDefaults, ParamValue } from '../../../types';

interface ParameterEditorProps {
  isFocused: boolean;
}

export function ParameterEditor({ isFocused }: ParameterEditorProps) {
  const {
    getFocusedStrategy,
    getCurrentParamDefs,
    getCurrentParams,
    focusedParamIndex,
    focus,
    setParamDefs,
    setStrategyParams,
    adjustParam,
  } = useAppStore();

  const strategy = getFocusedStrategy();
  const paramDefs = getCurrentParamDefs();
  const params = getCurrentParams();

  const isParameterFocused = focus === 'parameters' && isFocused;

  // Load parameter definitions when strategy changes
  useEffect(() => {
    if (!strategy) return;

    // Check if we already have the defs
    if (paramDefs.length > 0) return;

    // Fetch from backend
    invoke<StrategyDefaults>('get_strategy_defaults', { strategyId: strategy.id })
      .then((defaults) => {
        setParamDefs(strategy.id, defaults.params);
        setStrategyParams(strategy.id, defaults.values);
      })
      .catch((err) => {
        console.error('Failed to load strategy defaults:', err);
      });
  }, [strategy?.id, paramDefs.length, setParamDefs, setStrategyParams]);

  if (!strategy) {
    return (
      <div className="parameter-editor empty">
        <div className="empty-state">
          <p>Select a strategy to view parameters</p>
          <p className="hint">Use j/k to navigate, Enter to select</p>
        </div>

        <style>{`
          .parameter-editor {
            display: flex;
            flex-direction: column;
            height: 100%;
            padding: var(--space-md);
          }

          .parameter-editor.empty {
            justify-content: center;
            align-items: center;
          }

          .empty-state {
            text-align: center;
            color: var(--muted);
          }

          .empty-state .hint {
            font-size: var(--font-size-sm);
            margin-top: var(--space-sm);
          }
        `}</style>
      </div>
    );
  }

  const isEditable = strategy.has_params;

  return (
    <div className="parameter-editor">
      <div className="parameter-header">
        <span className="strategy-name">{strategy.name}</span>
        {!isEditable && <span className="fixed-label">Fixed Parameters</span>}
        {isEditable && <span className="keyboard-hint">h/l: adjust values</span>}
      </div>

      <div className="parameter-list">
        {paramDefs.length === 0 ? (
          <div className="loading">Loading parameters...</div>
        ) : (
          paramDefs.map((def, index) => {
            const value = params.values[def.key] ?? def.default;
            const isRowFocused = isParameterFocused && focusedParamIndex === index;

            return (
              <ParamRow
                key={def.key}
                def={def}
                value={value as ParamValue}
                isFocused={isRowFocused}
                isEditable={isEditable}
                onAdjust={adjustParam}
              />
            );
          })
        )}
      </div>

      {isEditable && paramDefs.length > 0 && (
        <div className="parameter-info">
          <div className="info-label">Default:</div>
          <div className="info-value">
            {paramDefs.map((d) => `${d.label}: ${d.default}`).join(', ')}
          </div>
        </div>
      )}

      <style>{`
        .parameter-editor {
          display: flex;
          flex-direction: column;
          height: 100%;
        }

        .parameter-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: var(--space-sm) var(--space-md);
          border-bottom: 1px solid var(--border);
        }

        .strategy-name {
          font-weight: 600;
          color: var(--cyan);
        }

        .fixed-label {
          font-size: var(--font-size-sm);
          color: var(--muted);
          background: var(--bg-hover);
          padding: 2px 8px;
          border-radius: var(--radius-xs);
        }

        .keyboard-hint {
          font-size: var(--font-size-xs);
          color: var(--muted);
        }

        .parameter-list {
          flex: 1;
          overflow-y: auto;
          padding: var(--space-md);
        }

        .loading {
          color: var(--muted);
          text-align: center;
          padding: var(--space-lg);
        }

        .parameter-info {
          border-top: 1px solid var(--border);
          padding: var(--space-sm) var(--space-md);
          display: flex;
          gap: var(--space-sm);
          font-size: var(--font-size-sm);
        }

        .info-label {
          color: var(--muted);
        }

        .info-value {
          color: var(--fg);
        }
      `}</style>
    </div>
  );
}
