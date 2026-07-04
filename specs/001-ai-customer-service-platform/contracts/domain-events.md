# Contract: Domain Events

Internal module-to-module contract (Constitution I: modules communicate via
application services and domain events). Transport: in-process dispatch for
same-transaction consumers + transactional outbox for async consumers
(research R-02). Webhook-visible events (FR-INT-002) are a public subset,
marked ⚡.

## Envelope

```json
{
  "event_id": "evt_01J...",
  "event_type": "conversation.escalated",
  "occurred_at": "2026-07-03T12:00:00Z",
  "tenant_id": "t_...",          // null for platform-scope events
  "aggregate_type": "conversation",
  "aggregate_id": "c_...",
  "actor": { "type": "system|tenant_user|platform_user|api_credential", "id": "..." },
  "request_id": "req_...",
  "payload": { }
}
```

Rules: events are immutable facts, named `<aggregate>.<past-tense-verb>`;
payloads carry IDs + minimal denormalized fields (consumers fetch details via
the owning module's service); at-least-once delivery ⇒ consumers idempotent
(keyed on `event_id`); ordering guaranteed per aggregate only.

## Catalog (producer → notable consumers)

### identity / tenancy / rbac (M1)
- `user.registered`, `user.login_succeeded/failed`, `user.locked_out` → audit
- `session.revoked`, `credential.created/revoked` → audit
- `invitation.sent/accepted/revoked` → audit, notifications
- `tenant.created/activated/suspended/reactivated` ⚡(suspended/activated) → audit, billing, notifications
- `tenant.deletion_requested/confirmed/purged` → audit, all modules (purge cascade)
- `membership.role_changed`, `membership.deactivated` → audit, rbac cache bust
- `platform.tenant_context_entered/exited` → audit (Tenant Switcher trail)

### customers / conversations / escalations (M2, M5)
- `customer.created/merged/gdpr_delete_requested/purged` → audit, analytics
- `conversation.started` ⚡ → analytics, notifications
- `conversation.message_added` → analytics (volume), realtime fan-out
- `conversation.status_changed` → analytics
- `conversation.resolved` ⚡ / `conversation.closed` → analytics, ai (memory extraction hook)
- `csat.submitted` ⚡ → analytics
- `conversation.escalated` ⚡ → escalations, notifications (≤5 s alert), analytics
- `escalation.assigned/requeued/returned_to_ai/offline_captured` → audit, analytics

### ai / prompts (M3)
- `ai.execution_completed` → **billing metering (idempotency key = execution id)**, analytics, audit(sample)
- `ai.execution_failed/failed_over` → platform health, analytics
- `ai.confidence_below_threshold` → escalations (trigger), analytics
- `prompt.version_published/rolled_back` ⚡(optional) → audit, analytics (version segmentation)
- `provider.config_changed`, `provider.failover_triggered` → audit, platform health

### knowledge (M4)
- `knowledge.source_added/updated/deleted/disabled` → audit
- `knowledge.ingestion_completed` ⚡ / `knowledge.ingestion_failed` ⚡ → notifications, audit
- `knowledge.quota_threshold_reached` → notifications

### tools / integrations (M5)
- `tool.registered/enabled/disabled` → audit
- `tool.invocation_failed` → analytics, platform health (error-rate)
- `webhook.delivery_failed_permanently` → notifications

### billing (M6)
- `usage.recorded` → analytics
- `usage.threshold_reached` (80/100%) ⚡ → notifications
- `subscription.plan_changed/trial_expiring/past_due/suspended` → audit, notifications, tenancy
- `invoice.issued/paid/payment_failed` ⚡(issued/paid) → audit, notifications

### flags / platform (M7)
- `flag.changed` → audit, flag cache bust (≤5 min propagation)
- `incident.declared/updated/resolved` → notifications (tenant banners), audit

## Consumer registration rules

- audit subscribes **in-transaction** to every event marked → audit
  (FR-AUDIT-001 coverage test walks this catalog).
- All other consumers subscribe via outbox (async, retried).
- A module may not subscribe to another module's event and then reach into
  that module's tables — payload + service calls only (R-01).
