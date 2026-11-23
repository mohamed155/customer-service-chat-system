use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Signal {
    Session { session_id: Uuid },
    User { user_id: Uuid },
    Membership { tenant_id: Uuid, user_id: Uuid },
}

impl Signal {
    pub fn matches(self, session: Option<Uuid>, user: Uuid, tenant: Uuid) -> bool {
        match self {
            Self::Session { session_id } => Some(session_id) == session,
            Self::User { user_id } => user_id == user,
            Self::Membership { tenant_id, user_id } => tenant_id == tenant && user_id == user,
        }
    }
}

#[derive(Clone)]
pub struct Bus(
    broadcast::Sender<Signal>,
    Option<std::sync::Arc<dyn Transport>>,
);

pub trait Transport: Send + Sync {
    fn send(&self, signal: Signal);
}

impl Default for Bus {
    fn default() -> Self {
        Self(broadcast::channel(256).0, None)
    }
}

impl Bus {
    pub fn subscribe(&self) -> broadcast::Receiver<Signal> {
        self.0.subscribe()
    }

    pub fn with_transport(mut self, transport: impl Transport + 'static) -> Self {
        self.1 = Some(std::sync::Arc::new(transport));
        self
    }

    pub fn publish(&self, signal: Signal) {
        self.deliver(signal);
        if let Some(transport) = &self.1 {
            transport.send(signal);
        }
    }

    pub fn deliver(&self, signal: Signal) {
        let _ = self.0.send(signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_match_only_the_intended_stream_scope() {
        let session = crate::ids::new_id();
        let user = crate::ids::new_id();
        let tenant = crate::ids::new_id();
        let other = crate::ids::new_id();
        assert!(Signal::Session {
            session_id: session
        }
        .matches(Some(session), user, tenant));
        assert!(!Signal::Session { session_id: other }.matches(Some(session), user, tenant));
        assert!(!Signal::Session {
            session_id: session
        }
        .matches(None, user, tenant));
        assert!(Signal::User { user_id: user }.matches(Some(session), user, tenant));
        assert!(!Signal::User { user_id: other }.matches(Some(session), user, tenant));
        assert!(Signal::Membership {
            tenant_id: tenant,
            user_id: user
        }
        .matches(Some(session), user, tenant));
        assert!(!Signal::Membership {
            tenant_id: other,
            user_id: user
        }
        .matches(Some(session), user, tenant));
        assert!(!Signal::Membership {
            tenant_id: tenant,
            user_id: other
        }
        .matches(Some(session), user, tenant));
    }
}
