#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! The tool port `ai` declares (`ai::ports::ToolRuntime`) and the server's adapter for it
//! (`server::ai_ports`), MOH-195. `ai` no longer depends on `tools`, so these tests pin what the
//! wiring must keep, on real Postgres as the runtime role: the caller's transaction is the
//! caller's (an approval request and its event commit or roll back together, a cancellation
//! rolls back with the escalation that caused it), and no tenant reads or changes another's
//! requests.
#[path = "support/tool_fixtures.rs"]
mod tool_fixtures;

use std::time::Duration;

use ai::ports::{
    NewToolRequest, NewToolRequestStatus, ResolvedTool, ToolSource, ValidationFailure,
};
use serde_json::json;
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
            eprintln!("skipping ai_ports_tools tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping ai_ports_tools tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup() {
    db::testing::migrate().await;
    sqlx::query(
        "TRUNCATE TABLE ai_generations, ai_usage_records, ai_credentials, ai_configurations, \
         agent_configurations, agent_avatar_uploads, \
         escalations, agent_availability, agent_skills, skills, \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users, \
         tool_requests, tenant_tool_policies \
         RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
}

struct Fixture {
    tenant_id: Uuid,
    conversation_id: Uuid,
    generation_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("Ports Tools Tenant")
            .bind(format!("ports-tools-{}", Uuid::new_v4().simple()))
            .fetch_one(pool)
            .await
            .unwrap();
    let seed_pool = db::testing::seed_pool();
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'Ports Customer') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&seed_pool)
    .await
    .unwrap();
    let conversation_id = new_conversation(&seed_pool, tenant_id, customer_id).await;
    let generation_id = tool_fixtures::seed_generation(pool, tenant_id, conversation_id).await;
    Fixture {
        tenant_id,
        conversation_id,
        generation_id,
    }
}

async fn new_conversation(pool: &PgPool, tenant_id: Uuid, customer_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A tenant-registered tool (the table requires one for a `tenant`-sourced request).
async fn seed_tenant_tool(tenant_id: Uuid) -> Uuid {
    let seed_pool = db::testing::seed_pool();
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Ports Tools User') RETURNING id",
    )
    .bind(format!(
        "ports-tools-{}@example.com",
        Uuid::new_v4().simple()
    ))
    .fetch_one(&seed_pool)
    .await
    .unwrap();
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'admin') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&seed_pool)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO tenant_tools \
         (tenant_id, name, description, input_schema, endpoint_url, classification, \
          created_by_membership_id) \
         VALUES ($1, 'track_order', 'Looks up an order', $2, \
          'https://example.com/hook', 'approval', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(json!({"type": "object"}))
    .bind(membership_id)
    .fetch_one(&seed_pool)
    .await
    .unwrap()
}

fn request(f: &Fixture, status: NewToolRequestStatus, source: ToolSource) -> NewToolRequest {
    NewToolRequest {
        tenant_id: f.tenant_id,
        conversation_id: f.conversation_id,
        generation_id: f.generation_id,
        tool_name: "update_customer_contact".into(),
        tool_source: source,
        tenant_tool_id: None,
        arguments: json!({"email": "anna@example.com"}),
        status,
        approval_required: status == NewToolRequestStatus::AwaitingApproval,
        expires_at: None,
        chain_index: 0,
    }
}

/// The ports the engine runs on, wired as the server wires them.
fn ports(pool: &PgPool) -> ai::ports::AiPorts {
    server::ai_ports::ai_ports(
        pool.clone(),
        escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
    )
}

/// Ports for the tests that never touch a database (the pool connects only when used).
fn ports_without_database() -> ai::ports::AiPorts {
    ports(&db::lazy_pool(
        "postgres://localhost:5432/unused",
        1,
        Duration::from_secs(1),
    ))
}

async fn status_of(id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM tool_requests WHERE id = $1")
        .bind(id)
        .fetch_optional(&db::testing::seed_pool())
        .await
        .unwrap()
}

fn tool(name: &str, source: ToolSource, tenant_tool_id: Option<Uuid>) -> ResolvedTool {
    ResolvedTool {
        spec: ai_providers::ToolSpec {
            name: name.into(),
            description: "Updates a customer".into(),
            input_schema: json!({
                "type": "object",
                "required": ["email"],
                "properties": {"email": {"type": "string"}}
            }),
        },
        source,
        approval_required: true,
        tenant_tool_id,
    }
}

