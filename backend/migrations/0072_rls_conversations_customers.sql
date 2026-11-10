-- Migration 0072: Row-Level Security on conversations, messages, customers and related tables
-- (MOH-47, step 4a of MOH-31; ADR-004 section 2).
--
-- Every table below has `tenant_id uuid NOT NULL`. Each gets RLS enabled and forced (FORCE makes
-- the policy apply to the table owner too), with one policy for every command: a row is visible
-- and writable only when its tenant is the one the transaction set with
-- `set_config('app.tenant_id', ..., true)` (db::UnitOfWork). Without a tenant, or with an empty
-- one, NULLIF maps it to NULL, the comparison is never true and nothing matches: reads return no
-- rows and writes fail the WITH CHECK.
--
-- What keeps working without a tenant: the audited cross-tenant functions in the `xtenant` schema
-- (owned by app_platform, which has BYPASSRLS) and referential-integrity checks, which Postgres
-- exempts from RLS.
--
-- A data migration that UPDATEs or backfills one of these tables, run as app_owner, now touches
-- zero rows: it must `SET LOCAL ROLE app_platform` or set the tenant (see migrations/README.md).

DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY[
    'customers',
    'customer_channel_identifiers',
    'conversations',
    'messages',
    'message_attachments',
    'message_citations',
    'conversation_feedback',
    'escalations',
    'whatsapp_message_meta'
  ] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format(
      'CREATE POLICY tenant_isolation ON %I '
      'USING (tenant_id = NULLIF(current_setting(''app.tenant_id'', true), '''')::uuid) '
      'WITH CHECK (tenant_id = NULLIF(current_setting(''app.tenant_id'', true), '''')::uuid)',
      t);
  END LOOP;
END
$$;
