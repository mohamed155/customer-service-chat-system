import { HttpContext, HttpContextToken, HttpErrorResponse } from '@angular/common/http';
import { InjectionToken } from '@angular/core';

/**
 * Timeout and retry rules for API calls (MOH-166, principle XI).
 *
 * A call that gets no answer for `DEFAULT_TIMEOUT_MS` fails with a `timeout` error instead of
 * leaving a spinner on screen for ever. A safe call (GET, HEAD) that failed for a reason that is
 * likely to pass, a dropped connection or a gateway answer during a deploy, is tried again, at most
 * `MAX_RETRIES` times, after a delay that grows and is randomised (full jitter) so that many open
 * dashboards do not come back at the same moment.
 */
export const DEFAULT_TIMEOUT_MS = 30_000;
export const MAX_RETRIES = 2;
export const BACKOFF_BASE_MS = 400;
export const BACKOFF_CAP_MS = 4_000;
/** A longer `Retry-After` than this is not waited for: the error is shown instead. */
export const MAX_RETRY_AFTER_MS = 10_000;

/**
 * How long one call may go without an answer, in milliseconds; `0` for no limit. An upload sets it
 * to `0` with `noTimeout()`.
 */
export const HTTP_TIMEOUT_MS = new HttpContextToken<number>(() => DEFAULT_TIMEOUT_MS);

/** Set to `false` to send a safe call exactly once. */
export const HTTP_RETRY = new HttpContextToken<boolean>(() => true);

/** The source of randomness for the backoff; a test replaces it to pin the delays. */
export const RETRY_RANDOM = new InjectionToken<() => number>('RETRY_RANDOM', {
  providedIn: 'root',
  factory: () => Math.random,
});

/** The context for a call that may take as long as it needs, such as an upload. */
export function noTimeout(context: HttpContext = new HttpContext()): HttpContext {
  return context.set(HTTP_TIMEOUT_MS, 0);
}

export const isSafeMethod = (method: string): boolean => method === 'GET' || method === 'HEAD';

/** A dropped connection or a request that never left: no HTTP answer at all. */
const isNetworkFailure = (error: HttpErrorResponse): boolean =>
  error.status === 0 ||
  (typeof ProgressEvent !== 'undefined' && error.error instanceof ProgressEvent);

/** `Retry-After` as milliseconds: delta-seconds or an HTTP date. `null` when absent or invalid. */
export function retryAfterMs(value: string | null, now = Date.now()): number | null {
  if (value === null || value.trim() === '') return null;
  const seconds = Number(value);
  if (Number.isFinite(seconds)) return seconds >= 0 ? Math.round(seconds * 1000) : null;
  const at = Date.parse(value);
  return Number.isNaN(at) ? null : Math.max(0, at - now);
}

/**
 * How long to wait before attempt number `attempt` (1 for the first retry), or `null` when this
 * error is not worth another try.
 *
 * Tried again: no answer at all, 502, 503 and 504, and 429 only when the server says when
 * (`Retry-After`). Never any other 4xx. A `Retry-After` is the delay when there is one (so it is
 * waited for as sent, with no jitter), unless it is longer than `MAX_RETRY_AFTER_MS`.
 */
export function retryDelayMs(
  error: unknown,
  attempt: number,
  random: () => number,
  now = Date.now(),
): number | null {
  if (!(error instanceof HttpErrorResponse)) return null;
  const retryAfter = retryAfterMs(error.headers?.get('Retry-After') ?? null, now);

  if (error.status === 429) {
    return retryAfter !== null && retryAfter <= MAX_RETRY_AFTER_MS ? retryAfter : null;
  }
  const worthRetrying = isNetworkFailure(error) || [502, 503, 504].includes(error.status);
  if (!worthRetrying) return null;
  if (retryAfter !== null) return retryAfter <= MAX_RETRY_AFTER_MS ? retryAfter : null;

  // Full jitter: anywhere between 0 and the capped exponential step.
  const ceiling = Math.min(BACKOFF_CAP_MS, BACKOFF_BASE_MS * 2 ** (attempt - 1));
  return Math.floor(random() * ceiling);
}
