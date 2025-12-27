import { useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../store';
import { CategoryList } from './strategy/CategoryList';
import { ParameterEditor } from './strategy/ParameterEditor';
import type { StrategyCategory } from '../../types';

export function StrategyPanel() {
  const {
    activePanel,
    focus,
    setFocus,
    toggleFocus,
    navigateUp,
    navigateDown,
    navigateLeft,
    navigateRight,
    handleEnter,
    handleSpace,
    navigateParamUp,
    navigateParamDown,
    adjustParam,
    toggleEnsemble,
    selectAllInCategory,
    deselectAllInCategory,
    getFocusedCategory,
    setCategories,
    selectedStrategies,
    setSelectedStrategies,
  } = useAppStore();

  const isPanelActive = activePanel === 'strategy';
  const focusedCategory = getFocusedCategory();

  // Load categories from backend on mount
  useEffect(() => {
    invoke<StrategyCategory[]>('get_strategy_categories')
      .then(setCategories)
      .catch((err) => console.error('Failed to load categories:', err));

    // Load initial selection
    invoke<string[]>('get_strategy_selection')
      .then(setSelectedStrategies)
      .catch((err) => console.error('Failed to load selection:', err));
  }, [setCategories, setSelectedStrategies]);

  // Sync selection to backend when it changes
  useEffect(() => {
    if (selectedStrategies.size === 0) return;

    invoke('update_strategy_selection', {
      selected: Array.from(selectedStrategies),
    }).catch((err) => console.error('Failed to sync selection:', err));
  }, [selectedStrategies]);

  // Handle keyboard navigation
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!isPanelActive) return;

      // Don't handle if typing in an input
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      ) {
        return;
      }

      if (focus === 'selection') {
        switch (e.key) {
          case 'j':
          case 'ArrowDown':
            e.preventDefault();
            navigateDown();
            break;
          case 'k':
          case 'ArrowUp':
            e.preventDefault();
            navigateUp();
            break;
          case 'h':
          case 'ArrowLeft':
            e.preventDefault();
            navigateLeft();
            break;
          case 'l':
          case 'ArrowRight':
            e.preventDefault();
            navigateRight();
            break;
          case 'Enter':
            e.preventDefault();
            handleEnter();
            break;
          case ' ':
            e.preventDefault();
            handleSpace();
            break;
          case 'a':
            e.preventDefault();
            if (focusedCategory) {
              selectAllInCategory(focusedCategory.id);
            }
            break;
          case 'n':
            e.preventDefault();
            if (focusedCategory) {
              deselectAllInCategory(focusedCategory.id);
            }
            break;
          case 'e':
            e.preventDefault();
            toggleEnsemble();
            break;
          case 'Tab':
            e.preventDefault();
            setFocus('parameters');
            break;
        }
      } else if (focus === 'parameters') {
        switch (e.key) {
          case 'j':
          case 'ArrowDown':
            e.preventDefault();
            navigateParamDown();
            break;
          case 'k':
          case 'ArrowUp':
            e.preventDefault();
            navigateParamUp();
            break;
          case 'h':
          case 'ArrowLeft':
            e.preventDefault();
            adjustParam('decrement');
            break;
          case 'l':
          case 'ArrowRight':
            e.preventDefault();
            adjustParam('increment');
            break;
          case 'e':
            e.preventDefault();
            toggleEnsemble();
            break;
          case 'Tab':
            e.preventDefault();
            setFocus('selection');
            break;
          case 'Escape':
            e.preventDefault();
            setFocus('selection');
            break;
        }
      }
    },
    [
      isPanelActive,
      focus,
      focusedCategory,
      navigateUp,
      navigateDown,
      navigateLeft,
      navigateRight,
      handleEnter,
      handleSpace,
      navigateParamUp,
      navigateParamDown,
      adjustParam,
      toggleEnsemble,
      selectAllInCategory,
      deselectAllInCategory,
      setFocus,
    ]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  return (
    <div className="panel strategy-panel">
      <h1 className="panel-title">Strategy</h1>

      <div className="strategy-content">
        <div className="selection-pane">
          <CategoryList isFocused={isPanelActive && focus === 'selection'} />
        </div>
        <div className="parameters-pane">
          <ParameterEditor isFocused={isPanelActive && focus === 'parameters'} />
        </div>
      </div>

      <div className="strategy-footer">
        <span className="focus-indicator">
          {focus === 'selection' ? 'Selection' : 'Parameters'}
        </span>
        <span className="keyboard-hints">
          Tab: switch focus | j/k: navigate | Space: toggle | e: ensemble
        </span>
      </div>

      <style>{`
        .strategy-panel {
          display: flex;
          flex-direction: column;
          height: 100%;
        }

        .strategy-content {
          flex: 1;
          display: grid;
          grid-template-columns: 45% 55%;
          min-height: 0;
          border: 1px solid var(--border);
          border-radius: var(--radius-md);
          overflow: hidden;
          margin-top: var(--space-md);
        }

        .selection-pane {
          overflow: hidden;
        }

        .parameters-pane {
          overflow: hidden;
          background: var(--bg-secondary);
        }

        .strategy-footer {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: var(--space-sm) 0;
          margin-top: var(--space-sm);
        }

        .focus-indicator {
          font-size: var(--font-size-sm);
          color: var(--cyan);
          background: var(--bg-hover);
          padding: 2px 8px;
          border-radius: var(--radius-xs);
        }

        .keyboard-hints {
          font-size: var(--font-size-xs);
          color: var(--muted);
        }
      `}</style>
    </div>
  );
}
