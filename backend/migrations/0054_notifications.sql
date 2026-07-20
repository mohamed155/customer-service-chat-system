-- Notification inbox for tenant agents (spec 026-audit-logs).
-- Stores per-recipient notifications with deduplication and lifecycle states.
CREATE TABLE notifications (
    id                    uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id             uuid        NOT NULL REFERENCES tenants(id),
    recipient_membership_id uuid      NOT NULL REFERENCES tenant_memberships(id) ON DELETE CASCADE,
    kind                  text        NOT NULL CHECK (kind IN (
        'escalation.new',
        'conversation.assigned',
        'ai.response_failed',
        'tool.approval_required'
    )),
    state                 text        NOT NULL DEFAULT 'unread' CHECK (state IN (
        'unread',
        'read',
        'resolved'
    )),
    title                 text        NOT NULL,
    body                  text,
    subject_type          text        NOT NULL CHECK (subject_type IN (
        'conversation',
        'escalation',
        'tool_request'
    )),
    subject_id            uuid        NOT NULL,
    dedupe_key            text        NOT NULL,
    actor_membership_id   uuid        REFERENCES tenant_memberships(id),
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    read_at               timestamptz,

    PRIMARY KEY (id)
);

-- Enforce exactly one notification per (recipient, dedupe_key)
CREATE UNIQUE INDEX notifications_dedupe_uq
    ON notifications (recipient_membership_id, dedupe_key);

-- Recipient inbox ordering (newest first)
CREATE INDEX notifications_inbox_idx
    ON notifications (recipient_membership_id, created_at DESC, id DESC);

-- Quick unread count for badge display
CREATE INDEX notifications_unread_idx
    ON notifications (recipient_membership_id)
    WHERE state = 'unread';

-- Bulk-resolve all unread notifications for a given subject
CREATE INDEX notifications_resolve_idx
    ON notifications (tenant_id, subject_type, subject_id)
    WHERE state = 'unread';

-- Retention pruning by age
CREATE INDEX notifications_retention_idx
    ON notifications (created_at);

COMMENT ON TABLE notifications IS 'Per-agent in-app notifications with deduplication and state lifecycle';
COMMENT ON COLUMN notifications.kind IS 'Notification category driving icon/action behaviour';
COMMENT ON COLUMN notifications.state IS 'Lifecycle: unread → read (user viewed) → resolved (action taken or dismissed)';
COMMENT ON COLUMN notifications.subject_type IS 'Entity type the notification references';
COMMENT ON COLUMN notifications.subject_id IS 'Entity UUID the notification references';
COMMENT ON COLUMN notifications.dedupe_key IS 'Application-level deduplication per recipient, e.g. escalation-{id}';
COMMENT ON COLUMN notifications.actor_membership_id IS 'The tenant membership that caused this notification, NULL for system-caused';

-- DOWN: DROP TABLE notifications CASCADE;
