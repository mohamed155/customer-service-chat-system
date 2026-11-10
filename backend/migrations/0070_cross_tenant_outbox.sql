-- Migration 0070: the outbox queue as audited cross-tenant functions (MOH-46, part 2: MOH-54).
-- Conventions are in 0067 and 0068.
--
-- A background worker claims the oldest `outbox_events` row across all tenants, then processes,
-- retries or acknowledges it. With Row-Level Security on, a claim without a tenant sees nothing,
-- so the queue handling moves into functions. Each one keeps exactly the claim, lease and attempt
-- rules the worker used when it ran the SQL itself (FOR UPDATE SKIP LOCKED included); the
-- processing of a claimed event already runs in that event's tenant and is not touched.
--
-- Audit: a poll that finds nothing writes nothing. A claim, acknowledgement or retry writes at
-- most one row per function and event type per minute (the resource is the event type, not the
-- event id): the outbox already holds one row per event, and a row per event here would roughly
-- triple the write volume of every message.

SET LOCAL ROLE app_platform;

-- The common claim: oldest unclaimed event of the given types.
-- Used by the AI responder, the escalation consumer and the notification consumer.
CREATE FUNCTION xtenant.outbox_claim(p_event_types text[], p_claim_token uuid)
RETURNS TABLE (id uuid, tenant_id text, aggregate_id text, event_type text, payload jsonb)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  UPDATE outbox_events oe
     SET claimed_at = now(), claim_token = p_claim_token
   WHERE oe.id = (SELECT e.id FROM outbox_events e
                   WHERE e.event_type = ANY (p_event_types)
                     AND e.claimed_at IS NULL
                   ORDER BY e.created_at ASC
                   LIMIT 1
                   FOR UPDATE SKIP LOCKED)
  RETURNING oe.id, oe.tenant_id, oe.aggregate_id, oe.event_type, oe.payload INTO r;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('outbox_claim', NULL, NULL, r.event_type, '{}'::jsonb, interval '1 minute');
  RETURN QUERY SELECT r.id, r.tenant_id, r.aggregate_id, r.event_type, r.payload;
END
$$;

-- The knowledge indexer's claim: also honours the back-off (`available_at`) and dead-lettering,
-- and counts the attempt.
CREATE FUNCTION xtenant.outbox_claim_retryable(p_event_type text, p_claim_token uuid)
RETURNS TABLE (id uuid, tenant_id text, aggregate_id text, payload jsonb, attempts integer)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  UPDATE outbox_events oe
     SET claimed_at = now(), claim_token = p_claim_token, attempts = oe.attempts + 1
   WHERE oe.id = (SELECT e.id FROM outbox_events e
                   WHERE e.event_type = p_event_type
                     AND e.claimed_at IS NULL
                     AND e.available_at <= now()
                     AND e.dead_lettered_at IS NULL
                   ORDER BY e.created_at ASC
                   LIMIT 1
                   FOR UPDATE SKIP LOCKED)
  RETURNING oe.id, oe.tenant_id, oe.aggregate_id, oe.payload, oe.attempts INTO r;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('outbox_claim_retryable', NULL, NULL, p_event_type, '{}'::jsonb, interval '1 minute');
  RETURN QUERY SELECT r.id, r.tenant_id, r.aggregate_id, r.payload, r.attempts;
END
$$;

-- Done with an event: remove it. Returns whether it existed.
CREATE FUNCTION xtenant.outbox_ack(p_id uuid)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  v_type text;
BEGIN
  DELETE FROM outbox_events WHERE id = p_id RETURNING event_type INTO v_type;
  IF NOT FOUND THEN
    RETURN false;
  END IF;
  PERFORM xtenant._audit('outbox_ack', NULL, NULL, v_type, '{}'::jsonb, interval '1 minute');
  RETURN true;
END
$$;

-- Give a claimed event back so another poll picks it up.
CREATE FUNCTION xtenant.outbox_release(p_id uuid)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  v_type text;
BEGIN
  UPDATE outbox_events SET claimed_at = NULL, claim_token = NULL
   WHERE id = p_id RETURNING event_type INTO v_type;
  IF NOT FOUND THEN
    RETURN false;
  END IF;
  PERFORM xtenant._audit('outbox_release', NULL, NULL, v_type, '{}'::jsonb, interval '1 minute');
  RETURN true;
END
$$;

-- The indexer's retry: release the event (only if the caller still holds the claim) and set when
-- it becomes available again.
CREATE FUNCTION xtenant.outbox_reschedule(
  p_id uuid, p_claim_token uuid, p_available_at timestamptz, p_error text
) RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  v_type text;
BEGIN
  UPDATE outbox_events
     SET claimed_at = NULL, claim_token = NULL, available_at = p_available_at, last_error = p_error
   WHERE id = p_id AND claim_token = p_claim_token
  RETURNING event_type INTO v_type;
  IF NOT FOUND THEN
    RETURN false;
  END IF;
  PERFORM xtenant._audit('outbox_reschedule', NULL, NULL, v_type, '{}'::jsonb, interval '1 minute');
  RETURN true;
