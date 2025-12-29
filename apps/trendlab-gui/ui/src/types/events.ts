import type { ErrorEnvelope } from './error';
// Import sweep types directly - they are the authoritative source
import type {
  SweepProgress,
  SweepProgressPayload,
  SweepCompletePayload,
} from './sweep';
import type { Leaderboard, CrossSymbolLeaderboard } from './yolo';

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
  symbol: string;
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

// Note: SweepProgressPayload and SweepCompletePayload are defined in sweep.ts
// They are imported above for use in EventPayloadMap but not re-exported here

// ============================================================================
// YOLO Event Payloads
// ============================================================================

/** YOLO started event payload */
export interface YoloStartedPayload {
  jobId: string;
  totalSymbols: number;
  totalStrategies: number;
  randomizationPct: number;
}

/** YOLO progress event payload */
export interface YoloProgressPayload {
  iteration: number;
  phase: string;
  completedConfigs: number;
  totalConfigs: number;
}

/** YOLO iteration complete event payload */
export interface YoloIterationCompletePayload {
  iteration: number;
  crossSymbolLeaderboard: CrossSymbolLeaderboard;
  perSymbolLeaderboard: Leaderboard;
  configsTestedThisRound: number;
}

/** YOLO stopped event payload */
export interface YoloStoppedPayload {
  crossSymbolLeaderboard: CrossSymbolLeaderboard;
  perSymbolLeaderboard: Leaderboard;
  totalIterations: number;
  totalConfigsTested: number;
}

// ============================================================================
// Worker Event Payloads (from trendlab-engine worker updates)
// ============================================================================

/** Worker fetch started */
export interface WorkerFetchStartedPayload {
  symbol: string;
  index: number;
  total: number;
}

/** Worker fetch complete */
export interface WorkerFetchCompletePayload {
  symbol: string;
}

/** Worker fetch all complete */
export interface WorkerFetchAllCompletePayload {
  symbolsFetched: number;
}

/** Worker sweep started */
export interface WorkerSweepStartedPayload {
  totalConfigs: number;
}

/** Worker sweep progress */
export interface WorkerSweepProgressPayload {
  completed: number;
  total: number;
}

/** Worker sweep cancelled */
export interface WorkerSweepCancelledPayload {
  completed: number;
}

/** Worker YOLO iteration */
export interface WorkerYoloIterationPayload {
  iteration: number;
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
  | 'sweep:cancelled'
  | 'yolo:started'
  | 'yolo:progress'
  | 'yolo:iteration_complete'
  | 'yolo:stopped'
  // Worker events (from trendlab-engine)
  | 'worker:search-results'
  | 'worker:fetch-started'
  | 'worker:fetch-complete'
  | 'worker:fetch-all-complete'
  | 'worker:sweep-started'
  | 'worker:sweep-progress'
  | 'worker:sweep-complete'
  | 'worker:sweep-cancelled'
  | 'worker:yolo-iteration'
  | 'worker:yolo-stopped';

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
  'yolo:started': YoloStartedPayload;
  'yolo:progress': YoloProgressPayload;
  'yolo:iteration_complete': YoloIterationCompletePayload;
  'yolo:stopped': YoloStoppedPayload;
  // Worker events
  'worker:search-results': Record<string, never>;
  'worker:fetch-started': WorkerFetchStartedPayload;
  'worker:fetch-complete': WorkerFetchCompletePayload;
  'worker:fetch-all-complete': WorkerFetchAllCompletePayload;
  'worker:sweep-started': WorkerSweepStartedPayload;
  'worker:sweep-progress': WorkerSweepProgressPayload;
  'worker:sweep-complete': Record<string, never>;
  'worker:sweep-cancelled': WorkerSweepCancelledPayload;
  'worker:yolo-iteration': WorkerYoloIterationPayload;
  'worker:yolo-stopped': Record<string, never>;
}
