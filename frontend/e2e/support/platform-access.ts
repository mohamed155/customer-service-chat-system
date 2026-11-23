import { Page, Route } from '@playwright/test';

interface Tenant {
  readonly id: string;
  readonly name: string;
}

export function platformAccessSession(tenant: Tenant, reason: string, durationMinutes: number) {
  return {
    id: `access-${tenant.id}`,
    tenantId: tenant.id,
    tenantName: tenant.name,
    reason,
    startedAt: new Date().toISOString(),
    expiresAt: new Date(Date.now() + durationMinutes * 60_000).toISOString(),
  };
}

export function noPlatformAccess(route: Route) {
  return route.fulfill({
    status: 404,
    json: { code: 'not_found', status: 404, detail: 'No open tenant access session' },
  });
}

/** Provides the platform session endpoints for mock browser journeys. */
export function mockPlatformAccess(
  tenants: readonly Tenant[],
  options: {
    initialTenant?: Tenant;
    beforeStart?: Promise<void>;
    onStart?: (tenantId: string) => void;
  } = {},
) {
  let active = options.initialTenant
    ? platformAccessSession(options.initialTenant, 'Investigating a customer request', 60)
    : null;
  return (route: Route, path: string): Promise<void> | undefined => {
    if (path === '/platform/access-session') {
      return active ? route.fulfill({ json: active }) : noPlatformAccess(route);
    }
    if (path === '/platform/access-session/end') {
      active = null;
      return route.fulfill({ status: 204 });
    }
    const tenant = tenants.find(({ id }) => path === `/platform/tenants/${id}/switch`);
    if (tenant) return startAccess(route, tenant);
    // A switch to a tenant the mock does not know must fail, not fall through to a success answer
    // without an access session.
    if (/^\/platform\/tenants\/[^/]+\/switch$/.test(path)) {
      return route.fulfill({
        status: 404,
        json: { code: 'not_found', status: 404, detail: 'Tenant not found' },
      });
    }
    return undefined;
  };

  async function startAccess(route: Route, tenant: Tenant): Promise<void> {
    await options.beforeStart;
    const { reason, durationMinutes } = route.request().postDataJSON() as {
      reason: string;
      durationMinutes: number;
    };
    active = platformAccessSession(tenant, reason, durationMinutes);
    options.onStart?.(tenant.id);
    const { tenantName: _tenantName, ...accessSession } = active;
    await route.fulfill({ json: { ...tenant, accessSession } });
  }
}

/** Completes the reason dialog after a workspace has been selected. */
export async function submitPlatformAccess(page: Page): Promise<void> {
  const dialog = page.getByRole('dialog', { name: /Start access to/ });
  await dialog.getByLabel('Reason').fill('Investigating a customer request');
  await dialog.getByRole('button', { name: 'Start access' }).click();
}
