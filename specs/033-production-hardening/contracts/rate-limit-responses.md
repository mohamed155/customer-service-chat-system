# Contract: Rate-Limit Refusal Responses

**Feature**: 033-production-hardening | **Requirements**: FR-001 – FR-008 | **SC**: SC-001, SC-002

Reuses the existing `ApiError` / `ErrorEnvelope` shape (Principle V) — this contract adds
headers and fixes semantics, it does not introduce a new error format.
`ApiError::rate_limited` already exists and is used by the widget scope today.

---

## Refusal response

```
HTTP/1.1 429 Too Many Requests
Retry-After: 42
Content-Type: application/json
```

```json
{
  "error": {
    "code": "rate_limited",
    "message": "Too many requests. Please retry later."
  }
}
```

**`Retry-After`** is seconds until the window resets. Required by FR-005 ("MUST tell the
caller when retrying is permitted") and currently **not** emitted by the existing widget
limiter — this is a fix, not just an extension.

**The body must not disclose** the limit, remaining budget, the identifier the limit was
keyed on, or any other tenant's activity (FR-005). The message is constant regardless of
which scope refused.

`X-RateLimit-*` headers are deliberately **not** emitted on refusals — they disclose the
budget, which tells an attacker exactly how to pace themselves under it.

---

## Sign-in refusal (FR-001) — account enumeration

The sign-in throttle is keyed on `(account_identifier, source_address)`. The refusal
response must be **indistinguishable between an existing and a non-existent account**:
same status, same body, same headers, and the same response timing envelope.

This is the requirement most easily broken by a natural implementation: throttling only
requests that reach the password-verification step means an attacker learns that
throttled ⇒ account exists. **The counter must increment before the account lookup
result is known.**

## Store-unavailable behaviour (FR-007)

| Scope | Counter store unreachable |
|-------|---------------------------|
| `login` | **Fail closed** — `503 Service Unavailable`, `Retry-After: 30` |
| all others | Degrade to per-instance in-memory counting; request proceeds under the local budget |

Both paths log at `WARN` and increment `rate_limit_degraded_total{scope}`. Rationale in
`research.md` §2.

## Budget resolution order

1. `tenant_rate_limit_overrides` row for `(tenant_id, scope)`, if present and not soft-deleted
2. Environment configuration (`APP_RATE_LIMIT_*`)
3. Compiled default

Overrides above `APP_RATE_LIMIT_MAX_OVERRIDE` are rejected at write time, so protection
cannot be disabled by configuration error.

## Identifier selection (FR-004)

| Caller | Keyed on |
|--------|----------|
| Authenticated | authenticated subject (user or tenant) |
| Anonymous | client address from the leftmost `X-Forwarded-For` entry |

Authenticated identity takes precedence so that many users behind one corporate gateway
are not collectively locked out (spec edge case). **`X-Forwarded-For` is only trustworthy
because Caddy overwrites it at the edge** — the current implementation in
`server/src/rate_limit.rs` reads it unconditionally, which is spoofable when nothing
sanitises it. Fixing this is part of FR-004.

## Existing budgets (retained)

| Scope | Budget | Change |
|-------|--------|--------|
| `widget_session` | 10 / 60 s per address | now shared across instances |
| `widget_message` | 10 / 60 s per session | now shared across instances |
| `tenant_global` | 600 / 60 s per tenant | now shared across instances |
| `login` | 10 / 60 s per account+address | **new** |
| `webhook` | 600 / 60 s per connection | **new** |

Values are unchanged from `server/src/rate_limit.rs`; what changes is that they become one
budget rather than one per process.
