// Flags hard-coded user-facing text in Angular templates (MOH-41, ADR-016, constitution XII).
//
// A template may not gain text that is written straight into it: a text node with letters, or a
// `placeholder`, `title`, `aria-label`, `alt` ... attribute with a literal value. Such text belongs
// in a message catalog. The scan covers `.html` templates and inline `template:` strings under
// `apps/` and `libs/` (spec files excluded).
//
// It is a ratchet. `tools/i18n-baseline.json` records, per file, how much text is still written in
// place. A file may not exceed its baseline, a file not in the baseline may have none, and a file
// below its baseline must lower it (`pnpm lint:i18n --update`), so the number only goes down.
//
// Limits: it reads templates, not TypeScript (a message string in a component class is not seen),
// and it cannot tell a name or a symbol from a word; a literal that must stay (a product name, a
// code) goes in a catalog entry too or in a `{{ '...' }}` binding.
import { readFileSync, readdirSync, statSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseTemplate, TmplAstRecursiveVisitor, tmplAstVisitAll } from '@angular/compiler';
import ts from 'typescript';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const baselineFile = join(root, 'tools', 'i18n-baseline.json');

/** Attributes whose literal value a person reads or hears. */
export const TEXT_ATTRIBUTES = new Set([
  'placeholder',
  'title',
  'alt',
  'label',
  'aria-label',
  'aria-placeholder',
  'aria-description',
  'aria-roledescription',
  'summary',
]);

const hasWord = (text) => /\p{L}{2,}/u.test(text);

/** The hard-coded text in one template: `[{ line, text }]`. */
export function scanTemplate(source, file = 'template.html') {
  const found = [];
  const lineOf = (span) => (span?.start?.line ?? 0) + 1;
  class Scanner extends TmplAstRecursiveVisitor {
    visitText(text) {
      const value = text.value.trim();
      if (hasWord(value)) found.push({ line: lineOf(text.sourceSpan), text: value });
    }
    visitElement(element) {
      for (const attribute of element.attributes) {
        if (TEXT_ATTRIBUTES.has(attribute.name.toLowerCase()) && hasWord(attribute.value)) {
          found.push({
            line: lineOf(attribute.sourceSpan),
            text: `${attribute.name}="${attribute.value}"`,
          });
        }
      }
      super.visitElement(element);
    }
  }
  const parsed = parseTemplate(source, file, { preserveWhitespaces: false });
  tmplAstVisitAll(new Scanner(), parsed.nodes);
  return found;
}

/** Inline templates of the components in a TypeScript file. */
export function inlineTemplates(source, file) {
  const out = [];
  const sf = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const visit = (node) => {
    if (
      ts.isPropertyAssignment(node) &&
      node.name.getText(sf) === 'template' &&
      (ts.isNoSubstitutionTemplateLiteral(node.initializer) || ts.isStringLiteral(node.initializer))
    ) {
      const line = sf.getLineAndCharacterOfPosition(node.initializer.getStart(sf)).line;
      out.push({ text: node.initializer.text, line });
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return out;
}

function* files(dir) {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name === 'dist' || name === 'generated') continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) yield* files(path);
    else if (/\.(html|ts)$/.test(name) && !/\.spec\.ts$/.test(name)) yield path;
  }
}

/** `{ 'apps/x/y.html': [{ line, text }] }` for every file that has hard-coded text. */
export function scan(base = root) {
  const result = {};
  for (const top of ['apps', 'libs']) {
    const dir = join(base, top);
    if (!existsSync(dir)) continue;
    for (const path of files(dir)) {
      const source = readFileSync(path, 'utf8');
      const rel = relative(base, path).split('\\').join('/');
      let hits = [];
      if (path.endsWith('.html')) {
        if (rel.includes('/src/index.html')) continue; // the document shell, not a template
        hits = scanTemplate(source, rel);
      } else if (source.includes('template:')) {
        for (const inline of inlineTemplates(source, rel)) {
          hits.push(
            ...scanTemplate(inline.text, rel).map((h) => ({ ...h, line: h.line + inline.line })),
          );
        }
      }
      if (hits.length) result[rel] = hits;
    }
  }
  return result;
}

/** Compare a scan with the baseline: `{ over, under }` lists. */
export function compare(found, baseline) {
  const over = [];
  const under = [];
  for (const [file, hits] of Object.entries(found)) {
    const allowed = baseline[file] ?? 0;
    if (hits.length > allowed) over.push({ file, allowed, hits });
  }
  for (const [file, allowed] of Object.entries(baseline)) {
    const count = found[file]?.length ?? 0;
    if (count < allowed) under.push({ file, allowed, count });
  }
  return { over, under };
}

function main() {
  const update = process.argv.includes('--update');
  const found = scan();
  if (update) {
    const baseline = Object.fromEntries(
      Object.entries(found)
        .map(([file, hits]) => [file, hits.length])
        .sort(([a], [b]) => a.localeCompare(b)),
    );
    writeFileSync(baselineFile, JSON.stringify(baseline, null, 2) + '\n');
    const total = Object.values(baseline).reduce((a, b) => a + b, 0);
    console.log(
      `i18n baseline written: ${total} hard-coded text(s) in ${Object.keys(baseline).length} file(s)`,
    );
    return 0;
  }
  const baseline = existsSync(baselineFile) ? JSON.parse(readFileSync(baselineFile, 'utf8')) : {};
  const { over, under } = compare(found, baseline);
  for (const { file, allowed, hits } of over) {
    console.error(
      `${file}: ${hits.length} hard-coded text(s), ${allowed} allowed. Move the new text to the message catalog (ADR-016):`,
    );
    for (const hit of hits) console.error(`  line ${hit.line}: ${hit.text.slice(0, 80)}`);
  }
  for (const { file, allowed, count } of under) {
    console.error(
      `${file}: ${count} hard-coded text(s), the baseline says ${allowed}. Lower it: pnpm lint:i18n --update`,
    );
  }
  if (over.length || under.length) return 1;
  const total = Object.values(baseline).reduce((a, b) => a + b, 0);
  console.log(
    `i18n: no new hard-coded text (${total} still to move, in ${Object.keys(baseline).length} files)`,
  );
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main());
}
