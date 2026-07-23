-- Integrations foundation (spec 028): catalog, per-tenant connections,
-- encrypted secrets, accepted webhook deliveries, and a per-connection
-- event log. One connection per (tenant, catalog) — reconnecting
-- reactivates the same row.

CREATE TABLE integration_catalog (
    id            uuid        NOT NULL DEFAULT gen_random_uuid(),
    slug          text        NOT NULL UNIQUE,
    name          text        NOT NULL,
    description   text        NOT NULL,
    category      text        NOT NULL,
    is_available  boolean     NOT NULL DEFAULT false,
    config_schema jsonb       NOT NULL DEFAULT '[]'::jsonb,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (id)
);

CREATE TABLE integration_connections (
    id                           uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id                    uuid        NOT NULL REFERENCES tenants(id),
    catalog_id                   uuid        NOT NULL REFERENCES integration_catalog(id),
    is_active                    boolean     NOT NULL DEFAULT true,
    config                       jsonb       NOT NULL DEFAULT '{}'::jsonb,
    webhook_token_hash           bytea       NOT NULL,
    webhook_token_ciphertext     bytea       NOT NULL,
    webhook_token_nonce          bytea       NOT NULL,
    connected_at                 timestamptz NOT NULL DEFAULT now(),
    connected_by_membership_id   uuid        REFERENCES tenant_memberships(id),
    disconnected_at              timestamptz,
    disconnected_by_membership_id uuid       REFERENCES tenant_memberships(id),
    created_at                   timestamptz NOT NULL DEFAULT now(),
    updated_at                   timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (id)
);

-- One connection per (tenant, integration) for all time; reconnect reactivates.
CREATE UNIQUE INDEX integration_connections_tenant_catalog_uq
    ON integration_connections (tenant_id, catalog_id);

-- Token resolves to exactly one connection (used by webhook intake lookup).
CREATE UNIQUE INDEX integration_connections_token_hash_uq
    ON integration_connections (webhook_token_hash);

-- List-page tenant join.
CREATE INDEX integration_connections_tenant_idx
    ON integration_connections (tenant_id);

CREATE TABLE integration_secrets (
    id            uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id     uuid        NOT NULL,
    connection_id uuid        NOT NULL REFERENCES integration_connections(id) ON DELETE CASCADE,
    field_key     text        NOT NULL,
    ciphertext    bytea       NOT NULL,
    nonce         bytea       NOT NULL,
    hint          text        NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (id)
);

-- Rotation = upsert of the row (old ciphertext replaced).
CREATE UNIQUE INDEX integration_secrets_connection_field_uq
    ON integration_secrets (connection_id, field_key);

-- Retention sweep + tenant isolation checks.
CREATE INDEX integration_secrets_tenant_idx
    ON integration_secrets (tenant_id);

CREATE TABLE integration_webhook_deliveries (
    id            uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id     uuid        NOT NULL,
    connection_id uuid        NOT NULL REFERENCES integration_connections(id) ON DELETE CASCADE,
    payload       jsonb       NOT NULL,
    received_at   timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (id)
);

-- Per-connection log + keyset pagination.
CREATE INDEX integration_webhook_deliveries_connection_idx
    ON integration_webhook_deliveries (connection_id, received_at DESC);

-- Retention sweep.
CREATE INDEX integration_webhook_deliveries_received_at_idx
    ON integration_webhook_deliveries (received_at);

CREATE TABLE integration_events (
    id                   uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id            uuid        NOT NULL,
    connection_id        uuid        NOT NULL REFERENCES integration_connections(id) ON DELETE CASCADE,
    event_type           text        NOT NULL CHECK (event_type IN (
        'connected',
        'config_updated',
        'secret_rotated',
        'disconnected',
        'delivery_accepted',
        'delivery_rejected'
    )),
    outcome              text        NOT NULL CHECK (outcome IN ('success', 'failure')),
    reason               text        CHECK (reason IN (
        'invalid_signature',
        'inactive_connection',
        'payload_too_large',
        'rate_limited',
        'malformed_payload'
    )),
    actor_membership_id  uuid        REFERENCES tenant_memberships(id),
    created_at           timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (id)
);

-- Keyset pagination + status derivation (newest 3 in 24h).
CREATE INDEX integration_events_connection_keyset_idx
    ON integration_events (connection_id, created_at DESC, id DESC);

-- Retention sweep.
CREATE INDEX integration_events_created_at_idx
    ON integration_events (created_at);

-- Tenant isolation.
CREATE INDEX integration_events_tenant_idx
    ON integration_events (tenant_id);

-- Seed catalog: one connectable entry + three "coming soon" placeholders.
INSERT INTO integration_catalog (slug, name, description, category, is_available, config_schema) VALUES
    (
        'generic-webhook',
        'Generic Webhook',
        'Receive events from any system that can send signed webhooks.',
        'automation',
        true,
        '[
            { "key": "source_label",  "label": "Source label",  "kind": "text",   "required": true },
            { "key": "signing_secret", "label": "Signing secret", "kind": "secret", "required": true }
        ]'::jsonb
    ),
    (
        'slack',
        'Slack',
        'Coming soon.',
        'messaging',
        false,
        '[]'::jsonb
    ),
    (
        'microsoft-teams',
        'Microsoft Teams',
        'Coming soon.',
        'messaging',
        false,
        '[]'::jsonb
    ),
    (
        'crm',
        'CRM',
        'Coming soon.',
        'crm',
        false,
        '[]'::jsonb
    );
