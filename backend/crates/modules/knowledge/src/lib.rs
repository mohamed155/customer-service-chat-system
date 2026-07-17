//! Knowledge base module — document storage, full-text and vector search,
//! content validation, and upload management for the knowledge-base feature.
//!
//! # Purpose
//! Provides a tenant-scoped knowledge base where operators can upload documents
//! (PDF, text, etc.), have them sanitised and indexed, and search across them
//! via full-text or vector similarity queries. Documents are stored in the
//! object storage backend (S3 or in-memory) and their metadata/indexed content
//! lives in PostgreSQL.
//!
//! # Responsibilities
//! - Document upload, content-type validation, HTML sanitisation via [`ammonia`]
//! - Full-text search (PostgreSQL `tsvector`) and vector search (pgvector)
//! - Tenant-scoped document CRUD via [`routes`]
//! - Persistent metadata/index storage via [`store`]
//! - File upload handling via [`upload`]
//! - Content validation/sanitisation via [`validate`]
//!
//! # Public Interfaces
//! - [`routes`] — Axum router (mounted by the server)
//! - [`store`] — document/index persistence
//! - [`upload`] — file upload handling
//! - [`validate`] — content validation and sanitisation
//!
//! # Dependencies
//! - [`storage`] — object storage backend (S3 / in-memory)
//! - [`authz`] — RBAC middleware and policy checks
//! - [`tenancy`] — multi-tenant context and scope resolution
//! - [`kernel`] — shared error types and response envelope

pub mod routes;
pub mod store;
pub mod upload;
pub mod validate;
