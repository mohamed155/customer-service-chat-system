# Data Model: AI Customer Service Platform

**Date**: 2026-07-03 | **Plan**: [plan.md](./plan.md) | **Spec source**: [spec.md §8](./spec.md)

Conventions applying to every entity below:

- **IDs**: UUIDv7 primary keys (time-ordered, index-friendly).
- **Tenancy**: every tenant-owned entity carries `tenant_id`; all indexes on
  tenant-owned tables lead with `tenant_id`; RLS + `TenantScope` enforce
  isolation (research R-03). Platform-owned entities are marked *(platform)*.
- **Timestamps**: `created_at`/`updated_at` UTC on all entities; soft-delete
  via `deleted_at` only where a recovery window is specified.
- **Enums**: stored as text with check constraints; listed per entity.
- **Migrations**: schema exists only through SQLx migrations (Constitution VIII).

Entities are grouped by owning module (one crate per module, research R-01).

---

## identity

### UserIdentity *(platform)*
A person's login identity, shared across tenant memberships.
- `id`, `email` (unique, citext), `password_hash` (argon2id), `email_verified_at`,
  `totp_secret` (encrypted, nullable), `totp_enforced` (bool), `locked_until`
  (nullable), `failed_attempts`, `last_login_at`
- Relationships: 1—N Session, 1—N TenantMembership, 0—1 PlatformUser
- Validation: email RFC-format + verification before first sign-in (FR-AUTH-001);
  lockout after 5 failures / 15 min (FR-AUTH-006)

### Session *(platform)*
- `id`, `user_identity_id`, `token_hash`, `created_at`, `last_seen_at`,
  `idle_expires_at`, `absolute_expires_at`, `ip`, `user_agent`, `revoked_at`,
  `assumed_tenant_id` (nullable — set during Tenant Switcher use)
- State: active → expired | revoked. Revocation immediate (research R-09).

### ApiCredential
- `id`, `tenant_id`, `name`, `key_hash`, `scopes` (permission subset),
  `last_used_at`, `created_by`, `revoked_at`
- Validation: scopes ⊆ creator's permissions; write-only secret (shown once).

### Invitation
- `id`, `tenant_id`, `email`, `role`, `invited_by`, `token_hash`,
  `expires_at` (default +7 days), `accepted_at`, `revoked_at`
- State: pending → accepted | expired | revoked.

## tenancy

### Tenant *(platform root of all tenant data)*
- `id`, `name`, `slug` (unique), `logo_url`, `locale`, `timezone`,
  `lifecycle_state`, `settings` (structured: business hours, auto-close policy,
  CSAT enabled, session window [default 30 min, research R-13], retention
  windows, security policy: 2FA enforcement/session limits/SSO),
  `deletion_requested_at`, `purge_after` (deletion + 30 days)
- `lifecycle_state` ∈ {trial, active, suspended, pending_deletion, deleted}
- Transitions: trial→active (payment) | trial→suspended (expiry);
  active→suspended (dunning/operator); suspended→active;
  any→pending_deletion (Owner two-step) → deleted (after 30-day window)
- Relationships: 1—N everything tenant-owned; 1—1 active Subscription; 1—1
  active AgentConfiguration (v1 constraint — modeled 1—N for future,
  Clarifications 2026-07-03)

### PlatformUser *(platform)*
- `id`, `user_identity_id` (unique), `platform_role`
  ∈ {super_admin, developer, sales, support, finance}, `is_active`
- Tenant Switcher capability derives from role (FR-RBAC-003); every switch
  audit-logged with `Session.assumed_tenant_id`.

## rbac

### Role catalog *(platform, seeded)*
Fixed roles: platform {super_admin, developer, sales, support, finance};
tenant {owner, admin, manager, agent, viewer}. Stored as seeded catalog +
`Permission` list and `role_permissions` mapping so the permission matrix is
data, not code constants scattered around.

### TenantMembership
- `id`, `tenant_id`, `user_identity_id`, `tenant_role`
  ∈ {owner, admin, manager, agent, viewer}, `is_active`, `deactivated_at`
- Unique (tenant_id, user_identity_id). Invariant: ≥1 active owner per tenant
  (FR-RBAC-004), enforced in application service + DB trigger guard.
- Role change bumps a per-user permission-cache version (≤1 min propagation,
  FR-RBAC-005).

## users

