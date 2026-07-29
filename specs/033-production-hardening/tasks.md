---

description: "Task list for 033-production-hardening"
---

# Tasks: Production Hardening

**Input**: Design documents from `/specs/033-production-hardening/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Included. Constitution Principle VII mandates test coverage for shipped
functionality, and SC-008 requires proving the gate fails on a deliberate break.

---

## ⚠️ READ THIS FIRST — rules for whoever implements these tasks

1. **The workspace must build green after every single task.** No task leaves
   `cargo build --workspace` red. If a task leaves it red, you have done something the
   task did not ask for — revert and re-read. This matters most in Phase 3, where the
   rate-limiter refactor is deliberately split into 8 small steps instead of 1 big one.
2. **Run the verification command printed in each task.** Do not mark a task `[X]` based on
   "it looks right". If the command fails, the task is not done.
3. **Never weaken an existing test to make it pass.** If an existing test goes red, either
   a numbered task tells you to update it (**T014, T015, and T023** are the only three that
   do), or you have broken something. Ask; do not edit the assertion.
4. **Do not invent files, endpoints, permissions, or database tables.** This feature adds
   exactly **one** table and **zero** new API endpoints, routes, permissions, or frontend
   pages. If you think you need one, you have misread the task.
5. **`[MANUAL]` tasks cannot be completed by a model.** They need a human, a provisioned
   host, or credentials that do not exist yet. Leave them unchecked and say so.
6. **`[BLOCKED: provisioning]` tasks require a staging VM that does not exist yet.** Do not
   attempt them. Do not fake them.
7. **Execute tasks in the order they appear in this file. The ID is a stable label, not a
   position.** T097–T108 were added after the first numbering pass and sit *inside* the
   phase they belong to, so you will see IDs out of numeric order. Follow the file order,
   not the numbers.
8. **Some tasks may already be applied in the working tree.** Tasks marked
   *(may already be applied — verify before editing)* had their content landed ahead of
   this list. Open the file first; if the change is present and correct, verify it and check
   the task off rather than applying it twice.

**Never pipe a build or test command to `tail`** — a failing build then reads as exit 0.
Redirect to a file and check `$?`.

### Test ordering — a deliberate, recorded exception

Constitution Principle VII is test-first. Three test groups here are sequenced *after* their
implementation, and this is recorded as a justified deviation in `plan.md`'s Complexity
Tracking rather than hidden:

- **T014, T015** update `tests/cors.rs` and `tests/health.rs`, which already exist and
  already pass. There is no failing-first state to reach.
- **T023** adapts four existing unit tests from sync to `#[tokio::test]`.
- **T016, T030, T031, T047, T052** are genuinely new suites. **Write and run each one
  before its implementation task and confirm it fails for the intended reason** — an
  assertion mismatch, not a compile error. If it fails to compile because the function does
  not exist yet, that is expected; write the test, watch it fail, then implement.

---

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1–US6, mapping to the user stories in spec.md
- **[MANUAL]**: Requires a human; a model must not check this off
- **[BLOCKED: provisioning]**: Requires the staging host to exist

Each task cites the requirement it discharges, e.g. `(FR-036)`. If you cannot see how your
change satisfies the cited requirement, re-read the requirement in `spec.md` before writing
code.

## Path Conventions

Repository root is `/Users/mohamedramadan/customer-service-chat-system`. All paths below are
relative to it. Backend Rust workspace root is `backend/` — `cargo` commands run from there.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Pin the toolchain, install the command-line tools later phases verify against,
and add configuration. Nothing here changes runtime behaviour.

- [X] T001 Create `backend/rust-toolchain.toml` pinning the toolchain (**FR-036**). Contents exactly: a `[toolchain]` table with `channel = "1.84.0"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"`. **Verify**: `cd backend && cargo --version` prints 1.84.0 and `cargo fmt --check` exits 0. If `cargo fmt --check` now fails on files you did not touch, STOP — report it, do not run `cargo fmt --all`. Formatting is currently clean at HEAD, so a new failure means the pinned version formats differently and that is a decision for a human.
- [X] T002 Install cargo-nextest — the test runner the gate will use (**FR-035**): `cargo install cargo-nextest --locked`. Do this **after T001**, so the pinned toolchain builds it. **Verify**: `cd backend && cargo nextest --version` succeeds.
- [X] T097 [P] Install the two other command-line tools later tasks verify against (**FR-036** — a verification command you cannot run is not a verification). On macOS: `brew install caddy k6`. On Linux, follow the vendors' documented installs. `caddy` is required by the **Verify** step of T098, T017, and T062; `k6` by T084–T086. **Verify**: `caddy version` and `k6 version` both succeed. If you cannot install them, say so explicitly — do not check off a task whose verification you skipped.
- [X] T098 Create `infra/Caddyfile` as an empty scaffold with one commented section header per block that later tasks fill in: `# --- widget document headers (T017) ---`, `# --- TLS + reverse proxy (T062) ---`, `# --- /metrics network ACL (T062) ---`. **This file is created exactly once, here.** T017 and T062 each fill in their own section; neither creates the file. **Verify**: `caddy validate --config infra/Caddyfile` exits 0. If it rejects a config with no site block, add a single `:80 { }` placeholder that T062 replaces and mark it with a `# placeholder — replaced by T062` comment. **Do not invent real routing here** — preventing three tasks from each inventing their own Caddyfile is this task's entire purpose.
- [X] T003 Record the baseline executed-test count (**FR-035**). **Prerequisites, all required**: T002 done; `docker compose -f infra/docker-compose.yml up -d` running; `cd backend && sqlx migrate run` already applied. Then run: `cd backend && REQUIRE_DB_TESTS=1 cargo nextest run --workspace > /tmp/baseline.txt 2>&1; echo "exit=$?"`. The exit code must be 0 — a baseline taken from a failing run is worthless. Read the "Summary [ … ] N tests run" number out of `/tmp/baseline.txt` and write **just that integer** into `specs/033-production-hardening/baseline-test-count.txt`. This number becomes the CI floor enforced in **T036**, and is refreshed at the end of the feature by **T093**.
- [X] T004 Add the new configuration fields to `backend/crates/shared/config/src/lib.rs` (**FR-006, FR-013** — thresholds and directive values adjustable per environment without a code change). Add each field listed in the "Configuration" table of `specs/033-production-hardening/data-model.md` to the `AppConfig` struct and read it in `from_env()`. **Every field must be optional with a working default** — no new `required()` calls. Follow the existing `env::var("...").ok()` / `.unwrap_or_else(...)` patterns already in that file. **Verify**: `cd backend && cargo build --workspace` exits 0 and `cargo test -p config` passes.

**Checkpoint**: Toolchain pinned, tools installed, config extended, nothing behaves
differently yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add the new limiter abstraction *alongside* the existing one, and the single
migration. **Nothing is removed in this phase** — the old synchronous path keeps working so
the workspace stays green.

**⚠️ CRITICAL**: US1 cannot start until this phase is complete.

- [X] T005 Create migration `backend/migrations/0061_tenant_rate_limit_overrides.sql` (**FR-006**) creating the `tenant_rate_limit_overrides` table exactly as specified in the "Persisted entities" section of `specs/033-production-hardening/data-model.md` — all 9 columns, the FK to `tenants(id)`, both CHECK constraints, and the partial unique index `UNIQUE (tenant_id, scope) WHERE deleted_at IS NULL`. Note the next free number is **0061** (0058 is absent from the sequence; do not reuse it). **Verify**: `cd backend && sqlx migrate run` succeeds, then `REQUIRE_DB_TESTS=1 cargo test -p db --test schema -- --exact run_migrations_succeeds` passes.
- [X] T006 [P] Define the `RateLimiter` async trait, `Scope` enum, `Budget` struct, and `Decision` enum in `backend/crates/shared/kernel/src/rate_limit.rs` (**FR-003**), exactly per the "Runtime structures" section of `data-model.md`. **ADD ONLY — do not modify or delete the existing `InMemoryRateLimitStore` or its synchronous `check()` method.** Both must coexist. Export the new types from `backend/crates/shared/kernel/src/lib.rs` alongside the existing `pub use rate_limit::InMemoryRateLimitStore;`. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T007 Implement `RateLimiter` for the existing `InMemoryRateLimitStore` in `backend/crates/shared/kernel/src/rate_limit.rs` (an `async fn` that wraps the existing sync logic). This is the degraded-mode and test implementation required by **FR-007**. **Verify**: `cd backend && cargo test -p kernel` passes.
- [X] T008 Create `backend/crates/shared/cache/src/limiter.rs` implementing `RedisRateLimiter` (**FR-003**) per `research.md` §1: key format `rl:{scope}:{identifier}:{window_epoch}`, `INCR` + `EXPIRE` issued in one pipelined round-trip, previous-window weighting for the boundary correction. Declare `pub mod limiter;` in `backend/crates/shared/cache/src/lib.rs`. Reuse the existing private `Cache::manager()` accessor — **do not import the `redis` crate into any module crate**; the cache crate's docs state that is its whole purpose. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T009 Add unit tests for `RedisRateLimiter` in `backend/crates/shared/cache/src/limiter.rs` (`#[cfg(test)] mod tests`) covering **FR-003**: allows up to the limit, refuses past it, resets after the window, and different keys are independent. Mirror the four test names already in `backend/crates/server/src/rate_limit.rs`. Gate them on Redis being reachable, following the `REQUIRE_DB_TESTS` convention used elsewhere. **Not parallel with T008 — same file.** **Verify**: `cd backend && cargo test -p cache` passes.
- [X] T010 Implement budget resolution in `backend/crates/shared/cache/src/limiter.rs` (**FR-006**): override row → environment config → compiled default, per `contracts/rate-limit-responses.md`. Include the 60-second in-process cache keyed by tenant ID described in `data-model.md`, and reject writes above `APP_RATE_LIMIT_MAX_OVERRIDE`. **Verify**: `cd backend && cargo build --workspace && cargo test -p cache` exits 0.
- [X] T099 Implement **identifier selection** in `backend/crates/shared/cache/src/limiter.rs` (**FR-004**) — the rule that decides *what* the budget is keyed on, which nothing else in this list covers. Per the "identifier selection" section of `contracts/rate-limit-responses.md`: when the caller is authenticated, key on the authenticated identity (user ID for user-scoped budgets, tenant ID for tenant-scoped ones) and **ignore the source address entirely**; only when the caller is anonymous, key on the source address. The purpose is stated in FR-004 — many users behind one corporate NAT or mobile carrier gateway share a source address, and keying an authenticated budget on it locks all of them out together. Expose this as a single function the call sites in T019–T024 use, so the rule exists in one place. Add a unit test proving two different authenticated identities arriving from the *same* source address get independent budgets. **Verify**: `cd backend && cargo test -p cache` passes.

**Checkpoint**: New limiter exists and is tested. Nothing uses it yet. Old path untouched.

---

## Phase 3 (US1): Security — spec priority P1, sequenced 1st 🎯 MVP

**Goal**: Sign-in and all public endpoints are throttled under one shared budget; every
response carries browser security headers; the widget still embeds.

**Independent Test**: Run the US1 section of `specs/033-production-hardening/quickstart.md`
end to end. Delivers real risk reduction with no other story built.

### 3a — Security headers

- [X] T011 [US1] Create `backend/crates/server/src/security_headers.rs` (**FR-009**) with a function returning the Scope A header layers from `contracts/security-headers.md` (`X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Strict-Transport-Security` built from `APP_HSTS_MAX_AGE`). Use `tower_http::set_header::SetResponseHeaderLayer`. **Omit the HSTS header entirely when max-age is 0** — do not emit `max-age=0`. Declare `mod security_headers;` in `backend/crates/server/src/lib.rs`. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T012 [US1] Add the Scope B function to `backend/crates/server/src/security_headers.rs` (**FR-010, FR-011, FR-013**): `X-Frame-Options: DENY` plus the CSP from `contracts/security-headers.md`, emitted as `Content-Security-Policy-Report-Only` while `APP_CSP_REPORT_ONLY` is true (the default) and as `Content-Security-Policy` when false. Directive values come from config (FR-013), not from constants. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T013 [US1] Wire the layers in `backend/crates/server/src/router.rs` (**FR-009 – FR-012**). Apply Scope A at the router root (around line 1044, where `rate_store` is currently constructed). Apply Scope B to the authenticated/platform/tenant routers **only** — do **not** apply it to `widget_router` or `hooks_router`. Read `contracts/security-headers.md` Scope C before doing this: the backend serves no HTML, so JSON responses are never framed, but Scope B must still not be applied to the widget scope. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T014 [US1] Update the existing test `backend/crates/server/tests/cors.rs` (**FR-009** regression) — it asserts on response headers and will go red now that new headers are present. Update the assertions to expect the new headers. **Do not delete or weaken any existing CORS assertion.** **Verify**: `cd backend && cargo test -p server --test cors` passes.
- [X] T015 [US1] Update the existing test `backend/crates/server/tests/health.rs` for the same reason (**FR-009** regression). Same rule: extend, never weaken. **Verify**: `cd backend && cargo test -p server --test health` passes.
- [X] T016 [P] [US1] Create `backend/crates/server/tests/security_headers.rs` (**FR-009 – FR-012, SC-003**) asserting: (a) every Scope A header present on `/api/v1/health`; (b) `X-Frame-Options: DENY` present on an authenticated route; (c) `X-Frame-Options` **absent** on a `/widget/v1/*` route; (d) HSTS absent when `APP_HSTS_MAX_AGE=0`. This is the automated check SC-003 requires. Per the test-ordering note above, write this before T011–T013 if you can. **Verify**: `cd backend && cargo test -p server --test security_headers` passes.
- [X] T017 [US1] Fill in the `# --- widget document headers ---` section of `infra/Caddyfile` (**FR-012**), the file created in **T098**. The widget app's `index.html` must be served with **no** `X-Frame-Options` and `Content-Security-Policy: frame-ancestors *`. Read the "Why `frame-ancestors *`" paragraph in `contracts/security-headers.md` before writing this — a restrictive value here breaks the embeddable widget for every tenant. **Verify**: `caddy validate --config infra/Caddyfile` exits 0.
- [X] T018 [MANUAL] [US1] Load the built widget in an `<iframe>` from an allowlisted origin and confirm it renders and can open a session (**FR-012, SC-003**). This is the regression check for T013/T017 and requires a browser. **Evidence to close**: a screenshot or a written confirmation that the widget rendered inside the frame.

### 3b — Rate limiter migration (one call site per task; workspace stays green throughout)

> Each of T019–T024 changes **one** file, and each uses the identifier-selection function
> from T099. After each, `cargo build --workspace` must exit 0. The old sync method still
> exists until T026, so unmigrated call sites keep compiling.

- [X] T019 [US1] Migrate `backend/crates/modules/widgets/src/public_routes.rs` (the `Extension<Arc<InMemoryRateLimitStore>>` at line ~480) to take `Extension<Arc<dyn RateLimiter>>` and `.await` the new trait method (**FR-002, FR-003**). **Verify**: `cd backend && cargo build --workspace && cargo test -p widgets` exits 0.
- [X] T020 [US1] Same migration for `backend/crates/modules/feedback/src/public_routes.rs` (line ~40) (**FR-002, FR-003**). **Verify**: `cd backend && cargo build --workspace && cargo test -p feedback` exits 0.
- [X] T021 [US1] Same migration for `backend/crates/modules/whatsapp/src/webhook.rs` (line ~119) (**FR-002, FR-003**). **Verify**: `cd backend && cargo build --workspace && cargo test -p whatsapp` exits 0.
- [X] T022 [US1] Same migration for `backend/crates/modules/integrations/src/webhook.rs` (line ~157) (**FR-002, FR-003**). **Verify**: `cd backend && cargo build --workspace && cargo test -p integrations` exits 0.
- [X] T023 [US1] Migrate `backend/crates/server/src/rate_limit.rs` (**FR-002, FR-003**): change `per_ip_creation_limit`, `check_session_message_limit`, and `check_global_tenant_limit` to use the async trait. Keep the four existing unit tests passing (adapt them to `#[tokio::test]`) — this is one of the three sanctioned existing-test updates. **Verify**: `cd backend && cargo test -p server --lib rate_limit` passes.
- [X] T024 [US1] In `backend/crates/server/src/router.rs` at line ~1044, replace `Arc::new(kernel::InMemoryRateLimitStore::default())` with a `RedisRateLimiter` built from the configured `redis_url`, typed as `Arc<dyn RateLimiter>` (**FR-003**). **CRITICAL — construct the Arc exactly once and clone it onto both routers**, as the existing code does: `widget_router` gets `rate_store.clone()`, `hooks_router` gets `rate_store`. The comment at lines 1041–1043 states they must share a budget. Constructing two instances silently splits the budget and breaks FR-003. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T025 [US1] Add the degraded-mode behaviour from `research.md` §2 to `backend/crates/shared/cache/src/limiter.rs` (**FR-007**): on a Redis error, `login` scope returns a refusal that the caller maps to `503`; all other scopes fall back to a process-local `InMemoryRateLimitStore`. Log at `WARN` and increment a `rate_limit_degraded_total` counter (a plain atomic for now — **T049** wires it to Prometheus). **Verify**: `cd backend && cargo test -p cache` passes.
- [X] T026 [US1] Delete the now-unused synchronous `check()` method and any dead helpers from `backend/crates/shared/kernel/src/rate_limit.rs`, keeping `InMemoryRateLimitStore` itself (still used by T007 and T025). **Verify**: `cd backend && cargo build --workspace && cargo clippy --workspace -- -D warnings` exits 0 with no dead-code warnings.

### 3c — Sign-in throttle, refusal contract, and refusal recording

- [X] T027 [US1] Add the login throttle in `backend/crates/server/src/router.rs`'s `login` handler (around line 198) or as a layer on that route (**FR-001**). Key on `(email, client_ip)` with scope `login`. **CRITICAL — increment the counter BEFORE the account lookup result is known.** Throttling only requests that reach password verification leaks account existence; see the "account enumeration" section of `contracts/rate-limit-responses.md`. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T028 [US1] Emit `Retry-After` on every 429 in `backend/crates/shared/kernel/src/lib.rs`'s `ApiError::rate_limited` (add a seconds parameter) and update all callers (**FR-005**). The current implementation omits it. **Do not add `X-RateLimit-*` headers** — they disclose the budget, which FR-005 forbids. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T029 [US1] Sanitise `X-Forwarded-For` handling in `backend/crates/server/src/rate_limit.rs` (**FR-004**). It currently trusts the header unconditionally, which is spoofable — and a spoofable identifier makes FR-004's per-source scoping meaningless. Only trust it when `APP_TRUSTED_PROXY=true` (add this config field); otherwise use the socket peer address. This depends on Caddy overwriting the header at the edge, which **T062** configures. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T100 [US1] Record refusals as security-relevant events (**FR-008**) in `backend/crates/shared/cache/src/limiter.rs` and the call sites. **Read FR-008 in `spec.md` before writing this — it constrains where the record goes, and the obvious implementation is wrong.** Two paths:
  - **Unauthenticated refusals** (widget, webhooks, login): emit a structured `WARN` log event with fields `event="rate_limit_refused"`, `scope`, `identifier_hash` (a SHA-256 hex digest of the identifier, **never the raw address or email**), and `budget`. Nothing else. **Do NOT write these to `audit_logs`** — that table has a `forbid_mutation` trigger blocking UPDATE and DELETE, so an anonymous flood would write rows nobody can ever prune, turning the audit trail into an amplification target.
  - **Authenticated, tenant-attributable refusals**: additionally write one `audit_logs` row via the existing audit writer with action `security.rate_limit.refused`, deduplicated to **at most one row per identifier per window** (reuse the 60-second in-process cache from T010 as the dedupe gate). Do not add a new audit category or permission — reuse the existing `security.*` action prefix so the category derivation in the `audit` crate keeps working unchanged.

  **Verify**: `cd backend && cargo test -p cache && cargo test -p server --test openapi_coverage` — the second must pass **unchanged**, proving no new route or permission crept in.
- [X] T030 [P] [US1] Create `backend/crates/server/tests/rate_limit_login.rs` (**FR-001, FR-005, SC-001**) asserting: 10 failures then 429; **an identical status sequence for an existing and a non-existent account** (the enumeration check); and `Retry-After` present on the 429. **Verify**: `cd backend && REQUIRE_DB_TESTS=1 cargo test -p server --test rate_limit_login` passes.
- [X] T031 [P] [US1] Create `backend/crates/server/tests/rate_limit_shared.rs` (**FR-003, SC-002**) asserting that two `RedisRateLimiter` instances sharing one Redis reach the limit at the combined count, not per-instance. This is SC-002 at unit level; the two-process version is T032. **Verify**: `cd backend && cargo test -p server --test rate_limit_shared` passes.
- [ ] T032 [MANUAL] [US1] Run the two-instance check from the US1 section of `quickstart.md` (two servers on ports 8080/8081, alternating requests) (**FR-003, SC-002**). **Expected**: ~10 total allowed across both, not 20. **Evidence to close**: pasted output showing the combined count.

**Checkpoint**: US1 complete. Sign-in throttled, budgets shared, refusals recorded, headers
present, widget still embeds. **This is the MVP — stop here and validate before continuing.**

---

## Phase 4 (US5): CI Gate — spec priority P5, sequenced 2nd

> **Sequencing note**: the spec ranks this P5, but it is unblocked and it protects every
> later phase. US2 and US4 are blocked on a host that does not exist. Doing US5 second is a
> scheduling decision, not a priority change — see "Implementation Strategy" below.

**Goal**: One authoritative, deterministic, non-vacuous backend gate that also publishes the
artifact the deployment procedure promotes.

**Independent Test**: Push a deliberately failing test; the gate fails and names it.

