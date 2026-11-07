# Threat model (v1, STRIDE)

- **Version:** 1.0 (2026-11-07, MOH-18)
- **Scope:** the AI Customer Service Platform as built today plus the decisions in ADR-001 to
  ADR-015: backend modules, `api`/`gateway`/`worker` units, dashboard, widget, data stores,
  third-party providers and CI/CD.
- **Method:** STRIDE per module. For each threat: the controls, whether each control is
  **in place** or a **gap** (with the ADR row that tracks it), and an owner.
- **Review:** update this document when a spec changes a trust boundary, adds a module or an
  external integration, and at least once per quarter. Each spec's security section references
  the rows it affects (constitution Principle V).

## 1. Assets

| Asset | Why it matters |
| -- | -- |
| Tenant conversations, messages, attachments | Customer PII and business-confidential content; cross-tenant exposure is the worst-case failure |
| Customer profiles and channel identifiers | PII (names, emails, phone numbers) |
| Knowledge base documents and embeddings | Tenant-confidential; also an injection vector into the AI |
| Tenant secrets (AI keys, channel tokens, OAuth tokens, tool credentials) | Direct access to tenants' third-party accounts and spend |
| User credentials, sessions, API keys | Account takeover |
| Platform operator access | Can reach every tenant |
| Audit log | Accountability and incident forensics |
| AI spend and quotas | Direct financial loss for tenants and the platform |
| Availability of inbox and widget | Support channels must stay up |

## 2. Trust boundaries

```
  Internet (untrusted)                       Our infrastructure                     Third parties
  ───────────────────                        ─────────────────                      ─────────────
  Host websites + widget  ──(B1)──▶  gateway ─┐
  Channel providers       ──(B2)──▶  (webhooks)│                                    AI providers
  (WhatsApp, Telegram)                         ├──▶ PostgreSQL (RLS) / Redis / S3 ──(B5)──▶ channels
  Dashboard users (browser)──(B3)──▶ api ──────┤                                    integrations
  Tenant servers (API keys)──(B3)──▶ api       └──▶ worker ──(B5)──────────────────▶ tenant URLs
  Platform operators       ──(B4)──▶ api (impersonation)                            KMS
  CI/CD, dependencies      ──(B6)──▶ build and release pipeline
```

B1 widget/public, B2 inbound webhooks, B3 authenticated API, B4 platform → tenant,
B5 outbound to third parties and tenant URLs (including the LLM, whose output is untrusted),
B6 supply chain.

## 3. Owners

Owners are roles until named people are assigned in `.github/CODEOWNERS` (ADR-015 L6).

| Owner | Covers |
| -- | -- |
| **Identity & Access** | `identity`, `authz`, `rbac`, `tenancy`, sessions, API keys |
| **Conversations** | `conversations`, `customers`, `escalations`, `feedback`, `notifications`, realtime |
| **Channels** | `widgets`, `whatsapp`, `telegram`, `gateway`, `email` |
| **AI Platform** | `ai`, `ai-providers`, `tools`, `knowledge`, `prompts` |
| **Platform Engineering** | data layer, async processing, `integrations`, `billing`, `audit`, `platform`, infra, CI/CD |
| **Frontend** | dashboard and widget apps |
| **Security lead** | this document, threat reviews, incident response |

## 4. STRIDE per module

Legend: **✓** control in place · **Gap** missing or partial (tracked where shown).

### 4.1 Identity and sessions (`identity`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Credential stuffing, password guessing | Argon2id; `(email, ip)` login throttle; uniform error messages | ✓ throttle · Gap: per-account lockout (ADR-005 A7), MFA (A6) | Identity & Access |
| S | Account enumeration via login timing | Dummy hash on unknown email | **Gap**: dummy and real hash parameters differ, about 3× timing difference (ADR-005 A3, high) | Identity & Access |
| S | Session theft via XSS | HttpOnly cookie, CSP, no tokens in JS storage | ✓ HttpOnly, no JS storage · Gap: CSP is `report-only`, not enforced | Identity & Access |
| T | Forged session token | Signed JWT | ✓ HS256 · Gap: EdDSA + `kid`, verify-only keys in other units (ADR-005 A1) | Identity & Access |
| R | User denies a login or sensitive action | Audit records for login, logout, role changes | ✓ | Identity & Access |
| I | Long-lived token replay after theft | Short access token + rotating refresh with reuse detection | Gap: single 8 h token (ADR-005 A2) | Identity & Access |
| D | Login floods exhausting memory (Argon2id) | Rate limits, login concurrency cap | ✓ rate limit · Gap: concurrency semaphore (ADR-005 consequences) | Identity & Access |
| E | CSRF on cookie-authenticated requests | `SameSite=Lax`, strict `Origin` check, custom header | Gap: missing-`Origin` requests pass, `/auth/login` unchecked (ADR-005 A4) | Identity & Access |

