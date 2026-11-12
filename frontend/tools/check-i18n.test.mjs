import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { compare, inlineTemplates, scan, scanTemplate } from './check-i18n.mjs';

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
