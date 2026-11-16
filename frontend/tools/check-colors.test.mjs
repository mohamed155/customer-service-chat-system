import assert from 'node:assert/strict';
import { test } from 'node:test';
import { check, EXEMPT, readFiles, readTokens } from './check-colors.mjs';

const src = 'apps/dashboard/src';
const ds = `${src}/app/design-system`;

const design = [
  {
    path: `${ds}/tokens/tokens.css`,
    source: ':root { --app-space-1: 4px; --app-chart-1: #0284c7; }',
  },
  {
    path: `${ds}/theme/themes.css`,
    source: `:root[data-theme='light'] { --app-panel: #ffffff; --app-red: #e11d48; }
             :root[data-theme='dark'] { --app-panel: #13161d; --app-red: #fb7185; }`,
  },
];
const exemptFiles = () => Object.keys(EXEMPT).map((path) => ({ path, source: 'color: #fff;' }));
const component = (source, path = `${src}/app/features/x/x.component.ts`) => [
  ...exemptFiles(),
  { path, source },
];
const run = (source, path) => check(component(source, path), design);

test('a token alone passes', () => {
  assert.deepEqual(
    run('styles: [`a { color: var(--app-red); background: var(--app-panel); }`]'),
    [],
  );
});

test('a hex color is reported, in 3, 4, 6 and 8 digits', () => {
  for (const hex of ['#fff', '#ffff', '#d92d20', '#d92d2080']) {
    const problems = run(`a { color: ${hex}; }`);
    assert.equal(problems.length, 1, hex);
    assert.match(problems[0], /hard-coded color/);
  }
});

test('rgb, rgba, hsl, hsla and the newer color functions are reported', () => {
  for (const fn of [
    'rgb(0 0 0)',
    'rgba(0, 0, 0, 0.4)',
    'hsl(0 0% 0%)',
    'hsla(0,0%,0%,1)',
    'oklch(0.5 0.2 30)',
  ]) {
    assert.equal(run(`a { background: ${fn}; }`).length, 1, fn);
  }
});