#[tokio::test]
async fn validation_keeps_the_tool_and_names_why_a_call_cannot_run() {
    let ports = ports_without_database();
    let tenant_tool = Uuid::new_v4();
    let tools = vec![
        tool("update_customer_contact", ToolSource::Builtin, None),
        tool("track_order", ToolSource::Tenant, Some(tenant_tool)),
    ];

    // A valid call returns the tool as resolved: source, approval and tenant tool id intact.
    let ok = ports
        .tools
        .validate(&tools, "track_order", &json!({"email": "a@b.co"}))
        .unwrap();
    assert_eq!(ok, tools[1]);
    assert_eq!(ok.source, ToolSource::Tenant);
    assert_eq!(ok.tenant_tool_id, Some(tenant_tool));
    let builtin = ports
        .tools
        .validate(
            &tools,
            "update_customer_contact",
            &json!({"email": "a@b.co"}),
        )
        .unwrap();
    assert_eq!(builtin.source, ToolSource::Builtin);

    // No such tool, or not enabled for this tenant.
    assert_eq!(
        ports
            .tools
            .validate(&tools, "delete_everything", &json!({})),
        Err(ValidationFailure::UnknownOrDisabled)
    );
    assert_eq!(
        ports.tools.validate(&[], "track_order", &json!({})),
        Err(ValidationFailure::UnknownOrDisabled)
    );

    // Arguments that do not match the schema say how.
    match ports.tools.validate(&tools, "track_order", &json!({})) {
        Err(ValidationFailure::SchemaMismatch(detail)) => assert!(detail.contains("email")),
        other => panic!("expected a schema mismatch, got {other:?}"),
    }
    assert!(matches!(
        ports
            .tools
            .validate(&tools, "track_order", &json!({"email": 7})),
        Err(ValidationFailure::SchemaMismatch(_))
    ));
}

#[tokio::test]
async fn the_call_limit_is_the_tools_modules_own() {
    assert_eq!(
        ports_without_database().tools.max_calls_per_generation(),
        tools::executor::MAX_TOOL_CALLS_PER_GENERATION
    );
}

/// The approval request and `ai.tool_approval_requested` are written through the port in one
/// transaction (the fix of #140): a rollback takes both, a commit keeps both. If the adapter
/// opened a transaction of its own, the request would survive the rollback.
#[tokio::test]
async fn a_request_inserted_through_the_port_follows_the_callers_transaction() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let f = seed(&pool).await;
    let ports = ports(&pool);

    let event_count = |tool_request_id: Uuid| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM outbox_events \
             WHERE event_type = 'ai.tool_approval_requested' AND aggregate_id = $1",
        )
        .bind(tool_request_id.to_string())
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap()
    };

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, f.tenant_id)
        .await
        .unwrap();
    let rolled_back = ports
        .tools
        .insert_request_in_tx(
            uow.conn(),
            request(
                &f,
                NewToolRequestStatus::AwaitingApproval,
                ToolSource::Builtin,
            ),
        )
        .await
        .unwrap();
    ai::outbox::emit_tool_approval_requested_in_tx(
        uow.conn(),
        f.tenant_id,
        rolled_back,
        f.conversation_id,
    )
    .await
    .unwrap();
    assert_eq!(
        status_of(rolled_back).await,
        None,
        "nothing before the commit"
    );
    uow.rollback().await.unwrap();
    assert_eq!(
        status_of(rolled_back).await,
        None,
        "a rollback takes the request"
    );
    assert_eq!(event_count(rolled_back).await, 0, "and its event");

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, f.tenant_id)
        .await
        .unwrap();
    let committed = ports
        .tools
        .insert_request_in_tx(
            uow.conn(),
            request(
                &f,
                NewToolRequestStatus::AwaitingApproval,
                ToolSource::Builtin,
            ),
        )
        .await
        .unwrap();
    ai::outbox::emit_tool_approval_requested_in_tx(
        uow.conn(),
        f.tenant_id,
        committed,
        f.conversation_id,
    )
    .await
    .unwrap();
    uow.commit().await.unwrap();
    assert_eq!(
        status_of(committed).await.as_deref(),
        Some("awaiting_approval")
    );
    assert_eq!(event_count(committed).await, 1);

    // Drain nothing else: leave no AI event for the next test's worker call to claim first.
    sqlx::query("DELETE FROM outbox_events WHERE event_type = 'ai.tool_approval_requested'")
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn a_request_is_found_by_its_tenant_and_never_by_another() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let a = seed(&pool).await;
    let b = seed(&pool).await;
    let ports = ports(&pool);

    let tenant_tool_id = seed_tenant_tool(a.tenant_id).await;
    let tenant_request = ports
        .tools
        .insert_request(
            &pool,
            NewToolRequest {
                tenant_tool_id: Some(tenant_tool_id),
                ..request(&a, NewToolRequestStatus::Pending, ToolSource::Tenant)
            },
        )
        .await
        .unwrap();
    let builtin_request = ports
        .tools
        .insert_request(
            &pool,
            request(&a, NewToolRequestStatus::Pending, ToolSource::Builtin),
        )
        .await
        .unwrap();

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, a.tenant_id)
        .await
        .unwrap();
    let found = ports
        .tools
        .find_request(uow.conn(), a.tenant_id, tenant_request)
        .await
        .unwrap()
        .expect("the owning tenant finds its request");
    assert_eq!(found.generation_id, a.generation_id);
    assert_eq!(found.tool_name, "update_customer_contact");
    assert_eq!(found.tool_source, ToolSource::Tenant);
    assert_eq!(found.tenant_tool_id, Some(tenant_tool_id));
    assert_eq!(found.arguments, json!({"email": "anna@example.com"}));
    let builtin = ports
        .tools
        .find_request(uow.conn(), a.tenant_id, builtin_request)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(builtin.tool_source, ToolSource::Builtin);
    uow.rollback().await.unwrap();

    // Tenant B asks for tenant A's request, naming either tenant: it is not there.
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, b.tenant_id)
        .await
        .unwrap();
    for named in [b.tenant_id, a.tenant_id] {
        assert!(
            ports
                .tools
                .find_request(uow.conn(), named, tenant_request)
                .await
                .unwrap()
                .is_none(),
            "tenant B must not read tenant A's request (naming {named})"
        );
    }
    uow.rollback().await.unwrap();
}

