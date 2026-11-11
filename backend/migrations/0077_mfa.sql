-- Migration 0077: multi-factor authentication (MOH-56, ADR-005 phase M1).
--
-- A user has at most one TOTP factor. Its secret is sealed with envelope encryption (ADR-014) under a
-- key of its own (`user:<id>`), so deleting a user destroys it. A factor is unconfirmed until the
-- user proves they can produce a code. Recovery codes are stored only as Argon2id hashes.
--
-- Both tables are about users, who have no tenant, so they are global tables (`GLOBAL_TABLES`).

CREATE TABLE mfa_factors (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    user_id         uuid        NOT NULL REFERENCES users (id),
    kind            text        NOT NULL CHECK (kind IN ('totp')),
    -- The sealed TOTP secret (the columns `secrets::Sealed` has).
    ciphertext      bytea       NOT NULL,
    nonce           bytea       NOT NULL,
    wrapped_dek     bytea       NOT NULL,
    kek_id          text        NOT NULL,
    kek_version     integer     NOT NULL,
    algorithm       text        NOT NULL,
    confirmed_at    timestamptz,
    -- The last time step a code was accepted for. A code is accepted only for a later step, so it
    -- cannot be used twice.
    last_used_step  bigint,
    -- Consecutive wrong codes; reset on success and when the account locks.
    failed_attempts integer     NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
    locked_until    timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT mfa_factors_one_per_user UNIQUE (user_id)
);

CREATE TABLE mfa_recovery_codes (
    id         uuid        PRIMARY KEY DEFAULT uuidv7(),
    user_id    uuid        NOT NULL REFERENCES users (id),
    code_hash  text        NOT NULL,
    used_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX mfa_recovery_codes_unused ON mfa_recovery_codes (user_id) WHERE used_at IS NULL;

-- Deleting a user destroys the key that protects their TOTP secret, whichever code path deletes
-- the user (the same rule as tenants, 0076).
CREATE FUNCTION destroy_user_kek() RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
  UPDATE secret_keks
     SET state = 'destroyed', wrapped_kek = NULL, root_key_id = NULL, destroyed_at = now()
   WHERE kek_id = 'user:' || OLD.id::text AND state <> 'destroyed';
  RETURN NULL;
END
$$;

CREATE TRIGGER users_destroy_kek_on_soft_delete
    AFTER UPDATE OF deleted_at ON users
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
    EXECUTE FUNCTION destroy_user_kek();

CREATE TRIGGER users_destroy_kek_on_delete
    AFTER DELETE ON users
    FOR EACH ROW
    EXECUTE FUNCTION destroy_user_kek();
