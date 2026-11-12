// Fails when unit-test coverage drops below the recorded baseline (MOH-66, constitution Definition
// of Done: at least 80% of new domain logic).
//
// `pnpm test:coverage` runs the dashboard and widget unit tests with coverage (the `coverage`
// configuration of the `test` targets writes `coverage/<project>/coverage-summary.json`), then this
// script compares the numbers with `tools/coverage-baseline.json`.
//
// It is a ratchet, like the i18n check: the baseline is the lowest number allowed and it only goes
// up. `pnpm test:coverage --update` records the current numbers, never lowering one. The dashboard
// also tracks `core/` and the stores on their own, because that is where the Definition of Done is
// strictest; the target for both is 80% of lines.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const baselineFile = join(root, 'tools', 'coverage-baseline.json');

export const TARGET = 80;
/** A run may differ from the baseline by this many points before it counts as a drop. */
export const TOLERANCE = 0.1;
const METRICS = ['lines', 'statements', 'functions', 'branches'];

/** Per project: the path fragment that selects each group of files. */
export const GROUPS = {
  dashboard: {
    core: (path) => path.includes('/apps/dashboard/src/app/core/'),
    stores: (path) => path.endsWith('.store.ts'),
  },
  widget: {},
};

const percent = (covered, total) => (total === 0 ? 100 : (covered / total) * 100);
const floor1 = (value) => Math.floor(value * 10) / 10;

/**
 * Totals for the whole project and for each group, from a `coverage-summary.json`.
 * `{ total: { lines, ... }, groups: { core: { lines }, ... } }`, as percentages.
 */
export function summarize(summary, groups = {}) {
  const sums = {};
  const add = (name, entry, metrics) => {
    const into = (sums[name] ??= {});
    for (const metric of metrics) {
      const m = (into[metric] ??= { covered: 0, total: 0 });
      m.covered += entry[metric].covered;
      m.total += entry[metric].total;
    }
  };
  for (const [path, entry] of Object.entries(summary)) {
    if (path === 'total') continue;
    add('total', entry, METRICS);
    for (const [name, inGroup] of Object.entries(groups)) {
      if (inGroup(path.split('\\').join('/'))) add(name, entry, ['lines']);
    }
  }
  const result = { total: {}, groups: {} };
  for (const [name, metrics] of Object.entries(sums)) {
    const out = Object.fromEntries(
      Object.entries(metrics).map(([k, v]) => [k, percent(v.covered, v.total)]),
    );
    if (name === 'total') result.total = out;
    else result.groups[name] = out;
  }
  for (const name of Object.keys(groups)) result.groups[name] ??= { lines: 100 };
  return result;
}

/** `[{ scope, metric, baseline, current }]` for every number that fell below its baseline. */
export function drops(baseline, current) {
  const found = [];
  const check = (scope, from, to) => {
    for (const [metric, floor] of Object.entries(from ?? {})) {
      const now = to?.[metric];
      if (now === undefined || now + TOLERANCE < floor) {
        found.push({ scope, metric, baseline: floor, current: now });
      }
    }
  };
  check('total', baseline.total, current.total);
  for (const [name, values] of Object.entries(baseline.groups ?? {})) {
    check(name, values, current.groups?.[name]);
  }
  return found;
}

/** The baseline with every number raised to the current one, never lowered. */
export function raised(baseline, current) {
  const up = (from = {}, to = {}) =>
    Object.fromEntries(
      Object.entries(to).map(([metric, value]) => [
        metric,
        Math.max(from[metric] ?? 0, floor1(value)),
      ]),
    );
  const groups = {};
  for (const [name, values] of Object.entries(current.groups)) {
    groups[name] = up(baseline.groups?.[name], values);
  }
  return { total: up(baseline.total, current.total), groups };
}

function readSummary(project) {
  const file = join(root, 'coverage', project, 'coverage-summary.json');
  if (!existsSync(file)) {
    throw new Error(`${file} is missing: run the ${project} tests with --configuration=coverage`);
  }
  return JSON.parse(readFileSync(file, 'utf8'));
}

function main() {
  const update = process.argv.includes('--update');
  const baseline = existsSync(baselineFile) ? JSON.parse(readFileSync(baselineFile, 'utf8')) : {};
  const next = { ...baseline };
  let failed = false;

  for (const project of Object.keys(GROUPS)) {
    const current = summarize(readSummary(project), GROUPS[project]);
    const line = (scope, values) =>
      `  ${scope.padEnd(8)} ` +
      Object.entries(values)
        .map(([k, v]) => `${k} ${v.toFixed(1)}%`)
        .join('  ');
    console.log(`${project}:`);
    console.log(line('total', current.total));
    for (const [name, values] of Object.entries(current.groups)) console.log(line(name, values));

    if (update) {
      next[project] = raised(baseline[project] ?? {}, current);
      continue;
    }
    if (!baseline[project]) {
      console.error(`${project}: no baseline. Run: pnpm test:coverage --update`);
      failed = true;
      continue;
    }
    for (const d of drops(baseline[project], current)) {
      failed = true;
      console.error(
        `${project} ${d.scope} ${d.metric}: ${d.current?.toFixed(1) ?? 'missing'}% is below the baseline ${d.baseline}%. ` +
          'Add tests for the code you changed.',
      );
    }
    const lines = baseline[project].groups ?? {};
    for (const [name, values] of Object.entries(lines)) {
      if (values.lines < TARGET) {
        console.log(
          `  ${name}: ${(TARGET - values.lines).toFixed(1)} points to the ${TARGET}% target`,
        );
      }
    }
  }

  if (update) {
    writeFileSync(baselineFile, JSON.stringify(next, null, 2) + '\n');
    console.log('coverage baseline written');
    return 0;
  }
  if (!failed) console.log('coverage: no drop below the baseline');
  return failed ? 1 : 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(main());
}
