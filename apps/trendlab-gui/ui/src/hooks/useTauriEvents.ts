import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef } from 'react';
import type {
  EventEnvelope,
  EventPayloadMap,
  EventType,
  JobCompletePayload,
  JobFailedPayload,
  JobProgressPayload,
} from '../types/events';
import { useAppStore } from '../store';

type EventHandler<T> = (payload: T, envelope: EventEnvelope<T>) => void;

/**
 * Subscribe to a single Tauri event
 * @param eventName - Event name to listen for
 * @param handler - Callback when event is received
 * @param deps - Dependencies for handler (similar to useEffect)
 */
export function useTauriEvent<E extends EventType>(
  eventName: E,
  handler: EventHandler<EventPayloadMap[E]>,
  deps: unknown[] = []
) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    listen<EventEnvelope<EventPayloadMap[E]>>(eventName, (event) => {
      handlerRef.current(event.payload.payload, event.payload);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [eventName, ...deps]);
}

/**
 * Subscribe to multiple events at once
 */
export function useTauriEvents(
  handlers: Partial<{
    [E in EventType]: EventHandler<EventPayloadMap[E]>;
  }>
) {
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    for (const [eventName, handler] of Object.entries(handlers)) {
      if (!handler) continue;

      listen<EventEnvelope<unknown>>(eventName, (event) => {
        (handler as EventHandler<unknown>)(event.payload.payload, event.payload);
      }).then((fn) => {
        unlisteners.push(fn);
      });
    }

    return () => {
      unlisteners.forEach((fn) => fn());
    };
    // We intentionally only run once on mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}

/**
 * Subscribe to job lifecycle events for a specific job
 * Automatically updates the jobs store
 */
export function useJobEvents(jobId: string | null) {
  const { updateJobProgress, completeJob, failJob, cancelJob } = useAppStore();

  useEffect(() => {
    if (!jobId) return;

    const unlisteners: UnlistenFn[] = [];

    // Progress
    listen<EventEnvelope<JobProgressPayload>>('job:progress', (event) => {
      if (event.payload.job_id === jobId) {
        const p = event.payload.payload;
        updateJobProgress(jobId, p.current, p.total, p.message);
      }
    }).then((fn) => unlisteners.push(fn));

    // Complete
    listen<EventEnvelope<JobCompletePayload>>('job:complete', (event) => {
      if (event.payload.job_id === jobId) {
        completeJob(jobId);
      }
    }).then((fn) => unlisteners.push(fn));

    // Failed
    listen<EventEnvelope<JobFailedPayload>>('job:failed', (event) => {
      if (event.payload.job_id === jobId) {
        failJob(jobId, event.payload.payload.error);
      }
    }).then((fn) => unlisteners.push(fn));

    // Cancelled
    listen<EventEnvelope<JobCompletePayload>>('job:cancelled', (event) => {
      if (event.payload.job_id === jobId) {
        cancelJob(jobId);
      }
    }).then((fn) => unlisteners.push(fn));

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, [jobId, updateJobProgress, completeJob, failJob, cancelJob]);
}

/**
 * Hook for managing a job lifecycle
 * Creates job, subscribes to events, handles cleanup
 */
export function useJob(jobType: string) {
  const { createJob, jobs, activeJobId } = useAppStore();

  const startJob = useCallback(
    (jobId: string) => {
      createJob(jobId, jobType);
    },
    [createJob, jobType]
  );

  const activeJob = activeJobId ? jobs[activeJobId] : null;

  // Subscribe to events for active job
  useJobEvents(activeJobId);

  return {
    startJob,
    activeJob,
    activeJobId,
    jobs,
  };
}
