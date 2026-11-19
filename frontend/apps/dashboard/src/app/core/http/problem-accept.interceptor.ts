import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { APP_CONFIG } from '../config/app-config';
import { targetsApiBaseUrl } from './api-request';

/**
 * What the dashboard asks the API for: an error as RFC 9457 `application/problem+json`, and the
 * legacy `application/json` envelope as a fallback at half the weight (ADR-012 gap A2). The error
 * mapper reads both, so it does not matter which one the backend answers with. A request that
 * already says what it accepts (a download, say) keeps it, and calls to other servers get nothing.
 */
export const ACCEPT_PROBLEM_JSON = 'application/problem+json, application/json;q=0.5';

export const problemAcceptInterceptor: HttpInterceptorFn = (request, next) => {
  const { apiBaseUrl } = inject(APP_CONFIG);
  if (!targetsApiBaseUrl(request.url, apiBaseUrl) || request.headers.has('Accept')) {
    return next(request);
  }
  return next(request.clone({ setHeaders: { Accept: ACCEPT_PROBLEM_JSON } }));
};
