# ADR-002: Backend stack — Rust, Axum, Tokio, SQLx, Tracing

- **Status:** Accepted (MOH-15)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §4.1, §16 and §18](../architecture/system-architecture.md#41-backend-cargo-workspace),
  constitution Principles VII–VIII and Technology Standards, ADR-001 (modular monolith),
  ADR-012 (API conventions), ADR-013 (UUIDv7)

## Context

The backend serves a dashboard API, public widget and webhook traffic, long-lived SSE/WebSocket
streams, AI provider calls that take seconds and stream tokens, and background workers for
ingestion, embeddings and outbound delivery.

The decision has to balance these forces:

- **Correctness and safety.** A multi-tenant product with tool calling can't afford null
  dereferences, data races or silently ignored errors. Wrong-tenant bugs must be hard to write.
- **Concurrency.** Many open streams and slow upstream calls need cheap concurrency and
  predictable latency without a garbage-collector pause budget.
- **Cost.** AI tokens dominate variable cost; compute should stay small and predictable.
- **Ecosystem.** PostgreSQL, Redis, S3, OpenTelemetry, JWT, Argon2 and HTTP clients must have
  mature libraries.
- **Team velocity.** Compile times and the learning curve slow iteration and must be managed.

## Decision

We will build the backend in **Rust (stable)** on this stack:

| Concern | Choice | Notes |
| -- | -- | -- |
| Language and toolchain | **Rust stable**, pinned in `backend/rust-toolchain.toml` with `rustfmt` and `clippy` | Toolchain bumps are deliberate PRs. |
| Async runtime | **Tokio** (multi-threaded) | One runtime per process. Blocking or CPU-heavy work (Argon2, parsing large documents) goes to `spawn_blocking` or a bounded pool. |
| HTTP framework | **Axum** on **Tower** / `tower-http` | Middleware (tenant context, auth, CSRF, rate limits, timeouts, tracing) is written as Tower layers. Axum is the only HTTP framework in request paths. |
| Database access | **SQLx** (PostgreSQL, `runtime-tokio`, rustls) | Plain SQL, no ORM. **Compile-time checked** queries (`query!`, `query_as!`) with offline metadata (`.sqlx/`) committed for CI. Migrations through `sqlx migrate`. |
| Serialization | **Serde** / `serde_json` | DTOs live in each module's `api` layer, separate from domain types. |
| Errors | **`thiserror`** typed enums per module, mapped once to RFC 9457 Problem Details (ADR-012) | No `unwrap` / `expect` / `panic!` in request or job paths (enforced by clippy lints). `anyhow` only in binaries, tests and tooling. |
| Observability | **`tracing`** + `tracing-subscriber` (JSON) + **OpenTelemetry** (OTLP) + Prometheus metrics | Spans carry `request_id`, `tenant_id`, `actor_id`. A redaction layer keeps secrets and raw PII out of logs. |
| API contract | **utoipa** generating OpenAPI 3.1 from code | Source for the generated TypeScript client (ADR-012). |
| Outbound HTTP | **reqwest** with rustls | Wrapped by the SSRF-guarded client in the platform layer. |
| Crypto | RustCrypto / `aws-lc-rs` (Argon2id, Ed25519, AES-GCM, HMAC-SHA256) | Algorithms per architecture §19. No home-made cryptography. |
| Testing | `cargo test`, **Testcontainers** (Postgres + pgvector, Redis, MinIO), `proptest` | Integration tests use real services, not mocks (constitution VI). |

Supporting rules:

- **TLS:** rustls everywhere. No OpenSSL dependency in release images.
- **Lints:** `cargo clippy -- -D warnings` in CI, with `unwrap_used`, `expect_used` and `panic`
  denied outside tests.
- **Supply chain:** `cargo-deny` (licences, advisories, duplicates) and `cargo audit` in CI.
- **New major dependencies** (another web framework, an ORM, a second async runtime) need an ADR.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Go** (net/http or chi + pgx) | Fast to learn and compile, good concurrency. But it has nil pointers, no sum types and no compile-time distinction between ID types, which weakens the type-driven tenant-safety rules (`TenantContext`, newtype IDs). Error handling is easy to skip. |
| **Node.js / TypeScript** (NestJS, Fastify) | Shares a language with the frontend. But single-threaded CPU limits (Argon2, chunking, token counting), runtime-only type safety, a large dependency surface and higher memory per connection. |
| **JVM** (Kotlin + Spring Boot / Ktor) | Mature ecosystem. Higher memory footprint and startup time, GC pauses under streaming load, and heavier framework conventions than this team needs. |
| **Actix Web** instead of Axum | Comparable performance. Axum's Tower middleware model is shared with `tower-http`, `tonic` and `reqwest` layers, and it's maintained by the Tokio team. Mixing the two would double the middleware stack. |
| **Diesel or SeaORM** instead of SQLx | An ORM DSL hides SQL that we need to tune (RLS, keyset pagination, `SKIP LOCKED`, pgvector, full-text search). Diesel's async story and SeaORM's runtime-built queries both lose plain-SQL clarity. SQLx keeps SQL as written and checks it at compile time. |
| **`log` + ad-hoc metrics** | No structured span context across async boundaries. `tracing` is the standard for Tokio and feeds OpenTelemetry directly. |

## Consequences

**Positive**

- Memory safety, no data races, and exhaustive `match` on typed errors.
- The type system carries security rules: newtype IDs, `TenantContext` that only authorization can
  construct, typestate for workflows.
- Small, predictable memory and CPU use per connection; cheap long-lived streams.
- SQL stays visible and tunable, and compile-time checking catches schema drift before deploy.

**Negative and costs**

- Steeper learning curve and slower compiles than Go or Node. Needs `sccache` / caching in CI and
  a workspace split that keeps incremental builds small.
- Compile-time SQL checks need a database or committed `.sqlx` metadata in CI, and every schema
  change has to regenerate it.
- Fewer ready-made SaaS libraries than JVM/Node (billing SDKs, some vendor SDKs). Some adapters
  will be thin HTTP clients we write ourselves.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| S1 | **No compile-time checked SQL.** There are about 2,000 runtime `sqlx::query(…)` / `query_as(…)` calls and no `query!` / `query_as!` macros. Column and type mistakes surface only at runtime or in tests. | `backend/crates/**` | Introduce `query!` / `query_as!` module by module, commit `.sqlx/` offline data, and add `cargo sqlx prepare --check` to CI. New queries use the macros from now on. |
| S2 | CI runs `cargo clippy --workspace -- -D warnings`, but there's no `[workspace.lints]` table, so `unwrap_used` / `expect_used` / `panic` aren't denied. | `backend/Cargo.toml`, `.github/workflows/backend.yml` | Add the lints with `allow` in tests, then fix the hits. |
| S3 | **Errors aren't typed per module.** Handlers return the shared `kernel::ApiError`, and `thiserror` is used in only two crates. Responses aren't RFC 9457 Problem Details (covered by ADR-012). | `backend/crates/shared/kernel/src/lib.rs` (`ApiError`) | Give each module a `thiserror` enum mapped to `ApiError` at the `api` layer, alongside the ADR-012 Problem Details work. |
| S4 | CI runs `cargo audit` but not `cargo-deny`, so licences and duplicate dependencies aren't checked. | `.github/workflows/backend.yml` | Add `deny.toml` and a `cargo deny check` step. |
| S5 | The core stack already matches this ADR: Rust 1.97.0 pinned, Axum 0.8 on Tower, Tokio, SQLx 0.9 with rustls, `tracing` + OpenTelemetry, utoipa. | `backend/Cargo.toml`, `backend/rust-toolchain.toml` | None. |
