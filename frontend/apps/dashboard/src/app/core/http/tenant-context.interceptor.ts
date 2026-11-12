import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { Store } from '@ngrx/store';
import { APP_CONFIG } from '../config/app-config';
import { selectActiveTenant } from '../state/tenant-context.feature';
import { apiRelativePath, isIdentityEndpoint, isPlatformEndpoint } from './api-request';

/**
 * Sends the active workspace with every workspace API call. The identity endpoint (`/me`), the
 * platform routes and any URL outside the API are left alone. The match is on exact path
 * segments: `tenant/members` and `tenant/availability/me` are workspace calls (MOH-60).
 */
export const tenantContextInterceptor: HttpInterceptorFn = (req, next) => {
  const { apiBaseUrl } = inject(APP_CONFIG);
  const path = apiRelativePath(req.url, apiBaseUrl);
  if (path === null || isIdentityEndpoint(path) || isPlatformEndpoint(path)) {
    return next(req);
  }

  // A signal read, not a subscription: nothing is left behind per request.
  const tenant = inject(Store).selectSignal(selectActiveTenant)();
  if (!tenant?.id) {
    return next(req);
  }
  return next(req.clone({ setHeaders: { 'X-Tenant-ID': tenant.id } }));
};
