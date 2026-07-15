# Data Model: Human Handoff & Routing

**Feature**: 014-human-handoff-routing | **Date**: 2026-07-14

Conventions from 005 apply unless noted: UUID PK (`gen_random_uuid()`), `tenant_id` on every tenant-owned table, `created_at`/`updated_at` + `set_updated_at` trigger, composite parent-tenant FKs per the 0027 pattern. Migrations: `0035_agent_skills.sql`, `0036_agent_availability.sql`, `0037_escalations.sql`.

## skills (0035)

Tenant-defined skill catalog (FR-018).

| Column | Type | Constraints |
|--------|------|-------------|
| id | UUID | PK |
| tenant_id | UUID | NOT NULL → tenants(id) ON DELETE RESTRICT |
| name | TEXT | NOT NULL, CHECK 1–50 chars after trim |
| created_at / updated_at | TIMESTAMPTZ | NOT NULL DEFAULT now(); `set_updated_at` trigger |

- `UNIQUE (tenant_id, id)` — composite FK target.
- Functional unique index `(tenant_id, lower(name))` — case-insensitive per-tenant uniqueness (R7).
- Hard delete (FR-019); no `deleted_at`. Rename allowed (PATCH) — id-based references keep routing stable.

## agent_skills (0035)

Link: this member can handle conversations requiring this skill.

| Column | Type | Constraints |
|--------|------|-------------|
| tenant_id | UUID | NOT NULL |
| membership_id | UUID | NOT NULL |
| skill_id | UUID | NOT NULL |
| created_at | TIMESTAMPTZ | NOT NULL DEFAULT now() |

- PK `(tenant_id, membership_id, skill_id)`.
- Composite FK `(tenant_id, membership_id)` → `tenant_memberships (tenant_id, id)`; composite FK `(tenant_id, skill_id)` → `skills (tenant_id, id)` ON DELETE CASCADE (skill deletion strips links, FR-019).
- Insert/hard-delete only — no `updated_at`/`deleted_at` (justified in plan Complexity Tracking).
- Index `(tenant_id, skill_id)` for candidate selection's match counting.

## agent_availability (0036)

Manual toggle state (FR-016); presence is runtime-only (R2), not stored here.

| Column | Type | Constraints |
|--------|------|-------------|
| tenant_id | UUID | NOT NULL |
| membership_id | UUID | NOT NULL |
| state | TEXT | NOT NULL, CHECK `available` \| `away`, DEFAULT `away` |
| state_changed_at | TIMESTAMPTZ | NOT NULL DEFAULT now() — drives the startup stale sweep |
| created_at / updated_at | TIMESTAMPTZ | trigger per conventions |

- PK `(tenant_id, membership_id)`; composite FK → `tenant_memberships (tenant_id, id)` ON DELETE CASCADE.
- Row created lazily on first toggle; absent row ≡ `away` (FR-016 default).
- **State transitions**: `away → available` (self toggle only, while signed in) · `available → away` (self toggle; presence grace-timeout, FR-017a; startup sweep; membership deactivation). Signing back in never auto-restores `available`.

## escalations (0037)

One row per handoff attempt; at most one *active* (queued/assigned) per conversation.

| Column | Type | Constraints |
|--------|------|-------------|
| id | UUID | PK |
| tenant_id | UUID | NOT NULL → tenants(id) ON DELETE RESTRICT |
| conversation_id | UUID | NOT NULL; composite FK `(tenant_id, conversation_id)` → `conversations (tenant_id, id)` |
| reason | TEXT | NOT NULL, CHECK 1–2000 chars |
| required_skill_ids | UUID[] | NOT NULL DEFAULT '{}' — live matching set; ids stripped on skill deletion while queued (FR-019) |
| required_skill_names | TEXT[] | NOT NULL DEFAULT '{}' — creation-time snapshot, never mutated (history/banner) |
| status | TEXT | NOT NULL, CHECK `queued` \| `assigned` \| `closed` |
| routing_reason | TEXT | NULL, CHECK `skill_match` \| `load_fallback` \| `manual_claim` \| `queue_auto` \| `manual_reassignment`; NULL iff never assigned |
| matched_skill_ids | UUID[] | NOT NULL DEFAULT '{}' — skills that produced a `skill_match`/`queue_auto` decision (observability, FR-010) |
| assigned_membership_id | UUID | NULL; composite FK → `tenant_memberships (tenant_id, id)` |
| escalated_at | TIMESTAMPTZ | NOT NULL DEFAULT now() — queue order key (FR-008) |
| assigned_at / closed_at | TIMESTAMPTZ | NULL |
| created_at / updated_at | TIMESTAMPTZ | trigger per conventions |

- **Partial unique index** `(tenant_id, conversation_id) WHERE status IN ('queued','assigned')` — enforces the FR-002 duplicate rule at the DB level.
- CHECK consistency: `status='assigned'` ⇒ `assigned_membership_id`, `routing_reason`, `assigned_at` all NOT NULL; `status='queued'` ⇒ all three NULL.
- Queue index `(tenant_id, escalated_at ASC) WHERE status = 'queued'` — queue page + drain scan.
- Conversation-lookup index `(tenant_id, conversation_id, created_at DESC)` — detail-page embed (latest escalation).
- **State transitions**: `queued → assigned` (claim, drain) · `queued → closed` (conversation resolved/closed, FR-015) · `assigned → closed` (conversation resolved/closed) · `assigned → assigned` (manual reassignment updates assignee + routing_reason). No transition out of `closed`; re-escalation creates a new row (partial unique index permits it once prior row is closed).

## conversations — alteration (0037)

| Change | Detail |
|--------|--------|
| `escalated_at TIMESTAMPTZ NULL` | Set when an escalation activates, cleared to NULL when it closes. Written **only** via the conversations crate's `set_escalated_in_tx` (module ownership, R5). Inbox `escalated` filter predicate: `escalated_at IS NOT NULL`. |
| Load-count index | `(tenant_id, assigned_membership_id) WHERE status IN ('open','pending') AND deleted_at IS NULL` — FR-007 load metric as an index-only count. |
| Escalated-inbox index | `(tenant_id, last_activity_at DESC, id DESC) WHERE escalated_at IS NOT NULL AND deleted_at IS NULL` — serves the escalated filter at inbox scale. |

## Outbox event types (0037, consumed per R5)

| Event | Payload (JSON) | Emitted by | Consumed for |
|-------|----------------|------------|--------------|
| `conversation.status_changed` | tenant_id, conversation_id, old/new status, actor, origin | conversations write txns | Close out active escalation on resolve/close (FR-015); drain trigger when load frees |
| `conversation.assignment_changed` | tenant_id, conversation_id, old/new membership, actor, origin | conversations write txns | Relabel routing_reason `manual_reassignment` on escalated conversations (FR-023); drain trigger. Events with `origin='escalations'` are ignored (loop guard) |

## Derived / runtime state (not persisted)

- **Presence**: `(tenant_id, membership_id) → live SSE connection count` in the escalations module registry (R2). Routing eligibility = `agent_availability.state='available'` ∧ present.
- **Load**: computed per decision from the load-count index; never cached (FR-014 re-evaluates between drain assignments).
- **Waiting time**: `now() − escalated_at`, computed at read time for the queue page.

## Relationships (summary)

```text
tenants 1─* skills 1─* agent_skills *─1 tenant_memberships
tenants 1─* agent_availability *─1 tenant_memberships
conversations 1─* escalations *─1 tenant_memberships (assignee, nullable)
conversations.escalated_at ← maintained by escalations via conversations public interface
```
