import { marker } from '../i18n/marker';
import { MeResponse, TenantSummary } from '../api/tenant-api.models';

/** A role line for the user menu: a catalog key, and the workspace name when the key takes one. */
export interface RoleLine {
  readonly key: string;
  readonly params?: { readonly tenant: string };
}

/** Catalog keys of a role inside a workspace: "Owner · {tenant}". */
export const TENANT_ROLE_KEYS: Record<string, string> = {
  owner: marker('roles.tenant.owner'),
  admin: marker('roles.tenant.admin'),
  manager: marker('roles.tenant.manager'),
  agent: marker('roles.tenant.agent'),
  viewer: marker('roles.tenant.viewer'),
};

/** Catalog keys of a platform staff role. */
export const PLATFORM_ROLE_KEYS: Record<string, string> = {
  super_admin: marker('roles.platform.super_admin'),
  developer: marker('roles.platform.developer'),
  support: marker('roles.platform.support'),
  sales: marker('roles.platform.sales'),
  finance: marker('roles.platform.finance'),
};

/** What to show under the user's name: platform staff by role, others by role in the workspace. */
export function roleLine(
  user: MeResponse | null,
  activeTenant: TenantSummary | null,
): RoleLine | null {
  if (!user) {
    return null;
  }

  if (user.platformRole) {
    const key = PLATFORM_ROLE_KEYS[user.platformRole];
    return key ? { key } : null;
  }

  if (activeTenant) {
    const membership = user.memberships.find((m) => m.tenantId === activeTenant.id);
    if (membership) {
      const key = TENANT_ROLE_KEYS[membership.role];
      return key ? { key, params: { tenant: activeTenant.name } } : null;
    }
  }

  return null;
}
