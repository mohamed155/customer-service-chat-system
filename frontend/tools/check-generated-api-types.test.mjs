import assert from 'node:assert/strict';
import { test } from 'node:test';
import { check as checkWith, EXEMPT, readFiles } from './check-generated-api-types.mjs';

/** The check with no exempt files, unless a test passes some. */
const check = (files, exempt = {}) => checkWith(files, exempt);

const core = (name, source) => ({ path: `apps/dashboard/src/app/core/${name}`, source });

test('the dashboard passes as it is', () => {
  assert.deepEqual(checkWith(readFiles()), []);
});

test('flags a hand-typed call in core/ and in layout/, with the file and line', () => {
  const problems = check([
    core('tenant/x.service.ts', "const a = 1;\nthis.api.get<MeResponse>('/me');\n"),
    {
      path: 'apps/dashboard/src/app/layout/topbar/y.component.ts',
      source: "this.api.put<Availability>('x', {});",
    },
  ]);

  assert.equal(problems.length, 2);
  assert.match(
    problems[0],
    /core\/tenant\/x\.service\.ts:2: api\.get<MeResponse> is typed by hand/,
  );
  assert.match(problems[1], /layout\/topbar\/y\.component\.ts:1: api\.put<Availability>/);
});

test('flags every method, `http` as well as `api`, and a call split over lines', () => {
  const source = [
    "this.api.post<Foo>('/a', {});",
    "this.api.patch<Foo>('/a', {});",
    "this.api.delete<Foo>('/a');",
    "this.http.get<Foo>('/a');",
    "this.api\n  .get<Foo>('/a');",
  ].join('\n');

  assert.equal(check([core('a.ts', source)]).length, 5);
});

test('accepts void, unknown, the generated types and a union of them', () => {
  const source = [
    "this.api.post<void>('/a', {});",
    "this.api.post<unknown>('/a', {});",
    "this.api.get<Schemas['MeResponse']>('/me');",
    "this.api.get<components['schemas']['MeResponse']>('/me');",
    "this.api.post<Schemas['MeResponse'] | Schemas['MfaRequiredResponse']>('/a', {});",
  ].join('\n');

  assert.deepEqual(check([core('a.ts', source)]), []);
});

test('accepts a name declared in the same file as a generated type, not one from elsewhere', () => {
  const declared =
    "type Status = Schemas['MfaStatusResponse'];\nthis.api.get<Status>('/auth/mfa/status');";
  const imported = "import { Status } from './x';\nthis.api.get<Status>('/auth/mfa/status');";

  assert.deepEqual(check([core('declared.ts', declared)]), []);
  assert.equal(check([core('imported.ts', imported)]).length, 1);
});

test('names the hand-typed member of a union', () => {
  const [problem] = check([core('a.ts', "this.api.get<Schemas['MeResponse'] | Mine>('/me');")]);

  assert.match(problem, /typed by hand \(Mine\)/);
});

test('ignores specs, files outside core/ and layout/, and generics in other code', () => {
  assert.deepEqual(
    check([
      core('a.spec.ts', "api.get<Foo>('/a');"),
      { path: 'apps/dashboard/src/app/features/x/x.ts', source: "this.api.get<Foo>('/a');" },
      core('b.ts', 'const m = new Map<string, number>(); items.get<Foo>(1);'),
    ]),
    [],
  );
});

test('a file in the exempt list may keep hand-typed calls; an entry no longer needed is flagged', () => {
  const path = 'apps/dashboard/src/app/core/api/old.ts';
  const exempt = { [path]: 'reason' };

  assert.deepEqual(check([{ path, source: "this.api.get<Foo>('/a');" }], exempt), []);
  const unneeded = check([{ path, source: "this.api.get<Schemas['X']>('/a');" }], exempt);
  assert.ok(unneeded.some((p) => p.includes(path) && p.includes('no hand-typed call any more')));
  const missing = check([core('other.ts', '')], exempt);
  assert.ok(missing.some((p) => p.includes(path) && p.includes('no such file exists')));
});

test('every exempt file has a reason, and the customers API one names its issue', () => {
  for (const [path, reason] of Object.entries(EXEMPT)) {
    assert.ok(reason.length > 20, `${path} needs a reason`);
  }
  const customers = Object.entries(EXEMPT).find(([path]) =>
    path.endsWith('customers-api.service.ts'),
  );
  assert.match(customers[1], /MOH-145/);
});
