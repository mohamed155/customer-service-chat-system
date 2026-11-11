// Run with: node --test .github/scripts/
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { checkPermissions, checkUses } from './check-workflows.mjs';

const SHA = '3d3c42e5aac5ba805825da76410c181273ba90b1';
const uses = (text) => checkUses(text.split('\n'), 'w.yml');

test('accepts a SHA pin with the tag in a comment', () => {
  assert.deepEqual(uses(`      - uses: actions/checkout@${SHA} # v7.0.1`), []);
  assert.deepEqual(uses(`        uses: github/codeql-action/analyze@${SHA} # v3.1.0`), []);
});

test('accepts local actions and docker digests', () => {
  assert.deepEqual(uses('      - uses: ./.github/actions/setup'), []);
  assert.deepEqual(uses(`      - uses: docker://alpine@sha256:${'a'.repeat(64)}`), []);
});

test('rejects tags, branches and short SHAs', () => {
  for (const ref of ['v4', 'main', 'v4.1.0', SHA.slice(0, 7), `${SHA}0`]) {
    const problems = uses(`      - uses: actions/checkout@${ref}`);
    assert.equal(problems.length, 1, ref);
    assert.match(problems[0], /w\.yml:1: .*not pinned/);
  }
});

test('rejects a docker tag and a reference without a version', () => {
  assert.equal(uses('      - uses: docker://alpine:3.20').length, 1);
  assert.equal(uses('      - uses: actions/checkout').length, 1);
});

test('requires the tag comment on a SHA pin', () => {
  const problems = uses(`      - uses: actions/checkout@${SHA}`);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /trailing comment/);
});

test('reports quoted values and the right line number', () => {
  const problems = uses(`steps:\n  - name: x\n    uses: "actions/checkout@v4"`);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /w\.yml:3:/);
});

test('ignores commented-out uses lines and run text', () => {
  assert.deepEqual(uses('      # - uses: actions/checkout@v4'), []);
  assert.deepEqual(uses('      - run: echo uses is fine'), []);
});

test('permissions: required at the top level, write-all rejected', () => {
  const check = (text) => checkPermissions(text.split('\n'), 'w.yml');
  assert.deepEqual(check('name: x\npermissions:\n  contents: read\njobs: {}'), []);
  assert.equal(check('name: x\njobs:\n  a:\n    permissions:\n      contents: read').length, 1);
  assert.equal(check('name: x\npermissions: write-all\njobs: {}').length, 1);
});
