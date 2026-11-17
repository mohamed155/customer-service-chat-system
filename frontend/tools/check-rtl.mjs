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
// The whole declaration is read, so a property and its value on two lines are one declaration. In
// a `.ts` file only the inline `styles: [...]` are read (an object `{ left: 0 }` is not CSS).
//
// A declaration that must stay physical says why on its line: `rtl-lint-ignore: <why>` (a corner
// the tenant picks for the chat widget, a value centred on both sides). Specs, fixtures and
// generated code are not scanned.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const roots = ['apps/dashboard/src', 'apps/widget/src'];

const MARKER = /rtl-lint-ignore:\s*[^\s*/-]/;

/** One rule: a pattern, and what to use instead. */
const RULES = [
  {
    pattern: /\b(?:margin|padding|scroll-margin|scroll-padding)-(?:left|right)\b/g,
    use: 'margin-inline-start/-end (padding-, scroll-margin-, scroll-padding- likewise)',
  },
  { pattern: /\bborder-(?:left|right)(?:-[a-z]+)?\b(?!-radius)/g, use: 'border-inline-start/-end' },
  {
    pattern: /\bborder-(?:top|bottom)-(?:left|right)-radius\b/g,
    use: 'border-start-start-radius, border-start-end-radius, border-end-start-radius, border-end-end-radius',
  },
  {
    pattern: /(?<![\w.#-])(?:left|right)\s*:\s*[^\s;{}]/g,
    use: 'inset-inline-start / inset-inline-end',
  },
  { pattern: /\b(?:float|clear)\s*:\s*(?:left|right)\b/g, use: 'inline-start / inline-end' },
  { pattern: /\btext-align\s*:\s*(?:left|right)\b/g, use: 'text-align: start / end' },
  {
    pattern:
      /\b(?:background-position|transform-origin|object-position)\s*:[^;{}]*\b(?:left|right)\b/g,
    use: 'a position that does not name a side (center), or a logical value set per direction',
  },
  {
    pattern: /\bto\s+(?:left|right)\b/g,
    use: 'a gradient that does not run along the inline axis',
  },
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

/** `text` with every character but the line ends replaced by a space: the lines stay where they are. */
const blank = (text) => text.replace(/[^\n]/g, ' ');

/**
 * `source` without its comments (their lines kept), so prose such as "right to left" is not read as
 * CSS. A `//` inside a quoted string (`content: ' // '`) is text, not a comment.
 */
function withoutComments(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/<!--[\s\S]*?-->/g, blank)
    .replace(/'(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*"|(^|[\s`])\/\/.*$/gm, (match, before) =>
      before === undefined ? match : `${before}${blank(match.slice(before.length))}`,
    );
}

/**
 * What is CSS in a file: all of a stylesheet or template, but in a `.ts` file only the inline
 * `styles: [...]`, and only the strings that are directly in it, not in an object inside it (everything else is blanked, the lines kept).
 */
function styleText(path, source) {
  if (!path.endsWith('.ts')) return source;
  const out = blank(source).split('');
  for (const start of source.matchAll(/\bstyles\s*:\s*\[/g)) {
    let depth = 1;
    let braces = 0;
    let quote = null;
    let end = start.index + start[0].length;
    for (; end < source.length && depth > 0; end += 1) {
      const ch = source[end];
      if (quote) {
        if (ch === '\\') {
          if (braces === 0) out[end] = ch;
          end += 1;
          if (braces === 0) out[end] = source[end] ?? ' ';
        } else if (ch === quote) quote = null;
        else if (braces === 0) out[end] = ch;
      } else if (ch === '`' || ch === "'" || ch === '"') quote = ch;
      else if (ch === '{') braces += 1;
      else if (ch === '}') braces -= 1;
      else if (ch === '[') depth += 1;
      else if (ch === ']') depth -= 1;
    }
  }
  return out.join('');
}

/** The text of the balanced parentheses that open at `open`, without them. */
function argumentsAt(text, open) {
  let depth = 0;
  for (let i = open; i < text.length; i += 1) {
    if (text[i] === '(') depth += 1;
    if (text[i] === ')') {
      depth -= 1;
      if (depth === 0) return text.slice(open + 1, i);
    }
  }
  return text.slice(open + 1);
}

/**
 * Every physical style in `css` (comment-free): `{ index, length, found, use }`. Each shorthand and
 * each `translateX` is judged by itself.
 */
function physicalIn(css) {
  const hits = [];
  for (const { pattern, use } of RULES) {
    for (const m of css.matchAll(pattern)) {
      hits.push({
        index: m.index,
        length: m[0].length,
        found: m[0].trim().replace(/\s+/g, ' '),
        use,
      });
    }
  }
  for (const m of css.matchAll(/\btranslateX\(/g)) {
    const argument = argumentsAt(css, m.index + m[0].length - 1);
    if (argument.trim() !== '0' && !/var\(--app-dir-sign\)/.test(argument)) {
      hits.push({
        index: m.index,
        length: m[0].length + argument.length,
        found: 'translateX(…)',
        use: 'calc(<distance> * var(--app-dir-sign)), or a logical property',
      });
    }
  }
  for (const m of css.matchAll(/\b(margin|padding|inset|border-radius)\s*:\s*([^;{}"'`<>]+)/g)) {
    const parts = values(m[2].replace(/\s*!important\s*$/, '').replace(/\/.*$/, ''));
    if (parts.length !== 4) continue;
    const [a, b, c, d] = parts;
    const differs = m[1] === 'border-radius' ? a !== b || d !== c : b !== d;
    if (differs) {
      hits.push({
        index: m.index,
        length: m[0].length,
        found: `${m[1]} with four different values`,
        use: `the logical form (${m[1]}-inline-start/-end, or ${m[1]}-block/-inline)`,
      });
    }
  }
  return hits;
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
    const css = withoutComments(styleText(path, source));
    const lineStarts = [];
    let offset = 0;
    for (const line of css.split('\n')) {
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
      return low;
    };
    for (const hit of physicalIn(css)) {
      // The whole declaration: from after the previous `;`, `{` or `}` to the next `;` or `}`.
      const before = Math.max(
        css.lastIndexOf(';', hit.index),
        css.lastIndexOf('{', hit.index),
        css.lastIndexOf('}', hit.index),
      );
      const start = before + 1 + (css.slice(before + 1).match(/^\s*/)?.[0].length ?? 0);
      const after = css.slice(hit.index + hit.length).search(/[;}]/);
      const end = after < 0 ? css.length : hit.index + hit.length + after;
      const first = lineOf(Math.min(start, hit.index));
      const last = lineOf(Math.max(end - 1, hit.index));
      // A marker on any line of the declaration covers it.
      if (original.slice(first, last + 1).some((line) => MARKER.test(line))) continue;
      problems.push(
        `${path}:${first + 1}: ${hit.found} is tied to the screen's left or right and does not ` +
          `mirror in a right-to-left language. Use ${hit.use}, or mark the line ` +
          `"rtl-lint-ignore: <why>" if it must stay physical. See docs/architecture/right-to-left.md.`,
      );
    }
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
