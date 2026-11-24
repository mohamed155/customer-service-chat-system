SET LOCAL ROLE app_platform;
CREATE OR REPLACE FUNCTION xtenant.privacy_claim(p_token uuid)
RETURNS TABLE (tenant_id uuid, request_id uuid)
LANGUAGE plpgsql SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
    WITH exhausted AS (
        SELECT r.tenant_id, r.id FROM privacy_requests r
        WHERE r.status = 'running' AND r.lease_until <= now() AND r.attempts >= 5
        FOR UPDATE SKIP LOCKED LIMIT 10
    )
    UPDATE privacy_requests r
       SET status = 'failed', failure_code = 'lease_exhausted',
           lease_token = NULL, lease_until = NULL, updated_at = now()
      FROM exhausted e WHERE r.tenant_id = e.tenant_id AND r.id = e.id;
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

RESET ROLE;

-- Existing jobs can hold notification payloads in memory when erasure starts.
-- Serialize fan-out with the pending transition before a notification is persisted.
CREATE FUNCTION suppress_erasing_customer_notification() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    conversation uuid;
    inactive boolean;
BEGIN
    CASE NEW.subject_type
        WHEN 'conversation' THEN conversation := NEW.subject_id;
        WHEN 'escalation' THEN
            SELECT e.conversation_id INTO conversation FROM escalations e
            WHERE e.tenant_id = NEW.tenant_id AND e.id = NEW.subject_id;
        WHEN 'tool_request' THEN
            SELECT t.conversation_id INTO conversation FROM tool_requests t
            WHERE t.tenant_id = NEW.tenant_id AND t.id = NEW.subject_id;
        ELSE RETURN NEW;
    END CASE;

    SELECT (c.erasure_pending_at IS NOT NULL OR c.erased_at IS NOT NULL)
      INTO inactive
      FROM conversations a JOIN customers c ON c.tenant_id = a.tenant_id AND c.id = a.customer_id
     WHERE a.tenant_id = NEW.tenant_id AND a.id = conversation
     FOR SHARE OF c;
    IF inactive THEN
        RETURN NULL;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER suppress_erasing_customer_notification BEFORE INSERT ON notifications
    FOR EACH ROW EXECUTE FUNCTION suppress_erasing_customer_notification();

-- Synchronous and pending tool results are kept for routing metadata. A result arriving after
-- erasure must not repopulate the argument, result, or error fields on that tombstone.
CREATE FUNCTION scrub_erasing_customer_tool_update() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    inactive boolean;
BEGIN
    SELECT (c.erasure_pending_at IS NOT NULL OR c.erased_at IS NOT NULL)
      INTO inactive
      FROM conversations a JOIN customers c ON c.tenant_id = a.tenant_id AND c.id = a.customer_id
     WHERE a.tenant_id = NEW.tenant_id AND a.id = NEW.conversation_id
     FOR SHARE OF c;
    IF inactive THEN
        NEW.arguments := '{}'::jsonb;
        NEW.result := NULL;
        NEW.error := NULL;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER scrub_erasing_customer_tool_update
    BEFORE UPDATE OF arguments, result, error ON tool_requests
    FOR EACH ROW EXECUTE FUNCTION scrub_erasing_customer_tool_update();
