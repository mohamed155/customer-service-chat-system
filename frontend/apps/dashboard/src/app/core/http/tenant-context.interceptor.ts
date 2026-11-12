import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { Store } from '@ngrx/store';
import { takeUntil } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { selectActiveTenant } from '../state/tenant-context.feature';
import { TenantScope } from '../state/tenant-scope';
import {
  apiRelativePath,
  isIdentityEndpoint,
  isPlatformEndpoint,
  isTenantEndpoint,
} from './api-request';

/**
 * Sends the active workspace with every workspace API call. The identity endpoint (`/me`), the
 * platform routes and any URL outside the API are left alone. The match is on exact path
 * segments: `tenant/members` and `tenant/availability/me` are workspace calls (MOH-60).
 *
 * Calls under `/tenant/` are also cancelled when the active workspace changes.
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
  const request = req.clone({ setHeaders: { 'X-Tenant-ID': tenant.id } });
  if (!isTenantEndpoint(path)) {
    return next(request);
  }

  // A request for the old workspace's data is cancelled when the workspace changes, so its
  // response can never reach a store that now belongs to the new one (MOH-61). The caller sees
  // the stream complete without a value.
  return next(request).pipe(takeUntil(inject(TenantScope).changes$));
};
