# Phase 0 Research: Notifications

All findings below were verified against the code at `027-notifications` planning time. File and line references are the evidence; re-check them if the surrounding code has moved.

---

## R1. How do notifications get created? (the load-bearing decision)

**Decision**: Trigger sites emit a new private outbox event type `notification.requested`. A new notifications worker is the only consumer of that type. It resolves recipients, fans out rows, broadcasts over SSE, then deletes the event.

**Rationale**: The obvious design — have a notifications worker consume the existing `conversation.assignment_changed` / `ai.tool_decision` events — **does not work**, and this is not a style preference but a correctness constraint:

- `outbox_events` consumers claim a row (`UPDATE … SET claimed_at, claim_token … FOR UPDATE SKIP LOCKED`) and then **`DELETE` it** when done (`escalations/src/events.rs:255-283`, `ai/src/agent_responder.rs:20-74`).
- `conversation.assignment_changed` and `conversation.status_changed` are already claimed and deleted by `escalations::events::process_escalation_outbox_once` (`events.rs:259`).
- Therefore each row is delivered to **exactly one** consumer. A second consumer would race the first and see roughly half the events — silently, and non-deterministically.

A private event type sidesteps this entirely: existing consumers filter by `event_type IN (…)` and will never see `notification.*`, and the notifications worker will never see theirs.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Add a `consumer_group` column to `outbox_events` so multiple consumers each process every row | Architecturally the "right" general fix, but it changes the claim predicate of all three existing consumers (invitations, escalations, agent responder). Breaking the AI pipeline to add a bell is a bad trade. Worth revisiting if a fourth multi-consumer need appears. |
| Write notification rows synchronously, in the same transaction as the domain change | Simplest and gives exactly-once for free, but violates FR-017: a failure writing a notification would roll back the assignment/escalation that caused it. Also couples every trigger module directly to the notifications schema. |
| Broadcast-only, in-memory via `presence::Runtime` | No persistence, so no inbox, no unread count across sessions, nothing for FR-005/FR-006. This is essentially what exists today and what the feature replaces. |

---

## R2. How does auto-resolve (FR-011a) get its signal?

**Decision**: A second private event type, `notification.resolved`, emitted at the two sites where fanned-out work gets taken: escalation claim (`escalations/src/routing.rs`, the `claim` path around line 258) and tool decision (`tools/src/approval.rs::decide`). The worker handles it with a single set-based `UPDATE`.

**Rationale**: Resolution has exactly the same visibility problem as creation — the natural signals are consumed and deleted by other workers before a notifications consumer could see them. Designing creation without designing resolution would have produced a data model that cannot satisfy FR-011a, since resolve needs to find *all unread siblings by subject*, which requires an index the "creation-only" model would not have had.

**Consequence for the data model**: notifications need `(tenant_id, subject_type, subject_id)` indexed for unread rows, so a resolve is one statement rather than a scan. See [data-model.md](data-model.md).

**Resolve fires at four sites, not two.** A queued escalation can be taken manually *or* automatically, and both must clear the other recipients' badges:

| Site | File | Why |
|---|---|---|
| `claim_in_tx` | `escalations/routing.rs:220` | manual claim |
| `drain_one_for_membership_in_tx` | `escalations/routing.rs:278` | auto-drain when an agent becomes available |
| `drain_any_in_tx` | `escalations/routing.rs:375` | auto-drain sweep |
| `approval::decide` | `tools/approval.rs` | tool request decided |

The two drain paths were missed in the first pass of this plan and are the easiest thing to half-wire: it is natural to add the *create* emit for the newly-assigned agent and forget the *resolve* for everyone else, which would leave stale unread badges for the whole team until retention. Both are folded into one helper (see R2a) so they cannot be wired apart.

### R2a. Escalation emits are collapsed into two helpers

Every escalation site ends in one of exactly two states, so rather than nine inline emit blocks that can subtly diverge, `escalations/src/routing.rs` gets two private helpers:

- `notify_escalation_queued(tx, …)` — fan-out create. One call site: the queued branch of `route_new_escalation_in_tx`.
- `notify_escalation_assigned(tx, …, assignee)` — create for the assignee **and** resolve everyone else's row for that escalation, in that order. Four call sites: the assigned branch of `route_new_escalation_in_tx`, `claim_in_tx`, `drain_one_for_membership_in_tx`, `drain_any_in_tx`.

