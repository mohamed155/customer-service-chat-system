//! What conversations do: the use cases, the ports they need, and the shapes they exchange
//! (ADR-017). Nothing here knows about SQL or HTTP; `infrastructure` implements the ports and
//! `api` calls the use cases.

pub mod audit;
pub mod error;
pub mod events;
pub mod model;
pub mod ports;
pub mod records;
pub mod service;

use async_trait::async_trait;
use uuid::Uuid;

pub use error::{ConversationError, StoreError, StoreResult};
pub use events::ConversationEvent;
pub use service::ConversationActor;

/// Whether the tenant's AI agent is set up and the platform's AI is available to it. The server
/// implements it, because the answer lives in the `ai` module, which depends on this one.
#[async_trait]
pub trait AiAgentStatus: Send + Sync {
    async fn agent_configured(&self, tenant_id: Uuid) -> bool;
    async fn platform_ai_available(&self, tenant_id: Uuid) -> bool;
}
