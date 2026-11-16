// Keeps hard-coded colors out of the dashboard (MOH-72, constitution principle XII).
//
// Every color a screen shows comes from a design token (`var(--app-…)`), so the light and the dark
// theme both reach it. The tokens live in `apps/dashboard/src/app/design-system/`, the one place a
// color value may be written. This check scans the rest of the dashboard (`.ts` with inline styles,
// `.html`, `.css`, `.scss`) and fails on:
//
//   1. a color literal: `#hex`, `rgb()`, `rgba()`, `hsl()`, `hsla()`, `hwb()`, `lab()`, `lch()`,
//      `oklab()`, `oklch()`, or a color name (`white`, `black`, ...) as the value of a color property;
//      a literal inside a `var(--x, #fff)` fallback counts too, because it shows when the token is
//      not defined;
//   2. a `var(--app-…)` that is not defined in the design system in both themes (a token that is
//      misspelled or does not exist falls back to nothing, or to the literal beside it).
//
// The design system itself is checked for one thing: a themed token must be defined in the light
// and the dark theme, or one theme shows the other's fallback.
//
// A line may keep a literal with `color-lint-ignore: <why>` on it (a value a tenant stores, not a
// color of the dashboard). A whole file is listed in EXEMPT with the reason. Specs, fixtures and
// generated code are not scanned.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const src = 'apps/dashboard/src';
const designSystem = `${src}/app/design-system/`;

/** Files that may keep literals, with the reason. */
export const EXEMPT = {
  [`${src}/index.html`]:
    'the loading skeleton shown before the app and its stylesheet load: no tokens exist yet',
};