### 4.2 Tenancy and authorization (`tenancy`, `authz`, `rbac`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Forged `X-Tenant-ID` to enter another tenant | Server-side membership check on every request; 404 for invisible tenants | ✓ | Identity & Access |
| T | Row written into another tenant | Composite FKs; RLS `WITH CHECK` | Partial: ~36 composite FKs, ~80 single-column (ADR-004 T3); **no RLS** (T1) | Platform Engineering |
| R | Untracked role or membership changes | Audit on role/membership changes | ✓ | Identity & Access |
| I | Cross-tenant read through a missed `WHERE tenant_id` | App-layer `TenantContext` + RLS (two layers) | **Gap (high)**: one layer only; the app DB user is a superuser, which bypasses RLS (ADR-004 T1/T2) | Platform Engineering |
| D | Noisy tenant starving others | Per-tenant rate limits, quotas, fair scheduling | Partial: rate limits ✓; fair job scheduling missing (ADR-007 Q2) | Platform Engineering |
| E | Privilege escalation via role names in code | Permission-based checks, declared per endpoint | ✓ (ADR-006) | Identity & Access |

### 4.3 Conversations and customers (`conversations`, `customers`, `escalations`, `feedback`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Agent acts as another agent | Actor from session, never from request body | ✓ | Conversations |
| T | Messages altered after the fact | Messages are append-only (no code path runs `UPDATE messages`) | ✓ | Conversations |
| R | Disputed assignment or resolution | Audit trail of assignment and status changes | ✓ | Conversations |
| I | IDOR on conversation or customer IDs | Tenant-scoped queries; endpoint isolation tests | ✓ per feature · Gap: shared isolation harness (ADR-004 T5) | Conversations |
| I | Stale data of a previous tenant shown after switch | Tenant-scoped stores reset on switch | Gap to confirm: `NotificationsStore` (ADR-003 F2) | Frontend |
| D | Escalation storms overfill agents | Atomic conditional capacity update in routing (§11.1) | Verify: routing is load-based under per-tenant advisory locks; confirm with a concurrency test | Conversations |
| E | Agent reads conversations outside their scope | Policy layer (assigned-only unless Manager) | See ADR-006 current-state table | Identity & Access |

### 4.4 Widget and public gateway (`widgets`, future `gateway`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Visitor impersonates another customer to read history | Hashed anonymous session tokens; identity-verification HMAC for logged-in customers | ✓ anonymous sessions · Gap: identity verification HMAC (architecture §6.3) | Channels |
| S | Widget embedded on unauthorised sites | Per-widget allowed origins | ✓ | Channels |
| T | Host page script tampers with the chat | Conversation in a cross-origin iframe; launcher in Shadow DOM | ✓ iframe · Gap: iframe `sandbox`, launcher Shadow DOM (ADR-011 W1/W3) | Frontend |
| I | Host page script reads session token or messages | Token stored on the widget origin only | ✓ (ADR-011 W5) | Frontend |
| D | Spam or floods from public endpoints | Rate limits per widget, visitor and IP; bot challenge escalation | ✓ rate limits · Gap: separate gateway unit (ADR-010 G1), bot challenge | Channels |
| E | Public request reaches internal endpoint | Separate routers per unit | Gap: one router serves both (ADR-010 G1) | Platform Engineering |

### 4.5 Channels (`whatsapp`, `telegram`, `email`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Forged webhook deliveries | HMAC signature verification, constant-time compare | ✓ WhatsApp | Channels |
| T | Replayed webhook | Timestamp window; dedupe by provider message ID | ✓ dedupe | Channels |
| R | Disputed delivery | Delivery status lifecycle stored (sent/delivered/read/failed) | ✓ | Channels |
| I | Media fetched from attacker URLs | Media fetched only from provider APIs; SSRF guard | ✓ provider API · verify SSRF guard coverage | Channels |
| D | Webhook storm overwhelming the API | Fast ACK + async processing; gateway isolation | ✓ async · Gap: gateway unit (ADR-010) | Channels |
| E | Channel token used beyond its tenant | Tokens encrypted with tenant-scoped AAD | ✓ AAD · Gap: envelope encryption (ADR-014 E1) | Platform Engineering |

