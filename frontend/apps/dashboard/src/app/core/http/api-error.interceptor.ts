import { HttpInterceptorFn } from '@angular/common/http';
import { inject, Injector } from '@angular/core';
import { Router } from '@angular/router';
import { Store } from '@ngrx/store';
import { catchError, throwError } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { MFA_ENROLMENT_REQUIRED } from '../auth/mfa.service';
import { ApiErrorNotificationService } from '../errors/api-error-notification.service';
import { isApiError } from '../errors/api-error';
import { errorMessageKey } from '../errors/error-message';
import { mapHttpError } from '../errors/http-error.mapper';
import { LocaleService } from '../i18n/locale.service';
import { marker } from '../i18n/marker';
import { REQUEST_ID_HEADER } from '../errors/request-reference';
import { APP_PATHS } from '../router/app-paths';
import { selectActiveTenant, tenantContextActions } from '../state/tenant-context.feature';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';
import {
  apiRelativePath,
  isIdentityEndpoint,
  isLoginEndpoint,
  isPasswordResetEndpoint,
  isPlatformEndpoint,
  isRefreshEndpoint,
} from './api-request';

export const apiErrorInterceptor: HttpInterceptorFn = (request, next) => {
  const store = inject(Store);
  const injector = inject(Injector);
  const router = inject(Router);
  const config = inject(APP_CONFIG);
  const currentUser = inject(CurrentUserService);
  const errorNotifications = inject(ApiErrorNotificationService);
  const locale = inject(LocaleService);
  const activeTenant = store.selectSignal(selectActiveTenant);

  const path = apiRelativePath(request.url, config.apiBaseUrl);
  const requestTenant = activeTenant();
  // The access session held when the request went out, so a refusal that arrives after another
  // session replaced it does not end the replacement. Read lazily: the service needs HttpClient.
  const requestSessionId = injector.get(TenantContextService).accessSession()?.id ?? null;

  return next(request).pipe(
    catchError((error: unknown) => {
      // A nested refresh request has already been mapped with its own request reference.
      const apiError = isApiError(error)
        ? error
        : mapHttpError(error, request.headers.get(REQUEST_ID_HEADER));
      if (isSessionExpired(apiError, path)) {
        currentUser.clear();
        errorNotifications.show(
          locale.translate(marker('errors.sessionExpired')),
          apiError.requestId,
        );
        void router.navigate([`/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`], {
          queryParams: { returnUrl: currentPath() },
        });
      } else if (
        apiError.status === 403 &&
        path !== null &&
        !isPlatformEndpoint(path) &&
        (apiError.code === 'platform_access_required' ||
          apiError.code === 'platform_access_expired')
      ) {
        const tenantContext = injector.get(TenantContextService);
        if (
          tenantContext.rejectAccess(
            request.headers.get('X-Tenant-ID'),
            requestTenant,
            requestSessionId,
          )
        ) {
          errorNotifications.show(locale.translate(errorMessageKey(apiError)), apiError.requestId);
          void router.navigate([`/${APP_PATHS.platform.base}`, APP_PATHS.platform.tenants]);
        }
      } else if (apiError.status === 403 && apiError.code === MFA_ENROLMENT_REQUIRED) {
        // The account needs a second factor before anything else: send the user to set one up,
        // and bring them back to where they were afterwards.
        if (!router.url.startsWith(`/${APP_PATHS.mfaSetup}`)) {
          void router.navigate([`/${APP_PATHS.mfaSetup}`], {
            queryParams: { returnUrl: currentPath() },
          });
        }
      } else if (
        apiError.status === 403 &&
        apiError.code === 'unauthorized' &&
        path !== null &&
        !isPasswordResetEndpoint(path)
      ) {
        if (!isIdentityEndpoint(path)) {
          void currentUser.load();
        }
        if (!isIdentityEndpoint(path) && !isPlatformEndpoint(path)) {
          errorNotifications.show(locale.translate(errorMessageKey(apiError)), apiError.requestId);
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

const isSessionExpired = (error: { status: number; code: string }, path: string | null): boolean =>
  error.status === 401 &&
  error.code === 'unauthenticated' &&
  path !== null &&
  !isIdentityEndpoint(path) &&
  !isLoginEndpoint(path) &&
  !isPasswordResetEndpoint(path) &&
  // A refresh that fails is reported by the call that needed it, once, not twice.
  !isRefreshEndpoint(path);

const currentPath = (): string => {
  if (typeof window === 'undefined') return '/';
  return `${window.location.pathname}${window.location.search}`;
};
