// Enforces the module boundary rules of constitution Principle IV and ADR-001 (MOH-79).
//
// What it checks, from `cargo metadata`, the source tree and the migrations:
//   1. Every module crate is registered in `module-boundaries.json`, and every module -> module
//      dependency is on that module's `dependsOn` list. A new edge fails until someone adds it on
//      purpose; an edge that no longer exists fails until it is removed (a ratchet, like the
//      frontend coverage baseline).
//   2. Hard edges: shared and adapter crates never depend on a module or on `server`; no module
//      depends on `server`; no module calls an event consumer (`analytics`, `audit`,
//      `notifications`) unless the call is a named exception with a reason.
//   3. The workspace crate graph (normal, build and dev edges) has no cycle.
//   4. Layering: a module that is not on the `flatModules` list has `domain/`, `application/`,
//      `infrastructure/` and `api/` under `src/`, and its layers import inward only (`domain`
//      imports no framework and no outer layer). A flat module is listed until it migrates.
//   5. Table ownership: every table created by a migration has an owner; SQL in a crate that names
//      another module's table is counted and may not grow (`crossModuleSql` is the ratchet). A
//      read-model crate (`analytics`) may read the tables it lists and may not write anything.
//   6. `docs/architecture/module-dependencies.md` is generated from the same data and must be
//      current (`--write-doc` regenerates it).
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const backendDir = resolve(here, '..');
const repoRoot = resolve(backendDir, '..');
const DOC_PATH = 'docs/architecture/module-dependencies.md';
const LAYERS = ['domain', 'application', 'infrastructure', 'api'];

/** Frameworks and vendor SDKs the domain layer must not name (Principle IV). */
const DOMAIN_FORBIDDEN_CRATES = [
  'sqlx',
  'axum',
  'redis',
  'reqwest',
  'hyper',
  'tower',
  'tower_http',
  'lettre',
  'aws_sdk_s3',
  'actix_web',
  'actix_http',
];

const kindOf = (dir) =>
  dir.startsWith('modules/')
    ? 'module'
    : dir.startsWith('shared/')
      ? 'shared'
      : dir === 'server'
        ? 'server'
        : 'adapter';

/**
 * `source` without Rust comments, so a table named in prose is not counted as SQL. Strings, raw
 * strings and character literals are kept whole (a `//` inside an SQL string is not a comment),
 * block comments nest, and every removed character becomes a space so lines stay aligned.
 */
