//! The HTTP side of conversations (ADR-017): handlers, query parameters and response envelopes.
//! Handlers extract and authorise the request, call a use case of `application` on the caller's
//! transaction, and turn the outcome into a response.

pub mod history;
pub mod routes;
