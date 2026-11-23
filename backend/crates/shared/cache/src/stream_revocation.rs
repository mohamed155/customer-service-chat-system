use futures::StreamExt;
use kernel::stream_revocation::{Bus, Signal, Transport};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration};

const CHANNEL: &str = "customer-service:auth:stream-revalidation:v1";
const DEADLINE: Duration = Duration::from_secs(2);

struct Outgoing(mpsc::Sender<Signal>);
impl Transport for Outgoing {
    fn send(&self, signal: Signal) {
        if self.0.try_send(signal).is_err() {
            tracing::warn!("stream revalidation signal queue unavailable");
        }
    }
}

pub fn connect(client: redis::Client, workers: &mut tokio::task::JoinSet<&'static str>) -> Bus {
    let local = Bus::default();
    let subscriber = local.clone();
    let subscriber_client = client.clone();
    workers.spawn(async move {
        let mut delay = Duration::from_secs(1);
        loop {
            let subscription = timeout(DEADLINE, async {
                let mut sub = subscriber_client.get_async_pubsub().await?;
                sub.subscribe(CHANNEL).await?;
                Ok::<_, redis::RedisError>(sub)
            })
            .await;
            if let Ok(Ok(mut sub)) = subscription {
                delay = Duration::from_secs(1);
                let mut messages = sub.on_message();
                while let Some(message) = messages.next().await {
                    if let Ok(signal) = serde_json::from_slice(message.get_payload_bytes()) {
                        subscriber.deliver(signal);
                    }
                }
            }
            tracing::warn!("stream revalidation Redis subscription disconnected");
            sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(30));
        }
    });

    let (sender, mut receiver) = mpsc::channel(256);
    workers.spawn(async move {
        let mut connection = None;
        while let Some(signal) = receiver.recv().await {
            let Ok(payload) = serde_json::to_string(&signal) else {
                tracing::warn!("failed to encode stream revalidation signal");
                continue;
            };
            let published = timeout(DEADLINE, async {
                if connection.is_none() {
                    connection = Some(client.get_multiplexed_async_connection().await?);
                }
                let Some(conn) = connection.as_mut() else {
                    return Ok::<(), redis::RedisError>(());
                };
                redis::cmd("PUBLISH")
                    .arg(CHANNEL)
                    .arg(payload)
                    .query_async::<i64>(conn)
                    .await?;
                Ok(())
            })
            .await;
            if !matches!(published, Ok(Ok(()))) {
                connection = None;
                tracing::warn!("stream revalidation Redis publish failed");
            }
        }
        "stream revalidation Redis publisher"
    });
    local.with_transport(Outgoing(sender))
}