Pairing create-and-resolve *inside* the helper is the point: a caller physically cannot wire one without the other. At the route-assigned site the resolve is a harmless no-op (nothing was queued), and at `claim_in_tx` the create is suppressed by FR-009 because the claimant is the actor.

### R2b. Expiry and cancellation do not resolve

**Decision**: Notifications are **not** resolved when a tool request expires (`tool_expiry_sweeper`) or an escalation is removed/cancelled. Only "claimed or decided" resolves, exactly as FR-011a says.

**Rationale**: FR-011b and SC-005 already require that clicking a stale notification lands on the settled or unavailable state rather than an error, so the click path handles staleness gracefully without a second resolve mechanism. Wiring resolve into the sweepers would add two more integration points and a background-job dependency for a case the UI already covers. Recorded so the omission reads as a decision rather than a gap.

**Alternatives considered**: resolving lazily at read time (when listing, check whether each subject is still open) was rejected — it turns every list request into a fan-out of joins across escalations/tool_requests, breaks SC-004, and makes the unread count non-trivial to compute.

---

## R3. How is the double-notify case (FR-009a) detected?

**Decision**: Skip emitting the assignment notification when the assignment's `origin` is `"escalations"`.

**Rationale**: This required no new plumbing — the discriminator already exists. `conversations::queries::assign_in_tx` takes an `origin: &str` parameter and records it on the emitted event (`queries.rs:918-…`, `outbox.rs:45`). Escalation routing calls `assign_in_tx` (`routing.rs:121`, `:258`, `:344`, `:423`) with the escalations origin, and the existing escalations consumer *already* branches on exactly this value to avoid reprocessing its own writes (`events.rs:276-283`). FR-009a follows the established precedent rather than inventing a mechanism.

**Verification that the collision is real**: `routing.rs:121` calls `conversations::queries::assign_in_tx` during auto-routing — not only on claim — and `assign_in_tx` emits `conversation.assignment_changed`. So routing genuinely produces both an escalation fact and an assignment fact for the same agent. FR-009a is not guarding a hypothetical.

---

## R4. Deduplication and the AI-failure suppression window

**Decision**: A `dedupe_key TEXT NOT NULL` column with `UNIQUE (recipient_membership_id, dedupe_key)`, and all inserts use `ON CONFLICT DO NOTHING`.

**Rationale**: Puts FR-010 in the database, where a retried or replayed event cannot defeat it, rather than in application logic that would need its own read-then-write race handling. Key formats:

| Kind | `dedupe_key` |
|---|---|
| Escalation (routed or queued) | `escalation:<escalation_id>` |
| Conversation assigned | `assigned:<conversation_id>:<assigned_membership_id>` |
| Tool approval required | `tool_approval:<tool_request_id>` |
| AI response failed | `ai_failed:<conversation_id>:<unix_minute / 15>` |

The AI-failure key folds a 15-minute time bucket into the key, so the spec's suppression assumption is enforced by the same unique index that enforces dedup — no separate throttle state, no timer. The trade-off is bucket-boundary behavior (two failures either side of a boundary both notify), which is acceptable for a coarse anti-flood rule.

---

## R5. Live delivery and the SSE permission gate

**Decision**: Reuse `GET /tenant/events`. Add a `presence::Event::NotificationCreated` variant carrying **primitive fields only** (`membership_id`, `notification_id`, `unread_count`), filtered per-recipient in `GuardedStream`.

**Rationale**: The stream already exists, already multiplexes event types, and already demonstrates per-member filtering — `AvailabilityChanged` is delivered only when `ev.membership_id == self.membership_id` (`events.rs:63-77`). Carrying primitives rather than a notification DTO means `escalations` does not take a dependency on `notifications`, keeping the module graph acyclic.

**Verified — the gate excludes no recipient**: `/tenant/events` is guarded by `conversations.view` (`server/src/router.rs:426-427`; asserted in `server/tests/rbac.rs:77`). Checking `authz/src/matrix.rs:126-132`: `TENANT_ADMIN`, `TENANT_MANAGER`, `TENANT_AGENT`, and `TENANT_VIEWER` all include `Permission::ConversationsView`, and Owner maps to `Permission::TENANT` (the full tenant set). Every role that can receive a notification can also hold the live channel open.

