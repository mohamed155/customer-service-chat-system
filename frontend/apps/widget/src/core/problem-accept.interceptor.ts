import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { WIDGET_API_BASE } from './widget-api.service';

/**
 * What the widget asks its API for: an error as RFC 9457 `application/problem+json`, and the
 * legacy `application/json` envelope as a fallback at half the weight (ADR-012 gap A2). The widget
 * reads either (`readErrorCode`), and it asks without waiting for the server to change its default:
 * a build already embedded on a customer's page sends no preference and keeps getting the legacy
 * format. A request that already says what it accepts keeps it, and only the widget's own API is
 * asked.
 */
export const ACCEPT_PROBLEM_JSON = 'application/problem+json, application/json;q=0.5';

export const problemAcceptInterceptor: HttpInterceptorFn = (request, next) => {
  const base = inject(WIDGET_API_BASE);
  if (!request.url.startsWith(`${base}/`) || request.headers.has('Accept')) {
    return next(request);
  }
  return next(request.clone({ setHeaders: { Accept: ACCEPT_PROBLEM_JSON } }));
};