### UserProfile *(platform, 1—1 UserIdentity)*
- `user_identity_id`, `display_name`, `avatar_url`, `locale`,
  `notification_preferences` (per category × channel; security categories
  non-optional, FR-NOTIF-003)

### AgentStatus
- `tenant_id`, `membership_id`, `availability` ∈ {online, away, offline},
  `skill_tags` (string set), `active_conversation_count` (derived/cached)
- Drives routing: skill-tag match first, load-based fallback (FR-USER-006).

## customers

### Customer
- `id`, `tenant_id`, `external_id` (tenant-asserted, nullable), `email`
  (nullable), `name` (nullable), `is_anonymous`, `merged_into_id` (nullable —
  merge chain), `attributes` (typed custom key-values, FR-CUST-005),
  `gdpr_delete_requested_at`
- Merge: anonymous profile → identified profile sets `merged_into_id`;
  conversations re-linked; reads follow the chain (FR-CUST-002).
- GDPR delete: request → content purge within 30 days; anonymized aggregates
  retained (FR-CUST-004).
- Indexes: (tenant_id, external_id), (tenant_id, email).

## conversations

### Conversation
- `id`, `tenant_id`, `customer_id`, `channel` (∈ {web_widget} in v1 —
  extensible), `status`, `assignee_membership_id` (nullable),
  `prompt_version_id` (version that served it), `tags` (set), `disposition`,
  `csat_rating` (1–5, nullable), `csat_comment`, `resolved_at`, `closed_at`,
  `last_activity_at`
- `status` ∈ {open, active_ai, active_human, waiting, resolved, closed}
- Transitions: open→active_ai (first AI turn) | active_human (handoff);
  active_*↔waiting; active_*→resolved (agent/AI/auto 24 h idle);
  resolved→closed (auto 72 h) | reopened→active_* within window; all
  transitions recorded as system Messages + timestamps (FR-CONV-002/007)
- Indexes: (tenant_id, status, last_activity_at), (tenant_id, customer_id),
  (tenant_id, assignee), FTS index on message content for search (FR-CONV-004)

### Message
- `id`, `tenant_id`, `conversation_id`, `seq` (monotonic per conversation —
  the realtime replay cursor), `author_type` ∈ {customer, ai, agent, system},
  `author_id` (nullable), `content`, `content_format`, `citations`
  (list of {knowledge_segment_id, source_id, snippet_ref}), `visibility`
  ∈ {public, internal_note}, `created_at`
- Unique (conversation_id, seq). Internal notes never delivered to widget
  (FR-CONV-005).

## escalations

### Escalation
- `id`, `tenant_id`, `conversation_id`, `trigger`
  ∈ {customer_request, low_confidence, topic_rule, sentiment,
  repeated_failure, manual}, `trigger_detail`, `queued_at`, `priority`,
  `required_tags` (from conversation tags), `assigned_membership_id`,
  `assigned_at`, `assignment_method` ∈ {skill_match, load_fallback,
  manual_claim, auto_none_available}, `outcome`
  ∈ {resolved_by_agent, returned_to_ai, abandoned, offline_captured},
  `resolved_at`
- State: queued → assigned → (resolved_by_agent | returned_to_ai) |
  requeued (agent disconnect ⇒ priority boost) | offline_captured
- Index: (tenant_id, queued_at) partial where assigned_at IS NULL.

## ai

### AgentConfiguration
- `id`, `tenant_id` (v1: one active per tenant), `name`, `is_active`,
  `behavior_constraints` (blocked topics, disclaimers, tone/length,
  business-hours behavior), `confidence_thresholds`
  ({answer, caveat, clarify, escalate} bounds), `escalation_rules`
  (topic/keyword rules), `language_policy` (enabled languages),
  `knowledge_collection_ids` (scope), `enabled_tool_ids`

### AiExecution
One AI turn's complete timeline (FR-AI-007).
- `id`, `tenant_id`, `conversation_id`, `message_id` (the AI reply),
  `prompt_version_id`, `context_snapshot` (assembler inputs, R-06),
  `context_hash`, `retrievals` (ordered {segment_id, score, rank, used}),
  `model_calls` (ordered {provider, model, latency_ms, input_tokens,
  output_tokens, error?, failover_from?}), `confidence_score`,
  `confidence_action` ∈ {answer, caveat, clarify, escalate},
  `decision_summary`, `total_latency_ms`, `status`
  ∈ {completed, failed, failed_over}
