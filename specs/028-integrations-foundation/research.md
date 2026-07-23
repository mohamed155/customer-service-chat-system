# Phase 0 Research: Integrations Foundation

All findings below were verified against the code at `028-integrations-foundation` planning time. File and line references are the evidence; re-check them if the surrounding code has moved.

---

## R1. Does the codebase already give us the secrets-encryption pattern we need?

**Decision**: Copy `backend/crates/ai-providers/src/crypto.rs` (AES-256-GCM, `MasterKey`/`seal`/`open`/`hint`) into `backend/crates/modules/integrations/src/crypto.rs` with two surgical changes:

- `open` returns `String` instead of `SecretKey` (the integrations crate must not depend on `ai-providers` — that would be a workspace layering inversion).
- `aad` becomes `pub fn aad(tenant_id: uuid::Uuid, slug: &str, field_key: &str) -> String` returning `format!("integration|{tenant_id}|{slug}|{field_key}")` (no `Option` — integrations are always tenant-scoped).

**Rationale**: Inventing a new crypto layer for the same primitive (`AES-256-GCM + AAD + hint`) would create two slightly different key-management code paths in the same project. The ai-providers one is already battle-tested by 015. Reusing it keeps `APP_INTEGRATION_SECRETS_KEY` in the same form as `APP_AI_KEY_ENCRYPTION_KEY` (base64, 32 bytes) so the operator's mental model is the same.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Depend on `ai-providers` directly and call its `seal`/`open` | The `ai-providers` crate is a leaf, and `integrations` would be the only consumer. Doing it in a leaf instead of `integrations` would also force every future module that needs at-rest crypto to depend on `ai-providers` and inherit any future AI-specific behaviour from it. |
| Reuse the `tenancy` crate's `secret_keys` table | `tenancy::secrets` is designed for one-secret-per-tenant API keys, not for per-connection / per-field AES payloads. The integration_secrets table needs per-row AAD (tenant + slug + field_key) which the tenancy helper does not expose. |
| Hash-only / store only the hint | The detail endpoint has to be able to redisplay the webhook URL (FR-008), so the token needs to be recoverable — i.e. encrypted, not hashed. The signing secret used to verify inbound HMACs is also encrypted so future read-back of the full value is possible without a re-keying migration. |

---

## R2. Where do the audit writers go?

**Decision**: Mirror `backend/crates/modules/widgets/src/audit.rs` exactly — four `record_*_in_tx` functions (`record_connected_in_tx`, `record_config_updated_in_tx`, `record_secret_rotated_in_tx`, `record_disconnected_in_tx`), each calling `tenancy::audit::record_in_tx` with `RESOURCE_TYPE = "integration_connection"` and an action under the `integration.` prefix. The audit module's `CATEGORY_PREFIXES` constant gains one new entry: `("integrations", &["integration."])`, appended after the `widgets` entry.

**Rationale**: The `widgets` module is the only one so far that writes its own audit category; reusing its shape (and its `RESOURCE_TYPE`/action naming) keeps every new module cheap to onboard. The `CATEGORY_PREFIXES` extension is the only module-mutating change and is what `category_for_action` consults to bucket audit rows for the UI.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Make `audit` know about every module's resource types | That would force every new feature to edit `audit::model::CATEGORY_PREFIXES` AND `audit::routes` to filter for the right category. The prefix-based approach is already in place and tested by 5+ modules. |
| Skip the audit trail for connect/update/disconnect | Violates SC-004 (100% of lifecycle actions in the audit trail) and FR-012. |

---

## R3. How do we keep the webhook intake from leaking connection existence?

**Decision**: Every 404 branch of the public `POST /hooks/v1/{token}` handler returns the **byte-identical body**. The four 404 paths (unknown token, inactive connection, bad signature, and any other lookup miss) are merged into a single `404` response. Only the **event** rows written differ — and only when the connection is identifiable:

- Unknown token ⇒ 404, no event.
- Inactive connection ⇒ 404 + `delivery_rejected`/`inactive_connection` event.
- Bad signature ⇒ 404 + `delivery_rejected`/`invalid_signature` event.

**Rationale**: FR-009 forbids leaking information about existing connections through differential error responses. A bot scanning tokens can otherwise distinguish "the token resolves to a real connection that is currently inactive" from "the token is unknown" by status code or body shape. The byte-identical body removes the side channel.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Return 401 for bad signature and 404 for inactive | Leaks connection existence; both are real connections. |
| Use opaque request IDs in the body and log the mapping server-side | Adds a mapping table that the rest of the system has to consult for incident response; the byte-identical body is sufficient on its own. |
| Don't log the rejection at all | Loses observability for legitimate debugging; instead, throttle the rejection event rows to one per connection per reason per minute (see R4). |

