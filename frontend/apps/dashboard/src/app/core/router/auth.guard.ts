import { inject } from '@angular/core';
import { CanMatchFn, UrlSegment, Router } from '@angular/router';
import { CurrentUserService } from '../tenant/current-user.service';
import { APP_PATHS } from './app-paths';

const loginPath = `/${APP_PATHS.auth.base}/${APP_PATHS.auth.login}`;

export const authGuard: CanMatchFn = (_route, segments) => {
  const currentUser = inject(CurrentUserService);
  const router = inject(Router);

  if (currentUser.currentUser() != null) {
    return true;
  }

  return router.parseUrl(`${loginPath}?returnUrl=${encodeURIComponent(attemptedUrl(segments))}`);
};

const attemptedUrl = (segments: UrlSegment[]): string => {
  const path = segments.map((segment) => segment.path).join('/');
  return path.length > 0 ? `/${path}` : '/';
};
