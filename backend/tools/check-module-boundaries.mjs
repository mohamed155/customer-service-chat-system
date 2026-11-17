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

/** `source` without Rust comments, so a table named in prose is not counted as SQL. */
export function stripComments(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .split('\n')
    .map((line) => line.replace(/(^|[^:"'\\])\/\/.*$/, '$1'))
    .join('\n');
}

/** Table names created (and not dropped again) by the migrations' SQL texts. */
export function migrationTables(sqlTexts) {
  const created = new Set();
  const dropped = new Set();
  for (const sql of sqlTexts) {
    const text = sql.replace(/--.*$/gm, '');
    for (const m of text.matchAll(/create\s+table\s+(?:if\s+not\s+exists\s+)?(?:public\.)?"?(\w+)"?/gi))
      created.add(m[1].toLowerCase());
    for (const m of text.matchAll(/drop\s+table\s+(?:if\s+exists\s+)?(?:public\.)?"?(\w+)"?/gi))
      dropped.add(m[1].toLowerCase());
  }
  return [...created].filter((t) => !dropped.has(t)).sort();
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

/** Count SQL references from `sources` to `tables`, split into reads and writes. */
export function sqlReferences(sources, tables) {
  if (tables.length === 0) return [];
  const names = [...tables].sort((a, b) => b.length - a.length).join('|');
  const read = new RegExp(`\\b(from|join|update|into)\\s+"?(?:public\\.)?(${names})\\b`, 'gi');
  const write = new RegExp(
    `\\b(insert\\s+into|update|delete\\s+from)\\s+"?(?:public\\.)?(${names})\\b`,
    'gi',
  );
  const found = [];
  for (const { text } of sources) {
    const clean = stripComments(text);
    for (const m of clean.matchAll(read)) found.push({ table: m[2].toLowerCase(), write: false });
    for (const m of clean.matchAll(write)) found.push({ table: m[2].toLowerCase(), write: true });
  }
  return found;
}

/**
 * Cross-module SQL references per crate: `actual[crate][table]` counts SQL that names a table
 * another module owns, and `problems` lists what a read-model crate may not do.
 */
export function crossModuleCounts({ packages, config, sources }) {
  const problems = [];
  const modules = packages.filter((p) => kindOf(p.dir) === 'module').map((p) => p.name);
  const owner = new Map(
    Object.entries(config.modules).flatMap(([name, spec]) => spec.tables.map((t) => [t, name])),
  );
  const known = [...owner.keys()];
  const actual = {};
  for (const crate of [...modules, 'server']) {
    const own = new Set(config.modules[crate]?.tables ?? []);
    const readModel = config.readModels?.[crate];
    for (const ref of sqlReferences(sources[crate] ?? [], known)) {
      if (own.has(ref.table)) continue;
      if (readModel) {
        if (ref.write)
          problems.push(`${crate} is a read model and writes ${ref.table}; read models only read`);
        else if (!readModel.tables.includes(ref.table))
          problems.push(
            `${crate} reads ${ref.table}, which is not on its read-model list; add it to readModels.${crate}.tables with a reason, or read it through ${owner.get(ref.table)}`,
          );
        continue;
      }
      actual[crate] ??= {};
      actual[crate][ref.table] = (actual[crate][ref.table] ?? 0) + 1;
    }
  }
  return { actual, problems };
}

/**
 * Problems found in `input`:
 *   packages:   [{ name, dir, deps: [{ name, kind }] }]  (`dir` relative to backend/crates)
 *   config:     module-boundaries.json
 *   layers:     { [crate]: { domain, application, infrastructure, api } } (booleans)
 *   sources:    { [crate]: [{ path, text }] }  (path relative to the crate's src/)
 *   tables:     tables created by the migrations
 */
export function analyze({ packages, config, layers, sources, tables }) {
  const problems = [];
  const byName = new Map(packages.map((p) => [p.name, { ...p, kind: kindOf(p.dir) }]));
  const modules = packages.filter((p) => kindOf(p.dir) === 'module').map((p) => p.name);
  const consumers = new Set(config.eventConsumers);
  const exceptions = config.exceptions ?? [];

  // 1. Registry.
  for (const name of modules)
    if (!(name in config.modules))
      problems.push(
        `module "${name}" is not registered in tools/module-boundaries.json (add it under "modules" with its dependsOn list and the tables it owns)`,
      );
  for (const name of Object.keys(config.modules))
    if (!modules.includes(name))
      problems.push(`"${name}" is registered in module-boundaries.json but is no module crate; remove it`);

  // 2 and 1b. Edges.
  const usedExceptions = new Set();
  const edges = new Map();
  for (const pkg of byName.values()) {
    const set = (edges.get(pkg.name) ?? new Set());
    edges.set(pkg.name, set);
    for (const dep of pkg.deps) {
      const target = byName.get(dep.name);
      if (!target) continue;
      set.add(dep.name);
      if (dep.kind === 'dev') continue;
      if ((pkg.kind === 'shared' || pkg.kind === 'adapter') && (target.kind === 'module' || target.kind === 'server'))
        problems.push(
          `${pkg.name} (${pkg.kind}) depends on ${dep.name} (${target.kind}): shared and adapter crates must not depend on modules or the server`,
        );
      if (pkg.kind === 'module' && target.kind === 'server')
        problems.push(`${pkg.name} depends on server: modules are wired together by the server only`);
      if (pkg.kind === 'module' && target.kind === 'module') {
        const allowed = config.modules[pkg.name]?.dependsOn ?? [];
        const exception = exceptions.find(
          (e) => e.rule === 'event-consumer-called' && e.from === pkg.name && e.to === dep.name,
        );
        if (consumers.has(dep.name)) {
          if (exception) usedExceptions.add(exception);
          else
            problems.push(
              `${pkg.name} depends on ${dep.name}, an event consumer: producers publish events and are never coupled to consumers (record a named exception with a reason if it cannot be avoided)`,
            );
        }
        if (!allowed.includes(dep.name))
          problems.push(
            `${pkg.name} -> ${dep.name} is a new module dependency: remove it, or add "${dep.name}" to modules.${pkg.name}.dependsOn in tools/module-boundaries.json and explain it in the PR`,
          );
      }
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
    if (e.rule === 'event-consumer-called' && !usedExceptions.has(e))
      problems.push(`exception ${e.from} -> ${e.to} (${e.rule}) matches no dependency; remove it`);

  // 3. Cycles.
  const cycle = findCycle(byName.keys(), edges);
  if (cycle) problems.push(`dependency cycle: ${cycle.join(' -> ')}`);

  // 4. Layering.
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
        `${name} is not a flat module (not in flatModules) but src/ lacks ${missing.join(', ')}. New and changed modules use domain/, application/, infrastructure/ and api/ (ADR-001 amendment)`,
      );
    }
    for (const file of sources[name] ?? []) {
      const layer = LAYERS.find((l) => file.path === `${l}.rs` || file.path.startsWith(`${l}/`));
      if (!layer || layer === 'api') continue;
      const text = stripComments(file.text);
      const at = (what) => `${name}/src/${file.path}: ${layer} ${what}`;
      if (layer === 'domain') {
        for (const crate of DOMAIN_FORBIDDEN_CRATES)
          if (new RegExp(`\\b${crate}::`).test(text))
            problems.push(at(`uses ${crate}; domain code names no framework or vendor SDK (Principle IV)`));
      }
      const outer = { domain: ['application', 'infrastructure', 'api'], application: ['infrastructure', 'api'], infrastructure: ['api'] }[layer];
      for (const o of outer)
        if (new RegExp(`\\bcrate::${o}\\b`).test(text))
          problems.push(at(`imports crate::${o}; dependencies point inward (api -> application -> domain)`));
      if (layer === 'application' && /\baxum::/.test(text))
        problems.push(at('uses axum; HTTP types belong in api/'));
    }
  }

  // 5. Table ownership.
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
  const actual = counted.actual;
  const baseline = config.crossModuleSql ?? {};
  for (const crate of new Set([...Object.keys(actual), ...Object.keys(baseline)])) {
    for (const t of new Set([...Object.keys(actual[crate] ?? {}), ...Object.keys(baseline[crate] ?? {})])) {
      const now = actual[crate]?.[t] ?? 0;
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
    'the rules of [ADR-001](../adr/0001-modular-monolith-hexagonal-modules.md) (amended by MOH-79) as',
    'they hold in the code today, and CI checks them (`backend/tools/check-module-boundaries.mjs`,',
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
  const tables = migrationTables(walk(join(backendDir, 'migrations'), '.sql').map((f) => readFileSync(f, 'utf8')));
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
