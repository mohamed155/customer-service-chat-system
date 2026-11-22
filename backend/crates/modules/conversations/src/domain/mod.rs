//! The rules of conversations and messages, with no framework, no database and no HTTP in them
//! (ADR-017). Everything here is a pure function or a plain value, unit-tested without a database.

pub mod assignment;
pub mod channel;
pub mod cursor;
pub mod inbox;
pub mod message;
pub mod status;
pub mod whatsapp;

pub use assignment::{assignee_is_told, assignment_change, origin_tells_assignee};
pub use channel::{is_known_channel, CHANNELS};
pub use cursor::{decode_cursor, decode_timeline_cursor, encode_cursor, encode_timeline_cursor};
pub use inbox::{
    awaiting_ai_decision, clamp_page_size, AssigneeFilter, FilterError, InboxFilter, StatusFilter,
};
pub use message::{
    attribute_staff_message, confidence_band, validate_body, BodyError, MessageKind, SenderRole,
};
pub use status::{status_after_message, status_change, ConversationStatus};