---

## R4. Won't unbounded rejection traffic blow up the event log?

**Decision**: The two rejection reasons that are reachable **without** a valid signature (`rate_limited`, `inactive_connection`) are throttled to at most one event row per connection per reason per minute. The other two rejection reasons (`invalid_signature`, `malformed_payload`) are not throttled because they are only reachable after the per-connection 60/min limit has already been applied.

The throttle is implemented as a call into the same `kernel::InMemoryRateLimitStore` that the rest of the platform uses for rate limiting, with a budget of `1` request per `60` seconds keyed by `integration_evt:{connection_id}:{reason}`.

**Rationale**: A misbehaving external system sending a flood of stale or wrong signatures would otherwise generate unbounded `integration_events` rows precisely when the platform is trying to shed load. The status code is always returned (we never lie to a caller about why their request failed), but the persistent record is throttled to keep the database healthy. The other two rejection reasons are not throttled because they can only happen after a valid signature was already provided — at that point the connection has already proven it can authenticate, so the per-connection 60/min cap bounds event growth.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Don't write rejection events at all | Loses the signal admins need to diagnose repeated failures. |
| Throttle all four rejection reasons uniformly | Hides `invalid_signature` floods — an attacker guessing tokens would generate no log signal at all. |
| Use a per-tenant throttle key | Multi-tenant isolation breaks: one tenant's misbehaving source would silence events for other tenants' connections sharing a backend. The connection-id-keyed throttle is the right granularity. |

---

## R5. How do we keep `list_integrations` from being an N+1?

**Decision**: `list_integrations` calls `queries::list_catalog_with_status(&pool, ctx.tenant_id)` **exactly once**. That single query uses a `LEFT JOIN LATERAL` to pull, per catalog row, the connection row (if any) and the connection's 3 most recent event outcomes (last 24 h). The handler derives each row's status using those outcomes in-process. **No per-row query is performed inside the loop.**

```sql
SELECT cat.*, c.id AS connection_id, c.is_active, ev.outcomes
FROM integration_catalog cat
LEFT JOIN integration_connections c
  ON c.catalog_id = cat.id AND c.tenant_id = $1
LEFT JOIN LATERAL (
  SELECT array_agg(e.outcome ORDER BY e.created_at DESC) AS outcomes
  FROM (
    SELECT outcome, created_at FROM integration_events
    WHERE connection_id = c.id AND created_at > now() - interval '24 hours'
    ORDER BY created_at DESC LIMIT 3
  ) e
) ev ON TRUE
ORDER BY cat.name;
```

`get_integration` (the detail endpoint, single-connection) calls a separate `recent_event_outcomes` helper — there is exactly one connection in play, so the per-row cost is paid once per request.

**Rationale**: Constitution Principle X ("Performance & Efficiency") lists N+1 as a defect. A catalog of ~10 entries × one events query per row = 10 round-trips per list call. The lateral-join shape is also used by 025's analytics timeseries query and is the established pattern in this codebase for "give me the parent row plus a small derived aggregate from a child table in one round trip."

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Pull all connections, then all events, and zip in Rust | Two queries, but the events query has no tenant filter on its own, so it pulls every event in the system and relies on a Rust-side filter. The lateral join is cheaper and more correct. |
| Caching status with a 30 s TTL | Adds a cache layer, eviction policy, and a tenant-isolation bug surface for a status that's read on every list page load. The lateral join is fast enough at this scale. |
| Skip the 24h events window | Loses SC-006 (error state visible within 1 minute). The lateral join handles it without code complexity. |

---

## R6. What is the existing `Permission::IntegrationsView` / `Permission::IntegrationsManage` matrix, and do we need to change it?

**Decision**: Reuse unchanged. Verified at `backend/crates/modules/authz/src/matrix.rs:14-15, 33-34, 54, 96`:

- `Permission::IntegrationsView` is granted to Owner, Admin, Manager, Viewer (and the platform staff Developer in tenant context).
- `Permission::IntegrationsManage` is granted to Owner, Admin, Manager.
- Agent gets neither.

**Rationale**: The clarification session confirmed that the existing matrix already encodes the intended RBAC story (Owner/Admin/Manager manage; Viewer read-only; Agent no access). Adding a new permission would force every existing role-matrix test to learn about the new permission, and the dashboard's `PAGE_PERMISSIONS` map would have to be re-validated. Reusing the existing `integrations.view` / `integrations.manage` permissions also keeps the front-end permission gate (`requirePermission: 'integrations.view'`) trivial.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Add a new `integrations.admin` permission just for US2's connect flow | Splits the matrix without a reason; the existing `integrations.manage` already covers it. |
| Make Agent a view-only role on integrations | The spec clarification explicitly excluded this. |

