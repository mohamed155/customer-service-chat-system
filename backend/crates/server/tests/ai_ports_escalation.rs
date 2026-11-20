#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! The escalation and realtime ports `ai` declares (`ai::ports::EscalationRouting`,
//! `ai::ports::AiRealtime`) and the server's adapters for them (`server::ai_ports`), MOH-195.
//! `ai` no longer depends on `escalations`, so these tests pin what the wiring must keep:
//!
//! - the events the dashboard reads over SSE are byte-for-byte what they were (the JSON of every
//!   event, and only the tenant's own connected people receive it);
//! - the escalation goes to a present agent (with the skills asked for) or to the queue, runs in
//!   the caller's transaction, and cannot cross tenants.

use std::time::Duration;

use ai::ports::{
    AiPorts, AiRealtimeEvent, EscalationError, FailureCategory, GenerationCompleted,
    GenerationDelta, GenerationFailed, GenerationStarted, GenerationSuperseded, SupersededReason,
    ToolRequestCreated, ToolRequestUpdated, ToolSource,
};
use chrono::{DateTime, TimeZone, Utc};
use escalations::model::ConversationAiEvent;
use escalations::presence::{ConversationToolEvent, Event, Runtime};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping ai_ports_escalation tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping ai_ports_escalation tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

fn ports_with(pool: &PgPool, presence: &std::sync::Arc<Runtime>) -> AiPorts {
    server::ai_ports::ai_ports(pool.clone(), presence.clone())
}

/// A presence runtime and ports for the tests that never touch a database.
fn offline() -> (std::sync::Arc<Runtime>, AiPorts) {
    let pool = db::lazy_pool(
        "postgres://localhost:5432/unused",
        1,
        Duration::from_secs(1),
    );
    let presence = Runtime::new(pool.clone(), Duration::from_secs(45));
    let ports = ports_with(&pool, &presence);
    (presence, ports)
}

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
}

/// The name of the SSE event an event becomes, and the JSON it carries.
fn wire(event: Event) -> (&'static str, Value) {
    match event {
        Event::ConversationAi(ConversationAiEvent::Started(p)) => {
            ("ai.message.started", serde_json::to_value(p).unwrap())
        }
        Event::ConversationAi(ConversationAiEvent::Delta(p)) => {
            ("ai.message.delta", serde_json::to_value(p).unwrap())
        }
        Event::ConversationAi(ConversationAiEvent::Completed(p)) => {
            ("ai.message.completed", serde_json::to_value(p).unwrap())
        }
        Event::ConversationAi(ConversationAiEvent::Superseded(p)) => {
            ("ai.message.superseded", serde_json::to_value(p).unwrap())
        }
        Event::ConversationAi(ConversationAiEvent::Failed(p)) => {
            ("ai.message.failed", serde_json::to_value(p).unwrap())
        }
        Event::ConversationTool(ConversationToolEvent::Created(p)) => {
            ("tool.request.created", serde_json::to_value(p).unwrap())
        }
        Event::ConversationTool(ConversationToolEvent::Updated(p)) => {
            ("tool.request.updated", serde_json::to_value(p).unwrap())
        }
        _ => panic!("the AI published an event that is not an AI event"),
    }
}

/// Publishes through the port and returns what a connected person of the tenant receives.
async fn delivered(event: AiRealtimeEvent) -> (&'static str, Value) {
    let (presence, ports) = offline();
    let (tenant, agent) = (Uuid::new_v4(), Uuid::new_v4());
    let (_guard, mut rx) = presence.connect(tenant, agent);
    ports.realtime.publish(tenant, event);
    wire(
        rx.try_recv()
            .expect("the connected person receives the event"),
    )
}