export function stripComments(source) {
  const n = source.length;
  let out = '';
  let i = 0;
  const isIdent = (ch) => ch !== undefined && /[A-Za-z0-9_]/.test(ch);
  while (i < n) {
    const c = source[i];
    const next = source[i + 1];
    if (c === '/' && next === '/') {
      while (i < n && source[i] !== '\n') {
        out += ' ';
        i++;
      }
      continue;
    }
    if (c === '/' && next === '*') {
      let depth = 1;
      out += '  ';
      i += 2;
      while (i < n && depth > 0) {
        if (source[i] === '/' && source[i + 1] === '*') {
          depth++;
          out += '  ';
          i += 2;
        } else if (source[i] === '*' && source[i + 1] === '/') {
          depth--;
          out += '  ';
          i += 2;
        } else {
          out += source[i] === '\n' ? '\n' : ' ';
          i++;
        }
      }
      continue;
    }
    if (!isIdent(source[i - 1])) {
      const raw = /^b?r(#*)"/.exec(source.slice(i, i + 40));
      if (raw) {
        const close = `"${raw[1]}`;
        const end = source.indexOf(close, i + raw[0].length);
        const stop = end < 0 ? n : end + close.length;
        out += source.slice(i, stop);
        i = stop;
        continue;
      }
    }
    if (c === '"' || (c === 'b' && next === '"' && !isIdent(source[i - 1]))) {
      let j = c === '"' ? i + 1 : i + 2;
      while (j < n && source[j] !== '"') j += source[j] === '\\' ? 2 : 1;
      out += source.slice(i, j + 1);
      i = j + 1;
      continue;
    }
    if (c === "'") {
      const literal = /^'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'/.exec(
        source.slice(i, i + 14),
      );
      if (literal) {
        out += literal[0];
        i += literal[0].length;
        continue;
      }
    }
    out += c;
    i++;
  }
  return out;
}

/** Table names left by the migrations' SQL texts, applied in the order given. */
export function migrationTables(sqlTexts) {
  const live = new Set();
  const statement =
    /\b(create\s+table\s+(?:if\s+not\s+exists\s+)?|drop\s+table\s+(?:if\s+exists\s+)?)(?:"?public"?\.)?"?(\w+)"?/gi;
  for (const sql of sqlTexts) {
    for (const m of sql.replace(/--.*$/gm, '').matchAll(statement)) {
      const name = m[2].toLowerCase();
      if (/^create/i.test(m[1])) live.add(name);
      else live.delete(name);
    }
  }
  return [...live].sort();
}

function findCycle(nodes, edges) {
  const state = new Map();
  const stack = [];
  const visit = (node) => {
    state.set(node, 1);
    stack.push(node);
    for (const next of edges.get(node) ?? []) {
      if (state.get(next) === 1) return [...stack.slice(stack.indexOf(next)), next];
      if (!state.has(next)) {
        const cycle = visit(next);
        if (cycle) return cycle;
      }
    }
    stack.pop();
    state.set(node, 2);
    return null;
  };
  for (const node of [...nodes].sort()) {
    if (!state.has(node)) {
      const cycle = visit(node);
      if (cycle) return cycle;
    }
  }
  return null;
}

/**
 * SQL references from `sources` to `tables`: each statement that names one is a read
 * (`FROM`, `JOIN`, `INTO`) or a write (`INSERT INTO`, `UPDATE`, `DELETE FROM`, `TRUNCATE`).
 * A schema qualifier and quotes around either part are accepted.
 */
export function sqlReferences(sources, tables) {
  if (tables.length === 0) return [];
  const names = [...tables].sort((a, b) => b.length - a.length).join('|');
  const statement = new RegExp(
    `\\b(insert\\s+into|update|delete\\s+from|truncate(?:\\s+table)?(?:\\s+only)?|from|join|into)\\s+(?:"?public"?\\.)?"?(${names})\\b`,
    'gi',
  );
  const found = [];
  for (const { text } of sources) {
    for (const m of stripComments(text).matchAll(statement)) {
      found.push({ table: m[2].toLowerCase(), write: /^(insert|update|delete|truncate)/i.test(m[1]) });
    }
  }
  return found;
}

/**
 * Cross-module SQL references per crate: `actual[crate][table]` counts SQL that names a table
 * another module owns, and `problems` lists what a read-model crate may not do. Shared tables
 * (the outbox) are used by every crate and are not counted, except that a read model may not
 * write them.
 */
export function crossModuleCounts({ packages, config, sources }) {
  const problems = [];
  const modules = packages.filter((p) => kindOf(p.dir) === 'module').map((p) => p.name);
  const owner = new Map(
    Object.entries(config.modules).flatMap(([name, spec]) => spec.tables.map((t) => [t, name])),
  );
  const shared = new Set(config.sharedTables ?? []);
  const known = [...owner.keys(), ...shared];
  const actual = {};
  for (const crate of [...modules, 'server']) {
    const own = new Set(config.modules[crate]?.tables ?? []);
    const readModel = config.readModels?.[crate];
    for (const ref of sqlReferences(sources[crate] ?? [], known)) {
      if (own.has(ref.table)) continue;
      if (readModel) {
        if (ref.write)
          problems.push(`${crate} is a read model and writes ${ref.table}; read models only read`);
        else if (!shared.has(ref.table) && !readModel.tables.includes(ref.table))
          problems.push(
            `${crate} reads ${ref.table}, which is not on its read-model list; add it to readModels.${crate}.tables with a reason, or read it through ${owner.get(ref.table)}`,
          );
        continue;
      }
      if (shared.has(ref.table)) continue;
      actual[crate] ??= {};
      actual[crate][ref.table] = (actual[crate][ref.table] ?? 0) + 1;
    }
  }
  return { actual, problems };
}

/** The module path of a file under `src/`: `domain/order.rs` is `domain::order`, `domain.rs` and `domain/mod.rs` are `domain`. */
function modulePathOf(filePath) {
  const parts = filePath.replace(/\.rs$/, '').split('/');
  if (parts[parts.length - 1] === 'mod') parts.pop();
  return parts;
}

/** The first identifier of each top-level item of a `{ ... }` group starting at `text[open]`. */
function groupHeads(text, open) {
  const heads = [];
  let depth = 0;
  let item = '';
  for (let i = open; i < text.length; i++) {
    const ch = text[i];
    if (ch === '{') {
      if (depth++ > 0) item += ch;
    } else if (ch === '}') {
      if (--depth === 0) break;
      item += ch;
    } else if (ch === ',' && depth === 1) {
      heads.push(item);
      item = '';
    } else item += ch;
  }
  heads.push(item);
  return heads.map((h) => /^\s*([A-Za-z_]\w*)/.exec(h)?.[1]).filter(Boolean);
}

/**
 * The crate-root layers a file's `crate::`, `self::` and `super::` paths reach, including paths
 * in a group (`use crate::{infrastructure::Repo, api}`). `super::` is resolved from the file's own
 * module, so `super::infrastructure` in `domain.rs` reaches the layer and the same text in
 * `domain/order.rs` names a sibling module instead.
 */
export function layersReached(filePath, text) {
  const here = modulePathOf(filePath);
  const reached = new Set();
  const path = /\b(crate|self|super)((?:\s*::\s*super)*)\s*::\s*(\{|[A-Za-z_]\w*)/g;
  for (const m of text.matchAll(path)) {
    const ups = m[1] === 'super' ? 1 + (m[2].match(/super/g)?.length ?? 0) : 0;
    const base = m[1] === 'crate' ? [] : here.slice(0, Math.max(0, here.length - ups));
    if (base.length > 0) continue;
    const heads =
      m[3] === '{' ? groupHeads(text, m.index + m[0].length - 1) : [m[3]];
    for (const head of heads) if (LAYERS.includes(head)) reached.add(head);
  }
  return reached;
}

const OUTER = {
  domain: ['application', 'infrastructure', 'api'],
  application: ['infrastructure', 'api'],
  infrastructure: ['api'],
};

function checkRegistry({ packages, config }) {
  const problems = [];
  const modules = packages.filter((p) => kindOf(p.dir) === 'module').map((p) => p.name);
  for (const name of modules)
    if (!(name in config.modules))
      problems.push(
        `module "${name}" is not registered in tools/module-boundaries.json (add it under "modules" with its dependsOn list and the tables it owns)`,
      );
  for (const name of Object.keys(config.modules))
    if (!modules.includes(name))
      problems.push(`"${name}" is registered in module-boundaries.json but is no module crate; remove it`);
  return problems;
}

/** Edge rules, and the exceptions they used. */
function checkEdges({ packages, config }) {
  const problems = [];
  const byName = new Map(packages.map((p) => [p.name, { ...p, kind: kindOf(p.dir) }]));
  const consumers = new Set(config.eventConsumers);
  const exceptions = config.exceptions ?? [];
  const used = new Set();
  for (const pkg of byName.values()) {
    for (const dep of pkg.deps) {
      const target = byName.get(dep.name);
      if (!target || dep.kind === 'dev') continue;
      if ((pkg.kind === 'shared' || pkg.kind === 'adapter') && (target.kind === 'module' || target.kind === 'server'))
        problems.push(
          `${pkg.name} (${pkg.kind}) depends on ${dep.name} (${target.kind}): shared and adapter crates must not depend on modules or the server`,
        );
      if (pkg.kind !== 'module') continue;
      if (target.kind === 'server')
        problems.push(`${pkg.name} depends on server: modules are wired together by the server only`);
      if (target.kind !== 'module') continue;
      const exception = exceptions.find(
        (e) => e.rule === 'event-consumer-called' && e.from === pkg.name && e.to === dep.name,
      );
      if (consumers.has(dep.name)) {
        if (exception) used.add(exception);
        else
          problems.push(
            `${pkg.name} depends on ${dep.name}, an event consumer: producers publish events and are never coupled to consumers (record a named exception with a reason if it cannot be avoided)`,
          );
      }
      if (!(config.modules[pkg.name]?.dependsOn ?? []).includes(dep.name))
        problems.push(
          `${pkg.name} -> ${dep.name} is a new module dependency: remove it, or add "${dep.name}" to modules.${pkg.name}.dependsOn in tools/module-boundaries.json and explain it in the PR`,
        );
    }
  }
  for (const [name, spec] of Object.entries(config.modules)) {
    const pkg = byName.get(name);
    if (!pkg) continue;
    const actual = new Set(pkg.deps.filter((d) => d.kind !== 'dev').map((d) => d.name));
    for (const dep of spec.dependsOn)
      if (!actual.has(dep) || byName.get(dep)?.kind !== 'module')
        problems.push(
          `modules.${name}.dependsOn lists "${dep}" but ${name} no longer depends on it; remove the entry`,
        );
  }
  for (const e of exceptions)
    if (e.rule === 'event-consumer-called' && !used.has(e))
      problems.push(`exception ${e.from} -> ${e.to} (${e.rule}) matches no dependency; remove it`);
  return problems;
}

function checkCycles({ packages }) {
  const edges = new Map(packages.map((p) => [p.name, new Set(p.deps.map((d) => d.name))]));
  const cycle = findCycle(edges.keys(), edges);
  return cycle ? [`dependency cycle: ${cycle.join(' -> ')}`] : [];
}

function checkLayers({ packages, config, layers, sources }) {
  const problems = [];
  const modules = packages.filter((p) => kindOf(p.dir) === 'module').map((p) => p.name);
  const flat = new Set(config.flatModules);
  for (const name of config.flatModules)
    if (!modules.includes(name))
      problems.push(`flatModules lists "${name}" but there is no such module crate; remove it`);
  for (const name of modules) {
    const have = layers[name] ?? {};
    const complete = LAYERS.every((l) => have[l]);
    if (flat.has(name) && complete)
      problems.push(`${name} has all four layers: remove it from flatModules in tools/module-boundaries.json`);
    if (!flat.has(name) && !complete) {
      const missing = LAYERS.filter((l) => !have[l]).map((l) => `${l}/`);
      problems.push(
        `${name} is not a flat module (not in flatModules) but src/ lacks ${missing.join(', ')}. New and changed modules use domain/, application/, infrastructure/ and api/ (ADR-017)`,
      );
    }
    for (const file of sources[name] ?? []) {
      const layer = LAYERS.find((l) => file.path === `${l}.rs` || file.path.startsWith(`${l}/`));
      if (!layer || layer === 'api') continue;
      const text = stripComments(file.text);
      const at = (what) => `${name}/src/${file.path}: ${layer} ${what}`;
      if (layer === 'domain')
        for (const crate of DOMAIN_FORBIDDEN_CRATES)
          if (new RegExp(`\\b${crate}::`).test(text))
            problems.push(at(`uses ${crate}; domain code names no framework or vendor SDK (Principle IV)`));
      const reached = layersReached(file.path, text);
      for (const outer of OUTER[layer])
        if (reached.has(outer))
          problems.push(at(`imports crate::${outer}; dependencies point inward (api -> application -> domain)`));
      if (layer === 'application' && /\baxum::/.test(text))
        problems.push(at('uses axum; HTTP types belong in api/'));
    }
  }
  return problems;
}

function checkTables({ packages, config, sources, tables }) {
  const problems = [];
  const owner = new Map();
  for (const [name, spec] of Object.entries(config.modules))
    for (const t of spec.tables) {
      if (owner.has(t)) problems.push(`table ${t} is owned by both ${owner.get(t)} and ${name}`);
      owner.set(t, name);
    }
  const shared = new Set(config.sharedTables);
  for (const t of tables) {
    if (shared.has(t) && owner.has(t)) problems.push(`table ${t} is both shared and owned by ${owner.get(t)}`);
    if (!owner.has(t) && !shared.has(t))
      problems.push(`table ${t} (created by a migration) has no owner: add it to modules.<module>.tables or sharedTables`);
  }
  for (const t of [...owner.keys(), ...shared])
    if (!tables.includes(t)) problems.push(`table ${t} is in module-boundaries.json but no migration creates it; remove it`);

  const counted = crossModuleCounts({ packages, config, sources });
  problems.push(...counted.problems);
  const baseline = config.crossModuleSql ?? {};
  for (const crate of new Set([...Object.keys(counted.actual), ...Object.keys(baseline)])) {
    const tablesHere = new Set([...Object.keys(counted.actual[crate] ?? {}), ...Object.keys(baseline[crate] ?? {})]);
    for (const t of tablesHere) {
      const now = counted.actual[crate]?.[t] ?? 0;
      const was = baseline[crate]?.[t] ?? 0;
      if (now > was)
        problems.push(
          `${crate} names ${t} (owned by ${owner.get(t)}) ${now} times in SQL, up from ${was}: use ${owner.get(t)}'s public interface instead of a new cross-module query`,
        );
      if (now < was)
        problems.push(
          `${crate} names ${t} ${now} times in SQL, down from ${was}: lower crossModuleSql.${crate}.${t} to ${now} in tools/module-boundaries.json to keep the gain`,
        );
    }
  }
  return problems;
}

/**
 * Problems found in `input`:
 *   packages:   [{ name, dir, deps: [{ name, kind }] }]  (`dir` relative to backend/crates)
 *   config:     module-boundaries.json
 *   layers:     { [crate]: { domain, application, infrastructure, api } } (booleans)
 *   sources:    { [crate]: [{ path, text }] }  (path relative to the crate's src/)
 *   tables:     tables created by the migrations
 */
export function analyze(input) {
  return [
    ...checkRegistry(input),
    ...checkEdges(input),
    ...checkCycles(input),
    ...checkLayers(input),
    ...checkTables(input),
  ];
}

/** The generated architecture document. */
export function renderDoc({ packages, config, layers }) {
  const modules = packages.filter((p) => kindOf(p.dir) === 'module').map((p) => p.name).sort();
  const usedBy = Object.fromEntries(modules.map((m) => [m, []]));
  for (const m of modules)
    for (const d of config.modules[m]?.dependsOn ?? []) usedBy[d]?.push(m);
  const flat = new Set(config.flatModules);
  const list = (xs) => (xs.length ? xs.map((x) => `\`${x}\``).join(', ') : '–');
  const lines = [];
  lines.push('# Module dependencies');
  lines.push('');
  lines.push(
    '<!-- Generated by backend/tools/check-module-boundaries.mjs --write-doc from `cargo metadata` and',
    '     backend/tools/module-boundaries.json. Do not edit by hand: CI fails when this file is stale. -->',
    '',
  );
  lines.push(
    'The module crates of the backend, what each depends on, and the tables each owns. It documents',
    'the rules of [ADR-001](../adr/0001-modular-monolith-hexagonal-modules.md) as amended by',
    '[ADR-017](../adr/0017-module-layering-and-ci-checked-boundaries.md), as they hold in the code',
    'today, and CI checks them (`backend/tools/check-module-boundaries.mjs`,',
    'the `module-boundaries` step of the Backend workflow). To change a rule or accept an edge, edit',
    '`backend/tools/module-boundaries.json` in the same PR and say why.',
    '',
  );
  lines.push('## Module crates', '');
  lines.push('| Module | Layout | Depends on (modules) | Used by (modules) | Tables owned |', '| -- | -- | -- | -- | -- |');
  for (const m of modules) {
    const spec = config.modules[m] ?? { dependsOn: [], tables: [] };
    const layout = flat.has(m) ? 'flat' : LAYERS.every((l) => layers[m]?.[l]) ? 'layered' : 'incomplete';
    lines.push(
      `| \`${m}\` | ${layout} | ${list([...spec.dependsOn].sort())} | ${list(usedBy[m].sort())} | ${list([...spec.tables].sort())} |`,
    );
  }
  lines.push('', `Shared tables, owned by no module: ${list([...config.sharedTables].sort())}.`, '');
  lines.push('## Graph', '', '```mermaid', 'flowchart LR');
  for (const m of modules)
    for (const d of [...(config.modules[m]?.dependsOn ?? [])].sort()) lines.push(`  ${m} --> ${d}`);
  lines.push('```', '');
  lines.push('## Rules the check enforces', '');
  lines.push(
    '- A module depends on another module only through an edge listed above. A new edge fails CI until it is added on purpose; an edge that disappears must be removed from the list.',
    '- Shared and adapter crates never depend on a module or on `server`; no module depends on `server`.',
    `- No module calls an event consumer (${list([...config.eventConsumers].sort())}); the exceptions are listed below.`,
    '- The crate graph has no cycle.',
    '- A module that is not flat has `domain/`, `application/`, `infrastructure/` and `api/` under `src/`, and its layers import inward only. `domain/` names no framework or vendor SDK.',
    '- Every table created by a migration has one owner. SQL that names another module\'s table may not grow (counts below). A read-model crate may read the tables on its list and writes nothing.',
    '',
  );
  lines.push('## Accepted exceptions', '');
  if ((config.exceptions ?? []).length) {
    lines.push('| Rule | From | To | Reason |', '| -- | -- | -- | -- |');
    for (const e of config.exceptions) lines.push(`| ${e.rule} | \`${e.from}\` | \`${e.to}\` | ${e.reason} |`);
    lines.push('');
  } else lines.push('None.', '');
  const readModels = Object.entries(config.readModels ?? {});
  if (readModels.length) {
    lines.push('Read models (may read other modules\' tables, never write):', '');
    for (const [crate, spec] of readModels)
      lines.push(`- \`${crate}\` reads ${list([...spec.tables].sort())}. ${spec.reason}`);
    lines.push('');
  }
  lines.push('## Cross-module SQL still to remove', '');
  lines.push(
    'Counts of SQL references to a table another module owns, per crate. They may only go down.',
    '',
    '| Crate | Table | Owner | References |',
    '| -- | -- | -- | -- |',
  );
  const owner = new Map(Object.entries(config.modules).flatMap(([n, s]) => s.tables.map((t) => [t, n])));
  let total = 0;
  for (const crate of Object.keys(config.crossModuleSql ?? {}).sort())
    for (const [t, n] of Object.entries(config.crossModuleSql[crate]).sort()) {
      lines.push(`| \`${crate}\` | \`${t}\` | \`${owner.get(t)}\` | ${n} |`);
      total += n;
    }
  lines.push('', `Total: ${total}.`, '');
  return lines.join('\n');
}

// ── CLI ───────────────────────────────────────────────────────────────────────────────────────

function walk(dir, ext) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    return statSync(full).isDirectory() ? walk(full, ext) : full.endsWith(ext) ? [full] : [];
  });
}

