-- Migration 0076: envelope encryption for tenant secrets (MOH-33, ADR-014 gaps E1-E4).
--
-- Each secret is sealed under its own data key (DEK). The DEK is wrapped by a key-encryption key
-- (KEK) per tenant or for the platform, and each KEK is wrapped by the root key held by the KMS
-- (`crates/shared/secrets`). This migration adds:
--
--   1. `secret_keks`: one row per (kek_id, version) holding the wrapped KEK. A KEK id is
--      `tenant:<uuid>` or `platform`. Destroying a KEK clears its wrapped key (crypto-shredding).
--   2. Four metadata columns next to every sealed secret: the wrapped DEK, the KEK id and version,
--      and the algorithm. They are NULL for a row sealed the earlier way (one static key); such a
--      row stays readable until the re-encryption job (`server reencrypt-secrets`) moves it.
--   3. The platform credential functions return and accept that metadata.
--
-- No existing ciphertext is changed here, so nothing is unreadable between this migration and the
-- job: readers open both kinds of row.

-- 1. KEKs. Platform-level (no tenant column): the rows hold wrapped keys only.
CREATE TABLE secret_keks (
    kek_id       text        NOT NULL,
    version      integer     NOT NULL CHECK (version >= 1),
    state        text        NOT NULL CHECK (state IN ('active', 'retired', 'destroyed')),
    root_key_id  text,
    wrapped_kek  bytea,
    created_at   timestamptz NOT NULL DEFAULT now(),
    retired_at   timestamptz,
    destroyed_at timestamptz,
    PRIMARY KEY (kek_id, version),
    -- A destroyed KEK has no key left; every other KEK has one.
    CONSTRAINT secret_keks_wrapped_unless_destroyed
        CHECK ((state = 'destroyed') = (wrapped_kek IS NULL AND root_key_id IS NULL))
);

-- At most one active version per KEK id.
CREATE UNIQUE INDEX secret_keks_one_active ON secret_keks (kek_id) WHERE state = 'active';

-- Destroying a KEK is an UPDATE that clears its key. The runtime role can't delete the rows: a
-- stray DELETE would lose the record that a secret was ever sealed under that version.
REVOKE DELETE ON secret_keks FROM app_runtime;

COMMENT ON TABLE secret_keks IS
    'Wrapped key-encryption keys for envelope encryption (ADR-014). Clearing wrapped_kek destroys '
    'every secret sealed under that KEK.';

-- 2. Per-secret key metadata.
ALTER TABLE ai_credentials
    ADD COLUMN wrapped_dek bytea,
    ADD COLUMN kek_id      text,
    ADD COLUMN kek_version integer,
    ADD COLUMN algorithm   text,
    ADD CONSTRAINT ai_credentials_envelope_complete CHECK (
        (wrapped_dek IS NULL) = (kek_id IS NULL)
        AND (kek_id IS NULL) = (kek_version IS NULL)
        AND (kek_id IS NULL) = (algorithm IS NULL)
    );

ALTER TABLE integration_secrets
    ADD COLUMN wrapped_dek bytea,
    ADD COLUMN kek_id      text,
    ADD COLUMN kek_version integer,
    ADD COLUMN algorithm   text,
    ADD CONSTRAINT integration_secrets_envelope_complete CHECK (
        (wrapped_dek IS NULL) = (kek_id IS NULL)
        AND (kek_id IS NULL) = (kek_version IS NULL)
        AND (kek_id IS NULL) = (algorithm IS NULL)
    );

ALTER TABLE integration_connections
    ADD COLUMN webhook_token_wrapped_dek bytea,
    ADD COLUMN webhook_token_kek_id      text,
    ADD COLUMN webhook_token_kek_version integer,
    ADD COLUMN webhook_token_algorithm   text,
    ADD CONSTRAINT integration_connections_token_envelope_complete CHECK (
        (webhook_token_wrapped_dek IS NULL) = (webhook_token_kek_id IS NULL)
        AND (webhook_token_kek_id IS NULL) = (webhook_token_kek_version IS NULL)
        AND (webhook_token_kek_id IS NULL) = (webhook_token_algorithm IS NULL)
    );

-- Tool credentials are one text value, `base64(ciphertext)||base64(nonce)`; the metadata sits next to it.
ALTER TABLE tenant_tools
    ADD COLUMN credential_wrapped_dek bytea,
    ADD COLUMN credential_kek_id      text,
    ADD COLUMN credential_kek_version integer,
    ADD COLUMN credential_algorithm   text,
    ADD CONSTRAINT tenant_tools_credential_envelope_complete CHECK (
        (credential_wrapped_dek IS NULL) = (credential_kek_id IS NULL)
        AND (credential_kek_id IS NULL) = (credential_kek_version IS NULL)
        AND (credential_kek_id IS NULL) = (credential_algorithm IS NULL)
    );

-- 3. The platform credential functions (platform-wide rows have tenant_id NULL and are reached
-- through the audited xtenant functions, 0071). The signatures change, so the old ones go.
SET LOCAL ROLE app_platform;

DROP FUNCTION xtenant.platform_ai_credential_secret(text);
DROP FUNCTION xtenant.platform_ai_credential_set(uuid, text, bytea, bytea, text);

