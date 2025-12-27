import { useEffect, useCallback } from 'react';
import { useAppStore, type PanelId } from '../store';

/** Keyboard action types */
export type KeyboardAction =
  // Global
  | { type: 'quit' }
  | { type: 'cancel' }
  | { type: 'next_panel' }
  | { type: 'prev_panel' }
  | { type: 'go_to_panel'; panel: number }
  // Navigation
  | { type: 'move_up' }
  | { type: 'move_down' }
  | { type: 'move_left' }
  | { type: 'move_right' }
  | { type: 'confirm' }
  // Selection
  | { type: 'toggle_selection' }
  | { type: 'select_all' }
  | { type: 'select_none' }
  // Panel actions
  | { type: 'fetch' }
  | { type: 'search' }
  | { type: 'sort' }
  | { type: 'toggle_view' }
  | { type: 'toggle_ensemble' }
  | { type: 'toggle_drawdown' }
  | { type: 'toggle_chart_mode' }
  | { type: 'toggle_volume' }
  | { type: 'toggle_crosshair' }
  | { type: 'reset_defaults' }
  | { type: 'show_help' };

/** Handler for keyboard actions */
export type KeyboardActionHandler = (action: KeyboardAction) => void;

/** Check if event target is an input element */
function isInputElement(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

/**
 * Central keyboard navigation hook matching TUI shortcuts
 * @param onAction - Callback for panel-specific actions
 * @param options - Configuration options
 */
export function useKeyboardNavigation(
  onAction?: KeyboardActionHandler,
  options: {
    enableQuit?: boolean;
  } = {}
) {
  const { enableQuit = false } = options;
  const {
    activePanel,
    goToPanel,
    nextPanel,
    previousPanelAction,
  } = useAppStore();

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Ignore if typing in an input (except for Escape)
      if (isInputElement(e.target) && e.key !== 'Escape') {
        return;
      }

      let action: KeyboardAction | null = null;

      switch (e.key) {
        // === Global Navigation ===
        case 'q':
          if (enableQuit) {
            e.preventDefault();
            action = { type: 'quit' };
          }
          break;

        case 'Escape':
          e.preventDefault();
          action = { type: 'cancel' };
          break;

        case 'Tab':
          e.preventDefault();
          if (e.shiftKey) {
            previousPanelAction();
          } else {
            nextPanel();
          }
          return; // Already handled

        case '1':
        case '2':
        case '3':
        case '4':
        case '5':
          e.preventDefault();
          goToPanel(parseInt(e.key, 10) - 1);
          return; // Already handled

        // === Vim-Style Navigation ===
        case 'j':
        case 'ArrowDown':
          e.preventDefault();
          action = { type: 'move_down' };
          break;

        case 'k':
        case 'ArrowUp':
          e.preventDefault();
          action = { type: 'move_up' };
          break;

        case 'h':
        case 'ArrowLeft':
          e.preventDefault();
          action = { type: 'move_left' };
          break;

        case 'l':
        case 'ArrowRight':
          e.preventDefault();
          action = { type: 'move_right' };
          break;

        case 'Enter':
          e.preventDefault();
          action = { type: 'confirm' };
          break;

        // === Selection ===
        case ' ':
          e.preventDefault();
          action = { type: 'toggle_selection' };
          break;

        case 'a':
          e.preventDefault();
          action = { type: 'select_all' };
          break;

        case 'n':
          e.preventDefault();
          action = { type: 'select_none' };
          break;

        // === Panel-Specific Actions ===
        case 'f':
          if (activePanel === 'data') {
            e.preventDefault();
            action = { type: 'fetch' };
          }
          break;

        case 's':
          e.preventDefault();
          if (activePanel === 'data') {
            action = { type: 'search' };
          } else if (activePanel === 'results') {
            action = { type: 'sort' };
          }
          break;

        case 'v':
          e.preventDefault();
          if (activePanel === 'chart') {
            action = { type: 'toggle_volume' };
          } else if (activePanel === 'results') {
            action = { type: 'toggle_view' };
          }
          break;

        case 'e':
          if (activePanel === 'strategy') {
            e.preventDefault();
            action = { type: 'toggle_ensemble' };
          }
          break;

        case 'd':
          if (activePanel === 'chart') {
            e.preventDefault();
            action = { type: 'toggle_drawdown' };
          }
          break;

        case 'm':
          if (activePanel === 'chart') {
            e.preventDefault();
            action = { type: 'toggle_chart_mode' };
          }
          break;

        case 'c':
          if (activePanel === 'chart') {
            e.preventDefault();
            action = { type: 'toggle_crosshair' };
          }
          break;

        case 'R':
          e.preventDefault();
          action = { type: 'reset_defaults' };
          break;

        case '?':
          e.preventDefault();
          action = { type: 'show_help' };
          break;
      }

      if (action && onAction) {
        onAction(action);
      }
    },
    [activePanel, enableQuit, goToPanel, nextPanel, previousPanelAction, onAction]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);
}

/** Get keyboard hint for current panel */
export function getKeyboardHints(panel: PanelId): string[] {
  const common = ['1-5 panels', 'Tab next', 'j/k navigate'];

  switch (panel) {
    case 'data':
      return [...common, 's search', 'f fetch', 'Space select'];
    case 'strategy':
      return [...common, 'Space toggle', 'e ensemble', 'a/n all/none'];
    case 'sweep':
      return [...common, 'Enter start'];
    case 'results':
      return [...common, 's sort', 'v view mode', 'Enter chart'];
    case 'chart':
      return [...common, 'd drawdown', 'v volume', 'm mode', 'c crosshair'];
    default:
      return common;
  }
}