END
$$;

-- AI responder, claim-time coalescing: drop older unclaimed customer messages of the same
-- conversation (their content is already in the history the engine loads). Returns how many.
CREATE FUNCTION xtenant.outbox_coalesce_customer_messages(p_conversation_id uuid, p_keep_id uuid)
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  n bigint;
BEGIN
  DELETE FROM outbox_events
   WHERE event_type = 'conversation.customer_message'
     AND claimed_at IS NULL
     AND payload->>'conversation_id' = p_conversation_id::text
     AND id != p_keep_id
     AND created_at < (SELECT created_at FROM outbox_events WHERE id = p_keep_id);
  GET DIAGNOSTICS n = ROW_COUNT;
  IF n > 0 THEN
    PERFORM xtenant._audit('outbox_coalesce_customer_messages', NULL, NULL,
                           'conversation.customer_message', jsonb_build_object('removed', n),
                           interval '1 minute');
  END IF;
  RETURN n;
END
$$;

-- WhatsApp sender: take (claim and remove) the oldest outbound message event.
CREATE FUNCTION xtenant.outbox_take_whatsapp_outbound()
RETURNS TABLE (id uuid, tenant_id uuid, conversation_id uuid, message_id uuid)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  WITH claimed AS (
    SELECT e.id, e.tenant_id,
           (e.payload->>'conversationId')::uuid AS conversation_id,
           (e.payload->>'messageId')::uuid AS message_id
      FROM outbox_events e
     WHERE e.event_type = 'whatsapp.outbound_message'
     ORDER BY e.created_at ASC
     LIMIT 1
     FOR UPDATE SKIP LOCKED
  )
  DELETE FROM outbox_events oe USING claimed c WHERE oe.id = c.id
  RETURNING oe.id, oe.tenant_id::uuid AS tenant_id, c.conversation_id, c.message_id INTO r;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('outbox_take_whatsapp_outbound', NULL, NULL,
                         'whatsapp.outbound_message', '{}'::jsonb, interval '1 minute');
  RETURN QUERY SELECT r.id, r.tenant_id, r.conversation_id, r.message_id;
END
$$;

-- Invitation e-mail delivery. Four steps, each one atomic in the function (the worker used to
-- open a transaction per step): give up on events that used all their attempts, claim the next
-- one, schedule a retry, and finish one together with the invitation's delivery status.
CREATE FUNCTION xtenant.invitation_delivery_expire_exhausted(p_max_attempts integer)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  r record;
BEGIN
  UPDATE outbox_events event
     SET processed_at = now(), dead_lettered_at = now(),
         last_error = 'delivery claim expired after maximum attempts',
         claimed_at = NULL, claim_token = NULL
    FROM (SELECT e.id FROM outbox_events e
           WHERE e.event_type = 'invitation.email_delivery'
             AND e.processed_at IS NULL AND e.dead_lettered_at IS NULL
             AND e.attempts >= p_max_attempts
             AND (e.claimed_at IS NULL OR e.claimed_at <= now() - interval '5 minutes')
           ORDER BY e.created_at, e.id FOR UPDATE SKIP LOCKED LIMIT 1) exhausted
   WHERE event.id = exhausted.id
  RETURNING event.aggregate_id, event.tenant_id INTO r;
  IF NOT FOUND THEN
    RETURN false;
  END IF;
  UPDATE tenant_invitations
     SET email_delivery_status = 'failed', email_delivery_error = 'Max retries exceeded'
   WHERE id::text = r.aggregate_id AND tenant_id::text = r.tenant_id
     AND email_delivery_status = 'queued';
  PERFORM xtenant._audit('invitation_delivery_expire_exhausted', NULL, NULL,
                         'invitation.email_delivery', '{}'::jsonb, interval '1 minute');
  RETURN true;
END
$$;

CREATE FUNCTION xtenant.invitation_delivery_claim(p_claim_token uuid, p_max_attempts integer)
RETURNS TABLE (id uuid, aggregate_id text, tenant_id text, payload jsonb, attempts integer)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  UPDATE outbox_events event
     SET claimed_at = now(), claim_token = p_claim_token, attempts = event.attempts + 1
    FROM (SELECT e.id FROM outbox_events e
           WHERE e.event_type = 'invitation.email_delivery'
             AND e.processed_at IS NULL AND e.dead_lettered_at IS NULL
             AND e.available_at <= now() AND e.attempts < p_max_attempts
             AND (e.claimed_at IS NULL OR e.claimed_at <= now() - interval '5 minutes')
           ORDER BY e.available_at, e.created_at, e.id FOR UPDATE SKIP LOCKED LIMIT 1) claimable
   WHERE event.id = claimable.id
  RETURNING event.id, event.aggregate_id, event.tenant_id, event.payload, event.attempts INTO r;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('invitation_delivery_claim', NULL, NULL,
                         'invitation.email_delivery', '{}'::jsonb, interval '1 minute');
  RETURN QUERY SELECT r.id, r.aggregate_id, r.tenant_id, r.payload, r.attempts;
