// Module boundaries for the dashboard (MOH-62, architecture section 4.2).
//
// 1. A feature may not import another feature. `features/tenant/team` and
//    `features/tenant/conversations` are two features; so are `features/auth` and
//    `features/tenant/*`. Code two features need belongs in `shared/` or `core/`.
//    The route file of an area (`features/tenant/tenant.routes.ts`) is the one place that wires
//    the area's features together, so it may import its own area's features and nothing else.
// 2. Presentational code (`shared/` and `core/ui/`) may not import a store, an API service or a
//    facade: it gets its data through inputs and reports through outputs.
//
// The rule resolves relative imports to real paths, so `../customers/x` is seen for what it is.
// (`core/` and `shared/` importing `features/` stays in `no-restricted-imports`.)
import { dirname, resolve, sep } from 'node:path';

const APP = ['apps', 'dashboard', 'src', 'app'].join('/');

/** Path segments below `src/app`, or `null` for a file outside it. */
function appSegments(file) {
  const normal = file.split(sep).join('/');
  const at = normal.indexOf(`${APP}/`);
  if (at < 0) return null;
  return normal.slice(at + APP.length + 1).split('/');
}

/**
 * The feature a file belongs to: `tenant/team`, `platform/billing`, `auth`, ... A file that sits
 * directly in an area folder (`features/tenant/tenant.routes.ts`) belongs to `tenant/*` (the area
 * itself). `null` for anything outside `features/`.
 */
export function featureOf(segments) {
  if (segments?.[0] !== 'features') return null;
  const [, first, second] = segments;
  if (first === 'tenant' || first === 'platform') {
    return segments.length > 3 ? `${first}/${second}` : `${first}/*`;
  }
  return first ?? null;
}

const STORE_LIKE = /(\.store|-api\.service|\.facade)(\.ts)?$/;

function isStoreLike(segments) {
  const joined = segments.join('/');
  return (
    STORE_LIKE.test(joined) || joined === 'core/api/api.service' || joined.startsWith('core/state/')
  );
}

const isPresentational = (segments) =>
  segments?.[0] === 'shared' || (segments?.[0] === 'core' && segments?.[1] === 'ui');

/** @type {import('eslint').Rule.RuleModule} */
export const featureBoundaries = {
  meta: {
    type: 'problem',
    schema: [],
    messages: {
      crossFeature:
        '{{from}} imports {{to}}. A feature may not import another feature: move what both need to shared/ or core/.',
      areaRoute:
        '{{from}} (the area route file) may only import its own area’s features, not {{to}}.',
      presentational:
        'Presentational code ({{from}}) may not import a store, API service or facade ({{to}}). Pass data in through inputs and report through outputs.',
    },
  },
  create(context) {
    const filename = context.filename ?? context.getFilename();
    const here = appSegments(filename);
    if (!here) return {};
    const fromFeature = featureOf(here);
    const isSpec = filename.endsWith('.spec.ts');

    function check(node, source) {
      if (typeof source !== 'string' || !source.startsWith('.')) return;
      const target = appSegments(resolve(dirname(filename), source));
      if (!target) return;

      if (fromFeature !== null) {
        const toFeature = featureOf(target);
        if (toFeature !== null && toFeature !== fromFeature) {
          const areaOfTarget = toFeature.split('/')[0];
          const routeFileOfSameArea =
            fromFeature.endsWith('/*') && toFeature.startsWith(`${fromFeature.split('/')[0]}/`);
          if (!routeFileOfSameArea) {
            context.report({
              node,
              messageId: fromFeature.endsWith('/*') ? 'areaRoute' : 'crossFeature',
              data: {
                from: fromFeature,
                to: toFeature === `${areaOfTarget}/*` ? areaOfTarget : toFeature,
              },
            });
          }
        }
      }

      if (!isSpec && isPresentational(here) && isStoreLike(target)) {
        context.report({
          node,
          messageId: 'presentational',
          data: { from: here.slice(0, 2).join('/'), to: target.join('/') },
        });
      }
    }

    return {
      ImportDeclaration: (node) => check(node, node.source.value),
      ExportAllDeclaration: (node) => check(node, node.source.value),
      ExportNamedDeclaration: (node) => node.source && check(node, node.source.value),
      ImportExpression: (node) =>
        node.source.type === 'Literal' ? check(node, node.source.value) : undefined,
    };
  },
};

export default { rules: { 'feature-boundaries': featureBoundaries } };
