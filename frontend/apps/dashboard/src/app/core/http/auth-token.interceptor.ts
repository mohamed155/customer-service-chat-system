import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { APP_CONFIG } from '../config/app-config';

export const authTokenInterceptor: HttpInterceptorFn = (request, next) => {
  const { apiBaseUrl } = inject(APP_CONFIG);

  if (!targetsApiBaseUrl(request.url, apiBaseUrl)) {
    return next(request);
  }

  return next(request.clone({ withCredentials: true }));
};

function targetsApiBaseUrl(requestUrl: string, apiBaseUrl: string): boolean {
  const apiBase = normalizeBase(apiBaseUrl);
  const apiPath = pathnameFromUrl(apiBase);
  const requestPath = pathnameFromUrl(requestUrl);

  if (!pathMatchesBase(requestPath, apiPath)) {
    return false;
  }

  if (!isAbsoluteUrl(apiBase) || !isAbsoluteUrl(requestUrl)) {
    return true;
  }

  return new URL(requestUrl).origin === new URL(apiBase).origin;
}

function normalizeBase(url: string): string {
  return url.replace(/\/+$/, '');
}

function pathnameFromUrl(url: string): string {
  if (!isAbsoluteUrl(url)) {
    return `/${url.replace(/^\//, '').split(/[?#]/, 1)[0].replace(/\/+$/, '')}`;
  }

  return new URL(url).pathname.replace(/\/+$/, '');
}

function pathMatchesBase(path: string, basePath: string): boolean {
  return path === basePath || path.startsWith(`${basePath}/`);
}

function isAbsoluteUrl(url: string): boolean {
  return /^https?:\/\//i.test(url);
}
