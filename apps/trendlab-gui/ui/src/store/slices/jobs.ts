import type { StateCreator } from 'zustand';
import type { ErrorEnvelope } from '../../types/error';
import type { JobStatus } from '../../types/events';

/** Job info */
export interface JobInfo {
  id: string;
  type: string; // e.g., 'ping', 'fetch', 'sweep'
  status: JobStatus;
  progress: {
    current: number;
    total: number;
    message: string;
  } | null;
  startedAt: number;
  completedAt: number | null;
  error: ErrorEnvelope | null;
}

/** Jobs slice state */
export interface JobsSlice {
  jobs: Record<string, JobInfo>;
  activeJobId: string | null;
  lastError: ErrorEnvelope | null;

  // Actions
  createJob: (id: string, type: string) => void;
  updateJobStatus: (id: string, status: JobStatus) => void;
  updateJobProgress: (id: string, current: number, total: number, message: string) => void;
  completeJob: (id: string) => void;
  failJob: (id: string, error: ErrorEnvelope) => void;
  cancelJob: (id: string) => void;
  setActiveJob: (id: string | null) => void;
  clearJob: (id: string) => void;
  clearAllJobs: () => void;
}

/** Create jobs slice */
export const createJobsSlice: StateCreator<JobsSlice> = (set) => ({
  jobs: {},
  activeJobId: null,
  lastError: null,

  createJob: (id, type) =>
    set((state) => ({
      jobs: {
        ...state.jobs,
        [id]: {
          id,
          type,
          status: 'queued',
          progress: null,
          startedAt: Date.now(),
          completedAt: null,
          error: null,
        },
      },
      activeJobId: id,
    })),

  updateJobStatus: (id, status) =>
    set((state) => {
      const job = state.jobs[id];
      if (!job) return state;
      return {
        jobs: {
          ...state.jobs,
          [id]: { ...job, status },
        },
      };
    }),

  updateJobProgress: (id, current, total, message) =>
    set((state) => {
      const job = state.jobs[id];
      if (!job) return state;
      return {
        jobs: {
          ...state.jobs,
          [id]: {
            ...job,
            status: 'running',
            progress: { current, total, message },
          },
        },
      };
    }),

  completeJob: (id) =>
    set((state) => {
      const job = state.jobs[id];
      if (!job) return state;
      return {
        jobs: {
          ...state.jobs,
          [id]: {
            ...job,
            status: 'completed',
            completedAt: Date.now(),
          },
        },
        activeJobId: state.activeJobId === id ? null : state.activeJobId,
      };
    }),

  failJob: (id, error) =>
    set((state) => {
      const job = state.jobs[id];
      if (!job) return state;
      return {
        jobs: {
          ...state.jobs,
          [id]: {
            ...job,
            status: 'failed',
            completedAt: Date.now(),
            error,
          },
        },
        lastError: error,
        activeJobId: state.activeJobId === id ? null : state.activeJobId,
      };
    }),

  cancelJob: (id) =>
    set((state) => {
      const job = state.jobs[id];
      if (!job) return state;
      return {
        jobs: {
          ...state.jobs,
          [id]: {
            ...job,
            status: 'cancelled',
            completedAt: Date.now(),
          },
        },
        activeJobId: state.activeJobId === id ? null : state.activeJobId,
      };
    }),

  setActiveJob: (id) => set({ activeJobId: id }),

  clearJob: (id) =>
    set((state) => {
      const { [id]: _, ...rest } = state.jobs;
      return {
        jobs: rest,
        activeJobId: state.activeJobId === id ? null : state.activeJobId,
      };
    }),

  clearAllJobs: () => set({ jobs: {}, activeJobId: null }),
});
