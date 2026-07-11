ALTER TABLE audit_logs
    DROP CONSTRAINT IF EXISTS audit_logs_traceable;

ALTER TABLE audit_logs
    ADD CONSTRAINT audit_logs_resource_required
    CHECK (resource_id IS NOT NULL);
