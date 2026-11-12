import assert from 'node:assert/strict';
import { test } from 'node:test';
import { drops, GROUPS, raised, summarize, TOLERANCE } from './check-coverage.mjs';

const entry = (covered, total) => ({
  lines: { covered, total },
  statements: { covered, total },
  functions: { covered, total },
  branches: { covered, total },
});

const summary = {
  total: entry(1, 1), // ignored: the script adds the files up itself
  '/w/apps/dashboard/src/app/core/a.ts': entry(8, 10),
  '/w/apps/dashboard/src/app/core/b.ts': entry(2, 10),
  '/w/apps/dashboard/src/app/features/x/x.store.ts': entry(9, 10),
  '/w/apps/dashboard/src/app/features/x/x.component.ts': entry(0, 10),
};

test('adds files up for the project and for each group (lines only for groups)', () => {
  const result = summarize(summary, GROUPS.dashboard);
  assert.equal(result.total.lines, (19 / 40) * 100);
  assert.deepEqual(result.groups.core, { lines: 50 });
  assert.deepEqual(result.groups.stores, { lines: 90 });
});

test('an empty group counts as fully covered, not as a failure', () => {
  const result = summarize(
    { '/w/apps/dashboard/src/app/features/y.ts': entry(1, 2) },
    GROUPS.dashboard,
  );
  assert.deepEqual(result.groups.core, { lines: 100 });
});

test('a project with no groups has only a total', () => {
  const result = summarize({ '/w/apps/widget/src/main.ts': entry(3, 4) }, GROUPS.widget);
  assert.equal(result.total.lines, 75);
  assert.deepEqual(result.groups, {});
});

test('reports a number below its baseline, in total or in a group', () => {
  const baseline = { total: { lines: 60 }, groups: { core: { lines: 50 } } };
  const current = { total: { lines: 59 }, groups: { core: { lines: 49 } } };
  assert.deepEqual(
    drops(baseline, current).map((d) => `${d.scope}.${d.metric}`),
    ['total.lines', 'core.lines'],
  );
});

test('a number at or above its baseline, or within the tolerance, is not a drop', () => {
  const baseline = { total: { lines: 60 }, groups: { core: { lines: 50 } } };
  assert.deepEqual(drops(baseline, { total: { lines: 60 }, groups: { core: { lines: 80 } } }), []);
  assert.deepEqual(
    drops(baseline, { total: { lines: 60 - TOLERANCE / 2 }, groups: { core: { lines: 50 } } }),
    [],
  );
});

test('a group that disappeared counts as a drop', () => {
  const baseline = { total: {}, groups: { stores: { lines: 80 } } };
  assert.equal(drops(baseline, { total: {}, groups: {} }).length, 1);
});

test('raising the baseline never lowers a number and rounds the new ones down', () => {
  const baseline = { total: { lines: 70, branches: 50 }, groups: { core: { lines: 90 } } };
  const current = { total: { lines: 65.55, branches: 61.99 }, groups: { core: { lines: 72 } } };
  assert.deepEqual(raised(baseline, current), {
    total: { lines: 70, branches: 61.9 },
    groups: { core: { lines: 90 } },
  });
});

test('a first baseline takes the current numbers, rounded down', () => {
  const current = { total: { lines: 79.99 }, groups: { stores: { lines: 85.04 } } };
  assert.deepEqual(raised({}, current), {
    total: { lines: 79.9 },
    groups: { stores: { lines: 85 } },
  });
});
