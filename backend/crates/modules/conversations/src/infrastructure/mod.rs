//! The database side of conversations (ADR-017): every SQL statement of the module lives here, as
//! an implementation of the ports `application` declares or as a single-statement function that
//! other modules call through the crate's facade.

pub mod attachments;
pub mod directory;
pub mod enrichment;
pub mod history;
pub mod inbound;
pub mod messages;
pub mod outbox;
pub mod unit;

pub use enrichment::PgEnrichment;
pub use unit::PgUnit;
