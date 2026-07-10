import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { Router } from '@angular/router';
import { Store } from '@ngrx/store';
import { catchError, throwError } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { ApiErrorNotificationService } from '../errors/api-error-notification.service';
import { mapHttpError, userMessageFor } from '../errors/http-error.mapper';
import { APP_PATHS } from '../router/app-paths';
import { selectActiveTenant, tenantContextActions } from '../state/tenant-context.feature';
import { CurrentUserService } from '../tenant/current-user.service';

export const apiErrorInterceptor: HttpInterceptorFn = (request, next) => {
  const store = inject(Store);
  const router = inject(Router);
  const config = inject(APP_CONFIG);
  const currentUser = inject(CurrentUserService);
  const errorNotifications = inject(ApiErrorNotificationService);
  const activeTenant = store.selectSignal(selectActiveTenant);

  return next(request).pipe(
    catchError((error: unknown) => {
      const apiError = mapHttpError(error);
      if (isSessionExpired(apiError, request.url, config.apiBaseUrl)) {
        currentUser.clear();
        errorNotifications.show('Your session has expired. Sign in again to continue.');
        void router.navigate([`/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`], {
          queryParams: { returnUrl: currentPath() },
        });
      } else if (apiError.status === 403 && apiError.code === 'unauthorized') {
        const url = request.url;
        if (!url.includes('/me')) {
          void currentUser.load();
        }
        if (!url.includes('/me') && !url.includes('/platform/')) {
          errorNotifications.show(userMessageFor(apiError));
          const tenantId = request.headers.get('X-Tenant-ID');
          if (tenantId && activeTenant()?.id === tenantId) {
            store.dispatch(tenantContextActions.clearActiveTenant());
          }
        }
      }
      return throwError(() => apiError);
    }),
  );
};

const isSessionExpired = (
  error: { status: number; code: string },
  requestUrl: string,
  apiBaseUrl: string,
): boolean =>
  error.status === 401 &&
  error.code === 'unauthenticated' &&
  targetsApiBaseUrl(requestUrl, apiBaseUrl) &&
  !requestUrl.includes('/auth/login');

const targetsApiBaseUrl = (requestUrl: string, apiBaseUrl: string): boolean => {
  const apiBase = apiBaseUrl.replace(/\/+$/, '');
  const path = requestUrl.startsWith('http') ? new URL(requestUrl).pathname : requestUrl;
  return path === apiBase || path.startsWith(`${apiBase}/`);
};

const currentPath = (): string => {
  if (typeof window === 'undefined') return '/';
  return `${window.location.pathname}${window.location.search}`;
};