---

## R7. What about reusing the `widgets` audit pattern verbatim vs. the notifications pattern?

**Decision**: Use the **`widgets` pattern** (`backend/crates/modules/widgets/src/audit.rs`), not the notifications pattern. The notifications module does not have its own `audit.rs` — it calls `tenancy::audit::record_in_tx` directly from `notifications::worker`. The widgets module's `audit.rs` is the closest analog: it has four `record_*_in_tx` functions and an in-crate constant for the action strings.

**Rationale**: Integrations has four discrete lifecycle actions (`connected`, `config_updated`, `secret_rotated`, `disconnected`) — same shape as widgets' `created`/`updated`/`deleted`. Co-locating the four writers in `integrations::audit` keeps the call sites short and the action strings next to the writers that own them.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Inline `tenancy::audit::record_in_tx` calls in each handler | Spreads the action strings across four files; the in-crate `audit.rs` is the established home for them (mirrors widgets). |

---

## R8. Where do we mount the public `POST /hooks/v1/{token}` route?

**Decision**: As a separate `hooks_routes()` function in `backend/crates/server/src/router.rs`, merged into the app router with its own `RequestBodyLimitLayer` of 256 KB. The handler lives in `integrations::webhook` (a new module file), and the existing `Arc<kernel::InMemoryRateLimitStore>` is cloned so the hooks router can `Extension(rate_store)` it the same way `widget_router` does.

**Rationale**: The webhook endpoint is unauthenticated (public) and rate-limited. It cannot share the tenant middleware (which requires `X-Tenant-ID`); it needs to be parallel to the widget public router rather than nested inside it. The body-limit layer is added at the router level rather than in the handler so the limit applies before any work is done (no large-body HMAC verification attempts, no large-body JSON parsing).

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Reuse `widget_routes()` and add a new sub-router inside | Widget routes assume a `session_id` URL parameter; mixing the two would force a generic `path` parameter that adds zero value. |
| Use a tower middleware on the global app router | The 256 KB cap is per-endpoint, not platform-wide. |
| Don't cap the body size | FR-014 requires it. |

---

## R9. What about the FR-005 confidentiality invariant — is the existing `crypto::open` enough?

**Decision**: Yes. The token used for HMAC is also stored encrypted (not just hashed), so the detail endpoint can `crypto::open` it and rebuild the webhook URL. The hash (`webhook_token_hash`) is the *index* for the public intake handler — it must be unidirectional so the intake lookup is O(1) and doesn't need the master key. The signing secret in `integration_secrets.ciphertext` is encrypted so the intake handler can decrypt it per-request to verify the HMAC.

This means the table carries both an encrypted copy (for read-back) and a hash (for lookup) of the same webhook token. The task description explains why this is necessary: the detail page has to redisplay the URL, and HMAC verification has to happen against the original secret.

**Rationale**: Two different access patterns, two different storage shapes — that's the right trade-off. Storing only the hash would make the URL unrecoverable; storing only the ciphertext would force a linear scan of all `integration_connections` rows on every intake.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Store only the hash, regenerate the URL with a placeholder token | Loses the URL entirely on detail reload — admins would have no way to share the URL. |
| Store only the ciphertext, scan all connections to find the right one | O(N) on every delivery, where N grows with the number of connected tenants. Catastrophic at scale. |
| Derive a stable ID from the token, store the ID + ciphertext | Adds a third storage shape and a new key-derivation function. The hash+ciphertext pair is the simplest correct shape. |

---

## R10. Where does the 90-day retention sweeper live?

**Decision**: Mirror the notification retention sweeper exactly (`backend/crates/server/src/main.rs` spawn pattern). The sweep function lives in `integrations::retention` and is started by the server with the same shutdown/join-set integration as `notification_retention_sweeper`. Logs go through the same tracing channel.

**Rationale**: FR-015 requires 90-day retention; the established platform convention (per the assumption in spec.md) is to mirror notifications' retention. Reusing the spawn shape means a future operator can grep for "retention" and find both.

**Alternatives considered**:

| Alternative | Rejected because |
|---|---|
| Run as a separate cron / external job | Adds a deployment surface for what's already a periodic DB query. The server is the natural owner. |
| Use pg_cron | Adds an infra dependency the rest of the platform doesn't have. |
