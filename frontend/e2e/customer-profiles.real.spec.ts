import { readFileSync } from 'node:fs';
import { BrowserContext, expect, Page, test } from '@playwright/test';
import pg from 'pg';

const AGENT = 'c0a00000-0000-0000-0000-000000000001';
const VIEWER = 'c0a00000-0000-0000-0000-000000000002';
const TENANT_A = '20000000-0000-0000-0000-000000000001';
const TENANT_B = '20000000-0000-0000-0000-000000000002';
const SARA_CUSTOMER_ID = 'c0c00000-0000-0000-0000-000000000001';
const FOREIGN_CUSTOMER_ID = 'c0c00000-0000-0000-0000-0000000000ff';

const tenantA = {
  id: TENANT_A,
  name: 'Customer Tenant',
  slug: 'customer-tenant',
  status: 'active' as const,
  plan: 'trial' as const,
};

const database = new pg.Pool({ connectionString: process.env['DATABASE_URL'] });

const REAL_BACKEND = process.env['CI_REAL_BACKEND'] === 'true';

if (REAL_BACKEND) {
  test.describe.serial('Customer Profiles — real backend', () => {
    let agentContext: BrowserContext;
    let agentPage: Page;
    let viewerContext: BrowserContext;
    let viewerPage: Page;

    test.beforeAll(async ({ browser }) => {
      // pagination seed: 30 customers in Tenant A
      // conversation-history seed: Sara Ali with 3 conversations
      // cross-tenant-notfound seed: foreign customer in Tenant B
      // duplicate-conflict seed: Sara Ali has 'sara@example.com' + '+201001234567' identifiers
      const seed = readFileSync('e2e/customer-profiles.seed.sql', 'utf8');
      await database.query(seed);

      agentContext = await browser.newContext({
        extraHTTPHeaders: { 'X-Dev-User-Id': AGENT, 'X-Tenant-ID': TENANT_A },
      });
      await agentContext.addInitScript(
        ({ id, activeTenant }) => {
          localStorage.setItem('app.devUserId', id);
          localStorage.setItem('app.tenant', JSON.stringify(activeTenant));
        },
        { id: AGENT, activeTenant: tenantA },
      );
      agentPage = await agentContext.newPage();

      viewerContext = await browser.newContext({
        extraHTTPHeaders: { 'X-Dev-User-Id': VIEWER, 'X-Tenant-ID': TENANT_A },
      });
      await viewerContext.addInitScript(
        ({ id, activeTenant }) => {
          localStorage.setItem('app.devUserId', id);
          localStorage.setItem('app.tenant', JSON.stringify(activeTenant));
        },
        { id: VIEWER, activeTenant: tenantA },
      );
      viewerPage = await viewerContext.newPage();
    });

    test.afterAll(async () => {
      await agentContext.close();
      await viewerContext.close();
      await database.end();
    });

    test('pagination: Agent can load more than one page of customers', async () => {
      // pagination
      const listResponse = agentPage.waitForResponse(
        (res) => res.url().includes('/api/v1/tenant/customers') && res.request().method() === 'GET',
      );
      await agentPage.goto('/tenant/customers');
      const response = await listResponse;
      expect(response.status()).toBe(200);
      await expect(agentPage.getByRole('heading', { name: 'Customers' })).toBeVisible();

      await expect(agentPage.getByText('Customer 01')).toBeVisible();
      await expect(agentPage.getByText('Customer 25')).toBeVisible();

      const loadMore = agentPage.getByRole('button', { name: 'Load more' });
      await expect(loadMore).toBeVisible();

      const nextPageResponse = agentPage.waitForResponse(
        (res) => res.url().includes('/api/v1/tenant/customers') && res.request().method() === 'GET',
      );
      await loadMore.click();
      await nextPageResponse;

      await expect(agentPage.getByText('Customer 26')).toBeVisible();
    });

    test('conversation-history: Agent sees conversation history on a customer profile', async () => {
      // conversation-history
      const detailResponse = agentPage.waitForResponse(
        (res) =>
          res.url().includes(`/api/v1/tenant/customers/${SARA_CUSTOMER_ID}`) &&
          res.request().method() === 'GET',
      );
      await agentPage.goto(`/tenant/customers/${SARA_CUSTOMER_ID}`);
      await detailResponse;
      await expect(agentPage.getByText('Sara Ali')).toBeVisible();

      const convResponse = agentPage.waitForResponse((res) =>
        res.url().includes(`/api/v1/tenant/customers/${SARA_CUSTOMER_ID}/conversations`),
      );
      await convResponse;

      await expect(agentPage.getByText('Open').first()).toBeVisible();
      await expect(agentPage.getByText('Closed')).toBeVisible();
    });

    test('update-customer: Agent can edit a customer display name', async () => {
      // update-customer
      const originalName = 'Sara Ali';
      const updatedName = 'Sara Updated';

      await agentPage.goto(`/tenant/customers/${SARA_CUSTOMER_ID}`);
      await expect(agentPage.getByText(originalName)).toBeVisible();

      const editButton = agentPage.getByRole('button', { name: 'Edit' });
      await expect(editButton).toBeVisible();
      await editButton.click();

      const displayNameInput = agentPage.getByLabel('Display name');
      await displayNameInput.clear();
      await displayNameInput.fill(updatedName);

      const patchResponse = agentPage.waitForResponse(
        (res) =>
          res.url().includes(`/api/v1/tenant/customers/${SARA_CUSTOMER_ID}`) &&
          res.request().method() === 'PATCH' &&
          res.status() === 200,
      );
      await agentPage.getByRole('button', { name: 'Save' }).click();
      const patch = await patchResponse;
      expect((await patch.json()).data.display_name).toBe(updatedName);

      await expect(agentPage.getByText(updatedName)).toBeVisible();
    });

    test('duplicate-conflict: Creating a customer with existing identifier returns 409', async () => {
      // duplicate-conflict
      await agentPage.goto('/tenant/customers');
      await expect(agentPage.getByRole('button', { name: 'New customer' })).toBeVisible();
      await agentPage.getByRole('button', { name: 'New customer' }).click();

      await agentPage.getByLabel('Display name').fill('Conflict Person');
      await agentPage.getByLabel('Email').fill('conflict@test.com');

      await agentPage.getByRole('button', { name: 'Add identifier' }).click();
      await agentPage.getByLabel('Channel').fill('whatsapp');
      await agentPage.getByLabel('Identifier value').fill('+201001234567');

      const createResponse = agentPage.waitForResponse(
        (res) =>
          res.url().includes('/api/v1/tenant/customers') && res.request().method() === 'POST',
      );
      await agentPage.getByRole('button', { name: 'Create' }).click();
      const response = await createResponse;
      expect(response.status()).toBe(409);

      await expect(agentPage.getByText(/already|held|conflict/i).first()).toBeVisible();
    });

    test('viewer-refusal: Viewer cannot create or edit customers', async () => {
      // viewer-refusal: buttons hidden and server refuses mutations
      await viewerPage.goto('/tenant/customers');
      await expect(viewerPage.getByRole('button', { name: 'New customer' })).toHaveCount(0);

      const postStatus = await viewerPage.evaluate(async () => {
        const response = await fetch('/api/v1/tenant/customers', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            display_name: 'Viewer Attempt',
            email: 'viewer-attempt@test.com',
          }),
        });
        return response.status;
      });
      expect(postStatus).toBe(403);

      await viewerPage.goto(`/tenant/customers/${SARA_CUSTOMER_ID}`);
      await expect(viewerPage.getByRole('button', { name: 'Edit' })).toHaveCount(0);

      const patchStatus = await viewerPage.evaluate(async (customerId) => {
        const response = await fetch(`/api/v1/tenant/customers/${customerId}`, {
          method: 'PATCH',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ display_name: 'Viewer PATCH' }),
        });
        return response.status;
      }, SARA_CUSTOMER_ID);
      expect(patchStatus).toBe(403);
    });

    test('cross-tenant-notfound: Accessing another tenant customer returns 404', async () => {
      // cross-tenant-notfound
      const detailResponse = agentPage.waitForResponse(
        (res) =>
          res.url().includes(`/api/v1/tenant/customers/${FOREIGN_CUSTOMER_ID}`) &&
          res.request().method() === 'GET',
      );
      await agentPage.goto(`/tenant/customers/${FOREIGN_CUSTOMER_ID}`);
      const response = await detailResponse;
      expect(response.status()).toBe(404);

      await expect(agentPage.getByText(/not.?found|404/i)).toBeVisible();
    });

    test('search: Agent can filter customers by name fragment', async () => {
      // search: seeded name fragment
      const listResponse = agentPage.waitForResponse(
        (res) => res.url().includes('/api/v1/tenant/customers') && res.request().method() === 'GET',
      );
      await agentPage.goto('/tenant/customers');
      await listResponse;

      const searchInput = agentPage.getByPlaceholder(/search/i);
      await expect(searchInput).toBeVisible();

      const searchResponse = agentPage.waitForResponse(
        (res) =>
          res.url().includes('/api/v1/tenant/customers') &&
          res.url().includes('q=') &&
          res.request().method() === 'GET',
      );
      await searchInput.fill('Sara');
      await searchResponse;

      await expect(agentPage.getByText('Sara Ali')).toBeVisible();

      const emptyResponse = agentPage.waitForResponse(
        (res) =>
          res.url().includes('/api/v1/tenant/customers') &&
          res.url().includes('q=') &&
          res.request().method() === 'GET',
      );
      await searchInput.fill('zzz_nonexistent_zzz');
      await emptyResponse;

      await expect(agentPage.getByText(/no.*result|empty/i)).toBeVisible();
    });

    test('create-customer: Agent can create a new customer that appears in the list', async () => {
      // successful-create
      await agentPage.goto('/tenant/customers');
      await expect(agentPage.getByRole('button', { name: 'New customer' })).toBeVisible();
      await agentPage.getByRole('button', { name: 'New customer' }).click();

      await agentPage.getByLabel('Display name').fill('Test Created');
      await agentPage.getByLabel('Email').fill('test-created@example.com');

      const createResponse = agentPage.waitForResponse(
        (res) =>
          res.url().includes('/api/v1/tenant/customers') && res.request().method() === 'POST',
      );
      await agentPage.getByRole('button', { name: /create|save/i }).click();
      const response = await createResponse;
      expect(response.status()).toBe(201);

      await expect(agentPage.getByText('Test Created')).toBeVisible();
    });
  });
}
