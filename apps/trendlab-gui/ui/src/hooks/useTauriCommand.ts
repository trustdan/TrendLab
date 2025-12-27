import { invoke } from '@tauri-apps/api/core';
import { useCallback, useState } from 'react';
import { toErrorEnvelope, type ErrorEnvelope } from '../types/error';

/** Command execution state */
export interface CommandState<T> {
  data: T | null;
  error: ErrorEnvelope | null;
  isLoading: boolean;
}

/** Hook return type */
export interface UseCommandReturn<T, A extends unknown[]> {
  execute: (...args: A) => Promise<T>;
  data: T | null;
  error: ErrorEnvelope | null;
  isLoading: boolean;
  reset: () => void;
}

/**
 * Type-safe wrapper for Tauri command invocation
 * @param command - The Tauri command name
 * @param argsMapper - Function to map hook args to command payload
 */
export function useTauriCommand<T, A extends unknown[] = []>(
  command: string,
  argsMapper?: (...args: A) => Record<string, unknown>
): UseCommandReturn<T, A> {
  const [state, setState] = useState<CommandState<T>>({
    data: null,
    error: null,
    isLoading: false,
  });

  const execute = useCallback(
    async (...args: A): Promise<T> => {
      setState((s) => ({ ...s, isLoading: true, error: null }));

      try {
        const payload = argsMapper ? argsMapper(...args) : undefined;
        const result = await invoke<T>(command, payload);
        setState({ data: result, error: null, isLoading: false });
        return result;
      } catch (err) {
        const error = toErrorEnvelope(err);
        setState({ data: null, error, isLoading: false });
        throw error;
      }
    },
    [command, argsMapper]
  );

  const reset = useCallback(() => {
    setState({ data: null, error: null, isLoading: false });
  }, []);

  return {
    execute,
    data: state.data,
    error: state.error,
    isLoading: state.isLoading,
    reset,
  };
}

/**
 * Simple one-shot command invocation (no state tracking)
 */
export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    throw toErrorEnvelope(err);
  }
}

// Pre-typed command hooks for common operations

/** Start a ping job (smoke test) */
export function usePingJob() {
  return useTauriCommand<{ job_id: string }>('ping_job');
}

/** Cancel a job by ID */
export function useCancelJob() {
  return useTauriCommand<boolean, [string]>('cancel_job', (jobId) => ({
    job_id: jobId,
  }));
}