END
$$;

CREATE FUNCTION xtenant.invitation_delivery_retry(p_id uuid, p_claim_token uuid, p_error text)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  UPDATE outbox_events
     SET claimed_at = NULL, claim_token = NULL,
         available_at = now() + interval '1 second', last_error = p_error
   WHERE id = p_id AND claim_token = p_claim_token
     AND event_type = 'invitation.email_delivery';
  IF NOT FOUND THEN
    RETURN false;
  END IF;
  PERFORM xtenant._audit('invitation_delivery_retry', NULL, NULL,
                         'invitation.email_delivery', '{}'::jsonb, interval '1 minute');
  RETURN true;
END
$$;

-- Finish one event (an error text marks it dead-lettered) and, when it still holds the claim,
-- record the outcome on the invitation (p_invitation_id NULL: no invitation to update).
CREATE FUNCTION xtenant.invitation_delivery_finalize(
  p_id uuid, p_claim_token uuid, p_error text,
  p_invitation_id uuid, p_tenant_id uuid, p_status text, p_status_error text
) RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  n bigint;
BEGIN
  UPDATE outbox_events
     SET processed_at = now(),
         dead_lettered_at = CASE WHEN p_error IS NULL THEN NULL ELSE now() END,
         last_error = p_error, claimed_at = NULL, claim_token = NULL
   WHERE id = p_id AND claim_token = p_claim_token
     AND event_type = 'invitation.email_delivery';
  GET DIAGNOSTICS n = ROW_COUNT;
  IF n <> 1 THEN
    RETURN false;
  END IF;
  IF p_invitation_id IS NOT NULL THEN
    UPDATE tenant_invitations
       SET email_delivery_status = p_status, email_delivery_error = p_status_error
     WHERE tenant_id = p_tenant_id AND id = p_invitation_id
       AND email_delivery_status = 'queued';
  END IF;
  PERFORM xtenant._audit('invitation_delivery_finalize', NULL, NULL,
                         'invitation.email_delivery', '{}'::jsonb, interval '1 minute');
  RETURN true;
END
$$;

-- WhatsApp media: the next attachment waiting to be fetched (fewer than 5 attempts). It is a
-- read; the worker marks the row stored or failed in the attachment's own tenant afterwards.
CREATE FUNCTION xtenant.pending_attachment_next()
RETURNS TABLE (
  id uuid, tenant_id uuid, message_id uuid, kind text, status text, provider_media_id text,
  storage_key text, mime_type text, size_bytes bigint, file_name text, fetch_attempts integer,
  created_at timestamptz, updated_at timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  SELECT a.id, a.tenant_id, a.message_id, a.kind, a.status, a.provider_media_id, a.storage_key,
         a.mime_type, a.size_bytes, a.file_name, a.fetch_attempts, a.created_at, a.updated_at
    INTO r
    FROM message_attachments a
   WHERE a.status = 'pending' AND a.fetch_attempts < 5
   ORDER BY a.created_at ASC
   LIMIT 1
   FOR UPDATE SKIP LOCKED;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('pending_attachment_next', r.tenant_id, NULL, r.id::text,
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY SELECT r.id, r.tenant_id, r.message_id, r.kind, r.status, r.provider_media_id,
                      r.storage_key, r.mime_type, r.size_bytes, r.file_name, r.fetch_attempts,
                      r.created_at, r.updated_at;
END
$$;

REVOKE ALL ON FUNCTION
  xtenant.outbox_claim(text[], uuid),
  xtenant.outbox_claim_retryable(text, uuid),
  xtenant.outbox_ack(uuid),
  xtenant.outbox_release(uuid),
  xtenant.outbox_reschedule(uuid, uuid, timestamptz, text),
  xtenant.outbox_coalesce_customer_messages(uuid, uuid),
  xtenant.outbox_take_whatsapp_outbound(),
  xtenant.invitation_delivery_expire_exhausted(integer),
  xtenant.invitation_delivery_claim(uuid, integer),
  xtenant.invitation_delivery_retry(uuid, uuid, text),
  xtenant.invitation_delivery_finalize(uuid, uuid, text, uuid, uuid, text, text),
  xtenant.pending_attachment_next()
FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  xtenant.outbox_claim(text[], uuid),
  xtenant.outbox_claim_retryable(text, uuid),
  xtenant.outbox_ack(uuid),
  xtenant.outbox_release(uuid),
  xtenant.outbox_reschedule(uuid, uuid, timestamptz, text),
  xtenant.outbox_coalesce_customer_messages(uuid, uuid),
  xtenant.outbox_take_whatsapp_outbound(),
  xtenant.invitation_delivery_expire_exhausted(integer),
  xtenant.invitation_delivery_claim(uuid, integer),
  xtenant.invitation_delivery_retry(uuid, uuid, text),
  xtenant.invitation_delivery_finalize(uuid, uuid, text, uuid, uuid, text, text),
  xtenant.pending_attachment_next()
TO app_runtime;

RESET ROLE;
