import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  analyze,
  crossModuleCounts,
  layersReached,
  migrationTables,
  renderDoc,
  sqlReferences,
  stripComments,
} from './check-module-boundaries.mjs';

const pkg = (name, dir, deps = []) => ({
  name,
  dir,
  deps: deps.map((d) => (typeof d === 'string' ? { name: d, kind: 'normal' } : d)),
});

/** A small, clean workspace: `orders` (layered) uses `customers` (flat); `reports` is a read model. */
function fixture(patch = {}) {
  const base = {
    packages: [
      pkg('kernel', 'shared/kernel'),
      pkg('orders', 'modules/orders', ['kernel', 'customers']),
      pkg('customers', 'modules/customers', ['kernel']),
      pkg('reports', 'modules/reports', ['kernel']),
      pkg('server', 'server', ['orders', 'customers', 'reports', 'kernel']),
    ],
    config: {
      eventConsumers: ['reports'],
      flatModules: ['customers', 'reports'],
      sharedTables: ['outbox_events'],
      exceptions: [],
      readModels: { reports: { tables: ['orders'], reason: 'aggregates' } },
      modules: {
        orders: { dependsOn: ['customers'], tables: ['orders'] },
        customers: { dependsOn: [], tables: ['customers'] },
        reports: { dependsOn: [], tables: [] },
      },
      crossModuleSql: {},
    },
    layers: {
      orders: { domain: true, application: true, infrastructure: true, api: true },
      customers: {},
      reports: {},
    },
    sources: {},
    tables: ['customers', 'orders', 'outbox_events'],
  };
  const merged = { ...base, ...patch };
  merged.config = { ...base.config, ...(patch.config ?? {}) };
  return merged;
}

const has = (problems, text) =>
  assert.ok(problems.some((p) => p.includes(text)), `expected "${text}" in:\n${problems.join('\n')}`);

test('a clean workspace has no problems', () => {
  assert.deepEqual(analyze(fixture()), []);
});

test('an unregistered module crate is reported, and so is a registry entry with no crate', () => {
  const f = fixture();
  f.packages.push(pkg('billing', 'modules/billing', ['kernel']));
  f.config.modules.ghost = { dependsOn: [], tables: [] };
  const problems = analyze(f);
  has(problems, 'module "billing" is not registered');
  has(problems, '"ghost" is registered');
});

test('a new module dependency fails until it is listed, and a vanished one fails until removed', () => {
  const added = fixture();
  added.packages[2] = pkg('customers', 'modules/customers', ['kernel', 'orders']);
  has(analyze(added), 'customers -> orders is a new module dependency');

  const gone = fixture();
  gone.config.modules.orders = { dependsOn: ['customers', 'reports'], tables: ['orders'] };
  has(analyze(gone), 'modules.orders.dependsOn lists "reports"');
});

test('shared and adapter crates may not depend on modules or the server, and modules not on the server', () => {
  const f = fixture();
  f.packages[0] = pkg('kernel', 'shared/kernel', ['orders']);
  f.packages.push(pkg('providers', 'ai-providers', ['server']));
  f.packages[3] = pkg('reports', 'modules/reports', ['kernel', 'server']);
  const problems = analyze(f);
  has(problems, 'kernel (shared) depends on orders (module)');
  has(problems, 'providers (adapter) depends on server (server)');
  has(problems, 'reports depends on server');
});

test('a module may not call an event consumer unless the exception is named, and a stale exception fails', () => {
  const f = fixture();
  f.packages[2] = pkg('customers', 'modules/customers', ['kernel', 'reports']);
  f.config.modules.customers.dependsOn = ['reports'];
  has(analyze(f), 'customers depends on reports, an event consumer');

  f.config.exceptions = [
    { rule: 'event-consumer-called', from: 'customers', to: 'reports', reason: 'because' },
  ];
  assert.deepEqual(analyze(f), []);

  const stale = fixture();
  stale.config.exceptions = [
    { rule: 'event-consumer-called', from: 'orders', to: 'reports', reason: 'because' },
  ];
  has(analyze(stale), 'exception orders -> reports');
});

test('a cycle through a dev dependency is found', () => {
  const f = fixture();
  f.packages[2] = pkg('customers', 'modules/customers', ['kernel', { name: 'orders', kind: 'dev' }]);
  has(analyze(f), 'dependency cycle: customers -> orders -> customers');
});

