import { create } from 'zustand';
import { devtools } from 'zustand/middleware';
import { createNavigationSlice, type NavigationSlice } from './slices/navigation';
import { createStatusSlice, type StatusSlice } from './slices/status';
import { createJobsSlice, type JobsSlice } from './slices/jobs';
import { createDataSlice, type DataSlice } from './slices/data';

/** Combined app store type */
export type AppStore = NavigationSlice & StatusSlice & JobsSlice & DataSlice;

/** Main app store */
export const useAppStore = create<AppStore>()(
  devtools(
    (...a) => ({
      ...createNavigationSlice(...a),
      ...createStatusSlice(...a),
      ...createJobsSlice(...a),
      ...createDataSlice(...a),
    }),
    { name: 'TrendLab' }
  )
);

// Re-export slice types and constants
export * from './slices/navigation';
export * from './slices/status';
export * from './slices/jobs';
export * from './slices/data';
