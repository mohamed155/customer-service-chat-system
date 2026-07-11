ALTER TABLE audit_logs
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON audit_logs
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

ALTER TABLE audit_logs
    ADD CONSTRAINT audit_logs_traceable
    CHECK (actor_user_id IS NOT NULL OR resource_id IS NOT NULL);
