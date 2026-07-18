//! Vendor-agnostic AI provider adapters.
//!
//! # Purpose
//! Provides a uniform [`ChatProvider`] trait and HTTP implementations for
//! OpenAI, Anthropic, and Gemini.  Consumers interact through the trait alone;
//! no provider-specific types leak into public APIs.
//!
//! # Responsibilities
//! - Vendor HTTP communication (request construction, transport, response
//!   parsing)
//! - Request / response serialisation (JSON body, header injection)
//! - SSE stream decoding (server-sent event → [`StreamEvent`])
//! - Error normalisation (vendor errors → [`ProviderError`] taxonomy)
//!
//! # Public Interfaces
//! - [`ChatProvider`] trait — the core abstraction
//! - [`ChatRequest`], [`ChatCompletion`], [`StreamEvent`] — shared types
//! - [`ProviderError`] taxonomy (`ErrorCategory` enum)
//! - [`Registry`] — fixed catalog of available providers
//!
//! # Extension Points
//! - **New provider**: write an adapter module, add a [`ProviderKind`] enum
//!   variant, register in [`Registry`].
//! - **New capability**: add a method to [`ChatProvider`] and implement in
//!   every adapter.
//! - **HTTP client swap**: replace the underlying `reqwest::Client` in each
//!   adapter (only consumed internally).
//!
//! ## Dependencies
//! - `reqwest` — HTTP client for vendor API communication
//! - `serde` / `serde_json` — request / response serialisation
//! - `futures` — [`StreamEvent`] stream types
//! - `tokio` — async runtime
//! - `async-trait` — [`ChatProvider`] trait marker
//! - `tracing` — structured logging throughout adapters
//! - `thiserror` — error type infrastructure
//! - `zeroize` — secure zeroing of [`SecretKey`] on drop
//! - `bytes` — byte buffer for raw transport
//!
//! ## Data Model
//! - [`ChatProvider`] trait — the central abstraction all consumers depend on
//! - [`ChatRequest`], [`ChatCompletion`], [`StreamEvent`] — shared contract
//!   types for request, response, and streaming
//! - [`ErrorCategory`], [`ProviderError`] — normalised error taxonomy across
//!   all vendors
//! - [`SecretKey`] — credential wrapper with zeroize-on-drop semantics
//! - [`ProviderKind`], [`Registry`] — provider catalog and instantiation
//! - [`Role`], [`Message`], [`TokenUsage`], [`FinishReason`] — supporting
//!   value types
//! - [`ChatStream`] — `BoxStream` alias for streaming responses

pub mod anthropic;
pub mod contract;
pub mod crypto;
pub mod gemini;
pub mod openai;
pub mod registry;
pub(crate) mod sse;
pub use contract::*;
pub use registry::*;
