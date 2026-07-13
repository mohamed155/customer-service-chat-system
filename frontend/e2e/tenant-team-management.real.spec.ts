import { readFileSync } from 'node:fs';
import { Browser, BrowserContext, expect, Page, test } from '@playwright/test';
import pg from 'pg';

const OWNER = '10000000-0000-0000-0000-000000000001';
const MANAGER = '10000000-0000-0000-0000-000000000003';
const MEMBER = '10000000-0000-0000-0000-000000000004';
const PLATFORM = '10000000-0000-0000-0000-000000000006';
const ALPHA = '20000000-0000-0000-0000-000000000001';
const BETA = '20000000-0000-0000-0000-000000000002';
const ADMIN_MEMBERSHIP = '30000000-0000-0000-0000-000000000002';
const MANAGER_MEMBERSHIP = '30000000-0000-0000-0000-000000000003';
const MEMBER_MEMBERSHIP = '30000000-0000-0000-0000-000000000004';
const FOREIGN_MEMBERSHIP = '30000000-0000-0000-0000-000000000005';

const tenants = {
  alpha: { id: ALPHA, name: 'T150 Alpha', slug: 't150-alpha', status: 'active', plan: 'trial' },
  beta: { id: BETA, name: 'T150 Beta', slug: 't150-beta', status: 'active', plan: 'trial' },
};

const database = new pg.Pool({ connectionString: process.env['DATABASE_URL'] });

async function queryJson(sql: string): Promise<unknown> {
  const result = await database.query<Record<string, unknown>>(sql);
  return Object.values(result.rows[0])[0];
}

async function devContext(
  browser: Browser,
  userId: string,
  tenant: (typeof tenants)[keyof typeof tenants],
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({
    extraHTTPHeaders: { 'X-Dev-User-Id': userId, 'X-Tenant-ID': tenant.id },
  });
  await context.addInitScript(
    ({ id, activeTenant }) => {
      localStorage.setItem('app.devUserId', id);
      localStorage.setItem('app.tenant', JSON.stringify(activeTenant));
    },
    { id: userId, activeTenant: tenant },
  );
  return { context, page: await context.newPage() };
}

async function inviteThroughUi(page: Page, email: string, role: string) {
  await page.getByRole('button', { name: 'Invite' }).click();
  await page.getByLabel('Email').fill(email);
  if (role !== 'agent') {
    await page
      .getByRole('dialog')
      .getByRole('button', { name: new RegExp(`^${role}$`, 'i') })
      .click();
  }
  const responsePromise = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/v1/tenant/members/invitations') &&
      response.request().method() === 'POST',
  );
  await page.getByRole('button', { name: 'Send invitation' }).click();
  const response = await responsePromise;
  expect(response.status()).toBe(201);
  const body = await response.json();
  expect(body).toMatchObject({
    invitation: { email, role, status: 'pending' },
    emailSent: false,
    emailDeliveryStatus: 'unconfigured',
  });
  expect(body.acceptUrl).toMatch(/^http:\/\/127\.0\.0\.1:4201\/invite\/[A-Za-z0-9_-]+$/);
  await expect(page.getByRole('heading', { name: 'Invitation sent' })).toBeVisible();
  const acceptUrl = (await page
    .getByLabel('Invitation link')
    .locator('span')
    .textContent())!.trim();
  expect(acceptUrl).toBe(body.acceptUrl);
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  return { acceptUrl, invitationId: body.invitation.id as string };
}

test.beforeAll(async () => {
  const seed = readFileSync('e2e/tenant-team-management.seed.sql', 'utf8');
  await database.query(seed);
});

test.afterAll(() => database.end());

