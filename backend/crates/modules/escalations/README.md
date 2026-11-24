# `escalations` module

The escalations module routes human handoffs to available team members, limits automatic load per agent, and keeps unassigned handoffs in a tenant queue.

## Responsibility

- Owns escalation records, skill catalogs, agent availability, routing capacity, assignment ranking, and queue draining.
- Does not own membership identity or conversation load; it asks `tenancy` for member eligibility and `conversations` for open and pending load.
- Does not enforce the capacity limit on an explicit manual claim. A manual claim updates the fairness timestamp so later automatic choices account for it.

## Public interface

### HTTP endpoints

| Method | Path | Permission | Purpose |
| -- | -- | -- | -- |
| `POST` | `/api/v1/tenant/conversations/{id}/escalate` | `conversations.manage` | Create and route or queue a handoff. |
| `GET` | `/api/v1/tenant/escalations/queue` | `conversations.view` | List queued handoffs. |
| `POST` | `/api/v1/tenant/escalations/{id}/claim` | `conversations.manage` | Manually claim a queued handoff. |
| `GET`, `PUT` | `/api/v1/tenant/availability/me` | `conversations.manage` | Read or change the calling member's availability. |
| `GET` | `/api/v1/tenant/skills` | `members.view` | List tenant skills. |
| `POST` | `/api/v1/tenant/skills` | `members.view` | Create a tenant skill. |
| `PATCH`, `DELETE` | `/api/v1/tenant/skills/{id}` | `members.manage` | Rename or delete a tenant skill. |
| `PUT` | `/api/v1/tenant/members/{membershipId}/skills` | `members.manage` | Replace a member's skills. |
| `GET` | `/api/v1/tenant/routing/capacity` | `members.view` | Read the default, overrides, effective capacities, and current loads. |
| `PUT` | `/api/v1/tenant/routing/capacity/default` | `members.manage` | Set the tenant's inherited capacity. An increase drains queued handoffs while agents have room. |
| `PUT` | `/api/v1/tenant/routing/capacity/agents/{membership_id}` | `members.manage` | Set an override from 0 through 200, or `null` to inherit. |
| `GET` | `/api/v1/tenant/events` | `conversations.view` | Stream tenant routing and conversation events. |

The full request, response, and error contract is in [`docs/api/openapi.json`](../../../../docs/api/openapi.json).

### Rust API for other modules

| Item | Purpose |
| -- | -- |
| `routing::route_new_escalation_in_tx` | Route a new handoff inside the caller's transaction. |
| `routing::claim_in_tx` | Claim a queued escalation inside the caller's transaction. |
| `routing::has_open_escalation` | Check whether a conversation already has an active escalation. |
| `events::process_escalation_outbox_once` | Apply one conversation status or assignment event to escalation state. |
| `presence::Runtime` | Register current presence and broadcast tenant-scoped SSE events. |

## Owned tables

| Table | Tenant-scoped | Purpose |
| -- | -- | -- |
| `escalations` | Yes (`tenant_id`) | Queued, assigned, and closed handoffs. |
| `skills` | Yes (`tenant_id`) | Tenant skill catalog. |
| `agent_skills` | Yes (`tenant_id`) | Skill assignments to memberships. |
| `agent_availability` | Yes (`tenant_id`) | Availability, nullable capacity override, and last assignment time per membership. |
| `routing_settings` | Yes (`tenant_id`) | Tenant default routing capacity. |

Migrations: `0035_agent_skills.sql`, `0036_agent_availability.sql`, `0037_escalations.sql`, and `0101_routing_capacity.sql`.

Tenant default capacity is 10 until configured, then ranges from 1 through 200. A member override is `NULL` to inherit or 0 through 200; zero disables automatic assignment. Capacity counts open and pending conversations assigned to the member.

## Events

### Published

| Event type | When | Payload |
| -- | -- | -- |
| `notification.requested` | A handoff is queued or assigned. | Tenant, escalation, actor, optional target membership, and notification text. |
| `notification.resolved` | A queued escalation is assigned. | Tenant, escalation, and assigned membership. |
| SSE `escalation.assigned`, `escalation.queued`, `escalation.removed`, `availability.changed` | Routing or availability changes. | Versioned tenant event payloads. |

### Consumed

| Event type | From | What this module does |
| -- | -- | -- |
| `conversation.status_changed` | `conversations` | Close a handoff when its conversation closes; attempt to drain the queue when load is freed. |
| `conversation.assignment_changed` | `conversations` | Relabel the escalation assignment and attempt to drain the queue when the previous member loses load. |

## Permissions

| Permission | Allows | Default roles |
| -- | -- | -- |
| `conversations.view` | View the escalation queue. | Owner, Admin, Manager, Agent, Viewer |
| `conversations.manage` | Escalate, claim, and change availability. | Owner, Admin, Manager, Agent |
| `members.view` | Read skills and routing capacity. | Owner, Admin, Manager |
| `members.manage` | Change skills and routing capacity. | Owner, Admin, Manager |

Audit records include escalation creation, assignment, claim and close; availability and skill changes; and `routing.capacity_changed`. Capacity audit details contain membership identifiers and old/new numeric values, never display names.

## Configuration

None. Tenant routing settings are stored in `routing_settings`; capacity defaults are not application configuration.

## Tests

Pure ranking tests live beside `queries.rs`. PostgreSQL-backed routing and API tests are in `backend/crates/server/tests/escalations.rs`; lock behavior is in `backend/crates/server/tests/escalation_routing_locks.rs`; RLS coverage is in `backend/crates/shared/db/tests/tenant_isolation.rs`. Run with a disposable database and Redis using `REQUIRE_DB_TESTS=1`.