export function load() {
  const meta = JSON.parse(
    execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], {
      cwd: backendDir,
      maxBuffer: 256 * 1024 * 1024,
    }).toString(),
  );
  const crates = join(backendDir, 'crates');
  const packages = meta.packages
    .filter((p) => p.manifest_path.startsWith(crates))
    .map((p) => ({
      name: p.name,
      dir: relative(crates, dirname(p.manifest_path)),
      deps: p.dependencies.map((d) => ({ name: d.name, kind: d.kind ?? 'normal' })),
    }));
  const config = JSON.parse(readFileSync(join(here, 'module-boundaries.json'), 'utf8'));
  const layers = {};
  const sources = {};
  for (const p of packages) {
    const src = join(crates, p.dir, 'src');
    layers[p.name] = Object.fromEntries(
      LAYERS.map((l) => [l, existsSync(join(src, l)) || existsSync(join(src, `${l}.rs`))]),
    );
    sources[p.name] = walk(src, '.rs').map((f) => ({
      path: relative(src, f).split('\\').join('/'),
      text: readFileSync(f, 'utf8'),
    }));
  }
  const tables = migrationTables(walk(join(backendDir, 'migrations'), '.sql').sort().map((f) => readFileSync(f, 'utf8')));
  return { packages, config, layers, sources, tables };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const input = load();
  const docFile = join(repoRoot, DOC_PATH);
  const doc = renderDoc(input);
  if (process.argv.includes('--write-doc')) {
    writeFileSync(docFile, doc);
    console.log(`wrote ${DOC_PATH}`);
    process.exit(0);
  }
  if (process.argv.includes('--print-sql-baseline')) {
    // The `crossModuleSql` object the config should hold, for updating it after a refactor.
    console.log(JSON.stringify(crossModuleCounts(input).actual, null, 2));
    process.exit(0);
  }
  const problems = analyze(input);
  if (!existsSync(docFile) || readFileSync(docFile, 'utf8') !== doc)
    problems.push(`${DOC_PATH} is out of date: run \`node backend/tools/check-module-boundaries.mjs --write-doc\` and commit it`);
  if (problems.length) {
    console.error(`Module boundary check failed (${problems.length}):`);
    for (const p of problems) console.error(`  - ${p}`);
    process.exit(1);
  }
  console.log('Module boundaries OK.');
}