/// Every event reaches the dashboard with the same name and the same JSON as before the port.
#[tokio::test]
async fn every_ai_event_keeps_its_wire_name_and_json() {
    let (c, g, t, tool) = (
        Uuid::from_u128(1),
        Uuid::from_u128(2),
        Uuid::from_u128(3),
        Uuid::from_u128(4),
    );

    assert_eq!(
        delivered(AiRealtimeEvent::Started(GenerationStarted {
            conversation_id: c,
            generation_id: g,
            trigger_message_id: t,
            started_at: at(),
        }))
        .await,
        (
            "ai.message.started",
            json!({
                "conversationId": c, "generationId": g, "triggerMessageId": t,
                "startedAt": "2026-01-02T03:04:05Z"
            })
        )
    );
    assert_eq!(
        delivered(AiRealtimeEvent::Delta(GenerationDelta {
            conversation_id: c,
            generation_id: g,
            text: "Hel".into(),
        }))
        .await,
        (
            "ai.message.delta",
            json!({"conversationId": c, "generationId": g, "text": "Hel"})
        )
    );
    assert_eq!(
        delivered(AiRealtimeEvent::Completed(GenerationCompleted {
            conversation_id: c,
            generation_id: g,
            message: json!({"id": t, "body": "Hello"}),
        }))
        .await,
        (
            "ai.message.completed",
            json!({
                "conversationId": c, "generationId": g,
                "message": {"id": t, "body": "Hello"}
            })
        )
    );
    assert_eq!(
        delivered(AiRealtimeEvent::ToolRequestCreated(ToolRequestCreated {
            id: tool,
            conversation_id: c,
            tool_name: "update_customer_contact".into(),
            tool_source: ToolSource::Builtin,
            arguments: json!({"email": "anna@example.com"}),
            approval_required: true,
            expires_at: Some(at()),
            chain_index: 2,
            created_at: at(),
        }))
        .await,
        (
            "tool.request.created",
            json!({
                "id": tool, "conversationId": c, "toolName": "update_customer_contact",
                "toolSource": "builtin", "arguments": {"email": "anna@example.com"},
                "approvalRequired": true, "expiresAt": "2026-01-02T03:04:05Z",
                "chainIndex": 2, "createdAt": "2026-01-02T03:04:05Z"
            })
        )
    );
    assert_eq!(
        delivered(AiRealtimeEvent::ToolRequestUpdated(ToolRequestUpdated {
            id: tool,
            conversation_id: c,
            status: "cancelled".into(),
            decided_by_display_name: None,
            duration_ms: None,
            has_result: false,
            error: None,
        }))
        .await,
        (
            "tool.request.updated",
            json!({
                "id": tool, "conversationId": c, "status": "cancelled",
                "decidedByDisplayName": null, "durationMs": null,
                "hasResult": false, "error": null
            })
        )
    );
}

#[tokio::test]
async fn a_tenant_registered_tool_is_announced_as_tenant_sourced() {
    let (_, json) = delivered(AiRealtimeEvent::ToolRequestCreated(ToolRequestCreated {
        id: Uuid::from_u128(1),
        conversation_id: Uuid::from_u128(2),
        tool_name: "track_order".into(),
        tool_source: ToolSource::Tenant,
        arguments: json!({}),
        approval_required: false,
        expires_at: None,
        chain_index: 0,
        created_at: at(),
    }))
    .await;
    assert_eq!(json["toolSource"], "tenant");
    assert_eq!(json["expiresAt"], Value::Null);
}

/// Why a reply was dropped and why a generation failed keep their wire spelling, one by one.
#[tokio::test]
async fn every_reason_and_failure_category_keeps_its_wire_spelling() {
    let (c, g) = (Uuid::from_u128(1), Uuid::from_u128(2));
    for (reason, spelled) in [
        (SupersededReason::NewerMessage, "newer_message"),
        (SupersededReason::Escalated, "escalated"),
    ] {
        let (name, json) = delivered(AiRealtimeEvent::Superseded(GenerationSuperseded {
            conversation_id: c,
            generation_id: g,
            reason,
        }))
        .await;
        assert_eq!(name, "ai.message.superseded");
        assert_eq!(
            json,
            json!({"conversationId": c, "generationId": g, "reason": spelled})
        );
    }
    for (category, spelled) in [
        (FailureCategory::Unavailable, "unavailable"),
        (FailureCategory::Timeout, "timeout"),
        (FailureCategory::RateLimited, "rate_limited"),
        (FailureCategory::Authentication, "authentication"),
        (FailureCategory::InvalidRequest, "invalid_request"),
        (FailureCategory::Internal, "internal"),
    ] {
        let (name, json) = delivered(AiRealtimeEvent::Failed(GenerationFailed {
            conversation_id: c,
            generation_id: g,
            category,
        }))
        .await;
        assert_eq!(name, "ai.message.failed");
        assert_eq!(
            json,
            json!({"conversationId": c, "generationId": g, "category": spelled})
        );
    }
}

