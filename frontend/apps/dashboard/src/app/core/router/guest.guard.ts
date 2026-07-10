import { inject } from '@angular/core';
import { CanMatchFn, Router } from '@angular/router';
import { CurrentUserService } from '../tenant/current-user.service';
import { APP_PATHS } from './app-paths';

export const guestGuard: CanMatchFn = () => {
  const currentUser = inject(CurrentUserService);
  const router = inject(Router);

  if (currentUser.currentUser() == null) {
    return true;
  }

  return router.parseUrl(`/${APP_PATHS.root}`);
};