### 4.6 AI runtime and providers (`ai`, `ai-providers`, `prompts`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Prompt injection makes the model claim to be staff or system | Instruction hierarchy; untrusted content delimited | ✓ | AI Platform |
| T | Published prompt changed silently | Immutable prompt versions | ✓ | AI Platform |
| R | Can't explain an AI answer | Turn record: provider, model, prompt version, usage, tool calls, citations | ✓ | AI Platform |
| I | Model leaks another tenant's data | Tenant-scoped retrieval only; no secrets in context; output filters | ✓ tenant-scoped retrieval · Gap: RLS on vector tables (ADR-004 T1) | AI Platform |
| D | Runaway token spend or provider outage | Per-tenant plan allowances; circuit breakers; fallback; safe escalation | ✓ billing enforcement and fallback/escalation · Gap: no circuit breakers or per-provider bulkheads (§10.2) | AI Platform |
| E | Model output treated as trusted commands | Tools are the only actions; server-side validation and authz | ✓ (see 4.7) | AI Platform |

### 4.7 Tools (`tools`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Tool call executed as a more privileged user | Required permission per tool, checked server-side | ✓ | AI Platform |
| T | LLM-crafted arguments reach other tenants' IDs | JSON Schema validation; IDs re-checked against tenant scope | ✓ | AI Platform |
| R | Disputed tool execution | `tool.executed` audit; approval records | ✓ | AI Platform |
| I | HTTP tool exfiltrates to attacker URL or hits internal network (SSRF) | HTTPS only, DNS resolution checks, private-range blocklist, re-check on redirect | ✓ SSRF guard · Gap: egress proxy in production (§12.2) | AI Platform |
| D | Tool loops | Max iterations and total time per turn | ✓ | AI Platform |
| E | Irreversible action without consent | Approval policy; irreversible tools need approval unless tenant opts in | ✓ | AI Platform |

### 4.8 Knowledge base (`knowledge`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | — | Uploads tied to authenticated tenant users | ✓ | AI Platform |
| T | Poisoned document injects instructions | Retrieved text delimited as untrusted; red-team eval set | ✓ delimiting · Gap: red-team set in CI (§10.5) | AI Platform |
| R | Unknown publisher of a harmful answer | Draft → published lifecycle with audit | ✓ | AI Platform |
| I | Unpublished or other-tenant chunks retrieved | `status = 'published'` + tenant filter in retrieval | ✓ · Gap: RLS (ADR-004 T1) | AI Platform |
| D | Huge or malicious files exhaust workers | Size limits; per-tenant ingestion caps; AV scan + quarantine | Partial: size limits ✓; AV scan, quarantine and fair scheduling missing (§7.4, ADR-007) | AI Platform |
| E | Malicious file exploits a parser | Magic-byte detection; parsing in worker, not API | Partial | AI Platform |

### 4.9 Integrations and secrets (`integrations`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Forged integration callbacks | Signed callbacks (constant-time compare) / OAuth state | ✓ inbound webhook signature check | Platform Engineering |
| T | Ciphertext moved between tenants | AES-GCM with tenant-scoped AAD | ✓ (ADR-014 E3) | Platform Engineering |
| R | Untracked connect/disconnect | Integration event log + audit | ✓ | Platform Engineering |
| I | Database leak exposes tenant secrets | Envelope encryption with KMS-held root key | **Gap (high)**: single static env key per secret type, no rotation (ADR-014 E1) | Platform Engineering |
| D | Third-party outage cascades | Timeouts, circuit breakers, health status | ✓ health status · Gap: no circuit breakers (§15) | Platform Engineering |
| E | Secrets returned to the dashboard | Write-only secrets, masked hints | ✓ | Platform Engineering |

### 4.10 Async processing and realtime (outbox, workers, SSE)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Forged events injected into the queue | Only server code writes outbox/jobs; DB credentials restricted | ✓ | Platform Engineering |
| T | Job payload tenant ID tampered | Payload from trusted producers; worker re-establishes tenant context + RLS | Partial: RLS missing (ADR-004 T1) | Platform Engineering |
| R | Lost history of processed events | Mark dispatched, keep for retention window | Gap: rows deleted after processing (ADR-007 Q3) | Platform Engineering |
| I | SSE stream receives another tenant's events | Tenant-scoped channels from server-side context | ✓ | Conversations |
| I | Stream stays open after logout or revocation | Close stream on revocation | Gap (ADR-009 RT3, ADR-005 A8) | Conversations |
| D | Duplicate or stuck jobs | Leases, retries with jitter, dead letters, inbox dedupe | Partial (ADR-007 Q2–Q5) | Platform Engineering |
| E | — | Workers use the same least-privilege DB role as the API | Gap until roles split (ADR-004 T2) | Platform Engineering |

