//! # WhatsApp Channel Module
//!
//! ## Purpose
//! WhatsApp Business Cloud API integration: webhook verification and intake,
//! inbound message processing (dedupe, customer identity resolution,
//! conversation create-or-append, media storage), and outbound message
//! sending with full lifecycle tracking.
//!
//! ## Responsibilities
//! - Meta webhook GET handshake (subscription verification)
//! - Signed POST delivery intake with HMAC-SHA256 verification and per-connection rate limiting
//! - Inbound message deduplication by provider message ID (wamid)
//! - Customer identity resolution using E.164 normalization
//! - Conversation auto-creation and open-conversation reuse
//! - Media attachment fetch from Meta Graph API to S3-compatible storage
//! - Outbound message sender worker with sent/delivered/read/failed lifecycle
//! - 24-hour customer-service window enforcement
//! - Attachment download endpoint for the dashboard
//!
//! ## Public Interfaces
//! - `GET /integrations/whatsapp/webhook/{token}` — Meta subscription verification
//! - `POST /integrations/whatsapp/webhook/{token}` — Meta delivery intake
//! - `GET /tenant/conversations/{conversation_id}/attachments/{attachment_id}` — media download
//!
//! ## Dependencies
//! - `integrations`: connection lifecycle, secret management, webhook token hashing
//! - `conversations`: message and conversation CRUD, outbox event emission
//! - `customers`: customer entity and channel identifier operations
//! - `storage`: S3-compatible object persistence for attachments
//! - `kernel`: shared types, rate limiting
//! - External: `reqwest` for Meta Graph API calls, `hmac`/`sha2` for signature verification
//!
//! ## Data Model
//! Migration `0057_whatsapp_channel.sql`:
//! - `whatsapp_message_meta`: channel-specific side record for every WhatsApp message
//!   (direction, wamid, delivery_status with monotonic transitions)
//! - `message_attachments`: channel-generic media storage (image/audio/video/document)
//! - Catalog seed: `integration_catalog` row for slug `whatsapp`
//!
//! ## Extension Points
//! - `WhatsAppApi` trait: production `GraphWhatsAppApi` impl for Meta, `MockWhatsAppApi` for tests
//! - New media types require adding to the `kind` CHECK constraint and `message_attachments` enum
//! - Outbound template messages (v2+) add a new sender type alongside `send_text`

pub mod api;
pub mod identity;
pub mod inbound;
pub mod media;
pub mod model;
pub mod queries;
pub mod routes;
pub mod sender;
pub mod webhook;
pub mod window;
