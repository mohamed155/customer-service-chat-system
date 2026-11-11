#!/usr/bin/env node
// Supply-chain checks for GitHub Actions workflows (MOH-39, ADR-015 section 3, threat model 4.14 S).
//
//   1. Every `uses:` is pinned to a full 40-character commit SHA, with the tag in a trailing
//      comment (`uses: actions/checkout@<sha> # v7.0.1`). A tag can be moved; a SHA cannot. Local
//      actions (`./path`) and `docker://image@sha256:<digest>` references are also accepted.
//   2. Every workflow declares a top-level `permissions:` block, so the token is least-privilege
//      no matter what the repository default is, and none of them grants `write-all`.
//
// Usage: node .github/scripts/check-workflows.mjs [root]   (root defaults to the current directory)
// Exit code 1 lists every violation as `file:line: message`.
//
// The files are read line by line instead of with a YAML parser, so the script has no dependencies
// and runs on a bare runner. That is enough for the two rules above; flow-style `{ uses: x }` on a
// single line is not recognised, so keep `uses:` on its own line (as every workflow here does).

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const USES = /^\s*(?:-\s+)?uses:\s*(?<value>.*?)\s*$/;
const FULL_SHA = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+(?:\/[^@\s]+)?@[0-9a-f]{40}$/;
const DOCKER_DIGEST = /^docker:\/\/\S+@sha256:[0-9a-f]{64}$/;

function splitComment(raw) {
  // `value # comment`: a `#` starts a comment only after whitespace, and quotes are stripped.
  const match = /^(?<value>.*?)(?:\s+#\s*(?<comment>.*))?$/.exec(raw);
  const value = match.groups.value.replace(/^(["'])(.*)\1$/, '$2');
  return { value, comment: (match.groups.comment ?? '').trim() };
}

export function checkUses(lines, file) {
  const problems = [];
  lines.forEach((line, index) => {
    const found = USES.exec(line);
    if (!found || line.trimStart().startsWith('#')) return;
    const { value, comment } = splitComment(found.groups.value);
    const where = `${file}:${index + 1}`;
    if (value.startsWith('./')) return;
    if (DOCKER_DIGEST.test(value)) return;
    if (!FULL_SHA.test(value)) {
      problems.push(`${where}: \`uses: ${value}\` is not pinned to a full commit SHA`);
    } else if (comment === '') {
      problems.push(`${where}: \`uses: ${value}\` needs the tag in a trailing comment (# v1.2.3)`);
    }
  });
  return problems;
}

export function checkPermissions(lines, file) {
  const index = lines.findIndex((line) => /^permissions\s*:/.test(line));
  if (index === -1) {
    return [`${file}:1: no top-level \`permissions:\` block (add \`permissions: { contents: read }\`)`];
  }
  if (/^permissions\s*:\s*write-all\b/.test(lines[index])) {
    return [`${file}:${index + 1}: top-level \`permissions: write-all\` is not least-privilege`];
  }
  return [];
}

function workflowFiles(root) {
  const files = [];
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (/\.ya?ml$/.test(name)) files.push(path);
    }
  };
  walk(join(root, '.github', 'workflows'));
  try {
    walk(join(root, '.github', 'actions'));
  } catch {
    // no local actions
  }
  return files;
}

export function run(root) {
  const problems = [];
  const files = workflowFiles(root);
  for (const path of files) {
    const file = relative(root, path);
    const lines = readFileSync(path, 'utf8').split('\n');
    problems.push(...checkUses(lines, file));
    if (file.startsWith(join('.github', 'workflows'))) problems.push(...checkPermissions(lines, file));
  }
  return { files, problems };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const { files, problems } = run(process.argv[2] ?? process.cwd());
  if (problems.length > 0) {
    for (const problem of problems) console.error(`::error::${problem}`);
    console.error(`${problems.length} workflow problem(s) in ${files.length} file(s).`);
    process.exit(1);
  }
  console.log(`OK: ${files.length} workflow file(s) pinned by SHA with explicit permissions.`);
}
