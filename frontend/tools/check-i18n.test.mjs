import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import {
  checkCatalogs,
  codeKeys,
  compare,
  flatten,
  inlineTemplates,
  scan,
  scanTemplate,
  templateKeys,
} from './check-i18n.mjs';

test('flags text nodes and literal text attributes', () => {
  const hits = scanTemplate(
    '<p>Hello there</p><input placeholder="Your name" aria-label="Name"><img alt="Logo">',
  );
  assert.deepEqual(
    hits.map((h) => h.text),
    ['Hello there', 'placeholder="Your name"', 'aria-label="Name"', 'alt="Logo"'],
  );
});

test('ignores bindings, catalog lookups, symbols and attributes that are not text', () => {
  const hits = scanTemplate(
    `<p>{{ i18n.t('a.b') }}</p><span>—</span><b>1/2</b><input [placeholder]="i18n.t('x')" class="wide" type="text">
     <button [attr.aria-label]="label()">★</button>`,
  );
  assert.deepEqual(hits, []);
});

test('sees text inside control-flow blocks and nested elements', () => {
  const hits = scanTemplate(
    '@if (a) { <div><span>Inside</span></div> } @else { <i>Other text</i> } @for (x of xs; track x) { <li>Item</li> }',
  );
  assert.deepEqual(hits.map((h) => h.text).sort(), ['Inside', 'Item', 'Other text']);
});

test('reads inline templates of components and skips other template properties', () => {
  const source =
    "@Component({ selector: 'x', template: `<p>Visible</p>` })\nclass A {}\nconst other = { notTemplate: 'Text' };";
  const templates = inlineTemplates(source, 'a.ts');
  assert.equal(templates.length, 1);
  assert.deepEqual(
    scanTemplate(templates[0].text).map((h) => h.text),
    ['Visible'],
  );
});

test('scans a tree: html files, inline templates, and skips specs and node_modules', () => {
  const base = mkdtempSync(join(tmpdir(), 'i18n-'));
  mkdirSync(join(base, 'apps/a/src'), { recursive: true });
  mkdirSync(join(base, 'apps/a/node_modules/x'), { recursive: true });
  writeFileSync(join(base, 'apps/a/src/one.html'), '<p>Plain text</p>');
  writeFileSync(
    join(base, 'apps/a/src/two.ts'),
    '@Component({ template: `<b>Inline text</b>` }) class T {}',
  );
  writeFileSync(
    join(base, 'apps/a/src/two.spec.ts'),
    '@Component({ template: `<b>Spec text</b>` }) class T {}',
  );
  writeFileSync(join(base, 'apps/a/src/clean.html'), "<p>{{ i18n.t('k') }}</p>");
  writeFileSync(join(base, 'apps/a/node_modules/x/bad.html'), '<p>Ignored text</p>');
  writeFileSync(join(base, 'apps/a/src/index.html'), '<title>Shell title</title>');
  const found = scan(base);
  assert.deepEqual(Object.keys(found).sort(), ['apps/a/src/one.html', 'apps/a/src/two.ts']);
});

test('is a ratchet: no new text, and a file that improved must lower its baseline', () => {
  const found = {
    'a.html': [
      { line: 1, text: 'x' },
      { line: 2, text: 'y' },
    ],
    'new.html': [{ line: 1, text: 'z' }],
    'better.html': [{ line: 1, text: 'w' }],
  };
  const baseline = { 'a.html': 2, 'better.html': 3, 'gone.html': 1 };
  const { over, under } = compare(found, baseline);
  assert.deepEqual(
    over.map((o) => o.file),
    ['new.html'],
  );
  assert.deepEqual(under.map((u) => u.file).sort(), ['better.html', 'gone.html']);
  assert.deepEqual(compare({ 'a.html': found['a.html'] }, { 'a.html': 2 }), {
    over: [],
    under: [],
  });
});

