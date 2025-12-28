import type { SliceCreator } from '../types';
import type {
  StrategyCategory,
  StrategyInfo,
  StrategyParamValues,
  ParamDef,
  StrategyFocus,
  ParamValue,
} from '../../types';
import { STRATEGY_CATEGORIES, getAllStrategies } from '../../types';

/** Strategy panel state */
export interface StrategySlice {
  // Categories and selection
  categories: StrategyCategory[];
  expandedCategories: Set<string>;
  selectedStrategies: Set<string>;

  // Navigation
  focusedCategoryIndex: number;
  focusedStrategyIndex: number; // -1 means focused on category header
  focus: StrategyFocus;

  // Parameters
  strategyParams: Record<string, StrategyParamValues>;
  paramDefs: Record<string, ParamDef[]>;
  focusedParamIndex: number;

  // Ensemble
  ensembleEnabled: boolean;

  // Actions - Categories
  setCategories: (categories: StrategyCategory[]) => void;
  toggleCategoryExpanded: (categoryId: string) => void;
  expandCategory: (categoryId: string) => void;
  collapseCategory: (categoryId: string) => void;

  // Actions - Selection
  toggleStrategy: (strategyId: string) => void;
  selectAllInCategory: (categoryId: string) => void;
  deselectAllInCategory: (categoryId: string) => void;
  setSelectedStrategies: (strategies: string[]) => void;

  // Actions - Navigation (Selection mode)
  navigateUp: () => void;
  navigateDown: () => void;
  navigateLeft: () => void;
  navigateRight: () => void;
  handleEnter: () => void;
  handleSpace: () => void;

  // Actions - Focus
  setFocus: (focus: StrategyFocus) => void;
  toggleFocus: () => void;

  // Actions - Parameters
  setParamDefs: (strategyId: string, defs: ParamDef[]) => void;
  setStrategyParams: (strategyId: string, params: StrategyParamValues) => void;
  updateParam: (strategyId: string, key: string, value: ParamValue) => void;
  navigateParamUp: () => void;
  navigateParamDown: () => void;
  adjustParam: (direction: 'increment' | 'decrement') => void;

  // Actions - Ensemble
  toggleEnsemble: () => void;
  setEnsembleEnabled: (enabled: boolean) => void;

  // Helpers
  getFocusedCategory: () => StrategyCategory | null;
  getFocusedStrategy: () => StrategyInfo | null;
  getSelectedCountForCategory: (categoryId: string) => number;
  isCategoryExpanded: (categoryId: string) => boolean;
  isStrategySelected: (strategyId: string) => boolean;
  getCurrentParamDefs: () => ParamDef[];
  getCurrentParams: () => StrategyParamValues;
}

/** Build navigation list for current state */
function buildNavigationList(
  categories: StrategyCategory[],
  expanded: Set<string>
): Array<{ type: 'category' | 'strategy'; categoryIndex: number; strategyIndex: number }> {
  const items: Array<{ type: 'category' | 'strategy'; categoryIndex: number; strategyIndex: number }> = [];

  categories.forEach((category, catIndex) => {
    // Category header
    items.push({ type: 'category', categoryIndex: catIndex, strategyIndex: -1 });

    // Strategies if expanded
    if (expanded.has(category.id)) {
      category.strategies.forEach((_, stratIndex) => {
        items.push({ type: 'strategy', categoryIndex: catIndex, strategyIndex: stratIndex });
      });
    }
  });

  return items;
}

