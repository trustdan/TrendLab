import type { ErrorEnvelope } from './error';
import type { SweepProgress } from './sweep';

/** Event envelope wrapping all events */
export interface EventEnvelope<T = unknown> {
  event: string;
  job_id: string;
  ts_ms: number;
  payload: T;
}

/** Job status */
export type JobStatus = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';

/** Generic job progress payload */
export interface JobProgressPayload {
  message: string;
  current: number;
  total: number;
}

/** Job completion payload */
export interface JobCompletePayload {
  message: string;
}

/** Job failure payload */
export interface JobFailedPayload {
  error: ErrorEnvelope;
}

/** Data fetch progress payload */
export interface DataProgressPayload {
  ticker: string;
  current: number;
  total: number;
  message: string;
}

/** Data fetch complete payload */
export interface DataCompletePayload {
  tickers: string[];
  barCount: number;
  message: string;
}

/** Sweep progress event payload (alias for domain type) */
export type SweepProgressPayload = SweepProgress;

/** Sweep complete payload */
export interface SweepCompletePayload {
  totalConfigs: number;
  bestResult?: {
    ticker: string;
    strategy: string;
    sharpe: number;
  };
  message: string;
}

/** All event types */
export type EventType =
  | 'job:progress'
  | 'job:complete'
  | 'job:failed'
  | 'job:cancelled'
  | 'data:progress'
  | 'data:complete'
  | 'data:failed'
  | 'sweep:started'
  | 'sweep:progress'
  | 'sweep:complete'
  | 'sweep:failed'
  | 'sweep:cancelled';

/** Map event type to payload type */
export interface EventPayloadMap {
  'job:progress': JobProgressPayload;
  'job:complete': JobCompletePayload;
  'job:failed': JobFailedPayload;
  'job:cancelled': JobCompletePayload;
  'data:progress': DataProgressPayload;
  'data:complete': DataCompletePayload;
  'data:failed': JobFailedPayload;
  'sweep:started': { job_id: string };
  'sweep:progress': SweepProgressPayload;
  'sweep:complete': SweepCompletePayload;
  'sweep:failed': JobFailedPayload;
  'sweep:cancelled': JobCompletePayload;
}