/// Only the tenant's own connected people hear the event, and nobody connected is not an error.
#[tokio::test]
async fn an_event_reaches_only_the_tenants_own_connected_people() {
    let (presence, ports) = offline();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let (_ga, mut in_a) = presence.connect(a, Uuid::new_v4());
    let (_gb, mut in_b) = presence.connect(b, Uuid::new_v4());
    let event = || {
        AiRealtimeEvent::Delta(GenerationDelta {
            conversation_id: Uuid::from_u128(1),
            generation_id: Uuid::from_u128(2),
            text: "x".into(),
        })
    };

    ports.realtime.publish(a, event());
    assert!(in_a.try_recv().is_ok());
    assert!(
        in_b.try_recv().is_err(),
        "tenant B must not hear tenant A's event"
    );

    // Nobody connected for this tenant: the event is dropped and nothing fails.
    ports.realtime.publish(Uuid::new_v4(), event());
}

// ── Escalation routing (real Postgres) ───────────────────────────────────────────────────────

async fn setup() {
    db::testing::migrate().await;
    sqlx::query(
        "TRUNCATE TABLE escalations, agent_availability, agent_skills, skills, \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Ports Escalation Tenant")
        .bind(format!("ports-esc-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// An active agent who is available for work; returns the membership id.
async fn seed_agent(pool: &PgPool, tenant_id: Uuid) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Ports Agent') RETURNING id",
    )
    .bind(format!("agent-{}@example.com", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap();
    let seed_pool = db::testing::seed_pool();
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'agent', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&seed_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_availability (tenant_id, membership_id, state) \
         VALUES ($1, $2, 'available')",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .execute(&seed_pool)
    .await
    .unwrap();
    membership_id
}

async fn seed_conversation(tenant_id: Uuid) -> Uuid {
    let seed_pool = db::testing::seed_pool();
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'Ports Customer') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&seed_pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&seed_pool)
    .await
    .unwrap()
}

async fn route(
    pool: &PgPool,
    ports: &AiPorts,
    tenant_id: Uuid,
    conversation_id: Uuid,
    skills: &[Uuid],
) -> Result<Value, EscalationError> {
    let mut tx = db::UnitOfWork::begin_for_tenant(pool, tenant_id)
        .await
        .unwrap();
    let escalation = ports
        .escalation
        .route_new_escalation(
            &mut tx,
            tenant_id,
            conversation_id,
            "Customer asked for a person",
            skills,
            Uuid::nil(),
        )
        .await?;
    tx.commit().await.unwrap();
    Ok(escalation)
}

/// With nobody present the escalation waits in the queue; with an agent connected it goes to them.
/// The adapter, not `ai`, knows who is present.
#[tokio::test]
async fn an_escalation_goes_to_a_present_agent_or_waits_in_the_queue() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let tenant = seed_tenant(&pool).await;
    let agent = seed_agent(&pool, tenant).await;
    let presence = Runtime::new(pool.clone(), Duration::from_secs(45));
    let ports = ports_with(&pool, &presence);

    let queued = route(&pool, &ports, tenant, seed_conversation(tenant).await, &[])
        .await
        .unwrap();
    assert_eq!(queued["status"], "queued", "nobody is connected: {queued}");
    assert_eq!(queued["routing"], Value::Null);
    assert_eq!(queued["reason"], "Customer asked for a person");

    let (_guard, _rx) = presence.connect(tenant, agent);
    let conversation = seed_conversation(tenant).await;
    let assigned = route(&pool, &ports, tenant, conversation, &[])
        .await
        .unwrap();
    assert_eq!(
        assigned["status"], "assigned",
        "an agent is present: {assigned}"
    );
    assert_eq!(
        assigned["routing"]["assignedMembershipId"],
        json!(agent),
        "assigned to the agent who is connected"
    );
    assert_eq!(assigned["conversationId"], json!(conversation));
}

