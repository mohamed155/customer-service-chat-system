import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { APP_CONFIG } from '../config/app-config';

export const devIdentityInterceptor: HttpInterceptorFn = (req, next) => {
  const env = inject(APP_CONFIG).environmentName;
  if (env !== 'development') {
    return next(req);
  }

  const devUserId = localStorage.getItem('app.devUserId');
  if (devUserId) {
    const cloned = req.clone({
      setHeaders: { 'X-Dev-User-Id': devUserId },
    });
    return next(cloned);
  }

  return next(req);
};
