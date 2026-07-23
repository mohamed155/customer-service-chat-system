-- Migration 0057: WhatsApp Channel — message meta, attachments, catalog seed
--
-- See specs/029-whatsapp-channel/data-model.md for the full design.

-- 1. UNIQUE constraint on messages for composite FK target

DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'messages_tenant_id_id_uq'
    ) THEN
        ALTER TABLE messages
            ADD CONSTRAINT messages_tenant_id_id_uq UNIQUE (tenant_id, id);
    END IF;
END $$;

-- 2. whatsapp_message_meta — channel-specific side record per message

CREATE TABLE whatsapp_message_meta (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         UUID        NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    message_id        UUID        NOT NULL,
    conversation_id   UUID        NOT NULL,
    direction         TEXT        NOT NULL CHECK (direction IN ('inbound','outbound')),
    wamid             TEXT        NULL,
    provider_timestamp TIMESTAMPTZ NULL,
    delivery_status   TEXT        NULL CHECK (delivery_status IN ('pending','sent','delivered','read','failed')),
    failure_reason    TEXT        NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT whatsapp_message_meta_message_fkey
        FOREIGN KEY (tenant_id, message_id)
        REFERENCES messages (tenant_id, id),

    CONSTRAINT whatsapp_message_meta_conversation_fkey
        FOREIGN KEY (tenant_id, conversation_id)
        REFERENCES conversations (tenant_id, id),

    CONSTRAINT whatsapp_message_meta_direction_check
        CHECK (
            (direction = 'inbound' AND delivery_status IS NULL)
            OR (direction = 'outbound' AND delivery_status IS NOT NULL)
        )
);

CREATE UNIQUE INDEX whatsapp_message_meta_wamid_uq
    ON whatsapp_message_meta (tenant_id, wamid)
    WHERE wamid IS NOT NULL;

CREATE UNIQUE INDEX whatsapp_message_meta_message_id_uq
    ON whatsapp_message_meta (message_id);

CREATE INDEX whatsapp_message_meta_conversation_idx
    ON whatsapp_message_meta (tenant_id, conversation_id);

CREATE OR REPLACE FUNCTION set_whatsapp_message_meta_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_whatsapp_message_meta_updated_at
    BEFORE UPDATE ON whatsapp_message_meta
    FOR EACH ROW
    EXECUTE FUNCTION set_whatsapp_message_meta_updated_at();

-- 3. message_attachments — channel-generic stored media

CREATE TABLE message_attachments (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         UUID        NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    message_id        UUID        NOT NULL,
    kind              TEXT        NOT NULL CHECK (kind IN ('image','audio','video','document')),
    status            TEXT        NOT NULL CHECK (status IN ('pending','stored','failed')) DEFAULT 'pending',
    provider_media_id TEXT        NULL,
    storage_key       TEXT        NULL,
    mime_type         TEXT        NULL,
    size_bytes        BIGINT      NULL,
    file_name         TEXT        NULL,
    fetch_attempts    INT         NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT message_attachments_message_fkey
        FOREIGN KEY (tenant_id, message_id)
        REFERENCES messages (tenant_id, id),

    CONSTRAINT message_attachments_status_check
        CHECK (
            (status = 'stored' AND storage_key IS NOT NULL AND mime_type IS NOT NULL)
            OR status IN ('pending', 'failed')
        )
);

CREATE INDEX message_attachments_message_idx
    ON message_attachments (tenant_id, message_id);

CREATE INDEX message_attachments_pending_idx
    ON message_attachments (status, updated_at)
    WHERE status = 'pending';

CREATE OR REPLACE FUNCTION set_message_attachments_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_message_attachments_updated_at
    BEFORE UPDATE ON message_attachments
    FOR EACH ROW
    EXECUTE FUNCTION set_message_attachments_updated_at();

-- 4. Seed integration_catalog for whatsapp

INSERT INTO integration_catalog (slug, name, description, category, is_available, config_schema) VALUES
    (
        'whatsapp',
        'WhatsApp',
        'Send and receive messages through Meta''s WhatsApp Business Cloud API.',
        'messaging',
        true,
        '[
            { "key": "phone_number_id",  "label": "Phone Number ID",  "kind": "text",   "required": true },
            { "key": "business_phone",   "label": "Business Phone",   "kind": "text",   "required": true },
            { "key": "access_token",     "label": "Access Token",     "kind": "secret", "required": true },
            { "key": "app_secret",       "label": "App Secret",       "kind": "secret", "required": true },
            { "key": "verify_token",     "label": "Verify Token",     "kind": "secret", "required": true }
        ]'::jsonb
    )
ON CONFLICT (slug) DO NOTHING;
