# ADR-018: Large append-only tables stay unpartitioned until a measured trigger fires

- **Status:** Accepted
- **Date:** 2026-11-18
- **Deciders:** Platform engineering
- **Related:** [System architecture §7.1, §10.7 and §14](../architecture/system-architecture.md#71-postgresql-conventions)
  (amended in the same PR; §15 gives the RTO), [ADR-004](0004-multi-tenancy-rls-cells.md) (row-level security),
  [ADR-007](0007-postgres-queue-transactional-outbox.md) (Q3: processed rows are deleted),
  [ADR-013](0013-uuidv7-identifiers.md) (UUIDv7 identifiers),
  [Tenant Unit of Work](../security/tenant-unit-of-work.md),
  [Operations runbook §11](../runbooks/operations.md#11-check-the-size-of-the-large-append-only-tables),
  MOH-86, MOH-142 (per-tenant retention), MOH-202 (size metrics and alerts)

## Context

The architecture (§7.1) says the large append-only tables "are range-partitioned by month" so that
retention can detach old partitions instead of running slow deletes. MOH-86 asked whether that is
true, whether it should be, and what converting later costs. This ADR reports what the code has
and what the spike measured, then decides.

**What exists.**

- **Nothing is partitioned.** `pg_partitioned_table` is empty on a database migrated to 0078.
- **The architecture names tables that do not exist.** `audit_events`, `ai_turns` and
  `usage_events` are `audit_logs`, `ai_generations` and `ai_usage_records`. "Outbox history" is not
  a table: ADR-007 Q3 deletes an outbox row once it is processed, so `outbox_events` holds only the
  backlog and the dead letters.
- **No retention exists for the other four.** `audit_logs` has an append-only trigger that raises
  on `UPDATE` and `DELETE`, so there is no way to remove a row. `messages` and the AI tables are
  kept forever. Per-tenant retention (constitution Principle X, "retention periods are
  configurable") is MOH-142 and is not built.
- **Every table is under forced row-level security** with the `tenant_isolation` policy.

What each candidate looks like to a partitioning scheme:

| Table | Primary key | Foreign keys that point at it | Other |
|---|---|---|---|
| `messages` | `(id)`, also `UNIQUE (tenant_id, id)` | five, from four tables: `message_citations(message_id)`, `ai_generations(trigger_message_id)` and `(response_message_id)`, `whatsapp_message_meta(tenant_id, message_id)`, `message_attachments(tenant_id, message_id)` | Timeline index `(tenant_id, conversation_id, created_at DESC, seq DESC)` |
| `audit_logs` | `(id)` | none | Append-only trigger; read by `(tenant_id, created_at DESC)` |
| `ai_generations` | `(id)` | `tool_requests(generation_id)` | |
| `ai_usage_records` | `(id)` | `ai_generations(usage_record_id)` | Holds `request_content` and `response_content` when a tenant turns on `capture_content` (default off, audited) |
| `outbox_events` | `(id)` | none | Partial indexes over unprocessed rows |

**What the repository cannot tell us: current volume.** There is no production data in the
repository, and the development database used for this spike has 0 rows in `messages`,
`ai_generations`, `ai_usage_records` and `outbox_events`, and 1 in `audit_logs`. "Current row
counts" are therefore not measurable here, and this ADR does not present zeros as a measurement.
What it does have is (a) bytes per row, measured on the real schema, and (b) volumes modelled from
the scale targets in spec 001 (NFR-SCAL-001, NFR-SCAL-002, SC-008): 1,000 tenants, **1M
conversations a month platform-wide**, up to **50k conversations a month for one tenant**.

**Measured: bytes per row** (heap, every index and TOAST; 300,000 synthetic rows in a table copied
from the real schema with `CREATE TABLE ... (LIKE ... INCLUDING INDEXES)`; PostgreSQL 16.15):

| Table | Bytes per row | Note |
|---|---|---|
| `messages` | 498 | Body averaging 160 characters |
| `audit_logs` | 332 | `details` of about 150 bytes of JSON |
| `ai_generations` | 311 | |
| `ai_usage_records` | 219 | Content capture off |
| `ai_usage_records` | **4,875** | Content capture on: a 3 KB request and a 1.5 KB response; 22 times larger |

**Modelled: volume.** The specs give conversations, not rows per conversation, so these multipliers
are assumptions of this ADR: **12 messages**, **5 AI generations** and **1.2 usage records per
generation** (retries and tool continuations) per conversation, and **0.3 audit rows** per
conversation. The audit figure is the least certain: it depends on which actions are audited and
how often tenants change settings.

| Scale | Table | Rows per year | GB per year | Months to the trigger below |
|---|---|---|---|---|
| Launch (100k conversations a month) | `messages` | 14.4M | 7.2 | 167 (to 100 GB) |
| | `ai_generations` | 6.0M | 1.9 | 322 (to 50 GB) |
| | `ai_usage_records` | 7.2M | 1.6 | 381 (to 50 GB) |
| | `audit_logs` | 0.4M | 0.1 | 1,667 (to 50M rows) |
| Target (1M a month) | `messages` | 144M | 71.7 | **17** (to 100 GB) |
| | `ai_generations` | 60M | 18.7 | 32 (to 50 GB) |
| | `ai_usage_records` | 72M | 15.8 | 38 (to 50 GB) |
| | `audit_logs` | 3.6M | 1.2 | 167 (to 50M rows) |
| Target, every tenant capturing content | `ai_usage_records` | 72M | **351** | **1.7** (to 50 GB) |

The last row is the finding that matters most: **row count is not what makes `ai_usage_records`
large; captured content is.** One tenant at the 50k-a-month ceiling with capture on adds 17.6 GB a
year. That is a retention question (MOH-142), and partitioning does not answer it.

**Measured: what partitioning does** (a throwaway database made from the migrated schema, on
PostgreSQL 16.15; the statements are in the description of the PR that introduced this ADR):

- **F1. The partition key must be in every unique key.** `PRIMARY KEY (id)` on a table partitioned
  by `created_at` is rejected: "PRIMARY KEY constraint on table lacks column created_at which is
  part of the partition key". With `PRIMARY KEY (id, created_at)`, a foreign key to `id` alone
  becomes impossible ("there is no unique constraint matching given keys"). So a table that other
  tables reference by `id` cannot be partitioned by `created_at` without rewriting every reference.
- **F2. Partitioning by `id` range keeps every key and foreign key unchanged.** `id` is a UUIDv7
  whose first 48 bits are a millisecond timestamp, so `RANGE (id)` with monthly bounds works with
  `PRIMARY KEY (id)`, `UNIQUE (tenant_id, id)`, a single-column foreign key and a composite one,
  all created against the partitioned table. A row inserted with the default id landed in the
  partition of the month it was inserted in. Two conditions come with it, covered under "Preconditions" below.
- **F3. A new partition is readable across tenants by the runtime role.** A partition created with
  `PARTITION OF` has row-level security off and no policy, and the default privileges of
  `app_owner` grant `arwd` on it to `app_runtime` and `app_platform`. With `app.tenant_id` set to
  tenant 1, the parent returned 2 rows and the fresh partition, queried directly, returned 4: both
  tenants' rows. After `ENABLE` and `FORCE ROW LEVEL SECURITY` plus the policy on the partition, the
  direct read returned 2; after revoking the grants, it failed with "permission denied".
- **F4. Reads touch every partition.** The conversation timeline query (tenant, conversation,
  newest 50) over 24 monthly partitions and 480,000 rows probed all 24 indexes (3 warm runs):
  0.36 to 0.69 ms, against 0.11 to 0.16 ms on one table. Adding a lower bound
  `id >= uuid_lower_bound(<conversation start>)` reduced the plan to 7 of the 24 partitions. The
  cost grows with the number of partitions, and is small in absolute terms.
- **F5. The append-only trigger does not stop retention by drop.** A row trigger created on the
  partitioned `audit_logs` is copied to every partition and still blocks `DELETE`. `ALTER TABLE ...
  DETACH PARTITION` and `DROP TABLE` ran without raising. Removing a partition is the one way to
  remove audit rows, so it must be a privileged, audited operation.
- **F6. Converting `audit_logs` later is cheap.** Test table: 1M rows, 283 MB, the real foreign
  keys to `users` and `tenants`, the append-only trigger. Preparation, with no write lock:
  `CREATE UNIQUE INDEX CONCURRENTLY ... (id, created_at)` took 638 ms, and adding then validating a
  `CHECK (created_at < <boundary>)` took 48 ms. The swap, in one transaction (rename; replace the
  primary key `USING INDEX`; drop the legacy trigger; create the partitioned parent; `ATTACH` the
  legacy table as `FROM (MINVALUE) TO (<boundary>)`; create the next partition; create the trigger
  on the parent), took **38 ms in all, 1.2 ms of it the `ATTACH`**. The legacy table's foreign keys
  were adopted without duplicates, the trigger still blocked `DELETE`, and a `created_at` filter
  pruned to one partition. Without the validated `CHECK`, the same `ATTACH` took 631 ms because it
  scans the table while locked. Build time scales with size (about 64 s per 100M rows, an
  estimate by linear scaling); the swap does not.
- **F7. Converting `messages` later is cheap if its foreign keys are re-pointed.** Test: a
  `messages`-like table of 1M rows (227 MB) partitioned by `id`, with a single-column and a
  composite foreign key from two child tables of 1M rows each. The primary key already contains
  `id`, so no key changes. The swap transaction took **9 ms**. **A foreign key does not follow the
  rename:** a control showed `confrelid` still pointing at `ml2_legacy` after the swap, so rows in
  new partitions could never be referenced. The two foreign keys were dropped and re-added
  `NOT VALID` against the new parent inside the swap transaction, then validated afterwards: 285 ms
  and 743 ms (a scan of each child; `SHARE UPDATE EXCLUSIVE`, so writes continue). Scaled
  linearly to 100M rows that is about 30 s and 75 s per child, an estimate. `messages` has five
  such foreign keys.

**The premise of the issue is partly wrong.** MOH-86 says converting a large table later is "far
harder than starting partitioned". F6 and F7 show that with two techniques (a validated `CHECK`
before `ATTACH`, and foreign keys re-pointed `NOT VALID` in the swap transaction) the locked part is
tens of milliseconds, independent of size. What grows with size is non-blocking work: an index
build and constraint validation. Starting partitioned costs something every day instead: F3 and F4,
a maintenance job, a rule about row-level security on every new partition, and nothing to show for
it until a retention rule or a size problem exists.

## Decision

**1. We will not partition any of the five tables now.** Each gets a trigger below, checked by the
query in the operations runbook (§11) and, once MOH-202 is built, by an alert.

**2. Per table, when a trigger fires (any one of them):**

| Table | Verdict | Method | Trigger |
|---|---|---|---|
| `messages` | Later | Monthly `RANGE (id)`; the existing table becomes the first partition (F7) | Total size reaches **100 GB** (about 17 months at target scale); or a **platform-wide** maximum age for messages is adopted; or a restore rehearsal exceeds the **1 h RTO** |
| `audit_logs` | Later | Monthly `RANGE (created_at)`, `PRIMARY KEY (id, created_at)` (F6); no table references it by `id` | Any **retention period for audit records** is adopted (the append-only trigger leaves dropping a partition as the only removal); or **50M rows** (about 16 GB); or the RTO rehearsal |
| `ai_generations` | Later | Monthly `RANGE (id)`, because `tool_requests(generation_id)` references `id` | **50 GB**; or a retention period is adopted; or the RTO rehearsal |
| `ai_usage_records` | Later, after content retention | Monthly `RANGE (id)`, because `ai_generations(usage_record_id)` references `id` | **50 GB with content retention in place** (an expiry of `request_content` and `response_content` is the first fix, MOH-142); or a retention period is adopted; or the RTO rehearsal. Billing and platform analytics read this table by period, so no partition is dropped before their windows have passed |
| `outbox_events` | Never, while ADR-007 Q3 stands | None | Not a partitioning trigger. The monitoring threshold is **100,000 rows**: the normal state is near zero, because processed rows are deleted, so more means workers are behind or dead letters are piling up. If Q3 changes to keep history, `RANGE (created_at)` works: nothing references this table |

The 100 GB and 50 GB figures are judgments, not measurements: this spike did not measure vacuum,
index maintenance or restore at those sizes. They are set so that a restore stays well inside the
RTO, and the shared RTO trigger catches the case where they are wrong. Revisit them with the first
real measurement.

Time partitions serve a platform-wide maximum age. **They cannot implement per-tenant retention**
(MOH-142), because a dropped month removes every tenant's rows. Per-tenant retention needs row
deletes whatever the table layout, so MOH-142 does not wait for this decision.

**3. Preconditions before converting a table partitioned by `id`.**

- **Every id must be a UUIDv7.** Migration 0075 changed the default of 40 tables from
  `gen_random_uuid()` to `uuidv7()` and says "existing rows keep their v4 ids". Rows from before it,
  and any row inserted with an id from elsewhere, have a random id that no time range holds.
  Count them first:

  ```sql
  SELECT count(*) FILTER (WHERE get_byte(uuid_send(id), 6) >> 4 <> 7) AS non_v7_ids,
         count(*) AS total
  FROM messages;
  ```

  If `non_v7_ids` is not 0, the `FROM (MINVALUE) TO (<boundary>)` attach fails its `CHECK`, because
  a random id is almost always above any 2026 boundary. Attach the legacy table as the `DEFAULT`
  partition instead (PostgreSQL then scans it whenever a new range is added, to prove it holds no
  row of that range; rehearse that cost).
- **The embedded timestamp is used only to place a row, never as business time.** ADR-013 point 6
  says the id's timestamp is not trusted for business logic; this keeps that rule, because
  `created_at` stays the business time and retention windows are written against it. The two
  clocks differ by at most the length of one transaction (`created_at` is `now()`, the id comes
  from `clock_timestamp()`), so a row near a month boundary can sit in the neighbouring partition.
  A rule that must be exact on `created_at` uses a row delete, not a drop.
- ADR-013 point 2 says ids are generated in the application. For these tables most inserts omit the
  id and take the `uuidv7()` SQL default, as the `INSERT INTO messages` statements show. Both paths
  produce UUIDv7; any new writer that supplies a different id (an import, a test generator that
  ignores time) lands in the `DEFAULT` partition, which the maintenance job alerts on.

**4. Conversion method (expand and contract).** Rehearse it on a restore of production first. For
`messages`, partitioned by `id`:

1. Pick the boundary: the first day of the month after the one in which the work runs, so that
   `max(id)` is below it.
2. `ALTER TABLE messages ADD CONSTRAINT ... CHECK (id < uuid_lower_bound(<boundary>)) NOT VALID`,
   then `VALIDATE CONSTRAINT` (no write lock). Without it the `ATTACH` scans the table while locked.
3. In one transaction: rename `messages` to `messages_legacy`; create the partitioned `messages`
   with the same columns, keys, checks, foreign keys, indexes, triggers, row-level security and
   policy; `ATTACH PARTITION messages_legacy FOR VALUES FROM (MINVALUE) TO (<boundary>)`; create the
   next months' partitions with rule R1 below; **drop and re-add each inbound foreign key `NOT
   VALID` against the new parent** (F7).
4. After the commit, `VALIDATE CONSTRAINT` each re-pointed foreign key.
5. Start the maintenance job. Run the schema rule and the isolation harness.

For `audit_logs` the same steps apply with `RANGE (created_at)`, with two more steps inside the
swap: build `UNIQUE (id, created_at)` `CONCURRENTLY` first and then replace the primary key with
`ADD CONSTRAINT ... PRIMARY KEY USING INDEX`, and drop the append-only trigger on the legacy table
and create it on the parent (it is copied to the partitions; a trigger of the same name on the
legacy table makes the `ATTACH` fail). The helper:

```sql
CREATE FUNCTION uuid_lower_bound(ts timestamptz) RETURNS uuid LANGUAGE sql IMMUTABLE AS
$$ SELECT (lpad(to_hex((extract(epoch FROM ts) * 1000)::bigint), 12, '0') || repeat('0', 20))::uuid $$;
```

The spike verified that the `ATTACH` adopts the legacy table's primary key, unique index and
foreign keys. It did not verify the same for the non-unique indexes (the timeline index): create
the parent's indexes before the `ATTACH` and confirm in the rehearsal that the legacy ones are
matched and not rebuilt.

**5. Scheduler: a job in the server, not `pg_partman`.** The Postgres image
(`pgvector/pgvector:0.8.6-pg16`) has neither `pg_partman` nor `pg_cron` among its available
extensions. The architecture describes the scheduler as leader-elected, but the code has no leader
election: a search of `backend/crates` for `leader` and for advisory locks finds nothing, and each
instance runs its own sweepers. So the partition job must be written as the other sweepers are,
**idempotent and safe to run on several instances at once**: it calls `pg_try_advisory_lock`,
creates partitions with `CREATE TABLE IF NOT EXISTS`, always keeps the next three months created,
and alerts when its `DEFAULT` partition holds any row. Whether `pg_partman` would apply the rules
below to the partitions it creates was not checked; if it is ever adopted, that check comes first.

**6. Rules for every partitioned table, from the first one.**

- **R1. A partition is created isolated or not at all.** In the same transaction as `CREATE TABLE
  ... PARTITION OF`, the creating function either revokes every privilege on the partition from
  `app_runtime` and `app_platform` (the parent's grants are enough for queries through the parent),
  or enables and forces row-level security and creates the `tenant_isolation` policy on it. The job
  runs a catalog check each cycle (a partition with `relrowsecurity` false and a grant to a runtime
  role is an error) and refuses to leave one exposed. A partition created by hand has neither.
- **R2. CI must see partitions, and today it does not.** The schema rule
  (`db::testing::isolation::rls_rule_violations`, run by `rls_schema_rule.rs`) selects
  `relkind IN ('r', 'p') AND NOT relispartition`: it checks a partitioned parent and **skips every
  partition**. A partition without row-level security therefore passes CI today, which is F3. The
  migration that first partitions a table must extend the rule so that each partition has
  row-level security enabled and forced and the policy, or no grant to a runtime role, and add a
  planted-partition test that must fail, as the existing planted-table tests do. The partitions
  that the job creates at run time do not exist in CI's migrated schema, so R1's catalog check is
  what covers them in production. `just verify-roles` (`infra/postgres/verify-roles.sh`) gains a
  check that a partition created with `PARTITION OF` is unreadable across tenants by `app_runtime`.
- **R3. Retention by drop is privileged and audited.** It runs as the owner, through a function in
  the `xtenant` schema that records who asked and what it removed, like
  `xtenant.integration_retention_sweep`, never as `app_runtime`. Dropping an `audit_logs` partition
  is itself an audit event, written to a table that is not being dropped.
- **R4. Keys stay compatible with `RANGE (id)` until then.** Until a trigger fires, a new unique key
  on `messages`, `ai_generations` or `ai_usage_records` must not need `created_at`, and a new table
  that references one of them does so by `id` (or `(tenant_id, id)`), as today.

## Alternatives considered

| Option | Why not |
|---|---|
| **Partition all four tables now** (the architecture's plan) | Pays F3, F4, a maintenance job and a row-level-security rule on every new partition from day one, for a table of a few thousand rows. Nothing needs it: no retention rule exists, and the largest table is 17 months from 100 GB at target scale. The "later" path costs tens of milliseconds of lock (F6, F7). |
| **Partition `audit_logs` now, the others later** | It is the easiest to convert and the only one with no foreign keys. But it also has the lowest volume (167 months to 50M rows at target scale), and the one reason to partition it, retention, is a policy nobody has set. The trigger covers it the day a retention period is adopted. |
| **`RANGE (created_at)` for `messages`** (what the architecture implies) | Requires `PRIMARY KEY (id, created_at)`, which makes the five single-column and composite foreign keys impossible (F1) and forces all four child tables and the code that writes them to carry `created_at`. `RANGE (id)` (F2) gets the same partitions with no schema change to any other table. |
| **Hash partition by `tenant_id`** | It would prune a timeline query to one partition, but it removes nothing over time: there is no month to drop, so it does not serve retention, and a tenant at the 50k-a-month ceiling still lands whole in one partition. |
| **`pg_partman`** | Not in the Postgres image, and it has not been checked against rule R1. See decision 5. |
| **Delete old rows in batches, as the integration and notification sweeps do** | Works for `integration_events` and notifications today and stays the right tool for per-tenant retention. It does not work for `audit_logs`, where the trigger forbids it, and it leaves bloat and a slow `VACUUM` on a very large table. It is what MOH-142 will use until a partition trigger fires. |
| **Do nothing and drop the architecture's claim** | Loses the retention path for `audit_logs`, which has none. The measured conversion is cheap enough to keep the option open, so the architecture keeps the plan, with a trigger. |

## Consequences

**Positive**

- No partition maintenance, no per-partition isolation rule and no extra planner cost until there
  is a reason, and the architecture document stops describing tables that do not exist.
- The cost of waiting is known: F6 and F7 give the steps and the timings, so the conversion is a
  rehearsed procedure rather than a research project.
- F3 is recorded before anyone creates a partition: a partition made by `CREATE TABLE ... PARTITION
  OF` exposes every tenant's rows to the runtime role.
- The biggest growth risk is named: captured AI content, not row count (22 times per row).

**Negative / costs**

- The triggers are only as good as what watches them. Until MOH-202 is built, the check is a query
  someone has to run (runbook §11).
- The size thresholds are judgments. The volume model rests on four multipliers this ADR chose;
  real traffic may differ by a factor of several, and the first production measurement should
  replace them.
- If a precondition fails (rows with a v4 id in `messages`), the conversion is harder than the
  rehearsed one: a `DEFAULT` partition and its scans.
- The scale estimates for validation and index builds (about 30 to 75 s per child, 64 s per index
  per 100M rows) come from linear scaling of 1M-row tests, not from a large table.

**Follow-ups**

- MOH-202: export size and row-estimate metrics for the five tables and alert at the triggers above.
- MOH-142: per-tenant retention. It should also decide how long `request_content` and
  `response_content` are kept, the largest growth risk found here.
- The architecture describes a leader-elected scheduler that the code does not have (decision 5).
  This ADR works around it; closing that gap is a separate decision.