const MARKER = /color-lint-ignore:\s*\S/;
const FUNCTIONS = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/;
const HEX = /(?<![\w&])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b/;
const NAMES = new Set(
  (
    'white black red green blue yellow orange purple pink gray grey silver gold brown cyan magenta ' +
    'lime navy teal maroon olive aqua fuchsia'
  ).split(' '),
);
const COLOR_PROPERTY =
  /(?:^|[;{\s"'])(color|background(?:-color)?|border(?:-[a-z]+)*|outline(?:-color)?|fill|stroke|text-decoration-color|caret-color|accent-color|box-shadow|text-shadow)\s*:\s*([^;}\n]*)/dg;
// What looks like a hex color but is a link target or an SVG reference: `href="#abc"`, `url(#abc)`.
const NOT_A_COLOR =
  /(?:\b(?:xlink:)?href|\brouterLink|\bfragment)\s*=\s*(?:"[^"]*"|'[^']*')|url\(\s*['"]?#[^)]*\)/g;
const COLOR_ATTRIBUTE = /\b(?:fill|stroke|color|bgcolor)="([a-z]+)"/g;
const TOKEN_USE = /var\(\s*(--app-[a-z0-9-]+)/g;
const TOKEN_DEFINITION = /(--app-[a-z0-9-]+)\s*:/g;

/** The `--app-…` names defined in `text`. */
function definitions(text) {
  return new Set([...text.matchAll(TOKEN_DEFINITION)].map((m) => m[1]));
}

/** The body of the first rule whose selector contains `selector`, or ''. */
function block(text, selector) {
  const start = text.indexOf(selector);
  if (start < 0) return '';
  const open = text.indexOf('{', start);
  const close = text.indexOf('}', open);
  return open < 0 || close < 0 ? '' : text.slice(open + 1, close);
}

/**
 * What the design system defines: `themed` (in the light and the dark theme), `fixed` (theme
 * independent, in tokens.css), and the parity problems of the themes.
 * @param {{ path: string, source: string }[]} files design-system files
 */
export function readTokens(files) {
  const fixed = new Set();
  const themed = new Set();
  const problems = [];
  for (const { path, source } of files) {
    if (path.endsWith('tokens.css')) {
      for (const name of definitions(source)) fixed.add(name);
    }
    if (path.endsWith('themes.css')) {
      const light = definitions(block(source, "[data-theme='light']"));
      const dark = definitions(block(source, "[data-theme='dark']"));
      for (const name of light) {
        if (dark.has(name)) themed.add(name);
        else problems.push(`${path}: ${name} is defined in the light theme but not the dark one`);
      }
      for (const name of dark) {
        if (!light.has(name))
          problems.push(`${path}: ${name} is defined in the dark theme but not the light one`);
      }
    }
  }
  return { fixed, themed, problems };
}

/** The hex, color function or color-name attribute in one line of code, or `null`. */
function literalIn(line) {
  const text = line.replace(/var\(\s*--[a-z0-9-]+\s*\)/g, '').replace(NOT_A_COLOR, ' ');
  const hex = text.match(HEX);
  if (hex) return hex[0];
  const fn = text.match(FUNCTIONS);
  if (fn) return `${fn[0]}…)`;
  for (const m of text.matchAll(COLOR_ATTRIBUTE)) {
    if (NAMES.has(m[1])) return m[1];
  }
  return null;
}

/**
 * Every literal in `source` with its 1-based line: the per-line ones, and the color names that
 * are the value of a color property (found over the whole text, so a property and its value on
 * two lines are still one declaration).
 */
function literalsIn(source) {
  const lines = source.split('\n');
  const found = [];
  lines.forEach((line, index) => {
    const literal = literalIn(line);
    if (literal) found.push({ line: index + 1, literal });
  });
  const lineStarts = [];
  let offset = 0;
  for (const line of lines) {
    lineStarts.push(offset);
    offset += line.length + 1;
  }
  const lineOf = (position) => {
    let low = 0;
    let high = lineStarts.length - 1;
    while (low < high) {
      const mid = Math.ceil((low + high) / 2);
      if (lineStarts[mid] <= position) low = mid;
      else high = mid - 1;
    }
    return low + 1;
  };
  for (const m of source.matchAll(COLOR_PROPERTY)) {
    // A name beside a token (`var(--x, white)`) is a fallback and counts; the token alone does not.
    const value = m[2]
      .replace(/var\(\s*--[\w-]+\s*\)/g, ' ')
      .replace(/var\(\s*--[\w-]+\s*,/g, ' ')
      .replace(/color-mix\(/g, ' ')
      .toLowerCase();
    const named = value.split(/[^a-z-]+/).find((word) => NAMES.has(word));
    if (named) found.push({ line: lineOf(m.indices[2][0]), literal: named });
  }
  return found;
}

/**
 * Problems in `files` (`[{ path, source }]`, paths relative to the frontend root), against the
 * design system's `designFiles`.
 */
export function check(files, designFiles, exempt = EXEMPT) {
  const { fixed, themed, problems } = readTokens(designFiles);
  const known = new Set([...fixed, ...themed]);
  const scanned = files.filter(
    (f) =>
      f.path.startsWith(`${src}/`) &&
      !f.path.startsWith(designSystem) &&
      !/\.spec\.ts$/.test(f.path) &&
      !f.path.includes('/core/api/generated/') &&
      !f.path.includes('/shared/fixtures/') &&
      /\.(ts|html|css|scss)$/.test(f.path),
  );
  const seen = new Set(scanned.map((f) => f.path));
  for (const path of Object.keys(exempt)) {
    if (!seen.has(path))
      problems.push(`${path}: is in EXEMPT but no such file exists; remove the entry`);
  }
  for (const { path, source } of scanned) {
    if (path in exempt) continue;
    const lines = source.split('\n');
    // A custom property a component sets itself (`--app-x: …` in its own styles) is defined for
    // that component only.
    const local = definitions(source);
    const reported = new Set();
    for (const { line, literal } of literalsIn(source)) {
      if (MARKER.test(lines[line - 1]) || reported.has(`${line}:${literal}`)) continue;
      reported.add(`${line}:${literal}`);
      problems.push(
        `${path}:${line}: hard-coded color ${literal}. Use a design token (var(--app-…), see ` +
          `docs/architecture/design-tokens.md), add one to design-system/ if none fits, or ` +
          `mark the line "color-lint-ignore: <why>" if it is a value, not a dashboard color.`,
      );
    }
    lines.forEach((line, index) => {
      if (MARKER.test(line)) return;
      for (const m of line.matchAll(TOKEN_USE)) {
        if (!known.has(m[1]) && !local.has(m[1])) {
          problems.push(
            `${path}:${index + 1}: ${m[1]} is not defined in the design system (light and dark). ` +
              `Use an existing token or add it to design-system/theme/themes.css.`,
          );
        }
      }
    });
  }
  return problems;
}

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules') continue;
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

/** Every file under the dashboard's `src/`, as `{ path, source }`. */
export function readFiles(base = root) {
  return walk(join(base, src)).map((full) => ({
    path: relative(base, full).split(sep).join('/'),
    source: readFileSync(full, 'utf8'),
  }));
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const files = readFiles();
  const design = files.filter((f) => f.path.startsWith(designSystem));
  const problems = check(files, design);
  if (problems.length) {
    console.error(problems.join('\n'));
    console.error(`\ncolors: ${problems.length} problem(s)`);
    process.exit(1);
  }
  console.log('colors: every dashboard color is a design token defined in both themes');
}
