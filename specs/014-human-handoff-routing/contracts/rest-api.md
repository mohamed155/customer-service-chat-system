# REST API Contract: Human Handoff & Routing

**Feature**: 014-human-handoff-routing | **Date**: 2026-07-14

All endpoints follow the platform contract (`specs/001-ai-customer-service-platform/contracts/rest-api.md`): mounted under the tenant context (`X-Tenant-ID` + session cookie), standard `ApiResponse<T>` envelope, standard error envelope with `X-Request-Id`, cursor pagination where listed. Cross-tenant access → `404 not_found` (never confirms existence). Route→permission mapping in [permissions.md](./permissions.md); SSE stream in [events.md](./events.md).

## Representations

```jsonc
// Escalation (embedded and standalone)
{
  "id": "uuid",
  "conversationId": "uuid",
  "reason": "Customer asked for a human on a billing dispute",
  "requiredSkills": [{ "id": "uuid|null", "name": "billing" }], // id null if skill deleted since (snapshot name)
  "status": "queued | assigned | closed",
  "routing": {                       // null while queued
    "reason": "skill_match | load_fallback | manual_claim | queue_auto | manual_reassignment",
    "matchedSkills": ["billing"],    // present for skill_match / queue_auto
    "assignedMembershipId": "uuid",
    "assignedAt": "ISO-8601"
  },
  "escalatedAt": "ISO-8601",
  "closedAt": "ISO-8601 | null"
}

// QueueEntry (queue list rows)
{
  "escalation": { /* Escalation, status always queued */ },
  "conversation": { "id": "uuid", "channel": "web_chat", "customer": { "id": "uuid", "name": "…" } },
  "waitingSeconds": 342
}

// Skill
{ "id": "uuid", "name": "billing", "agentCount": 3 }

// Availability
{ "membershipId": "uuid", "state": "available | away", "stateChangedAt": "ISO-8601" }
```

## Endpoints

### POST /tenant/conversations/{id}/escalate — escalate a conversation

Interim invoker surface; canonical contract is the escalations application service the AI subsystem will call (research R6).

- Body: `{ "reason": string (1–2000), "requiredSkillIds": uuid[] (optional, must exist in tenant catalog) }`
- `201` → `Escalation` — outcome is `assigned` (routing succeeded synchronously) or `queued`.
- Errors: `404` conversation not found/cross-tenant · `409 escalation_active` already queued/assigned (FR-002) · `422` reason length / unknown skill id · `409 conversation_closed_state`? — no: escalating resolved/closed conversations is allowed only for `open`/`pending`; resolved/closed → `422 invalid_state`.
- Audit: `escalation.created` + (`escalation.assigned` | `escalation.queued`).

### GET /tenant/escalations/queue — list queued escalations

- Query: cursor pagination (`limit`, `cursor`); ordered `escalated_at ASC` (longest-waiting first, FR-012).
- `200` → `PaginatedResponse<QueueEntry>`.

### POST /tenant/escalations/{id}/claim — manually claim a queued escalation

- No body. Allowed regardless of caller's availability state (US3 scenario 3); caller must hold `conversations.manage`.
- `200` → `Escalation` (status `assigned`, routing.reason `manual_claim`, assignee = caller).
- Errors: `404` unknown/cross-tenant · `409 already_claimed` with `{ "assignedMembershipId": … }` when CAS loses or escalation no longer queued (US2 scenario 3).
- Audit: `escalation.claimed`.

### GET /tenant/availability/me — caller's availability

- `200` → `Availability` (absent row reported as `away`).

### PUT /tenant/availability/me — toggle caller's availability

- Body: `{ "state": "available" | "away" }`. Self-only by construction (path is `/me`; FR-016).
- `200` → `Availability`. Toggling to `available` triggers a queue-drain pass (FR-014); response returns before drain results (assignments arrive via SSE).
- Audit: `availability.changed`.

### GET /tenant/skills — skill catalog

- `200` → `ApiResponse<Skill[]>` (catalog is small; no pagination).

### POST /tenant/skills — create skill

- Body: `{ "name": string (1–50) }` · `201` → `Skill` · `409 duplicate_name` (case-insensitive) · Audit: `skill.created`.

### PATCH /tenant/skills/{id} — rename skill

- Body: `{ "name": string }` · `200` → `Skill` · `409 duplicate_name` · Audit: `skill.updated`.

### DELETE /tenant/skills/{id} — delete skill

- `204`. Cascades: links removed, id stripped from queued escalations' `requiredSkillIds`; snapshot names untouched (FR-019). Audit: `skill.deleted`.

### PUT /tenant/members/{membershipId}/skills — set a member's skill set

- Body: `{ "skillIds": uuid[] }` (full replacement — idempotent).
- `200` → `Skill[]` now assigned. Errors: `404` membership unknown/cross-tenant · `422` unknown skill id or membership not agent-capable.
- Audit: `member.skills_changed`.

## Extended existing payloads

- **`GET /tenant/conversations/{id}`** (013): response gains `"escalation": Escalation | null` — the latest escalation record (active, or most recent closed for banner history), embedded via the escalations module's public query (R5).
- **`GET /tenant/conversations`** (013): new filter param `escalated=true` — predicate `escalated_at IS NOT NULL`, combinable with existing status/assignment/channel filters (FR-001a).
- **`GET /tenant/members`** (011): rows gain `"skills": Skill[]` and `"availability": "available" | "away"` for the team page and assignee pickers.
- **`PATCH /tenant/conversations/{id}`** (013, assignment changes): behavior unchanged; when the conversation has an active escalation, the outbox event relabels routing to `manual_reassignment` (FR-023) — no contract change.

## Audit actions added

`escalation.created` · `escalation.queued` · `escalation.assigned` (payload: routing reason, matched skills, candidate load) · `escalation.claimed` · `escalation.closed` · `skill.created` · `skill.updated` · `skill.deleted` · `member.skills_changed` · `availability.changed`
