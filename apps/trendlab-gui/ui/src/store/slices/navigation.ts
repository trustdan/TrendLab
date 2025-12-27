import type { StateCreator } from 'zustand';

/** Panel identifiers */
export type PanelId = 'data' | 'strategy' | 'sweep' | 'results' | 'chart';

/** Panel metadata */
export interface PanelMeta {
  id: PanelId;
  label: string;
  shortcut: string;
  icon: string;
}

/** All panels in order */
export const PANELS: PanelMeta[] = [
  { id: 'data', label: 'Data', shortcut: '1', icon: 'database' },
  { id: 'strategy', label: 'Strategy', shortcut: '2', icon: 'settings' },
  { id: 'sweep', label: 'Sweep', shortcut: '3', icon: 'play' },
  { id: 'results', label: 'Results', shortcut: '4', icon: 'table' },
  { id: 'chart', label: 'Chart', shortcut: '5', icon: 'chart' },
];

/** Navigation state */
export interface NavigationSlice {
  activePanel: PanelId;
  previousPanel: PanelId | null;
  setActivePanel: (panel: PanelId) => void;
  nextPanel: () => void;
  previousPanelAction: () => void;
  goToPanel: (index: number) => void;
}

/** Create navigation slice */
export const createNavigationSlice: StateCreator<NavigationSlice> = (set, get) => ({
  activePanel: 'data',
  previousPanel: null,

  setActivePanel: (panel) =>
    set((state) => ({
      previousPanel: state.activePanel,
      activePanel: panel,
    })),

  nextPanel: () =>
    set((state) => {
      const currentIndex = PANELS.findIndex((p) => p.id === state.activePanel);
      const nextIndex = (currentIndex + 1) % PANELS.length;
      return {
        previousPanel: state.activePanel,
        activePanel: PANELS[nextIndex].id,
      };
    }),

  previousPanelAction: () =>
    set((state) => {
      const currentIndex = PANELS.findIndex((p) => p.id === state.activePanel);
      const prevIndex = (currentIndex - 1 + PANELS.length) % PANELS.length;
      return {
        previousPanel: state.activePanel,
        activePanel: PANELS[prevIndex].id,
      };
    }),

  goToPanel: (index) => {
    if (index >= 0 && index < PANELS.length) {
      set((state) => ({
        previousPanel: state.activePanel,
        activePanel: PANELS[index].id,
      }));
    }
  },
});
