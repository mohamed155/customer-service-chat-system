import { inject } from '@angular/core';
import { CanMatchFn, Router } from '@angular/router';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';

export const areaAccessGuard: CanMatchFn = (route) => {
  const currentUser = inject(CurrentUserService);
  const tenantContext = inject(TenantContextService);
  const router = inject(Router);

  const area = route.data?.['area'] as string | undefined;

  if (area === 'platform') {
    if (!currentUser.isPlatformUser()) {
      return router.parseUrl('/');
    }
    return true;
  }

  if (area === 'tenant') {
    if (!tenantContext.activeTenant()) {
      return router.parseUrl('/tenant/select');
    }
    return true;
  }

  return true;
};
