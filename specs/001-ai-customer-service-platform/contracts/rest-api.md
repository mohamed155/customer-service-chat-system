# Contract: REST API (v1)

Base path: `/api/v1`. These conventions bind every endpoint (spec API-001..010);
API contract tests in CI verify them per endpoint.

## Conventions

### Authentication
| Caller | Mechanism | Header |
|--------|-----------|--------|
| Dashboard user | Opaque session token | `Authorization: Bearer <session>` |
| Programmatic | Scoped API key | `Authorization: Bearer <api_key>` |
| Widget | Short-lived signed widget token | `Authorization: Bearer <widget_token>` |

Every request is authorized server-side against role + tenant scope. Platform
users operating via Tenant Switcher carry the assumed tenant in their session;
all such requests are audit-tagged.

### Error envelope (all non-2xx)
```json
{
  "error": {
    "code": "validation_failed",
    "message": "Human-readable summary.",
    "details": [{ "field": "email", "code": "invalid_format", "message": "..." }],
    "request_id": "req_01J..."
  }
}
```
Status usage: 400 validation, 401 unauthenticated, 403 unauthorized/cross-tenant
(indistinguishable from 404 for tenant-scoped resource probing), 404 not found,
409 conflict (incl. optimistic-lock), 422 semantic rejection, 429 rate-limited
(with `Retry-After`), 5xx server. Unknown filter/sort params → 400, never
silently ignored.

### Pagination (all list endpoints)
Request: `?limit=25&cursor=<opaque>` (limit ≤100).
Response envelope:
```json
{ "items": [...], "next_cursor": "c_...", "has_more": true }
```

### Filtering & sorting
Typed, documented params per endpoint: equality (`status=open`), ranges
(`created_after`/`created_before`), sets (`tag=a&tag=b`), free-text (`q=`)
where supported. Sorting: `sort=<field>&order=asc|desc` over a per-endpoint
whitelist; default newest-first.

### Idempotency
All POST/PATCH that create or mutate billable/externally-visible state accept
`Idempotency-Key: <uuid>`; replays return the original result with
`Idempotency-Replayed: true`.

### Other
- Timestamps ISO 8601 UTC; IDs opaque strings.
- Every response carries `X-Request-Id`.
- Rate-limit headers: `X-RateLimit-Limit/Remaining/Reset`.
- Versioning: breaking change ⇒ `/api/v2`, ≥6-month deprecation window.

## Endpoint catalog (by module, with milestone)

Roles column = minimum tenant role unless prefixed `P:` (platform role).

### identity (M1)
| Method & Path | Purpose | Access |
|---|---|---|
| POST /auth/signup | Create identity + tenant (self-serve) | public |
| POST /auth/login · /auth/logout | Session start/end | public / any |
| POST /auth/verify-email · /auth/password-reset[/confirm] | Flows | public |
| POST /auth/2fa/setup · /auth/2fa/verify | TOTP | self |
| GET/DELETE /auth/sessions[/{id}] | List/revoke sessions | self |
| GET/POST/DELETE /api-credentials[/{id}] | API keys | Admin |
| POST /invitations · POST /invitations/{token}/accept · DELETE /invitations/{id} | Invites | Admin / public / Admin |

### tenancy & platform (M1, provider config M3, flags/health M7)
| Method & Path | Purpose | Access |
|---|---|---|
| GET/PATCH /tenant | Own tenant profile/settings | Admin (read: Viewer) |
| POST /tenant/deletion-request · POST /tenant/deletion-confirm | Two-step delete | Owner |
| GET/POST /platform/tenants · GET/PATCH /platform/tenants/{id} | Directory, provision, limits | P:sales+ |
| POST /platform/tenants/{id}/switch · DELETE /platform/switch | Enter/exit Tenant Switcher | P:per-role grant |
| GET/POST/PATCH /platform/users[/{id}] | Platform staff | P:super_admin |
| GET/PUT /platform/providers[/{id}] · GET/PUT /platform/routing-policy | AI provider config | P:super_admin (read: developer) |
| GET/POST/PATCH /platform/flags[/{key}] · /platform/flags/{key}/overrides | Feature flags | P:super_admin |
| GET /platform/health · GET/POST/PATCH /platform/incidents[/{id}] | Health & incidents | P:developer+ |

### users (M1; skills M5)
| Method & Path | Purpose | Access |
|---|---|---|
| GET /users · GET/PATCH /users/{id} | Memberships in tenant | Admin (self-PATCH: any) |
| PATCH /users/{id}/role · POST /users/{id}/deactivate | Role mgmt | Admin (last-Owner guard) |
| GET/PATCH /me · GET /me/tenants | Profile, tenant contexts | self |
| PUT /me/availability · PUT /users/{id}/skills | Agent status, skill tags | Agent / Admin |

