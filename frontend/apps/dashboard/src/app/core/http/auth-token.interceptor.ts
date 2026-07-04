import { HttpInterceptorFn } from '@angular/common/http';
/** Registered extension point for the future authentication feature. */
export const authTokenInterceptor: HttpInterceptorFn = (request, next) => next(request);
