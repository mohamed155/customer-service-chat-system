import { MeResponse, TenantSummary } from '../api/tenant-api.models';

export const TENANT_ROLE_LABELS: Record<string, string> = {
  owner: 'Owner',
  admin: 'Admin',
  manager: 'Manager',
  agent: 'Support Agent',
  viewer: 'Viewer',
};

export const PLATFORM_ROLE_LABELS: Record<string, string> = {
  super_admin: 'Super Admin',
  developer: 'Developer',
  support: 'Support Engineer',
  sales: 'Sales',
  finance: 'Finance',
};

export function roleLabel(
  user: MeResponse | null,
  activeTenant: TenantSummary | null,
): string | null {
  if (!user) {
    return null;
  }

  if (user.platformRole) {
    return PLATFORM_ROLE_LABELS[user.platformRole] ?? null;
  }

  if (activeTenant) {
    const membership = user.memberships.find((m) => m.tenantId === activeTenant.id);
    if (membership) {
      const label = TENANT_ROLE_LABELS[membership.role];
      return label ? `${label} · ${activeTenant.name}` : null;
    }
  }

  return null;
}