test('reads the keys a template looks up, through ternaries and fallbacks, not other strings', () => {
  const keys = templateKeys(
    `<p>{{ 'a.b' | transloco }}</p>
     <input [placeholder]="'a.c' | transloco" [attr.aria-label]="(on() ? 'a.d' : 'a.e') | transloco: { n: 1 }">
     <span>{{ (state() === 'ready' ? 'a.f' : 'a.g') | transloco }}</span>
     <i>{{ title() ?? 'a.h' | transloco }}</i>
     @if (ok()) { <b>{{ 'a.i' | transloco }}</b> }
     <u [title]="'not.a.key'">{{ 'plain text' }}</u>`,
  );
  assert.deepEqual(keys.map((k) => k.key).sort(), [
    'a.b',
    'a.c',
    'a.d',
    'a.e',
    'a.f',
    'a.g',
    'a.h',
    'a.i',
  ]);
});

test('reads the keys code marks, with a literal only', () => {
  const keys = codeKeys(
    "const a = marker('x.one'); this.t.translate('x.two', {}); translate(name); marker(`x.three`); other('x.four');",
  );
  assert.deepEqual(
    keys.map((k) => k.key),
    ['x.one', 'x.two', 'x.three'],
  );
});

test('flattens a catalog to dotted keys', () => {
  assert.deepEqual(flatten({ a: { b: 'x', c: { d: 'y' } }, e: 'z' }), {
    'a.b': 'x',
    'a.c.d': 'y',
    e: 'z',
  });
});

function project(files) {
  const base = mkdtempSync(join(tmpdir(), 'i18n-catalogs-'));
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(join(base, path, '..'), { recursive: true });
    writeFileSync(join(base, path), content);
  }
  return base;
}

const EN = 'apps/dashboard/public/i18n/shared/en.json';
const TPL = 'apps/dashboard/src/app/x.component.html';

test('catalogs: a key a template uses must be in English, and every English key must be used', () => {
  const base = project({
    [EN]: JSON.stringify({ a: { used: 'Used', dead: 'Dead' } }),
    [TPL]: "<p>{{ 'a.used' | transloco }}</p><p>{{ 'a.typo' | transloco }}</p>",
  });
  const problems = checkCatalogs(base);
  assert.equal(problems.length, 2);
  assert.match(
    problems.join('\n'),
    /x\.component\.html:1: the key "a\.typo" is not in the English catalog/,
  );
  assert.match(problems.join('\n'), /"a\.dead" is not used/);
});

test('catalogs: a clean project has no problems; marker() and translate() count as uses', () => {
  const base = project({
    [EN]: JSON.stringify({ a: 'A', b: 'B', c: 'C {n}' }),
    [TPL]: "<p>{{ 'a' | transloco }}</p>",
    'apps/dashboard/src/app/y.ts':
      "const k = marker('b'); const t = this.i18n.translate('c', { n: 1 });",
  });
  assert.deepEqual(checkCatalogs(base), []);
});

test('catalogs: another language may only translate English keys, with the same values', () => {
  const base = project({
    [EN]: JSON.stringify({ a: 'Hello {name}', b: 'B' }),
    [TPL]: "<p>{{ 'a' | transloco }}{{ 'b' | transloco }}</p>",
    'apps/dashboard/public/i18n/shared/de.json': JSON.stringify({
      a: 'Hallo {nom}',
      b: 'B',
      extra: 'Zusatz',
    }),
  });
  const problems = checkCatalogs(base).join('\n');
  assert.match(problems, /shared\/de: "extra" is not in the English catalog/);
  assert.match(problems, /shared\/de: "a" must use the same \{values\} as English/);
  assert.doesNotMatch(problems, /"b"/);
});

test('catalogs: a feature scope is a folder, and its keys are written scope.key', () => {
  const base = project({
    [EN]: JSON.stringify({ a: 'A' }),
    'apps/dashboard/public/i18n/knowledge/en.json': JSON.stringify({ title: 'Articles' }),
    [TPL]: "<p>{{ 'a' | transloco }}{{ 'knowledge.title' | transloco }}</p>",
  });
  assert.deepEqual(checkCatalogs(base), []);
});