test('a module that is not flat needs the four layers, and a flat one that has them must leave the list', () => {
  const missing = fixture();
  missing.layers.orders = { domain: true, application: true };
  has(analyze(missing), 'orders is not a flat module');
  has(analyze(missing), 'infrastructure/, api/');

  const migrated = fixture();
  migrated.layers.customers = { domain: true, application: true, infrastructure: true, api: true };
  has(analyze(migrated), 'customers has all four layers: remove it from flatModules');

  const ghost = fixture();
  ghost.config.flatModules = ['customers', 'reports', 'nothing'];
  has(analyze(ghost), 'flatModules lists "nothing"');
});

test('layers import inward only and the domain names no framework', () => {
  const f = fixture();
  f.sources.orders = [
    { path: 'domain/order.rs', text: 'use sqlx::PgPool;\nuse crate::api::Dto;' },
    { path: 'application/place.rs', text: 'use crate::infrastructure::Repo;\nuse axum::Json;' },
    { path: 'infrastructure/repo.rs', text: 'use crate::api::Dto;' },
    { path: 'api/routes.rs', text: 'use sqlx::PgPool; use crate::domain::Order;' },
  ];
  const problems = analyze(f);
  has(problems, 'orders/src/domain/order.rs: domain uses sqlx');
  has(problems, 'orders/src/domain/order.rs: domain imports crate::api');
  has(problems, 'orders/src/application/place.rs: application imports crate::infrastructure');
  has(problems, 'orders/src/application/place.rs: application uses axum');
  has(problems, 'orders/src/infrastructure/repo.rs: infrastructure imports crate::api');
  assert.equal(problems.filter((p) => p.includes('api/routes.rs')).length, 0);
});

test('a comment that names a forbidden crate or layer is not an import', () => {
  const f = fixture();
  f.sources.orders = [
    { path: 'domain/order.rs', text: '// never use sqlx:: here, or crate::api\n/* axum::Json */ let x = 1;' },
  ];
  assert.deepEqual(analyze(f), []);
});

test('every migrated table needs one owner, and the config may not name a table that does not exist', () => {
  const f = fixture({ tables: ['customers', 'orders', 'outbox_events', 'invoices'] });
  has(analyze(f), 'table invoices (created by a migration) has no owner');

  const stale = fixture({ tables: ['customers', 'outbox_events'] });
  has(analyze(stale), 'table orders is in module-boundaries.json but no migration creates it');

  const twice = fixture();
  twice.config.modules.customers.tables = ['customers', 'orders'];
  has(analyze(twice), 'table orders is owned by both');
});

test('SQL that names another module\'s table is a ratchet: it may not grow and must be lowered when it shrinks', () => {
  const f = fixture();
  f.sources.customers = [
    { path: 'queries.rs', text: 'sqlx::query("SELECT id FROM orders JOIN customers ON true")' },
  ];
  has(analyze(f), 'customers names orders (owned by orders) 1 times in SQL, up from 0');

  f.config.crossModuleSql = { customers: { orders: 1 } };
  assert.deepEqual(analyze(f), []);

  f.sources.customers = [{ path: 'queries.rs', text: 'SELECT 1' }];
  has(analyze(f), 'lower crossModuleSql.customers.orders to 0');
});

test('a read model reads the tables it lists and never writes', () => {
  const ok = fixture();
  ok.sources.reports = [{ path: 'queries.rs', text: 'SELECT count(*) FROM orders' }];
  assert.deepEqual(analyze(ok), []);

  const unlisted = fixture();
  unlisted.sources.reports = [{ path: 'queries.rs', text: 'SELECT * FROM customers' }];
  has(analyze(unlisted), 'reports reads customers, which is not on its read-model list');

  const writes = fixture();
  writes.sources.reports = [{ path: 'queries.rs', text: 'UPDATE orders SET total = 0' }];
  has(analyze(writes), 'reports is a read model and writes orders');
});

test('a module may use its own tables freely', () => {
  const f = fixture();
  f.sources.orders = [{ path: 'infrastructure/repo.rs', text: 'INSERT INTO orders VALUES (1)' }];
  assert.deepEqual(crossModuleCounts({ ...f }).actual, {});
});

test('migrationTables applies create and drop in order, so a table dropped and created again is live', () => {
  assert.deepEqual(
    migrationTables(['CREATE TABLE orders (id int);', 'DROP TABLE orders;', 'CREATE TABLE orders (id uuid);']),
    ['orders'],
  );
  assert.deepEqual(migrationTables(['DROP TABLE IF EXISTS gone;', 'CREATE TABLE gone (id int);']), ['gone']);
});