test('a literal beside a token (a fallback) is reported', () => {
  const problems = run('a { color: var(--app-red, #d92d20); }');
  assert.equal(problems.length, 1);
  assert.match(problems[0], /#d92d20/);
});

test('a color name as a value is reported, even as a fallback', () => {
  assert.equal(run('a { color: white; }').length, 1);
  assert.equal(run('a { border: 1px solid black; }').length, 1);
  assert.equal(run('a { color: var(--app-red, white); }').length, 1);
  assert.equal(run('<svg stroke="white"></svg>', `${src}/app/x/x.component.html`).length, 1);
});

test('words that are not colors are left alone', () => {
  assert.deepEqual(
    run('a { color: currentColor; border-color: transparent; background: none; outline: 0; }'),
    [],
  );
  assert.deepEqual(run("const label = 'green light'; // a { color: var(--app-red) }"), []);
});

test('a token that the design system does not define is reported', () => {
  const problems = run('a { color: var(--app-danger); }');
  assert.equal(problems.length, 1);
  assert.match(problems[0], /--app-danger is not defined/);
});

test('a token defined only in one theme is reported as a design-system problem', () => {
  const problems = check(component('a {}'), [
    ...design,
    {
      path: `${ds}/theme/themes.css`,
      source: `:root[data-theme='light'] { --app-only-light: #fff; } :root[data-theme='dark'] { --app-x: #000; }`,
    },
  ]);
  assert.ok(problems.some((p) => /--app-only-light.*light theme but not the dark/.test(p)));
  assert.ok(problems.some((p) => /--app-x.*dark theme but not the light/.test(p)));
});

test('a token from tokens.css (not themed) counts as defined', () => {
  assert.deepEqual(run('a { padding: var(--app-space-1); stroke: var(--app-chart-1); }'), []);
});

test('a custom property a component sets for itself counts as defined', () => {
  assert.deepEqual(run('a { --app-local: 1px; margin: var(--app-local); }'), []);
});

test('a custom property one component sets does not define it for another', () => {
  const problems = check(
    [
      ...exemptFiles(),
      {
        path: `${src}/app/a/a.component.ts`,
        source: 'a { --app-local: 1px; margin: var(--app-local); }',
      },
      { path: `${src}/app/b/b.component.ts`, source: 'b { margin: var(--app-local); }' },
    ],
    design,
  );
  assert.equal(problems.length, 1);
  assert.match(problems[0], /b\.component\.ts:1: --app-local is not defined/);
});

test('a declaration split over lines is still checked, and reported on the value line', () => {
  const problems = run('a {\n  color:\n    white;\n  background:\n    var(--app-red);\n}');
  assert.equal(problems.length, 1);
  assert.match(problems[0], /x\.component\.ts:3: hard-coded color white/);
});

test('a color name inside parentheses that run over lines is found on its own line', () => {
  const problems = run('a {\n  color: var(--app-red,\n    white);\n}');
  assert.equal(problems.length, 1);
  assert.match(problems[0], /x\.component\.ts:3: hard-coded color white/);
  assert.deepEqual(
    run('a {\n  color: var(--app-red,\n    white /* color-lint-ignore: x */);\n}'),
    [],
  );
});

test('a color name in a marked multi-line declaration is skipped on the marked line', () => {
  assert.deepEqual(run('a {\n  color:\n    white; /* color-lint-ignore: a value */\n}'), []);
});

test('a link target or an SVG reference that looks like a hex color is not a color', () => {
  const html = (source) => run(source, `${src}/app/x/x.component.html`);
  assert.deepEqual(html('<a href="#abc">Section</a>'), []);
  assert.deepEqual(html('<a [routerLink]=\'x\' fragment="#add">Go</a>'), []);
  assert.deepEqual(html('<rect fill="url(#fade)"></rect>'), []);
  assert.deepEqual(run('a { fill: url(#bad); }'), []);
  // while a real color next to it is still found
  assert.equal(html('<a href="#abc" style="color: #abc">Section</a>').length, 1);
});

test('the design system itself may hold literals', () => {
  assert.deepEqual(check(component('a {}'), design), []);
});

test('specs, fixtures and generated code are not scanned', () => {
  for (const path of [
    `${src}/app/x/x.spec.ts`,
    `${src}/app/shared/fixtures/a.fixtures.ts`,
    `${src}/app/core/api/generated/api.ts`,
  ]) {
    assert.deepEqual(run('a { color: #fff; }', path), [], path);
  }
});

test('a line marked color-lint-ignore with a reason passes; one without does not', () => {
  assert.deepEqual(run("const c = '#0066FF'; // color-lint-ignore: a tenant's stored setting"), []);
  assert.equal(run("const c = '#0066FF'; // color-lint-ignore:").length, 1);
  assert.equal(run("const c = '#0066FF';").length, 1);
});

test('an exempt file is skipped, and one that no longer exists is reported', () => {
  assert.deepEqual(check(exemptFiles(), design), []);
  const problems = check([], design);
  assert.equal(problems.length, Object.keys(EXEMPT).length);
  assert.match(problems[0], /no such file exists/);
});

test('an HTML entity or an id is not a color', () => {
  assert.deepEqual(run('<p>Order &#123; and #hello</p>', `${src}/app/x/x.component.html`), []);
});

test('readTokens finds themed and fixed tokens', () => {
  const { fixed, themed, problems } = readTokens(design);
  assert.ok(fixed.has('--app-space-1'));
  assert.ok(themed.has('--app-panel'));
  assert.deepEqual(problems, []);
});

test('the dashboard as it is passes', () => {
  const files = readFiles();
  const problems = check(
    files,
    files.filter((f) => f.path.startsWith(`${ds}/`)),
  );
  assert.deepEqual(problems, []);
});
