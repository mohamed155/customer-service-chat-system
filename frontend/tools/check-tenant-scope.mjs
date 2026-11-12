// Checks that every store holding workspace data resets when the workspace changes (MOH-61,
// architecture section 17).
//
// A store under `features/tenant/` or provided in root (so it outlives a page) must use
// `withTenantScope()` from `core/state/tenant-scope`. That one feature resets the store when the
// active workspace changes and reloads it for the new one, so a store added later cannot forget
// its own reset. A store that holds no workspace data is listed in EXEMPT with the reason.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const appDir = 'apps/dashboard/src/app';

/** Stores that hold no workspace data, with the reason. */
export const EXEMPT = {
  [`${appDir}/features/tenant/settings/settings.store.ts`]:
    'holds only which settings tab is open (UI state), no workspace data',
  [`${appDir}/features/platform/tenants/tenants.store.ts`]:
    'the platform operator tenant directory: it lists all workspaces, not the active one',
};

const isTenantArea = (file) => file.includes(`${appDir}/features/tenant/`);
const isRootProvided = (source) => /providedIn:\s*['"]root['"]/.test(source);

/** Problems in `files` (`[{ path, source }]`, paths relative to the frontend root). */
export function check(files) {
  const problems = [];
  const stores = files.filter((f) => f.path.endsWith('.store.ts') && !f.path.endsWith('.spec.ts'));
  for (const { path, source } of stores) {
    const needsScope = isTenantArea(path) || isRootProvided(source);
    const hasScope = source.includes('withTenantScope(');
    if (path in EXEMPT) {
      if (hasScope)
        problems.push(
          `${path}: is exempt (${EXEMPT[path]}) but uses withTenantScope(); remove it from EXEMPT`,
        );
      continue;
    }
    if (needsScope && !hasScope) {
      problems.push(
        `${path}: holds workspace data but does not use withTenantScope() ` +
          `(core/state/tenant-scope). Add it as the last feature, or list the store in ` +
          `tools/check-tenant-scope.mjs EXEMPT with the reason.`,
      );
    }
  }
  const seen = new Set(stores.map((f) => f.path));
  for (const path of Object.keys(EXEMPT)) {
    if (!seen.has(path))
      problems.push(`${path}: is in EXEMPT but no such store exists; remove the entry`);
  }
  return problems;
}

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules') continue;
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
  console.log('tenant scope: every workspace store resets on a workspace change');
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main());
}
