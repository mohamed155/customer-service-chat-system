use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub tenant_id: Option<String>,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub actor: Value,
    pub request_id: String,
    pub payload: Value,
}

type Subscriber = Arc<dyn Fn(&Event) + Send + Sync>;
#[derive(Clone, Default)]
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
}
impl EventBus {
    pub fn subscribe<F>(&self, subscriber: F)
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        self.subscribers
            .lock()
            .expect("event bus lock poisoned")
            .push(Arc::new(subscriber));
    }
    pub fn publish(&self, event: &Event) {
        for subscriber in self
            .subscribers
            .lock()
            .expect("event bus lock poisoned")
            .iter()
        {
            subscriber(event);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboxEvent {
    pub id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub tenant_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
    pub attempts: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subscriber_receives_published_event() {
        let bus = EventBus::default();
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&received);
        bus.subscribe(move |event| sink.lock().unwrap().push(event.event_id.clone()));
        let event = Event {
            event_id: "evt_1".into(),
            event_type: "test.published".into(),
            occurred_at: Utc::now(),
            tenant_id: None,
            aggregate_type: "test".into(),
            aggregate_id: "1".into(),
            actor: serde_json::json!({"type":"system","id":"system"}),
            request_id: "req_1".into(),
            payload: serde_json::json!({}),
        };
        bus.publish(&event);
        assert_eq!(*received.lock().unwrap(), vec!["evt_1"]);
    }
}