- [X] T033 [US5] Delete `.github/workflows/ci.yml` entirely (**FR-037**). It runs no migrations and omits `REQUIRE_DB_TESTS`, so its `cargo test --workspace` silently skips DB-backed tests while reporting green — see `research.md` §9. `backend.yml` already covers everything it does, with more rigour. **Verify**: `ls .github/workflows/` shows only `backend.yml`, `frontend.yml`, and `gate-verification.yml`.
- [X] T034 [US5] *(may already be applied — verify before editing)* In `.github/workflows/backend.yml`, replace `dtolnay/rust-toolchain@stable` with the pinned toolchain (**FR-036**) — it will now be read from `backend/rust-toolchain.toml` created in T001, so remove the explicit `stable` reference and let the file govern. **Verify**: the workflow file no longer contains the string `@stable`.
- [X] T035 [US5] *(may already be applied — verify before editing)* In `.github/workflows/backend.yml`, install cargo-nextest via `taiki-e/install-action@v2` with `tool: cargo-nextest`, and change `cargo test --workspace` to `cargo nextest run --workspace` (**FR-034**). **Verify**: YAML parses — `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/backend.yml'))"`.
- [X] T036 [US5] *(may already be applied — verify before editing)* Add an executed-test-count floor step to `.github/workflows/backend.yml` (**FR-035**): parse the count from nextest output and fail if it is below the integer in `specs/033-production-hardening/baseline-test-count.txt` (written by T003). Publish the count as a `verify` job output (`test_count`) so T103's `release` job can consume it via `needs.verify.outputs.test_count`. This is what makes FR-035 enforceable rather than informational. **Verify**: YAML parses, the step reads the file path above, and `needs.verify.outputs.test_count` appears in the `release` job.
- [X] T037 [US5] Remove `continue-on-error: true` from the secret-scan step in `.github/workflows/frontend.yml` and ensure `cargo audit` in `backend.yml` is not advisory (**FR-038**). **Verify**: neither file contains `continue-on-error` on those steps.
- [X] T038 [US5] Unify the pnpm version (**FR-036** — an unpinned or divergent tool version can change the verdict). In `.github/workflows/frontend.yml` the `verify` job pins 10.12.4 and `real-backend-e2e` pins 10.34.4; set **both** to 10.34.4. `infra/Dockerfile.frontend` also pins 10.12.4 — set it to 10.34.4 too, or T101's image build uses a different resolver than the gate did. **Verify**: `grep -c "10.34.4" .github/workflows/frontend.yml` returns 2, `grep -c "10.12.4" .github/workflows/frontend.yml` returns 0, and `grep -c "10.12.4" infra/Dockerfile.frontend` returns 0.
- [X] T039 [US5] *(may already be applied — verify before editing)* Add a `concurrency` group to both workflows so superseded pushes cancel (supports **SC-009**'s determinism by removing overlapping runs on the same ref): `concurrency: { group: "${{ github.workflow }}-${{ github.ref }}", cancel-in-progress: true }`. **Verify**: YAML parses.
- [X] T101 [US5] Parameterise `infra/Dockerfile.frontend` so it can build **both** Angular apps (prerequisite for **FR-022**: the deployment procedure promotes a build of every service, and the widget currently has no image at all). It hardcodes `RUN pnpm exec ng build dashboard` and copies `dist/dashboard/browser`. Add `ARG APP=dashboard` in the builder stage, use `RUN pnpm exec ng build ${APP}`, and `COPY --from=builder /app/dist/${APP}/browser /usr/share/nginx/html` in the runtime stage. **`ARG` scope ends at each `FROM`** — you must declare `ARG APP` a *second* time after `FROM nginx:alpine`, or `${APP}` expands to empty there and you silently ship an image with no files in it. Do not create a second Dockerfile — one parameterised file, two build invocations. **Verify**: `docker build -f infra/Dockerfile.frontend --build-arg APP=dashboard -t t101-dash .` and the same with `--build-arg APP=widget` both succeed.
- [X] T102 [US5] Add a `release` job to `.github/workflows/backend.yml` that **builds and pushes all three images tagged with the immutable Git SHA** (**FR-022, FR-026** — and the precondition for FR-040 and for T063's SHA-tagged compose overlay). The existing final step only runs `docker build -f infra/Dockerfile.backend .` with no push and no frontend image, so there is nothing for `ops/deploy.sh` to pull. Requirements, all of them:
  - `needs: verify` — never publish an image from a red gate.
  - `if: github.event_name == 'push' && github.ref == 'refs/heads/master'` — **do not push from pull requests**, which on forks would expose registry credentials.
  - `permissions: { contents: read, packages: write }` at the job level.
  - Log in with `docker/login-action@v3` against `ghcr.io` using `${{ github.actor }}` and `${{ secrets.GITHUB_TOKEN }}`.
  - Push three tags, all `${{ github.sha }}` — **never `latest`**, which makes rollback and FR-040 unverifiable: `ghcr.io/<owner>/<repo>-backend:${{ github.sha }}` from `infra/Dockerfile.backend`; `…-dashboard:${{ github.sha }}` from `infra/Dockerfile.frontend --build-arg APP=dashboard`; `…-widget:${{ github.sha }}` with `APP=widget`.

  Remove the old bare `docker build` step it replaces. **Verify**: YAML parses, `grep -c latest .github/workflows/backend.yml` returns 0, and the file contains three distinct `:${{ github.sha }}` tags.
- [X] T103 [US5] In the same `release` job, publish `release-manifest.json` as a build artifact (**FR-040**). Contents: `commit_sha`, `built_at` (ISO-8601 UTC), `workflow_run_id`, `tests_executed` (the count T036 parsed), and an `images` object mapping `backend`/`dashboard`/`widget` to their full pushed reference including the SHA tag. Upload with `actions/upload-artifact@v4` named `release-manifest`. This is the artifact `ops/deploy.sh` (T064) reads to know exactly what it is promoting — the manifest and the deploy script must agree on these field names, so if you change one, change both. **Verify**: YAML parses and the job contains an `actions/upload-artifact` step.
- [X] T040 [US5] Create `.github/workflows/gate-verification.yml` (**FR-039**) — a `workflow_dispatch` + weekly `schedule` job that applies a patch breaking one assertion, runs the gate, asserts it **fails**, and reverts. This is the regression test for the gate itself. **Verify**: YAML parses.
- [ ] T041 [MANUAL] [US5] Push a branch with one deliberately broken test assertion and confirm the gate fails and names it (**FR-039, SC-008**). **Evidence to close**: a link to the failed CI run.
- [ ] T042 [MANUAL] [US5] Re-run the same SHA three times and confirm an identical verdict each time (**FR-036, SC-009**). **Evidence to close**: three run links with the same result.

**Checkpoint**: The gate catches real breakage, is deterministic, and publishes a
SHA-pinned, manifest-described release that the deploy script can consume.

---

## Phase 5 (US3): Observability — spec priority P3, sequenced 3rd

**Goal**: Real metrics, exported traces, aggregated logs, grouped errors, Grafana
dashboards, and alerts that reach a human.

**Independent Test**: Inject a failure and a slow path; find both via dashboard → trace →
log → grouped error using one correlation ID.

### 5a — Metrics

- [X] T043 [US3] Add dependencies to `backend/Cargo.toml` workspace deps (**FR-015 – FR-020**): `metrics`, `metrics-exporter-prometheus`, `opentelemetry`, `opentelemetry-otlp`, `tracing-opentelemetry`, `sentry`, `sentry-tracing`. Add the ones needed to `backend/crates/shared/observability/Cargo.toml`. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T044 [US3] Create `backend/crates/shared/observability/src/metrics.rs` (**FR-015**) defining the recorder and the seven metric series listed in `data-model.md`. **Label sets are a hard constraint**: `route` must be the route *template* (`/tenant/conversations/:id`), never a concrete path, and **no series may carry `tenant_id`, user ID, or session ID**. Declare `pub mod metrics;` in `lib.rs`. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T045 [US3] Replace the stub body of `metrics()` in `backend/crates/shared/observability/src/lib.rs` (lines ~59–72, currently returning `"# no metrics yet\n"`) with the real Prometheus exposition from the recorder (**FR-015**). **Also update the `#[utoipa::path]` annotation above it** — the response description no longer matches. **Verify**: `cd backend && cargo test -p server --test openapi_valid && cargo test -p server --test openapi_contract` pass.
- [X] T046 [US3] Add HTTP metrics middleware in `backend/crates/shared/observability/src/metrics.rs` recording `http_requests_total` and `http_request_duration_seconds` (**FR-015**), and wire it in `backend/crates/server/src/router.rs` next to the existing `trace_middleware`. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T047 [P] [US3] Create `backend/crates/server/tests/metrics_labels.rs` (**FR-019**, Constitution Principle II) asserting the `/metrics` body contains **no UUID-shaped label value** (regex `[0-9a-f]{8}-[0-9a-f]{4}`). This enforces the label constraint in `contracts/metrics-exposition.md`. **Verify**: `cd backend && cargo test -p server --test metrics_labels` passes.
- [X] T048 [US3] Populate the `dependency_up` gauge (**FR-015**) from the health-check results that **spec 032 already computes** — call the existing readiness path, do not write new probes. Re-probing would double dependency load on the single host; see the 032 boundary in `plan.md`. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T049 [US3] Wire `rate_limit_refusals_total` and `rate_limit_degraded_total` from the atomics added in T025 and T100 into the metrics recorder (**FR-008, FR-015**). The refusal counter is half of FR-008's "recorded where an operator will see it". **Verify**: `cd backend && cargo build --workspace` exits 0.

### 5b — Traces and errors

- [X] T050 [US3] Create `backend/crates/shared/observability/src/otlp.rs` (**FR-014, FR-016, FR-020**): build a `tracing-opentelemetry` layer exporting OTLP/gRPC to `APP_OTLP_ENDPOINT`, with parent-based ratio sampling from `APP_TRACE_SAMPLE_RATIO`. **When `APP_OTLP_ENDPOINT` is unset, add no layer and log nothing** — local dev and CI must be unaffected, which FR-016 requires explicitly. Attach the existing request ID as a span attribute; this is the join key FR-014 depends on. Declare `pub mod otlp;` in `lib.rs` and call it from `init_observability`. **Verify**: `cd backend && cargo build --workspace && cargo test -p observability` exits 0.
- [X] T051 [US3] Create `backend/crates/shared/observability/src/errors.rs` (**FR-017, FR-019**): initialise Sentry from `APP_SENTRY_DSN` with `APP_RELEASE_SHA` as the release. **Unset DSN disables capture entirely.** Implement the `before_send` scrubber removing everything listed under "Scrubbing at capture" in `contracts/metrics-exposition.md` — request bodies, `Authorization`, `Cookie`, `X-Tenant-ID`, any header matching `/secret|token|key|password/i`, message content, and customer PII. Also attach the request ID so FR-014's correlation reaches the third surface. **Verify**: `cd backend && cargo build --workspace` exits 0.
- [X] T052 [US3] Add unit tests to `backend/crates/shared/observability/src/errors.rs` (**FR-019, SC-007**) asserting the scrubber removes a known secret and a known email from a synthetic event. FR-019's masking is the half that fails silently, so it needs a direct test. **Not parallel with T051 — same file.** **Verify**: `cd backend && cargo test -p observability` passes.

### 5c — Collection stack (config files first, then the one shared compose file)

- [X] T053 [P] [US3] Create `infra/otel-collector-config.yaml` (**FR-020**) with an OTLP receiver and a **tail-sampling processor retaining 100% of traces containing an error span** — already verified complete — head sampling alone cannot satisfy FR-020's "a request that errored MUST always be retained"; see `research.md` §5. **Verify**: `python3 -c "import yaml;yaml.safe_load(open('infra/otel-collector-config.yaml'))"`.
- [X] T054 [P] [US3] Update `infra/prometheus.yml` (**FR-015**) with `rule_files:` pointing at `prometheus-alerts.yml` and an `alerting:` block pointing at the `alertmanager` service. **Verify**: `python3 -c "import yaml;yaml.safe_load(open('infra/prometheus.yml'))"`.
- [X] T104 [P] [US3] Create `infra/promtail-config.yaml` (**FR-014**) — the log-shipping half of "logs and traces retrievable together". Configure a `docker_sd_configs` (or `static_configs` over `/var/lib/docker/containers`) scrape of the compose stack's container logs, pushing to `http://loki:3100/loki/api/v1/push`. Add a pipeline stage that parses the JSON log line so `request_id` becomes a queryable **structured/detected field**. **The label set is exactly `container` and `level` — nothing else.** Do **not** promote `request_id`, `tenant_id`, user ID, or session ID to a label: in Loki every distinct label value creates a separate stream, so a per-request label creates one stream per request. This is the same unbounded-cardinality rule T044 states for metrics and T047 test-enforces; it applies identically here. Retrieval by correlation identifier is a line filter over a small stream set — `{container="backend"} | json | request_id="…"` — which is what Loki is built for, not a full-text scan. **Verify**: `python3 -c "import yaml;yaml.safe_load(open('infra/promtail-config.yaml'))"`.
- [X] T105 [P] [US3] Create `infra/grafana/provisioning/datasources/datasources.yaml` (**FR-014, FR-018**) declaring two datasources as code: Prometheus at `http://prometheus:9090` and Loki at `http://loki:3100`. On the Loki datasource, configure a **derived field** matching `request_id` in the **parsed log line** (not a label — see T104) and linking to the trace backend, so an operator clicks from a log line to its trace. That click is what makes SC-006's 5-minute target reachable. **Verify**: `python3 -c "import yaml;yaml.safe_load(open('infra/grafana/provisioning/datasources/datasources.yaml'))"`.
- [X] T107 [US3] Create `infra/prometheus-alerts.yml` (**FR-021**) with three rule groups: (a) **error rate** — `http_requests_total{status=~"5.."}` ratio above a documented threshold for 5m; (b) **latency** — p95 of `http_request_duration_seconds` above the documented target for 10m; (c) **dependency health** — `dependency_up == 0` for 2m, labelled by dependency. Use SC-014's numbers as the thresholds — error rate ≥ 1%, p95 ≥ 500 ms — and state each one in a comment next to its rule. `loadtest/README.md` (T087, later phase) documents the same two numbers; if you change one, change both. **T079 extends this same file** with the backup-staleness rule — it does not create it. **Verify**: `python3 -c "import yaml;yaml.safe_load(open('infra/prometheus-alerts.yml'))"` and `promtool check rules infra/prometheus-alerts.yml` if `promtool` is available.
- [X] T108 [US3] Create `infra/alertmanager.yml` (**FR-021** — a rule that fires into nothing does not "notify an on-call recipient"). One default route with a grouping interval, and a **single receiver whose destination is read from environment variables** (`APP_ALERT_WEBHOOK_URL` or SMTP settings) so no real destination is committed — FR-025 forbids secrets in version control, and the spec's "Alert delivery" assumption records that the concrete destination is supplied at deploy time. Add an inhibition rule so a `dependency_up == 0` alert suppresses the error-rate alert it causes. **Verify**: `python3 -c "import yaml;yaml.safe_load(open('infra/alertmanager.yml'))"`.
- [X] T055 [P] [US3] Create `infra/grafana/dashboards/platform-overview.json` (**FR-018**) with panels for request rate, error rate, latency distribution (p50/p95/p99), and outbox backlog, over a selectable range. **This is FR-018 — do not create an Angular page.** Spec 032 owns the in-product health page; this feature adds no route, nav entry, or permission. **Verify**: `python3 -c "import json;json.load(open('infra/grafana/dashboards/platform-overview.json'))"`.
- [X] T056 [US3] *(may already be applied — verify before editing)* Add `otel-collector`, `prometheus`, and `grafana` services to `infra/docker-compose.yml` under an `observability` profile — already verified complete (**FR-015, FR-018**) so they do not start by default for regular development. **Verify**: `docker compose -f infra/docker-compose.yml config` exits 0 and `docker compose -f infra/docker-compose.yml config --services` does **not** list them without `--profile observability`.
- [X] T106 [US3] Add `loki`, `promtail`, and `alertmanager` to the **existing** `observability` profile in `infra/docker-compose.yml` (**FR-014, FR-021**). **This edits the same file as T056 — run it after T056, not in parallel.** Constraints: use `grafana/loki` in **single-binary mode with filesystem storage** and a named volume (`loki-data`) — do **not** configure an object-store backend; the target is one cost-constrained VM, per the spec's Assumptions. Pin explicit image tags the way the services already in that profile are pinned (`grafana/loki:3.4.2`, `grafana/promtail:3.4.2`, `prom/alertmanager:v0.28.0`) — never `latest`. Mount `infra/promtail-config.yaml`, `infra/alertmanager.yml`, and `infra/grafana/provisioning/` read-only. Promtail needs `/var/run/docker.sock` and `/var/lib/docker/containers` mounted read-only to read container logs. **Verify**: `docker compose -f infra/docker-compose.yml --profile observability config` exits 0 and lists all six observability services.
- [X] T109 [US3] Add the log-and-trace correlation walkthrough to `docs/runbook.md`'s "investigate a reported error" entry (**FR-014, SC-006**): paste a request ID into Grafana → Loki query `{container="backend"} | json | request_id="…"` (a **line filter over a bounded stream selector**, never `{request_id="…"}` — see T104) → click the derived field to the trace → find the same ID in the grouped error record. This is the written procedure SC-006 times an operator against; if T058 has not run yet, create the file with just this section and T058 will extend it. **Verify**: prose only — but walk it yourself against the local stack with `--profile observability` up and correct anything that does not match.

**Checkpoint**: Metrics real, traces exported, logs aggregated and joinable by request ID,
errors grouped and scrubbed, alerts routed to a configurable destination.

---

## Phase 6 (US6): Documentation — spec priority P6, sequenced 4th

> Sequenced before US2/US4 because it is unblocked and because `docs/deployment.md` and
> `docs/backup-restore.md` are the deliverables those blocked phases execute against.

- [X] T057 [P] [US6] Create `docs/configuration.md` listing **every** environment variable the platform reads (**FR-046, SC-013**). Source it from `backend/crates/shared/config/src/lib.rs` directly — **not** from `README.md`, which is known to be partial. Include the new alert and observability variables from T004, T105, and T108. For each: purpose, required-per-environment, default, and how to generate it if secret. **Verify**: run the `comm` check from the US6 section of `quickstart.md`; it must print nothing.
- [X] T058 [P] [US6] Create (or extend, if T109 created it) `docs/runbook.md` covering the five operational tasks in **FR-047**: promote a release, roll back, restore a backup, rotate a credential, investigate a reported error. Each entry: steps, expected outcome, and what to do when it fails. Do not overwrite T109's correlation walkthrough — it *is* the "investigate a reported error" entry.
- [X] T059 [P] [US6] Rewrite the Quickstart section of `README.md` to take a developer from a clean machine to a running backend, dashboard, and widget with seed data, with zero undocumented steps (**FR-045**). Include the `APP_HSTS_MAX_AGE=0` local-dev note from `contracts/security-headers.md` — omitting it pins the developer's browser to HTTPS on localhost. Mention that observability services start only with `--profile observability`.
- [X] T060 [US6] Update each module's doc header (`//! # Purpose / Responsibilities / Public Interfaces / Dependencies / Extension Points`) for the crates this feature changed: `kernel`, `cache`, `observability`, `config`, `server` (**FR-049**, Constitution "Documentation & Future Readiness"). **Verify**: `cd backend && cargo doc --workspace --no-deps` exits 0.
- [ ] T061 [MANUAL] [US6] Have a person who has never set up this project follow only `README.md` on a clean machine (**FR-048, SC-012**). Record every point where they ask or guess; fix each. **Self-review does not discharge this** — the author cannot detect their own assumed knowledge. **Evidence to close**: notes from the session and the resulting doc fixes. **Target**: running stack in under 60 minutes, zero questions.

**Checkpoint**: A newcomer can get running; operators have a runbook.

---

## Phase 7 (US2): Staging Deployment — spec priority P2, sequenced 5th

> **⚠️ This entire phase is blocked on a staging host that does not exist.** T062–T068 write
> the scripts and config (doable now). T069–T074 require the provisioned VM, a container
> registry, DNS, and TLS. Do not fake them.

- [X] T062 [US2] Fill in the `# --- TLS + reverse proxy ---` and `# --- /metrics network ACL ---` sections of `infra/Caddyfile`, the file created in **T098** (**FR-026**, Principle III remediation). TLS via automatic certificates; reverse proxy to the backend; **`/metrics` restricted to loopback and the compose network** (`research.md` §3); and `X-Forwarded-For` **overwritten** at the edge — T029's trust model depends on this, and a Caddy config that appends rather than overwrites leaves the header spoofable. Do not touch T017's widget section. **Verify**: `caddy validate --config infra/Caddyfile` exits 0.
- [X] T063 [P] [US2] Create `infra/docker-compose.staging.yml` as an overlay on `infra/docker-compose.yml` (**FR-026, FR-040**). **All images referenced by immutable Git-SHA tag — never `latest`.** Use the three image references T102 pushes, parameterised by a `RELEASE_SHA` environment variable. Rollback and FR-040 are unverifiable with mutable tags. **Verify**: `RELEASE_SHA=abc1234 docker compose -f infra/docker-compose.yml -f infra/docker-compose.staging.yml config` exits 0 and the output contains no `:latest`.
- [X] T064 [US2] Create `ops/deploy.sh` implementing the five steps and the failure behaviour in `contracts/deployment-scripts.md` (**FR-022, FR-023**). Include `set -euo pipefail`, `--dry-run`, and the exit-code convention (0/1/2). It pulls the SHA-tagged images T102 pushed and reads `release-manifest.json` (T103) to confirm the images it is about to run are the ones the gate built — field names must match T103 exactly. **A migration failure must abort with the old release still serving.** **Verify**: `bash -n ops/deploy.sh` and `./ops/deploy.sh --dry-run abc1234` prints a plan without acting.
- [X] T065 [US2] Create `ops/rollback.sh` per the same contract (**FR-024**). **It must refuse by default when migrations ran between the two SHAs**, print which ones, and point at `docs/backup-restore.md` (T078); `--force` overrides. Refusing is stronger than documenting a caveat. **Verify**: `bash -n ops/rollback.sh`.
- [X] T066 [P] [US2] Create `docs/deployment.md` (**FR-024, FR-047**): bootstrap a fresh host, promote a release, roll back, and the rollback limitation for destructive migrations. Include obtaining and storing registry credentials, since T102 pushes to an authenticated registry. **Verify**: prose only, no command needed.
- [X] T067 [US2] Add a `/ready` readiness gate to `ops/deploy.sh` (**FR-027**) — poll until ready or a 120s timeout, and abort leaving the old release serving on timeout. `/ready`'s contract is unchanged by this feature; do not modify it. **Verify**: `bash -n ops/deploy.sh`.
- [X] T068 [US2] Add `.env.staging.example` at the repository root listing every variable staging needs (**FR-025**), **with placeholder values only — never real secrets**. Include the alert destination variables from T108, and the `RELEASE_SHA` and registry-path variables **named exactly as T063's overlay and T102's pushed tags use them** — an example file whose variable names differ from the compose overlay is worse than none. **Verify**: `grep -iE '(secret|key|password|token)=.+' .env.staging.example` returns only placeholders.
- [ ] T069 [MANUAL] [BLOCKED: provisioning] [US2] Provision the staging VM, install Docker, configure DNS, obtain TLS certificates, and add the container-registry credentials as repository secrets so T102's `release` job can push (**FR-026**). **Evidence to close**: the host responds to `/api/v1/health` over HTTPS, and one `release` job has pushed three SHA-tagged images.
- [ ] T070 [MANUAL] [BLOCKED: provisioning] [US2] Run `./ops/deploy.sh <sha>` against staging (**FR-022, SC-004**). **Target**: under 30 minutes, zero manual steps outside the procedure.
- [ ] T071 [MANUAL] [BLOCKED: provisioning] [US2] Deploy a SHA with a deliberately broken migration (**FR-023**); confirm the deploy aborts and the previous release never stops serving (curl `/health` throughout).
- [ ] T072 [MANUAL] [BLOCKED: provisioning] [US2] Run `./ops/rollback.sh <previous-sha>` (**FR-024, SC-005**). **Target**: under 15 minutes.
- [ ] T073 [MANUAL] [BLOCKED: provisioning] [US2] Confirm `/metrics` is unreachable from the public internet and reachable from the host's loopback (Principle III remediation, `plan.md` Complexity Tracking).
- [ ] T074 [MANUAL] [BLOCKED: provisioning] [US2] Soak CSP in report-only mode on staging (**FR-011**), collect violations, fix them, then set `APP_CSP_REPORT_ONLY=false` and confirm no breakage (`research.md` §4).

**Checkpoint**: A commit can be promoted to staging and rolled back.

---

## Phase 8 (US4): Backup & Restore — spec priority P4, sequenced 6th

> Scripts (T075–T079) are writable now and can be rehearsed against the **local** compose
> stack, since staging runs the same services. T080–T083 need staging and off-host storage.

- [X] T075 [US4] Create `ops/backup.sh` per `contracts/deployment-scripts.md` (**FR-028, FR-029, FR-031**). **Order is significant: mirror objects FIRST, then dump the database.** A restored row then always finds its object; the reverse order can capture a row pointing at an object that was never mirrored — this is FR-029's "consistency window". Include `age` encryption (FR-031), `nice`/`ionice` wrapping, retention pruning (FR-031), and the textfile-collector write of `backup_last_success_timestamp_seconds`. **The script must reject a destination that resolves to the local MinIO endpoint** (**FR-028** — a backup on the backed-up machine is not a backup). **Verify**: `bash -n ops/backup.sh` and run `./ops/backup.sh --target all` against the local stack with an off-host destination.
- [X] T076 [US4] Create `ops/restore.sh` per the same contract (**FR-029, FR-033**): refuses to target the live stack without `--force-live`; compares the dump's schema version against the target's migration state, applying intervening migrations if the dump is older and **failing loudly if the dump is newer than the code**; restores objects then database. **Verify**: `bash -n ops/restore.sh`.
- [X] T077 [US4] Create `ops/verify-backup.sh` (**FR-029, FR-030, SC-010**): restore the latest backup into a throwaway container, run the migration checker, count rows per tenant, compare against source, and report measured RPO/RTO against the 24h/4h objectives. **Non-zero exit if either objective is missed.** **Verify**: `bash -n ops/verify-backup.sh` and a successful local run.
- [X] T078 [P] [US4] Create `docs/backup-restore.md` (**FR-030, FR-047**): schedule, what is captured, the documented RPO/RTO, the restore procedure, the drill procedure, and the destructive-migration path referenced by `ops/rollback.sh` (T065).
- [X] T079 [US4] **Extend** `infra/prometheus-alerts.yml` — the file created in **T107** — with a fourth rule group firing when `backup_last_success_timestamp_seconds` is **stale** (older than 26 hours) (**FR-032**). **Alert on staleness, not on a failure event** — a backup that never ran emits no failure, so absence is the only signal that catches the silent case. Do not modify T107's three groups. **Verify**: `python3 -c "import yaml;yaml.safe_load(open('infra/prometheus-alerts.yml'))"` and the file contains four rule groups.
- [ ] T080 [MANUAL] [BLOCKED: provisioning] [US4] Configure the off-host backup destination and the `age` keypair (**FR-028, FR-031**); store the private key somewhere that survives host loss.
- [ ] T081 [MANUAL] [BLOCKED: provisioning] [US4] Install the nightly cron entry on staging and confirm one unattended run succeeds (**FR-028**).
- [ ] T082 [MANUAL] [BLOCKED: provisioning] [US4] Run the full drill: `./ops/verify-backup.sh` against a populated staging, recording measured RPO and RTO against the objectives (**FR-030, SC-010**).
- [ ] T083 [MANUAL] [BLOCKED: provisioning] [US4] Stop the cron job past one interval and confirm the staleness alert fires and reaches the T108 receiver (**FR-032, SC-011**).

**Checkpoint**: Backups run, and a restore has actually been rehearsed.

---

## Phase 9 (US2): Load Testing — capacity evidence

- [X] T084 [P] [US2] Create `loadtest/dashboard-read.js` (k6) exercising authenticated dashboard reads (**FR-041, FR-043, SC-014**). **`K6_BASE_URL` must have no default — exit 2 if unset** — and the host must match `LOADTEST_ALLOWED_HOSTS`. A `localhost` default is one careless variable away from pointing at production (FR-043). Encode SC-014's thresholds in the script (`http_req_failed < 1%`, `http_req_duration p(95) < 500ms`). **Verify**: `k6 run --vus 1 --duration 5s loadtest/dashboard-read.js` against a local backend (k6 installed by T097).
- [X] T085 [P] [US2] Create `loadtest/widget-conversation.js` covering session creation and message send (**FR-041, FR-043**). Same guard rails. **Note**: set `APP_RATE_LIMIT_ENABLED=false` on the target, or the limiter from US1 will refuse the load and the test measures the limiter rather than capacity.
- [X] T086 [P] [US2] Create `loadtest/webhook-ingest.js` covering signed inbound webhooks (**FR-041, FR-043**). Same guard rails.
- [X] T087 [P] [US2] Create `loadtest/README.md` documenting the concurrency and latency targets and how to record results as capacity evidence (**FR-042**). **T107's alert thresholds are read from this document** — if you change a target here, update the rule comment there.
- [ ] T088 [MANUAL] [BLOCKED: provisioning] [US2] Run all three scenarios against staging at the documented concurrency target; record throughput, latency distribution, and error rate (**FR-041, SC-014**).
- [ ] T089 [MANUAL] [BLOCKED: provisioning] [US2] Run a one-hour sustained soak; confirm no upward drift in latency or memory (**FR-044, SC-014**).

---

## Phase 10: Polish & Cross-Cutting Concerns

- [ ] T090 **[BLOCKED: Docker daemon not running]** Run the full gate locally and confirm green: `cd backend && cargo fmt --check && cargo clippy --workspace -- -D warnings && REQUIRE_DB_TESTS=1 cargo nextest run --workspace > /tmp/gate.txt 2>&1; echo "exit=$?"`. **Do not pipe to `tail`.**
- [X] T091 Run the frontend gate: `cd frontend && pnpm lint && pnpm format:check && pnpm exec ng build dashboard && pnpm exec ng build widget && pnpm exec ng test dashboard --watch=false --browsers=ChromeHeadless && pnpm exec ng test widget --watch=false --browsers=ChromeHeadless`.
- [ ] T092 Run the real-backend e2e suite: `cd frontend && pnpm test:e2e:real`. It boots a real backend, so it exercises the new header and limiter middleware.
- [ ] T093 Update the executed-test-count floor in `specs/033-production-hardening/baseline-test-count.txt` to the post-feature count (re-run the T003 command), and confirm the CI step from **T036** still reads that same path.
- [X] T094 [P] Add the `033-production-hardening` entry to the "Recent Changes" list in `CLAUDE.md`, following the format of the existing entries.
- [ ] T095 Work through `specs/033-production-hardening/quickstart.md` top to bottom, confirming every non-`[MANUAL]` scenario behaves as documented. Fix the quickstart where reality differs — it is the validation guide operators will use.
- [X] T096 Confirm no new permission, route, nav entry, or database table beyond `tenant_rate_limit_overrides` was introduced: `cd backend && cargo test -p server --test openapi_coverage && cargo test -p server --test auth && cargo test -p authz`. All must pass **unchanged** — if any needs editing, the feature has grown beyond its designed scope and that needs a human decision.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001–T004, T097, T098 — no dependencies
- **Phase 2 (Foundational)**: T005–T010, T099 — needs Phase 1 — **blocks US1**
- **Phase 3 (US1)**: T011–T032, T100 — needs Phase 2. **This is the MVP.**
- **Phase 4 (US5)**: T033–T042, T101–T103 — needs Phase 1 only (T001 for the toolchain pin, T003 for the count)
- **Phase 5 (US3)**: T043–T056, T104–T109 — needs Phase 1; T049 needs T025 and T100 from US1
- **Phase 6 (US6)**: T057–T061 — T057 needs T004; T058 must not clobber T109; the rest is independent
- **Phase 7 (US2)**: T062–T068 need T098 and T102; T069–T074 need a provisioned host
- **Phase 8 (US4)**: T075–T079 need nothing except T107 (T079 extends its file); T080–T083 need a host and off-host storage
- **Phase 9**: T084–T087 need T097 (k6); T088–T089 need a host
- **Phase 10 (Polish)**: needs everything else

### Critical path inside US1

T019 → T020 → T021 → T022 → T023 → T024 → T025 → T026 must run **in this order**. Each
migrates one call site and leaves the workspace green. Do not batch them. T099 must precede
all of them; T100 follows T029.

### Shared-file sequences — these are NOT parallel

- `infra/Caddyfile`: T098 creates → T017 fills one section → T062 fills two others
- `infra/docker-compose.yml`: T056 → T106
- `infra/prometheus-alerts.yml`: T107 creates → T079 extends
- `backend/crates/shared/cache/src/limiter.rs`: T008 → T009 → T010 → T099 → T025 → T100
- `backend/crates/shared/observability/src/errors.rs`: T051 → T052
- `docs/runbook.md`: T109 → T058
- `.github/workflows/backend.yml`: T034 → T035 → T036 → T039 → T102 → T103

### Parallel Opportunities

- Phase 5: T053, T054, T104, T105, T055 are five separate config files — fully parallel
- Phase 6: T057, T058, T059 are three separate documents — fully parallel
- Phase 9: T084, T085, T086, T087 are four separate files — fully parallel
- T016, T030, T031, T047 are independent new test files

---

## Implementation Strategy

### MVP scope — stop and validate here

**Phase 1 + Phase 2 + Phase 3 (T001–T032 plus T097–T100).** This is the only slice that
reduces active, exploitable risk, and the only one that needs no infrastructure that does not
yet exist. Sign-in goes from unthrottled to throttled, budgets stop multiplying per instance,
refusals become visible, and every response carries browser protections. Ship it, validate
it, then continue.

### Recommended order after the MVP

The spec's priority ranking (P1–P6) reflects *value*. The order below additionally accounts
for what is *blocked*, which the ranking could not know:

1. **US1** (P1) — MVP, unblocked
2. **US5** (P5) — unblocked, and protects every phase after it; also produces the SHA-tagged
   images and release manifest US2 deploys
3. **US3** (P3) — unblocked
4. **US6** (P6) — unblocked; produces the documents US2/US4 execute against
5. *(provision the staging host and registry credentials)*
6. **US2** (P2) — was blocked
7. **US4** (P4) — was blocked; also the largest build in the feature

This is a scheduling decision, not a re-prioritisation. If the host is provisioned early,
run US2 in its spec-priority position instead.

### Honest completion reporting

**109 tasks total** — T001–T109. IDs are labels, not positions; see rule 7.

**17 are `[MANUAL]` and cannot be completed by a model**: T018, T032, T041, T042, T061,
T069, T070, T071, T072, T073, T074, T080, T081, T082, T083, T088, T089.

- **12 of those are additionally `[BLOCKED: provisioning]`** — they need a staging VM,
  registry credentials, or off-host storage that does not exist yet: T069–T074, T080–T083,
  T088, T089.
- **5 need only a human, no new infrastructure**: T018 (iframe embed check), T032
  (two-instance shared-budget run), T041 and T042 (CI gate verification), T061 (clean-machine
  documentation walkthrough).

**92 of 109 are model-completable.** An all-`[X]` tasks.md for this feature would be false.
When reporting, state which manual tasks remain open and what evidence would close them.

---

## Phase 11: Convergence

**Why this phase exists**: a codebase assessment found eight gaps between what Phases 1–10
claim and what the code actually does. Every task below is a **defect in already-checked-off
work**, not new scope. Nothing here adds a feature, an endpoint, a permission, a table, or a
page — the same rule 4 from the top of this file still applies.

**Read this before starting**: three of these (T110, T111, T112) mean the system is broken
right now — the backend CI gate cannot run at all, and the metrics endpoint violates two
constitution principles. Do these three first, in order.

Do **not** re-do work already tracked by the still-unchecked tasks above (T030, T031, T041,
T042, T061, T069–T074, T080–T083, T088–T093, T095, T096). They are remaining work, not
defects, and they are not repeated here.

- [X] T110 **CRITICAL** Stop recording concrete request paths as metric labels per **data-model.md label table, FR-015** (contradicts — Constitution Principle II *and* Principle X). **The problem**: `backend/crates/shared/observability/src/metrics.rs` builds its labels from `request.uri().path()`, which is the **concrete** URL. A real request to `/api/v1/widget/v1/conversations/3f9a.../messages` therefore creates a Prometheus time series whose label contains that conversation UUID. Two separate constitution breaches: every distinct UUID makes a *new* permanent series (unbounded cardinality — Principle X, this will eventually exhaust Prometheus memory), and the UUIDs disclose tenant activity to anyone who can scrape (Principle II). Make exactly these four edits:
  1. In `metrics.rs`, add `use axum::extract::MatchedPath;` and, inside `metrics_middleware`, **replace** the `let path = request.uri().path().to_owned();` line with:
     ```rust
     let route = request
         .extensions()
         .get::<MatchedPath>()
         .map(|m| m.as_str().to_owned())
         .unwrap_or_else(|| "<unmatched>".to_owned());
     ```
     `MatchedPath` yields the route *template* (e.g. `/api/v1/widget/v1/conversations/{conversationId}/messages`), which is a bounded set. **This has been verified to work at the middleware's current position** in `router.rs` — do **not** move the middleware or switch it to `route_layer`. The `<unmatched>` fallback is for 404s and is required: falling back to the concrete path would let anyone flood the series count by requesting random URLs.
  2. In the same function, rename the label key `"path"` to `"route"` in **both** the `metrics::counter!` and `metrics::histogram!` calls, and pass `route` as the value. `"route"` is the name `data-model.md` specifies.
  3. Remove the `"status"` label from the `metrics::histogram!` call only (`data-model.md` gives `http_request_duration_seconds` the labels `route` and `method` only). **Leave the `"status"` label on the counter exactly as it is** — `infra/prometheus-alerts.yml` and `infra/grafana/dashboards/platform-overview.json` both query `http_requests_total{status=~"5.."}`, so renaming or removing it silently breaks the FR-021 error-rate alert while everything still looks green. Do **not** rename it to `status_class`.
  4. In `backend/crates/shared/observability/src/trace.rs`, add one field to the `request_span` info_span: `route = tracing::field::Empty`, and record the same `MatchedPath` template into it from `trace_middleware`. Keep the existing `path` field and its concrete value untouched — logs stay debuggable; this only adds the bounded template alongside it.

  **Then fix the guard test, which currently passes for two independent wrong reasons.** `backend/crates/server/tests/metrics_labels.rs` is vacuous today and would pass even with this bug fully present. Make **both** of these edits to it:
  - **(a) Install the recorder.** Add `observability::metrics::init();` as the **very first line of the test body**. Without it there is no global metrics recorder in the test binary — `metrics::init()` is only ever called from `init_observability` in `main.rs`, never from `router::app` — so every `counter!`/`histogram!` call silently discards into a no-op recorder and `/metrics` renders an essentially empty body. This is why the existing assertion passes. `observability` is already a dependency of the `server` package, so it is available to `tests/*.rs` with no `Cargo.toml` change.
  - **(b) Exercise a UUID-bearing route.** *After* the line above and **before** scraping `/metrics`, send a **POST** to `/api/v1/widget/v1/conversations/3f9a2b1c-1111-2222-3333-444455556666/messages` (that route is `send_message`, a POST). The response status does not matter — a 4xx/5xx still records, because `MatchedPath` is inserted during path matching, before method routing and before the handler runs.

  Keep the existing no-UUID assertion, and **add** a second assertion that the body contains the literal substring `route=`. **Verify**: `cd backend && cargo test -p server --test metrics_labels` passes, and prove it is no longer vacuous by temporarily reverting edit 1 and watching the test go **red**.

- [X] T111 **CRITICAL** Repair the unrunnable action reference in `.github/workflows/backend.yml` per **FR-034, FR-036, FR-037** (contradicts). The file contains `- uses: dtolnay/rust-toolchain` with **no `@ref`**. A GitHub Actions `uses:` value must always carry a ref (`owner/repo@ref`), so this workflow fails to resolve the action and **the entire backend gate never executes** — which is why nothing has caught T112–T114 below. T034 removed `@stable` but did not put anything valid in its place. Replace that whole step with a step that installs the toolchain from the pinned file created in T001, keeping `backend/rust-toolchain.toml` as the single source of the version:
  ```yaml
      - name: Install toolchain from rust-toolchain.toml
        run: rustup show active-toolchain || rustup toolchain install
  ```
  The job already sets `defaults: { run: { working-directory: backend } }`, so this runs in `backend/` and rustup reads `backend/rust-toolchain.toml`, installing 1.84.0 with the `rustfmt` and `clippy` components declared there. **Do not** hardcode `1.84.0` in the workflow — two copies of the version will drift. **Also add a following step that proves the install actually happened**, because a silent no-op here would leave the toolchain floating while the workflow still goes green: `- run: rustc --version` , and confirm in the run log that it prints `1.84.0`. **Verify**: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/backend.yml'))"` succeeds and `grep -n 'uses: dtolnay' .github/workflows/backend.yml` returns nothing.

- [X] T112 **CRITICAL** Add the missing Redis service to `.github/workflows/backend.yml` per **FR-034, FR-003** (missing). The `verify` job sets `REQUIRE_DB_TESTS: "1"` but declares only a `postgres` service. `backend/crates/shared/cache/src/limiter.rs` **panics** with `"REQUIRE_DB_TESTS=1 but Redis is unreachable"` under exactly that combination, so the gate cannot pass and FR-003's shared-budget tests never execute in CI. Add this second entry under the existing `services:` key, alongside `postgres`:
  ```yaml
      redis:
        image: redis:7-alpine
        ports: ["6379:6379"]
        options: >-
          --health-cmd "redis-cli ping" --health-interval 5s
          --health-timeout 5s --health-retries 10
  ```
  Also add `REDIS_URL: redis://localhost:6379` to the job's existing `env:` block — that is the variable name `limiter.rs` reads. Pin the image tag; never use `redis:latest`. **Verify**: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/backend.yml'))"` succeeds, and the file contains both `redis:7-alpine` and `REDIS_URL`.

