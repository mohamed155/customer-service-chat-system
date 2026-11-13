// Checks that the shared dashboard code types its API calls with the generated types (MOH-169,
// ADR-012 gap A5).
//
// `core/api/generated/api.ts` is generated from the backend's OpenAPI document. A call such as
// `api.get<Schemas['MeResponse']>('/me')` follows the contract: when the backend changes, the call
// stops compiling. A call such as `api.get<MyHandWrittenType>('/me')` does not: the type is the
// author's word, and the whole dashboard breaks at run time instead. `core/` and `layout/` are the
// calls every page depends on, so there a type argument on `api.get/post/put/patch/delete` (and
// `http.*`) has to be:
//   - `void` or `unknown` (nothing to type),
//   - `Schemas['…']` or `components['schemas']['…']` (the generated types),
//   - a name declared in the same file as `type X = Schemas['…']`, or
//   - a union of those.
// A file that cannot follow this yet is listed in EXEMPT with the reason: the list is short and
// each entry names the issue that removes it.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const appDir = 'apps/dashboard/src/app';
const scopes = [`${appDir}/core/`, `${appDir}/layout/`];

/** Files that may keep hand-typed calls, with the reason. */
export const EXEMPT = {
  [`${appDir}/core/api/api.service.ts`]:
    'the wrapper itself: its methods are generic over the type the caller names',
  [`${appDir}/core/api/customers-api.service.ts`]:
    'the customers API, moved here from the features because two use it (MOH-62); it moves to the generated types with MOH-145',
};

const CALL = /\b(?:api|http)\s*\.\s*(?:get|post|put|patch|delete)\s*</g;

/** The text between the `<` at `open` and its matching `>`, or null. */
function typeArgument(source, open) {
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    const ch = source[i];
    if (ch === '<') depth += 1;
    else if (ch === '>' && source[i - 1] !== '=') {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  return null;
}

/** Split on `|` that is not inside brackets. */
function unionMembers(text) {
  const parts = [];
  let depth = 0;
  let current = '';
  for (const ch of text) {
    if ('<[({'.includes(ch)) depth += 1;
    else if ('>])}'.includes(ch)) depth -= 1;
    if (ch === '|' && depth === 0) {
      parts.push(current);
      current = '';
    } else current += ch;
  }
  parts.push(current);
  return parts.map((p) => p.trim()).filter(Boolean);
}

/** Names declared as `type X = Schemas['…']` (or `components[…]`) in `source`. */
function generatedAliases(source) {
  const names = new Set();
  for (const m of source.matchAll(
    /\btype\s+([A-Za-z_]\w*)\s*=\s*(?:Schemas|components)\s*\[[^;]*;/g,
  )) {
    names.add(m[1]);
  }
  return names;
}

function isGenerated(member, aliases) {
  return (
    member === 'void' ||
    member === 'unknown' ||
    /^(?:Schemas|components)\s*\[/.test(member) ||
    aliases.has(member)
  );
}

const lineOf = (source, index) => source.slice(0, index).split('\n').length;

/** Problems in `files` (`[{ path, source }]`, paths relative to the frontend root). */
export function check(files, exempt = EXEMPT) {
  const problems = [];
  const inScope = files.filter(
    (f) => scopes.some((s) => f.path.startsWith(s)) && !f.path.endsWith('.spec.ts'),
  );
  const flagged = new Set();
  for (const { path, source } of inScope) {
    const aliases = generatedAliases(source);
    for (const match of source.matchAll(CALL)) {
      const argument = typeArgument(source, match.index + match[0].length - 1);
      if (argument === null) continue;
      const handTyped = unionMembers(argument).filter((m) => !isGenerated(m, aliases));
      if (handTyped.length === 0) continue;
      flagged.add(path);
      if (path in exempt) continue;
      problems.push(
        `${path}:${lineOf(source, match.index)}: ${match[0].replace(/\s+/g, '')}${argument}> ` +
          `is typed by hand (${handTyped.join(', ')}). Use a type from core/api/generated ` +
          `(\`Schemas['…']\`) and map it where the app needs a narrower type, or list the file in ` +
          `tools/check-generated-api-types.mjs EXEMPT with the issue that removes it.`,
      );
    }
  }
  const seen = new Set(inScope.map((f) => f.path));
  for (const path of Object.keys(exempt)) {
    if (!seen.has(path)) {
      problems.push(`${path}: is in EXEMPT but no such file exists; remove the entry`);
    } else if (!flagged.has(path)) {
      problems.push(`${path}: is in EXEMPT but has no hand-typed call any more; remove the entry`);
    }
  }
  return problems;
}

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (name.endsWith('.ts')) out.push(full);
  }
  return out;
}

/** The dashboard's TypeScript files under `base`. */
export function readFiles(base = root) {
  return walk(join(base, appDir)).map((full) => ({
    path: relative(base, full).split('\\').join('/'),
    source: readFileSync(full, 'utf8'),
  }));
}

function main() {
  const problems = check(readFiles());
  for (const problem of problems) console.error(problem);
  if (problems.length) return 1;
  console.log('api types: shared code calls the API with the generated types');
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main());
}
