import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { APP_CONFIG } from '../config/app-config';
import { targetsApiBaseUrl } from './api-request';

export const authTokenInterceptor: HttpInterceptorFn = (request, next) => {
  const { apiBaseUrl } = inject(APP_CONFIG);

  if (!targetsApiBaseUrl(request.url, apiBaseUrl)) {
    return next(request);
  }

  // The session cookie rides on these requests, so each one also carries a header a cross-site
  // form can't set. The backend rejects state-changing cookie requests without it (MOH-36).
  return next(
    request.clone({ withCredentials: true, setHeaders: { 'X-Requested-With': 'XMLHttpRequest' } }),
  );
};
