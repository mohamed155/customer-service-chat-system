import { HttpContextToken, HttpErrorResponse, HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { catchError, switchMap, throwError } from 'rxjs';
import { SessionRefreshService } from '../auth/session-refresh.service';
import { APP_CONFIG } from '../config/app-config';
import { mapHttpError } from '../errors/http-error.mapper';
import { apiRelativePath, isSessionEndpoint } from './api-request';

/** Marks a request that was already sent again after a refresh, so it is retried only once. */
const REFRESHED = new HttpContextToken<boolean>(() => false);

/**
 * A call that answers `401 unauthenticated` met an expired access token (they last ten minutes).
 * Ask for a new one once (`SessionRefreshService`, single flight) and send the call again. If the
 * refresh token is refused, forward the original 401 for session-expired handling. Other refresh
 * errors propagate as they are. This sits inside `apiErrorInterceptor` so it sees the original
 * call's raw HTTP error before the session-expired handling does.
 *
 * Not refreshed: the calls that start or end a session, a `401` with another code (the step-up
 * prompt answers `401 mfa_step_up_required`), and a call that was already retried.
 */
export const sessionRefreshInterceptor: HttpInterceptorFn = (request, next) => {
  const refresher = inject(SessionRefreshService);
  const { apiBaseUrl } = inject(APP_CONFIG);
  const path = apiRelativePath(request.url, apiBaseUrl);

  if (path === null || isSessionEndpoint(path)) {
    return next(request);
  }

  return next(request).pipe(
    catchError((error: unknown) => {
      const expired =
        error instanceof HttpErrorResponse &&
        error.status === 401 &&
        mapHttpError(error).code === 'unauthenticated' &&
        !request.context.get(REFRESHED);
      if (!expired) {
        return throwError(() => error);
      }
      return refresher
        .refresh()
        .pipe(
          switchMap((refreshed) =>
            refreshed
              ? next(request.clone({ context: request.context.set(REFRESHED, true) }))
              : throwError(() => error),
          ),
        );
    }),
  );
};
