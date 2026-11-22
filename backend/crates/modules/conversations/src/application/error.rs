//! Why a conversation use case failed.

use std::error::Error;
use std::fmt;

use uuid::Uuid;

/// A failure of whatever stores conversations (the database, today), kept as the store reported it.
#[derive(Debug)]
pub struct StoreError(Box<dyn Error + Send + Sync>);

impl StoreError {
    pub fn new(source: impl Error + Send + Sync + 'static) -> Self {
        Self(Box::new(source))
    }

    /// The store's own error when it is a `T`, so a caller that speaks the store's language (the
    /// facade over SQL) can hand it on unchanged.
    pub fn downcast<T: Error + 'static>(self) -> Result<T, Self> {
        self.0.downcast::<T>().map(|boxed| *boxed).map_err(Self)
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.0)
    }
}

/// What a port call returns.
pub type StoreResult<T> = Result<T, StoreError>;

/// Why a use case did not do what was asked.
#[derive(Debug)]
pub enum ConversationError {
    /// There is no such conversation in this tenant, or it was deleted.
    NotFound,
    /// The customer does not exist in this tenant.
    CustomerNotFound { tenant_id: Uuid, customer_id: Uuid },
    /// The member a conversation was to be assigned to is not an active member of this tenant.
    MembershipNotActive {
        tenant_id: Uuid,
        membership_id: Uuid,
    },
    /// The store failed.
    Store(StoreError),
}

impl fmt::Display for ConversationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("Conversation not found or soft-deleted"),
            Self::CustomerNotFound {
                tenant_id,
                customer_id,
            } => write!(f, "Customer {customer_id} not found in tenant {tenant_id}"),
            Self::MembershipNotActive {
                tenant_id,
                membership_id,
            } => write!(
                f,
                "Membership {membership_id} is not active in tenant {tenant_id}"
            ),
            Self::Store(error) => error.fmt(f),
        }
    }
}

impl Error for ConversationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StoreError> for ConversationError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// What a use case returns.
pub type Outcome<T> = Result<T, ConversationError>;