test('migrationTables drops tables that a later migration drops', () => {
  assert.deepEqual(
    migrationTables([
      'CREATE TABLE IF NOT EXISTS public.users (id int); -- create table fake (x)\nCREATE TABLE "orders" (id int);',
      'DROP TABLE orders;',
    ]),
    ['users'],
  );
});

test('sqlReferences reads and writes, case-insensitively and across lines', () => {
  const refs = sqlReferences(
    [{ text: 'select *\n  from Orders\n join customers c on 1;\ninsert into orders values (1);' }],
    ['orders', 'customers'],
  );
  assert.deepEqual(
    refs.map((r) => `${r.table}:${r.write}`).sort(),
    ['customers:false', 'orders:false', 'orders:true'],
  );
});

test('a write statement is one write, not a read and a write', () => {
  const refs = sqlReferences(
    [{ text: 'UPDATE orders SET a = 1; INSERT INTO orders VALUES (1); DELETE FROM orders;' }],
    ['orders'],
  );
  assert.deepEqual(refs.map((r) => r.write), [true, true, true]);
});

test('sqlReferences sees quoted and schema-qualified names, and TRUNCATE as a write', () => {
  const refs = sqlReferences(
    [
      {
        text: [
          'SELECT * FROM public."orders"',
          'SELECT * FROM "public"."orders"',
          'SELECT * FROM "orders"',
          'SELECT * FROM public.orders',
          'TRUNCATE TABLE orders',
          'TRUNCATE ONLY public."orders"',
          'truncate orders',
        ].join(';\n'),
      },
    ],
    ['orders'],
  );
  assert.deepEqual(
    refs.map((r) => r.write),
    [false, false, false, false, true, true, true],
  );
});

test('TRUNCATE writes every table it names, not only the first', () => {
  const refs = sqlReferences(
    [
      {
        text: [
          'TRUNCATE TABLE scratch, orders',
          'TRUNCATE orders, customers RESTART IDENTITY CASCADE',
          'truncate only public."orders" *, "customers"',
          'sqlx::query("TRUNCATE TABLE scratch, customers")',
          'TRUNCATE TABLE scratch',
        ].join(';\n'),
      },
    ],
    ['orders', 'customers'],
  );
  assert.deepEqual(
    refs.map((r) => `${r.table}:${r.write}`),
    ['orders:true', 'orders:true', 'customers:true', 'orders:true', 'customers:true', 'customers:true'],
  );

  const f = fixture();
  f.sources.reports = [{ path: 'queries.rs', text: 'TRUNCATE TABLE scratch, orders' }];
  has(analyze(f), 'reports is a read model and writes orders');
});

test('a quoted identifier inside a normal Rust string (\\"orders\\") is seen in every statement form', () => {
  const rust = String.raw`sqlx::query("TRUNCATE TABLE scratch, \"orders\"");
sqlx::query("SELECT 1 FROM public.\"orders\" JOIN \"customers\" c ON true");
sqlx::query("INSERT INTO \"public\".\"orders\" VALUES (1)");`;
  const refs = sqlReferences([{ text: rust }], ['orders', 'customers']);
  assert.deepEqual(
    refs.map((r) => `${r.table}:${r.write}`).sort(),
    ['customers:false', 'orders:false', 'orders:true', 'orders:true'],
  );

  const f = fixture();
  f.sources.reports = [{ path: 'queries.rs', text: String.raw`sqlx::query("TRUNCATE TABLE scratch, \"orders\"");` }];
  has(analyze(f), 'reports is a read model and writes orders');
});

test('a read model may not truncate, and a shared table is writable by no read model', () => {
  const truncate = fixture();
  truncate.sources.reports = [{ path: 'queries.rs', text: 'TRUNCATE TABLE orders' }];
  has(analyze(truncate), 'reports is a read model and writes orders');

  const shared = fixture();
  shared.sources.reports = [
    { path: 'queries.rs', text: 'SELECT * FROM outbox_events; DELETE FROM outbox_events' },
  ];
  const problems = analyze(shared);
  has(problems, 'reports is a read model and writes outbox_events');
  assert.equal(problems.filter((p) => p.includes('reads outbox_events')).length, 0);

  const everyone = fixture();
  everyone.sources.customers = [{ path: 'queries.rs', text: 'INSERT INTO outbox_events VALUES (1)' }];
  assert.deepEqual(analyze(everyone), []);
});

