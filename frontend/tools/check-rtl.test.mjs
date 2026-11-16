import assert from 'node:assert/strict';
import { test } from 'node:test';
import { check, readFiles } from './check-rtl.mjs';

const dash = 'apps/dashboard/src/app/x/x.component.ts';
const run = (source, path = dash) => check([{ path, source }]);

test('logical properties pass', () => {
  assert.deepEqual(
    run(
      'a { margin-inline-start: 4px; padding-inline: 0 8px; inset-inline-end: 0; text-align: start; ' +
        'border-inline-end: 1px solid; border-start-start-radius: 4px; float: inline-start; }',
    ),
    [],
  );
});

test('physical margin, padding and border properties are reported', () => {
  for (const css of [
    'margin-left: 4px',
    'margin-right: auto',
    'padding-left: 1em',
    'padding-right: 0',
    'border-left: 1px solid x',
    'border-right: 0',
    'border-left-color: x',
    'border-right-width: 2px',
    'scroll-margin-left: 4px',
    'scroll-padding-right: 4px',
  ]) {
    assert.equal(run(`a { ${css}; }`).length, 1, css);
  }
});

test('physical corners are reported, logical corners are not', () => {
  for (const corner of ['top-left', 'top-right', 'bottom-left', 'bottom-right']) {
    assert.equal(run(`a { border-${corner}-radius: 4px; }`).length, 1, corner);
  }
  assert.deepEqual(run('a { border-end-end-radius: 4px; border-radius: 4px; }'), []);
});

test('left and right as a position, and text-align, float and clear, are reported', () => {
  assert.equal(run('a { left: 0; }').length, 1);
  assert.equal(run('a { position: fixed; right: -4px; }').length, 1);
  assert.equal(run('a { text-align: left; }').length, 1);
  assert.equal(run('a { text-align: right; }').length, 1);
  assert.equal(run('a { float: right; }').length, 1);
  assert.equal(run('a { clear: left; }').length, 1);
  assert.deepEqual(run('a { text-align: center; float: none; top: 0; bottom: 0; }'), []);
});

test('words that are not properties are left alone: prose, comments and other names', () => {
  assert.deepEqual(run('/** The text runs from right to left. */\nconst a = 1;'), []);
  assert.deepEqual(run('// the request never left: no answer\nconst a = 1;'), []);
  assert.deepEqual(run('a { margin: 0; --left-pad: 1px; background: var(--app-left); }'), []);
});

test('a four-value shorthand with different left and right is reported', () => {
  assert.equal(run('a { margin: 1px 4px 1px 0; }').length, 1);
  assert.equal(run('a { padding: 0 8px 0 4px; }').length, 1);
  assert.equal(run('a { inset: auto 16px 16px auto; }').length, 1);
  assert.equal(run('a { inset: 0 0 0 auto; }').length, 1);
  assert.equal(run('a { border-radius: 4px 0 0 4px; }').length, 1);
});

test('symmetric shorthands pass', () => {
  assert.deepEqual(
    run(
      'a { margin: 0; padding: 4px 8px; margin: 1px 4px 2px; padding: 1px 2px 3px 2px; border-radius: 4px 4px 0 0; }',
    ),
    [],
  );
});

test('translateX is reported unless it is scaled by the direction sign or is zero', () => {
  assert.equal(run('a { transform: translateX(-100%); }').length, 1);
  assert.deepEqual(run('a { transform: translateX(calc(-100% * var(--app-dir-sign))); }'), []);
  assert.deepEqual(run('a { transform: translateX(0); }'), []);
});

test('left and right in a position, an origin or a gradient are reported', () => {
  assert.equal(run('a { background-position: right 4px center; }').length, 1);
  assert.equal(run('a { transform-origin: left; }').length, 1);
  assert.equal(run('a { background: linear-gradient(to right, a, b); }').length, 1);
  assert.deepEqual(run('a { background-position: center; transform-origin: center; }'), []);
});

test('a line marked rtl-lint-ignore with a reason passes; one without does not', () => {
  assert.deepEqual(
    run('a { right: 20px; /* rtl-lint-ignore: the corner is a tenant setting */ }'),
    [],
  );
  assert.equal(run('a { right: 20px; /* rtl-lint-ignore: */ }').length, 1);
  assert.equal(run('a { right: 20px; }').length, 1);
});

test('the report names the file, the line and the fix', () => {
  const [problem] = run('a {\n  margin-left: 4px;\n}');
  assert.match(problem, /x\.component\.ts:2: margin-left/);
  assert.match(problem, /margin-inline-start/);
});

test('templates and stylesheets are scanned too, and so is the widget', () => {
  assert.equal(
    run('<p style="margin-left: 4px">x</p>', 'apps/dashboard/src/app/x/x.component.html').length,
    1,
  );
  assert.equal(run('a { padding-right: 2px; }', 'apps/dashboard/src/styles.css').length, 1);
  assert.equal(
    run('a { text-align: right; }', 'apps/widget/src/components/x.component.ts').length,
    1,
  );
});

test('specs, fixtures, generated code and other apps are not scanned', () => {
  for (const path of [
    'apps/dashboard/src/app/x/x.spec.ts',
    'apps/dashboard/src/app/shared/fixtures/a.fixtures.ts',
    'apps/dashboard/src/app/core/api/generated/api.ts',
    'libs/ui/src/x.ts',
  ]) {
    assert.deepEqual(run('a { margin-left: 4px; }', path), [], path);
  }
});

test('the dashboard and the widget as they are pass', () => {
  assert.deepEqual(check(readFiles()), []);
});
