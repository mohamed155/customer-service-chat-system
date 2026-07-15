use crate::audit;
use crate::model::AvailabilityChangedEvent;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use tracing::error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Event {
    EscalationAssigned(crate::model::EscalationAssignedEvent),
    EscalationQueued(crate::model::EscalationQueuedEvent),
    EscalationRemoved(crate::model::EscalationRemovedEvent),
    AvailabilityChanged(AvailabilityChangedEvent),
}

struct TenantPresence {
    members: HashMap<Uuid, usize>,
    tx: broadcast::Sender<Event>,
}

pub struct PresenceGuard {
    tenant_id: Uuid,
    membership_id: Uuid,
    registry: Arc<Mutex<HashMap<Uuid, TenantPresence>>>,
    runtime: Arc<RuntimeInner>,
}

impl Drop for PresenceGuard {
    fn drop(&mut self) {
        let tenant_id = self.tenant_id;
        let membership_id = self.membership_id;
        let registry = self.registry.clone();
        let runtime = self.runtime.clone();
        tokio::spawn(async move {
            let should_revert = {
                let mut map = registry.lock().await;
                if let Some(tp) = map.get_mut(&tenant_id) {
                    if let Some(count) = tp.members.get_mut(&membership_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            tp.members.remove(&membership_id);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            };
            if should_revert {
                schedule_presence_revert(runtime, tenant_id, membership_id).await;
            }
        });
    }
}

struct RuntimeInner {
    pool: PgPool,
    grace: Duration,
    registry: Arc<Mutex<HashMap<Uuid, TenantPresence>>>,
}

#[derive(Clone)]
pub struct Runtime {
    inner: Arc<RuntimeInner>,
}

impl Runtime {
    pub fn new(pool: PgPool, grace: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(RuntimeInner {
                pool,
                grace,
                registry: Arc::new(Mutex::new(HashMap::new())),
            }),
        })
    }

    pub fn present_membership_ids(&self, tenant_id: Uuid) -> Vec<Uuid> {
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                let registry = self.inner.registry.clone();
                handle.block_on(async move {
                    let map = registry.lock().await;
                    map.get(&tenant_id)
                        .map(|tp| tp.members.keys().copied().collect())
                        .unwrap_or_default()
                })
            }
            Err(_) => Vec::new(),
        }
    }

    pub async fn present_membership_ids_async(&self, tenant_id: Uuid) -> Vec<Uuid> {
        let map = self.inner.registry.lock().await;
        map.get(&tenant_id)
            .map(|tp| tp.members.keys().copied().collect())
            .unwrap_or_default()
    }

    pub fn connect(
        self: &Arc<Self>,
        tenant_id: Uuid,
        membership_id: Uuid,
    ) -> (PresenceGuard, broadcast::Receiver<Event>) {
        let registry = self.inner.registry.clone();
        let rx = {
            let mut map = registry.blocking_lock();
            let tp = map.entry(tenant_id).or_insert_with(|| {
                let (tx, _) = broadcast::channel(256);
                TenantPresence {
                    members: HashMap::new(),
                    tx,
                }
            });
            *tp.members.entry(membership_id).or_insert(0) += 1;
            tp.tx.subscribe()
        };
        let guard = PresenceGuard {
            tenant_id,
            membership_id,
            registry: self.inner.registry.clone(),
            runtime: self.inner.clone(),
        };
        (guard, rx)
    }

    pub fn broadcast(&self, tenant_id: Uuid, event: Event) {
        let registry = self.inner.registry.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let map = registry.lock().await;
                if let Some(tp) = map.get(&tenant_id) {
                    let _ = tp.tx.send(event);
                }
            });
        } else {
            let map = registry.blocking_lock();
            if let Some(tp) = map.get(&tenant_id) {
                let _ = tp.tx.send(event);
            }
        }
    }

    pub async fn startup_sweep(&self) -> sqlx::Result<()> {
        let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT tenant_id, membership_id FROM agent_availability WHERE state = 'available'",
        )
        .fetch_all(&self.inner.pool)
        .await?;
        for (tenant_id, membership_id) in rows {
            schedule_presence_revert(self.inner.clone(), tenant_id, membership_id).await;
        }
        Ok(())
    }
}

async fn schedule_presence_revert(
    runtime: Arc<RuntimeInner>,
    tenant_id: Uuid,
    membership_id: Uuid,
) {
    tokio::spawn(async move {
        tokio::time::sleep(runtime.grace).await;
        let still_absent = {
            let map = runtime.registry.lock().await;
            map.get(&tenant_id)
                .and_then(|tp| tp.members.get(&membership_id))
                .copied()
                .unwrap_or(0)
                == 0
        };
        if still_absent {
            let mut tx = match runtime.pool.begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    error!(%e, "presence revert: failed to begin transaction");
                    return;
                }
            };
            let result: Result<(), sqlx::Error> = async {
                let updated = sqlx::query_scalar::<_, String>(
                    "UPDATE agent_availability SET state = 'away', state_changed_at = now() \
                     WHERE tenant_id = $1 AND membership_id = $2 AND state = 'available' \
                     RETURNING state",
                )
                .bind(tenant_id)
                .bind(membership_id)
                .fetch_optional(&mut *tx)
                .await?;
                if updated.is_some() {
                    audit::record_availability_changed(
                        &mut tx,
                        // No actor user id for automated reverts
                        Uuid::nil(),
                        tenant_id,
                        membership_id,
                        Some("available"),
                        "away",
                        "presence_timeout",
                    )
                    .await?;
                }
                Ok(())
            }
            .await;
            if let Err(e) = result {
                let _ = tx.rollback().await;
                error!(%e, tenant_id = %tenant_id, membership_id = %membership_id, "presence revert failed");
                return;
            }
            if let Err(e) = tx.commit().await {
                error!(%e, "presence revert: commit failed");
                return;
            }
            let event = AvailabilityChangedEvent {
                v: 1,
                membership_id,
                state: crate::model::AvailabilityState::Away,
                cause: "presence_timeout".into(),
            };
            {
                let map = runtime.registry.lock().await;
                if let Some(tp) = map.get(&tenant_id) {
                    let _ = tp.tx.send(Event::AvailabilityChanged(event));
                }
            }
        }
    });
}
