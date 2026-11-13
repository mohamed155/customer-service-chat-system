import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { symbolicate } from './symbolicate.mjs';

// Two segments. Generated line 1, column 0 is `boom` at src/app/boom.ts line 12, column 4 (AAWIA:
// source 0, line +11, column +4, name 0). Generated line 2, column 0 is src/app/other.ts line 7,
// column 4, with no name (ACLA: source +1, line -5, column +0).
const MAP = {
  version: 3,
  file: 'main-AAAAAAAA.js',
  sources: ['src/app/boom.ts', 'src/app/other.ts'],
  names: ['boom'],
  mappings: 'AAWIA;ACLA',
};

function maps() {
  const dir = mkdtempSync(join(tmpdir(), 'symbolicate-'));
  writeFileSync(join(dir, 'main-AAAAAAAA.js.map'), JSON.stringify(MAP));
  return dir;
}

test('resolves a stack line to the original file, line, column and name (browser columns count from 1)', () => {
  const out = symbolicate('    at fn (https://app.test/main-AAAAAAAA.js:1:1)', maps());

  assert.equal(out, '    at fn (src/app/boom.ts:12:5)  [boom]');
});

test('resolves a line without a name and a location without a URL', () => {
  const out = symbolicate('main-AAAAAAAA.js:2:1', maps());

  assert.equal(out, 'src/app/other.ts:7:5');
});

test('leaves a line without a location as it is', () => {
  assert.equal(
    symbolicate('TypeError: x is not a function', maps()),
    'TypeError: x is not a function',
  );
});

test('says so when a file has no map or a position is not in the map', () => {
  const dir = maps();

  assert.match(
    symbolicate('at a (https://app.test/chunk-ZZZZZZZZ.js:1:1)', dir),
    /\[no source map for chunk-ZZZZZZZZ\.js\]$/,
  );
  assert.match(
    symbolicate('at a (https://app.test/main-AAAAAAAA.js:99:1)', dir),
    /\[not in the map\]$/,
  );
});