- Relationships: 1—N ToolInvocation; N—1 Message (1—1 in practice).
- Also the idempotent metering key for AI-interaction usage (R-11).

## prompts

### PromptVersion
- `id`, `tenant_id`, `agent_configuration_id`, `version_number` (monotonic),
  `sections` ({persona, instructions, constraints, escalation_guidance}),
  `status` ∈ {draft, published, superseded}, `change_note`, `created_by`,
  `published_at`, `published_by`
- Invariant: exactly one `published` per agent configuration (partial unique
  index). Published/superseded versions immutable. Draft concurrent-edit
  conflict via `lock_version` optimistic concurrency (spec edge case).
- Rollback = re-publish an older version as a new version_number referencing
  `rolled_back_from` (history stays linear and auditable, FR-PROMPT-005).

### SandboxSession
- `id`, `tenant_id`, `prompt_version_id` (draft), `created_by`, `messages`
  (ephemeral transcript), `expires_at`
- Never touches live conversations; reads live knowledge index read-only
  (accepted tech debt).

## knowledge

### KnowledgeCollection
- `id`, `tenant_id`, `name`, `description`

### KnowledgeSource
- `id`, `tenant_id`, `collection_id` (nullable), `kind`
  ∈ {file, article, url, crawl}, `title`, `origin` (filename/url), `s3_key`
  (nullable), `status` ∈ {queued, processing, ready, failed, disabled},
  `failure_reason` (actionable, nullable), `content_bytes` (quota accounting),
  `checksum`, `last_ingested_at`, `crawl_bounds` (for kind=crawl)
- Transitions: queued→processing→ready | failed; ready→queued (re-ingest —
  old segments serve until swap, R-08); ready↔disabled; any→deleted (hard
  delete of segments + embeddings, FR-KB-003)

### KnowledgeSegment
- `id`, `tenant_id`, `source_id`, `ordinal`, `content`, `heading_path`,
  `token_count`, `embedding` (vector), `fts` (generated tsvector),
  `generation` (visibility-swap key)
- Indexes: HNSW on embedding (per-tenant filtered), GIN on fts; only
  segments whose `generation` = source's current generation are retrievable.

### IngestionJob
- `id`, `tenant_id`, `source_id`, `stage` ∈ {fetch, extract, segment, embed,
  swap}, `attempts`, `run_after`, `locked_by`, `last_error`
- Postgres job queue, `FOR UPDATE SKIP LOCKED` workers (R-08).

## tools

### Tool
- `id`, `tenant_id` (NULL ⇒ platform-provided starter tool), `name`,
  `description`, `input_schema` (JSON Schema), `output_schema`,
  `endpoint_url` (tenant tools), `timeout_ms`, `rate_limit_per_conversation`,
  `status` ∈ {pending_approval, enabled, disabled}
- Only enabled tools are invocable, and only if listed in
  `AgentConfiguration.enabled_tool_ids` (FR-AI-002, FR-INT-005).

### ToolInvocation
- `id`, `tenant_id`, `ai_execution_id`, `tool_id`, `input` (validated),
  `output` (validated, nullable), `status` ∈ {ok, invalid_input,
  invalid_output, timeout, error}, `latency_ms`, `error_detail`

## integrations

### WidgetConfig
- `tenant_id` (1—1), `theme` (colors/logo/position), `welcome_message`,
  `launcher_style`, `citations_visible_to_customers` (bool),
  `identity_verification_secret` (encrypted), `installed_detected_at`

### WebhookSubscription
- `id`, `tenant_id`, `url`, `secret` (encrypted, HMAC signing), `event_types`
  (subset of catalog per FR-INT-002), `is_active`

### WebhookDelivery
- `id`, `tenant_id`, `subscription_id`, `event_type`, `payload`,
  `attempt_count`, `next_retry_at`, `status` ∈ {pending, delivered, failed},
  `response_summary`

## notifications

### Notification
- `id`, `tenant_id` (nullable for platform notices), `recipient_identity_id`,
  `category`, `channel` ∈ {in_app, email}, `title`, `body_ref`,
  `delivered_at`, `read_at`
- Respect preferences + quiet hours except security-critical (FR-NOTIF-003/004).

## billing *(plans platform-owned; the rest tenant-owned)*

### Plan *(platform)*
- `id`, `name`, `monthly_fee`, `included_seats`, `included_ai_interactions`,
  `knowledge_storage_bytes`, `entitlements` (feature keys), `overage_pricing`,
  `is_active`

