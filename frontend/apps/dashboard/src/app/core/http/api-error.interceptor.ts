import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { Store } from '@ngrx/store';
import { catchError, throwError } from 'rxjs';
import { mapHttpError } from '../errors/http-error.mapper';
import { selectActiveTenant, tenantContextActions } from '../state/tenant-context.feature';

export const apiErrorInterceptor: HttpInterceptorFn = (request, next) => {
  const store = inject(Store);
  return next(request).pipe(
    catchError((error: unknown) => {
      const apiError = mapHttpError(error);
      if (apiError.status === 403 && apiError.code === 'unauthorized') {
        const url = request.url;
        if (!url.includes('/me') && !url.includes('/platform/')) {
          const tenantId = request.headers.get('X-Tenant-ID');
          const activeTenant = toSignal(store.select(selectActiveTenant))();
          if (tenantId && activeTenant?.id === tenantId) {
            store.dispatch(tenantContextActions.clearActiveTenant());
          }
        }
      }
      return throwError(() => apiError);
    }),
  );
};