/// The skills the caller asks for reach the router: the agent who has them is chosen over a
/// present agent who has not.
#[tokio::test]
async fn the_required_skills_reach_the_router() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let tenant = seed_tenant(&pool).await;
    let (without, with) = (
        seed_agent(&pool, tenant).await,
        seed_agent(&pool, tenant).await,
    );
    let seed_pool = db::testing::seed_pool();
    let skill: Uuid = sqlx::query_scalar(
        "INSERT INTO skills (tenant_id, name) VALUES ($1, 'billing') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&seed_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_skills (tenant_id, membership_id, skill_id) VALUES ($1, $2, $3)",
    )
    .bind(tenant)
    .bind(with)
    .bind(skill)
    .execute(&seed_pool)
    .await
    .unwrap();
    let presence = Runtime::new(pool.clone(), Duration::from_secs(45));
    let ports = ports_with(&pool, &presence);
    let (_g1, _r1) = presence.connect(tenant, without);
    let (_g2, _r2) = presence.connect(tenant, with);

    let escalation = route(
        &pool,
        &ports,
        tenant,
        seed_conversation(tenant).await,
        &[skill],
    )
    .await
    .unwrap();

    assert_eq!(escalation["routing"]["assignedMembershipId"], json!(with));
    assert_eq!(escalation["routing"]["matchedSkills"], json!(["billing"]));
    assert_eq!(escalation["requiredSkills"][0]["id"], json!(skill));
}

/// The escalation is the caller's: a rollback takes it, a commit keeps it, a second one for the
/// same conversation is refused, and another tenant's conversation is not reachable.
#[tokio::test]
async fn routing_follows_the_callers_transaction_and_stays_in_its_tenant() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let (a, b) = (seed_tenant(&pool).await, seed_tenant(&pool).await);
    let presence = Runtime::new(pool.clone(), Duration::from_secs(45));
    let ports = ports_with(&pool, &presence);
    let conversation = seed_conversation(a).await;
    let open = |tenant: Uuid, conversation: Uuid| {
        let (pool, ports) = (pool.clone(), ports.clone());
        async move {
            ports
                .escalation
                .has_open_escalation(&pool, tenant, conversation)
                .await
                .unwrap()
        }
    };

    // Rolled back with the caller.
    let mut tx = db::UnitOfWork::begin_for_tenant(&pool, a).await.unwrap();
    ports
        .escalation
        .route_new_escalation(&mut tx, a, conversation, "r", &[], Uuid::nil())
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(
        !open(a, conversation).await,
        "a rollback takes the escalation"
    );

    // Committed.
    route(&pool, &ports, a, conversation, &[]).await.unwrap();
    assert!(open(a, conversation).await);
    assert!(
        !open(b, conversation).await,
        "another tenant does not see it"
    );

    // A second escalation for the same conversation is refused.
    assert!(matches!(
        route(&pool, &ports, a, conversation, &[]).await,
        Err(EscalationError::Duplicate)
    ));

    // Tenant B cannot escalate tenant A's conversation, naming either tenant: the request fails
    // (which error says why is the escalations module's own) and nothing is created.
    let other = seed_conversation(a).await;
    for named in [a, b] {
        let mut tx = db::UnitOfWork::begin_for_tenant(&pool, b).await.unwrap();
        let result = ports
            .escalation
            .route_new_escalation(&mut tx, named, other, "r", &[], Uuid::nil())
            .await;
        assert!(
            result.is_err(),
            "tenant B must not escalate tenant A's conversation (naming {named}): {result:?}"
        );
        tx.rollback().await.unwrap();
    }
    assert!(!open(a, other).await, "no escalation was created for it");
}