### Subscription
- `id`, `tenant_id`, `plan_id`, `status` ∈ {trialing, active, past_due,
  suspended, canceled}, `trial_ends_at`, `current_period_start/end`,
  `pending_plan_id` (scheduled downgrade), `payment_processor_ref`
- Dunning: active→past_due (payment fail) → retries → suspended → active
  (payment) — every transition audit-logged (M6 acceptance).

### UsageRecord
- `id`, `tenant_id`, `metric` ∈ {ai_interaction, seat, storage_bytes},
  `quantity`, `idempotency_key` (unique — e.g. AiExecution id),
  `occurred_at`, `period_key`
- Append-only; unique key = double-billing impossible (R-11, SC-010).

### Invoice
- `id`, `tenant_id`, `period_start/end`, `line_items` ({description, metric,
  quantity, unit_price, amount}), `total`, `status` ∈ {draft, issued, paid,
  failed, credited}, `issued_at`, `paid_at`, `processor_ref`

## audit

### AuditEvent *(append-only, no updates/deletes)*
- `id`, `occurred_at`, `actor_type` ∈ {platform_user, tenant_user, system,
  api_credential}, `actor_id`, `acting_tenant_id` (nullable — set for
  switcher context), `tenant_id` (nullable for platform-scope events),
  `action` (catalog key per FR-AUDIT-001), `target_type`, `target_id`,
  `before_summary`/`after_summary` (redacted), `ip`, `user_agent`,
  `request_id`
- Written in-transaction with the action via the event bus (R-02).
  Retention ≥12 months (FR-AUDIT-004). Index: (tenant_id, occurred_at),
  (action, occurred_at).

## flags

### FeatureFlag *(platform)*
- `key` (pk), `description`, `default_enabled`, `created_at`

### FlagOverride *(platform)*
- `flag_key`, `scope_type` ∈ {plan, tenant}, `scope_id`, `enabled`
- Resolution: tenant override > plan override > default; unresolvable ⇒
  default (fail-safe, FR-FLAG-003). Propagation ≤5 min via Redis bust.

## platform

### AiProviderConfig *(platform)*
- `id`, `provider` ∈ {openai, anthropic, gemini}, `credentials_encrypted`
  (envelope-encrypted, last-4 only readable), `model_catalog`
  ({model, capabilities, cost_per_token}), `is_active`

### RoutingPolicy *(platform)*
- `id`, `capability` ∈ {chat, embed}, `default_provider_model`,
  `fallback_chain` (ordered), `overrides` (per plan / per tenant / BYO-key
  tenants)

### Incident *(platform)*
- `id`, `severity`, `title`, `status` ∈ {investigating, identified,
  monitoring, resolved}, `status_history`, `affected_tenant_scope`
  (all | list), `created_by`, timestamps
- Drives tenant banners + notifications (FR-HEALTH-003).

### OutboxEvent *(shared/events infrastructure)*
- `id`, `aggregate_type`, `aggregate_id`, `tenant_id` (nullable),
  `event_type`, `payload`, `created_at`, `processed_at`, `attempts`
- The durability seam for async consumers (R-02).

---

## Relationship summary (cross-module)

```text
Tenant 1—N {TenantMembership, Customer, Conversation, KnowledgeSource,
            KnowledgeCollection, Tool(tenant), ApiCredential, Subscription,
            Invoice, UsageRecord, WebhookSubscription, AuditEvent(tenant),
            Escalation, AgentConfiguration(1 active in v1)}
UserIdentity 1—N TenantMembership; 0—1 PlatformUser; 1—N Session
Customer 1—N Conversation
Conversation 1—N Message; N—1 PromptVersion; 1—N Escalation
Message(ai) 1—1 AiExecution 1—N ToolInvocation
AiExecution N—M KnowledgeSegment (via retrievals list)
AgentConfiguration 1—N PromptVersion (one published);
                   N—M KnowledgeCollection; N—M Tool (enabled set)
KnowledgeCollection 1—N KnowledgeSource 1—N KnowledgeSegment
Plan 1—N Subscription; Subscription N—1 Tenant
FeatureFlag 1—N FlagOverride
```

Module-boundary rule: a module reads another module's data only through that
module's application service (or reacts to its domain events) — e.g. billing
never joins to `ai_executions`; it consumes usage events (R-01, R-02).