### customers (M2)
| Method & Path | Purpose | Access |
|---|---|---|
| GET /customers · GET/PATCH /customers/{id} | Directory, attributes | Manager (Agent: served only) |
| POST /customers/{id}/gdpr-delete | Privacy deletion | Admin |

### conversations & escalations (M2; routing M5)
| Method & Path | Purpose | Access |
|---|---|---|
| GET /conversations | Search/filter/sort (status, channel, assignee, customer, tag, dates, q) | Manager (Viewer read) |
| GET /conversations/{id} · GET /conversations/{id}/messages | Detail + history (paginated by seq) | per role |
| POST /conversations/{id}/messages | Agent/system message (idempotent) | Agent |
| POST /conversations/{id}/notes · /tags · /assign · /resolve · /close · /reopen | Ops | Agent/Manager |
| GET /conversations/{id}/timeline | AI execution timeline (M3) | Manager+ (P:developer via switcher) |
| GET /escalations?status=queued | Queue view | Agent |
| POST /escalations/{id}/claim · /return-to-ai (M5) | Handoff ops | Agent |
| GET /widget/session (POST) · POST /widget/messages · GET /widget/conversations/{id}/messages | Widget surface (widget token; rate-limited; idempotent sends) | widget |
| POST /widget/csat | 1–5 rating + comment | widget |

### ai & prompts (M3)
| Method & Path | Purpose | Access |
|---|---|---|
| GET/PATCH /agent-config | Behavior constraints, thresholds, rules, scopes | Admin |
| GET/POST /prompt-versions · GET /prompt-versions/{id} | Drafts + history | Admin |
| PATCH /prompt-versions/{id} | Edit draft (optimistic lock, 409 on conflict) | Admin |
| POST /prompt-versions/{id}/publish · /rollback | Version lifecycle (audited) | Admin |
| POST /sandbox/sessions · POST /sandbox/sessions/{id}/messages | Sandbox testing | Admin |

### knowledge (M4)
| Method & Path | Purpose | Access |
|---|---|---|
| GET/POST /knowledge/collections[/{id}] | Collections | Manager |
| GET/POST /knowledge/sources · GET/PATCH/DELETE /knowledge/sources/{id} | Sources (upload via presigned S3 flow), status polling | Manager |
| POST /knowledge/sources/{id}/reingest · /disable · /enable | Lifecycle | Manager |
| POST /knowledge/retrieval-test | Question → ranked passages | Manager |
| GET /knowledge/usage | Quota consumption | Manager |

### tools & integrations (M5; widget config M2)
| Method & Path | Purpose | Access |
|---|---|---|
| GET/POST/PATCH /tools[/{id}] · POST /tools/{id}/enable · /disable | Tool registry | Admin |
| GET/PUT /widget-config · GET /widget-config/snippet | Widget appearance + embed | Admin |
| GET/POST/PATCH/DELETE /webhooks[/{id}] · GET /webhooks/{id}/deliveries | Subscriptions + delivery log | Admin |

### notifications (M2+)
| Method & Path | Purpose | Access |
|---|---|---|
| GET /notifications · POST /notifications/{id}/read · POST /notifications/read-all | Center | self |
| GET/PUT /me/notification-preferences | Prefs (security categories locked) | self |

### analytics (M6)
| Method & Path | Purpose | Access |
|---|---|---|
| GET /analytics/overview · /conversations · /csat · /topics · /knowledge-gaps | Dashboards (range + segment params; freshness field) | Manager (Viewer read) |
| POST /analytics/exports · GET /analytics/exports/{id} | Async CSV export | Manager |
| GET /platform/analytics/* | Cross-tenant aggregates (no content) | P:per role |

### billing (M6)
| Method & Path | Purpose | Access |
|---|---|---|
| GET /billing/subscription · POST /billing/subscription/change-plan | Plan mgmt (idempotent; proration rules) | Owner |
| GET /billing/usage · GET /billing/invoices[/{id}] | Usage vs plan, invoices | Owner |
| GET/POST/PATCH /platform/plans[/{id}] | Plan catalog | P:finance/super_admin |
| POST /platform/billing/adjustments | Credits/refunds (audited) | P:finance |
| POST /billing/processor-webhook | Payment processor callbacks (signature-verified) | external |

### audit (M1)
| Method & Path | Purpose | Access |
|---|---|---|
| GET /audit-events | Tenant audit search (action, actor, target, dates) | Admin (read-only) |
| GET /platform/audit-events | Platform-wide | P:super_admin |
