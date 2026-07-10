import { computed, inject, Injectable, Signal } from '@angular/core';
import { TenantContextService } from '../tenant/tenant-context.service';
import { CurrentUserService } from '../tenant/current-user.service';
import { Permission } from './permissions';

@Injectable({ providedIn: 'root' })
export class PermissionsService {
  private readonly currentUserService = inject(CurrentUserService);
  private readonly tenantContext = inject(TenantContextService);

  readonly effective: Signal<Set<Permission>> = computed(() => {
    const user = this.currentUserService.currentUser();
    const tenant = this.tenantContext.activeTenant();

    if (!user) return new Set<Permission>();

    const all = new Set<Permission>(user.platformPermissions);

    if (tenant) {
      if (user.platformRole && user.staffTenantPermissions) {
        for (const p of user.staffTenantPermissions) {
          all.add(p);
        }
      } else {
        const membership = user.memberships.find((m) => m.tenantId === tenant.id);
        if (membership) {
          for (const p of membership.permissions) {
            all.add(p);
          }
        }
      }
    }

    return all;
  });

  has(permission: Permission): boolean {
    return this.effective().has(permission);
  }
}
