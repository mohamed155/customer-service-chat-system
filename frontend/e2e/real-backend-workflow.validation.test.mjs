import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const t150Spec = readFileSync(
  new URL('./tenant-team-management.real.spec.ts', import.meta.url),
  'utf8',
);

const customerProfilesSpec = readFileSync(
  new URL('./customer-profiles.real.spec.ts', import.meta.url),
  'utf8',
);

test('T150 drives security-sensitive behavior through browser pages', () => {
  assert.doesNotMatch(t150Spec, /\brequest\.newContext\b/);
  for (const evidence of [
    "getByRole('button', { name: 'Switch tenant' })",
    "getByRole('button', { name: 'Invite' })",
    "getByRole('button', { name: 'Revoke' })",
    "getByRole('button', { name: 'Accept & join' })",
    "getByRole('button', { name: 'Accept invitation' })",
    'context().cookies()',
    'single-use',
    'foreign-membership',
    'refused-op-audit',
    'audit-exact-counts',
    'both-tenant-audits',
    'audit-timestamps',
    'foreign-mutation-no-audit',
  ]) {
    assert.ok(t150Spec.includes(evidence), `missing browser-workflow evidence: ${evidence}`);
  }
});

test('T150 database setup and assertions use a portable client', () => {
  assert.doesNotMatch(t150Spec, /node:child_process/);
  assert.doesNotMatch(t150Spec, /\b(?:psql|podman)\b/);
  assert.doesNotMatch(t150Spec, /E2E_POSTGRES_CONTAINER/);
  assert.match(t150Spec, /from ['"]pg['"]/);
  assert.match(t150Spec, /process\.env\[['"]DATABASE_URL['"]\]/);
});

test('T155 customer profiles spec covers mandatory scenarios', () => {
  for (const evidence of [
    'pagination',
    'search',
    'conversation-history',
    'successful-create',
    'update-customer',
    'duplicate-conflict',
    'viewer-refusal',
    'cross-tenant-notfound',
  ]) {
    assert.ok(
      customerProfilesSpec.includes(evidence),
      `missing customer-profiles evidence: ${evidence}`,
    );
  }
});

test('T155 customer profiles uses portable DB client and env gating', () => {
  assert.doesNotMatch(customerProfilesSpec, /node:child_process/);
  assert.doesNotMatch(customerProfilesSpec, /\b(?:psql|podman)\b/);
  assert.doesNotMatch(customerProfilesSpec, /E2E_POSTGRES_CONTAINER/);
  assert.match(customerProfilesSpec, /from ['"]pg['"]/);
  assert.match(customerProfilesSpec, /process\.env\[['"]DATABASE_URL['"]\]/);
  assert.match(customerProfilesSpec, /CI_REAL_BACKEND/);
  assert.doesNotMatch(customerProfilesSpec, /test\.skip\(!REAL_BACKEND/);
  assert.match(customerProfilesSpec, /test\.describe\.serial/);
  assert.doesNotMatch(customerProfilesSpec, /\bdevContext\b/);
  assert.doesNotMatch(customerProfilesSpec, /\bisVisible\(\)/);
});
