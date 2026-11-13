#!/usr/bin/env node
// Read a stack trace from a browser error report (MOH-168) in terms of the original source.
//
// The production build writes hidden source maps; they are not in the served image. The release job
// publishes them as a private registry image, `<repo>-dashboard-sourcemaps:<commit>`, which holds
// nothing but the `.map` files. The stack in a report names files such as `main-4TYQZ6FJ.js`; the
// `release` field of the report is the hash of the main script, so pick the build whose maps
// contain `main-<release>.js.map`.
//
//   docker create --name maps ghcr.io/<owner>/<repo>-dashboard-sourcemaps:<commit> true
//   docker cp maps:/ ./maps && docker rm maps
//   node tools/symbolicate.mjs ./maps stack.txt          # or pipe the stack on stdin
//
// Each line that names `<file>.js:<line>:<column>` is resolved through `<file>.js.map`; any other
// line, and a file with no map, is printed as it is.
import { readFileSync, existsSync } from 'node:fs';
import { basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { originalPositionFor, TraceMap } from '@jridgewell/trace-mapping';

const LOCATION = /(?:https?:\/\/[^\s)]*\/)?([\w.-]+\.js):(\d+):(\d+)/;

/** Resolve one stack line. `loadMap(file)` returns a `TraceMap` or `undefined`. */
export function symbolicateLine(line, loadMap) {
  const match = LOCATION.exec(line);
  if (!match) return line;
  const [whole, file, lineNumber, column] = match;
  const map = loadMap(file);
  if (!map) return `${line}  [no source map for ${file}]`;
  // Browsers count columns from 1; the map counts them from 0.
  const original = originalPositionFor(map, {
    line: Number(lineNumber),
    column: Number(column) - 1,
  });
  if (original.source == null || original.line == null) return `${line}  [not in the map]`;
  const place = `${original.source}:${original.line}:${(original.column ?? 0) + 1}`;
  const resolved = line.replace(whole, place);
  return original.name ? `${resolved}  [${original.name}]` : resolved;
}

export function symbolicate(stack, mapsDir) {
  const cache = new Map();
  const loadMap = (file) => {
    if (!cache.has(file)) {
      const path = join(mapsDir, `${basename(file)}.map`);
      cache.set(file, existsSync(path) ? new TraceMap(readFileSync(path, 'utf8')) : undefined);
    }
    return cache.get(file);
  };
  return stack
    .split('\n')
    .map((line) => symbolicateLine(line, loadMap))
    .join('\n');
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [mapsDir, stackFile] = process.argv.slice(2);
  if (!mapsDir) {
    console.error(
      'usage: node tools/symbolicate.mjs <maps-dir> [stack-file]  (stack on stdin otherwise)',
    );
    process.exit(2);
  }
  const stack = readFileSync(stackFile ?? 0, 'utf8');
  process.stdout.write(`${symbolicate(stack, mapsDir)}\n`);
}
