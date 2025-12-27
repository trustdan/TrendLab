/** Error codes (stable identifiers) */
export type ErrorCode =
  | 'InvalidInput'
  | 'NotFound'
  | 'Io'
  | 'ProviderError'
  | 'DataError'
  | 'BacktestError'
  | 'Cancelled'
  | 'Internal'
  | 'Unknown';

/** Error envelope for command failures and job errors */
export interface ErrorEnvelope {
  code: ErrorCode;
  message: string;
  details?: Record<string, unknown>;
  retryable: boolean;
}

/** Check if error is retryable */
export function isRetryable(error: ErrorEnvelope): boolean {
  return error.retryable;
}

/** Create error from unknown catch value */
export function toErrorEnvelope(error: unknown): ErrorEnvelope {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    return error as ErrorEnvelope;
  }
  if (error instanceof Error) {
    return {
      code: 'Unknown',
      message: error.message,
      retryable: false,
    };
  }
  return {
    code: 'Unknown',
    message: String(error),
    retryable: false,
  };
}

/** User-friendly error messages */
export const ERROR_MESSAGES: Record<ErrorCode, string> = {
  InvalidInput: 'Invalid input provided',
  NotFound: 'Resource not found',
  Io: 'I/O error occurred',
  ProviderError: 'Data provider error',
  DataError: 'Data processing error',
  BacktestError: 'Backtest execution error',
  Cancelled: 'Operation was cancelled',
  Internal: 'Internal error',
  Unknown: 'An unexpected error occurred',
};
