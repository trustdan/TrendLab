import type { StateCreator } from 'zustand';

/** Operation state */
export type OperationState = 'idle' | 'loading' | 'success' | 'error';

/** Status message with optional details */
export interface StatusMessage {
  text: string;
  type: 'info' | 'success' | 'warning' | 'error';
  timestamp: number;
  details?: string;
}

/** Status slice state */
export interface StatusSlice {
  operationState: OperationState;
  statusMessage: StatusMessage | null;
  statusHistory: StatusMessage[];
  setOperationState: (state: OperationState) => void;
  setStatus: (text: string, type?: StatusMessage['type'], details?: string) => void;
  clearStatus: () => void;
}

/** Create status slice */
export const createStatusSlice: StateCreator<StatusSlice> = (set) => ({
  operationState: 'idle',
  statusMessage: null,
  statusHistory: [],

  setOperationState: (operationState) => set({ operationState }),

  setStatus: (text, type = 'info', details) =>
    set((state) => {
      const message: StatusMessage = {
        text,
        type,
        timestamp: Date.now(),
        details,
      };
      return {
        statusMessage: message,
        statusHistory: [...state.statusHistory.slice(-49), message], // Keep last 50
      };
    }),

  clearStatus: () => set({ statusMessage: null }),
});
