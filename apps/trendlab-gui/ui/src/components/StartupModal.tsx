import { useState, useEffect, useCallback } from 'react';
import { VscClose, VscPlay, VscSettings, VscRocket } from 'react-icons/vsc';

export type StartupMode = 'manual' | 'full-auto' | null;

interface StartupModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectMode: (mode: 'manual' | 'full-auto') => void;
}

/** Get last selected mode from localStorage */
function getLastMode(): StartupMode {
  try {
    const stored = localStorage.getItem('trendlab-startup-mode');
    if (stored === 'manual' || stored === 'full-auto') {
      return stored;
    }
  } catch {
    // localStorage not available
  }
  return null;
}

/** Save selected mode to localStorage */
function saveMode(mode: 'manual' | 'full-auto') {
  try {
    localStorage.setItem('trendlab-startup-mode', mode);
  } catch {
    // localStorage not available
  }
}

export function StartupModal({ isOpen, onClose, onSelectMode }: StartupModalProps) {
  const [rememberChoice, setRememberChoice] = useState(false);
  const lastMode = getLastMode();

  // Handle keyboard navigation
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!isOpen) return;

      switch (e.key) {
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
        case 'm':
        case 'M':
        case '1':
          e.preventDefault();
          handleSelectMode('manual');
          break;
        case 'a':
        case 'A':
        case '2':
          e.preventDefault();
          handleSelectMode('full-auto');
          break;
      }
    },
    [isOpen, onClose]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  const handleSelectMode = (mode: 'manual' | 'full-auto') => {
    if (rememberChoice) {
      saveMode(mode);
    }
    onSelectMode(mode);
    onClose();
  };

  if (!isOpen) return null;

  return (
    <div className="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="startup-modal-title">
      <div className="modal-content">
        <button className="modal-close" onClick={onClose} aria-label="Close modal">
          <VscClose size={20} />
        </button>

        <div className="modal-header">
          <h2 id="startup-modal-title" className="modal-title">
            Welcome to TrendLab
          </h2>
          <p className="modal-subtitle">Choose how you want to work today</p>
        </div>

        <div className="mode-options">
          <button
            className={`mode-card ${lastMode === 'manual' ? 'last-used' : ''}`}
            onClick={() => handleSelectMode('manual')}
          >
            <div className="mode-icon">
              <VscSettings size={32} />
            </div>
            <div className="mode-info">
              <h3 className="mode-name">Manual Mode</h3>
              <p className="mode-description">
                Full control over symbol selection, strategy configuration, and sweep parameters.
                Step through each panel at your own pace.
              </p>
            </div>
            <div className="mode-shortcut">
              <kbd>M</kbd>
            </div>
          </button>

          <button
            className={`mode-card ${lastMode === 'full-auto' ? 'last-used' : ''}`}
            onClick={() => handleSelectMode('full-auto')}
          >
            <div className="mode-icon">
              <VscRocket size={32} />
            </div>
            <div className="mode-info">
              <h3 className="mode-name">Full-Auto Mode</h3>
              <p className="mode-description">
                YOLO mode - continuous optimization across all strategies with parameter randomization.
                Sit back and watch the leaderboard evolve.
              </p>
            </div>
            <div className="mode-shortcut">
              <kbd>A</kbd>
            </div>
          </button>
        </div>

        <div className="modal-footer">
          <label className="remember-choice">
            <input
              type="checkbox"
              checked={rememberChoice}
              onChange={(e) => setRememberChoice(e.target.checked)}
            />
            <span>Remember my choice</span>
          </label>
          {lastMode && (
            <span className="last-used-hint">
              Last used: {lastMode === 'manual' ? 'Manual' : 'Full-Auto'}
            </span>
          )}
        </div>
      </div>

      <style>{`
        .modal-overlay {
          position: fixed;
          inset: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          background: rgba(0, 0, 0, 0.7);
          backdrop-filter: blur(4px);
          z-index: 1000;
          animation: fadeIn 0.2s ease;
        }

        @keyframes fadeIn {
          from { opacity: 0; }
          to { opacity: 1; }
        }

        .modal-content {
          position: relative;
          background: var(--bg);
          border: 1px solid var(--border);
          border-radius: var(--radius-lg);
          padding: var(--space-xl);
          max-width: 600px;
          width: 90%;
          box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
          animation: slideUp 0.3s ease;
        }

        @keyframes slideUp {
          from {
            opacity: 0;
            transform: translateY(20px);
          }
          to {
            opacity: 1;
            transform: translateY(0);
          }
        }

        .modal-close {
          position: absolute;
          top: var(--space-md);
          right: var(--space-md);
          background: transparent;
          border: none;
          color: var(--muted);
          cursor: pointer;
          padding: var(--space-xs);
          border-radius: var(--radius-sm);
          transition: color 0.15s ease;
        }

        .modal-close:hover {
          color: var(--fg);
        }

        .modal-header {
          text-align: center;
          margin-bottom: var(--space-xl);
        }

        .modal-title {
          margin: 0 0 var(--space-xs) 0;
          font-size: var(--font-size-xl);
          color: var(--cyan);
        }

        .modal-subtitle {
          margin: 0;
          color: var(--muted);
          font-size: var(--font-size-md);
        }

        .mode-options {
          display: flex;
          flex-direction: column;
          gap: var(--space-md);
        }

        .mode-card {
          display: flex;
          align-items: flex-start;
          gap: var(--space-md);
          padding: var(--space-lg);
          background: var(--bg-secondary);
          border: 2px solid var(--border);
          border-radius: var(--radius-md);
          cursor: pointer;
          text-align: left;
          transition: all 0.15s ease;
        }

        .mode-card:hover {
          border-color: var(--cyan);
          background: var(--bg-hover);
        }

        .mode-card:focus {
          outline: none;
          border-color: var(--cyan);
          box-shadow: 0 0 0 3px rgba(115, 218, 202, 0.2);
        }

        .mode-card.last-used {
          border-color: var(--purple);
        }

        .mode-icon {
          flex-shrink: 0;
          color: var(--cyan);
          padding: var(--space-sm);
          background: rgba(115, 218, 202, 0.1);
          border-radius: var(--radius-sm);
        }

        .mode-info {
          flex: 1;
        }

        .mode-name {
          margin: 0 0 var(--space-xs) 0;
          font-size: var(--font-size-md);
          color: var(--fg);
        }

        .mode-description {
          margin: 0;
          font-size: var(--font-size-sm);
          color: var(--muted);
          line-height: 1.5;
        }

        .mode-shortcut {
          flex-shrink: 0;
        }

        .mode-shortcut kbd {
          display: inline-block;
          padding: var(--space-xs) var(--space-sm);
          background: var(--bg-hover);
          border: 1px solid var(--border);
          border-radius: var(--radius-xs);
          font-family: var(--font-mono);
          font-size: var(--font-size-sm);
          color: var(--muted);
        }

        .modal-footer {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-top: var(--space-lg);
          padding-top: var(--space-md);
          border-top: 1px solid var(--border);
        }

        .remember-choice {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          cursor: pointer;
          font-size: var(--font-size-sm);
          color: var(--muted);
        }

        .remember-choice input[type="checkbox"] {
          accent-color: var(--cyan);
        }

        .last-used-hint {
          font-size: var(--font-size-xs);
          color: var(--purple);
        }
      `}</style>
    </div>
  );
}
