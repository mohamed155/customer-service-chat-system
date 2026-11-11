-- Migration 0075: new primary keys default to UUIDv7 (MOH-42, ADR-013 gap U1).
--
-- ADR-013 says every new primary key is a UUIDv7, generated in the application (`kernel::ids::new_id`).
-- Rows inserted without an id (most inserts into messages, audit_logs and the low-volume tables, and
-- every row written by a SQL function) used `gen_random_uuid()`, a random v4. This migration adds a
-- `uuidv7()` function and makes it the default of all 40 primary-key `id` columns that had the v4
-- default. Existing rows keep their v4 ids; v4 and v7 values coexist in one column (ADR-013 point 8).
--
-- `ALTER COLUMN ... SET DEFAULT` changes the catalog only. It does not rewrite the table.
--
-- The function is a stand-in for the built-in `uuidv7()` of PostgreSQL 18. It lives in `public`, and
-- the comment below names the schema, because on 18 an unqualified `uuidv7()` finds the built-in in
-- `pg_catalog` first. A database created on 18 from these migrations therefore ends up with the
-- built-in as the default. A database upgraded from an earlier version keeps the defaults bound to
-- `public.uuidv7()` (a default stores the function it was created with), so before dropping ours a
-- later migration must run `ALTER TABLE ... ALTER COLUMN id SET DEFAULT pg_catalog.uuidv7()` for the
-- tables below; see ADR-013. Its randomness comes from `gen_random_uuid()`; only the first 48 bits (the
-- Unix millisecond timestamp) and the version bits are replaced.
--
-- From here on a migration must not use `gen_random_uuid()` as a column default:
-- `server/tests/uuid_hygiene.rs` fails on it. A table whose ids the application always supplies needs
-- no default; one that wants a fallback uses `uuidv7()`.

CREATE FUNCTION uuidv7() RETURNS uuid
LANGUAGE sql
VOLATILE
PARALLEL SAFE
AS $$
  SELECT encode(
    set_bit(
      set_bit(
        overlay(
          uuid_send(gen_random_uuid())
          PLACING substring(int8send((extract(epoch FROM clock_timestamp()) * 1000)::bigint) FROM 3)
          FROM 1 FOR 6
        ),
        52, 1
      ),
      53, 1
    ),
    'hex'
  )::uuid
$$;

COMMENT ON FUNCTION public.uuidv7() IS
  'UUIDv7 (RFC 9562): 48-bit Unix ms timestamp, version 7, random rest. Stand-in for PostgreSQL 18''s built-in (MOH-42).';

ALTER TABLE agent_avatar_uploads ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE agent_configurations ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE agent_prompt_versions ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE agent_prompts ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE ai_configurations ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE ai_credentials ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE ai_generations ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE ai_usage_records ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE audit_logs ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE billing_plans ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE conversation_feedback ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE conversations ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE customer_channel_identifiers ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE customers ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE escalations ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE integration_catalog ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE integration_connections ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE integration_events ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE integration_secrets ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE integration_webhook_deliveries ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE knowledge_categories ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE knowledge_chunks ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE knowledge_items ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE message_attachments ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE message_citations ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE messages ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE notifications ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skills ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE tenant_invitations ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE tenant_memberships ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE tenant_rate_limit_overrides ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE tenant_subscriptions ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE tenant_tool_policies ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE tenant_tools ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE tenants ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE tool_requests ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE users ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE whatsapp_message_meta ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE widget_instances ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE widget_sessions ALTER COLUMN id SET DEFAULT uuidv7();
