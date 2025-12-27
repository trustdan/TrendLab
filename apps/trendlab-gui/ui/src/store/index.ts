import { create } from 'zustand';
import { devtools } from 'zustand/middleware';
import { createNavigationSlice } from './slices/navigation';
import { createStatusSlice } from './slices/status';
import { createJobsSlice } from './slices/jobs';
import { createDataSlice } from './slices/data';
import { createStrategySlice } from './slices/strategy';
import { createSweepSlice } from './slices/sweep';
import { createResultsSlice } from './slices/results';
import { createChartSlice } from './slices/chart';
import type { AppStore } from './types';

/** Main app store */
export const useAppStore = create<AppStore>()(
  devtools(
    (...a) => ({
      ...createNavigationSlice(...a),
      ...createStatusSlice(...a),
      ...createJobsSlice(...a),
      ...createDataSlice(...a),
      ...createStrategySlice(...a),
      ...createSweepSlice(...a),
      ...createResultsSlice(...a),
      ...createChartSlice(...a),
    }),
    { name: 'TrendLab' }
  )
);

// Re-export store types
export type { AppStore } from './types';

// Re-export slice types and constants
export * from './slices/navigation';
export * from './slices/status';
export * from './slices/jobs';
export * from './slices/data';
export * from './slices/strategy';
export * from './slices/sweep';
export * from './slices/results';
export * from './slices/chart';
