-- Migration 0071: the sweeps and the platform routes as audited cross-tenant functions
-- (MOH-46, part 3: MOH-55). Conventions are in 0067 and 0068; the outbox is 0070.
--
-- Sweeps delete or revert rows in every tenant. Platform routes read across tenants or write the
-- platform-wide rows (tenant_id IS NULL) that no tenant owns. Each is one narrow function with
-- the SQL its caller ran before, and the same audit row the route wrote. The tenants,
-- billing_plans, users and integration_catalog tables have no tenant_id and stay out of RLS, so
-- a query on only those (the tenant counts) needs no function.
--
-- Audit:
--   - a sweep writes one row per run with the count of rows it changed, and none when it changed
--     nothing;
--   - a platform write writes the domain audit row it always wrote (ai_config.updated,
--     ai_credential.set/deleted, billing_subscription.changed), with no tenant or the tenant it
--     changed;
--   - a platform read writes at most one row per function per hour (the dashboards poll).

SET LOCAL ROLE app_platform;

-- ---------------------------------------------------------------------------------------------
-- Sweeps
-- ---------------------------------------------------------------------------------------------

-- Integration retention: events and accepted webhook deliveries older than 90 days.
CREATE FUNCTION xtenant.integration_retention_sweep()
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  n_events bigint;
  n_deliveries bigint;
BEGIN
  DELETE FROM integration_events WHERE created_at < now() - interval '90 days';
  GET DIAGNOSTICS n_events = ROW_COUNT;
  DELETE FROM integration_webhook_deliveries WHERE received_at < now() - interval '90 days';
  GET DIAGNOSTICS n_deliveries = ROW_COUNT;
  IF n_events + n_deliveries > 0 THEN
    PERFORM xtenant._audit('integration_retention_sweep', NULL, NULL, 'integration_retention',
                           jsonb_build_object('events', n_events, 'deliveries', n_deliveries));
  END IF;
  RETURN n_events + n_deliveries;
END
$$;

-- Notification retention: notifications older than 90 days.
CREATE FUNCTION xtenant.notification_retention_sweep()
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  n bigint;
BEGIN
  DELETE FROM notifications WHERE created_at < now() - interval '90 days';
  GET DIAGNOSTICS n = ROW_COUNT;
  IF n > 0 THEN
    PERFORM xtenant._audit('notification_retention_sweep', NULL, NULL, 'notification_retention',
                           jsonb_build_object('deleted', n));
  END IF;
  RETURN n;
END
$$;

-- Widget sessions past their expiry.
CREATE FUNCTION xtenant.widget_session_sweep()
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  n bigint;
BEGIN
  DELETE FROM widget_sessions WHERE expires_at <= now();
  GET DIAGNOSTICS n = ROW_COUNT;
  IF n > 0 THEN
    PERFORM xtenant._audit('widget_session_sweep', NULL, NULL, 'widget_session_expiry',
                           jsonb_build_object('deleted', n));
  END IF;
  RETURN n;
END
$$;

-- Tool approvals that waited past their deadline: mark them expired and queue the decision event
-- for each, in one transaction as before. The event is the one conversations::outbox emits
-- (`ai.tool_decision`, outcome "expired"); the AI responder reads it.
CREATE FUNCTION xtenant.tool_request_expiry_sweep()
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  r record;
  n bigint := 0;
BEGIN
  FOR r IN
    UPDATE tool_requests
       SET status = 'expired', decided_at = now()
     WHERE status = 'awaiting_approval' AND expires_at < now()
    RETURNING id, tenant_id, conversation_id
  LOOP
    INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at)
    VALUES (gen_random_uuid(), 'conversation', r.conversation_id::text, r.tenant_id::text,
            'ai.tool_decision',
            jsonb_build_object('tenantId', r.tenant_id, 'conversationId', r.conversation_id,
                               'toolRequestId', r.id, 'outcome', 'expired'),
            now());
    n := n + 1;
  END LOOP;
  IF n > 0 THEN
    PERFORM xtenant._audit('tool_request_expiry_sweep', NULL, NULL, 'tool_request_expiry',
                           jsonb_build_object('expired', n));
  END IF;
  RETURN n;
END
$$;

