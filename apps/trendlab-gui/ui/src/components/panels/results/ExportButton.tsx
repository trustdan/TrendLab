import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { VscExport, VscCheck, VscError, VscLoading } from 'react-icons/vsc';
import { useAppStore } from '../../../store';

type ExportState = 'idle' | 'exporting' | 'success' | 'error';

export function ExportButton() {
  const { selectedResultId, results } = useAppStore();
  const [exportState, setExportState] = useState<ExportState>('idle');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const selectedResult = results.find((r) => r.id === selectedResultId);
  const canExport = selectedResult !== undefined;

  async function handleExport() {
    if (!selectedResultId || !selectedResult) return;

    setExportState('exporting');
    setErrorMessage(null);

    try {
      const artifactPath = await invoke<string>('export_artifact', {
        resultId: selectedResultId,
      });
      console.log('Exported artifact to:', artifactPath);
      setExportState('success');

      // Reset to idle after showing success
      setTimeout(() => setExportState('idle'), 2000);
    } catch (err) {
      console.error('Export failed:', err);
      setErrorMessage(err instanceof Error ? err.message : String(err));
      setExportState('error');

      // Reset to idle after showing error
      setTimeout(() => {
        setExportState('idle');
        setErrorMessage(null);
      }, 3000);
    }
  }

  function getButtonContent() {
    switch (exportState) {
      case 'exporting':
        return (
          <>
            <VscLoading className="spin" size={14} />
            <span>Exporting...</span>
          </>
        );
      case 'success':
        return (
          <>
            <VscCheck size={14} />
            <span>Exported!</span>
          </>
        );
      case 'error':
        return (
          <>
            <VscError size={14} />
            <span>Failed</span>
          </>
        );
      default:
        return (
          <>
            <VscExport size={14} />
            <span>Export Artifact</span>
          </>
        );
    }
  }

  return (
    <div className="export-button-container">
      <button
        className={`export-button ${exportState}`}
        onClick={handleExport}
        disabled={!canExport || exportState === 'exporting'}
        title={
          canExport
            ? `Export ${selectedResult.strategy} config for Pine Script generation`
            : 'Select a result to export'
        }
      >
        {getButtonContent()}
      </button>

      {errorMessage && <div className="export-error">{errorMessage}</div>}

      <style>{`
        .export-button-container {
          position: relative;
        }

        .export-button {
          display: flex;
          align-items: center;
          gap: var(--space-xs);
          padding: var(--space-xs) var(--space-sm);
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          background: var(--bg-secondary);
          color: var(--fg);
          font-size: var(--font-size-sm);
          cursor: pointer;
          transition: all 0.15s ease;
        }

        .export-button:hover:not(:disabled) {
          border-color: var(--cyan);
          color: var(--cyan);
        }

        .export-button:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .export-button.exporting {
          color: var(--blue);
          border-color: var(--blue);
        }

        .export-button.success {
          color: var(--green);
          border-color: var(--green);
        }

        .export-button.error {
          color: var(--red);
          border-color: var(--red);
        }

        .export-error {
          position: absolute;
          top: 100%;
          right: 0;
          margin-top: var(--space-xs);
          padding: var(--space-xs) var(--space-sm);
          background: var(--red);
          color: var(--bg);
          font-size: var(--font-size-xs);
          border-radius: var(--radius-sm);
          white-space: nowrap;
          z-index: 10;
        }

        @keyframes spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }

        .spin {
          animation: spin 1s linear infinite;
        }
      `}</style>
    </div>
  );
}