/// Cancelling runs in the caller's transaction (it rolls back with the escalation that caused it),
/// touches only that conversation's calls that still wait, and reports exactly those.
#[tokio::test]
async fn cancelling_follows_the_callers_transaction_and_stays_in_its_conversation() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let f = seed(&pool).await;
    let other = seed(&pool).await;
    let ports = ports(&pool);

    let waiting = ports
        .tools
        .insert_request(
            &pool,
            request(
                &f,
                NewToolRequestStatus::AwaitingApproval,
                ToolSource::Builtin,
            ),
        )
        .await
        .unwrap();
    let finished = ports
        .tools
        .insert_request(
            &pool,
            request(&f, NewToolRequestStatus::Pending, ToolSource::Builtin),
        )
        .await
        .unwrap();
    let elsewhere = ports
        .tools
        .insert_request(
            &pool,
            request(
                &other,
                NewToolRequestStatus::AwaitingApproval,
                ToolSource::Builtin,
            ),
        )
        .await
        .unwrap();

    let pending = ports
        .tools
        .awaiting_approval(&pool, f.tenant_id, f.conversation_id)
        .await
        .unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|p| (p.id, p.conversation_id))
            .collect::<Vec<_>>(),
        [(waiting, f.conversation_id)],
        "only this conversation's call that still waits"
    );
    assert!(ports
        .tools
        .awaiting_approval(&pool, other.tenant_id, f.conversation_id)
        .await
        .unwrap()
        .is_empty());

    // Rolled back with the caller: the call still waits.
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, f.tenant_id)
        .await
        .unwrap();
    let ids = ports
        .tools
        .cancel_pending(&mut uow, f.tenant_id, f.conversation_id)
        .await
        .unwrap();
    assert_eq!(ids, [waiting]);
    uow.rollback().await.unwrap();
    assert_eq!(
        status_of(waiting).await.as_deref(),
        Some("awaiting_approval")
    );

    // Committed: it is cancelled; nothing else in the conversation or in another tenant changed.
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, f.tenant_id)
        .await
        .unwrap();
    let ids = ports
        .tools
        .cancel_pending(&mut uow, f.tenant_id, f.conversation_id)
        .await
        .unwrap();
    assert_eq!(ids, [waiting]);
    uow.commit().await.unwrap();
    assert_eq!(status_of(waiting).await.as_deref(), Some("cancelled"));
    assert_eq!(status_of(finished).await.as_deref(), Some("pending"));
    assert_eq!(
        status_of(elsewhere).await.as_deref(),
        Some("awaiting_approval"),
        "another tenant's request is untouched"
    );

    // Naming another tenant's conversation under this tenant's transaction cancels nothing.
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, f.tenant_id)
        .await
        .unwrap();
    assert!(ports
        .tools
        .cancel_pending(&mut uow, f.tenant_id, other.conversation_id)
        .await
        .unwrap()
        .is_empty());
    uow.rollback().await.unwrap();
    assert_eq!(
        status_of(elsewhere).await.as_deref(),
        Some("awaiting_approval")
    );
}

#[tokio::test]
async fn a_request_runs_pending_to_executing_to_its_outcome_for_its_own_tenant_only() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let a = seed(&pool).await;
    let b = seed(&pool).await;
    let ports = ports(&pool);
    let id = ports
        .tools
        .insert_request(
            &pool,
            request(&a, NewToolRequestStatus::Pending, ToolSource::Builtin),
        )
        .await
        .unwrap();

    assert!(
        !ports
            .tools
            .mark_executing(&pool, id, b.tenant_id)
            .await
            .unwrap(),
        "another tenant cannot start it"
    );
    assert_eq!(status_of(id).await.as_deref(), Some("pending"));
    assert!(ports
        .tools
        .mark_executing(&pool, id, a.tenant_id)
        .await
        .unwrap());
    assert_eq!(status_of(id).await.as_deref(), Some("executing"));

    assert!(
        !ports
            .tools
            .mark_terminal(&pool, id, b.tenant_id, "succeeded", None, None)
            .await
            .unwrap(),
        "another tenant cannot finish it"
    );
    assert!(ports
        .tools
        .mark_terminal(
            &pool,
            id,
            a.tenant_id,
            "succeeded",
            Some(json!({"ok": true})),
            None
        )
        .await
        .unwrap());
    let (status, result): (String, Option<serde_json::Value>) =
        sqlx::query_as("SELECT status, result FROM tool_requests WHERE id = $1")
            .bind(id)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(status, "succeeded");
    assert_eq!(result, Some(json!({"ok": true})));
}