/** Create strategy slice */
export const createStrategySlice: SliceCreator<StrategySlice> = (set, get) => ({
  // Initial state
  categories: STRATEGY_CATEGORIES,
  expandedCategories: new Set(['channel']), // First category expanded by default
  selectedStrategies: new Set(getAllStrategies().map((s) => s.id)), // All selected by default
  focusedCategoryIndex: 0,
  focusedStrategyIndex: -1, // -1 = on category header
  focus: 'selection',
  strategyParams: {},
  paramDefs: {},
  focusedParamIndex: 0,
  ensembleEnabled: false,

  // Category actions
  setCategories: (categories) => set({ categories }),

  toggleCategoryExpanded: (categoryId) =>
    set((state) => {
      const newExpanded = new Set(state.expandedCategories);
      if (newExpanded.has(categoryId)) {
        newExpanded.delete(categoryId);
      } else {
        newExpanded.add(categoryId);
      }
      return { expandedCategories: newExpanded };
    }),

  expandCategory: (categoryId) =>
    set((state) => ({
      expandedCategories: new Set([...state.expandedCategories, categoryId]),
    })),

  collapseCategory: (categoryId) =>
    set((state) => {
      const newExpanded = new Set(state.expandedCategories);
      newExpanded.delete(categoryId);
      return { expandedCategories: newExpanded, focusedStrategyIndex: -1 };
    }),

  // Selection actions
  toggleStrategy: (strategyId) =>
    set((state) => {
      const newSelected = new Set(state.selectedStrategies);
      if (newSelected.has(strategyId)) {
        newSelected.delete(strategyId);
      } else {
        newSelected.add(strategyId);
      }
      return { selectedStrategies: newSelected };
    }),

  selectAllInCategory: (categoryId) =>
    set((state) => {
      const category = state.categories.find((c) => c.id === categoryId);
      if (!category) return state;
      const newSelected = new Set(state.selectedStrategies);
      category.strategies.forEach((s) => newSelected.add(s.id));
      return { selectedStrategies: newSelected };
    }),

  deselectAllInCategory: (categoryId) =>
    set((state) => {
      const category = state.categories.find((c) => c.id === categoryId);
      if (!category) return state;
      const newSelected = new Set(state.selectedStrategies);
      category.strategies.forEach((s) => newSelected.delete(s.id));
      return { selectedStrategies: newSelected };
    }),

  setSelectedStrategies: (strategies) =>
    set({ selectedStrategies: new Set(strategies) }),

  // Navigation actions
  navigateUp: () =>
    set((state) => {
      const navList = buildNavigationList(state.categories, state.expandedCategories);
      // Find current position
      let currentIndex = navList.findIndex(
        (item) =>
          item.categoryIndex === state.focusedCategoryIndex &&
          item.strategyIndex === state.focusedStrategyIndex
      );
      if (currentIndex === -1) currentIndex = 0;

      // Move up with wrap
      const newIndex = (currentIndex - 1 + navList.length) % navList.length;
      const newItem = navList[newIndex];

      return {
        focusedCategoryIndex: newItem.categoryIndex,
        focusedStrategyIndex: newItem.strategyIndex,
      };
    }),

  navigateDown: () =>
    set((state) => {
      const navList = buildNavigationList(state.categories, state.expandedCategories);
      // Find current position
      let currentIndex = navList.findIndex(
        (item) =>
          item.categoryIndex === state.focusedCategoryIndex &&
          item.strategyIndex === state.focusedStrategyIndex
      );
      if (currentIndex === -1) currentIndex = 0;

      // Move down with wrap
      const newIndex = (currentIndex + 1) % navList.length;
      const newItem = navList[newIndex];

      return {
        focusedCategoryIndex: newItem.categoryIndex,
        focusedStrategyIndex: newItem.strategyIndex,
      };
    }),

  navigateLeft: () => {
    const state = get();
    const category = state.categories[state.focusedCategoryIndex];
    if (!category) return;

    if (state.focusedStrategyIndex >= 0) {
      // On a strategy - collapse to category header
      set({ focusedStrategyIndex: -1 });
    } else if (state.expandedCategories.has(category.id)) {
      // On category header - collapse it
      get().collapseCategory(category.id);
    }
  },

  navigateRight: () => {
    const state = get();
    const category = state.categories[state.focusedCategoryIndex];
    if (!category) return;

    if (state.focusedStrategyIndex === -1) {
      // On category header
      if (!state.expandedCategories.has(category.id)) {
        // Expand the category
        get().expandCategory(category.id);
      } else if (category.strategies.length > 0) {
        // Already expanded - move to first strategy
        set({ focusedStrategyIndex: 0 });
      }
    }
  },

  handleEnter: () => {
    const state = get();
    const category = state.categories[state.focusedCategoryIndex];
    if (!category) return;

    if (state.focusedStrategyIndex === -1) {
      // On category header - toggle expand/collapse
      get().toggleCategoryExpanded(category.id);
    } else {
      // On strategy - switch to parameter editing
      set({ focus: 'parameters', focusedParamIndex: 0 });
    }
  },

  handleSpace: () => {
    const state = get();
    const category = state.categories[state.focusedCategoryIndex];
    if (!category) return;

    if (state.focusedStrategyIndex === -1) {
      // On category header - toggle all in category
      const allSelected = category.strategies.every((s) =>
        state.selectedStrategies.has(s.id)
      );
      if (allSelected) {
        get().deselectAllInCategory(category.id);
      } else {
        get().selectAllInCategory(category.id);
      }
    } else {
      // On strategy - toggle selection
      const strategy = category.strategies[state.focusedStrategyIndex];
      if (strategy) {
        get().toggleStrategy(strategy.id);
      }
    }
  },

  // Focus actions
  setFocus: (focus) => set({ focus, focusedParamIndex: 0 }),

  toggleFocus: () =>
    set((state) => ({
      focus: state.focus === 'selection' ? 'parameters' : 'selection',
      focusedParamIndex: 0,
    })),

  // Parameter actions
  setParamDefs: (strategyId, defs) =>
    set((state) => ({
      paramDefs: { ...state.paramDefs, [strategyId]: defs },
    })),

  setStrategyParams: (strategyId, params) =>
    set((state) => ({
      strategyParams: { ...state.strategyParams, [strategyId]: params },
    })),

  updateParam: (strategyId, key, value) =>
    set((state) => {
      const current = state.strategyParams[strategyId] ?? { values: {} };
      return {
        strategyParams: {
          ...state.strategyParams,
          [strategyId]: {
            values: { ...current.values, [key]: value },
          },
        },
      };
    }),

  navigateParamUp: () =>
    set((state) => {
      const defs = get().getCurrentParamDefs();
      if (defs.length === 0) return state;
      const newIndex = (state.focusedParamIndex - 1 + defs.length) % defs.length;
      return { focusedParamIndex: newIndex };
    }),

  navigateParamDown: () =>
    set((state) => {
      const defs = get().getCurrentParamDefs();
      if (defs.length === 0) return state;
      const newIndex = (state.focusedParamIndex + 1) % defs.length;
      return { focusedParamIndex: newIndex };
    }),

  adjustParam: (direction) => {
    const state = get();
    const strategy = get().getFocusedStrategy();
    if (!strategy || !strategy.has_params) return;

    const defs = get().getCurrentParamDefs();
    const def = defs[state.focusedParamIndex];
    if (!def) return;

    const params = get().getCurrentParams();
    const currentValue = params.values[def.key] ?? def.default;

    let newValue: ParamValue;

    if (def.type === 'enum' && def.options) {
      // Cycle through enum options
      const currentIndex = def.options.indexOf(String(currentValue));
      const delta = direction === 'increment' ? 1 : -1;
      const newIndex = (currentIndex + delta + def.options.length) % def.options.length;
      newValue = def.options[newIndex];
    } else if (def.type === 'int' || def.type === 'float') {
      // Numeric adjustment
      const step = def.step ?? 1;
      const delta = direction === 'increment' ? step : -step;
      let numValue = Number(currentValue) + delta;

      // Clamp to min/max
      if (def.min !== undefined) numValue = Math.max(def.min, numValue);
      if (def.max !== undefined) numValue = Math.min(def.max, numValue);

      // Round for float precision
      if (def.type === 'float') {
        newValue = Math.round(numValue * 100) / 100;
      } else {
        newValue = Math.round(numValue);
      }
    } else {
      return;
    }

    get().updateParam(strategy.id, def.key, newValue);
  },

  // Ensemble actions
  toggleEnsemble: () =>
    set((state) => ({ ensembleEnabled: !state.ensembleEnabled })),

  setEnsembleEnabled: (enabled) => set({ ensembleEnabled: enabled }),

  // Helpers
  getFocusedCategory: () => {
    const state = get();
    return state.categories[state.focusedCategoryIndex] ?? null;
  },

  getFocusedStrategy: () => {
    const state = get();
    const category = state.categories[state.focusedCategoryIndex];
    if (!category || state.focusedStrategyIndex < 0) return null;
    return category.strategies[state.focusedStrategyIndex] ?? null;
  },

  getSelectedCountForCategory: (categoryId) => {
    const state = get();
    const category = state.categories.find((c) => c.id === categoryId);
    if (!category) return 0;
    return category.strategies.filter((s) => state.selectedStrategies.has(s.id)).length;
  },

  isCategoryExpanded: (categoryId) => get().expandedCategories.has(categoryId),

  isStrategySelected: (strategyId) => get().selectedStrategies.has(strategyId),

  getCurrentParamDefs: () => {
    const strategy = get().getFocusedStrategy();
    if (!strategy) return [];
    return get().paramDefs[strategy.id] ?? [];
  },

  getCurrentParams: () => {
    const strategy = get().getFocusedStrategy();
    if (!strategy) return { values: {} };
    return get().strategyParams[strategy.id] ?? { values: {} };
  },
});
