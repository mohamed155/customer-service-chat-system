import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { check, EXEMPT, readFiles } from './check-tenant-scope.mjs';

const app = 'apps/dashboard/src/app';
const exemptFiles = () =>
  Object.keys(EXEMPT).map((path) => ({ path, source: "signalStore(withState({ tab: 'a' }))" }));

test('a workspace store without withTenantScope is reported', () => {
  const problems = check([
    ...exemptFiles(),
    { path: `${app}/features/tenant/x/x.store.ts`, source: 'signalStore(withState({}))' },
  ]);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /x\.store\.ts.*withTenantScope/);
});

test('a store that uses withTenantScope passes', () => {
  const problems = check([
    ...exemptFiles(),
    {
      path: `${app}/features/tenant/x/x.store.ts`,
      source: 'signalStore(withState({}), withTenantScope({ initialState }))',
    },
  ]);
  assert.deepEqual(problems, []);
});

test('a root-provided store anywhere is held to the same rule', () => {
  const problems = check([
    ...exemptFiles(),
    {
      path: `${app}/core/notifications/n.store.ts`,
      source: "signalStore({ providedIn: 'root' }, withState({}))",
    },
  ]);
  assert.equal(problems.length, 1);
});

test('a root-provided store is found whichever quotes it uses', () => {
  const problems = check([
    ...exemptFiles(),
    {
      path: `${app}/core/x/x.store.ts`,
      source: 'signalStore({ providedIn: "root" }, withState({}))',
    },
  ]);
  assert.equal(problems.length, 1);
});

test('a component-provided store outside features/tenant is not covered', () => {
  const problems = check([
    ...exemptFiles(),
    { path: `${app}/layout/app-shell/layout.store.ts`, source: 'signalStore(withState({}))' },
  ]);
  assert.deepEqual(problems, []);
});

test('spec files are ignored', () => {
  const problems = check([
    ...exemptFiles(),
    { path: `${app}/features/tenant/x/x.store.spec.ts`, source: 'describe()' },
  ]);
  assert.deepEqual(problems, []);
});

test('an exempt entry for a store that is gone, or that now uses the scope, is reported', () => {
  assert.equal(check([]).length, Object.keys(EXEMPT).length);
  const [first] = Object.keys(EXEMPT);
  const problems = check([
    ...exemptFiles().slice(1),
    { path: first, source: 'withTenantScope({ initialState })' },
  ]);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /exempt/);
});

test('reads store files from disk with paths relative to the frontend root', () => {
  const base = mkdtempSync(join(tmpdir(), 'scope-'));
  mkdirSync(join(base, app, 'features/tenant/y'), { recursive: true });
  writeFileSync(join(base, app, 'features/tenant/y/y.store.ts'), 'x');
  const files = readFiles(base);
  assert.deepEqual(
    files.map((f) => f.path),
    [`${app}/features/tenant/y/y.store.ts`],
  );
});

test('the real dashboard passes', () => {
  assert.deepEqual(check(readFiles()), []);
});
