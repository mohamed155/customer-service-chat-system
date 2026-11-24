-- Validate PostgreSQL's accepted spellings before casting and return one canonical representation.
CREATE FUNCTION privacy_normalized_uuid(text) RETURNS text
LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT
SET search_path = pg_catalog, public, pg_temp AS $$
    SELECT CASE
        WHEN lower($1) ~ '^(\{)?([0-9a-f]{4}-?){7}[0-9a-f]{4}(\})?$'
         AND (left(lower($1), 1) = '{') = (right(lower($1), 1) = '}')
        THEN translate(lower($1), '{}', '')::uuid::text
    END
$$;

REVOKE ALL ON FUNCTION privacy_normalized_uuid(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION privacy_normalized_uuid(text) TO app_runtime, app_platform;

-- Keep the delivery receipt while fencing writes of payloads attributed to an erasing subject.
CREATE FUNCTION scrub_erasing_customer_webhook_delivery() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog, public, pg_temp AS $$
DECLARE
    field_name text;
    raw_id text;
    normalized_id text;
    parsed_id uuid;
    candidate uuid;
    subject uuid;
    inactive boolean;
BEGIN
    FOREACH field_name IN ARRAY ARRAY[
        'customer_id', 'customerId', 'conversation_id', 'conversationId', 'message_id', 'messageId'
    ] LOOP
        raw_id := NEW.payload ->> field_name;
        IF raw_id IS NULL THEN
            CONTINUE;
        END IF;
        normalized_id := privacy_normalized_uuid(raw_id);
        IF normalized_id IS NULL THEN
            RETURN NEW;
        END IF;
        parsed_id := normalized_id::uuid;

        CASE field_name
            WHEN 'customer_id', 'customerId' THEN
                candidate := parsed_id;
            WHEN 'conversation_id', 'conversationId' THEN
                SELECT c.customer_id INTO candidate
                  FROM conversations c
                 WHERE c.tenant_id = NEW.tenant_id AND c.id = parsed_id;
            WHEN 'message_id', 'messageId' THEN
                SELECT c.customer_id INTO candidate
                  FROM messages m JOIN conversations c
                    ON c.tenant_id = m.tenant_id AND c.id = m.conversation_id
                 WHERE m.tenant_id = NEW.tenant_id AND m.id = parsed_id;
        END CASE;
        -- Every supplied identifier must resolve to the same customer.
        IF candidate IS NULL OR (subject IS NOT NULL AND subject <> candidate) THEN
            RETURN NEW;
        END IF;
        subject := candidate;
    END LOOP;

    IF subject IS NOT NULL THEN
        -- Lock active customers too, serializing accepted deliveries with the pending transition.
        SELECT (c.erasure_pending_at IS NOT NULL OR c.erased_at IS NOT NULL)
          INTO inactive
          FROM customers c
         WHERE c.tenant_id = NEW.tenant_id AND c.id = subject
         FOR SHARE;
        IF inactive THEN
            NEW.payload := '{}'::jsonb;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER scrub_erasing_customer_webhook_delivery
    BEFORE INSERT ON integration_webhook_deliveries
    FOR EACH ROW EXECUTE FUNCTION scrub_erasing_customer_webhook_delivery();
