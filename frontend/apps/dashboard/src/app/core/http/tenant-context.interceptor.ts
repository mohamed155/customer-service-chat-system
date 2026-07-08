import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { Store } from '@ngrx/store';
import { selectActiveTenant } from '../state/tenant-context.feature';

export const tenantContextInterceptor: HttpInterceptorFn = (req, next) => {
  const store = inject(Store);
  const tenant = toSignal(store.select(selectActiveTenant))();

  const url = req.url;
  if (url.includes('/me') || url.includes('/platform/')) {
    return next(req);
  }

  if (tenant?.id) {
    const cloned = req.clone({
      setHeaders: { 'X-Tenant-ID': tenant.id },
    });
    return next(cloned);
  }

  return next(req);
};