-- Escalation presence, at server start: every agent still marked available, in every tenant, so
-- the server can schedule their revert to away.
CREATE FUNCTION xtenant.available_agents()
RETURNS TABLE (tenant_id uuid, membership_id uuid)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
BEGIN
  RETURN QUERY
    SELECT a.tenant_id, a.membership_id FROM agent_availability a WHERE a.state = 'available';
  IF FOUND THEN
    PERFORM xtenant._audit('available_agents', NULL, NULL, 'presence_startup_sweep',
                           '{}'::jsonb, interval '1 minute');
  END IF;
END
$$;

-- ---------------------------------------------------------------------------------------------
-- Platform dashboards (aggregates over every tenant)
-- ---------------------------------------------------------------------------------------------

CREATE FUNCTION xtenant.platform_active_conversation_count()
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  n bigint;
BEGIN
  SELECT COUNT(*)::bigint INTO n FROM conversations
   WHERE status IN ('open', 'pending') AND deleted_at IS NULL;
  PERFORM xtenant._audit('platform_active_conversation_count', NULL, NULL, 'platform_dashboard',
                         '{}'::jsonb, interval '1 hour');
  RETURN n;
END
$$;

CREATE FUNCTION xtenant.platform_ai_usage_totals(p_from timestamptz, p_to timestamptz)
RETURNS TABLE (total bigint, successes bigint, failures bigint, input_tokens bigint, output_tokens bigint)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  PERFORM xtenant._audit('platform_ai_usage_totals', NULL, NULL, 'platform_dashboard',
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY
    SELECT COUNT(*)::bigint,
           COUNT(*) FILTER (WHERE r.status = 'success')::bigint,
           COUNT(*) FILTER (WHERE r.status = 'failure')::bigint,
           COALESCE(SUM(r.input_tokens), 0)::bigint,
           COALESCE(SUM(r.output_tokens), 0)::bigint
      FROM ai_usage_records r
     WHERE r.created_at >= p_from AND r.created_at < p_to;
END
$$;

-- Outbox backlog. The server polls it every 15 seconds, so this one is throttled to a row an hour.
CREATE FUNCTION xtenant.platform_queue_health()
RETURNS TABLE (pending bigint, oldest timestamptz, stuck bigint)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  PERFORM xtenant._audit('platform_queue_health', NULL, NULL, 'platform_dashboard',
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY
    SELECT COUNT(*)::bigint, MIN(e.created_at), COUNT(*) FILTER (WHERE e.attempts > 5)::bigint
      FROM outbox_events e WHERE e.processed_at IS NULL;
END
$$;

CREATE FUNCTION xtenant.platform_provider_metrics(p_from timestamptz, p_to timestamptz)
RETURNS TABLE (
  provider text, total bigint, successes bigint, failures bigint, input_tokens bigint,
  output_tokens bigint, p50_latency_ms bigint, p95_latency_ms bigint, authentication bigint,
  rate_limited bigint, unavailable bigint, timeout bigint, invalid_request bigint
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  PERFORM xtenant._audit('platform_provider_metrics', NULL, NULL, 'platform_dashboard',
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY
    SELECT r.provider::text,
           COUNT(*)::bigint,
           COUNT(*) FILTER (WHERE r.status = 'success')::bigint,
           COUNT(*) FILTER (WHERE r.status = 'failure')::bigint,
           COALESCE(SUM(r.input_tokens), 0)::bigint,
           COALESCE(SUM(r.output_tokens), 0)::bigint,
           (percentile_cont(0.5)  WITHIN GROUP (ORDER BY r.latency_ms::float8))::bigint,
           (percentile_cont(0.95) WITHIN GROUP (ORDER BY r.latency_ms::float8))::bigint,
           COUNT(*) FILTER (WHERE r.error_category = 'authentication')::bigint,
           COUNT(*) FILTER (WHERE r.error_category = 'rate_limited')::bigint,
           COUNT(*) FILTER (WHERE r.error_category = 'unavailable')::bigint,
           COUNT(*) FILTER (WHERE r.error_category = 'timeout')::bigint,
           COUNT(*) FILTER (WHERE r.error_category = 'invalid_request')::bigint
      FROM ai_usage_records r
     WHERE r.created_at >= p_from AND r.created_at < p_to
     GROUP BY r.provider;
END
$$;

CREATE FUNCTION xtenant.platform_recent_provider_failures(p_from timestamptz, p_to timestamptz)
RETURNS TABLE (provider text, model text, error_category text, latency_ms integer, created_at timestamptz)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  PERFORM xtenant._audit('platform_recent_provider_failures', NULL, NULL, 'platform_dashboard',
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY
    SELECT r.provider::text, r.model::text, r.error_category::text, r.latency_ms, r.created_at
      FROM ai_usage_records r
     WHERE r.status = 'failure' AND r.created_at >= p_from AND r.created_at < p_to
     ORDER BY r.created_at DESC
     LIMIT 50;
END
$$;

-- ---------------------------------------------------------------------------------------------
-- Platform audit listing (every tenant's entries, plus the platform's own)
-- ---------------------------------------------------------------------------------------------

CREATE FUNCTION xtenant.platform_audit_entries(
  p_requested_by  uuid,
  p_tenant_id     uuid,
  p_from          timestamptz,
  p_to            timestamptz,
  p_prefixes      text[],
  p_actor_id      uuid,
  p_cursor_ts     timestamptz,
  p_cursor_id     uuid,
  p_limit         bigint
) RETURNS TABLE (
  id uuid, action text, actor_user_id uuid, resource_type text, resource_id text,
  tenant_id uuid, details jsonb, created_at timestamptz, display_name text, email text,
  platform_role text, deleted_at timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  PERFORM xtenant._audit('platform_audit_entries', NULL, p_requested_by,
                         COALESCE(p_requested_by::text, 'platform'), '{}'::jsonb, interval '1 hour');
  RETURN QUERY
    SELECT a.id, a.action, a.actor_user_id, a.resource_type, a.resource_id, a.tenant_id, a.details,
           a.created_at, u.display_name, u.email::text, u.platform_role, u.deleted_at
      FROM audit_logs a
      LEFT JOIN users u ON u.id = a.actor_user_id
     WHERE (p_tenant_id IS NULL OR a.tenant_id = p_tenant_id)
       AND (p_from IS NULL OR a.created_at >= p_from)
       AND (p_to IS NULL OR a.created_at <= p_to)
       AND (p_prefixes IS NULL OR a.action LIKE ANY (p_prefixes))
       AND (p_actor_id IS NULL OR a.actor_user_id = p_actor_id)
       AND (p_cursor_ts IS NULL OR (a.created_at, a.id) < (p_cursor_ts, p_cursor_id))
     ORDER BY a.created_at DESC, a.id DESC
     LIMIT p_limit;
END
$$;

-- ---------------------------------------------------------------------------------------------
-- Billing subscriptions managed by a platform principal
-- ---------------------------------------------------------------------------------------------

CREATE FUNCTION xtenant.platform_subscriptions_list()
RETURNS TABLE (
  tenant_id uuid, tenant_name text, plan_id uuid, plan_name text, plan_is_default boolean,
  status text, explicit boolean
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  PERFORM xtenant._audit('platform_subscriptions_list', NULL, NULL, 'platform_billing',
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY
    SELECT t.id, t.name, COALESCE(p.id, d.id), COALESCE(p.name, d.name),
           COALESCE(p.is_default, d.is_default), COALESCE(s.status, 'active'),
           (s.id IS NOT NULL)
      FROM tenants t
      LEFT JOIN tenant_subscriptions s ON s.tenant_id = t.id
      LEFT JOIN billing_plans p ON p.id = s.plan_id
      LEFT JOIN LATERAL (SELECT bp.id, bp.name, bp.is_default FROM billing_plans bp
                          WHERE bp.is_default LIMIT 1) d ON true
     WHERE t.deleted_at IS NULL
     ORDER BY t.name;
END
$$;

-- Assign a plan and status to a tenant. `outcome` is 'ok', 'plan_not_found' or 'retired_plan'
-- (a retired plan can only be kept, not newly assigned); the rest of the row is the new state.
-- Writes the billing_subscription.changed audit row, for the tenant, as the route always did.
CREATE FUNCTION xtenant.platform_subscription_set(
  p_actor_id uuid, p_tenant_id uuid, p_plan_id uuid, p_status text
) RETURNS TABLE (outcome text, subscription_id uuid, plan_id uuid, status text)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  v_plan_status text;
  v_before record;
  v_id uuid;
BEGIN
  SELECT bp.status INTO v_plan_status FROM billing_plans bp WHERE bp.id = p_plan_id FOR UPDATE;
  IF NOT FOUND THEN
    RETURN QUERY SELECT 'plan_not_found'::text, NULL::uuid, NULL::uuid, NULL::text;
    RETURN;
  END IF;

  SELECT s.id, s.plan_id, s.status INTO v_before
    FROM tenant_subscriptions s WHERE s.tenant_id = p_tenant_id FOR UPDATE;

  IF v_plan_status = 'retired' AND (NOT FOUND OR v_before.plan_id <> p_plan_id) THEN
    RETURN QUERY SELECT 'retired_plan'::text, NULL::uuid, NULL::uuid, NULL::text;
    RETURN;
  END IF;

  INSERT INTO tenant_subscriptions (tenant_id, plan_id, status, assigned_by)
  VALUES (p_tenant_id, p_plan_id, p_status, p_actor_id)
  ON CONFLICT (tenant_id) DO UPDATE SET
    plan_id = EXCLUDED.plan_id, status = EXCLUDED.status, assigned_by = EXCLUDED.assigned_by
  RETURNING id INTO v_id;

  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (NULLIF(p_actor_id, '00000000-0000-0000-0000-000000000000'::uuid),
          'billing_subscription.changed', 'billing_subscription', v_id::text, p_tenant_id,
          jsonb_build_object('plan_id', p_plan_id, 'status', p_status,
                             'previous_plan_id', v_before.plan_id,
                             'previous_status', v_before.status));

  RETURN QUERY SELECT 'ok'::text, v_id, p_plan_id, p_status;
END
$$;

-- ---------------------------------------------------------------------------------------------
-- Platform-wide AI configuration and credentials (rows with tenant_id IS NULL)
-- ---------------------------------------------------------------------------------------------

-- The platform default configuration, read when there is no tenant (the platform's own view).
CREATE FUNCTION xtenant.platform_ai_config()
RETURNS SETOF ai_configurations
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  PERFORM xtenant._audit('platform_ai_config', NULL, NULL, 'platform_ai',
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY
    SELECT * FROM ai_configurations c WHERE c.tenant_id IS NULL AND c.deleted_at IS NULL LIMIT 1;
END
$$;

CREATE FUNCTION xtenant.platform_ai_config_set(
  p_actor_id uuid, p_provider text, p_model text, p_max_output_tokens integer,
  p_temperature real, p_fallbacks jsonb, p_details jsonb
) RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  v_id uuid;
BEGIN
  SELECT c.id INTO v_id FROM ai_configurations c
   WHERE c.tenant_id IS NULL AND c.deleted_at IS NULL FOR UPDATE;
  IF FOUND THEN
    UPDATE ai_configurations
       SET provider = p_provider, model = p_model, max_output_tokens = p_max_output_tokens,
           temperature = p_temperature, fallbacks = p_fallbacks, updated_at = now()
     WHERE id = v_id;
  ELSE
    INSERT INTO ai_configurations (provider, model, max_output_tokens, temperature, fallbacks)
    VALUES (p_provider, p_model, p_max_output_tokens, p_temperature, p_fallbacks)
    RETURNING id INTO v_id;
  END IF;
  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (NULLIF(p_actor_id, '00000000-0000-0000-0000-000000000000'::uuid),
          'ai_config.updated', 'ai_configuration', v_id::text, NULL, p_details);
  RETURN v_id;
END
$$;

-- The platform credential for a provider: the sealed key, for the AI client to open.
CREATE FUNCTION xtenant.platform_ai_credential_secret(p_provider text)
RETURNS TABLE (ciphertext bytea, nonce bytea)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
BEGIN
  RETURN QUERY
    SELECT k.ciphertext, k.nonce FROM ai_credentials k
     WHERE k.tenant_id IS NULL AND k.provider = p_provider AND k.deleted_at IS NULL LIMIT 1;
  IF FOUND THEN
    PERFORM xtenant._audit('platform_ai_credential_secret', NULL, NULL, p_provider,
                           '{}'::jsonb, interval '1 hour');
  END IF;
END
$$;

-- The same credential's display hint (never the key).
CREATE FUNCTION xtenant.platform_ai_credential_hint(p_provider text)
RETURNS TABLE (key_hint text)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
BEGIN
  RETURN QUERY
    SELECT k.key_hint FROM ai_credentials k
     WHERE k.tenant_id IS NULL AND k.provider = p_provider AND k.deleted_at IS NULL LIMIT 1;
END
$$;

-- Set (or rotate) the platform credential for a provider. The caller seals the key.
CREATE FUNCTION xtenant.platform_ai_credential_set(
  p_actor_id uuid, p_provider text, p_ciphertext bytea, p_nonce bytea, p_key_hint text
) RETURNS TABLE (id uuid, rotated boolean)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  v_id uuid;
  v_rotated boolean;
BEGIN
  SELECT k.id INTO v_id FROM ai_credentials k
   WHERE k.tenant_id IS NULL AND k.provider = p_provider AND k.deleted_at IS NULL FOR UPDATE;
  IF FOUND THEN
    UPDATE ai_credentials
       SET ciphertext = p_ciphertext, nonce = p_nonce, key_hint = p_key_hint, deleted_at = NULL
     WHERE id = v_id;
    v_rotated := true;
  ELSE
    INSERT INTO ai_credentials (provider, ciphertext, nonce, key_hint)
    VALUES (p_provider, p_ciphertext, p_nonce, p_key_hint) RETURNING id INTO v_id;
    v_rotated := false;
  END IF;
  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (NULLIF(p_actor_id, '00000000-0000-0000-0000-000000000000'::uuid),
          'ai_credential.set', 'ai_credential', v_id::text, NULL,
          jsonb_build_object('provider', p_provider, 'key_hint', p_key_hint, 'rotated', v_rotated));
  RETURN QUERY SELECT v_id, v_rotated;
END
$$;

-- Soft-delete the platform credential for a provider. Returns its id, or NULL when there is none.
CREATE FUNCTION xtenant.platform_ai_credential_delete(p_actor_id uuid, p_provider text)
RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  v_id uuid;
BEGIN
  SELECT k.id INTO v_id FROM ai_credentials k
   WHERE k.tenant_id IS NULL AND k.provider = p_provider AND k.deleted_at IS NULL FOR UPDATE;
  IF NOT FOUND THEN
    RETURN NULL;
  END IF;
  UPDATE ai_credentials SET deleted_at = now() WHERE id = v_id;
  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (NULLIF(p_actor_id, '00000000-0000-0000-0000-000000000000'::uuid),
          'ai_credential.deleted', 'ai_credential', v_id::text, NULL,
          jsonb_build_object('provider', p_provider));
  RETURN v_id;
END
$$;

REVOKE ALL ON FUNCTION
  xtenant.integration_retention_sweep(),
  xtenant.notification_retention_sweep(),
  xtenant.widget_session_sweep(),
  xtenant.tool_request_expiry_sweep(),
  xtenant.available_agents(),
  xtenant.platform_active_conversation_count(),
  xtenant.platform_ai_usage_totals(timestamptz, timestamptz),
  xtenant.platform_queue_health(),
  xtenant.platform_provider_metrics(timestamptz, timestamptz),
  xtenant.platform_recent_provider_failures(timestamptz, timestamptz),
  xtenant.platform_audit_entries(uuid, uuid, timestamptz, timestamptz, text[], uuid, timestamptz, uuid, bigint),
  xtenant.platform_subscriptions_list(),
  xtenant.platform_subscription_set(uuid, uuid, uuid, text),
  xtenant.platform_ai_config(),
  xtenant.platform_ai_config_set(uuid, text, text, integer, real, jsonb, jsonb),
  xtenant.platform_ai_credential_secret(text),
  xtenant.platform_ai_credential_hint(text),
  xtenant.platform_ai_credential_set(uuid, text, bytea, bytea, text),
  xtenant.platform_ai_credential_delete(uuid, text)
FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  xtenant.integration_retention_sweep(),
  xtenant.notification_retention_sweep(),
  xtenant.widget_session_sweep(),
  xtenant.tool_request_expiry_sweep(),
  xtenant.available_agents(),
  xtenant.platform_active_conversation_count(),
  xtenant.platform_ai_usage_totals(timestamptz, timestamptz),
  xtenant.platform_queue_health(),
  xtenant.platform_provider_metrics(timestamptz, timestamptz),
  xtenant.platform_recent_provider_failures(timestamptz, timestamptz),
  xtenant.platform_audit_entries(uuid, uuid, timestamptz, timestamptz, text[], uuid, timestamptz, uuid, bigint),
  xtenant.platform_subscriptions_list(),
  xtenant.platform_subscription_set(uuid, uuid, uuid, text),
  xtenant.platform_ai_config(),
  xtenant.platform_ai_config_set(uuid, text, text, integer, real, jsonb, jsonb),
  xtenant.platform_ai_credential_secret(text),
  xtenant.platform_ai_credential_hint(text),
  xtenant.platform_ai_credential_set(uuid, text, bytea, bytea, text),
  xtenant.platform_ai_credential_delete(uuid, text)
TO app_runtime;

RESET ROLE;