- [X] T113 Fix the shell redirection that discards the test count in `.github/workflows/backend.yml` per **FR-035, SC-009** (contradicts). The step currently runs `cargo nextest run --workspace 2>&1 > /tmp/nextest.out`. **That order is backwards**: `2>&1` first points stderr at wherever stdout currently goes (the console), and *then* `>` moves stdout to the file — so stderr never reaches the file. cargo-nextest prints its `Summary [ ... ] N tests run` line to **stderr**, so `/tmp/nextest.out` never contains it, the `grep -oP '\d+(?= tests run)'` matches nothing, and `COUNT` falls back to `0`. Change that one line to put the redirect first:
  ```bash
          cargo nextest run --workspace > /tmp/nextest.out 2>&1; EXIT=$?
  ```
  Change nothing else in the step. **Verify**: `grep -n '> /tmp/nextest.out 2>&1' .github/workflows/backend.yml` matches, and `grep -c '2>&1 > /tmp/nextest.out' .github/workflows/backend.yml` returns 0.

- [ ] T114 Replace the implausible CI test-count floor in `specs/033-production-hardening/baseline-test-count.txt` per **FR-035, SC-009**. The file currently reads **100**, but the backend contains roughly **1577** test functions across 77 integration test files. A floor of 100 means ~90% of the suite could stop running and the gate would still report green — which is precisely the regression FR-035 exists to detect, so the requirement is unmet despite T003 being checked off. **Do T113 first** — until the redirection bug is fixed the count cannot be read at all. Then, with `docker compose -f infra/docker-compose.yml up -d` running and `cd backend && sqlx migrate run` applied, run:
  ```bash
  cd backend && REQUIRE_DB_TESTS=1 cargo nextest run --workspace > /tmp/baseline.txt 2>&1; E=$?; echo "exit=$E"; exit "$E"
  ```
  The exit code **must** be 0 — a floor taken from a failing run is worthless. Read the integer from the `Summary [ ... ] N tests run` line in `/tmp/baseline.txt` and write **only that integer** into `specs/033-production-hardening/baseline-test-count.txt`. If the number is still anywhere near 100, stop and report it rather than writing it — that would mean the suite is not actually running. T093 (below, unchecked) will refresh this file at the end of the feature; that is expected and does not substitute for completing this task first. **Verify**: the file holds a single integer in the four-figure range, and `cat` shows no trailing text.

- [X] T115 Add the missing secret scan to the gate per **FR-038** (missing). FR-038 requires the gate to fail on "a committed credential-shaped string", and this is currently checked **nowhere** — there is no gitleaks, trufflehog, or detect-secrets step in any workflow. T037 was written on the assumption that a secret-scan step already existed in `frontend.yml` and only needed its `continue-on-error` removed; no such step exists, so only the `cargo audit` (vulnerable-dependency) half of FR-038 is satisfied. Add a new top-level job to `.github/workflows/backend.yml`:
  ```yaml
    secret-scan:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
          with: { fetch-depth: 0 }
        - uses: gitleaks/gitleaks-action@v2
          env:
            GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  ```
  `fetch-depth: 0` is required — gitleaks scans history, and a shallow clone silently scans almost nothing. **Do not** add `continue-on-error` to this job; FR-038 states the check must not be advisory. Place it as a sibling of the existing `verify` and `release` jobs, and add `secret-scan` to the `release` job's `needs:` list so a leaked credential blocks image publication. **If the action fails on licensing** (`gitleaks-action@v2` requires a `GITLEAKS_LICENSE` secret for organisation-owned or private repositories), do not disable the job — replace the action step with a direct binary run instead, which has no licence requirement: download the gitleaks release tarball in a `run:` step and execute `gitleaks detect --source . --redact --exit-code 1`. FR-038 requires that the check exists and is non-advisory; it does not mandate that particular action. **Verify**: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/backend.yml'))"` succeeds, the file contains `gitleaks`, and the `release` job's `needs:` names both `verify` and `secret-scan`.

- [X] T116 Populate the outbox backlog gauge and align its dashboard panel per **FR-015, FR-018** (partial). Three different names are currently in play and none of them work together: `backend/crates/shared/observability/src/metrics.rs:61` declares `outbox_backlog` via `describe_gauge!` but **no code anywhere ever sets it**, so the series never appears in the exposition at all; `data-model.md` specifies `outbox_backlog` with a `queue` label; and `infra/grafana/dashboards/platform-overview.json` queries a fourth spelling, `outbox_pending_total`. The result is that FR-015's required "background-work backlog" measurement does not exist and FR-018's backlog panel renders "No data". Two edits:
  1. **Add a setter in `observability`, and drive it from `server`.** Do **not** query the database from the `observability` crate — it has no `sqlx` dependency and deliberately touches no application table; adding one would cross the layering boundary Principle I protects. Note also that `spawn_dependency_updater(checks: Vec<Arc<dyn HealthCheck>>)` has **no pool**, so it cannot do this itself. Instead:
     - In `backend/crates/shared/observability/src/metrics.rs`, add a tiny setter that takes an already-computed number:
       ```rust
       pub fn set_outbox_backlog(count: i64) {
           metrics::gauge!("outbox_backlog", "queue" => "outbox").set(count as f64);
       }
       ```
     - In `backend/crates/server/src/main.rs`, right after the existing `observability::metrics::spawn_dependency_updater(health_checks.clone());` call, spawn a 15-second loop that reuses spec 032's **existing** query rather than writing a new one — `platform::queries::queue_health(&pool)` returns `(i64, Option<DateTime<Utc>>, i64)` whose `.0` is the pending count (`SELECT COUNT(*) ... FROM outbox_events WHERE processed_at IS NULL`) — and passes that `.0` to `observability::metrics::set_outbox_backlog(...)`. The `server` crate already depends on `platform`, and this mirrors T048's rule of reading 032's existing results instead of re-probing. **Do not** add the `metrics` crate to `server/Cargo.toml`; the setter is why it is not needed. **Do not** create a new table, migration, endpoint, or query.
  2. In `infra/grafana/dashboards/platform-overview.json`, change the panel's `"expr"` from `outbox_pending_total` to `outbox_backlog`.

  **Verify**: `cd backend && cargo build --workspace` exits 0; `python3 -c "import json;json.load(open('infra/grafana/dashboards/platform-overview.json'))"` succeeds; and with the stack up, `curl -s localhost:8080/metrics | grep outbox_backlog` shows a value.

- [X] T117 Pin the floating Rust toolchain in `.github/workflows/frontend.yml` per **FR-036** (partial). Line ~70, in the `real-backend-e2e` job, still reads `- uses: dtolnay/rust-toolchain@stable`. `@stable` floats — the same commit can yield a different verdict on a different day, which is exactly what FR-036 forbids, and this job gates merges because it boots a real backend. T034 fixed only `backend.yml`. Replace that step with the same file-driven install used in T111:
  ```yaml
      - name: Install toolchain from rust-toolchain.toml
        run: rustup show active-toolchain || rustup toolchain install
        working-directory: backend
  ```
  Note the explicit `working-directory: backend` — unlike `backend.yml`, this workflow does **not** set a `backend` default, so without it rustup will not find `rust-toolchain.toml` and will silently leave the toolchain floating. For the same reason, add a following step `- run: rustc --version` with `working-directory: backend` and confirm the run log prints `1.84.0`. **Verify**: `grep -c 'dtolnay/rust-toolchain@stable' .github/workflows/frontend.yml` returns 0 and the YAML parses.

**Checkpoint**: the backend gate actually runs, actually fails on breakage, and enforces a
meaningful test-count floor; `/metrics` carries bounded, non-identifying labels; the backlog
panel shows data.

---

## Phase 12: Convergence

**Why this phase exists**: a second codebase assessment found fourteen gaps between what
Phases 1–11 claim and what the code actually does. Every task below is a **defect in
already-checked-off work**, not new scope. Nothing here adds a feature, an endpoint, a
permission, a table, or a page — rule 4 from the top of this file still applies.

**Read this before starting**: T118 and T119 mean two headline requirements do not work at
all right now — every anonymous caller shares a single rate-limit bucket, and no error is
ever captured. Do those two first, in order.

Do **not** re-do work already tracked by the still-unchecked tasks above (T032, T041, T042,
T061, T069–T074, T080–T083, T088–T093, T095, T096). In particular:
`specs/033-production-hardening/baseline-test-count.txt` still holds the wrong value (100) —
that is **T093's** job, not a task here. Do not add a duplicate.

Verification note: `cargo build --workspace`, `cargo fmt --check`, and
`cargo clippy --workspace --all-targets -- -D warnings` all exit 0 at the time this phase was
written. The test suite was **not** run (no Docker daemon available), so T090/T092 remain the
runtime gate.

- [X] T118 **CRITICAL** Use the real client address instead of the hard-coded loopback per **FR-004, FR-002** (contradicts). **The problem**: `backend/crates/server/src/rate_limit.rs` contains this shape in **two** places (`login_rate_limit` at line ~51 and `per_ip_creation_limit` at line ~94):

  ```rust
  let client_ip = if config.trusted_proxy { /* read X-Forwarded-For */ } else {
      "127.0.0.1".to_string()          // <-- every anonymous caller on Earth
  };
  ```

  `APP_TRUSTED_PROXY` defaults to **false**, so in the default configuration every anonymous caller is keyed on the literal string `127.0.0.1`. They therefore all share **one** 10-per-minute bucket: one attacker exhausts it and locks out every legitimate user at once, which is the exact collective-lockout failure FR-004 exists to prevent. `grep -rn ConnectInfo backend/crates` returns nothing today — the peer address is never extracted anywhere. Three edits, all required:

  1. **Serve with connection info.** In `backend/crates/server/src/main.rs`, find the `axum::serve(listener, ...)` call and change the service argument to `app.into_make_service_with_connect_info::<std::net::SocketAddr>()`. Without this, step 2 finds nothing at runtime.
  2. **Read it optionally in both functions.** In `backend/crates/server/src/rate_limit.rs`, add `use axum::extract::ConnectInfo;` and `use std::net::SocketAddr;`, then replace **both** `else { "127.0.0.1".to_string() }` branches with a lookup through the request extensions:

     ```rust
     } else {
         request
             .extensions()
             .get::<ConnectInfo<SocketAddr>>()
             .map(|ConnectInfo(addr)| addr.ip().to_string())
             .unwrap_or_else(|| "unknown".to_string())
     };
     ```

     In `login_rate_limit` the request has already been split, so read from `parts.extensions` instead of `request.extensions()`. **Do NOT add `ConnectInfo<SocketAddr>` as a handler/middleware argument.** Roughly 80 files under `backend/crates/server/tests/` build the router with `tower::ServiceExt::oneshot` and never populate that extension; a required extractor makes every one of them return 500. The optional lookup above keeps them all green.
  3. **Route it through the one identifier function.** `cache::limiter::select_identifier` was written in T099 and is currently called from **nowhere** (`grep -rn select_identifier backend/crates` finds only its definition and a doc line). In `per_ip_creation_limit`, pass the resolved address through it: `let identifier = cache::limiter::select_identifier(None, &client_ip);` and use `identifier` in the `store.check(...)` call. Leave `login_rate_limit`'s `format!("{}:{}", email, client_ip)` composite key exactly as it is — T027 requires that shape.

  **Do not** change the `trusted_proxy == true` branch; T029 and T062 own it. **Verify**: `cd backend && cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings` exit 0, then `REQUIRE_DB_TESTS=1 cargo test -p server --test rate_limit_login && cargo test -p server --test widget_public_foundation` pass, and `grep -c '"127.0.0.1".to_string()' crates/server/src/rate_limit.rs` returns 0.

