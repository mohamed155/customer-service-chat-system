// Keeps physical left/right out of the dashboard's and the widget's styles (MOH-73, constitution
// principle XII).
//
// The layout has to mirror for a right-to-left language, so a style says where something goes
// relative to the reading direction (`margin-inline-start`, `inset-inline-end`, `text-align: end`),
// never relative to the screen (`margin-left`, `right: 0`, `text-align: right`). This check scans
// `.ts` (inline styles), `.html`, `.css` and `.scss` under `apps/dashboard/src` and
// `apps/widget/src`, and fails on:
//
//   - `margin-`, `padding-`, `border-` and `scroll-margin/padding-` with `left` or `right`,
//     `border-top-left-radius` and its three siblings;
//   - `left:` and `right:` as a position, and `float`/`clear`/`text-align` with `left` or `right`;
//   - a four-value `margin`, `padding`, `inset` or `border-radius` whose left and right differ;
//   - `translateX(...)`, which does not mirror (use `calc(... * var(--app-dir-sign))`, which does);
//   - `left` or `right` in `background-position`, `transform-origin`, `object-position` and in a
//     gradient direction (`to right`).
//
// A line that must stay physical says why on the line: `rtl-lint-ignore: <why>` (a corner the
// tenant picks for the chat widget, a value centred on both sides). Specs, fixtures and generated
// code are not scanned.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const roots = ['apps/dashboard/src', 'apps/widget/src'];

const MARKER = /rtl-lint-ignore:\s*[^\s*/]/;

/** One rule: a pattern, and what to use instead. */
const RULES = [
  {
    pattern: /\b(?:margin|padding|scroll-margin|scroll-padding)-(?:left|right)\b/,
    use: 'margin-inline-start/-end (padding-, scroll-margin-, scroll-padding- likewise)',
  },
  { pattern: /\bborder-(?:left|right)(?:-[a-z]+)?\b(?!-radius)/, use: 'border-inline-start/-end' },
  {
    pattern: /\bborder-(?:top|bottom)-(?:left|right)-radius\b/,
    use: 'border-start-start-radius, border-start-end-radius, border-end-start-radius, border-end-end-radius',
  },
  {
    pattern: /(?:^|[\s;{"'])(?:left|right)\s*:\s*[^\s;{}]/,
    use: 'inset-inline-start / inset-inline-end',
  },
  { pattern: /\b(?:float|clear)\s*:\s*(?:left|right)\b/, use: 'inline-start / inline-end' },
  { pattern: /\btext-align\s*:\s*(?:left|right)\b/, use: 'text-align: start / end' },
  {
    pattern:
      /\b(?:background-position|transform-origin|object-position)\s*:[^;]*\b(?:left|right)\b/,
    use: 'a position that does not name a side (center), or a logical value set per direction',
  },
  { pattern: /\bto\s+(?:left|right)\b/, use: 'a gradient that does not run along the inline axis' },
];

/** The values of a CSS shorthand, split on spaces outside parentheses. */
function values(text) {
  const out = [];
  let depth = 0;
  let current = '';
  for (const ch of text) {
    if (ch === '(') depth += 1;
    if (ch === ')') depth -= 1;
    if (/\s/.test(ch) && depth === 0) {
      if (current) out.push(current);
      current = '';
    } else {
      current += ch;
    }
  }
  if (current) out.push(current);
  return out;
}

/** A four-value shorthand whose left and right (or whose corners across) differ, or `null`. */
function asymmetricShorthand(line) {
  const m = line.match(/\b(margin|padding|inset|border-radius)\s*:\s*([^;{}]+)/);
  if (!m) return null;
  const parts = values(m[2].replace(/\s*!important\s*$/, '').replace(/\/.*$/, ''));
  if (parts.length !== 4) return null;
  const [a, b, c, d] = parts;
  const differs = m[1] === 'border-radius' ? a !== b || d !== c : b !== d;
  return differs ? m[1] : null;
}

/** `source` without its comments (their lines kept), so prose such as "right to left" is not read as CSS. */
function withoutComments(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, (comment) => comment.replace(/[^\n]/g, ' '))
    .replace(/<!--[\s\S]*?-->/g, (comment) => comment.replace(/[^\n]/g, ' '))
    .replace(/(^|\s)\/\/.*$/gm, '$1');
}

/** What the line does that is tied to the screen's left or right, or `null`. */
function physical(line) {
  for (const { pattern, use } of RULES) {
    const hit = line.match(pattern);
    if (hit) return { found: hit[0].trim(), use };
  }
  if (/\btranslateX\((?!\s*0\s*\))/.test(line) && !/var\(--app-dir-sign\)/.test(line)) {
    return {
      found: 'translateX(…)',
      use: 'calc(<distance> * var(--app-dir-sign)), or a logical property',
    };
  }
  const shorthand = asymmetricShorthand(line);
  if (shorthand) {
    return {
      found: `${shorthand} with four different values`,
      use: `the logical form (${shorthand}-inline-start/-end, or ${shorthand}-block/-inline)`,
    };
  }
  return null;
}

/**
 * Problems in `files` (`[{ path, source }]`, paths relative to the frontend root).
 */
export function check(files) {
  const problems = [];
  const scanned = files.filter(
    (f) =>
      roots.some((r) => f.path.startsWith(`${r}/`)) &&
      !/\.spec\.ts$/.test(f.path) &&
      !f.path.includes('/core/api/generated/') &&
      !f.path.includes('/shared/fixtures/') &&
      /\.(ts|html|css|scss)$/.test(f.path),
  );
  for (const { path, source } of scanned) {
    const original = source.split('\n');
    withoutComments(source)
      .split('\n')
      .forEach((line, index) => {
        if (MARKER.test(original[index])) return;
        const hit = physical(line);
        if (hit) {
          problems.push(
            `${path}:${index + 1}: ${hit.found} is tied to the screen's left or right and does not ` +
              `mirror in a right-to-left language. Use ${hit.use}, or mark the line ` +
              `"rtl-lint-ignore: <why>" if it must stay physical. See docs/architecture/right-to-left.md.`,
          );
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

/** Every file under the scanned roots, as `{ path, source }`. */
export function readFiles(base = root) {
  return roots.flatMap((r) =>
    walk(join(base, r)).map((full) => ({
      path: relative(base, full).split(sep).join('/'),
      source: readFileSync(full, 'utf8'),
    })),
  );
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const problems = check(readFiles());
  if (problems.length) {
    console.error(problems.join('\n'));
    console.error(`\nrtl: ${problems.length} problem(s)`);
    process.exit(1);
  }
  console.log('rtl: every style is logical, so the layout mirrors for a right-to-left language');
}
