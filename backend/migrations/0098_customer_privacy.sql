ALTER TABLE customers
    ADD COLUMN erasure_pending_at timestamptz,
    ADD COLUMN erased_at timestamptz;

CREATE TABLE privacy_requests (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    customer_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('export', 'erase')),
    status text NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'completed', 'failed', 'expired')),
    requested_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    available_at timestamptz NOT NULL DEFAULT now(),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    lease_token uuid,
    lease_until timestamptz,
    failure_code text,
    object_key text,
    expires_at timestamptz,
    UNIQUE (tenant_id, id),
    FOREIGN KEY (tenant_id, customer_id) REFERENCES customers (tenant_id, id)
);

-- Includes terminal requests so the same customer always gets the same erasure receipt.
CREATE UNIQUE INDEX privacy_one_erasure ON privacy_requests (tenant_id, customer_id)
    WHERE kind = 'erase';
CREATE INDEX privacy_requests_ready ON privacy_requests (available_at, id)
    WHERE status IN ('queued', 'running', 'failed');

CREATE TABLE privacy_request_steps (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    request_id uuid NOT NULL,
    step_name text NOT NULL,
    status text NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, request_id, step_name),
    FOREIGN KEY (tenant_id, request_id) REFERENCES privacy_requests (tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE privacy_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE privacy_requests FORCE ROW LEVEL SECURITY;
CREATE POLICY privacy_requests_tenant ON privacy_requests
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE privacy_request_steps ENABLE ROW LEVEL SECURITY;
ALTER TABLE privacy_request_steps FORCE ROW LEVEL SECURITY;
CREATE POLICY privacy_request_steps_tenant ON privacy_request_steps
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, UPDATE ON privacy_requests TO app_platform;

SET LOCAL ROLE app_platform;
CREATE FUNCTION xtenant.privacy_claim(p_token uuid)
RETURNS TABLE (tenant_id uuid, request_id uuid)
LANGUAGE plpgsql SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
    UPDATE privacy_requests r
       SET status = 'failed', failure_code = 'lease_exhausted',
           lease_token = NULL, lease_until = NULL, updated_at = now()
     WHERE r.status = 'running' AND r.lease_until <= now() AND r.attempts >= 5
       AND r.expires_at IS NULL;
    IF FOUND THEN
        PERFORM xtenant._audit('privacy_claim_exhausted', NULL, NULL, 'privacy', '{}'::jsonb, interval '1 minute');
    END IF;
    RETURN QUERY
    WITH candidate AS (
        SELECT r.id, r.tenant_id FROM privacy_requests r
        WHERE r.available_at <= now() AND
              ((r.status IN ('queued', 'failed') AND r.attempts < 5)
               OR (r.status = 'running' AND r.lease_until <= now() AND (r.attempts < 5 OR r.expires_at <= now()))
               OR (r.status IN ('completed', 'failed') AND r.kind = 'export' AND r.expires_at <= now()))
        ORDER BY r.available_at, r.id
        FOR UPDATE SKIP LOCKED LIMIT 1
    )
    UPDATE privacy_requests r
       SET status = 'running', lease_token = p_token, lease_until = now() + interval '10 minutes',
           updated_at = now(), attempts = r.attempts + 1
      FROM candidate c WHERE r.id = c.id AND r.tenant_id = c.tenant_id
    RETURNING r.tenant_id, r.id;
    IF FOUND THEN
        PERFORM xtenant._audit('privacy_claim', NULL, NULL, 'privacy', '{}'::jsonb, interval '1 minute');
    END IF;
END;
$$;

REVOKE ALL ON FUNCTION xtenant.privacy_claim(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION xtenant.privacy_claim(uuid) TO app_runtime;
RESET ROLE;

-- Serialize new conversation activity with the transition to erasure pending.
-- Scrubbing existing rows remains possible for the owning modules.
CREATE FUNCTION guard_customer_privacy_activity() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    subject uuid;
    inactive boolean;
BEGIN
    IF TG_TABLE_NAME IN ('messages', 'ai_generations', 'escalations', 'tool_requests',
                         'conversation_feedback', 'whatsapp_message_meta') THEN
        SELECT c.customer_id INTO subject FROM conversations c
        WHERE c.tenant_id = NEW.tenant_id AND c.id = NEW.conversation_id;
    ELSIF TG_TABLE_NAME IN ('message_citations', 'message_attachments') THEN
        SELECT c.customer_id INTO subject
        FROM messages m JOIN conversations c
          ON c.tenant_id = m.tenant_id AND c.id = m.conversation_id
        WHERE m.tenant_id = NEW.tenant_id AND m.id = NEW.message_id;
    ELSE
        subject := NEW.customer_id;
    END IF;

    IF subject IS NOT NULL THEN
        -- Lock active customers too, so pending cannot race an accepted insert.
        SELECT (c.erasure_pending_at IS NOT NULL OR c.erased_at IS NOT NULL) INTO inactive
          FROM customers c WHERE c.tenant_id = NEW.tenant_id AND c.id = subject FOR SHARE;
        IF inactive THEN
            RAISE EXCEPTION 'customer activity is unavailable' USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER customer_privacy_conversations BEFORE INSERT ON conversations
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_messages BEFORE INSERT ON messages
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_widget_sessions BEFORE INSERT OR UPDATE OF customer_id ON widget_sessions
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_ai_generations BEFORE INSERT ON ai_generations
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_escalations BEFORE INSERT ON escalations
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_tool_requests BEFORE INSERT ON tool_requests
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_conversation_feedback BEFORE INSERT ON conversation_feedback
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_whatsapp_message_meta BEFORE INSERT ON whatsapp_message_meta
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_message_citations BEFORE INSERT ON message_citations
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
CREATE TRIGGER customer_privacy_message_attachments BEFORE INSERT ON message_attachments
    FOR EACH ROW EXECUTE FUNCTION guard_customer_privacy_activity();
