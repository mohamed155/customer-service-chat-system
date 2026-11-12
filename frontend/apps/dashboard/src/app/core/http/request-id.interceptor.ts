import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { APP_CONFIG } from '../config/app-config';
import { newRequestId, REQUEST_ID_HEADER } from '../errors/request-reference';
import { apiRelativePath } from './api-request';

/**
 * Gives every API call its own ID, so an error on screen can be matched to the backend's logs
 * (principle VII). Calls to other servers get none, and a call that already carries one keeps it.
 */
export const requestIdInterceptor: HttpInterceptorFn = (req, next) => {
  const { apiBaseUrl } = inject(APP_CONFIG);
  if (apiRelativePath(req.url, apiBaseUrl) === null || req.headers.has(REQUEST_ID_HEADER)) {
    return next(req);
  }
  return next(req.clone({ setHeaders: { [REQUEST_ID_HEADER]: newRequestId() } }));
};