test('T150 browser workflow enforces tenant team lifecycle end to end', async ({ browser }) => {
  const workflowStartedAt = Date.now();
  const ownerAlpha = await devContext(browser, OWNER, tenants.alpha);
  const ownerBeta = await devContext(browser, OWNER, tenants.beta);
  const manager = await devContext(browser, MANAGER, tenants.alpha);
  const member = await devContext(browser, MEMBER, tenants.alpha);
  const platform = await devContext(browser, PLATFORM, tenants.alpha);

  const initialRosterResponse = ownerAlpha.page.waitForResponse((response) =>
    response.url().includes('/api/v1/tenant/members?'),
  );
  await ownerAlpha.page.goto('/tenant/team');
  const initialRoster = await initialRosterResponse;
  expect(initialRoster.status(), `initial roster failed: ${await initialRoster.text()}`).toBe(200);
  await expect(ownerAlpha.page.getByRole('heading', { name: 'Team' })).toBeVisible();
  await expect(ownerAlpha.page.getByText('admin@t150.test')).toBeVisible();
  await expect(ownerAlpha.page.getByText('foreign@t150.test')).toHaveCount(0);

  // Browser tenant switching must replace the roster rather than merge foreign records.
  await platform.page.goto('/tenant/team');
  await expect(platform.page.getByText('admin@t150.test')).toBeVisible();
  await platform.page.getByRole('button', { name: 'Switch tenant' }).click();
  const switchResponse = platform.page.waitForResponse((response) =>
    response.url().endsWith(`/api/v1/platform/tenants/${BETA}/switch`),
  );
  await platform.page.locator('.option').filter({ hasText: 'T150 Beta' }).click();
  expect((await switchResponse).status()).toBe(200);
  await expect
    .poll(() =>
      platform.page.evaluate(() => JSON.parse(localStorage.getItem('app.tenant') ?? 'null')?.id),
    )
    .toBe(BETA);
  await platform.context.setExtraHTTPHeaders({
    'X-Dev-User-Id': PLATFORM,
    'X-Tenant-ID': BETA,
  });
  await platform.page.goto('/tenant/team');
  await expect(platform.page.getByText('foreign@t150.test')).toBeVisible();
  await expect(platform.page.getByText('admin@t150.test')).toHaveCount(0);

  // foreign-membership: crafted mutation is refused and leaves the real foreign row unchanged.
  const foreignStatus = await ownerAlpha.page.evaluate(async (membershipId) => {
    const response = await fetch(`/api/v1/tenant/members/${membershipId}`, {
      method: 'PATCH',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ role: 'viewer' }),
    });
    return response.status;
  }, FOREIGN_MEMBERSHIP);
  expect(foreignStatus).toBe(404);
  expect(
    await queryJson(
      `SELECT json_build_object('role', role, 'tenant_id', tenant_id) FROM tenant_memberships WHERE id = '${FOREIGN_MEMBERSHIP}'`,
    ),
  ).toEqual({ role: 'manager', tenant_id: BETA });

  const revoked = await inviteThroughUi(ownerAlpha.page, 'revoked@t150.test', 'viewer');
  const revokedRow = ownerAlpha.page
    .locator('.invitation-row')
    .filter({ hasText: 'revoked@t150.test' });
  await revokedRow.getByRole('button', { name: 'Revoke' }).click();
  await expect(revokedRow).toContainText('revoked');
  const revokedPage = await ownerAlpha.context.newPage();
  await revokedPage.goto(revoked.acceptUrl);
  await expect(revokedPage.getByRole('heading', { name: 'Invitation issue' })).toBeVisible();

  // Anonymous acceptance creates a real account, membership and httpOnly session cookie.
  await ownerBeta.page.goto('/tenant/team');
  const betaInvite = await inviteThroughUi(ownerBeta.page, 'journey@t150.test', 'agent');
  const inviteeContext = await browser.newContext();
  const inviteePage = await inviteeContext.newPage();
  await inviteePage.goto(betaInvite.acceptUrl);
  await expect(inviteePage).toHaveURL(/\/invite\//);
  await expect(inviteePage.getByRole('heading', { name: 'Accept invitation' })).toBeVisible();
  await inviteePage.getByLabel('Display name').fill('Journey User');
  await inviteePage.getByLabel('Password').fill('validPassword123!');
  await inviteePage.getByRole('button', { name: 'Accept & join' }).click();
  await expect(inviteePage).toHaveURL(/\/tenant\/overview$/);
  await expect(inviteePage.getByRole('link', { name: 'Team' })).toHaveCount(0);
  const sessionCookie = (await inviteePage.context().cookies()).find(
    (cookie) => cookie.name === 'app_session',
  );
  expect(sessionCookie).toMatchObject({ httpOnly: true, sameSite: 'Lax' });
  expect(
    await queryJson(
      "SELECT json_build_object('role', tm.role, 'status', tm.status, 'tenant_id', tm.tenant_id) FROM tenant_memberships tm JOIN users u ON u.id = tm.user_id WHERE u.email = 'journey@t150.test' AND tm.tenant_id = '20000000-0000-0000-0000-000000000002'",
    ),
  ).toEqual({ role: 'agent', status: 'active', tenant_id: BETA });

  // Signed-in acceptance reuses that cookie, adds Alpha, and lands on a permission-valid page.
  const alphaInvite = await inviteThroughUi(ownerAlpha.page, 'journey@t150.test', 'viewer');
  await inviteePage.goto(alphaInvite.acceptUrl);
  await expect(inviteePage.getByText('You’re already signed in')).toBeVisible();
  await inviteePage.getByRole('button', { name: 'Accept invitation' }).click();
  await expect(inviteePage).toHaveURL(/\/tenant\/overview$/);
  await expect(inviteePage.getByRole('link', { name: 'Team' })).toHaveCount(0);
  expect(
    await queryJson(
      "SELECT json_agg(json_build_object('tenant_id', tm.tenant_id, 'role', tm.role) ORDER BY tm.tenant_id) FROM tenant_memberships tm JOIN users u ON u.id = tm.user_id WHERE u.email = 'journey@t150.test'",
    ),
  ).toEqual([
    { tenant_id: ALPHA, role: 'viewer' },
    { tenant_id: BETA, role: 'agent' },
  ]);
  // single-use: the consumed link cannot activate another membership or be previewed again.
  await inviteePage.goto(alphaInvite.acceptUrl);
  await expect(inviteePage.getByRole('heading', { name: 'Invitation issue' })).toBeVisible();
  expect(
    await queryJson(
      "SELECT count(*)::int FROM tenant_memberships tm JOIN users u ON u.id = tm.user_id WHERE u.email = 'journey@t150.test' AND tm.tenant_id = '20000000-0000-0000-0000-000000000001'",
    ),
  ).toBe(1);

  // Hierarchy refusal happens through the manager's browser and creates no audit row.
  await manager.page.goto('/tenant/team');
  await expect(manager.page.getByRole('link', { name: 'Team' })).toBeVisible();
  const refusedStatus = await manager.page.evaluate(async (membershipId) => {
    const response = await fetch(`/api/v1/tenant/members/${membershipId}`, {
      method: 'PATCH',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ status: 'disabled' }),
    });
    return response.status;
  }, ADMIN_MEMBERSHIP);
  expect(refusedStatus).toBe(403);

  // Role change through the owner UI is enforced on the manager's next browser navigation.
  const managerRow = ownerAlpha.page.locator('tr').filter({ hasText: 'manager@t150.test' });
  await managerRow.getByRole('button', { name: 'Agent' }).click();
  await expect(managerRow).toContainText('Agent');
  await manager.page.goto('/tenant/team');
  await expect(manager.page).not.toHaveURL(/\/tenant\/team$/);
  await expect(manager.page.getByRole('link', { name: 'Team' })).toHaveCount(0);

  // Disable and re-enable are browser actions; Beta remains usable throughout.
  await member.page.goto('/tenant/conversations');
  await expect(member.page).toHaveURL(/\/tenant\/conversations$/);
  const memberRow = ownerAlpha.page.locator('tr').filter({ hasText: 'member@t150.test' });
  await memberRow.getByRole('button', { name: 'Disable' }).click();
  await expect(memberRow).toContainText('Disabled');
  expect(await member.page.evaluate(async () => (await fetch('/api/v1/tenant')).status)).toBe(403);
  const memberBeta = await devContext(browser, MEMBER, tenants.beta);
  await memberBeta.page.goto('/tenant/team');
  await expect(memberBeta.page.getByRole('heading', { name: 'Team' })).toBeVisible();
  await memberRow.getByRole('button', { name: 'Enable' }).click();
  await expect(memberRow).toContainText('Active');
  await member.page.goto('/tenant/conversations');
  await expect(member.page).toHaveURL(/\/tenant\/conversations$/);
  expect(await member.page.evaluate(async () => (await fetch('/api/v1/tenant')).status)).toBe(200);

  // both-tenant-audits, audit-timestamps, refused-op-audit and audit-exact-counts.
  const audits =
    (await queryJson(`SELECT json_agg(row_to_json(a) ORDER BY a.tenant_id, a.action, a.resource_id) FROM (
    SELECT tenant_id::text, action, actor_user_id::text, resource_id, details, created_at
    FROM audit_logs
    WHERE tenant_id IN ('${ALPHA}', '${BETA}') AND action LIKE 'member.%'
  ) a`)) as {
      tenant_id: string;
      action: string;
      actor_user_id: string;
      resource_id: string;
      details: Record<string, string>;
      created_at: string;
    }[];
  const counts = Object.fromEntries(
    [ALPHA, BETA].map((tenantId) => [
      tenantId,
      Object.fromEntries(
        [...new Set(audits.filter((audit) => audit.tenant_id === tenantId).map((a) => a.action))]
          .sort()
          .map((action) => [
            action,
            audits.filter((audit) => audit.tenant_id === tenantId && audit.action === action)
              .length,
          ]),
      ),
    ]),
  );
  expect(counts).toEqual({
    [ALPHA]: {
      'member.disabled': 1,
      'member.enabled': 1,
      'member.invitation_accepted': 1,
      'member.invitation_revoked': 1,
      'member.invited': 2,
      'member.role_changed': 1,
    },
    [BETA]: {
      'member.invitation_accepted': 1,
      'member.invited': 1,
    },
  });
  expect(audits).toHaveLength(9);
  for (const audit of audits) {
    expect(Date.parse(audit.created_at)).toBeGreaterThanOrEqual(workflowStartedAt);
    expect(Date.parse(audit.created_at)).toBeLessThanOrEqual(Date.now());
  }
  expect(audits.filter((audit) => audit.resource_id === ADMIN_MEMBERSHIP)).toEqual([]);
  // foreign-mutation-no-audit: the crafted cross-tenant target never becomes an audit resource.
  expect(audits.filter((audit) => audit.resource_id === FOREIGN_MEMBERSHIP)).toEqual([]);

  const journeyUserId = String(
    await queryJson("SELECT to_json(id::text) FROM users WHERE email = 'journey@t150.test'"),
  );
  for (const expected of [
    {
      tenant_id: ALPHA,
      action: 'member.invited',
      actor_user_id: OWNER,
      resource_id: revoked.invitationId,
      details: { email: 'revoked@t150.test', role: 'viewer' },
    },
    {
      tenant_id: ALPHA,
      action: 'member.invited',
      actor_user_id: OWNER,
      resource_id: alphaInvite.invitationId,
      details: { email: 'journey@t150.test', role: 'viewer' },
    },
    {
      tenant_id: BETA,
      action: 'member.invited',
      actor_user_id: OWNER,
      resource_id: betaInvite.invitationId,
      details: { email: 'journey@t150.test', role: 'agent' },
    },
    {
      tenant_id: ALPHA,
      action: 'member.invitation_accepted',
      actor_user_id: journeyUserId,
      resource_id: alphaInvite.invitationId,
      details: { email: 'journey@t150.test', role: 'viewer', user_id: journeyUserId },
    },
    {
      tenant_id: BETA,
      action: 'member.invitation_accepted',
      actor_user_id: journeyUserId,
      resource_id: betaInvite.invitationId,
      details: { email: 'journey@t150.test', role: 'agent', user_id: journeyUserId },
    },
  ]) {
    expect(audits).toContainEqual(expect.objectContaining(expected));
  }
  expect(audits.find((audit) => audit.action === 'member.role_changed')).toMatchObject({
    actor_user_id: OWNER,
    resource_id: MANAGER_MEMBERSHIP,
    details: { previous_role: 'manager', new_role: 'agent' },
  });
  expect(audits.find((audit) => audit.action === 'member.disabled')).toMatchObject({
    actor_user_id: OWNER,
    resource_id: MEMBER_MEMBERSHIP,
    details: { role: 'agent', previous_status: 'active', new_status: 'disabled' },
  });
  expect(audits.find((audit) => audit.action === 'member.enabled')).toMatchObject({
    actor_user_id: OWNER,
    resource_id: MEMBER_MEMBERSHIP,
    details: { role: 'agent', previous_status: 'disabled', new_status: 'active' },
  });
  expect(audits.find((audit) => audit.action === 'member.invitation_revoked')).toMatchObject({
    actor_user_id: OWNER,
    resource_id: revoked.invitationId,
    details: { email: 'revoked@t150.test', role: 'viewer' },
  });

  await Promise.all([
    ownerAlpha.context.close(),
    ownerBeta.context.close(),
    manager.context.close(),
    member.context.close(),
    memberBeta.context.close(),
    platform.context.close(),
    inviteeContext.close(),
  ]);
});
