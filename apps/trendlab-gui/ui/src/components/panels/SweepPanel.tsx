import { useEffect, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../../store';
import {
  SelectionSummary,
  DepthSelector,
  CostModelEditor,
  DateRangeEditor,
  SweepControls,
  ProgressBar,
} from './sweep';
import type { SweepProgressPayload, SweepCompletePayload } from '../../types/sweep';

export function SweepPanel() {
  const {
    // State
    selectionSummary,
    depthOptions,
    depth,
    costModel,
    dateRange,
    isRunning,
    progress,
    sweepFocus,
    focusedDepthIndex,
    error,

    // Actions
    loadSweepState,
    loadSelectionSummary,
    loadDepthOptions,
    setDepth,
    setCostModel,
    setDateRange,
    startSweep,
    cancelSweep,
    updateProgress,
    handleComplete,
    handleCancelled,
    setSweepFocus,
    moveFocus,
    selectFocusedDepth,
  } = useAppStore();

  // Load initial data
  useEffect(() => {
    loadSweepState();
    loadSelectionSummary();
    loadDepthOptions();
  }, [loadSweepState, loadSelectionSummary, loadDepthOptions]);

  // Listen for sweep events
  useEffect(() => {
    const unsubscribers: Array<() => void> = [];

    listen<{ payload: SweepProgressPayload }>('sweep:progress', (event) => {
      updateProgress(event.payload.payload);
    }).then((unsub) => unsubscribers.push(unsub));

    listen<{ payload: SweepCompletePayload }>('sweep:complete', (event) => {
      handleComplete(event.payload.payload);
      // Reload selection summary after sweep
      loadSelectionSummary();
    }).then((unsub) => unsubscribers.push(unsub));

    listen('sweep:cancelled', () => {
      handleCancelled();
    }).then((unsub) => unsubscribers.push(unsub));

    return () => {
      unsubscribers.forEach((unsub) => unsub());
    };
  }, [updateProgress, handleComplete, handleCancelled, loadSelectionSummary]);

  // Keyboard navigation
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Ignore if focus is in an input
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      ) {
        return;
      }

      switch (e.key) {
        case 'j':
        case 'ArrowDown':
          e.preventDefault();
          moveFocus('down');
          break;
        case 'k':
        case 'ArrowUp':
          e.preventDefault();
          moveFocus('up');
          break;
        case ' ':
          e.preventDefault();
          if (sweepFocus === 'depth') {
            selectFocusedDepth();
          }
          break;
        case 'Enter':
          e.preventDefault();
          if (sweepFocus === 'controls' && !isRunning) {
            startSweep();
          }
          break;
        case 'Escape':
          if (isRunning) {
            e.preventDefault();
            cancelSweep();
          }
          break;
      }
    },
    [sweepFocus, isRunning, moveFocus, selectFocusedDepth, startSweep, cancelSweep]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  const canStart =
    !!selectionSummary &&
    selectionSummary.symbol_count > 0 &&
    selectionSummary.strategy_count > 0 &&
    !isRunning;

  return (
    <div className="panel sweep-panel">
      <h1 className="panel-title">Sweep</h1>

      {error && (
        <div className="sweep-error">
          {error}
        </div>
      )}

      {isRunning && progress && (
        <ProgressBar
          current={progress.current}
          total={progress.total}
          message={progress.message}
          symbol={progress.symbol}
          strategy={progress.strategy}
        />
      )}

      <div className="sweep-content">
        <SelectionSummary
          summary={selectionSummary}
          isActive={sweepFocus === 'summary'}
        />

        <DepthSelector
          options={depthOptions}
          selected={depth}
          focusedIndex={focusedDepthIndex}
          isActive={sweepFocus === 'depth'}
          onSelect={setDepth}
        />

        <div className="sweep-row">
          <CostModelEditor
            costModel={costModel}
            isActive={sweepFocus === 'cost'}
            onChange={setCostModel}
          />

          <DateRangeEditor
            dateRange={dateRange}
            isActive={sweepFocus === 'dates'}
            onChange={setDateRange}
          />
        </div>

        <SweepControls
          isRunning={isRunning}
          canStart={canStart}
          isActive={sweepFocus === 'controls'}
          estimatedConfigs={selectionSummary?.estimated_configs ?? 0}
          onStart={startSweep}
          onCancel={cancelSweep}
        />
      </div>

      <style>{`
        .sweep-panel {
          display: flex;
          flex-direction: column;
          height: 100%;
        }
        .sweep-error {
          padding: var(--space-sm) var(--space-md);
          background: rgba(255, 100, 100, 0.1);
          border: 1px solid var(--red);
          border-radius: var(--radius-sm);
          color: var(--red);
          margin-bottom: var(--space-md);
        }
        .sweep-content {
          flex: 1;
          overflow-y: auto;
        }
        .sweep-row {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: var(--space-md);
        }
        @media (max-width: 600px) {
          .sweep-row {
            grid-template-columns: 1fr;
          }
        }
      `}</style>
    </div>
  );
}
