import { inject } from '@angular/core';
import { CanMatchFn, Router } from '@angular/router';
import { APP_PATHS } from '../router/app-paths';
import { PermissionsService } from './permissions.service';
import { PAGE_PERMISSIONS, Permission } from './permissions';

const SIDEBAR_PAGE_ORDER = [
  APP_PATHS.tenant.overview,
  APP_PATHS.tenant.conversations,
  APP_PATHS.tenant.customers,
  APP_PATHS.tenant.aiAgent,
  APP_PATHS.tenant.knowledgeBase,
  APP_PATHS.tenant.integrations,
  APP_PATHS.tenant.analytics,
  APP_PATHS.tenant.settings,
] as const;

const findFirstPermitted = (permissions: PermissionsService): string | null => {
  for (const page of SIDEBAR_PAGE_ORDER) {
    const perm = PAGE_PERMISSIONS[page as keyof typeof PAGE_PERMISSIONS];
    if (perm && permissions.has(perm as Permission)) {
      return `/${APP_PATHS.tenant.base}/${page}`;
    }
  }
  return null;
};

export const permissionGuard: CanMatchFn = (route) => {
  const permissions = inject(PermissionsService);
  const router = inject(Router);

  const required = route.data?.['requiredPermission'] as Permission | undefined;
  if (!required) return false;

  if (permissions.has(required)) return true;

  const first = findFirstPermitted(permissions);
  if (first) return router.createUrlTree([first]);

  return router.createUrlTree(['/', APP_PATHS.tenant.base, APP_PATHS.tenant.select]);
};