### 4.11 Platform operations and audit (`platform`, `audit`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Operator acts in a tenant without trace | Impersonation sessions with reason, time box, banner | **Gap**: no impersonation sessions in code yet (ADR-006) | Identity & Access |
| T | Audit rows altered or deleted | Append-only table, no UPDATE/DELETE grants, SHA-256 hash chain | ✓ append-only trigger · Gap: no hash chain; grants depend on the DB role split (ADR-004 T2) | Platform Engineering |
| R | Operator denies an action | `actor` + `on_behalf_of_tenant` on every audited action | ✓ | Platform Engineering |
| I | Operators see more than they need | Separate platform permissions per role | ✓ (ADR-006) | Identity & Access |
| D | — | — | — | — |
| E | Compromised platform account reaches all tenants | Mandatory MFA for platform users; least privilege | **Gap (high)**: no MFA yet (ADR-005 A6) | Identity & Access |

### 4.12 Billing (`billing`)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Forged payment-provider webhooks | Signature verification | Verify when payment provider is integrated | Platform Engineering |
| T | Usage counts manipulated | Usage metered server-side per AI turn | ✓ | Platform Engineering |
| R | Disputed charges | Usage events kept and aggregated | ✓ | Platform Engineering |
| I | Billing data of other tenants | Tenant-scoped queries | ✓ · Gap: RLS | Platform Engineering |
| D | Quota bypass causing runaway cost | Quotas enforced before AI turns | ✓ | Platform Engineering |
| E | Tenant raises own plan limits | Plan changes require `billing.manage` + platform side | ✓ | Platform Engineering |

### 4.13 Web clients (dashboard, widget)

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Clickjacking or framing of the dashboard | HSTS, `X-Frame-Options` / `frame-ancestors` | ✓ `X-Frame-Options` (033) | Frontend |
| T | XSS via message or knowledge content | Angular escaping; no `bypassSecurityTrust*` without review; CSP | ✓ escaping · Gap: CSP is `report-only` | Frontend |
| R | — | — | — | — |
| I | Tokens or tenant data in `localStorage` | Cookies only for tokens; only theme and last tenant summary persisted | ✓ dashboard | Frontend |
| D | — | — | — | — |
| E | UI-only permission checks | Guards are UX; server enforces | ✓ | Frontend |

### 4.14 Infrastructure, CI/CD and supply chain

| STRIDE | Threat | Controls | Status | Owner |
| -- | -- | -- | -- | -- |
| S | Compromised GitHub Action or dependency | Actions pinned by SHA; lockfiles; `cargo audit`; secret scanning | ✓ lockfiles, `cargo audit`, secret scan · Gap: Actions pinned by tag (`@v4`), not SHA; no `cargo-deny` (ADR-002 S4) | Platform Engineering |
| T | Tampered release image | SHA-tagged images, release manifest | ✓ (033) · Gap: image signing, SBOM | Platform Engineering |
| R | Unknown change to production | Releases from CI only, manifest of deployed SHA | ✓ | Platform Engineering |
| I | Secrets in repo or logs | gitleaks secret scan; log redaction; `.env*` ignored | ✓ | Platform Engineering |
| D | Loss of database | PITR, daily snapshots, restore drills | ✓ backup/restore scripts (033) | Platform Engineering |
| E | App DB user can run DDL / bypass RLS | Separate migration and runtime roles | **Gap (high)** (ADR-004 T2) | Platform Engineering |

## 5. Top risks (ranked)

| # | Risk | Rows | Tracking |
| -- | -- | -- | -- |
| 1 | Cross-tenant data exposure through a single missed filter: no RLS, and the app DB user is a superuser | 4.2 I, 4.14 E | ADR-004 T1, T2 · [MOH-31](https://linear.app/mohamed-ahmed-ramadan/issue/MOH-31) (RLS), [MOH-23](https://linear.app/mohamed-ahmed-ramadan/issue/MOH-23) (DB roles) |
| 2 | Internal error text (including database errors) returned to clients | all API modules | ADR-012 A1 · [MOH-32](https://linear.app/mohamed-ahmed-ramadan/issue/MOH-32) |
| 3 | Tenant secrets under one static key per type, with no rotation or crypto-shredding | 4.9 I | ADR-014 E1 · [MOH-33](https://linear.app/mohamed-ahmed-ramadan/issue/MOH-33) |
| 4 | Platform accounts without MFA | 4.11 E | ADR-005 A6 |
| 5 | Account enumeration through login timing | 4.1 S | ADR-005 A3 |
| 6 | CSRF gaps (missing `Origin` allowed, login unchecked) | 4.1 E | ADR-005 A4 |
| 7 | Public and internal traffic share one process and router | 4.4 D/E | ADR-010 G1 |
| 8 | CSP not enforced (`report-only`); GitHub Actions pinned by tag, not SHA; no circuit breakers | 4.1 S, 4.6 D, 4.13 T, 4.14 S | this document |

Each risk should become a Linear issue with an owner and target date; this table links them once
created. Risks 1–3 are filed; 4–8 are not yet.
