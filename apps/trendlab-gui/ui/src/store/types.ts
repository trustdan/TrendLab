import type { StateCreator } from 'zustand';
import type { NavigationSlice } from './slices/navigation';
import type { StatusSlice } from './slices/status';
import type { JobsSlice } from './slices/jobs';
import type { DataSlice } from './slices/data';
import type { StrategySlice } from './slices/strategy';
import type { SweepSlice } from './slices/sweep';
import type { ResultsSlice } from './slices/results';
import type { ChartSlice } from './slices/chart';

/** Combined app store type */
export type AppStore = NavigationSlice &
  StatusSlice &
  JobsSlice &
  DataSlice &
  StrategySlice &
  SweepSlice &
  ResultsSlice &
  ChartSlice;

/** Slice creator type for Zustand v5 */
export type SliceCreator<T> = StateCreator<AppStore, [], [], T>;
