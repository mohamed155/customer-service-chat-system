-- Migration 0027: Composite FK for (tenant_id, customer_id)
--
-- Replace single-column FK on customer_channel_identifiers.customer_id → customers(id)
-- and conversations.customer_id → customers(id) with composite FKs that also
-- reference tenant_id, preventing cross-tenant child rows at the DB level.

-- 1. Unique constraint on customers(tenant_id, id) for the composite FK target
--    PostgreSQL does not allow partial UNIQUE indexes (WHERE clause) as FK
--    targets, so we use a full UNIQUE constraint instead.
DROP INDEX IF EXISTS customers_tenant_id_id_uq;
ALTER TABLE customers ADD CONSTRAINT customers_tenant_id_id_uq UNIQUE (tenant_id, id);

-- 2. Drop existing single-column FKs
ALTER TABLE customer_channel_identifiers
    DROP CONSTRAINT IF EXISTS customer_channel_identifiers_customer_id_fkey;

ALTER TABLE conversations
    DROP CONSTRAINT IF EXISTS conversations_customer_id_fkey;

-- 3. Add composite FKs with NOT VALID (no long lock on large tables)
ALTER TABLE customer_channel_identifiers
    ADD CONSTRAINT customer_channel_identifiers_parent_tenant_fkey
    FOREIGN KEY (tenant_id, customer_id)
    REFERENCES customers (tenant_id, id)
    NOT VALID;

ALTER TABLE conversations
    ADD CONSTRAINT conversations_parent_tenant_fkey
    FOREIGN KEY (tenant_id, customer_id)
    REFERENCES customers (tenant_id, id)
    NOT VALID;

-- 4. Validate constraints
ALTER TABLE customer_channel_identifiers
    VALIDATE CONSTRAINT customer_channel_identifiers_parent_tenant_fkey;

ALTER TABLE conversations
    VALIDATE CONSTRAINT conversations_parent_tenant_fkey;
