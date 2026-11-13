import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { retry, throwError, timeout, timer } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { apiRelativePath } from './api-request';
import {
  HTTP_RETRY,
  HTTP_TIMEOUT_MS,
  isSafeMethod,
  MAX_RETRIES,
  RETRY_RANDOM,
  retryDelayMs,
} from './http-resilience';

/**
 * Timeout and retry for API calls (MOH-166). It sits closest to the network, inside the error
 * interceptor, so it sees the raw HTTP error (status and `Retry-After`) and the error interceptor
 * maps whatever finally gets through, a timeout included, to an `ApiError`.
 *
 * Each attempt has its own timeout, and a timeout is not retried: the server may be working on it.
 * Calls to other servers are left alone.
 */
export const httpResilienceInterceptor: HttpInterceptorFn = (req, next) => {
  const { apiBaseUrl } = inject(APP_CONFIG);
  if (apiRelativePath(req.url, apiBaseUrl) === null) return next(req);

  const random = inject(RETRY_RANDOM);
  const limit = req.context.get(HTTP_TIMEOUT_MS);
  let attempt = next(req);
  if (limit > 0) attempt = attempt.pipe(timeout({ each: limit }));

  if (!isSafeMethod(req.method) || !req.context.get(HTTP_RETRY)) return attempt;
  return attempt.pipe(
    retry({
      count: MAX_RETRIES,
      delay: (error, retryCount) => {
        const wait = retryDelayMs(error, retryCount, random);
        return wait === null ? throwError(() => error) : timer(wait);
      },
    }),
  );
};
