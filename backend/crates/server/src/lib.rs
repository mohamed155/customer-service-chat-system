//! Server — HTTP composition root and Axum router
//!
//! # Purpose
//! Assembles the Axum router from all module crates, applies middleware
//! layers (CORS, security headers, rate limiting, request-ID, tracing,
//! metrics, CSRF protection), and builds the application state. The
//! binary in `main.rs` is a thin wrapper around this crate.
//!
//! # Public Interfaces
//! - `router::app(state)` / `router::app_with_test_routes(state)` / etc.
//! - `router::documented_openapi()` — OpenAPI spec for the coverage gate
//! - `router::configured_email_sender()`, `router::docs_surface_enabled()`
//! - `rate_limit::per_ip_creation_limit`, `rate_limit::login_rate_limit`
//! - `rate_limit::ServerTenantRateLimit`
//! - `security_headers::scope_a_layers`, `security_headers::scope_b_layers`
//! - `state::AppState`
//! - `openapi::ApiDoc`
//! - `handlers` — composite handlers that cross module boundaries
//!
//! # Dependencies
//! - All module crates (identity, tenancy, widgets, feedback, integrations, etc.)
//! - `kernel`, `config`, `cache`, `observability`, `authz`
//! - `axum`, `tokio`, `tower-http`, `utoipa`
//!
//! # Extension Points
//! - Add new middleware layers in `build_app` or `api_routes`.
//! - Add new route groups as `fn *_routes()` called from `api_routes`.

pub mod handlers;
pub mod internal_error;
pub mod openapi;
pub mod rate_limit;
pub mod rate_limit_audit;
pub mod router;
pub mod run_mode;
pub mod secret_maintenance;
pub mod security_headers;
pub mod state;