test('stripComments removes line, doc and block comments but keeps a URL in a string', () => {
  const text = stripComments('let a = 1; // c\n/// doc\n/* b\nc */ let url = "http://x";');
  assert.ok(!text.includes('doc') && !text.includes('// c') && !/\bb\b/.test(text));
  assert.ok(text.includes('http://x'));
});

test('stripComments keeps a // or /* inside a string, a raw string and a char literal', () => {
  const sql = `sqlx::query("SELECT 'a // b' FROM orders") // trailing`;
  const out = stripComments(sql);
  assert.ok(out.includes(`'a // b' FROM orders`));
  assert.ok(!out.includes('trailing'));

  const raw = stripComments('let q = r#"SELECT "x" // y FROM orders"#; // gone');
  assert.ok(raw.includes('// y FROM orders') && !raw.includes('gone'));

  const chars = stripComments(`let q = '"'; let lt: &'a str = x; /* c */ let z = "// keep";`);
  assert.ok(chars.includes('"// keep"') && !chars.includes('/* c */'));

  const nested = stripComments('a /* one /* two */ still comment */ b');
  assert.ok(nested.includes('a ') && nested.includes(' b') && !nested.includes('still'));
});

test('layersReached resolves crate, self and super paths and grouped imports', () => {
  const reach = (path, text) => [...layersReached(path, text)].sort();
  assert.deepEqual(reach('domain.rs', 'use super::infrastructure::Repo;'), ['infrastructure']);
  assert.deepEqual(reach('domain/mod.rs', 'use super::api::Dto;'), ['api']);
  assert.deepEqual(reach('domain/order.rs', 'use super::super::api::Dto;'), ['api']);
  assert.deepEqual(reach('domain/order.rs', 'use crate::{infrastructure::Repo};'), ['infrastructure']);
  assert.deepEqual(
    reach('domain/order.rs', 'use crate::{api::Dto, domain::Order, application::{a, b}};'),
    ['api', 'application', 'domain'],
  );
  assert.deepEqual(reach('domain.rs', 'use super::{api::Dto, order::Order};'), ['api']);
  assert.deepEqual(reach('domain.rs', 'let x = crate::infrastructure::Repo::new();'), ['infrastructure']);
  // `super` in a nested file is the layer itself, so a sibling module that shares a layer's name is not the layer.
  assert.deepEqual(reach('domain/order.rs', 'use super::infrastructure::Repo;'), []);
  assert.deepEqual(reach('domain/order.rs', 'use super::Order; use self::money::Money;'), []);
});

test('an outward import written with super or in a group fails the layer check', () => {
  const f = fixture();
  f.sources.orders = [
    { path: 'domain.rs', text: 'use super::infrastructure::Repo;' },
    { path: 'application/place.rs', text: 'use crate::{infrastructure::Repo, domain::Order};' },
    { path: 'application/ok.rs', text: 'use crate::{domain::Order}; use super::place::Place;' },
  ];
  const problems = analyze(f);
  has(problems, 'orders/src/domain.rs: domain imports crate::infrastructure');
  has(problems, 'orders/src/application/place.rs: application imports crate::infrastructure');
  assert.equal(problems.filter((p) => p.includes('ok.rs')).length, 0);
});

test('a // inside an SQL string does not hide a cross-module table from the count', () => {
  const f = fixture();
  f.sources.customers = [
    { path: 'queries.rs', text: `sqlx::query("SELECT 'a // b' FROM orders") // note` },
  ];
  has(analyze(f), 'customers names orders (owned by orders) 1 times in SQL, up from 0');
});

test('the generated document lists modules, layouts, edges, exceptions and the SQL total', () => {
  const f = fixture();
  f.config.crossModuleSql = { customers: { orders: 2 } };
  f.config.exceptions = [{ rule: 'event-consumer-called', from: 'orders', to: 'reports', reason: 'r' }];
  const doc = renderDoc(f);
  assert.match(doc, /\| `orders` \| layered \| `customers` \|/);
  assert.match(doc, /\| `customers` \| flat \|/);
  assert.match(doc, /orders --> customers/);
  assert.match(doc, /\| event-consumer-called \| `orders` \| `reports` \| r \|/);
  assert.match(doc, /\| `customers` \| `orders` \| `orders` \| 2 \|/);
  assert.match(doc, /Total: 2\./);
  assert.equal(renderDoc(f), doc);
});