-- The platform credential for a provider: the sealed key and the metadata to open it.
CREATE FUNCTION xtenant.platform_ai_credential_secret(p_provider text)
RETURNS TABLE (
  ciphertext bytea, nonce bytea, wrapped_dek bytea, kek_id text, kek_version integer, algorithm text
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
BEGIN
  RETURN QUERY
    SELECT k.ciphertext, k.nonce, k.wrapped_dek, k.kek_id, k.kek_version, k.algorithm
      FROM ai_credentials k
     WHERE k.tenant_id IS NULL AND k.provider = p_provider AND k.deleted_at IS NULL LIMIT 1;
  IF FOUND THEN
    PERFORM xtenant._audit('platform_ai_credential_secret', NULL, NULL, p_provider,
                           '{}'::jsonb, interval '1 hour');
  END IF;
END
$$;

-- Set (or rotate) the platform credential for a provider. The caller seals the key; a caller that
-- still seals the earlier way leaves the four metadata arguments out.
CREATE FUNCTION xtenant.platform_ai_credential_set(
  p_actor_id uuid, p_provider text, p_ciphertext bytea, p_nonce bytea, p_key_hint text,
  p_wrapped_dek bytea DEFAULT NULL, p_kek_id text DEFAULT NULL,
  p_kek_version integer DEFAULT NULL, p_algorithm text DEFAULT NULL
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
       SET ciphertext = p_ciphertext, nonce = p_nonce, key_hint = p_key_hint, deleted_at = NULL,
           wrapped_dek = p_wrapped_dek, kek_id = p_kek_id, kek_version = p_kek_version,
           algorithm = p_algorithm
     WHERE id = v_id;
    v_rotated := true;
  ELSE
    INSERT INTO ai_credentials (provider, ciphertext, nonce, key_hint,
                                wrapped_dek, kek_id, kek_version, algorithm)
    VALUES (p_provider, p_ciphertext, p_nonce, p_key_hint,
            p_wrapped_dek, p_kek_id, p_kek_version, p_algorithm) RETURNING id INTO v_id;
    v_rotated := false;
  END IF;
  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (NULLIF(p_actor_id, '00000000-0000-0000-0000-000000000000'::uuid),
          'ai_credential.set', 'ai_credential', v_id::text, NULL,
          jsonb_build_object('provider', p_provider, 'key_hint', p_key_hint, 'rotated', v_rotated));
  RETURN QUERY SELECT v_id, v_rotated;
END
$$;

-- Replace a platform credential's seal (re-encryption to envelope form, or a re-wrap) without
-- changing the key it holds: the update applies only while the row still has the ciphertext the
-- caller read, so a rotation by an operator in between is never overwritten.
CREATE FUNCTION xtenant.platform_ai_credential_reseal(
  p_provider text, p_old_ciphertext bytea, p_ciphertext bytea, p_nonce bytea,
  p_wrapped_dek bytea, p_kek_id text, p_kek_version integer, p_algorithm text
) RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  v_id uuid;
BEGIN
  UPDATE ai_credentials
     SET ciphertext = p_ciphertext, nonce = p_nonce, wrapped_dek = p_wrapped_dek,
         kek_id = p_kek_id, kek_version = p_kek_version, algorithm = p_algorithm
   WHERE tenant_id IS NULL AND provider = p_provider AND deleted_at IS NULL
     AND ciphertext = p_old_ciphertext
  RETURNING id INTO v_id;
  IF v_id IS NULL THEN
    RETURN false;
  END IF;
  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (NULL, 'ai_credential.resealed', 'ai_credential', v_id::text, NULL,
          jsonb_build_object('provider', p_provider, 'kek_id', p_kek_id,
                             'kek_version', p_kek_version));
  RETURN true;
END
$$;

REVOKE ALL ON FUNCTION
  xtenant.platform_ai_credential_secret(text),
  xtenant.platform_ai_credential_set(uuid, text, bytea, bytea, text, bytea, text, integer, text),
  xtenant.platform_ai_credential_reseal(text, bytea, bytea, bytea, bytea, text, integer, text)
FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  xtenant.platform_ai_credential_secret(text),
  xtenant.platform_ai_credential_set(uuid, text, bytea, bytea, text, bytea, text, integer, text),
  xtenant.platform_ai_credential_reseal(text, bytea, bytea, bytea, bytea, text, integer, text)
TO app_runtime;

RESET ROLE;

-- 4. Crypto-shredding (ADR-014 section 4): deleting a tenant destroys its KEK, whichever code
-- path deletes it (a route, a console, a script). Every secret sealed under that KEK is
-- unrecoverable from then on, backups included. Other processes drop the KEK from their short
-- cache within its TTL (`secrets::SecretBox`). Restoring `deleted_at` does not bring the key back.
CREATE FUNCTION destroy_tenant_kek() RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
  UPDATE secret_keks
     SET state = 'destroyed', wrapped_kek = NULL, root_key_id = NULL, destroyed_at = now()
   WHERE kek_id = 'tenant:' || OLD.id::text AND state <> 'destroyed';
  RETURN NULL;
END
$$;

CREATE TRIGGER tenants_destroy_kek_on_soft_delete
    AFTER UPDATE OF deleted_at ON tenants
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
    EXECUTE FUNCTION destroy_tenant_kek();

CREATE TRIGGER tenants_destroy_kek_on_delete
    AFTER DELETE ON tenants
    FOR EACH ROW
    EXECUTE FUNCTION destroy_tenant_kek();
