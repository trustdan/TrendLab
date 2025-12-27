import { useEffect, useRef, useCallback } from 'react';
import { useAppStore } from '../store';

/**
 * Hook to manage focus when switching between panels.
 * Stores the last focused element per panel and restores focus when returning.
 */
export function useFocusManagement() {
  const activePanel = useAppStore((s) => s.activePanel);
  const previousPanel = useRef(activePanel);
  const focusHistory = useRef<Map<string, HTMLElement | null>>(new Map());

  // Store current focus when leaving a panel
  useEffect(() => {
    if (previousPanel.current !== activePanel) {
      // Save the currently focused element for the previous panel
      const currentFocus = document.activeElement as HTMLElement | null;
      if (currentFocus && currentFocus !== document.body) {
        focusHistory.current.set(previousPanel.current, currentFocus);
      }

      // Restore focus for the new panel, or focus the panel container
      const savedFocus = focusHistory.current.get(activePanel);
      if (savedFocus && document.body.contains(savedFocus)) {
        savedFocus.focus();
      } else {
        // Focus the main content area for screen readers
        const panelElement = document.querySelector(`[data-panel="${activePanel}"]`);
        if (panelElement instanceof HTMLElement) {
          panelElement.focus();
        }
      }

      previousPanel.current = activePanel;
    }
  }, [activePanel]);
}

/**
 * Hook to create a roving tabindex for keyboard navigation within a container.
 * Returns handlers to attach to list items.
 */
export function useRovingTabIndex<T extends HTMLElement>(
  itemCount: number,
  focusedIndex: number,
  onIndexChange: (index: number) => void
) {
  const containerRef = useRef<T>(null);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (itemCount === 0) return;

      switch (e.key) {
        case 'ArrowDown':
        case 'j':
          e.preventDefault();
          onIndexChange(Math.min(itemCount - 1, focusedIndex + 1));
          break;
        case 'ArrowUp':
        case 'k':
          e.preventDefault();
          onIndexChange(Math.max(0, focusedIndex - 1));
          break;
        case 'Home':
          e.preventDefault();
          onIndexChange(0);
          break;
        case 'End':
          e.preventDefault();
          onIndexChange(itemCount - 1);
          break;
      }
    },
    [itemCount, focusedIndex, onIndexChange]
  );

  const getItemProps = useCallback(
    (index: number) => ({
      tabIndex: index === focusedIndex ? 0 : -1,
      'aria-selected': index === focusedIndex,
    }),
    [focusedIndex]
  );

  return {
    containerRef,
    containerProps: {
      role: 'listbox' as const,
      'aria-activedescendant': focusedIndex >= 0 ? `item-${focusedIndex}` : undefined,
      onKeyDown: handleKeyDown,
    },
    getItemProps,
  };
}

/**
 * Hook to announce status changes to screen readers.
 */
export function useAnnounce() {
  const announce = useCallback((message: string, priority: 'polite' | 'assertive' = 'polite') => {
    const announcer = document.getElementById('sr-announcer');
    if (announcer) {
      announcer.setAttribute('aria-live', priority);
      announcer.textContent = message;
      // Clear after announcement
      setTimeout(() => {
        announcer.textContent = '';
      }, 1000);
    }
  }, []);

  return announce;
}
