//! Knowledge base module — document storage, full-text and vector search,
//! content validation, upload management, and the embedding-indexing/retrieval
//! pipeline for the knowledge-base feature.
//!
//! # Purpose
//! Provides a tenant-scoped knowledge base where operators can upload documents
//! (PDF, text, etc.), have them sanitised, text-extracted, deterministically
//! chunked, and indexed (both full-text and vector embeddings), and search
//! across them via full-text or vector similarity queries. Documents are stored
//! in the object storage backend (S3 or in-memory) and their metadata/indexed
//! content lives in PostgreSQL.  An outbox-driven indexer asynchronously builds
//! and maintains embedding vectors.
//!
//! # Responsibilities
//! - Document upload, content-type validation, HTML sanitisation via [`ammonia`]
//! - Text extraction from PDF and HTML documents via [`chunking::extract_text`]
//! - Deterministic content chunking via [`chunking::chunk_text`]
//! - Full-text search (PostgreSQL `tsvector`) and vector search (pgvector)
//! - Outbox-driven embedding indexing via [`indexer::run_knowledge_indexer_worker`]
//! - Tenant-scoped similarity search via [`retrieval::search`]
//! - Index state tracking (pending/indexed/failed) via [`index_state`]
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
//! - [`chunking`] — text extraction and deterministic chunking
//! - [`chunking::extract_text`] — extract plain text from PDF, HTML, or plain input
//! - [`chunking::chunk_text`] — split text into deterministic, size-bounded chunks
//! - [`index_state`] — per-item indexing status (pending / indexed / failed / not_indexable)
//! - [`index_state::*`] — query and update index-state rows
//! - [`indexer::run_knowledge_indexer_worker`] — outbox-driven embedding-indexing worker
//! - [`retrieval::search`] — tenant-scoped similarity search returning ranked chunks
//!
//! # Dependencies
//! - [`storage`] — object storage backend (S3 / in-memory)
//! - [`authz`] — RBAC middleware and policy checks
//! - [`tenancy`] — multi-tenant context and scope resolution
//! - [`kernel`] — shared error types and response envelope
//! - [`ai_providers`] — embedding provider for vector generation
//!
//! # Data Model
//! Migration 0047 creates the embedding-index tables:
//! - `knowledge_chunks` — chunked content with pgvector embeddings and a content hash
//! - `knowledge_index_state` — per-item indexing lifecycle (pending/indexed/failed/not_indexable)
//!
//! # Extension Points
//! - **Embedding provider**: new vectorizers can be added in [`ai_providers`]
//!   while the indexer and search remain unchanged.
//! - **Storage backend**: [`storage::ObjectStorage`] can be swapped for any
//!   S3-compatible or local store without touching indexing or search logic.

pub mod chunking;
pub mod index_state;
pub mod indexer;
pub mod retrieval;
pub mod routes;
pub mod store;
pub mod upload;
pub mod validate;