- [X] T119 **CRITICAL** Install the error-capture layer that makes Sentry receive anything per **FR-017, FR-014, SC-007** (missing). **The problem**: `sentry-tracing` is declared at `backend/crates/shared/observability/Cargo.toml:24` but `grep -rn sentry_tracing backend/crates` returns **nothing**. `errors::init` builds a client and a `before_send` scrubber, but no layer ever forwards a `tracing` event to it, so **zero** errors are captured no matter what `APP_SENTRY_DSN` is set to. FR-017's grouped error record does not exist. Two edits in `backend/crates/shared/observability/src/lib.rs`:
  1. Add `sentry_tracing::layer()` to the subscriber in **both** branches of `init_observability` — the `if let Some(endpoint) = otlp_endpoint` branch (which already uses `Registry::default().with(...)`) and the `else` branch. The `else` branch currently uses the `tracing_subscriber::fmt()` builder, which cannot take extra layers; convert it to the same `Registry::default().with(fmt_layer).with(sentry_tracing::layer())` shape the first branch uses. Keep the existing `EnvFilter` behaviour and the `LogFormat::Json` / `LogFormat::Pretty` split identical.
  2. Configure the layer so `tracing::error!` events become Sentry **events** (not just breadcrumbs) — use `sentry_tracing::layer().event_filter(|md| match *md.level() { tracing::Level::ERROR => sentry_tracing::EventFilter::Event, tracing::Level::WARN => sentry_tracing::EventFilter::Breadcrumb, _ => sentry_tracing::EventFilter::Ignore })`.

  **Then attach the correlation identifier** (FR-014's third surface, per T051): in `backend/crates/shared/observability/src/trace.rs`, inside `trace_middleware` where the request ID is already computed, add `sentry::configure_scope(|scope| scope.set_tag("request_id", &request_id));`. Add `sentry.workspace = true` to `observability`'s dependencies only if it is not already there (it is). **Do not** change the scrubber in `errors.rs`; its tests already pass. **Verify**: `cd backend && cargo build --workspace && cargo test -p observability` exit 0, `grep -c sentry_tracing crates/shared/observability/src/lib.rs` returns at least 1, and `cargo run -p server` with **no** `APP_SENTRY_DSN` set still starts and logs normally (FR-016/FR-017 both require the unset case to be a full no-op).

- [X] T120 Wire the budget resolver so per-tenant overrides actually take effect per **FR-006** (missing). **The problem**: `BudgetResolver` in `backend/crates/shared/cache/src/limiter.rs` is fully implemented and fully unused — `grep -rn BudgetResolver backend/crates` finds only its definition and one doc line. The `tenant_rate_limit_overrides` table created by migration `0061` (this feature's **only** migration) is read by **no** SQL anywhere. Every call site passes a hard-coded `Budget { max_requests: 10, ... }` literal, so FR-006's "adjustable per tenant without a code change" is false. Do this:
  1. In `backend/crates/server/src/state.rs` (the `AppState` struct), add a field `pub budget_resolver: Arc<cache::limiter::BudgetResolver>`. Construct it in `backend/crates/server/src/main.rs` with `Arc::new(cache::limiter::BudgetResolver::new(config.rate_limit_max_override))` and add the same field to **every** `AppState { .. }` literal that fails to compile — there are many under `backend/crates/server/tests/`; use `Arc::new(cache::limiter::BudgetResolver::new(10_000))` in the tests. This is a mechanical fix-the-compiler pass; **do not** change any test assertion.
  2. Add a fetcher function in `backend/crates/server/src/rate_limit.rs` that reads the override row. Model the SQL on the existing writers in `backend/crates/modules/tools/src/audit.rs`:

     ```rust
     async fn fetch_override(pool: &sqlx::PgPool, tenant_id: &str, scope: kernel::Scope) -> Option<kernel::Budget> {
         let id = uuid::Uuid::parse_str(tenant_id).ok()?;
         let row: Option<(i32, i32)> = sqlx::query_as(
             "SELECT max_requests, window_seconds FROM tenant_rate_limit_overrides \
              WHERE tenant_id = $1 AND scope = $2 AND deleted_at IS NULL",
         )
         .bind(id)
         .bind(scope.as_str())
         .fetch_optional(pool)
         .await
         .ok()
         .flatten();
         row.map(|(m, w)| kernel::Budget { max_requests: m as u32, window: std::time::Duration::from_secs(w as u64) })
     }
     ```

     Check the real column names against the `Persisted entities` section of `specs/033-production-hardening/data-model.md` and the migration file before writing this — use whatever the migration actually created.
  3. Change `check_global_tenant_limit` in the same file to resolve its budget through `resolver.resolve(tenant_id, Scope::TenantGlobal, |t, s| fetch_override(&pool, &t, s)).await` instead of the `GLOBAL_TENANT_LIMIT` literal. **Only this one call site** — it is the only scope with a tenant ID in hand. Leave the anonymous widget/webhook/login literals alone; they are covered by env config, which FR-006 also accepts.

  **Do not** add an endpoint, route, or permission for editing overrides. Rows are inserted by an operator with SQL; FR-006 says "without a code change", not "with a UI". **Verify**: `cd backend && cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings` exit 0, and `cargo test -p server --test openapi_coverage && cargo test -p server --test auth` pass **unchanged**.

- [X] T121 Write the durable audit entry for tenant-attributable refusals per **FR-008** (missing). **Read FR-008 in `spec.md` first — it is very specific about which refusals may be written and which must not.** Today `grep -rn 'security.rate_limit.refused' backend/crates` returns nothing: only the structured log half of T100 was built. Add the audit half, and **only** for the authenticated, tenant-attributable case:
  - Create `backend/crates/server/src/rate_limit_audit.rs` with a single `pub async fn record_refusal(pool: &sqlx::PgPool, tenant_id: uuid::Uuid)`. Copy the shape of `record_execution` in `backend/crates/modules/tools/src/audit.rs` verbatim: one `INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details) VALUES ($1,$2,$3,$4,$5,$6)` with `actor_user_id` = `Option::<Uuid>::None`, `action` = `"security.rate_limit.refused"`, `resource_type` = `"rate_limit"`, `resource_id` = the tenant ID as a string. Log the error and return on failure — an audit write must never fail a request.
  - Call it from `check_global_tenant_limit` in `backend/crates/server/src/rate_limit.rs` **only on the `Decision::Refused` branch**, and **only** for `Scope::TenantGlobal`. Deduplicate to at most one row per tenant per window with a process-local `Mutex<HashMap<Uuid, Instant>>` and a 60-second gate, mirroring `BudgetResolver`'s cache.
  - **Do NOT** write an audit row for `Scope::Login`, `Scope::WidgetSession`, `Scope::WidgetMessage`, or `Scope::Webhook`. Those callers are unauthenticated. `audit_logs` carries a `forbid_mutation` trigger blocking UPDATE and DELETE, so an anonymous flood would write rows nobody can ever prune — FR-008 forbids this explicitly.
  - **Do NOT** add a `security.` entry to `CATEGORY_PREFIXES` in `backend/crates/modules/audit/src/model.rs`. The action falls through to the `"other"` category, which is correct and keeps that module's existing tests green.

  **Verify**: `cd backend && cargo build --workspace` exits 0 and `cargo test -p audit && cargo test -p server --test audit_logs && cargo test -p server --test openapi_coverage` all pass **unchanged**.

- [X] T122 Repair `.github/workflows/gate-verification.yml` so its verdict is real per **FR-039, SC-008** (contradicts). The workflow that proves the gate can fail is itself broken in three ways, so it has never produced a meaningful result. Three edits to that file:
  1. Line ~24 reads `- uses: dtolnay/rust-toolchain` with **no `@ref`**. A GitHub Actions `uses:` must be `owner/repo@ref`, so the action never resolves and the job dies before it starts. This is the identical defect T111 fixed in `backend.yml`; apply the identical fix — replace the whole step with:
     ```yaml
           - name: Install toolchain from rust-toolchain.toml
             run: rustup show active-toolchain || rustup toolchain install
     ```
     The job already sets `defaults: { run: { working-directory: backend } }`, so rustup reads `backend/rust-toolchain.toml`. Do **not** hardcode the version.
  2. The job declares only a `postgres` service but sets `REQUIRE_DB_TESTS: "1"`. `backend/crates/shared/cache/src/limiter.rs` **panics** with `"REQUIRE_DB_TESTS=1 but Redis is unreachable"` under that combination — so the gate would fail for the wrong reason and this workflow would report `"OK: gate correctly rejected broken code"` while proving nothing. Add the same `redis` block T112 added to `backend.yml` under `services:`, and add `REDIS_URL: redis://localhost:6379` to the job's `env:`.
  3. The run step has `cargo nextest run --workspace 2>&1 > /tmp/gate-out.txt` — the same backwards redirection T113 fixed. Change it to `cargo nextest run --workspace > /tmp/gate-out.txt 2>&1; EXIT=$?` so the failure output actually lands in the file that gets `cat`-ed.

  **Verify**: `python3 -c "import yaml;yaml.safe_load(open('.github/workflows/gate-verification.yml'))"` succeeds, `grep -c 'uses: dtolnay' .github/workflows/gate-verification.yml` returns 0, and the file contains both `redis:7-alpine` and `REDIS_URL`.

- [X] T123 Make Grafana actually load the dashboard per **FR-018** (missing). **The problem**: `infra/docker-compose.yml` mounts `./grafana/dashboards` (which contains only the raw dashboard file `platform-overview.json`) at `/etc/grafana/provisioning/dashboards`. Grafana treats **every file in that directory as a provider config**, not as a dashboard — it fails to parse `platform-overview.json` as a provider and loads **no dashboards at all**. FR-018's operator-facing health view therefore does not exist, even with the observability profile up. Two edits:
  1. Create `infra/grafana/provisioning/dashboards/dashboards.yaml`:
     ```yaml
     apiVersion: 1
     providers:
       - name: platform
         orgId: 1
         folder: ''
         type: file
         disableDeletion: false
         updateIntervalSeconds: 30
         allowUiUpdates: false
         options:
           path: /var/lib/grafana/dashboards
           foldersFromFilesStructure: false
     ```
  2. In `infra/docker-compose.yml`, change the `grafana` service's volume list so the provider directory and the dashboard directory are mounted at the two **different** paths the provider above expects:
     ```yaml
           - ./grafana/provisioning:/etc/grafana/provisioning:ro
           - ./grafana/dashboards:/var/lib/grafana/dashboards:ro
     ```
     Replace the two existing `./grafana/dashboards:/etc/grafana/provisioning/dashboards` and `./grafana/provisioning/datasources:/etc/grafana/provisioning/datasources` lines with these two. The single `./grafana/provisioning` mount already covers the datasources subdirectory created in T105. Leave the `grafana-data` volume line alone.

  **Verify**: `docker compose -f infra/docker-compose.yml --profile observability config` exits 0, and with the stack up, `curl -s -u admin:admin localhost:3001/api/search?query=Platform` lists the dashboard (anonymous admin is enabled, so plain `curl -s localhost:3001/api/search` also works).

- [X] T124 Make the deploy scripts reference the images CI actually publishes per **FR-022, FR-040** (contradicts). **The problem**: T102 pushes exactly three tags — `ghcr.io/<owner>/<repo>-backend:<sha>`, `<repo>-dashboard:<sha>`, `<repo>-widget:<sha>` (note the **hyphen**, they are suffixes on the repository name). `ops/deploy.sh:53-55` and `ops/rollback.sh:52-53` instead build `${IMAGE_REGISTRY}/backend:<sha>` and `${IMAGE_REGISTRY}/frontend:<sha>` — a repository path that is never pushed — and the widget image is never referenced at all. Every deploy would fail at the `docker manifest inspect` step. `infra/docker-compose.staging.yml` has the same two wrong names. Also, T064 required `deploy.sh` to read `release-manifest.json`; it currently never mentions it. Edits:
  1. In **both** `ops/deploy.sh` and `ops/rollback.sh`, replace the image constants with the three real ones, defaulting `IMAGE_BASE` to the repository path (keep `IMAGE_BASE` overridable by environment):
     ```bash
     readonly IMAGE_BASE="${IMAGE_BASE:-ghcr.io/mohamed155/customer-service-chat-system}"
     readonly BACKEND_IMAGE="${IMAGE_BASE}-backend:${SHA}"
     readonly DASHBOARD_IMAGE="${IMAGE_BASE}-dashboard:${SHA}"
     readonly WIDGET_IMAGE="${IMAGE_BASE}-widget:${SHA}"
     ```
     (`rollback.sh` uses `${PREVIOUS_SHA}` instead of `${SHA}`.) Update every `docker manifest inspect` and `docker pull` line so all **three** images are checked and pulled, not two.
  2. In `infra/docker-compose.staging.yml`, rename the `frontend` service to `dashboard`, point it at `${IMAGE_BASE}-dashboard:${APP_RELEASE_SHA}`, point `backend` at `${IMAGE_BASE}-backend:${APP_RELEASE_SHA}`, and **add** a `widget` service using `${IMAGE_BASE}-widget:${APP_RELEASE_SHA}` on a distinct host port. Keep the `${APP_RELEASE_SHA:?...}` required-variable guard on all three and keep every tag SHA-pinned — **never `latest`**.
  3. In `ops/deploy.sh`, add a step **between** the current Step 1 and Step 2 that reads the manifest when it is present: if a file path is given via `--manifest <path>` (add this flag), parse it with `python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['images']['backend'])"` and **abort with exit 1** if any of the three `images.*` values does not equal the corresponding `*_IMAGE` variable the script computed. Use the field names exactly as T103 writes them: `commit_sha`, `built_at`, `workflow_run_id`, `tests_executed`, `images.backend|dashboard|widget`. When `--manifest` is not supplied, skip the check and print a warning — the staging host may not have the artifact.

  **Verify**: `bash -n ops/deploy.sh && bash -n ops/rollback.sh` exit 0; `./ops/deploy.sh --dry-run abc1234` prints all three image names; `APP_RELEASE_SHA=abc1234 APP_DOMAIN=example.test docker compose -f infra/docker-compose.yml -f infra/docker-compose.staging.yml config` exits 0 and its output contains no `:latest`.

- [X] T125 Make the alert receiver URL resolve at runtime per **FR-021** (contradicts). `infra/alertmanager.yml:11` sets `url: '${APP_ALERT_WEBHOOK_URL}'`. **Alertmanager does not expand environment variables in its configuration file** — it uses that eleven-character literal as the webhook URL, so every alert delivery fails and FR-021's "notify an on-call recipient" is unmet. The compose plumbing is already correct (`infra/docker-compose.yml` passes `APP_ALERT_WEBHOOK_URL` into the container); only the config file is wrong. Prefer the first option that works:
  1. **Preferred** — if `prom/alertmanager:v0.28.0` supports `webhook_configs[].url_file`, replace `url:` with `url_file: /etc/alertmanager/webhook_url` and add a compose mount or an entrypoint that writes `$APP_ALERT_WEBHOOK_URL` into that path. Confirm support with `docker run --rm prom/alertmanager:v0.28.0 --help` / the 0.28 docs before committing to it.
  2. **Fallback** — rename the file to `infra/alertmanager.yml.tmpl`, keep `${APP_ALERT_WEBHOOK_URL}` in it, and give the `alertmanager` service an entrypoint that runs `envsubst < /etc/alertmanager/alertmanager.yml.tmpl > /tmp/alertmanager.yml` before starting with `--config.file=/tmp/alertmanager.yml`.

  Either way, **no real destination may be committed** — FR-025 forbids secrets in version control. **Verify**: `docker compose -f infra/docker-compose.yml --profile observability config` exits 0, and with `APP_ALERT_WEBHOOK_URL=http://example.test/hook` exported and the profile up, `curl -s localhost:9093/api/v2/status` shows the resolved URL in the loaded config (Alertmanager exposes its effective config there).

- [X] T126 Complete the configuration reference and the staging example per **FR-046, SC-013, FR-025** (missing). Three variables the platform or the deployment actually reads are documented nowhere. Two files:
  1. `docs/configuration.md` — add **`APP_TRUSTED_PROXY`**. It is read at `backend/crates/shared/config/src/lib.rs:400`, defaults to `false`, and controls whether `X-Forwarded-For` is trusted for rate-limit keying. Document that it **must be `true` in staging and production** (traffic arrives through Caddy, which overwrites the header per T062) and **must stay `false` in local development** (where nothing sanitises the header and it would be spoofable). SC-013 requires 100% coverage of what the code reads, so this omission alone fails that criterion. While in the file, add `APP_ALERT_WEBHOOK_URL` (read by Alertmanager, T125), `APP_DOMAIN` (required by `infra/docker-compose.staging.yml`'s Caddy service), and `IMAGE_BASE` (read by `ops/deploy.sh`, T124) under a clearly-labelled "deployment-time, not read by the backend" heading.
  2. `.env.staging.example` — add all four of the same variables with placeholder values only: `APP_TRUSTED_PROXY=true`, `APP_ALERT_WEBHOOK_URL=https://hooks.example.com/replace-me`, `APP_DOMAIN=staging.example.com`, `IMAGE_BASE=ghcr.io/mohamed155/customer-service-chat-system`. The names must match T124's script variables and the compose overlay **exactly**.

  **Verify**: run the `comm` check from the US6 section of `specs/033-production-hardening/quickstart.md`; it must print **nothing**. Also `grep -c APP_TRUSTED_PROXY docs/configuration.md .env.staging.example` returns 1 for each, and `grep -iE '(secret|key|password|token)=.+' .env.staging.example` still returns only placeholders.

- [X] T127 Make the backup scripts executable per **FR-028, FR-047** (partial). `ops/backup.sh`, `ops/restore.sh`, and `ops/verify-backup.sh` are mode `644`; only `deploy.sh` and `rollback.sh` carry the executable bit. `docs/backup-restore.md:8` installs the cron entry `0 2 * * * /opt/app/ops/backup.sh --target all >> /var/log/backup.log 2>&1`, which exits `126 Permission denied` — so the nightly backup FR-028 requires would never run, and FR-032's staleness alert would be the first anyone hears of it. Run `chmod +x ops/backup.sh ops/restore.sh ops/verify-backup.sh`, then commit the mode change (`git update-index --chmod=+x` is not needed if the working tree mode is already correct — `git status` will show the mode change). **Verify**: `ls -l ops/` shows `-rwxr-xr-x` on all five scripts, and `git diff --summary` reports `mode change 100644 => 100755` for the three.

- [X] T128 Hash the identifier in the degraded-mode log per **FR-019, FR-008** (contradicts). `backend/crates/shared/cache/src/limiter.rs`, inside `degrade()` (line ~73), logs the **raw** identifier:
  ```rust
  warn!("rate_limit_degraded scope={} identifier={}", scope.as_str(), identifier);
  ```
  For `Scope::Login` that identifier is `"<email>:<ip>"`, so every Redis blip writes customer email addresses and source addresses into the log stream that T104 ships to Loki. FR-019 requires masking **at the point telemetry is captured**, and the refusal path twelve lines below already does it correctly with `hash_identifier`. Make the degraded path match it — use the structured-field form, not the interpolated string:
  ```rust
  warn!(
      event = "rate_limit_degraded",
      scope = scope.as_str(),
      identifier_hash = %hash_identifier(identifier),
      "rate limit degraded",
  );
  ```
  Change nothing else in the function — the `degraded_total` counter, the metrics increment, and the `Scope::Login` fail-closed branch all stay exactly as they are. **Verify**: `cd backend && cargo test -p cache` passes and `grep -c 'identifier={}' crates/shared/cache/src/limiter.rs` returns 0.

- [X] T129 Add the `033-production-hardening` entry to `CLAUDE.md` per **FR-049** (missing). T094 is checked off but `grep -n 033-production CLAUDE.md` returns nothing. Add one bullet at the **top** of the "Recent Changes" list (line ~13, above the `029-whatsapp-channel` entry), matching the existing entries' single-paragraph style and ending with a `See specs/033-production-hardening/plan.md.` reference. Summarise what actually shipped: Redis-backed shared rate limiting with a sign-in throttle, browser security headers with a widget-specific exception, real Prometheus metrics plus OTLP trace export and Sentry error capture, a Grafana/Loki/Alertmanager observability profile, a consolidated pinned CI gate publishing SHA-tagged images and a release manifest, single-VM staging deploy/rollback and backup/restore scripts, k6 load tests, and the operator documentation set. **Do not** touch any other section of `CLAUDE.md`. **Verify**: `grep -c 033-production-hardening CLAUDE.md` returns at least 1.

- [X] T130 Honour the rate-limit configuration switches that are currently dead per **FR-006, FR-043** (partial). `config.rate_limit_enabled` and `config.rate_limit_fail_mode` are parsed in `backend/crates/shared/config/src/lib.rs` (lines ~370 and ~394) but are read by **no** non-test code — `grep -rn rate_limit_enabled backend/crates` finds only struct literals. Consequences: `loadtest/widget-conversation.js` and `loadtest/README.md` both instruct the operator to set `APP_RATE_LIMIT_ENABLED=false` on the load-test target, which silently does nothing, so T088's capacity run would measure the limiter instead of the platform (FR-043). Two edits, both in `backend/crates/server/src/rate_limit.rs`:
  1. At the top of `login_rate_limit` and `per_ip_creation_limit`, return `next.run(request).await` immediately when `!config.rate_limit_enabled`. In `check_session_message_limit` and `check_global_tenant_limit`, return `true` immediately in the same case — these take `&dyn RateLimiter` rather than config today, so add an `enabled: bool` parameter and pass `state.config.rate_limit_enabled` from each call site in `backend/crates/modules/{widgets,feedback,whatsapp,integrations}/src/`.
  2. Pass `config.rate_limit_fail_mode` into `RedisRateLimiter` (add it to `RedisRateLimiter::new`) and use it in `degrade()`: `"auth_closed"` (the default) keeps today's behaviour exactly — `Scope::Login` refuses, everything else falls back to the in-process store; `"open"` falls back to the in-process store for **all** scopes including login. Any other value must be treated as `"auth_closed"`. FR-007 requires a *documented* degradation behaviour, so also state both modes in the `APP_RATE_LIMIT_FAIL_MODE` entry of `docs/configuration.md`.

  **Do not** remove `rate_limit_max_override` — T120 gives it its consumer. **Verify**: `cd backend && cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings` exit 0; `cargo test -p cache && REQUIRE_DB_TESTS=1 cargo test -p server --test rate_limit_login` pass; and starting the server with `APP_RATE_LIMIT_ENABLED=false` lets more than 10 login attempts through in one minute.

- [X] T131 Delete the dead secret scan from `.github/workflows/frontend.yml` per **FR-038** (partial). The `Secret pattern scan` step ends in `|| true`, so it can never fail — FR-038 states the check "MUST NOT be advisory-only". T115 already added a real, non-advisory gitleaks job to `.github/workflows/backend.yml`, which discharges the requirement; the `frontend.yml` step is now a misleading leftover that looks like coverage and provides none. Remove the whole `- name: Secret pattern scan` step (three lines) from `frontend.yml`. **Do not** add `continue-on-error` anywhere, and **do not** touch the gitleaks job in `backend.yml`. **Verify**: `python3 -c "import yaml;yaml.safe_load(open('.github/workflows/frontend.yml'))"` succeeds and `grep -c 'Secret pattern scan' .github/workflows/frontend.yml` returns 0.

**Checkpoint**: anonymous callers get independent budgets, errors reach a grouped record,
per-tenant overrides take effect, refusals are auditable, the gate-verification workflow
produces a real verdict, the health dashboard loads, and the deploy scripts point at images
that exist.

---

## Phase 13: Convergence

**Why this phase exists**: a third codebase assessment found five gaps. Every task below is a
**defect in already-checked-off work**, not new scope. Nothing here adds a feature, an
endpoint, a permission, a table, or a page — rule 4 from the top of this file still applies.

**Read this before starting**: T132 and T133 mean **the backend test suite does not compile at
all**. `cargo nextest list --workspace` exits 101 today. Every claim in this file that rests on
a test passing — including this feature's own new suites (T016, T030, T031, T047) — has never
actually been verified. Do T132 first, then T133; nothing else can be verified until both are
done.

Do **not** re-do work already tracked by the still-unchecked tasks above (T032, T041, T042,
T061, T069–T074, T080–T083, T088–T093, T095, T096). In particular:
`specs/033-production-hardening/baseline-test-count.txt` still holds the wrong value (100) —
that is **T093's** job, not a task here. Phase 12 already said this; do not add a third
duplicate.

**Toolchain warning — read before running any `cargo fmt` or `cargo clippy`.** On the machine
where this phase was written, `rustup show active-toolchain` reported
`1.97.0 (directory override for '<repo>/backend')`. A rustup **directory override** silently
takes precedence over `backend/rust-toolchain.toml`, so the pinned 1.84.0 was **not** in
effect. Under 1.97.0, `cargo fmt --check` exits 1 and
`cargo clippy --workspace --all-targets -- -D warnings` exits 101 on files this feature never
touched (`ai-providers`, `billing` test helpers). **Those are toolchain skew, not defects, and
no task below asks you to fix them.** Before trusting any `fmt`/`clippy` result, run
`rustup show active-toolchain` in `backend/`; if it does not say 1.84.0, run
`rustup override unset` in `backend/` and re-check. **Never run `cargo fmt --all` to make a
skewed check pass** — T001 already forbids this.

- [X] T132 **CRITICAL** Exclude the `telegram` placeholder crate from the Cargo workspace per **FR-034, SC-008** (contradicts). **The problem**: `backend/Cargo.toml`'s `members` list uses the glob `"crates/modules/*"`, which picks up `backend/crates/modules/telegram/`. That directory contains **only** `Cargo.toml` and `tests/provisioner_tests.rs` — there is **no `src/` directory and no library target at all**. Its orphan test imports `telegram::api`, `telegram::model`, `telegram::provisioner`, and `integrations::provisioner`, none of which exist, so the workspace fails with `error[E0433]: cannot find module or crate 'telegram'`. Because `cargo nextest run --workspace` compiles every test target, **the CI gate built in Phase 4 can never pass**. Verify the shape yourself first: `find backend/crates/modules/telegram -type f` must print exactly those two files. Then make **one** edit to `backend/Cargo.toml`, adding an `exclude` key to the existing `[workspace]` table (keep `members` exactly as it is):
  ```toml
  [workspace]
  resolver = "2"
  members = [
    "crates/server", "crates/shared/*", "crates/modules/*", "crates/ai-providers"
  ]
  exclude = ["crates/modules/telegram"]
  ```
  `exclude` takes precedence over a glob in `members`, so the placeholder stays in the repository for whichever future spec implements the Telegram channel, but stops breaking the gate. **Do NOT delete `crates/modules/telegram/` or its test file** — that is a future feature's scaffolding, not garbage, and deleting it is not yours to decide. **Do NOT try to make the test compile** by writing `src/lib.rs`, an `api` module, a `provisioner`, or an `integrations::provisioner` — that is an entire unbuilt feature and is far outside this feature's scope (rule 4). Whoever implements the Telegram spec removes this `exclude` line. **This exact edit has been verified to work** — `exclude` does take precedence over the `crates/modules/*` glob; you should reproduce the check, not re-derive the approach. **Verify**: run
  ```bash
  cd backend && cargo metadata --format-version 1 --no-deps 2>/dev/null \
    | python3 -c "import json,sys; print([p['name'] for p in json.load(sys.stdin)['packages'] if p['name']=='telegram'])"
  ```
  It must print `[]` (an empty list) and `cargo metadata` must exit 0. Then `cargo nextest list --workspace > /tmp/list.txt 2>&1; echo "exit=$?"` must no longer report any `telegram` error — **it will still exit 101 on the `server` tests until T133, and that is expected**; confirm with `grep -c telegram /tmp/list.txt` returning 0.

- [X] T133 **CRITICAL** Restore the missing `AppConfig` fields in the broken server test files per **FR-034, SC-008** (contradicts). **Do T132 first** or you cannot compile to check your work. **The compiler is the authority on which files and how many — do not work from a count in this task, and do not stop because you have fixed some number of files. You are done when the verify command exits 0, not before.** **The problem**, and it is two separate botched mechanical passes, not one:
  1. **T004 added 13 new `AppConfig` fields and they were never propagated to the test files.** Many files under `backend/crates/server/tests/` build a `config::AppConfig { .. }` literal that is missing all 13, producing `error[E0063]: missing fields 'csp_policy', 'csp_report_only', 'hsts_max_age' and 10 other fields`.
  2. **T029's `trusted_proxy` line was inserted into the wrong block in most of those files.** A blind search-and-replace dropped `trusted_proxy: false,` outside the `AppConfig` literal — commonly inside the `match std::env::var("DATABASE_URL")` expression in the `get_pool()` helper, but **the landing spot differs from file to file**, so do not assume a fixed location. It produces `error: expected one of '@' or '|', found ':'` and ``error: `match` arm with no body``. One representative example — the stray line sits after `return None;` and its closing brace, immediately before the `};` that ends the `match`:
     ```rust
             eprintln!("skipping: DATABASE_URL not set");
             return None;
         }
             trusted_proxy: false,     // <-- WRONG BLOCK. Delete this line.
     };
     ```

  **The canonical template is `backend/crates/server/tests/cors.rs`, which compiles today and has the complete, correct field set.** Read its `config::AppConfig { .. }` literal (lines ~15–53) before editing anything, and copy values from it — do not invent values.

  For **each** failing file, make both edits:
  - **Delete the misplaced `trusted_proxy: ...` line — and identify it *only* by what the compiler points at.** The `error: expected one of '@' or '|', found ':'` message names the exact file and line; delete **that** line and no other. **Do not pattern-match on surrounding text to find it** — the misplacement landed in different helpers in different files (some are nowhere near a `get_pool` prologue), and a text heuristic will both miss real ones and delete correct ones. Files such as `cors.rs` already have `trusted_proxy` correctly placed inside the literal and merely mis-indented; the compiler does not flag those, so you must not touch them. Mis-indentation is harmless — `cargo fmt` is **not** part of this task (see the toolchain warning above).
  - **Add** the missing fields to the `AppConfig { .. }` literal, immediately before its closing `};`. The full block, matching `cors.rs`:
    ```rust
        rate_limit_enabled: true,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: true,
        csp_policy: "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".into(),
        csp_report_only: true,
        hsts_max_age: 31536000,
        otlp_endpoint: None,
        trace_sample_ratio: 1.0,
        sentry_dsn: None,
        release_sha: None,
        trusted_proxy: false,
    ```
    **Add only the fields a given file is actually missing** — several files already have `docs_enabled` and a few already have some others; a duplicate field is a compile error. Let the compiler drive it: the `E0063` message names every missing field for that file.

  **Work one file at a time and let the compiler tell you which is next**: `cd backend && cargo check -p server --all-targets --keep-going --message-format=short 2>&1 | grep "tests/"`. Re-run it after each file; the list shrinks. **Do not write a blanket `sed` across the test directory** — an unanchored substitution is exactly what caused defect 2 above, and you would be repeating the bug you are here to fix.

  **Do NOT change any test assertion, any `AppState` literal, or any value in `cors.rs`.** `budget_resolver` is already present and correct in all 67 files (T120's pass was done properly) — leave it alone. **Verify**: `cd backend && cargo nextest list --workspace > /tmp/list.txt 2>&1; echo "exit=$?"` exits **0**, and `grep -c "expected one of" /tmp/list.txt` returns 0. Then, with `docker compose -f infra/docker-compose.yml up -d` running and `sqlx migrate run` applied, `REQUIRE_DB_TESTS=1 cargo nextest run --workspace > /tmp/run.txt 2>&1; echo "exit=$?"`. **Report the `Summary [ ... ] N tests run` number in your completion message** — T093 needs it, but do not write it into `baseline-test-count.txt` yourself; that is T093's job.

- [X] T134 **CRITICAL** Give the WhatsApp webhook router the rate-limiter extension it requires per **FR-002** (contradicts). **This is a 033 defect introduced by T021 that breaks the already-shipped spec 029 channel** — 029's code was correct before T021 changed the handler's extractors. **The fix belongs entirely in this feature's own wiring (`backend/crates/server/src/router.rs`); you are not being asked to redesign or re-scope anything 029 built.** **The problem**: `backend/crates/server/src/rate_limit.rs`'s limiter Arc is built once in `router.rs` and handed out at three places — `:1050` `widget_router` gets `rate_store.clone()`, `:1052` `hooks_router` gets `rate_store` (**moved**), `:1088` the login route gets `login_store`. The WhatsApp public router, merged separately at `router.rs:1194–1198`, is layered with **only** `Extension(mk.clone())` and `DefaultBodyLimit` — it never receives the limiter. But `backend/crates/modules/whatsapp/src/webhook.rs:119` declares `Extension(store): Extension<Arc<dyn RateLimiter>>`. A missing extension fails during **extraction**, so **every** `POST /integrations/whatsapp/webhook/{token}` returns **500** and no inbound WhatsApp message is ever processed. FR-002's webhook budget is simultaneously unenforced. Confirm before editing: `grep -n "Extension(" backend/crates/server/src/router.rs | grep -iE "store|limiter"` returns exactly the three lines above, none of them near 1194.

  **The fix**: take one more clone of the Arc *before* it is moved at `:1052` — put it next to `let login_store = rate_store.clone();` at `:1048`, e.g. `let whatsapp_store = rate_store.clone();` — and add `.layer(Extension(whatsapp_store))` to the `whatsapp_webhook` router built at `:1194`, alongside the existing `.layer(Extension(mk.clone()))`.

  **CRITICAL — do NOT construct a second `RedisRateLimiter`.** It compiles, it passes, and it silently splits the budget in two, breaking **FR-003** and **SC-002** while every test still looks green. This is the exact trap T024 warns about; the whole point is that one Arc is shared by every limited surface.

  **The regression test must live in `backend/crates/server/tests/`, not in the `whatsapp` crate.** The bug is in `router.rs`; `backend/crates/modules/whatsapp/tests/intake_tests.rs:35–37` builds its **own** router and layers *both* extensions, which is precisely why this shipped broken. Add a test to a `server` integration test file that builds the real app and POSTs to `/integrations/whatsapp/webhook/some-token`.

  **Beware a vacuous test**: the route is only mounted inside `if let Some(ref mk) = master_key` at `:1191`, so if your test's `AppConfig` leaves `integration_secrets_key: None` the POST returns **404** and a naive `assert_ne!(status, 500)` passes while proving nothing. Set `integration_secrets_key` to the same base64 test key style used for `ai_key_encryption_key` in `cors.rs`, and assert the status is **neither 500 nor 404** (a 401/404-from-handler for an unknown token is fine — you are proving extraction succeeded, not that the token is valid). **Verify**: `cd backend && cargo build --workspace` exits 0 and your new test passes; prove it is not vacuous by temporarily reverting the `.layer(...)` addition and watching it go **red** with a 500.

- [X] T135 Return the standard rate-limit refusal from the WhatsApp webhook per **FR-005** (partial — also Constitution Principle V, which `plan.md` asserts as PASS with "429 responses reuse the existing `ApiError`/`ErrorEnvelope` shape with a `Retry-After` header"). **Do T134 first** — until the extension is wired, this branch is unreachable and you cannot test it. `backend/crates/modules/whatsapp/src/webhook.rs:150–155` hand-rolls its refusal:
  ```rust
  return (
      StatusCode::TOO_MANY_REQUESTS,
      Json(json!({"error":"rate_limited"})),
  )
      .into_response();
  ```
  This emits **no `Retry-After` header** (FR-005: a refusal "MUST tell the caller when retrying is permitted") and a body shape that matches no other endpoint on the platform. T028 added the `retry_after` parameter to `ApiError::rate_limited` and updated all callers; this one was missed. Replace it with the same call the other four limited surfaces use — see `backend/crates/modules/integrations/src/webhook.rs:231` for the pattern in the nearest-identical webhook context:
  ```rust
  return ApiError::rate_limited("Too many requests", retry_after).into_response();
  ```
  Derive `retry_after` from the `Decision::Refused { .. }` value the same way `integrations/src/webhook.rs` does — do not hard-code a constant. That means changing the `matches!(..., Decision::Refused { .. })` test into a `match` (or `if let`) so the refusal payload is in scope. **Do not add `X-RateLimit-*` headers** — T028 forbids them because they disclose the budget, which FR-005 prohibits. **Verify**: `cd backend && cargo build --workspace && cargo test -p whatsapp` exits 0, and the 429 response carries a `Retry-After` header.

- [X] T136 Document the test-harness environment variables per **FR-045, FR-046** (missing). Five variables the repository reads are documented in **no** file — `grep -rn "REQUIRE_DB_TESTS" README.md docs/` returns nothing. This matters more than a normal doc gap: **`REQUIRE_DB_TESTS` is the variable that decides whether database-backed tests run or silently self-skip.** A developer who follows the setup docs and runs `cargo test --workspace` without it gets a green result from a suite that skipped most of itself — which is the precise false-green failure this entire feature was created to eliminate (see T033 and `research.md` §9). FR-045 requires "zero undocumented steps". Note the SC-013 `comm` check in `quickstart.md` **passes** and will keep passing, because it only scans `config/src/lib.rs`; these five are read from test code, so that check structurally cannot catch them. Do not change the check.

  Add a short **"Running the test suite"** section to `docs/configuration.md`, under a clearly-labelled heading stating these are **test-harness only — not read by the running platform**, listing:
  - `REQUIRE_DB_TESTS` — set to `1` to make database- and Redis-backed tests **fail** instead of silently skipping when the dependency is unreachable. **Required for any run whose result you intend to trust**; the CI gate sets it. Default unset (tests self-skip).
  - `TEST_DATABASE_URL` / `TEST_REDIS_URL` — read by `crates/server/tests/live_deps.rs`; default unset, that suite skips.
  - `LIVE_AI_OPENAI_KEY` / `LIVE_AI_OPENAI_MODEL` — opt-in live-provider tests in `crates/server/tests/ai.rs`; default unset, skipped. Never commit a real key (FR-025).

  Also add one line to the test/verification instructions in `README.md` noting that `REQUIRE_DB_TESTS=1` is required for a trustworthy local run, pointing at the new section. **Do not** add these to `.env.staging.example` — they are not staging configuration. **Verify**: `grep -c REQUIRE_DB_TESTS docs/configuration.md README.md` returns at least 1 for each, and the `comm` check from the US6 section of `quickstart.md` still prints nothing.

**Checkpoint**: the workspace test suite compiles and can actually run for the first time, so
the Phase 4 gate has a real verdict; inbound WhatsApp deliveries are processed and
rate-limited instead of 500-ing; refusals carry the platform-standard envelope; and the
variable that governs whether tests really ran is written down.

---

## Phase 14: Convergence

**Why this phase exists**: a fourth codebase assessment found four gaps. Every task below is a
**defect in already-checked-off work**, not new scope. Nothing here adds a feature, an
endpoint, a permission, a table, or a page — rule 4 from the top of this file still applies.

**Read this before starting**: T137 and T138 mean **the CI gate's failure path does not work**.
The workflow that exists to prove the gate can fail (`gate-verification.yml`) never applies its
deliberate break, and when the real gate does fail it never prints which test failed. T139 means
two of this feature's own headline tests report **passed** when their dependencies are missing.
Do T137, T138, T139 in that order; T140 is independent and can be done any time.

Do **not** re-do work already tracked by the still-unchecked tasks above (T018, T032, T041,
T042, T061, T069–T074, T080–T083, T088–T093, T095, T096). In particular:

- `specs/033-production-hardening/baseline-test-count.txt` still holds the wrong value (100).
  That is **T093's** job. Phases 12 and 13 both already said this. **Do not add a fourth
  duplicate.**
- The one-hour soak run is **T089's** job. Do not add a soak task or a fourth k6 script.

**Verification note**: `cargo check --workspace --all-targets` exits 0 at the time this phase
was written, so T132/T133 are confirmed good. The test suite was **not** run (no Docker daemon
available), so T090/T092 remain the runtime gate. T137 and T138 are shell-behaviour defects
verified by reproducing the exact `bash -e` behaviour locally, and T137's no-op substitution was
verified with `perl -pe … | diff` against the current `health.rs` — not by watching a CI run.

**Shell background you need for T137 and T138.** GitHub Actions runs every `run:` block as
`bash --noprofile --norc -e -o pipefail {0}`. The `-e` matters: **a plain command that exits
non-zero terminates the whole step immediately.** So this shape is broken —

```bash
some_command > /tmp/out.txt 2>&1; EXIT=$?   # WRONG: on failure the step dies right here
cat /tmp/out.txt                            #        this line never runs
```

`set -e` is suppressed only when a command is part of an `&&`/`||` list, or is the condition of
`if`/`while`/`until`. So the correct shape is —

```bash
EXIT=0
some_command > /tmp/out.txt 2>&1 || EXIT=$?  # RIGHT: `||` suppresses -e, EXIT is assigned
cat /tmp/out.txt                             #        this line always runs
```

**Do not "simplify" `|| EXIT=$?` back to `; EXIT=$?` in either task** — that is the bug.

- [X] T137 **CRITICAL** Make `.github/workflows/gate-verification.yml` produce a real verdict per **FR-039, SC-008** (contradicts). This is the workflow whose entire purpose is to prove the gate rejects broken code, and it is broken in two independent ways, so it has never verified anything. **Both edits are required; either one alone still leaves the workflow meaningless.**

  **Defect 1 — the deliberate break is a no-op.** The "Apply deliberate break" step (line ~40) runs:
  ```bash
  perl -i -pe 's/assert_eq!\(.*true.*\)/assert_eq!(true, false)/' \
    crates/server/tests/health.rs || true
  ```
  **No assertion in `crates/server/tests/health.rs` contains the literal `true`** — every one compares a `StatusCode`, a `HealthStatus`, or a `CheckStatus`. The pattern therefore matches zero lines, the trailing `|| true` hides that, `health.rs` is left untouched, the gate runs clean and **passes**, and the next step prints `FAIL: gate passed despite deliberate break` and exits 1. So this workflow currently fails every single time while proving nothing about the gate. Confirmed with `perl -pe '<the pattern>' crates/server/tests/health.rs | diff crates/server/tests/health.rs -`, which reports **no difference**.

  Replace that whole step with this one, which uses a substitution **already verified to apply to 2 lines of the current `health.rs`** (lines 123 and 141, both `assert_eq!(response.status(), StatusCode::OK);`):
  ```yaml
        - name: Apply deliberate break
          run: |
            perl -i -pe 's/StatusCode::OK/StatusCode::IM_A_TEAPOT/' crates/server/tests/health.rs
            if git diff --quiet -- crates/server/tests/health.rs; then
              echo "FAIL: deliberate break was a no-op - health.rs unchanged, this workflow would prove nothing"
              exit 1
            fi
            git --no-pager diff -- crates/server/tests/health.rs
  ```
  Three things about this, all deliberate — do not change them:
  - `StatusCode::IM_A_TEAPOT` is a real `http::StatusCode` associated constant (HTTP 418), so the test still **compiles** and fails at the **assertion**. FR-039 wants the gate to reject broken code by failing a test, not by failing to build. Do not substitute a made-up constant.
  - **The `|| true` is gone.** A break that does not apply must be loud, which is the whole lesson of this defect.
  - The `git diff --quiet` guard is the self-check: it exits 0 when there is **no** change, so the `if` body fires exactly when the substitution was a no-op. Being inside an `if` condition, it is exempt from `set -e`. The job already sets `defaults: { run: { working-directory: backend } }`, so the relative path resolves correctly.

  **Defect 2 — the exit code is never captured, so the assert is vacuous.** The "Run gate (should fail)" step (line ~48) uses the broken `; EXIT=$?` shape described in the phase preamble above. When the gate fails — which is the *expected* outcome here — `bash -e` kills the step before `echo "exit_code=$EXIT" >> $GITHUB_OUTPUT` runs. The step's `continue-on-error: true` then lets the job continue with `steps.gate.outputs.exit_code` **empty**. The next step evaluates `[ "" -eq 0 ]`, which is a bash *error* (status 2), and because it sits in an `if` condition it is read simply as "false" — so the job prints `OK: gate correctly rejected broken code` **without ever having checked anything**. It would print `OK` just the same if the gate had died on a compile error, a missing service, or a Redis timeout.

  Replace the two steps at lines ~44–57 with:
  ```yaml
        - name: Run gate (should fail)
          id: gate
          continue-on-error: true
          run: |
            EXIT=0
            cargo nextest run --workspace > /tmp/gate-out.txt 2>&1 || EXIT=$?
            cat /tmp/gate-out.txt
            echo "exit_code=$EXIT" >> $GITHUB_OUTPUT

        - name: Assert gate failed
          run: |
            CODE="${{ steps.gate.outputs.exit_code }}"
            if [ -z "$CODE" ]; then
              echo "FAIL: gate step produced no exit_code output - cannot verify anything"
              exit 1
            fi
            if [ "$CODE" -eq 0 ]; then
              echo "FAIL: gate passed despite deliberate break"
              exit 1
            fi
            echo "OK: gate correctly rejected broken code (exit $CODE)"
  ```
  The `-z` guard is what stops this from ever silently reporting success again. **Keep `continue-on-error: true` on the gate step** — the gate is *supposed* to fail there, and without it the job would abort. **Do not touch** the `Revert break` step at the end, the `services:` block, or the toolchain step; T122 already fixed those and they are correct.

  **Verify**, all four:
  ```bash
  python3 -c "import yaml;yaml.safe_load(open('.github/workflows/gate-verification.yml'))"
  grep -c 'IM_A_TEAPOT' .github/workflows/gate-verification.yml    # must be 1
  grep -c '; EXIT=\$?' .github/workflows/gate-verification.yml     # must be 0
  cd backend && perl -pe 's/StatusCode::OK/StatusCode::IM_A_TEAPOT/' crates/server/tests/health.rs \
    | diff crates/server/tests/health.rs - > /dev/null; echo "diff-exit=$?"   # must print 1, meaning the break applies
  ```
  The last one is the important one: `diff-exit=1` means the substitution changes the file. If it prints `0`, the break is still a no-op and you have not fixed the defect — stop and report rather than checking the task off.

- [X] T138 **HIGH** Print the failing test in `.github/workflows/backend.yml` per **FR-034, SC-008** (contradicts). US5's acceptance scenario 2 requires that a deliberately failing test makes the gate "fail **and name the failing test**". It does not. The `Run tests and verify count floor` step (line ~47) uses the broken `; EXIT=$?` shape from the phase preamble:
  ```bash
          cargo nextest run --workspace > /tmp/nextest.out 2>&1; EXIT=$?
          cat /tmp/nextest.out
          test $EXIT -eq 0 || exit $EXIT
  ```
  Because all nextest output was redirected into `/tmp/nextest.out`, and `bash -e` terminates the step the instant nextest exits non-zero, **`cat /tmp/nextest.out` never runs** and the CI log contains no test output at all — just a failed step. A release manager reading that run cannot tell which test broke, or even whether a test broke rather than the runner dying. (T113 correctly fixed the *redirection order* here; this is the separate `set -e` defect that fix left behind.)

  Change **only** the first line of that `run:` block, so it becomes:
  ```bash
          EXIT=0
          cargo nextest run --workspace > /tmp/nextest.out 2>&1 || EXIT=$?
          cat /tmp/nextest.out
          test $EXIT -eq 0 || exit $EXIT
  ```
  **Change nothing else in the step** — the `COUNT=$(grep -oP ...)` parse, the `$GITHUB_OUTPUT` write, the `FLOOR` read from `../specs/033-production-hardening/baseline-test-count.txt`, and both echo lines all stay exactly as they are. `test $EXIT -eq 0 || exit $EXIT` still fails the job on a red suite; it just now does so *after* the output has been printed. Do **not** add `continue-on-error` anywhere in this workflow.

  **Verify**:
  ```bash
  python3 -c "import yaml;yaml.safe_load(open('.github/workflows/backend.yml'))"
  grep -c '|| EXIT=\$?' .github/workflows/backend.yml    # must be 1
  grep -c '; EXIT=\$?' .github/workflows/backend.yml     # must be 0
  ```

- [X] T139 Stop the two new rate-limit suites from silently self-skipping per **FR-034, SC-001, SC-002** (contradicts). FR-034 requires the gate to run "the complete test suite with **no category self-skipping**", and to "**fail if a category that should have run did not**". Two of this feature's own headline test suites violate that: neither reads `REQUIRE_DB_TESTS` at all, so when Postgres or Redis is unreachable they `return` early and nextest records them as **passed**. In CI — where `REQUIRE_DB_TESTS: "1"` is set — a misconfigured `REDIS_URL` or a service that failed its healthcheck would leave the gate **green** with SC-001 (sign-in throttle) and SC-002 (shared budget) never actually proven. The T036 executed-count floor cannot catch this either: an early-returning test still counts as a test that ran.

  **The correct pattern already exists in this repository** — copy its behaviour from `require_redis()` in `backend/crates/shared/cache/src/limiter.rs` (line ~362): when `REQUIRE_DB_TESTS != "1"` skip quietly, but when it **is** `"1"` and the dependency is unreachable, **panic**. Apply that to both files.

  1. In `backend/crates/server/tests/rate_limit_login.rs`, replace the two helpers at lines ~13–37 with:
     ```rust
     fn require_db_tests() -> bool {
         std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
     }

     async fn get_pool() -> Option<sqlx::PgPool> {
         let url = match std::env::var("DATABASE_URL") {
             Ok(v) => v,
             Err(_) => {
                 assert!(!require_db_tests(), "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
                 eprintln!("skipping rate_limit_login test: DATABASE_URL not set");
                 return None;
             }
         };
         let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
         if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
             assert!(
                 !require_db_tests(),
                 "REQUIRE_DB_TESTS=1 but the database is unreachable at {url}"
             );
             eprintln!("skipping rate_limit_login test: could not connect to DATABASE_URL");
             return None;
         }
         Some(pool)
     }

     async fn get_redis_url() -> Option<String> {
         let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
         let cache = match cache::Cache::new(&url) {
             Ok(c) => c,
             Err(e) => {
                 assert!(
                     !require_db_tests(),
                     "REQUIRE_DB_TESTS=1 but a Redis client could not be created for {url}: {e}"
                 );
                 return None;
             }
         };
         if cache.ping().await.is_err() {
             assert!(
                 !require_db_tests(),
                 "REQUIRE_DB_TESTS=1 but Redis is unreachable at {url}"
             );
             eprintln!("skipping rate_limit_login test: Redis unreachable at {url}");
             return None;
         }
         Some(url)
     }
     ```
     `assert!(!require_db_tests(), ...)` panics exactly when the variable is set and the dependency is missing, and is a no-op otherwise — that is the whole behaviour, in one line per site.
  2. In `backend/crates/server/tests/rate_limit_shared.rs`, replace `try_cache()` (line ~14) with:
     ```rust
     fn require_db_tests() -> bool {
         std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
     }

     async fn try_cache() -> Option<Cache> {
         let url = redis_url();
         let cache = match Cache::new(&url) {
             Ok(c) => c,
             Err(e) => {
                 assert!(
                     !require_db_tests(),
                     "REQUIRE_DB_TESTS=1 but a Redis client could not be created for {url}: {e}"
                 );
                 return None;
             }
         };
         let reachable = tokio::time::timeout(CONNECT_TIMEOUT, cache.ping())
             .await
             .map(|r| r.is_ok())
             .unwrap_or(false);
         if !reachable {
             assert!(
                 !require_db_tests(),
                 "REQUIRE_DB_TESTS=1 but Redis is unreachable at {url}"
             );
             eprintln!("Redis unreachable at {url} - skipping shared_budget_two_instances");
             return None;
         }
         Some(cache)
     }
     ```

  **Do NOT change any test assertion, any `AppConfig` or `AppState` literal, or the bodies of the test functions themselves** — only these three helpers. **Do NOT apply this to `backend/crates/server/tests/security_headers.rs`**: that suite needs neither Postgres nor Redis, so it has nothing to skip on and is correct as it stands.

  **Verify**: `cd backend && cargo check -p server --all-targets` exits 0. Then, **with Redis deliberately stopped**, `REQUIRE_DB_TESTS=1 cargo nextest run -p server --test rate_limit_shared` must **fail** with the panic message (this proves the fix is not vacuous); and with Redis running it must **pass**. Finally `grep -c REQUIRE_DB_TESTS crates/server/tests/rate_limit_login.rs crates/server/tests/rate_limit_shared.rs` must return at least 1 for each.

- [X] T140 Fix the impossible verification step in `loadtest/README.md` per **FR-043, FR-045** (contradicts). Under "Widget script note", the file tells the operator to confirm the load-test target has throttling disabled with:
  ```bash
  curl -s $K6_BASE_URL/api/v1/health | jq .rate_limit_enabled
  # Should print: false
  ```
  **`/health` returns only `{"status": "ok"}`** — see `liveness()` in `backend/crates/shared/observability/src/lib.rs`. That `jq` expression prints `null` forever, whatever the target's configuration is. So an operator "verifies" the precondition, sees output that is not the word `true`, proceeds, and T088's capacity run silently measures the rate limiter instead of the platform — the exact outcome the note exists to prevent.

  Replace those three lines with an empirical probe that needs no new endpoint. Use the **same session-creation URL that `loadtest/widget-conversation.js` itself posts to** (read it out of that file — do not invent a path), and exceed the 10-per-minute widget session budget:
  ```bash
  # Confirm rate limiting is really off: 15 rapid session creations, zero 429s.
  # (The widget session budget is 10 per minute, so a limited target WILL 429 here.)
  for i in $(seq 1 15); do
    curl -s -o /dev/null -w '%{http_code}\n' -X POST "$K6_BASE_URL/widget/v1/sessions" \
      -H 'content-type: application/json' -d '{}'
  done | sort | uniq -c
  # Expected: no 429 line. If 429 appears, APP_RATE_LIMIT_ENABLED is not false on the
  # target and the run would measure the limiter, not capacity (FR-043).
  ```
  **Do NOT add an endpoint, route, or field that exposes `rate_limit_enabled`** — rule 4 forbids new endpoints, and `/health`'s contract is explicitly unchanged by this feature (see the spec-032 boundary in `plan.md`). Change nothing else in `loadtest/README.md`; the targets table, the safety-guards section, and the results-recording section are all correct and are referenced by T107's alert thresholds.

  **Verify**: `grep -c 'rate_limit_enabled' loadtest/README.md` returns 0 for the `jq` form (the phrase may still appear in prose about the env var `APP_RATE_LIMIT_ENABLED`, which is fine and correct), and `grep -c 'jq .rate_limit_enabled' loadtest/README.md` returns 0.

**Checkpoint**: the gate-verification workflow applies a break that really breaks and reports a
verdict it really measured; a red suite names the test that failed; the sign-in-throttle and
shared-budget suites fail loudly instead of passing quietly when their dependencies are missing;
and the load-test precondition check actually checks something.

---

## Phase 15: Convergence

**Why this phase exists**: a fifth codebase assessment found ten gaps. Every task below is a
**defect in already-checked-off work**, not new scope. Nothing here adds a feature, an
endpoint, a permission, a table, or a page — rule 4 from the top of this file still applies.

**Read this before starting**: T141 and T142 mean **the staging deployment procedure cannot
work**. No observability service ever starts in staging, and the migration step of
`ops/deploy.sh` runs a command that does not exist. T143 and T144 mean the rate-limit
configuration and per-tenant override machinery built in Phases 12–13 is wired to nothing.
Do T141, T142, T143, T144 in that order; T145–T150 are independent of each other.

**These are fixes, not the blocked validations.** T070/T071/T073/T083/T088 (all still
unchecked, all `[BLOCKED: provisioning]`) would each *discover* these defects when finally
run — they do not fix them, and the fix has to exist before the validation can pass. Do not
delete a task below as a duplicate of a blocked validation task.

Do **not** re-do work already tracked by the still-unchecked tasks above (T018, T032, T041,
T042, T061, T069–T074, T080–T083, T088–T093, T095, T096). In particular:

- `specs/033-production-hardening/baseline-test-count.txt` still holds the wrong value (100).
  That is **T093's** job. Phases 12, 13, and 14 all already said this. **Do not add a fifth
  duplicate.**
- The one-hour soak run is **T089's** job. No task below adds a k6 script.

**Verification note**: `cargo check --workspace --all-targets` exits **0** at the time this
phase was written, and `caddy validate --config infra/Caddyfile` exits 0. Every YAML and JSON
file under `infra/` parses. **The Docker daemon was not running, so nothing below was verified
at runtime** — T090/T092 remain the runtime gate. Each task therefore gives you a *static*
verification you can always run, plus a clearly-labelled **[needs Docker]** check. Run the
static one; run the Docker one too if a daemon is available, and say which you ran.

- [X] T141 **CRITICAL** Start the observability stack in staging per **FR-018, FR-021, FR-014** (contradicts). **The problem**: all six observability services in `infra/docker-compose.yml` (`otel-collector`, `prometheus`, `grafana`, `loki`, `promtail`, `alertmanager`) sit behind `profiles: [observability]`, which is correct — T056/T106 put them there so they do not start during ordinary local development. But **nothing ever passes `--profile observability` in the deployment path**: `grep -n 'profile' ops/deploy.sh ops/rollback.sh docs/deployment.md docs/runbook.md` returns **nothing** today. So in staging — the only environment these were built for — Prometheus, Grafana, Loki, Promtail, Alertmanager and the OTLP collector are **never started**. FR-018's operator health view, FR-021's alert delivery, and FR-014's log/trace retrieval are all unreachable there. Three edits:
  1. In `ops/deploy.sh`, change the compose-files constant (line ~68) so every `docker compose` invocation in the script also selects the profile:
     ```bash
     readonly COMPOSE_FILES="--profile observability -f infra/docker-compose.yml -f infra/docker-compose.staging.yml"
     ```
     `--profile` is a **`docker compose` global flag and must come before `-f`**; putting it after the subcommand (`up --profile …`) is also valid but the constant is reused by `run`, `exec` and `ps` calls in this script, and `run --profile` is not accepted. Change **only** this line — every other use of `$COMPOSE_FILES` then inherits it.
  2. Make the same one-line change to the `COMPOSE_FILES` constant in `ops/rollback.sh`.
  3. In `docs/deployment.md`, add `--profile observability` to the `docker compose` commands in **step 4** ("Start infrastructure services") and **step 6** ("Start the full stack"), and add one sentence under "Bootstrap a Fresh Host" stating that omitting the profile leaves the platform running with no metrics, dashboards, logs, or alerting.

  **Do not** remove the `profiles:` keys from `infra/docker-compose.yml` — local development must keep starting without them, which is exactly what T056 was for. **Do not** add the profile to any command in `README.md` that describes local development.

  **Verify (static)**: `bash -n ops/deploy.sh && bash -n ops/rollback.sh` exit 0, and `grep -c 'profile observability' ops/deploy.sh ops/rollback.sh docs/deployment.md` returns at least 1 for each file. **Verify [needs Docker]**: `APP_RELEASE_SHA=abc1234 APP_DOMAIN=example.test docker compose --profile observability -f infra/docker-compose.yml -f infra/docker-compose.staging.yml config --services` lists `prometheus`, `grafana`, `loki`, `promtail`, `alertmanager`, and `otel-collector`.

- [X] T142 **CRITICAL** Give the backend image a real migration command per **FR-023** (missing). **The problem**: `ops/deploy.sh` step 4/6 (line ~206) and `docs/deployment.md` step 5 both run `docker compose … run --rm backend ./migrate`. **There is no `migrate` binary anywhere** — `backend/crates/server/` has no `src/bin/` directory and no `[[bin]]` section, and `infra/Dockerfile.backend`'s runtime stage copies only `/usr/local/bin/server` with `ENTRYPOINT ["server"]`. So `run --rm backend ./migrate` resolves to `server ./migrate`: the server ignores the unknown argument, **boots a full HTTP server, and never exits** — the deploy hangs at the migration step instead of failing cleanly, and FR-023's "apply pending schema changes before the new build accepts traffic" never happens. Two edits:
  1. In `backend/crates/server/src/main.rs`, at the **very top** of `main()` — after `AppConfig::from_env()` succeeds and *before* `init_observability` — add a migrate-and-exit branch:
     ```rust
     if std::env::args().nth(1).as_deref() == Some("migrate") {
         let pool = db::lazy_pool(
             &config.database_url,
             config.db_max_connections,
             Duration::from_millis(config.db_acquire_timeout_ms),
         );
         match db::run_migrations(&pool).await {
             Ok(()) => {
                 println!("migrations applied");
                 std::process::exit(0);
             }
             Err(e) => {
                 eprintln!("migration failed: {e}");
                 std::process::exit(1);
             }
         }
     }
     ```
     `db::run_migrations` already exists at `backend/crates/shared/db/src/lib.rs:24` and uses `sqlx::migrate!("../../../migrations")`, which **embeds the migration files into the binary at compile time**. That is why this needs **no** change to `infra/Dockerfile.backend` and no `COPY backend/migrations` in the runtime stage — do not add one. Exiting non-zero on failure is what lets `deploy.sh` abort with the old release still serving.
  2. Change `./migrate` to `migrate` (drop the `./`) in **both** `ops/deploy.sh` (the `docker compose … run --rm … backend ./migrate` line inside Step 4/6, and the matching `dry_run "…"` string just above it) and `docs/deployment.md` step 5. With `ENTRYPOINT ["server"]` the argument is appended to the entrypoint, so `migrate` becomes `server migrate` — a relative path `./migrate` would be looked up as a file and is wrong.

  **Do not** add a new crate, a new binary target, or a CLI-argument parsing library — one `if` on `args().nth(1)` is the whole change. **Do not** change `/health`, `/ready`, or any route.

  **Verify (static)**: `cd backend && cargo build --workspace` exits 0; `grep -c './migrate' ../ops/deploy.sh ../docs/deployment.md` returns **0** for both. **Verify [needs Docker + a database]**: with the local stack up, `cd backend && cargo run -p server -- migrate` prints `migrations applied` and **exits**, and `cargo run -p server` (no argument) still starts the server normally.

- [X] T143 Honour `APP_RATE_LIMIT_ENABLED` at the four module call sites per **FR-006, FR-043** (partial). T130 added the switch to `login_rate_limit` and `per_ip_creation_limit` in `backend/crates/server/src/rate_limit.rs`, and added an `enabled` parameter to `check_session_message_limit` / `check_global_tenant_limit` — but **those two functions have no callers** (see T144), so the switch reaches none of the four migrated module handlers. They call `store.check(...)` unconditionally. `loadtest/widget-conversation.js` and `loadtest/README.md` both tell the operator to set `APP_RATE_LIMIT_ENABLED=false` on the load-test target; it still does nothing for the widget path, so T088's capacity run would measure the limiter instead of the platform — the exact FR-043 failure T130 was written to prevent. Four handler edits plus one router edit:
  1. `backend/crates/modules/widgets/src/public_routes.rs`, `send_message` (line ~478): the handler already has `config` in its crate dependencies. Add the extractor `Extension(config): Extension<Arc<config::AppConfig>>` and wrap **both** existing `store.check(...)` blocks (the `Scope::WidgetMessage` one and the `Scope::TenantGlobal` one) in `if config.rate_limit_enabled { … }`.
  2. `backend/crates/modules/feedback/src/public_routes.rs`, `submit_feedback` (line ~38): **first add `config = { path = "../../shared/config" }` to `backend/crates/modules/feedback/Cargo.toml`** — that crate does not depend on `config` yet. Then make the same extractor + two `if` wrappers as step 1.
  3. `backend/crates/modules/integrations/src/webhook.rs`, `receive_webhook` (line ~155): it **already** takes `Extension(config): Extension<Arc<config::AppConfig>>`. Wrap **only** the per-connection request budget check — the one keyed `integration_conn:{}` at line ~203 — in `if config.rate_limit_enabled { … }`. **Leave the two `integration_evt:` checks exactly as they are.** Those are not request budgets; they throttle how often a *rejection event row* is written, and disabling them during a load test would flood the events table. Same rule in step 4.
  4. `backend/crates/modules/whatsapp/src/webhook.rs`, `receive_message` (line ~115): add `Extension(config): Extension<Arc<config::AppConfig>>` (the `whatsapp` crate already depends on `config`) and wrap **only** the `whatsapp_conn:{}` check at line ~137. **Leave the `whatsapp_evt:…invalid_signature` check alone**, for the reason in step 3.
  5. **`backend/crates/server/src/router.rs` line ~1196 — this is the step that breaks production if you skip it.** The WhatsApp public router is built separately from `api_routes`, and `Extension(state.config.clone())` is layered at line ~1092 on a **different** router. Adding the `config` extractor in step 4 without also layering it here makes extraction fail and **every** `POST /integrations/whatsapp/webhook/{token}` return **500** — the identical defect T134 was written to fix. Add `.layer(Extension(state.config.clone()))` to the `whatsapp_webhook` router alongside the existing `.layer(Extension(mk.clone()))` and `.layer(Extension(whatsapp_store))`.
  6. `backend/crates/modules/whatsapp/tests/intake_tests.rs` (line ~35) builds its **own** router and layers the extensions by hand; add the config extension there too or that suite starts returning 500. Build a `config::AppConfig` the way `backend/crates/server/tests/cors.rs` does — **copy its literal, do not invent field values.**

  **Do not** change any default budget value, and **do not** touch `login_rate_limit` or `per_ip_creation_limit`; T130 already did those correctly.

  **Verify (static)**: `cd backend && cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings` exit 0; `cargo test -p widgets && cargo test -p feedback && cargo test -p whatsapp && cargo test -p integrations` pass; and `grep -c rate_limit_enabled crates/modules/widgets/src/public_routes.rs crates/modules/feedback/src/public_routes.rs crates/modules/whatsapp/src/webhook.rs crates/modules/integrations/src/webhook.rs` returns at least 1 for each. **Verify [needs Docker]**: `REQUIRE_DB_TESTS=1 cargo nextest run -p server` still passes.

- [X] T144 Connect the per-tenant override and refusal-audit machinery to a real call site per **FR-006, FR-008** (missing). **Do T143 first.** **The problem**: `check_session_message_limit` and `check_global_tenant_limit` in `backend/crates/server/src/rate_limit.rs` are **dead code** — `grep -rn 'check_global_tenant_limit\|check_session_message_limit' backend/crates --include='*.rs'` finds only their definitions and two doc lines in `lib.rs`. T019–T022 migrated the four module handlers to call `store.check(...)` **inline with hard-coded budget literals** instead. Everything T120 and T121 attached to `check_global_tenant_limit` is therefore unreachable: `BudgetResolver` never resolves anything, `fetch_override` never runs, migration `0061`'s `tenant_rate_limit_overrides` table is read by nothing at runtime (FR-006's "adjustable per tenant without a code change" is false), and `rate_limit_audit::record_refusal` never writes a row (FR-008's durable audit half does not exist).

  **The module crates cannot call `server::rate_limit::*` — `server` depends on them, so that is an inverted dependency and will not compile.** Introduce a port in `kernel`, which all four modules already depend on, and implement it in `server`:

  1. In `backend/crates/shared/kernel/src/rate_limit.rs`, next to the existing `RateLimiter` trait, add:
     ```rust
     /// Tenant-scoped budget resolution and refusal recording (FR-006, FR-008).
     ///
     /// Implemented in the composition root, which owns the database pool and
     /// the audit writer. Modules depend only on this trait.
     #[async_trait::async_trait]
     pub trait TenantRateLimit: Send + Sync {
         /// Effective budget for this tenant at this scope, or `None` when
         /// rate limiting is disabled for the environment.
         async fn budget(&self, tenant_id: &str, scope: Scope) -> Option<Budget>;
         /// Record a tenant-attributable refusal (FR-008 durable audit half).
         async fn record_refusal(&self, tenant_id: &str);
     }
     ```
     Export it from `backend/crates/shared/kernel/src/lib.rs` alongside the existing `RateLimiter` re-export.
  2. In `backend/crates/server/src/rate_limit.rs`, add the implementation and **delete the two dead functions** `check_session_message_limit` and `check_global_tenant_limit` (keep `fetch_override` — this is its new consumer; keep the four `#[cfg(test)]` unit tests untouched):
     ```rust
     pub struct ServerTenantRateLimit {
         pub pool: sqlx::PgPool,
         pub resolver: Arc<cache::limiter::BudgetResolver>,
         pub enabled: bool,
     }

     #[async_trait::async_trait]
     impl kernel::TenantRateLimit for ServerTenantRateLimit {
         async fn budget(&self, tenant_id: &str, scope: Scope) -> Option<Budget> {
             if !self.enabled {
                 return None;
             }
             Some(
                 self.resolver
                     .resolve(tenant_id, scope, |t, s| fetch_override(&self.pool, t, s))
                     .await,
             )
         }

         async fn record_refusal(&self, tenant_id: &str) {
             if let Ok(id) = uuid::Uuid::parse_str(tenant_id) {
                 crate::rate_limit_audit::record_refusal(&self.pool, id).await;
             }
         }
     }
     ```
     Add `async-trait` to `backend/crates/server/Cargo.toml` only if it is not already there.
  3. In `backend/crates/server/src/router.rs`, in `api_routes`, next to `let login_store = rate_store.clone();` (line ~1048), construct the port and layer it on **`widget_router` only**:
     ```rust
     let tenant_limit: Arc<dyn kernel::TenantRateLimit> = Arc::new(crate::rate_limit::ServerTenantRateLimit {
         pool: state.db.clone(),
         resolver: state.budget_resolver.clone(),
         enabled: state.config.rate_limit_enabled,
     });
     ```
     then add `.layer(Extension(tenant_limit))` to the `widget_router` chain at line ~1050. `widget_routes()` (line ~101) mounts **both** `widgets::public_routes::send_message` and `feedback::public_routes::submit_feedback`, so one layer covers both handlers. **Do not** layer it on `hooks_router` or the WhatsApp router — those key on a connection ID under `Scope::Webhook`, not on a tenant, and have no override row to read.
  4. In `backend/crates/modules/widgets/src/public_routes.rs` (`send_message`) and `backend/crates/modules/feedback/src/public_routes.rs` (`submit_feedback`), add the extractor `Extension(tenant_limit): Extension<Arc<dyn kernel::TenantRateLimit>>` and rewrite **only** the `Scope::TenantGlobal` block. Replace the `if config.rate_limit_enabled { … }` wrapper you added in T143 around *that* block — `budget()` returning `None` now carries the disabled case — and replace the hard-coded `Budget { max_requests: 600, … }` literal:
     ```rust
     let tid = session.tenant_id.to_string();
     if let Some(budget) = tenant_limit.budget(&tid, Scope::TenantGlobal).await {
         if let Decision::Refused { retry_after } = store.check(Scope::TenantGlobal, &tid, budget).await {
             tenant_limit.record_refusal(&tid).await;
             return ApiError::rate_limited("Too many requests", retry_after).into_response();
         }
     }
     ```
     Keep each handler's existing refusal message string. **Leave the `Scope::WidgetMessage` block exactly as T143 left it** — it is keyed on a session, not a tenant, and has no override row.

  **Do not** add an endpoint, route, permission, table, or migration. Override rows are inserted by an operator with SQL; FR-006 says "without a code change", not "with a UI". **Do not** change any test assertion.

  **Verify (static)**: `cd backend && cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings` exit 0; `cargo test -p server --test openapi_coverage && cargo test -p server --test auth && cargo test -p authz` pass **unchanged**; and `grep -rn 'check_global_tenant_limit' crates --include='*.rs'` returns **nothing**. **Verify [needs Docker]**: with the stack up, insert one row into `tenant_rate_limit_overrides` for a tenant with `scope='tenant_global'` and `max_requests=1`, then send two widget messages for that tenant — the second is refused, and `SELECT count(*) FROM audit_logs WHERE action='security.rate_limit.refused'` returns 1.

- [X] T145 Make Prometheus actually load the alert rules per **FR-021, FR-032, FR-015** (missing). **The problem**: `infra/prometheus.yml:5` declares `rule_files: ["prometheus-alerts.yml"]`, which Prometheus resolves relative to its own config file at `/etc/prometheus/`. But the `prometheus` service in `infra/docker-compose.yml` (line ~66) mounts **only** `./prometheus.yml:/etc/prometheus/prometheus.yml` — `infra/prometheus-alerts.yml` is never mounted, so the path matches nothing and Prometheus starts with **zero rules loaded**. Every rule T107 and T079 wrote (error rate, latency, dependency health, backup staleness) is inert, so FR-021 and FR-032 are unmet even once T141 starts the service. Two edits:
  1. In `infra/docker-compose.yml`, add one line to the `prometheus` service's `volumes:` list, immediately after the existing `./prometheus.yml` line:
     ```yaml
           - ./prometheus-alerts.yml:/etc/prometheus/prometheus-alerts.yml:ro
     ```
  2. In `infra/prometheus.yml`, add a **second** scrape job so the metrics endpoint is reachable in local development too. The existing `backend` job targets `backend:8080`, which only resolves when `infra/docker-compose.staging.yml` is also in play — in local development the backend runs on the host via `cargo run -p server`, so the observability profile currently scrapes nothing and FR-018's dashboards show "No data" locally. Append:
     ```yaml
       - job_name: 'backend-local'
         static_configs:
           - targets: ['host.docker.internal:8080']
         metrics_path: /metrics
     ```
     and add `extra_hosts: ["host.docker.internal:host-gateway"]` to the `prometheus` service in `infra/docker-compose.yml` so the name also resolves on Linux. **One of the two jobs will always be down** — `backend` in local development, `backend-local` in staging — and that is expected and harmless; add a one-line YAML comment above each job saying which environment it serves so nobody "fixes" it later.

  **Do not** change any rule expression or threshold in `infra/prometheus-alerts.yml` — T107 and T079 own that file, and `loadtest/README.md` is keyed to the same numbers.

  **Verify (static)**: `python3 -c "import yaml;yaml.safe_load(open('infra/prometheus.yml'))"` succeeds and `grep -c 'prometheus-alerts.yml' infra/docker-compose.yml` returns 1. **Verify [needs Docker]**: with `--profile observability` up, `curl -s localhost:9090/api/v1/rules | python3 -c "import json,sys;print(len(json.load(sys.stdin)['data']['groups']))"` prints **4**.

- [X] T146 Remove the `envsubst` dependency from the Alertmanager entrypoint per **FR-021** (contradicts). `infra/alertmanager-entrypoint.sh` renders the config template with `envsubst`. **`prom/alertmanager:v0.28.0` is built on busybox, whose applet set does not include `envsubst`** — so the entrypoint exits non-zero on `set -e`, the container never starts, and no alert is ever delivered. Busybox *does* ship `sed`, which is all this substitution needs (one variable, one occurrence). Replace the `envsubst` line in `infra/alertmanager-entrypoint.sh` with:
  ```sh
  sed "s|\${APP_ALERT_WEBHOOK_URL}|${APP_ALERT_WEBHOOK_URL}|g" \
    /etc/alertmanager/alertmanager.yml.tmpl > /tmp/alertmanager.yml
  ```
  Keep everything else in the script identical — the shebang, `set -e`, and the final `exec /bin/alertmanager "$@"`. Also add a guard immediately above it so a missing destination fails loudly instead of writing a config with an empty URL:
  ```sh
  if [ -z "${APP_ALERT_WEBHOOK_URL:-}" ]; then
    echo "APP_ALERT_WEBHOOK_URL is not set - alerts would go nowhere" >&2
    exit 1
  fi
  ```
  **Do not** reopen `webhook_configs[].url_file` — T125 already evaluated and rejected that route. **Do not** commit a real destination anywhere; FR-025 forbids secrets in version control, and `infra/alertmanager.yml.tmpl` must keep its `${APP_ALERT_WEBHOOK_URL}` placeholder unchanged. **Do not** change the `alertmanager` service definition in `infra/docker-compose.yml` — its mounts, entrypoint, and `--config.file=/tmp/alertmanager.yml` are already correct.

  **Verify (static)**: `sh -n infra/alertmanager-entrypoint.sh` exits 0 and `grep -c envsubst infra/alertmanager-entrypoint.sh` returns **0**. **Verify [needs Docker]**: with `APP_ALERT_WEBHOOK_URL=http://example.test/hook` exported and the observability profile up, `docker compose -f infra/docker-compose.yml --profile observability ps alertmanager` shows the container **running** (not restarting), and `curl -s localhost:9093/api/v2/status` returns the loaded config with `http://example.test/hook` in it.

- [X] T147 Give exported traces somewhere to land, and finish the log→trace link, per **FR-014, FR-016, FR-018, SC-006** (missing/contradicts). **The problem**, three parts of one broken chain:
  - `infra/otel-collector-config.yaml`'s only exporter is `otlp: endpoint: "jaeger:4317"`. **There is no `jaeger` service in `infra/docker-compose.yml`**, so every sampled trace — including the 100% of errored traces T053's tail-sampling policy carefully retains — is exported into a black hole. FR-016's "exportable to an external collector" and FR-014's "logs **and traces** ... retrievable together" have no trace half at all, and SC-006 is unreachable.
  - `infra/grafana/provisioning/datasources/datasources.yaml` declares **no trace datasource**, and its Loki `derivedFields` block is three malformed entries: two point at `prometheus` as if it were a trace store, one uses `datasourceUid: __expr__`, and all three use `matcherType: label` against `request_id` — which **T104 deliberately keeps out of the Loki label set** to avoid one stream per request. A label matcher on a non-label matches nothing.
  - `infra/grafana/dashboards/platform-overview.json` has four panels (request rate, error rate, latency, backlog) but **no dependency-health panel**, while FR-015 lists dependency health among the required measurements and FR-018 requires the view to present them.

  Four edits:
  1. **Add Tempo** to the `observability` profile in `infra/docker-compose.yml`. Tempo is the choice — single binary, filesystem storage, native Grafana datasource — matching the single-VM constraint in the spec's Assumptions the same way T106 chose single-binary Loki. **Pin the tag explicitly, never `latest`**, the way the neighbouring services are pinned:
     ```yaml
       tempo:
         profiles:
           - observability
         image: grafana/tempo:2.7.1
         command: ["-config.file=/etc/tempo/tempo-config.yaml"]
         volumes:
           - ./tempo-config.yaml:/etc/tempo/tempo-config.yaml:ro
           - tempo-data:/var/tempo
         ports:
           - "3200:3200"
     ```
     Add `tempo-data:` to the top-level `volumes:` map, and add `tempo` to the `grafana` service's `depends_on` list. Create `infra/tempo-config.yaml` with a `server.http_listen_port: 3200`, a `distributor.receivers.otlp.protocols.grpc` receiver on `0.0.0.0:4317`, and `storage.trace.backend: local` writing to `/var/tempo/blocks` with `wal.path: /var/tempo/wal`. **Do not configure an object-store backend** — same cost constraint T106 states for Loki.
  2. In `infra/otel-collector-config.yaml`, change the exporter endpoint from `"jaeger:4317"` to `"tempo:4317"`. Change nothing else — the receivers, the `tail_sampling` processor, and the pipeline are correct and T053 owns them.
  3. In `infra/grafana/provisioning/datasources/datasources.yaml`, **delete all three existing `derivedFields` entries** and replace them with exactly one, plus add a Tempo datasource:
     ```yaml
       - name: Tempo
         type: tempo
         access: proxy
         url: http://tempo:3200
         uid: tempo
         editable: false
     ```
     and on the Loki datasource:
     ```yaml
         derivedFields:
           - name: request_id
             matcherType: regex
             matcherRegex: '"request_id":"([^"]+)"'
             datasourceUid: tempo
             urlDisplayLabel: 'Traces for this request'
             url: '{ .request_id = "$${__value.raw}" }'
             internalLink: true
     ```
     Three things about this, all deliberate:
     - `matcherType: regex` over the **raw log line** is required because `request_id` is a parsed JSON field, not a label (T104). A label matcher — which is what the three entries you are deleting used — matches nothing.
     - The `url` is a **TraceQL query, not a trace ID**. `request_id` is a span *attribute* (confirmed: `backend/crates/shared/observability/src/trace.rs:47` records `request_id = %request_id` on the request span, so no change is needed there), and Tempo's plain trace-ID lookup would 404 on it. TraceQL `{ .request_id = "…" }` is the attribute search that actually resolves.
     - The doubled `$$` prevents compose variable interpolation from eating the `$` if this file is ever templated.
  4. In `infra/grafana/dashboards/platform-overview.json`, add a fifth panel titled `Dependency Health` with the single target expression `dependency_up`, in the same panel shape as the existing four (copy an existing panel object and change `title`, `id`, `gridPos.y`, and `targets[0].expr`). This is FR-015's dependency-health measurement on FR-018's view.

  **Do not** add an Angular page, route, nav entry, or permission — FR-018 resolved to Grafana dashboards, per the spec-032 boundary in `plan.md`, and T055 says the same.

  **Verify (static)**: all four parse —
  ```bash
  python3 -c "import yaml;yaml.safe_load(open('infra/tempo-config.yaml'))"
  python3 -c "import yaml;yaml.safe_load(open('infra/otel-collector-config.yaml'))"
  python3 -c "import yaml;yaml.safe_load(open('infra/grafana/provisioning/datasources/datasources.yaml'))"
  python3 -c "import json;d=json.load(open('infra/grafana/dashboards/platform-overview.json'));print(len(d['panels']))"
  ```
  the last printing **5**; and `grep -c jaeger infra/otel-collector-config.yaml` returns **0**. **Verify [needs Docker]**: with the profile up and `APP_OTLP_ENDPOINT=http://localhost:4317` set on a locally-run backend, issue one request, then `curl -s "localhost:3200/api/search?limit=1"` returns a trace. **Then re-walk T109's "investigate a reported error" entry in `docs/runbook.md` against the running stack** — paste a real request ID into the Loki query, click the derived field, confirm you land on the trace — and **correct the runbook where reality differs**. That walkthrough is the written procedure SC-006 times an operator against; it currently describes a link that has never resolved. If you cannot run Docker, say so and leave this task unchecked rather than checking it off on the static parses alone.

- [X] T148 Make the backup metric reach Prometheus per **FR-032, SC-011** (missing). `ops/backup.sh:149–156` writes `backup_last_success_timestamp_seconds` into `${NODE_EXPORTER_TEXTFILE_DIR:-/var/lib/node_exporter/textfile_collector}`, and T079's `BackupStale` rule alerts on it. **But there is no node-exporter service anywhere**, and `infra/prometheus.yml` scrapes only the backend — so the series never exists, `time() - backup_last_success_timestamp_seconds > 93600` evaluates over an empty vector, and the alert **can never fire**. A backup that silently stops producing is exactly the case FR-032 exists to catch, so the requirement is unmet. Three edits:
  1. Add to the `observability` profile in `infra/docker-compose.yml`, pinning the tag:
     ```yaml
       node-exporter:
         profiles:
           - observability
         image: prom/node-exporter:v1.9.0
         command:
           - "--collector.textfile.directory=/textfile"
         volumes:
           - /var/lib/node_exporter/textfile_collector:/textfile:ro
         ports:
           - "9100:9100"
     ```
     **Bind-mount the host path, do not create a named volume.** `/var/lib/node_exporter/textfile_collector` is the exact default `ops/backup.sh:72` already writes to, so the cron job and the exporter agree with no extra environment variable and no `docker volume inspect` archaeology. Do not add anything to the top-level `volumes:` map for this.
  2. Add a scrape job to `infra/prometheus.yml`:
     ```yaml
       - job_name: 'node-exporter'
         static_configs:
           - targets: ['node-exporter:9100']
     ```
  3. In `docs/backup-restore.md`, in the section that shows the metric file, add one line stating that the directory `/var/lib/node_exporter/textfile_collector` must exist on the host (`sudo mkdir -p` it during host bootstrap) and is bind-mounted read-only into the `node-exporter` container, which is what carries the metric to Prometheus. Add the same one-line note to the bootstrap steps in `docs/deployment.md`.

  **Do not** change `ops/backup.sh` — its metric name, format, and default directory are correct and T075 owns them. **Do not** change the `BackupStale` rule.

  **Verify (static)**: `python3 -c "import yaml;yaml.safe_load(open('infra/prometheus.yml'))"` succeeds, `grep -c node-exporter infra/docker-compose.yml infra/prometheus.yml` returns at least 1 for each, and `grep -c NODE_EXPORTER_TEXTFILE_DIR docs/configuration.md docs/backup-restore.md` returns at least 1 for each. **Verify [needs Docker]**: with the profile up, `curl -s localhost:9090/api/v1/targets | grep -c node-exporter` is at least 1.

- [X] T149 Serve the dashboard and widget through the staging edge per **FR-026, FR-012** (contradicts). **The problem**: `infra/docker-compose.staging.yml` runs `dashboard` and `widget` as separate nginx containers publishing host ports 4200 and 8082, but `infra/Caddyfile`'s staging site block **never reverse-proxies to either of them**. Instead it declares `root * /app/widget` + `file_server` and applies T017's widget framing exception to `/index.html` — a directory that **is not mounted into the `caddy` container at all**, so that path 404s and T017's FR-012 header exception is dead configuration. The only way to reach the dashboard or widget in staging is an unencrypted direct host port, which is not "production's service topology". Two edits:
  1. In `infra/Caddyfile`, inside the `https://{$APP_DOMAIN}` block **only** (leave the `http://localhost:8080` dev block untouched), delete the `root * /app/widget` and `file_server` lines and the `@widget_index` matcher/header pair, and replace them with proxies to the two services, keeping the widget's framing exception as a response-header rewrite on the proxied response:
     ```
         @widget {
             path /widget /widget/*
         }
         handle @widget {
             uri strip_prefix /widget
             reverse_proxy widget:80
             header {
                 -X-Frame-Options
                 Content-Security-Policy "frame-ancestors *"
             }
         }
     ```
     and change the final catch-all `handle { reverse_proxy backend:8080 … }` so the **dashboard** is what an ordinary browser request reaches, with the API paths still going to the backend:
     ```
         handle {
             reverse_proxy dashboard:80
         }
     ```
     The existing `@api`, `@ws`, and `@metrics` blocks already route those paths to `backend:8080` and **must not be changed** — T062 owns them, and `@metrics`'s loopback/private-network ACL is the Principle III remediation.
  2. In `infra/docker-compose.staging.yml`, add `dashboard` and `widget` to the `caddy` service's `depends_on` list so the proxy targets exist when Caddy starts. Keep every image SHA-pinned and keep the `${APP_RELEASE_SHA:?…}` guards.

  **Read the "Why `frame-ancestors *`" paragraph in `contracts/security-headers.md` before writing the widget header block** — a restrictive value there breaks the embeddable widget for every tenant, which is the entire point of FR-012.

  **Verify (static)**: `caddy validate --config infra/Caddyfile` exits 0; `grep -c '/app/widget' infra/Caddyfile` returns **1** (the dev block only); `APP_RELEASE_SHA=abc1234 APP_DOMAIN=example.test docker compose -f infra/docker-compose.yml -f infra/docker-compose.staging.yml config` exits 0 and its output contains no `:latest`. **Verify [needs a provisioned host]**: this is what **T070** and **T074** exercise — do not attempt them here.

- [X] T150 Stop the Sentry scrubber from emptying every event per **FR-017, FR-019** (contradicts). `contracts/metrics-exposition.md`, under "Scrubbing at capture", specifies what is **removed** *and* what is **retained**: "Retained: request ID, route template, status, duration, tenant UUID". `scrub_event` in `backend/crates/shared/observability/src/errors.rs` retains **none** of it — it ends with `event.message = None; event.logentry = None; event.extra.clear();`, which strips the error text and every structured field from the event. What reaches Sentry is a record with no message and no context, which cannot satisfy FR-017's "enough context to locate the code path" and gives the grouping algorithm nothing to group on. Two edits to that file:
  1. Replace the blanket `event.extra.clear()` with an **allowlist** filter that keeps exactly the contract's retained keys and drops everything else:
     ```rust
     const RETAINED_EXTRA: [&str; 5] = ["request_id", "route", "status", "duration_ms", "tenant_id"];
     event.extra.retain(|k, _| RETAINED_EXTRA.contains(&k.as_str()));
     ```
     Place this where `event.extra.clear()` currently is, and delete that line.
  2. Delete the `event.message = None;` and `event.logentry = None;` lines. The error text is the developer-authored `tracing::error!` message, not customer data — FR-019's "message content" means **conversation** message content, which never appears in a `tracing::error!` string. **Leave every other part of the scrubber exactly as it is**: the header redaction, `request.data = None`, the `user` field clearing, the breadcrumb clearing, and `is_sensitive_header` all stay. Those are what stop the actual PII and credential paths, and their tests pass.

  **Then add one test** to the existing `#[cfg(test)] mod tests` in the same file proving the fix is not a leak. Build a synthetic event that has: `extra` containing both `request_id` **and** a key named `password`; a `user` with `email: Some("user@example.com")`; and an `Authorization: Bearer secret123` header. Assert after scrubbing that `extra` still contains `request_id`, that `extra` no longer contains `password`, that `user.email` is `None`, and that the `Authorization` header reads `[REDACTED]`.

  **Do not** widen the allowlist beyond those five keys, and **do not** touch `is_sensitive_header`.

  **Verify (static)**: `cd backend && cargo build --workspace && cargo test -p observability` pass (all four scrubber tests, including your new one), and `grep -c 'extra.clear()' crates/shared/observability/src/errors.rs` returns **0**.

**Checkpoint**: the staging deploy starts its observability stack and can actually apply
migrations; `APP_RATE_LIMIT_ENABLED` reaches every limited surface so a load test measures the
platform; per-tenant overrides and refusal audit rows finally have a live call site;
Prometheus loads its rules and can see both the backend and the backup metric; Alertmanager
starts and delivers; traces land in a queryable store that a log line links to; the staging
edge serves all three applications; and a captured error carries enough context to act on.

---

## Phase 16: Convergence

**Why this phase exists**: a sixth codebase assessment found nine gaps. Every task below is a
**defect in already-checked-off work**, not new scope. Nothing here adds a feature, an
endpoint, a permission, a table, or a page — rule 4 from the top of this file still applies.

### ⚠️ READ THIS FIRST — five Phase 15 tasks are `[X]` but were never applied

**T143, T145, T146, T147, and T148 are all marked `[X]` above, and none of their file changes
exist on disk.** This was re-verified on 2026-07-28 by running each task's own `grep`:

```bash
cd /Users/mohamedramadan/customer-service-chat-system
grep -c rate_limit_enabled backend/crates/modules/widgets/src/public_routes.rs   # T143 claims >=1 -> actually 0
grep -c 'prometheus-alerts.yml' infra/docker-compose.yml                          # T145 claims 1  -> actually 0
grep -c envsubst infra/alertmanager-entrypoint.sh                                 # T146 claims 0  -> actually 1
grep -c jaeger infra/otel-collector-config.yaml                                   # T147 claims 0  -> actually 1
grep -c node-exporter infra/docker-compose.yml                                    # T148 claims >=1 -> actually 0
```

**Rule for this phase: the code is the authority, not the checkbox. Do NOT skip a task below
because an older task with the same content is checked off.** Every previous phase preamble
warned you against creating duplicates; this one warns you against dismissing real work as a
duplicate. If you think a task here is already done, run the `grep` printed inside it — that
grep, and only that grep, decides.

**Do not un-check T143/T145/T146/T147/T148 above.** This file is append-only; the tasks below
supersede them.

**Read this before starting**: T151 means **the backend gate cannot go green** — two WhatsApp
test files hand the handler an extension of the wrong type, so every webhook POST in them
returns 500 instead of 200. T152 and T153 mean **no alert can ever be delivered** — Alertmanager
never starts, and Prometheus loads zero rules. Do T151, T152, T153 in that order. T154–T159 are
independent of each other, except that **T157 must come after T151** (both edit
`whatsapp/tests/intake_tests.rs`).

Do **not** re-do work already tracked by the still-unchecked tasks above (T018, T032, T041,
T042, T061, T069–T074, T080–T083, T088–T093, T095, T096). In particular:

- `specs/033-production-hardening/baseline-test-count.txt` still holds the wrong value (100).
  That is **T093's** job. Phases 12, 13, 14, and 15 all already said this. **Do not add a sixth
  duplicate.**
- The one-hour soak run is **T089's** job. No task below adds a k6 script.
- Provisioning the staging host is **T069's** job. No task below provisions anything.

**Verification note**: `cargo check --workspace --all-targets` exits **0** at the time this
phase was written. The test suite was **not** run (no Docker daemon available), so **T090/T092
remain the runtime gate**. Every task below therefore gives you a **static** check you can
always run, plus a clearly-labelled **[needs Docker]** check where one applies. Run the static
one; run the Docker one too if a daemon is available, and say which you ran.

**Toolchain warning (unchanged from Phase 13)**: `rustup show active-toolchain` in `backend/`
currently reports `1.97.0 (directory override)`, not the pinned 1.84.0. Under 1.97.0,
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` fail on files this feature
never touched. **That is toolchain skew, not a defect. No task below asks you to fix it. Never
run `cargo fmt --all` to make a skewed check pass.**

---

- [X] T151 **CRITICAL** Give the WhatsApp test routers a correctly-typed rate-limiter extension per **FR-002, FR-034** (contradicts). **The problem**: T021 changed `receive_message` in `backend/crates/modules/whatsapp/src/webhook.rs` (line ~119) to extract `Extension(store): Extension<Arc<dyn RateLimiter>>`. The two test files that build their own router were never updated. Both do this:

  ```rust
  fn test_app(pool: sqlx::PgPool, mk: Arc<MasterKey>) -> Router {
      whatsapp::routes::public_router()
          .layer(axum::extract::Extension(mk))
          .layer(axum::extract::Extension(Arc::new(
              InMemoryRateLimitStore::new(),          // <-- Arc<InMemoryRateLimitStore>
          )))
          .with_state(pool)
  }
  ```

  `Arc<InMemoryRateLimitStore>` does **not** silently become `Arc<dyn RateLimiter>` here. Rust only performs that unsizing coercion at a coercion site — an annotated `let`, a typed function parameter, or a typed struct field. `.layer(Extension(x))` infers its type parameter *from* `x`, so there is no expected type and no coercion. This is exactly why `backend/crates/server/src/router.rs:1047` writes the annotation explicitly: `let rate_store: Arc<dyn RateLimiter> = Arc::new(RedisRateLimiter::new(...));`. Without it you get the concrete type.

  The consequence: the extension stored in the request is keyed on `Arc<InMemoryRateLimitStore>`, the handler asks for `Arc<dyn RateLimiter>`, extraction finds nothing, and **every POST to the webhook in these two files returns 500** while the tests assert `StatusCode::OK`. It compiles cleanly, so `cargo check` has never caught it. It has not been caught at runtime either, because these are DB-backed tests that skip locally without `DATABASE_URL` — but in CI, where `REQUIRE_DB_TESTS=1` and Postgres is up, they run, and **the Phase 4 gate can never pass.**

  Make the identical two-line edit in **both** files — `backend/crates/modules/whatsapp/tests/intake_tests.rs` (`test_app`, line ~34) and `backend/crates/modules/whatsapp/tests/sender_tests.rs` (`test_app`, line ~306). Bind the store to an annotated local first, then layer that local:

  ```rust
  fn test_app(pool: sqlx::PgPool, mk: Arc<MasterKey>) -> Router {
      let store: Arc<dyn kernel::RateLimiter> = Arc::new(InMemoryRateLimitStore::new());
      whatsapp::routes::public_router()
          .layer(axum::extract::Extension(mk))
          .layer(axum::extract::Extension(store))
          .with_state(pool)
  }
  ```

  Add `use kernel::RateLimiter;` to the imports if you prefer the shorter `Arc<dyn RateLimiter>` spelling; either is fine. **Keep `use kernel::InMemoryRateLimitStore;` — it is still the concrete type being constructed.**

  **Do NOT** change any test assertion, any helper other than `test_app`, or anything in `backend/crates/modules/whatsapp/src/`. The handler is correct; the test wiring is what is wrong.

  **Verify (static)**: `cd backend && cargo check --workspace --all-targets` exits 0, and
  ```bash
  cd backend && grep -c 'Arc<dyn kernel::RateLimiter>\|Arc<dyn RateLimiter>' \
    crates/modules/whatsapp/tests/intake_tests.rs crates/modules/whatsapp/tests/sender_tests.rs
  ```
  returns at least 1 for each file.

  **Prove it is not a no-op — this check needs no database and takes thirty seconds.** Before making the edit, temporarily add this one line inside `test_app` in `intake_tests.rs`:
  ```rust
  let _: () = Arc::new(InMemoryRateLimitStore::new());   // TEMPORARY - delete after reading the error
  ```
  Run `cd backend && cargo check -p whatsapp --all-targets`. It will fail, and **the error message names the inferred type**. If it says `expected \`()\`, found \`Arc<InMemoryRateLimitStore>\`` then no coercion happens and this task is real work — delete the temporary line and apply the fix. If it instead names `Arc<dyn RateLimiter>`, the coercion did happen after all: say so in your report, delete the temporary line, and **apply the annotation anyway** — it is correct and harmless either way. **Do not leave the temporary line in the file.**

  **Verify [needs Docker]**: with `docker compose -f infra/docker-compose.yml up -d` running and `cd backend && sqlx migrate run` applied, `REQUIRE_DB_TESTS=1 cargo nextest run -p whatsapp` passes — in particular `assert_eq!(resp.status(), StatusCode::OK)` at `intake_tests.rs:279`.

- [X] T152 **CRITICAL** Remove the `envsubst` dependency from the Alertmanager entrypoint per **FR-021** (missing — **T146 claims this is done; it is not**). Run `grep -c envsubst infra/alertmanager-entrypoint.sh` first: it returns **1**, so the file is unchanged and this task is real work.

  `infra/alertmanager-entrypoint.sh` currently reads:
  ```sh
  #!/bin/sh
  # Expand env vars in alertmanager config template, then start alertmanager.
  # Alertmanager does not expand ${VAR} natively (no url_file support in v0.28.0).
  set -e
  envsubst < /etc/alertmanager/alertmanager.yml.tmpl > /tmp/alertmanager.yml
  exec /bin/alertmanager "$@"
  ```
  **`prom/alertmanager:v0.28.0` is built on busybox, whose applet set does not include `envsubst`.** The command fails, `set -e` kills the script, the container never starts, and no alert is ever delivered — so FR-021's "notify an on-call recipient" is unmet no matter how good the rules are. Busybox *does* ship `sed`, which is all this substitution needs (one variable, one occurrence).

  Rewrite the file to exactly this:
  ```sh
  #!/bin/sh
  # Expand env vars in alertmanager config template, then start alertmanager.
  # Alertmanager does not expand ${VAR} natively (no url_file support in v0.28.0).
  # busybox (the base image) has no envsubst, so use sed.
  set -e
  if [ -z "${APP_ALERT_WEBHOOK_URL:-}" ]; then
    echo "APP_ALERT_WEBHOOK_URL is not set - alerts would go nowhere" >&2
    exit 1
  fi
  sed "s|\${APP_ALERT_WEBHOOK_URL}|${APP_ALERT_WEBHOOK_URL}|g" \
    /etc/alertmanager/alertmanager.yml.tmpl > /tmp/alertmanager.yml
  exec /bin/alertmanager "$@"
  ```
  The `|` delimiter on `sed` is deliberate — the substituted value is a URL containing `/`, which would terminate a `/`-delimited expression. The guard is deliberate too: a missing destination must fail loudly rather than write a config with an empty URL that silently drops every alert.

  **Do not** reopen `webhook_configs[].url_file` — T125 already evaluated and rejected that route. **Do not** commit a real destination anywhere; FR-025 forbids secrets in version control, so `infra/alertmanager.yml.tmpl` must keep its literal `${APP_ALERT_WEBHOOK_URL}` placeholder unchanged. **Do not** change the `alertmanager` service in `infra/docker-compose.yml` — its mounts, `entrypoint`, and `--config.file=/tmp/alertmanager.yml` are already correct.

  **Verify (static)**: `sh -n infra/alertmanager-entrypoint.sh` exits 0, `grep -c envsubst infra/alertmanager-entrypoint.sh` returns **0**, and `grep -c APP_ALERT_WEBHOOK_URL infra/alertmanager.yml.tmpl` still returns **1**. **Verify [needs Docker]**: with `APP_ALERT_WEBHOOK_URL=http://example.test/hook` exported and the observability profile up, `docker compose -f infra/docker-compose.yml --profile observability ps alertmanager` shows the container **running** (not restarting), and `curl -s localhost:9093/api/v2/status` returns a loaded config containing `http://example.test/hook`.

- [X] T153 **CRITICAL** Make Prometheus actually load its alert rules, and scrape a locally-run backend, per **FR-021, FR-032, FR-015** (missing — **T145 claims this is done; it is not**). Run `grep -c 'prometheus-alerts.yml' infra/docker-compose.yml` first: it returns **0**, so this is real work.

  **The problem**: `infra/prometheus.yml` line 5 declares `rule_files: ["prometheus-alerts.yml"]`, which Prometheus resolves relative to its own config file at `/etc/prometheus/`. But the `prometheus` service in `infra/docker-compose.yml` mounts **only** `./prometheus.yml:/etc/prometheus/prometheus.yml` — `infra/prometheus-alerts.yml` is never mounted into the container, so the path matches nothing and Prometheus starts with **zero rules loaded**. All four rule groups T107 and T079 wrote (`error_rate`, `latency`, `dependency_health`, `backup`) are inert, so FR-021 and FR-032 are unmet even with T141's `--profile observability` and T152's working Alertmanager. Two edits:

  1. In `infra/docker-compose.yml`, in the `prometheus` service, add one line to `volumes:` immediately after the existing `./prometheus.yml` line, and add an `extra_hosts:` key to the same service. The service becomes:
     ```yaml
       prometheus:
         profiles:
           - observability
         image: prom/prometheus:v3.2.1
         command:
           - "--config.file=/etc/prometheus/prometheus.yml"
           - "--storage.tsdb.path=/prometheus"
           - "--web.console.libraries=/usr/share/prometheus/console_libraries"
           - "--web.console.templates=/usr/share/prometheus/consoles"
         volumes:
           - ./prometheus.yml:/etc/prometheus/prometheus.yml
           - ./prometheus-alerts.yml:/etc/prometheus/prometheus-alerts.yml:ro
           - prometheus-data:/prometheus
         extra_hosts:
           - "host.docker.internal:host-gateway"
         ports:
           - "9090:9090"
     ```
     `extra_hosts` is what makes `host.docker.internal` resolve on Linux (it already resolves on macOS); edit 2 needs it.
  2. In `infra/prometheus.yml`, append a **second** scrape job. The existing `backend` job targets `backend:8080`, which only resolves when `infra/docker-compose.staging.yml` is also in play. In local development the backend runs on the host via `cargo run -p server`, so the observability profile currently scrapes nothing and FR-018's dashboards show "No data" locally. The `scrape_configs:` block becomes:
     ```yaml
     scrape_configs:
       # Staging: the backend runs as a compose service (docker-compose.staging.yml).
       - job_name: 'backend'
         static_configs:
           - targets: ['backend:8080']
         metrics_path: /metrics

       # Local development: the backend runs on the host via `cargo run -p server`.
       - job_name: 'backend-local'
         static_configs:
           - targets: ['host.docker.internal:8080']
         metrics_path: /metrics
     ```
     **One of the two jobs will always be down** — `backend` in local development, `backend-local` in staging. That is expected and harmless; the comments above each job are there so nobody "fixes" it later. Keep them.

  **Do not** change any rule expression or threshold in `infra/prometheus-alerts.yml` — T107 and T079 own that file, and `loadtest/README.md` is keyed to the same numbers. **Do not** remove the `profiles:` key.

  **Verify (static)**: `python3 -c "import yaml;yaml.safe_load(open('infra/prometheus.yml'))"` succeeds; `python3 -c "import yaml;yaml.safe_load(open('infra/docker-compose.yml'))"` succeeds; `grep -c 'prometheus-alerts.yml' infra/docker-compose.yml` returns **1**; `grep -c 'backend-local' infra/prometheus.yml` returns **1**. **Verify [needs Docker]**: with `--profile observability` up, `curl -s localhost:9090/api/v1/rules | python3 -c "import json,sys;print(len(json.load(sys.stdin)['data']['groups']))"` prints **4**.

- [X] T154 **HIGH** Make the backup metric reach Prometheus per **FR-032, SC-011** (missing — **T148 claims this is done; it is not**). Run `grep -c node-exporter infra/docker-compose.yml infra/prometheus.yml` first: it returns **0** for both, so this is real work. **Do T153 first** — this adds a scrape job to the same file T153 edits.

  **The problem**: `ops/backup.sh` line 72 writes `backup_last_success_timestamp_seconds` into `${NODE_EXPORTER_TEXTFILE_DIR:-/var/lib/node_exporter/textfile_collector}`, and T079's `BackupStale` rule (`infra/prometheus-alerts.yml`, group `backup`) alerts on `time() - backup_last_success_timestamp_seconds > 93600`. **But there is no node-exporter service anywhere**, and Prometheus scrapes only the backend — so that series never exists, the expression evaluates over an empty vector, and **the alert can never fire**. A backup that silently stops producing is exactly the case FR-032 exists to catch. Three edits:

  1. Add this service to `infra/docker-compose.yml`, inside the `observability` profile, next to the other observability services (pin the tag; never `latest`):
     ```yaml
       node-exporter:
         profiles:
           - observability
         image: prom/node-exporter:v1.9.0
         command:
           - "--collector.textfile.directory=/textfile"
         volumes:
           - /var/lib/node_exporter/textfile_collector:/textfile:ro
         ports:
           - "9100:9100"
     ```
     **Bind-mount the host path; do not create a named volume and do not add anything to the top-level `volumes:` map for this.** `/var/lib/node_exporter/textfile_collector` is the exact default `ops/backup.sh` already writes to, so the cron job and the exporter agree with no extra environment variable.
  2. Append a third scrape job to `infra/prometheus.yml`:
     ```yaml
       # Backup staleness metric (textfile collector, written by ops/backup.sh).
       - job_name: 'node-exporter'
         static_configs:
           - targets: ['node-exporter:9100']
     ```
  3. Documentation, in **all three** of these files (all three are currently missing it — `grep -c node-exporter docs/deployment.md docs/backup-restore.md docs/configuration.md` returns 0 for each):
     - `docs/backup-restore.md`, in the section that shows the metric file: one line stating that the directory `/var/lib/node_exporter/textfile_collector` must exist on the host and is bind-mounted read-only into the `node-exporter` container, which is what carries the metric to Prometheus.
     - `docs/deployment.md`, in "Bootstrap a Fresh Host": one line in the numbered steps telling the operator to run `sudo mkdir -p /var/lib/node_exporter/textfile_collector` before starting the stack.
     - `docs/configuration.md`, under the existing "deployment-time, not read by the backend" heading added by T126: an entry for `NODE_EXPORTER_TEXTFILE_DIR` — read by `ops/backup.sh`, default `/var/lib/node_exporter/textfile_collector`, must match the host path bind-mounted into the `node-exporter` service.

  **Do not** change `ops/backup.sh` — its metric name, format, and default directory are correct and T075 owns them. **Do not** change the `BackupStale` rule.

  **Verify (static)**: `python3 -c "import yaml;yaml.safe_load(open('infra/prometheus.yml'))"` and `python3 -c "import yaml;yaml.safe_load(open('infra/docker-compose.yml'))"` both succeed; `grep -c node-exporter infra/docker-compose.yml infra/prometheus.yml` returns at least 1 for each; `grep -c NODE_EXPORTER_TEXTFILE_DIR docs/configuration.md` returns at least 1; `grep -c node_exporter docs/backup-restore.md docs/deployment.md` returns at least 1 for each. **Verify [needs Docker]**: with the profile up, `curl -s localhost:9090/api/v1/targets | grep -c node-exporter` is at least 1.

- [X] T155 **HIGH** Give exported traces somewhere to land per **FR-014, FR-016, SC-006** (missing — **T147 claims this is done; it is not**). Run `grep -c jaeger infra/otel-collector-config.yaml` first: it returns **1**, so this is real work.

  **The problem**: `infra/otel-collector-config.yaml`'s only exporter is `otlp: endpoint: "jaeger:4317"`. **There is no `jaeger` service in `infra/docker-compose.yml`** and no other trace store anywhere. Every sampled trace — including the 100% of errored traces T053's tail-sampling policy carefully retains — is exported into a black hole. FR-016's "exportable to an external collector" has no collector, FR-014's "logs **and traces** retrievable together" has no trace half, and SC-006 is unreachable. Three edits:

  1. Create `infra/tempo-config.yaml`:
     ```yaml
     server:
       http_listen_port: 3200

     distributor:
       receivers:
         otlp:
           protocols:
             grpc:
               endpoint: 0.0.0.0:4317

     storage:
       trace:
         backend: local
         local:
           path: /var/tempo/blocks
         wal:
           path: /var/tempo/wal
     ```
     **Do not configure an object-store backend** — the target is one cost-constrained VM, the same constraint under which T106 chose single-binary Loki.
  2. Add this service to `infra/docker-compose.yml`, inside the `observability` profile (pin the tag; never `latest`):
     ```yaml
       tempo:
         profiles:
           - observability
         image: grafana/tempo:2.7.1
         command: ["-config.file=/etc/tempo/tempo-config.yaml"]
         volumes:
           - ./tempo-config.yaml:/etc/tempo/tempo-config.yaml:ro
           - tempo-data:/var/tempo
         ports:
           - "3200:3200"
     ```
     Add `tempo-data:` to the top-level `volumes:` map (which currently holds `prometheus-data`, `grafana-data`, `loki-data`), and add `tempo` to the `grafana` service's existing `depends_on` list.
  3. In `infra/otel-collector-config.yaml`, change the exporter endpoint from `"jaeger:4317"` to `"tempo:4317"`. **Change nothing else in that file** — the receivers, the `tail_sampling` processor, and the pipeline are correct and T053 owns them.

  **Verify (static)**:
  ```bash
  python3 -c "import yaml;yaml.safe_load(open('infra/tempo-config.yaml'))"
  python3 -c "import yaml;yaml.safe_load(open('infra/otel-collector-config.yaml'))"
  python3 -c "import yaml;yaml.safe_load(open('infra/docker-compose.yml'))"
  grep -c jaeger infra/otel-collector-config.yaml   # must be 0
  grep -c 'tempo-data' infra/docker-compose.yml     # must be 2 (service volume + top-level volumes map)
  ```
  **Verify [needs Docker]**: with `--profile observability` up and `APP_OTLP_ENDPOINT=http://localhost:4317` set on a locally-run backend, issue one request, then `curl -s "localhost:3200/api/search?limit=1"` returns a trace.

- [X] T156 **HIGH** Repair the Grafana Loki→trace link per **FR-014, SC-006** (contradicts — **T147 claims this is done; it is not**). **Do T155 first** — the Tempo datasource this adds must point at a service that exists.

  **The problem**: `infra/grafana/provisioning/datasources/datasources.yaml` declares **no trace datasource**, and its Loki `derivedFields` block is three malformed entries: two point at `prometheus` as if it were a trace store, one uses `datasourceUid: __expr__`, one has an empty `url: ""`, and **all three use `matcherType: label` against `request_id`** — which T104 deliberately keeps *out* of the Loki label set to avoid creating one stream per request. A label matcher on something that is not a label matches nothing, so the derived field never renders and the operator has no click-through from a log line to its trace. That click is what makes SC-006's 5-minute target reachable.

  **Delete all three existing `derivedFields` entries** and replace them with exactly one, and **add** a Tempo datasource. The whole file becomes:
  ```yaml
  apiVersion: 1

  datasources:
    - name: Prometheus
      type: prometheus
      access: proxy
      url: http://prometheus:9090
      isDefault: true
      editable: false

    - name: Tempo
      type: tempo
      access: proxy
      url: http://tempo:3200
      uid: tempo
      editable: false

    - name: Loki
      type: loki
      access: proxy
      url: http://loki:3100
      editable: false
      jsonData:
        derivedFields:
          - name: request_id
            matcherType: regex
            matcherRegex: '"request_id":"([^"]+)"'
            datasourceUid: tempo
            urlDisplayLabel: 'Traces for this request'
            url: '{ .request_id = "$${__value.raw}" }'
            internalLink: true
  ```
  Four things about this, all deliberate — do not change them:
  - `matcherType: regex` over the **raw log line** is required because `request_id` is a parsed JSON field, not a label (T104). A label matcher — what the three deleted entries used — matches nothing.
  - The `url` is a **TraceQL query, not a trace ID**. `request_id` is a span *attribute* (confirmed: `backend/crates/shared/observability/src/trace.rs` records `request_id = %request_id` on the request span, so **no change is needed there**), and Tempo's plain trace-ID lookup would 404 on it. TraceQL `{ .request_id = "…" }` is the attribute search that actually resolves.
  - The doubled `$$` prevents compose variable interpolation from eating the `$` if this file is ever templated.
  - `uid: tempo` on the Tempo datasource is what `datasourceUid: tempo` refers to. Both must be present and must match.

  **Verify (static)**:
  ```bash
  python3 -c "import yaml;yaml.safe_load(open('infra/grafana/provisioning/datasources/datasources.yaml'))"
  grep -c '__expr__' infra/grafana/provisioning/datasources/datasources.yaml   # must be 0
  grep -c 'matcherType: label' infra/grafana/provisioning/datasources/datasources.yaml  # must be 0
  grep -c 'type: tempo' infra/grafana/provisioning/datasources/datasources.yaml # must be 1
  ```
  **Verify [needs Docker]**: with the profile up, open Grafana at `localhost:3001`, run `{container="backend"} | json` in Explore, expand a log line, and confirm a **Traces for this request** link is present and resolves. **Then re-walk T109's "investigate a reported error" entry in `docs/runbook.md` against the running stack and correct the runbook wherever reality differs** — that walkthrough is the written procedure SC-006 times an operator against, and it currently describes a link that has never resolved. If you cannot run Docker, say so and leave this task unchecked rather than checking it off on the static parses alone.

- [X] T157 **HIGH** Honour `APP_RATE_LIMIT_ENABLED` at the four module call sites per **FR-006, FR-043** (missing — **T143 claims this is done; it is not**). Run `grep -c rate_limit_enabled backend/crates/modules/widgets/src/public_routes.rs` first: it returns **0**, so this is real work. **Do T151 first** — both tasks edit `backend/crates/modules/whatsapp/tests/intake_tests.rs`.

  **The problem, and note that it is narrower than T143 described**: T144 *did* land, and it routed the `Scope::TenantGlobal` blocks in `widgets` and `feedback` through `tenant_limit.budget(...)`, which already returns `None` when rate limiting is disabled. **Those two blocks are correct — leave them alone.** What remains unguarded is every *other* limiter check in the four module handlers: they call `store.check(...)` unconditionally, so `APP_RATE_LIMIT_ENABLED=false` does nothing for them. `loadtest/widget-conversation.js` and `loadtest/README.md` both tell the operator to set that variable on the load-test target, so T088's capacity run would measure the limiter instead of the platform — the exact FR-043 failure this exists to prevent. Six edits:

  1. `backend/crates/modules/widgets/src/public_routes.rs`, `send_message` (line ~478). The `widgets` crate already depends on `config`. Add the extractor `Extension(config): Extension<Arc<config::AppConfig>>` and wrap **only** the `Scope::WidgetMessage` block (the one with the comment `// Per-session message rate limit (10/min, ...)`) in `if config.rate_limit_enabled { … }`. **Leave the `Scope::TenantGlobal` block below it exactly as T144 wrote it.**
  2. `backend/crates/modules/feedback/src/public_routes.rs`, `submit_feedback` (line ~38). **First add `config = { path = "../../shared/config" }` to `backend/crates/modules/feedback/Cargo.toml`** — that crate does not depend on `config` yet (confirm with `grep -c config backend/crates/modules/feedback/Cargo.toml`). Then make the same extractor addition and wrap **only** the `Scope::WidgetMessage` block (comment `// Rate limit: per-session 10/60s`). **Leave the `Scope::TenantGlobal` block alone.**
  3. `backend/crates/modules/integrations/src/webhook.rs`, `receive_webhook` (line ~155). It **already** takes `Extension(config): Extension<Arc<config::AppConfig>>` — do not add it again. Wrap **only** the per-connection request budget check, the one keyed `integration_conn:{}` at line ~205, in `if config.rate_limit_enabled { … }`. **Leave the two `integration_evt:` checks (lines ~188 and ~219) exactly as they are.** Those are not request budgets; they throttle how often a *rejection event row* is written, and disabling them during a load test would flood the events table.
  4. `backend/crates/modules/whatsapp/src/webhook.rs`, `receive_message` (line ~115). The `whatsapp` crate already depends on `config`. Add `Extension(config): Extension<Arc<config::AppConfig>>` and wrap **only** the `whatsapp_conn:{}` check at line ~137. **Leave the `whatsapp_evt:…invalid_signature` check alone**, for the reason in step 3.
  5. **`backend/crates/server/src/router.rs` line ~1203 — this is the step that breaks production if you skip it.** The WhatsApp public router is built separately from `api_routes`, and `Extension(state.config.clone())` is layered on a **different** router. Adding the `config` extractor in step 4 without also layering it here makes extraction fail and **every** `POST /integrations/whatsapp/webhook/{token}` return **500** — the identical defect T134 was written to fix. Add `.layer(Extension(state.config.clone()))` to the `whatsapp_webhook` router alongside the existing `.layer(Extension(mk.clone()))` and `.layer(Extension(whatsapp_store))`.
  6. `backend/crates/modules/whatsapp/tests/intake_tests.rs` and `backend/crates/modules/whatsapp/tests/sender_tests.rs` both build their own router in `test_app` and layer the extensions by hand; add `.layer(axum::extract::Extension(Arc::new(test_config())))` to **both**, or those suites start returning 500. Build the `config::AppConfig` value the way `backend/crates/server/tests/cors.rs` does — **copy its literal; do not invent field values.** **No `Cargo.toml` change is needed here**: `config` is already a normal dependency of the `whatsapp` crate (`backend/crates/modules/whatsapp/Cargo.toml` line 12), and `[dependencies]` entries are reachable from `tests/`. Do not add a `[dev-dependencies]` entry for it.

  **Do not** change any default budget value, and **do not** touch `login_rate_limit` or `per_ip_creation_limit` in `backend/crates/server/src/rate_limit.rs`; T130 already guarded those two correctly (`grep -n rate_limit_enabled backend/crates/server/src/rate_limit.rs` shows them at lines ~44 and ~103).

  **Verify (static)**: `cd backend && cargo build --workspace` exits 0, and
  ```bash
  cd backend && grep -c rate_limit_enabled \
    crates/modules/widgets/src/public_routes.rs \
    crates/modules/feedback/src/public_routes.rs \
    crates/modules/whatsapp/src/webhook.rs \
    crates/modules/integrations/src/webhook.rs
  ```
  returns at least 1 for each. **Verify [needs Docker]**: `REQUIRE_DB_TESTS=1 cargo nextest run -p widgets -p feedback -p whatsapp -p integrations` passes, and `REQUIRE_DB_TESTS=1 cargo nextest run -p server` still passes.

- [X] T158 Add the dependency-health panel to the operator health view per **FR-015, FR-018** (missing — **T147 claims this is done; it is not**). `python3 -c "import json;print(len(json.load(open('infra/grafana/dashboards/platform-overview.json'))['panels']))"` prints **4** today: Request Rate, Error Rate, Latency Distribution, Outbox Backlog. FR-015 lists **dependency health** among the required measurements and FR-018 requires the view to present them, so the view is incomplete.

  In `infra/grafana/dashboards/platform-overview.json`, add a **fifth** panel titled `Dependency Health` with the single target expression `dependency_up`. Build it by copying the existing `Outbox Backlog` panel object verbatim and changing only four things: `title` to `Dependency Health`, `id` to the next unused integer, `gridPos.y` so it sits below the existing panels rather than overlapping them, and `targets[0].expr` to `dependency_up`. Keep the same `datasource`, panel `type`, and field options as the panel you copied.

  `dependency_up` is the gauge T048 populates from spec 032's existing health-check results — **do not add a new metric, a new probe, or a new query.** **Do not** add an Angular page, route, nav entry, or permission; FR-018 resolved to Grafana dashboards per the spec-032 boundary in `plan.md`, and T055 says the same.

  **Verify (static)**:
  ```bash
  python3 -c "import json;d=json.load(open('infra/grafana/dashboards/platform-overview.json'));print(len(d['panels']));print([p['title'] for p in d['panels']])"
  ```
  must print **5** and list `Dependency Health`; and
  ```bash
  python3 -c "import json;d=json.load(open('infra/grafana/dashboards/platform-overview.json'));ys=[(p['gridPos']['y'],p['gridPos']['h']) for p in d['panels']];print(ys)"
  ```
  must show no two panels sharing the same `y` with overlapping heights. **Verify [needs Docker]**: with the profile up, the dashboard renders five panels and the new one shows a value rather than "No data".

- [X] T159 Correct the documented fail-mode value per **FR-007** (contradicts). `docs/configuration.md` line ~71 documents `APP_RATE_LIMIT_FAIL_MODE` as taking `auth_closed` **or `pass_open`**. The code does not recognise `pass_open`: `backend/crates/shared/cache/src/limiter.rs` line ~84 reads

  ```rust
  Scope::Login if self.fail_mode != "open" => Decision::Refused { .. },
  ```

  so the open-mode value is the literal string **`open`**, and anything else — including `pass_open` — is treated as `auth_closed`. An operator who follows the documentation and sets `APP_RATE_LIMIT_FAIL_MODE=pass_open` silently gets fail-closed behaviour on sign-in during a Redis outage. FR-007 requires a *documented* degradation behaviour, and a document that names a value the code ignores does not satisfy it.

  **The documentation is the defect, not the code.** T130 specified these exact two values (`"auth_closed"` (the default) and `"open"`, with any other value treated as `auth_closed`) and the code implements that faithfully. Change **only** the `docs/configuration.md` table row, so its description reads:

  > Behaviour when Redis is unreachable for rate limiting: `auth_closed` (the default — sign-in attempts are refused, all other scopes fall back to a per-process in-memory counter) or `open` (all scopes including sign-in fall back to the per-process counter). Any unrecognised value is treated as `auth_closed`.

  **Do NOT** change `backend/crates/shared/cache/src/limiter.rs`, `backend/crates/shared/config/src/lib.rs`, or any test — `backend/crates/server/tests/*.rs` contain roughly 70 `rate_limit_fail_mode: "auth_closed".into()` literals that are all correct. **Do NOT** add validation that rejects unknown values; T130 explicitly requires unknown values to fall back to `auth_closed`, and FR-007 asks for documented behaviour, not input validation.

  **Verify (static)**: `grep -c 'pass_open' docs/configuration.md` returns **0**, and `grep -c '`open`' docs/configuration.md` returns at least 1. Also confirm you changed nothing else: `git diff --stat docs/configuration.md` shows a single file with a small line count, and `git diff --name-only` lists no `.rs` file for this task.

**Checkpoint**: the backend gate is no longer blocked by two mistyped test extensions;
Alertmanager starts and Prometheus loads all four rule groups, so an alert can reach a human;
the backup-staleness metric has an exporter and a scrape job; exported traces land in Tempo and
a Loki log line links to them; `APP_RATE_LIMIT_ENABLED` reaches every limited surface so a load
test measures the platform; the health view shows dependency health; and the documented
fail-mode value is one the code actually accepts.

---

## Phase 17: Convergence

**Why this phase exists**: a seventh codebase assessment found five gaps. Every task below is
a **defect in already-checked-off work**, not new scope. Nothing here adds a feature, an
endpoint, a permission, a table, or a page — rule 4 from the top of this file still applies.

### ⚠️ Phase 16 landed. These four tasks are NOT duplicates of anything above.

Unlike Phase 16, this phase found **no** stale `[X]`. Every task from Phases 11–16 was
re-verified against disk on 2026-07-28 by re-running its own `grep`, and **all of them
passed** — T110–T117, T131–T139, and T151–T159 are genuinely applied. So:

- **Do not** re-apply any task above. Nothing above needs redoing.
- **Do not** dismiss a task below as "already done above" — each one names a file and line
  that no earlier task edited. Two of them (T161, T162) are defects in code that an earlier
  task explicitly *asserted* was correct without checking it.

**Read this before starting**: **T160 means the backend gate cannot go green.** One unit test
in the cache crate is guaranteed to panic in CI. Do T160 first.

**Order after that**: T161 and T163 are independent of everything. **T162 must come before
T164** — they install the same content-security-policy string in two places (the `AppConfig`
default and the Caddy edge), and T164's verification asserts the two are byte-identical.

Do **not** re-do work already tracked by the still-unchecked tasks above (T018, T032, T041,
T042, T061, T069–T074, T080–T083, T088–T093, T095, T096). In particular:

- `specs/033-production-hardening/baseline-test-count.txt` still holds the wrong value (100).
  That is **T093's** job. Phases 12, 13, 14, 15, and 16 have all already said this. **Do not
  add a seventh duplicate.**
- Flipping `APP_CSP_REPORT_ONLY=false` after a staging soak is **T074's** job. T162 below
  fixes the *policy string's default value*, which is a different thing — see the note inside
  it.
- The one-hour soak run is **T089's** job. Provisioning the staging host is **T069's**. No
  task below does either.

**Verification note**: `cargo check --workspace --all-targets` exits **0** at the time this
phase was written, and **there is no Docker daemon available**, so the test suite was not run
and no container was started. **T090/T092 remain the runtime gate.** Every task below gives
you a **static** check you can always run, plus a clearly-labelled **[needs Docker]** check
where one applies. Run the static one; run the Docker one too if a daemon is available, and
say which you ran.

**Toolchain warning (unchanged from Phases 13 and 16)**: `rustup show active-toolchain` in
`backend/` reports `1.97.0 (directory override)`, not the pinned 1.84.0. Under 1.97.0,
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` fail on files this feature
never touched. **That is toolchain skew, not a defect. No task below asks you to fix it. Never
run `cargo fmt --all` to make a skewed check pass.**

---

- [X] T160 **CRITICAL** Give the `resets_after_window` Redis-limiter test a whole-second window per **FR-034, SC-008** (contradicts). **This test is guaranteed to fail in CI**, so the Phase 4 gate can never go green.

  **The problem**, in `backend/crates/shared/cache/src/limiter.rs`. The test at line ~415 builds this budget:

  ```rust
  let budget = Budget {
      max_requests: 3,
      window: Duration::from_millis(200),     // <-- sub-second
  };
  ```

  But `RedisRateLimiter::check` (line ~95) starts with:

  ```rust
  let window_secs = budget.window.as_secs();
  if window_secs == 0 {
      return Decision::Refused { retry_after: Duration::from_secs(1) };
  }
  ```

  `Duration::from_millis(200).as_secs()` is **0** — integer truncation. So the guard fires on
  the **very first** iteration of the test's `for i in 0..3` loop, `check` returns `Refused`,
  and the loop's `other => panic!("request {i} should be allowed, got {other:?}")` arm panics
  at `i == 0`. The test never reaches the assertion it was written to make.

  This has never been caught because `require_redis()` returns `None` and the test exits early
  unless `REQUIRE_DB_TESTS=1` **and** Redis is reachable — which is true in exactly one place:
  the CI `verify` job, after T112 added the Redis service. It compiles cleanly, so
  `cargo check` cannot see it.

  **Fix the TEST, not `check()`.** This matters — read it before editing:

  > Second granularity is **inherent to the Redis design**, not a bug. The key format is
  > `rl:{scope}:{identifier}:{window_epoch}` where `window_epoch = now / window_secs`
  > (`research.md` §1), and the TTL is `window_secs * 2`. Every budget the system can actually
  > produce is a whole number of seconds: `APP_RATE_LIMIT_LOGIN_WINDOW_SECS` is a `u64` of
  > seconds, and `tenant_rate_limit_overrides.window_seconds` is an `integer` with
  > `CHECK > 0`. **Nothing reachable in production can hit the zero-guard.** If you rewrite
  > `check()` to use millisecond arithmetic you will silently change the Redis key space and
  > the TTL for every existing counter, for no benefit.

  Make exactly these two edits inside the `resets_after_window` test — change nothing else in
  the file:

  1. `window: Duration::from_millis(200)` → `window: Duration::from_secs(1)`
  2. `tokio::time::sleep(Duration::from_millis(250)).await;` → `tokio::time::sleep(Duration::from_millis(1200)).await;`

  1200 ms, not 1000 ms: the window boundary is wall-clock aligned (`now / window_secs`), so
  sleeping exactly one window can land you back inside the same epoch. The extra 200 ms
  guarantees a new epoch. Keep `max_requests: 3` and every assertion exactly as they are.

  **Use 1 second, not a longer window** — this is deliberate and counter-intuitive. With
  `window_secs == 1`, `elapsed = now % 1` is always `0`, so the previous-window correction term
  `prev_count * (elapsed / window_secs)` is always zero and the post-sleep request is always
  Allowed. A 2-second window reintroduces a `prev_count * 0.5` weighting that can refuse the
  reset check and make the test flaky. If this test ever *does* flake under CI load (four Redis
  round-trips must finish inside one second), the fix is a **longer** window **plus** accounting
  for the correction term — never a shorter one, and never deleting the test.

  **This is a fourth sanctioned edit to an existing test**, beyond T014, T015, and T023 named
  in rule 3 at the top of this file. You are authorised to make it. Do **not** delete the
  test, do **not** add `#[ignore]`, and do **not** weaken any assertion — the test is checking
  a real behaviour (a counter resets when its window rolls over) and after this edit it will
  actually check it.

  **Note**: the near-identical `resets_after_window` test in
  `backend/crates/server/src/rate_limit.rs` (line ~226) uses `Duration::from_millis(50)` and is
  **correct** — it exercises `InMemoryRateLimitStore`, which measures with `Instant` and
  handles sub-second windows fine. **Do not touch it.**

  **Verify (static)**:
  ```bash
  cd backend && grep -c 'from_millis(200)' crates/shared/cache/src/limiter.rs   # must be 0
  cd backend && cargo check -p cache --all-targets                              # must exit 0
  ```
  **Verify [needs Docker]**: with `docker compose -f infra/docker-compose.yml up -d` running,
  `cd backend && REQUIRE_DB_TESTS=1 cargo nextest run -p cache` passes — and note in your
  report that all four Redis tests **ran** rather than returned early (nextest prints the
  count). If you have no Docker daemon, say so and leave this task unchecked rather than
  checking it off on the static parses alone.

- [X] T161 **HIGH** Stop dropping errored traces at the SDK, and stop dropping every non-errored trace at the collector, per **FR-020, FR-014, FR-016** (contradicts). **This is one defect with two halves; applying only one half still leaves FR-020 unmet, which is why it is one task.** T155 stated "the `tail_sampling` processor and the pipeline are correct and T053 owns them" — that was an assumption, and it was never checked.

  **FR-020 requires two things**: trace sampling adjustable per environment, **and** "a request that errored MUST always be retained regardless of the sampling rate". Neither holds today.

  **Half one — the SDK throws errored traces away before anything can retain them.**
  `backend/crates/shared/observability/src/otlp.rs:22` sets:
  ```rust
  .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(sample_ratio))))
  ```
  A head sampler decides at **span start**, before it is known whether the request will fail.
  With `APP_TRACE_SAMPLE_RATIO=0.1` (the production default recorded in `data-model.md`), 90%
  of errored traces are never exported at all, so the collector's tail sampler cannot retain
  what it never receives. `research.md` §5 says the tail rule retains errors "regardless of the
  head sampling decision" — that sentence is only true if the SDK exports everything.

  **Half two — the collector throws away every trace that did *not* error.**
  `infra/otel-collector-config.yaml:11–21` configures `tail_sampling` with exactly one policy,
  `status_code: [ERROR]`. A trace that no policy samples is **dropped**, so only errored
  traces survive. Consequences: `APP_TRACE_SAMPLE_RATIO` has no effect on what is stored
  (FR-020's adjustability is inert), and a **slow but successful** request has no trace at all
  — which is half of US3's Independent Test ("injecting a deliberate failure **and a
  deliberate slow path** … confirming **both** are findable end to end") and of SC-006.

  **The fix: the SDK exports everything, and the collector becomes the single place sampling
  is decided.**

  **Two trade-offs this makes deliberately — record them, do not "fix" them in a later pass:**
  - Exporting 100% of spans from the SDK reverses `research.md` §5's cost rationale ("Sampling
    everything at 1.0 in production — rejected on cost/throughput grounds") for the
    **application→collector** hop, on a single VM that also runs Postgres, Redis, and MinIO.
    This is accepted because FR-020's "always retained" **cannot** be satisfied at head time —
    the outcome is unknown when the sampling decision is made. Cost control moves one hop
    later, to the collector's probabilistic policy, which is where it can coexist with the error
    rule. The expensive hop (collector→Tempo→disk) is still sampled.
  - `config.trace_sample_ratio` becomes inert inside the backend. **This is not a dead-switch
    defect** of the kind T130 and T143 fixed — it is documented behaviour after edit 4 below,
    and the field is deliberately retained because removing it would redden ~70 `AppConfig`
    struct literals in `backend/crates/server/tests/`.

  Four edits:

  1. `backend/crates/shared/observability/src/otlp.rs` — replace the `.with_sampler(...)` call
     with `.with_sampler(Sampler::AlwaysOn)`, and rename the parameter to `_sample_ratio` so
     it stays in the signature without tripping the unused-variable warning:
     ```rust
     pub fn build_layer(
         endpoint: &str,
         // Retained for signature stability. Sampling is now decided by the
         // collector's tail_sampling processor: head sampling cannot satisfy
         // FR-020 because the decision is made before the request's outcome
         // is known. See infra/otel-collector-config.yaml.
         _sample_ratio: f64,
     ) -> OpenTelemetryLayer<Registry, SdkTracer> {
     ```
     and inside the builder:
     ```rust
         .with_sampler(Sampler::AlwaysOn)
     ```
     **Do not change the function signature's shape or its one caller** in
     `observability/src/lib.rs` (`otlp::build_layer(&endpoint, trace_sample_ratio)`), and **do
     not remove the `trace_sample_ratio` field from `AppConfig`** — roughly 70 test files in
     `backend/crates/server/tests/` carry full `AppConfig` struct literals, and removing a
     field reddens every one of them for no benefit.

  2. `infra/otel-collector-config.yaml` — add a **second** policy so non-errored traces are
     sampled at a configurable rate instead of being dropped, and raise the in-memory trace
     buffer. Replace the whole `tail_sampling:` block with:
     ```yaml
       tail_sampling:
         decision_wait: 10s
         num_traces: 5000
         expected_new_traces_per_sec: 200
         policies:
           # FR-020: an errored request's trace is ALWAYS retained, whatever the
           # sampling percentage below. Do not remove or reorder this policy.
           - name: error-status-policy
             type: status_code
             config:
               status_code_source: status
               status_codes:
                 - ERROR
           # FR-020: everything else is retained at a per-environment percentage.
           # Without this policy the processor drops every non-errored trace, so a
           # slow-but-successful request would have no trace at all.
           - name: probabilistic-policy
             type: probabilistic
             config:
               sampling_percentage: ${env:OTEL_TAIL_SAMPLE_PERCENTAGE}
     ```
     `num_traces` was **100**, which is smaller than SC-014's 200 concurrent virtual users —
     the buffer would evict traces during the T088 capacity run. `sampling_percentage` is a
     **percentage (0–100)**, not a 0.0–1.0 ratio; do not paste `APP_TRACE_SAMPLE_RATIO`'s value
     into it, which would turn `0.1` into one-tenth of one percent. **Leave the `receivers`,
     `exporters`, and `service` blocks exactly as they are** — T053 and T155 own them and the
     `tempo:4317` endpoint is correct.

  3. `infra/docker-compose.yml`, `otel-collector` service — supply that variable so the
     collector never sees it unset. Add an `environment:` key to the service (it has none
     today), immediately after the `command:` line:
     ```yaml
         environment:
           OTEL_TAIL_SAMPLE_PERCENTAGE: "${OTEL_TAIL_SAMPLE_PERCENTAGE:-100}"
     ```
     The shell-level `:-100` default is deliberate: it means the collector always receives a
     value, so you are not relying on the collector's own default-value syntax. Change nothing
     else about the service — its `profiles`, `image` pin, mount, and ports are correct.

  4. `docs/configuration.md` — two rows, because an operator turning a knob that no longer does
     anything is the failure this creates:
     - Update the existing `APP_TRACE_SAMPLE_RATIO` row's description to: "Read by the backend
       but **no longer applied at the SDK** — every span is now exported so that FR-020's
       'an errored request's trace is always retained' can hold. The retention rate for
       non-errored traces is set by `OTEL_TAIL_SAMPLE_PERCENTAGE` on the collector."
     - Add a new row to the **"Deployment-time variables (not read by the backend)"** table
       (line ~119): `OTEL_TAIL_SAMPLE_PERCENTAGE` — percentage (0–100) of non-errored traces
       the collector retains; errored traces are always retained regardless; default `100`;
       used by `infra/otel-collector-config.yaml` via `infra/docker-compose.yml`.

  **Verify (static)**:
  ```bash
  cd backend && cargo build --workspace                                          # exits 0
  grep -c 'AlwaysOn' backend/crates/shared/observability/src/otlp.rs             # 1
  grep -c 'TraceIdRatioBased' backend/crates/shared/observability/src/otlp.rs    # 0
  python3 -c "import yaml;d=yaml.safe_load(open('infra/otel-collector-config.yaml'));print(len(d['processors']['tail_sampling']['policies']))"   # 2
  python3 -c "import yaml;yaml.safe_load(open('infra/docker-compose.yml'))"      # parses
  grep -c 'OTEL_TAIL_SAMPLE_PERCENTAGE' infra/docker-compose.yml docs/configuration.md  # >=1 each
  ```
  **Verify [needs Docker] — run this if you possibly can, it is the only authoritative check
  on the collector config:**
  ```bash
  docker run --rm -v "$(pwd)/infra:/etc/otel" \
    otel/opentelemetry-collector-contrib:0.118.0 \
    validate --config=/etc/otel/otel-collector-config.yaml
  ```
  Collector configs unmarshal **strictly** — an unrecognised key makes the container refuse to
  start rather than warn. This command is how you find out. **One key to watch**:
  `status_code_source: status` was already in the file before this task and has **not** been
  validated against a running collector. If `validate` rejects it, delete just that one line
  (`status_codes: [ERROR]` alone is the documented minimum for the `status_code` policy) and
  say in your report that you did. **If you cannot run Docker, leave the key exactly as it is,
  do not guess, and say explicitly that the collector config was never validated.**

  Then, with the profile up and `APP_OTLP_ENDPOINT=http://localhost:4317` on a locally-run
  backend: issue one request that returns 200 and one that returns 500, and confirm
  `curl -s "localhost:3200/api/search?limit=10"` returns **both** traces.

  **If you cannot run `validate`, leave this task unchecked** and state plainly that the
  collector config was never validated against a collector. The static YAML parse is **not**
  sufficient evidence here: `yaml.safe_load` accepts any well-formed YAML, while the collector
  unmarshals strictly and refuses to start on an unrecognised key — so a green static check can
  sit on top of a container that never boots.

- [X] T162 **HIGH** Replace the default `APP_CSP_POLICY` with the policy the contract specifies per **FR-011** (partial). **Read the next paragraph first — this is not T074 and not a duplicate of T012.**

  > **How this differs from T074.** T074 soaks CSP in report-only mode on staging to discover
  > *unknown* violations, then flips `APP_CSP_REPORT_ONLY=false`. This task is different: the
  > shipped default policy already omits three concessions that
  > `contracts/security-headers.md` wrote down **in advance**, before any soak. T012 was
  > supposed to source the policy from that contract and did not. Fixing the string is a
  > prerequisite for T074 succeeding, not a substitute for it. **Do not flip
  > `APP_CSP_REPORT_ONLY` in this task.**

  **The problem**: `backend/crates/shared/config/src/lib.rs:386–387` defaults the policy to:
  ```text
  default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'
  ```
  `contracts/security-headers.md` (§"CSP rollout", "Base policy") specifies a policy with three
  directives this one collapses into `default-src 'self'`, each for a stated reason:
  - **`style-src 'self' 'unsafe-inline'`** — the contract calls Angular's runtime style
    injection "the known breakage risk" and grants `'unsafe-inline'` as "a deliberate,
    documented concession". Under `default-src 'self'` alone, style-src inherits `'self'` with
    no `'unsafe-inline'`.
  - **`img-src 'self' data: blob:`** — spec 029's attachment rendering and the widget use
    `data:`/`blob:` image URLs, which `default-src 'self'` blocks.
  - **`object-src 'none'`** — a hardening directive that `default-src 'self'` does not supply.

  **Be precise about why this matters, because the obvious reason is wrong.** `csp_policy` is
  consumed only by `scope_b_layers`, which puts the header on the **backend's JSON responses**.
  A CSP on a JSON response governs almost nothing — the same argument
  `contracts/security-headers.md` Scope C makes about `frame-ancestors` on JSON being a no-op
  applies to `style-src` and `img-src` too. Angular's runtime style injection is governed by the
  CSP on the dashboard **document**, which Caddy serves and **T164 below** is what fixes. So
  this task is *not* "the dashboard breaks when enforcing mode is turned on". It is narrower and
  still real: **the default is the wrong string**, it diverges from the base policy the contract
  specified in advance, and it is the string an operator will copy to the edge and to
  `APP_CSP_POLICY` in staging. Fixing the default here is what makes T164 and T074 land on a
  correct policy instead of propagating this one.

  Two edits, both mechanical:

  1. `backend/crates/shared/config/src/lib.rs`, line ~386 — change the `unwrap_or_else` default
     to the contract's base policy, as a single line:
     ```rust
     let csp_policy = env::var("APP_CSP_POLICY")
         .unwrap_or_else(|_| "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'".into());
     ```
     Keep `frame-ancestors 'none'` — this string is only ever used by `scope_b_layers` in
     `backend/crates/server/src/security_headers.rs`, which is Scope B (dashboard and API), and
     Scope B is where anti-framing belongs. The widget scope never receives this header, per
     T013. `'unsafe-inline'` goes on `style-src` **only**, never on `script-src`, which the
     contract states would "defeat the policy's purpose". The contract writes `connect-src
     'self' {api_origin} {otlp_origin}`; `'self'` is the correct *default* because staging
     serves the dashboard and API from one origin behind Caddy, and an operator with a
     split-origin deployment overrides the whole string via `APP_CSP_POLICY` (FR-013).

  2. `backend/crates/shared/config/src/lib.rs`, line ~624–627 — the defaults test asserts the
     old string. Update that one `assert_eq!` to the new string. **This is the only assertion
     in the workspace that pins the default**, so nothing else needs to change.

  **Do NOT** edit the ~70 `csp_policy: "…"` literals in `backend/crates/server/tests/*.rs`.
  Those files construct their own `AppConfig` values and are not asserting the default; they
  compile and pass unchanged. **Do NOT** edit
  `backend/crates/server/tests/security_headers.rs` — it sets its own policy string and asserts
  on the *header name* (report-only vs enforcing) and on `X-Frame-Options`, not on the policy's
  content. **Do NOT** touch `backend/crates/server/src/security_headers.rs`; the layer code is
  correct and this is purely a default-value defect.

  **Verify (static)**:
  ```bash
  cd backend && cargo build --workspace && cargo test -p config      # both exit 0
  grep -c "unsafe-inline" crates/shared/config/src/lib.rs            # 2 (default + test)
  grep -c "object-src 'none'" crates/shared/config/src/lib.rs        # 2
  ```
  **Verify [needs Docker]**: `cd backend && cargo test -p server --test security_headers`
  passes **unchanged** — if it does not, you edited something this task told you not to.

- [X] T163 Correct three rows in `docs/configuration.md` per **FR-046** and US6 acceptance scenario 4 (contradicts). US6/AC4 states that when behaviour changes, "the change is not considered complete until the documentation matches, so documentation cannot silently drift from reality." One row now describes a mechanism that was deliberately deleted, and two variables the repository reads are in no table.

  Three edits, all inside `docs/configuration.md`. **Change nothing outside this file.**

  1. **Line ~127 describes a tool that is no longer used.** The `APP_ALERT_WEBHOOK_URL` row in
     the "Deployment-time variables (not read by the backend)" table reads "Expanded by
     `envsubst` at container start (see T125)." **T152 removed `envsubst`** — the
     `prom/alertmanager:v0.28.0` base image is busybox and has no such applet, which is why
     `infra/alertmanager-entrypoint.sh` now uses `sed`. Confirm with
     `grep -c envsubst infra/alertmanager-entrypoint.sh` (returns **0**). Rewrite the
     description to: "Webhook URL Alertmanager sends alert notifications to. Substituted into
     `infra/alertmanager.yml.tmpl` by `sed` in `infra/alertmanager-entrypoint.sh` at container
     start (busybox has no `envsubst`). **The container refuses to start if this is unset**, so
     a missing destination fails loudly rather than silently dropping every alert." Drop the
     "(see T125)" reference — a task ID is not useful to an operator reading this file.

  2. **Add a row for `K6_BASE_URL`** to that same table: base URL the k6 load-test scripts
     target; **no default — the scripts exit 2 if it is unset** (FR-043, so a careless default
     can never point at production); used by `loadtest/dashboard-read.js`,
     `loadtest/widget-conversation.js`, `loadtest/webhook-ingest.js`.

  3. **Add a row for `LOADTEST_ALLOWED_HOSTS`** to that same table: comma-separated allowlist
     of hosts the load-test scripts may target; a `K6_BASE_URL` host outside it aborts the run
     (FR-043); no default; used by the same three scripts.

  Both variables are already documented in `loadtest/README.md` — that is not a substitute:
  FR-046 requires **one** consolidated reference, and `docs/configuration.md` is it.

  **Do NOT** change `infra/alertmanager-entrypoint.sh`, `infra/alertmanager.yml.tmpl`, or any
  `loadtest/` file — all three are correct, and this task is a documentation defect only.
  **Do NOT** re-document variables already present in the file; run
  `grep -c '<VARNAME>' docs/configuration.md` for each of the two additions first and skip any
  that already returns non-zero.

  **Verify (static)**:
  ```bash
  grep -c envsubst docs/configuration.md                                   # must be 0
  grep -c 'K6_BASE_URL' docs/configuration.md                              # >=1
  grep -c 'LOADTEST_ALLOWED_HOSTS' docs/configuration.md                   # >=1
  git diff --name-only | grep -c '\.rs$'                                   # 0 — this task touches no Rust
  ```
  (Do **not** expect `git diff --name-only` to list only this one file — the working tree
  already carries many unrelated modifications from earlier phases.)
  Then re-run the FR-046/SC-013 completeness check from the US6 section of `quickstart.md` and
  confirm it still prints nothing:
  ```bash
  grep -oE 'env::var\("[A-Z_]+"\)' backend/crates/shared/config/src/lib.rs \
    | grep -oE '"[A-Z_]+"' | tr -d '"' | sort -u > /tmp/code-vars.txt
  grep -oE '`[A-Z_]{4,}`' docs/configuration.md | tr -d '`' | sort -u > /tmp/doc-vars.txt
  comm -23 /tmp/code-vars.txt /tmp/doc-vars.txt
  ```

- [X] T164 **HIGH** Emit browser security headers on the dashboard and widget **documents** at the edge per **FR-009, FR-010, FR-011, SC-003** (missing). **Do T162 first** — it fixes the policy string this task installs. This is the surface where these headers actually do something, and no task in Phases 1–16 covers it.

  **The problem**: `infra/Caddyfile`'s final block is
  ```
      handle {
          reverse_proxy dashboard:80
      }
  ```
  It sets **no response headers at all**. The dashboard's `index.html`, JavaScript, and CSS are
  therefore served with no `X-Content-Type-Options`, no `Referrer-Policy`, no
  `X-Frame-Options`, and no `Content-Security-Policy`. The widget `handle @widget` block sets
  only the two framing-related headers (`-X-Frame-Options` and `frame-ancestors *`), so it is
  missing the same two Scope A headers.

  **Why the backend's middleware does not cover this.** T011–T013 wired `scope_a_layers` and
  `scope_b_layers` into the Axum router, so they land on the **backend's** responses — the
  JSON under `/api/*`. Caddy proxies the *documents* from separate nginx containers
  (`dashboard:80`, `widget:80`) that the Rust middleware never touches. And per
  `contracts/security-headers.md` Scope C, `X-Frame-Options` and `frame-ancestors` "only govern
  responses that are *documents being embedded*" — they "have **no effect on JSON**". So
  **FR-010 ("Dashboard responses MUST instruct browsers to refuse rendering the dashboard
  inside a frame owned by any other site") is currently unmet in the only place it can be met**:
  the dashboard document can be framed by any site today, and the `X-Frame-Options: DENY` on
  the backend's JSON does nothing to prevent it. Same for FR-011's content-source restriction,
  which only governs script/style loading if it is on the document. SC-003's "100% of responses
  carry the required browser protection directives" fails on every document response.

  Two edits to `infra/Caddyfile`, inside the **`https://{$APP_DOMAIN}`** site block only.
  **Do not touch the `http://localhost:8080` local-dev block** — T017 owns it.

  1. **Scope A, site-wide.** Add this as the **first** directive inside the
     `https://{$APP_DOMAIN} { … }` block, before the `tls` directive:
     ```
         header {
             X-Content-Type-Options "nosniff"
             Referrer-Policy "strict-origin-when-cross-origin"
         }
     ```
     These two are safe for **every** surface including the widget, which is why they go
     site-wide. **Do NOT add `Content-Security-Policy` or `X-Frame-Options` here** — a site-wide
     CSP or anti-framing header would override the widget block's `frame-ancestors *` and break
     the embeddable widget for every tenant. That is the single most likely regression in this
     task and the exact failure `contracts/security-headers.md` Scope C warns about
     ("Scope B must not leak onto the widget document").

     **Do not add `Strict-Transport-Security`** — Caddy emits it automatically on an HTTPS site
     with automatic certificates, and the contract already notes that a duplicate identical
     value is harmless but not needed here.

  2. **Scope B, dashboard document only.** Change the final catch-all `handle` block to add the
     two document-governing headers:
     ```
         handle {
             header {
                 X-Frame-Options "DENY"
                 Content-Security-Policy "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'"
             }
             reverse_proxy dashboard:80
         }
     ```
     This CSP string must be **byte-identical** to the new `APP_CSP_POLICY` default T162 writes
     into `backend/crates/shared/config/src/lib.rs`. If the two ever differ, the dashboard
     document and the API disagree about policy and one of them is wrong. `style-src
     'unsafe-inline'` is the concession Angular's runtime style injection needs; `img-src …
     data: blob:` is what spec 029's attachment rendering needs.

     This block is a **catch-all**: it is only reached for paths that did not match `@widget`,
     `@api`, `@ws`, or `@metrics`, so the widget is unaffected. Verify that ordering survives
     your edit.

  **Note on enforcing vs report-only.** The backend honours `APP_CSP_REPORT_ONLY` (default
  `true`); Caddy has no such switch, so the header above is **enforcing** from the start on the
  dashboard document. That is acceptable *only if* the policy is correct, which is why T162 comes
  first. **T074 is still the task that soaks staging and flips the backend's
  `APP_CSP_REPORT_ONLY=false`**; if T074's soak finds a violation, fix it in **both** places —
  this Caddyfile block and the `APP_CSP_POLICY` default.

  **Do NOT** add an Angular-side header, a `<meta http-equiv="Content-Security-Policy">` tag, or
  a new nginx config — the edge is the one place this belongs, and the frontend image is shared
  by both apps (T101), so a change there would hit the widget too.

  **Verify (static)**:
  ```bash
  caddy validate --config infra/Caddyfile                                  # exits 0 (caddy installed by T097)
  grep -c 'X-Content-Type-Options' infra/Caddyfile                         # 1
  grep -c 'Referrer-Policy' infra/Caddyfile                                # 1
  grep -c 'X-Frame-Options "DENY"' infra/Caddyfile                         # 1
  grep -c "frame-ancestors \*" infra/Caddyfile                             # still 2 — widget framing untouched
  ```
  Then prove the two CSP strings match exactly:
  ```bash
  grep -o "default-src[^\"]*" infra/Caddyfile | sort -u
  grep -o "default-src[^\"]*" backend/crates/shared/config/src/lib.rs | sort -u
  ```
  Both commands must print the **same single line**.

  **Verify [needs Docker]**: with the staging overlay up, `curl -sI https://<domain>/` shows
  `x-frame-options: DENY` and the CSP; `curl -sI https://<domain>/widget/` shows **no**
  `x-frame-options` and `content-security-policy: frame-ancestors *`; both show `nosniff` and
  the referrer policy. Then load the dashboard in a browser and confirm the console reports
  **zero** CSP violations — if Angular reports a blocked style or image, the policy string is
  wrong and both copies need the same correction. If you have no Docker daemon, run the static
  checks, say so, and leave this task unchecked rather than claiming the browser check passed.

**Checkpoint**: the cache crate's Redis limiter tests can actually pass in CI, so the backend
gate is no longer guaranteed red; an errored request's trace survives any sampling rate and a
successful request still has a trace to find; the content-security policy is the one the
contract specified and it is emitted on the dashboard **document**, where anti-framing and
content-source restriction actually take effect, without leaking onto the embeddable widget;
and the configuration reference no longer documents a tool that was removed.

---

## Phase 18: Convergence

**Why this phase exists**: an eighth codebase assessment found **two** gaps. Both are defects in
already-checked-off work (T001's toolchain pin), not new scope. Nothing here adds a feature, an
endpoint, a permission, a table, or a page — rule 4 from the top of this file still applies.

### ⚠️ Read this before you touch anything

**1. T160–T164 are DONE. Their checkboxes are stale — do not re-do them.** Every one of the five
Phase 17 tasks was re-verified against disk on 2026-07-28 by running its own `Verify (static)`
block, and **all five passed**:

| Task | Check run | Result |
|------|-----------|--------|
| T160 | `grep -c 'from_millis(200)' backend/crates/shared/cache/src/limiter.rs` | `0` ✅ |
| T161 | `otlp.rs` uses `Sampler::AlwaysOn`; collector has a `probabilistic-policy` | applied ✅ |
| T162 | `grep -c "unsafe-inline"` / `"object-src 'none'"` in `config/src/lib.rs` | `2` / `2` ✅ |
| T163 | `grep -c envsubst docs/configuration.md` = 0; `K6_BASE_URL`, `LOADTEST_ALLOWED_HOSTS` present | ✅ |
| T164 | Caddyfile has `X-Content-Type-Options`, `Referrer-Policy`, `X-Frame-Options "DENY"`; both CSP strings byte-identical | ✅ |

This phase is **append-only** and cannot tick those boxes for you. Tick them yourself without
re-applying any edit, or just leave them — but **do not redo the work.**

**2. The two tasks below supersede a standing instruction.** Rule 3 at the top of this file, and
T001, both say: *"If `cargo fmt --check` now fails on files you did not touch, STOP … do not run
`cargo fmt --all`."* Phases 13, 16, and 17 each repeated a "toolchain skew, not a defect, no task
asks you to fix it" warning on that basis.

**That diagnosis was wrong, and this phase proves it.** The reasoning was that a rustup directory
override to 1.97.0 was defeating the 1.84.0 pin, so the failures were version drift on untouched
files. The measurement says otherwise:

```
85 files fail `cargo fmt --check`
 0 of them are outside the set of files this feature modified
```

So `cargo fmt --all` reformats **only 033's own files**. T166 is authorised to run it. Read T165
first — the order matters.

**3. Both tasks are statically verifiable. No Docker, no staging, no CI run is needed.**

---

- [X] T165 **CRITICAL** Pin a Rust toolchain that can actually load the workspace, per **FR-036, FR-034, SC-008** (contradicts). **This is why the backend gate has never gone green. Do this before T166.**

  **The problem.** `backend/rust-toolchain.toml` (written by T001) contains:

  ```toml
  [toolchain]
  channel = "1.84.0"
  ```

  But `backend/crates/modules/authz/Cargo.toml` line 4 declares:

  ```toml
  edition = "2024"
  ```

  Edition 2024 requires Cargo **1.85 or newer**. Under the pinned 1.84.0, `cargo metadata` cannot
  even parse the workspace. Reproduce it yourself:

  ```bash
  cd backend && cargo +1.84.0 fmt --check
  ```

  ```text
  `cargo metadata` exited with an error: error: failed to load manifest for workspace member
  .../crates/server ... Caused by: feature `edition2024` is required
  The package requires the Cargo feature called `edition2024`, but that feature is not
  stabilized in this version of Cargo (1.84.0 (66221abde 2024-11-19)).
  ```

  **This kills the entire gate, not one step.** `.github/workflows/backend.yml` runs
  `rustup show active-toolchain || rustup toolchain install`, which honours `rust-toolchain.toml`
  and selects 1.84.0. Every command after it — `cargo fmt --check`, `cargo clippy`,
  `sqlx migrate run`, `cargo nextest run` — aborts with the error above before doing any work. So
  **FR-034 ("the gate MUST execute the complete test suite") and SC-008 are unmet on every push,
  and every green-looking claim about this gate is vacuous.** T160 said "the Phase 4 gate can
  never go green" because of one Redis test; this is an earlier and harder blocker sitting in
  front of it.

  **Why nobody caught it locally**: `rustup override list` reports
  `.../backend  1.97.0-aarch64-apple-darwin`. A rustup *directory override* outranks
  `rust-toolchain.toml`, so every local `cargo` command has silently been running 1.97.0 — the one
  configuration in which the workspace builds. CI has no such override.

  **Make exactly this one edit.** In `backend/rust-toolchain.toml`, change the channel:

  ```toml
  [toolchain]
  channel = "1.97.0"
  components = ["rustfmt", "clippy"]
  profile = "minimal"
  ```

  Keep `components` and `profile` exactly as they are. 1.97.0 is not an arbitrary pick — it is the
  version this workspace has actually been building and testing under all along, and it was
  verified during this assessment to load the workspace, and to pass
  `cargo clippy --workspace -- -D warnings` with **exit 0**.

  > **Do NOT "fix" this by changing `authz` to edition 2021 instead.** It is tempting — `authz` is
  > the *only* crate with a literal edition (28 others use `edition.workspace = true`, i.e. 2021),
  > so it looks like a stray. But that file is **committed and pre-existing; this feature never
  > touched it**, and switching a crate's edition changes real compilation semantics. The pin is
  > what 033 introduced, so the pin is what 033 fixes.

  **Do NOT** edit `.github/workflows/backend.yml`, `frontend.yml`, or any `Cargo.toml`.

  **Also unblocked by this same one-line edit**: `.github/workflows/gate-verification.yml` runs
  cargo under the same pin and dies the same way. It needs **no** separate fix — do not go looking
  for a second edit there.

  **Second required step — remove the local override.** This is not optional cleanup; it is what
  makes the verification below meaningful. After the edit the override and the pin name the same
  version, so this changes nothing about what actually runs — it just stops the override from
  masking the file:
  ```bash
  rustup override unset --path backend
  ```

  **Verify (static)** — from the repository root, in this order:
  ```bash
  grep channel backend/rust-toolchain.toml       # channel = "1.97.0"
  cd backend && rustup show active-toolchain     # must print 1.97.0 ... (from rust-toolchain.toml)
  cd backend && cargo metadata --format-version 1 >/dev/null && echo "pin resolves OK"
  cd backend && cargo +1.84.0 metadata --format-version 1 >/dev/null 2>&1 || echo "1.84.0 still cannot load it — expected, this is the bug you just fixed"
  ```

  > **The bare `cargo metadata` (no `+toolchain`) is the whole point.** It is the only form that
  > reproduces CI, where no override exists and `rust-toolchain.toml` alone governs. If you skip
  > the `rustup override unset` step, or if you verify with `cargo +1.97.0 metadata` instead, the
  > check passes **no matter what you typed in the file** — a typo like `channel = "1.9.70"` would
  > sail through and CI would still die. The second command proves rustup read your edited file
  > and got a real toolchain out of it.

  The second command must name **1.97.0** and say it came from `rust-toolchain.toml`. The third
  must print `pin resolves OK`. The fourth must print its message.

- [X] T166 **CRITICAL** Make the 85 files this feature modified rustfmt-clean, per **FR-034, SC-008** (contradicts). **Do T165 first** — formatting output depends on which toolchain runs, and before T165 no toolchain runs at all.

  **The problem**: `.github/workflows/backend.yml` runs `cargo fmt --check` as a hard gate step
  (no `continue-on-error`). It fails. Once T165 lets the toolchain load the workspace, this is the
  next step that stops the gate.

  **Read this before you hesitate.** Rule 3 and T001 tell you to STOP rather than run
  `cargo fmt --all`, and Phases 13/16/17 called these failures "toolchain skew". The guard exists
  to stop you reformatting code this feature never touched. **That risk was measured during this
  assessment and is zero here:**

  ```
  files failing `cargo fmt --check` ............................. 85
  of those, files NOT modified by this feature ................... 0
  ```

  Every dirty file is one 033 edited, and the diffs are ordinary over-long lines this feature
  introduced — for example, the `csp_policy:` field T133 restored into ~80 test files:

  ```rust
  // 108 characters — over rustfmt's default max_width of 100
          csp_policy: "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".into(),
  ```

  There is no `rustfmt.toml` in this repository, so defaults apply and any rustfmt version flags
  these. **You are authorised to run `cargo fmt --all`.**

  **Do this, in order, from the repository root:**

  ```bash
  # 1. Record which files are already modified, BEFORE formatting.
  git status --porcelain | awk '{print $NF}' | grep '\.rs$' | sort -u > /tmp/before.txt

  # 2. Format, using the toolchain T165 pinned.
  cd backend && cargo fmt --all && cd ..

  # 3. Record the modified set again.
  git status --porcelain | awk '{print $NF}' | grep '\.rs$' | sort -u > /tmp/after.txt

  # 4. PROVE no previously-clean file was touched. This MUST print nothing.
  comm -13 /tmp/before.txt /tmp/after.txt
  ```

  **Step 4 is the safety check, not a formality.** If it prints any path, `cargo fmt --all`
  reformatted a file 033 had not modified — `git checkout -- <that path>` to restore it, then stop
  and report, because that means the pin from T165 is not the toolchain this repository was
  written under.

  **Do NOT** hand-edit any file to satisfy the formatter, **do NOT** add a `rustfmt.toml` to widen
  `max_width`, and **do NOT** add `#[rustfmt::skip]`. Let the tool do it.

  **Verify (static)**:
  ```bash
  cd backend && cargo fmt --check; echo "fmt exit=$?"        # must be 0
  cd backend && cargo clippy --workspace -- -D warnings      # must still exit 0
  ```
  Both must pass. Reformatting cannot change behaviour, so no test needs re-running for this task
  alone — but **T090 remains the runtime gate** for the workspace as a whole.

**Checkpoint**: the pinned toolchain can load the workspace, so the backend gate's commands
actually execute instead of aborting on the first one; `cargo fmt --check` and
`cargo clippy --workspace -- -D warnings` both pass under that same pinned toolchain; and a local
checkout with no rustup override now reproduces exactly what CI runs. Combined with T160, the
backend gate has no remaining known static blocker — **T090/T092 are still what prove it green at
runtime.**
