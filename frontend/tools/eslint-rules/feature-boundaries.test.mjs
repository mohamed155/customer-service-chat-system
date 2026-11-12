import { describe, it } from 'node:test';
import { RuleTester } from 'eslint';
import { featureBoundaries, featureOf } from './feature-boundaries.mjs';
import assert from 'node:assert/strict';

RuleTester.describe = describe;
RuleTester.it = it;

const app = '/work/frontend/apps/dashboard/src/app';
const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 2022, sourceType: 'module' },
});

ruleTester.run('feature-boundaries', featureBoundaries, {
  valid: [
    // a feature's own files, core and shared
    {
      filename: `${app}/features/tenant/team/team.store.ts`,
      code: "import { a } from './team-api.service'; import { b } from '../../../core/api/api.service'; import { c } from '../../../shared/components/button/button.component';",
    },
    // the area route file wires up its own area's features, lazily or not
    {
      filename: `${app}/features/tenant/tenant.routes.ts`,
      code: "export const r = [{ loadComponent: () => import('./team/team-list.component') }]; import { x } from './customers/customers.component';",
    },
    {
      filename: `${app}/features/platform/platform.routes.ts`,
      code: "import { x } from './tenants/tenant-list.component';",
    },
    // non-relative imports are not this rule's business
    {
      filename: `${app}/features/auth/login/login.component.ts`,
      code: "import { x } from '@angular/core'; import { y } from 'rxjs';",
    },
    // code outside features/ may import features (the app routes)
    {
      filename: `${app}/app.routes.ts`,
      code: "export const r = [{ loadChildren: () => import('./features/tenant/tenant.routes') }];",
    },
    // shared may import models and pure helpers
    {
      filename: `${app}/shared/components/table/table.component.ts`,
      code: "import { m } from '../../../core/api/tenant-api.models'; import { u } from '../../utils/relative-time';",
    },
    // a spec of a presentational component may use a store
    {
      filename: `${app}/shared/components/table/table.component.spec.ts`,
      code: "import { s } from '../../../core/notifications/notifications.store';",
    },
    // outside the dashboard app: not checked
    { filename: '/work/frontend/apps/widget/src/main.ts', code: "import { x } from '../../a/b';" },
  ],
  invalid: [
    {
      filename: `${app}/features/tenant/team/team-list.component.ts`,
      code: "import { x } from '../conversations/conversations.store';",
      errors: [
        { messageId: 'crossFeature', data: { from: 'tenant/team', to: 'tenant/conversations' } },
      ],
    },
    {
      filename: `${app}/features/auth/invite/accept.component.ts`,
      code: "import { x } from '../../tenant/team/team-api.service';",
      errors: [{ messageId: 'crossFeature', data: { from: 'auth', to: 'tenant/team' } }],
    },
    {
      filename: `${app}/features/platform/billing/b.component.ts`,
      code: "export { x } from '../../tenant/billing/billing.store';",
      errors: [{ messageId: 'crossFeature' }],
    },
    {
      filename: `${app}/features/tenant/team/lazy.ts`,
      code: "const m = () => import('../customers/customers.component');",
      errors: [{ messageId: 'crossFeature' }],
    },
    {
      // a feature reaching for a file that sits in the area folder
      filename: `${app}/features/tenant/overview/overview.component.ts`,
      code: "import { x } from '../shared-thing';",
      errors: [{ messageId: 'crossFeature', data: { from: 'tenant/overview', to: 'tenant' } }],
    },
    {
      // specs are held to the same rule
      filename: `${app}/features/tenant/team/team.store.spec.ts`,
      code: "import { x } from '../customers/customers.store';",
      errors: [{ messageId: 'crossFeature' }],
    },
    {
      // the route file of one area may not reach into another area
      filename: `${app}/features/tenant/tenant.routes.ts`,
      code: "import { x } from '../platform/tenants/tenant-list.component';",
      errors: [{ messageId: 'areaRoute' }],
    },
    {
      filename: `${app}/shared/components/table/table.component.ts`,
      code: "import { s } from '../../../core/notifications/notifications.store';",
      errors: [{ messageId: 'presentational' }],
    },
    {
      filename: `${app}/shared/components/table/table.component.ts`,
      code: "import { s } from '../../../core/api/customers-api.service';",
      errors: [{ messageId: 'presentational' }],
    },
    {
      filename: `${app}/shared/components/table/table.component.ts`,
      code: "import { s } from '../../../core/api/api.service';",
      errors: [{ messageId: 'presentational' }],
    },
    {
      filename: `${app}/core/ui/toast/toast.component.ts`,
      code: "import { s } from '../../state/tenant-scope';",
      errors: [{ messageId: 'presentational' }],
    },
  ],
});

describe('featureOf', () => {
  it('names the feature of a path', () => {
    assert.equal(featureOf(['features', 'tenant', 'team', 'a.ts']), 'tenant/team');
    assert.equal(featureOf(['features', 'tenant', 'tenant.routes.ts']), 'tenant/*');
    assert.equal(featureOf(['features', 'platform', 'billing', 'x', 'y.ts']), 'platform/billing');
    assert.equal(featureOf(['features', 'auth', 'login', 'l.ts']), 'auth');
    assert.equal(featureOf(['core', 'api', 'api.service.ts']), null);
    assert.equal(featureOf(null), null);
  });
});