**Documented nuance**: FR-012a says the inbox needs no permission beyond membership. That is true of the REST endpoints, which are the system of record. Live *push* additionally rides a `conversations.view`-gated stream. Since all tenant roles hold that permission today the distinction is currently invisible, but if a future role loses `conversations.view` its inbox must still work — the REST endpoints must therefore never be gated on it.

**Worker cadence**: the escalation worker sleeps 1 s when idle (`events.rs:503-514`). Matching that gives worst-case ~1 s from commit to broadcast, comfortably inside the 5 s of SC-002/SC-009.

---

## R6. Transactional integrity of the five emit sites

**Decision**: Provide two emit helpers — `emit_requested_in_tx(&mut Transaction, …)` and `emit_requested_on_pool(&PgPool, …)` — and use whichever the call site supports.

**Findings** (this is why the helper is split):

| Trigger | Site | Context | Atomic? |
|---|---|---|---|
| Escalation routed | `escalations/routing.rs:121` | inside `tx` | Yes |
| Escalation queued | `escalations/routing.rs` (no-candidate queue path) | inside `tx` — **not directly verified** | Confirm at task time |
| Conversation assigned | `conversations/queries.rs::assign_in_tx` | inside `tx` | Yes |
| Tool approval required | `ai/engine.rs:687` (status `awaiting_approval`) | pool | **No** |
| AI response failed | `ai/engine.rs:1248`, `:1525` via `generation_record::insert(pool, …)` | pool | **No** |

`generation_record::insert` takes `pool: &PgPool` (`generation_record.rs:50`), not a transaction, so the last two emissions cannot be made atomic with their domain write without restructuring the AI engine's persistence layer.

The routed-escalation and assignment rows were read directly; the pool-based rows were confirmed from `generation_record::insert`'s signature. The queued-escalation branch was **inferred** from the surrounding transactional structure rather than read — `/speckit-tasks` must confirm which branch emits the fan-out and whether a transaction is in scope there, not assume it.

**Accepted limitation**, recorded in plan.md: a crash between the domain write and the emit loses that one notification. Not fixed here because the fix is disproportionate to the failure mode.

---

## R7. Crate naming collision

**Decision**: Rename the existing `notifications` crate to `email`; give the new feature the `notifications` name.

**Rationale**: The existing crate is an email transport port (`EmailSender`, `EmailMessage`, SMTP + no-op impls) built for team invitations — its own doc comment describes it as "Email delivery — port and implementations." It is live code, not a placeholder, and `email` describes it accurately. Leaving it named `notifications` while adding the real notifications feature would leave two differently-meaning `notifications` in one workspace.

**Blast radius, measured**: two `Cargo.toml` dependency lines (`server`, `tenancy`), ~10 references in `tenancy/src/invitations.rs`, ~8 in `server/tests/team_members.rs`, and the `EmailSender` wiring in `server/src/main.rs`. All are compile-checked — a missed reference is a build error, not a runtime surprise, which is what makes this rename low-risk despite touching several files.

**Alternative considered**: name the new crate something else (`inbox`, `user_notifications`) and leave the email crate alone. Lower blast radius, but bakes an awkward name into a long-lived module to avoid a mechanical, compiler-verified rename.

---

## R8. What already exists on the frontend

**Finding**: The bell is **not** greenfield. `layout/topbar/topbar.component.ts:72-76` already renders a bell with a badge, and `core/realtime/notifications.service.ts` backs it with an ephemeral `inAppSignal` counter that increments on `escalation.assigned` SSE events and fires a browser `Notification` when the tab is hidden. `frontend/CLAUDE.md` documents the bell as "purely visual (no handlers) until later specs" — this is that spec.

**Decision**: **Replace**, not layer.

- `inAppSignal` is deleted and the badge reads `notificationsStore.unreadCount()`. Keeping both would double-count, since the same escalation would increment the local counter *and* arrive as a server notification.
- The browser Notification API integration is **kept** but re-pointed at the new notification stream, so it fires for all four kinds instead of only `escalation.assigned`. Desktop notifications are not in the spec, but they are existing user-facing behavior and silently dropping them would be a regression. This is a deliberate carry-over, flagged here so it is reviewed rather than assumed.

---

## Open items for `/speckit-tasks`

None blocking. Two items are deliberately deferred and recorded rather than resolved:

1. Extracting `members_with_permission` into a `tenancy` application service (Complexity Tracking in plan.md).
2. Making the two pool-based emit sites transactional (R6).
