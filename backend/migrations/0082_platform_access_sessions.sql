-- Staff authorization state, keyed by the platform actor across workspaces.
CREATE TABLE platform_access_sessions (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id uuid NOT NULL REFERENCES users (id),
    tenant_id uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    auth_session_id uuid NOT NULL,
    reason text NOT NULL CHECK (char_length(btrim(reason)) BETWEEN 10 AND 500),
    started_at timestamptz NOT NULL DEFAULT statement_timestamp(),
    expires_at timestamptz NOT NULL,
    ended_at timestamptz,
    end_reason text CHECK (end_reason IN ('ended', 'expired', 'replaced', 'signed_out')),
    ip inet,
    CHECK (expires_at > started_at AND expires_at <= started_at + interval '4 hours'),
    CHECK ((ended_at IS NULL) = (end_reason IS NULL))
);

CREATE UNIQUE INDEX platform_access_sessions_one_open
    ON platform_access_sessions (user_id) WHERE ended_at IS NULL;
CREATE INDEX platform_access_sessions_open_expiry
    ON platform_access_sessions (expires_at) WHERE ended_at IS NULL;
CREATE INDEX platform_access_sessions_actor_history
    ON platform_access_sessions (user_id, auth_session_id, tenant_id, started_at DESC, id DESC);
CREATE INDEX platform_access_sessions_tenant
    ON platform_access_sessions (tenant_id);

-- Keep the historical identifier even when tenant deletion cascades to the access session.
-- A foreign key would conflict with the audit log's append-only guarantee.
ALTER TABLE audit_logs ADD COLUMN platform_access_session_id uuid;

CREATE FUNCTION public.link_platform_access_audit()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
    IF NEW.platform_access_session_id IS NOT NULL
        OR NEW.actor_user_id IS NULL OR NEW.tenant_id IS NULL THEN
        RETURN NEW;
    END IF;

    -- Lifecycle rows identify the exact session even if it was closed in this transaction.
    IF NEW.resource_type = 'platform_access_session' AND NEW.action IN (
        'platform.tenant_access_started', 'platform.tenant_access_ended',
        'platform.tenant_access_expired'
    ) THEN
        NEW.platform_access_session_id := NEW.resource_id::uuid;
        RETURN NEW;
    END IF;

    -- Any other row is linked by the writer, from the access session that authorized the request.
    -- Guessing here from (actor, tenant) could attach a write made from another sign-in, or by a
    -- route no access session authorized, to a session it does not belong to.
    RETURN NEW;
END
$$;

CREATE TRIGGER audit_logs_link_platform_access
BEFORE INSERT ON audit_logs
FOR EACH ROW EXECUTE FUNCTION public.link_platform_access_audit();
REVOKE ALL ON FUNCTION public.link_platform_access_audit() FROM PUBLIC;

-- The previous API selected named columns, which all remain available during rollout.
SET LOCAL ROLE app_platform;
DROP FUNCTION xtenant.platform_audit_entries(
    uuid, uuid, timestamptz, timestamptz, text[], uuid, timestamptz, uuid, bigint
);

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
  platform_role text, deleted_at timestamptz, platform_access_session_id uuid
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
           a.created_at, u.display_name, u.email::text, u.platform_role, u.deleted_at,
           a.platform_access_session_id
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

REVOKE ALL ON FUNCTION xtenant.platform_audit_entries(
    uuid, uuid, timestamptz, timestamptz, text[], uuid, timestamptz, uuid, bigint
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION xtenant.platform_audit_entries(
    uuid, uuid, timestamptz, timestamptz, text[], uuid, timestamptz, uuid, bigint
) TO app_runtime;
RESET ROLE;
