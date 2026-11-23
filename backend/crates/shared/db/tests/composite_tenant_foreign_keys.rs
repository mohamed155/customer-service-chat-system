//! Catalog and row-level proof for the composite tenant foreign keys added by MOH-98.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use db::testing::{become_owner, become_platform, owner_tx};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

type OwnerTx = Transaction<'static, Postgres>;
type ForeignKeyCatalogRow = (
    String,
    String,
    String,
    Vec<String>,
    Vec<String>,
    String,
    Vec<String>,
);

static OWNER_TX_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn can_run() -> bool {
    if std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
        || std::env::var("MIGRATION_DATABASE_URL").is_ok_and(|url| !url.trim().is_empty())
        || std::env::var("DATABASE_URL").is_ok_and(|url| !url.trim().is_empty())
    {
        return true;
    }
    eprintln!(
        "skipping composite tenant foreign key test: MIGRATION_DATABASE_URL or DATABASE_URL not set"
    );
    false
}

struct Fixtures {
    tenant_a: Uuid,
    agent_configuration_a: Uuid,
    agent_configuration_b: Uuid,
    agent_prompt_a: Uuid,
    agent_prompt_b: Uuid,
    usage_record_a: Uuid,
    usage_record_b: Uuid,
    conversation_a: Uuid,
    conversation_b: Uuid,
    message_a: Uuid,
    message_b: Uuid,
    generation_a: Uuid,
    generation_b: Uuid,
    widget_instance_a: Uuid,
    widget_instance_b: Uuid,
    widget_session_a: Uuid,
    widget_session_b: Uuid,
    membership_a: Uuid,
    membership_b: Uuid,
    integration_connection_a: Uuid,
    integration_connection_b: Uuid,
    knowledge_item_a: Uuid,
    knowledge_item_b: Uuid,
    knowledge_category_a: Uuid,
    knowledge_category_b: Uuid,
    customer_a: Uuid,
    customer_b: Uuid,
    tenant_tool_a: Uuid,
    tenant_tool_b: Uuid,
}

async fn fixtures(tx: &mut OwnerTx) -> Fixtures {
    become_platform(tx).await;
    let tenant_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenants (name, slug) VALUES ('MOH-98 A', 'moh98-a-' || uuidv7()::text) RETURNING id",
    )
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let tenant_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenants (name, slug) VALUES ('MOH-98 B', 'moh98-b-' || uuidv7()::text) RETURNING id",
    )
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    let user_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, display_name) VALUES ('moh98-a-' || uuidv7()::text || '@example.test', 'MOH-98 A') RETURNING id",
    )
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let user_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, display_name) VALUES ('moh98-b-' || uuidv7()::text || '@example.test', 'MOH-98 B') RETURNING id",
    )
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let membership_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent') RETURNING id",
    )
    .bind(tenant_a)
    .bind(user_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let membership_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent') RETURNING id",
    )
    .bind(tenant_b)
    .bind(user_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    let customer_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'MOH-98 A') RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let customer_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'MOH-98 B') RETURNING id",
    )
    .bind(tenant_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let conversation_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let conversation_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_b)
    .bind(customer_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let message_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) VALUES ($1, $2, 'customer', 'trigger A') RETURNING id",
    )
    .bind(tenant_a)
    .bind(conversation_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let message_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) VALUES ($1, $2, 'customer', 'trigger B') RETURNING id",
    )
    .bind(tenant_b)
    .bind(conversation_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    let agent_configuration_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO agent_configurations (tenant_id, name) VALUES ($1, 'MOH-98 A') RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let agent_configuration_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO agent_configurations (tenant_id, name) VALUES ($1, 'MOH-98 B') RETURNING id",
    )
    .bind(tenant_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let agent_prompt_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO agent_prompts (tenant_id, active_version) VALUES ($1, 1) RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let agent_prompt_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO agent_prompts (tenant_id, active_version) VALUES ($1, 1) RETURNING id",
    )
    .bind(tenant_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    let usage_record_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ai_usage_records (tenant_id, provider, model, status, streamed, latency_ms) VALUES ($1, 'openai', 'moh98-test', 'success', false, 0) RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let usage_record_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ai_usage_records (tenant_id, provider, model, status, streamed, latency_ms) VALUES ($1, 'openai', 'moh98-test', 'success', false, 0) RETURNING id",
    )
    .bind(tenant_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let generation_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, outcome, latency_ms) VALUES ($1, $2, $3, 'success', 0) RETURNING id",
    )
    .bind(tenant_a)
    .bind(conversation_a)
    .bind(message_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let generation_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, outcome, latency_ms) VALUES ($1, $2, $3, 'success', 0) RETURNING id",
    )
    .bind(tenant_b)
    .bind(conversation_b)
    .bind(message_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    let widget_instance_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO widget_instances (tenant_id, public_id, name) VALUES ($1, 'moh98-' || uuidv7()::text, 'MOH-98 A') RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let widget_instance_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO widget_instances (tenant_id, public_id, name) VALUES ($1, 'moh98-' || uuidv7()::text, 'MOH-98 B') RETURNING id",
    )
    .bind(tenant_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let widget_session_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, customer_id, expires_at) VALUES ($1, $2, uuid_send(uuidv7()), $3, now() + interval '1 day') RETURNING id",
    )
    .bind(tenant_a)
    .bind(widget_instance_a)
    .bind(customer_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let widget_session_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, customer_id, expires_at) VALUES ($1, $2, uuid_send(uuidv7()), $3, now() + interval '1 day') RETURNING id",
    )
    .bind(tenant_b)
    .bind(widget_instance_b)
    .bind(customer_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    let integration_catalog_id =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM integration_catalog ORDER BY slug LIMIT 1")
            .fetch_one(&mut **tx)
            .await
            .unwrap();
    let integration_connection_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO integration_connections (tenant_id, catalog_id, webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) VALUES ($1, $2, uuid_send(uuidv7()), decode('00', 'hex'), decode('01', 'hex')) RETURNING id",
    )
    .bind(tenant_a)
    .bind(integration_catalog_id)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let integration_connection_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO integration_connections (tenant_id, catalog_id, webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) VALUES ($1, $2, uuid_send(uuidv7()), decode('02', 'hex'), decode('03', 'hex')) RETURNING id",
    )
    .bind(tenant_b)
    .bind(integration_catalog_id)
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    let knowledge_category_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_categories (tenant_id, name) VALUES ($1, 'MOH-98 A') RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let knowledge_category_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_categories (tenant_id, name) VALUES ($1, 'MOH-98 B') RETURNING id",
    )
    .bind(tenant_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let knowledge_item_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, source, created_by_display) VALUES ($1, 'article', 'MOH-98 A', 'authored', 'test') RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let knowledge_item_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, source, created_by_display) VALUES ($1, 'article', 'MOH-98 B', 'authored', 'test') RETURNING id",
    )
    .bind(tenant_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    let tenant_tool_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenant_tools (tenant_id, name, description, input_schema, endpoint_url, created_by_membership_id) VALUES ($1, 'moh98-a', 'fixture', '{}'::jsonb, 'https://example.test/a', $2) RETURNING id",
    )
    .bind(tenant_a)
    .bind(membership_a)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    let tenant_tool_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenant_tools (tenant_id, name, description, input_schema, endpoint_url, created_by_membership_id) VALUES ($1, 'moh98-b', 'fixture', '{}'::jsonb, 'https://example.test/b', $2) RETURNING id",
    )
    .bind(tenant_b)
    .bind(membership_b)
    .fetch_one(&mut **tx)
    .await
    .unwrap();

    Fixtures {
        tenant_a,
        agent_configuration_a,
        agent_configuration_b,
        agent_prompt_a,
        agent_prompt_b,
        usage_record_a,
        usage_record_b,
        conversation_a,
        conversation_b,
        message_a,
        message_b,
        generation_a,
        generation_b,
        widget_instance_a,
        widget_instance_b,
        widget_session_a,
        widget_session_b,
        membership_a,
        membership_b,
        integration_connection_a,
        integration_connection_b,
        knowledge_item_a,
        knowledge_item_b,
        knowledge_category_a,
        knowledge_category_b,
        customer_a,
        customer_b,
        tenant_tool_a,
        tenant_tool_b,
    }
}

fn assert_fk_violation<T>(result: Result<T, sqlx::Error>, relationship: &str) {
    match result {
        Ok(_) => panic!("cross-tenant reference for {relationship} was accepted"),
        Err(sqlx::Error::Database(error)) => assert_eq!(
            error.code().as_deref(),
            Some("23503"),
            "{relationship} failed for a reason other than its foreign key: {error}"
        ),
        Err(error) => panic!("{relationship} failed with a non-database error: {error}"),
    }
}

macro_rules! assert_cross_tenant_rejected {
    ($tx:ident, $relationship:literal, $sql:literal $(, $value:expr)* $(,)?) => {{
        sqlx::query("SAVEPOINT moh98_fk_attempt")
            .execute(&mut *$tx)
            .await
            .unwrap();
        let result = sqlx::query($sql)
            $(.bind($value))*
            .execute(&mut *$tx)
            .await;
        assert_fk_violation(result, $relationship);
        sqlx::query("ROLLBACK TO SAVEPOINT moh98_fk_attempt")
            .execute(&mut *$tx)
            .await
            .unwrap();
        sqlx::query("RELEASE SAVEPOINT moh98_fk_attempt")
            .execute(&mut *$tx)
            .await
            .unwrap();
    }};
}

macro_rules! assert_same_tenant_insert_succeeds {
    ($tx:ident, $relationship:literal, $sql:literal $(, $value:expr)* $(,)?) => {{
        sqlx::query($sql)
            $(.bind($value))*
            .execute(&mut *$tx)
            .await
            .unwrap_or_else(|error| panic!("same-tenant reference for {} failed: {error}", $relationship));
    }};
}

#[tokio::test]
async fn all_fourteen_parent_kinds_reject_cross_tenant_child_rows() {
    if !can_run() {
        return;
    }
    let _guard = OWNER_TX_TEST_LOCK.lock().await;
    let mut tx = owner_tx().await;
    let f = fixtures(&mut tx).await;

    assert_cross_tenant_rejected!(
        tx,
        "agent_avatar_uploads.agent_id -> agent_configurations",
        "INSERT INTO agent_avatar_uploads (tenant_id, agent_id, content_type, bytes) VALUES ($1, $2, 'image/png', decode('00', 'hex'))",
        f.tenant_a,
        f.agent_configuration_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "agent_avatar_uploads.agent_id -> agent_configurations",
        "INSERT INTO agent_avatar_uploads (tenant_id, agent_id, content_type, bytes) VALUES ($1, $2, 'image/png', decode('01', 'hex'))",
        f.tenant_a,
        f.agent_configuration_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "agent_prompt_versions.prompt_id -> agent_prompts",
        "INSERT INTO agent_prompt_versions (tenant_id, prompt_id, version_number, content, created_by_display) VALUES ($1, $2, 1, 'fixture', 'test')",
        f.tenant_a,
        f.agent_prompt_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "agent_prompt_versions.prompt_id -> agent_prompts",
        "INSERT INTO agent_prompt_versions (tenant_id, prompt_id, version_number, content, created_by_display) VALUES ($1, $2, 1, 'fixture', 'test')",
        f.tenant_a,
        f.agent_prompt_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "tool_requests.generation_id -> ai_generations",
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, arguments, status, approval_required, chain_index) VALUES ($1, $2, $3, 'fixture', 'builtin', '{}'::jsonb, 'pending', true, 0)",
        f.tenant_a,
        f.conversation_a,
        f.generation_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "tool_requests.generation_id -> ai_generations",
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, arguments, status, approval_required, chain_index) VALUES ($1, $2, $3, 'fixture', 'builtin', '{}'::jsonb, 'pending', true, 0)",
        f.tenant_a,
        f.conversation_a,
        f.generation_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "ai_generations.usage_record_id -> ai_usage_records",
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, usage_record_id, outcome, latency_ms) VALUES ($1, $2, $3, $4, 'success', 0)",
        f.tenant_a,
        f.conversation_a,
        f.message_a,
        f.usage_record_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "ai_generations.usage_record_id -> ai_usage_records",
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, usage_record_id, outcome, latency_ms) VALUES ($1, $2, $3, $4, 'success', 0)",
        f.tenant_a,
        f.conversation_a,
        f.message_a,
        f.usage_record_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "tool_requests.conversation_id -> conversations",
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, arguments, status, approval_required, chain_index) VALUES ($1, $2, $3, 'fixture', 'builtin', '{}'::jsonb, 'pending', true, 1)",
        f.tenant_a,
        f.conversation_b,
        f.generation_a,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "tool_requests.conversation_id -> conversations",
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, arguments, status, approval_required, chain_index) VALUES ($1, $2, $3, 'fixture', 'builtin', '{}'::jsonb, 'pending', true, 1)",
        f.tenant_a,
        f.conversation_a,
        f.generation_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "message_citations.message_id -> messages",
        "INSERT INTO message_citations (tenant_id, message_id, knowledge_item_id, item_title, passage_text, relevance_score, ordinal) VALUES ($1, $2, $3, 'fixture', 'passage', 0.5, 0)",
        f.tenant_a,
        f.message_b,
        f.knowledge_item_a,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "message_citations.message_id -> messages",
        "INSERT INTO message_citations (tenant_id, message_id, knowledge_item_id, item_title, passage_text, relevance_score, ordinal) VALUES ($1, $2, $3, 'fixture', 'passage', 0.5, 0)",
        f.tenant_a,
        f.message_a,
        f.knowledge_item_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "conversation_feedback.widget_session_id -> widget_sessions",
        "INSERT INTO conversation_feedback (tenant_id, conversation_id, widget_session_id, channel, rating) VALUES ($1, $2, $3, 'widget', 5)",
        f.tenant_a,
        f.conversation_a,
        f.widget_session_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "conversation_feedback.widget_session_id -> widget_sessions",
        "INSERT INTO conversation_feedback (tenant_id, conversation_id, widget_session_id, channel, rating) VALUES ($1, $2, $3, 'widget', 5)",
        f.tenant_a,
        f.conversation_a,
        f.widget_session_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "conversations.widget_instance_id -> widget_instances",
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, widget_instance_id) VALUES ($1, $2, 'widget', 'open', $3)",
        f.tenant_a,
        f.customer_a,
        f.widget_instance_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "conversations.widget_instance_id -> widget_instances",
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, widget_instance_id) VALUES ($1, $2, 'widget', 'open', $3)",
        f.tenant_a,
        f.customer_a,
        f.widget_instance_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "notifications.recipient_membership_id -> tenant_memberships",
        "INSERT INTO notifications (tenant_id, recipient_membership_id, kind, title, subject_type, subject_id, dedupe_key) VALUES ($1, $2, 'conversation.assigned', 'fixture', 'conversation', $3, 'moh98-' || uuidv7()::text)",
        f.tenant_a,
        f.membership_b,
        f.conversation_a,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "notifications.recipient_membership_id -> tenant_memberships",
        "INSERT INTO notifications (tenant_id, recipient_membership_id, kind, title, subject_type, subject_id, dedupe_key) VALUES ($1, $2, 'conversation.assigned', 'fixture', 'conversation', $3, 'moh98-' || uuidv7()::text)",
        f.tenant_a,
        f.membership_a,
        f.conversation_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "integration_events.connection_id -> integration_connections",
        "INSERT INTO integration_events (tenant_id, connection_id, event_type, outcome) VALUES ($1, $2, 'connected', 'success')",
        f.tenant_a,
        f.integration_connection_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "integration_events.connection_id -> integration_connections",
        "INSERT INTO integration_events (tenant_id, connection_id, event_type, outcome) VALUES ($1, $2, 'connected', 'success')",
        f.tenant_a,
        f.integration_connection_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "knowledge_documents.item_id -> knowledge_items",
        "INSERT INTO knowledge_documents (item_id, tenant_id, storage_key, original_filename, content_type, size_bytes) VALUES ($1, $2, 'moh98/' || uuidv7()::text, 'fixture.pdf', 'application/pdf', 1)",
        f.knowledge_item_b,
        f.tenant_a,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "knowledge_documents.item_id -> knowledge_items",
        "INSERT INTO knowledge_documents (item_id, tenant_id, storage_key, original_filename, content_type, size_bytes) VALUES ($1, $2, 'moh98/' || uuidv7()::text, 'fixture.pdf', 'application/pdf', 1)",
        f.knowledge_item_a,
        f.tenant_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "knowledge_items.category_id -> knowledge_categories",
        "INSERT INTO knowledge_items (tenant_id, item_type, title, category_id, source, created_by_display) VALUES ($1, 'article', 'MOH-98 cross category', $2, 'authored', 'test')",
        f.tenant_a,
        f.knowledge_category_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "knowledge_items.category_id -> knowledge_categories",
        "INSERT INTO knowledge_items (tenant_id, item_type, title, category_id, source, created_by_display) VALUES ($1, 'article', 'MOH-98 same category', $2, 'authored', 'test')",
        f.tenant_a,
        f.knowledge_category_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "widget_sessions.customer_id -> customers",
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, customer_id, expires_at) VALUES ($1, $2, uuid_send(uuidv7()), $3, now() + interval '1 day')",
        f.tenant_a,
        f.widget_instance_a,
        f.customer_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "widget_sessions.customer_id -> customers",
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, customer_id, expires_at) VALUES ($1, $2, uuid_send(uuidv7()), $3, now() + interval '1 day')",
        f.tenant_a,
        f.widget_instance_a,
        f.customer_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "widget_sessions.widget_instance_id -> widget_instances",
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, customer_id, expires_at) VALUES ($1, $2, uuid_send(uuidv7()), $3, now() + interval '1 day')",
        f.tenant_a,
        f.widget_instance_b,
        f.customer_a,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "widget_sessions.widget_instance_id -> widget_instances",
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, customer_id, expires_at) VALUES ($1, $2, uuid_send(uuidv7()), $3, now() + interval '1 day')",
        f.tenant_a,
        f.widget_instance_a,
        f.customer_a,
    );

    assert_cross_tenant_rejected!(
        tx,
        "tool_requests.tenant_tool_id -> tenant_tools",
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, tenant_tool_id, arguments, status, approval_required, chain_index) VALUES ($1, $2, $3, 'moh98-b', 'tenant', $4, '{}'::jsonb, 'pending', true, 2)",
        f.tenant_a,
        f.conversation_a,
        f.generation_a,
        f.tenant_tool_b,
    );
    assert_same_tenant_insert_succeeds!(
        tx,
        "tool_requests.tenant_tool_id -> tenant_tools",
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, tenant_tool_id, arguments, status, approval_required, chain_index) VALUES ($1, $2, $3, 'moh98-a', 'tenant', $4, '{}'::jsonb, 'pending', true, 2)",
        f.tenant_a,
        f.conversation_a,
        f.generation_a,
        f.tenant_tool_a,
    );
}

#[tokio::test]
async fn all_inventory_foreign_keys_are_composite_in_the_catalog() {
    if !can_run() {
        return;
    }
    let _guard = OWNER_TX_TEST_LOCK.lock().await;
    let mut tx = owner_tx().await;
    let rows: Vec<ForeignKeyCatalogRow> = sqlx::query_as(
        "WITH tenant_children AS (\
             SELECT c.oid, c.relname::text, tenant.attnum AS tenant_id_attnum \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_attribute tenant ON tenant.attrelid = c.oid \
               AND tenant.attname = 'tenant_id' AND NOT tenant.attisdropped \
             WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p') \
               AND NOT c.relispartition AND NOT c.relname = ANY($1) \
         ), tenant_parents AS (\
             SELECT c.oid, c.relname::text, tenant.attnum AS tenant_id_attnum \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_attribute tenant ON tenant.attrelid = c.oid \
               AND tenant.attname = 'tenant_id' AND NOT tenant.attisdropped \
             WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p') \
               AND NOT c.relname = ANY($1) \
         ) \
         SELECT child.relname, con.conname::text, parent.relname, \
                ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(attnum, ord) \
                      JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum \
                      ORDER BY k.ord), \
                ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY k(attnum, ord) \
                      JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.attnum \
                      ORDER BY k.ord), \
                con.confdeltype::text, \
                ARRAY(SELECT a.attname::text FROM unnest(con.confdelsetcols) WITH ORDINALITY k(attnum, ord) \
                      JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum \
                      ORDER BY k.ord) \
         FROM pg_constraint con \
         JOIN tenant_children child ON child.oid = con.conrelid \
         JOIN tenant_parents parent ON parent.oid = con.confrelid \
         WHERE con.contype = 'f' AND con.conparentid = 0 \
         ORDER BY child.relname COLLATE \"C\", con.conname COLLATE \"C\"",
    )
    .bind(db::testing::isolation::GLOBAL_TABLES)
    .fetch_all(&mut *tx)
    .await
    .unwrap();

    let expected = [
        (
            "agent_avatar_uploads",
            "agent_id",
            "agent_configurations",
            "c",
        ),
        ("agent_prompt_versions", "prompt_id", "agent_prompts", "r"),
        ("ai_generations", "conversation_id", "conversations", "r"),
        ("ai_generations", "response_message_id", "messages", "n"),
        ("ai_generations", "trigger_message_id", "messages", "r"),
        ("ai_generations", "usage_record_id", "ai_usage_records", "n"),
        (
            "conversation_feedback",
            "agent_configuration_id",
            "agent_configurations",
            "r",
        ),
        (
            "conversation_feedback",
            "widget_session_id",
            "widget_sessions",
            "n",
        ),
        (
            "conversations",
            "widget_instance_id",
            "widget_instances",
            "n",
        ),
        (
            "integration_connections",
            "connected_by_membership_id",
            "tenant_memberships",
            "a",
        ),
        (
            "integration_connections",
            "disconnected_by_membership_id",
            "tenant_memberships",
            "a",
        ),
        (
            "integration_events",
            "actor_membership_id",
            "tenant_memberships",
            "a",
        ),
        (
            "integration_events",
            "connection_id",
            "integration_connections",
            "c",
        ),
        (
            "integration_secrets",
            "connection_id",
            "integration_connections",
            "c",
        ),
        (
            "integration_webhook_deliveries",
            "connection_id",
            "integration_connections",
            "c",
        ),
        ("knowledge_chunks", "item_id", "knowledge_items", "c"),
        ("knowledge_documents", "item_id", "knowledge_items", "c"),
        ("knowledge_index_state", "item_id", "knowledge_items", "c"),
        ("knowledge_item_tags", "item_id", "knowledge_items", "c"),
        (
            "knowledge_items",
            "category_id",
            "knowledge_categories",
            "n",
        ),
        ("message_citations", "message_id", "messages", "c"),
        (
            "notifications",
            "actor_membership_id",
            "tenant_memberships",
            "a",
        ),
        (
            "notifications",
            "recipient_membership_id",
            "tenant_memberships",
            "c",
        ),
        (
            "tenant_tool_policies",
            "updated_by_membership_id",
            "tenant_memberships",
            "a",
        ),
        (
            "tenant_tools",
            "created_by_membership_id",
            "tenant_memberships",
            "a",
        ),
        ("tool_requests", "conversation_id", "conversations", "a"),
        (
            "tool_requests",
            "decided_by_membership_id",
            "tenant_memberships",
            "a",
        ),
        ("tool_requests", "generation_id", "ai_generations", "a"),
        ("tool_requests", "tenant_tool_id", "tenant_tools", "a"),
        ("widget_sessions", "customer_id", "customers", "n"),
        (
            "widget_sessions",
            "widget_instance_id",
            "widget_instances",
            "r",
        ),
    ];
    let expected_set_null_columns = [
        ("ai_generations", "response_message_id"),
        ("ai_generations", "usage_record_id"),
        ("conversation_feedback", "widget_session_id"),
        ("conversations", "widget_instance_id"),
        ("knowledge_items", "category_id"),
        ("widget_sessions", "customer_id"),
    ];
    for (child, column, parent, delete_action) in expected {
        let mut name = format!("{child}_tenant_id_{column}_fkey");
        name.truncate(63); // PostgreSQL truncates identifiers to NAMEDATALEN - 1 bytes.
        let found = rows.iter().find(|row| row.0 == child && row.1 == name);
        let Some((
            _,
            _,
            found_parent,
            child_columns,
            parent_columns,
            found_action,
            set_null_columns,
        )) = found
        else {
            panic!("missing composite constraint {name}");
        };
        assert_eq!(found_parent, parent, "{name} references the wrong parent");
        assert_eq!(child_columns, &["tenant_id", column]);
        assert_eq!(parent_columns, &["tenant_id", "id"]);
        assert_eq!(
            found_action, delete_action,
            "{name} changed its ON DELETE action"
        );
        let expected_set_null = expected_set_null_columns
            .iter()
            .find(|(set_child, set_column)| *set_child == child && *set_column == column)
            .map(|(_, set_column)| vec![set_column.to_string()])
            .unwrap_or_default();
        assert_eq!(
            set_null_columns, &expected_set_null,
            "{name} has unexpected ON DELETE SET NULL columns"
        );
    }
    assert!(
        rows.iter()
            .all(|(_, constraint, _, child_columns, parent_columns, _, _)| {
                child_columns.len() == 2
                    && child_columns[0] == "tenant_id"
                    && parent_columns == &["tenant_id", "id"]
                    && !constraint.ends_with("_legacy_no_action_fkey")
            }),
        "tenant-owned foreign keys must all be composite; found {rows:?}"
    );
    let found_set_null_columns: Vec<(String, String)> = rows
        .iter()
        .filter(|row| !row.6.is_empty())
        .map(|row| (row.0.clone(), row.6[0].clone()))
        .collect();
    let expected_set_null_columns: Vec<(String, String)> = expected_set_null_columns
        .iter()
        .map(|(child, column)| (child.to_string(), column.to_string()))
        .collect();
    assert_eq!(found_set_null_columns, expected_set_null_columns);
}

#[tokio::test]
async fn deleting_a_knowledge_item_cascades_to_chunks() {
    if !can_run() {
        return;
    }
    let _guard = OWNER_TX_TEST_LOCK.lock().await;
    let mut tx = owner_tx().await;
    become_platform(&mut tx).await;
    let tenant_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenants (name, slug) VALUES ('MOH-98 cascade', 'moh98-cascade-' || uuidv7()::text) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let item_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, source, created_by_display) VALUES ($1, 'article', 'MOH-98 cascade', 'authored', 'test') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let embedding = format!("[{}]", vec!["0"; 1536].join(","));
    sqlx::query(
        "INSERT INTO knowledge_chunks (tenant_id, item_id, ordinal, content, embedding, content_hash) VALUES ($1, $2, 0, 'fixture chunk', $3::vector, $4)",
    )
    .bind(tenant_id)
    .bind(item_id)
    .bind(embedding)
    .bind(format!("moh98-{item_id}"))
    .execute(&mut *tx)
    .await
    .unwrap();

    sqlx::query("DELETE FROM knowledge_items WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(item_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let chunks = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM knowledge_chunks WHERE tenant_id = $1 AND item_id = $2",
    )
    .bind(tenant_id)
    .bind(item_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(chunks, 0);
}

#[tokio::test]
async fn deleting_a_response_message_nulls_only_its_reference() {
    if !can_run() {
        return;
    }
    let _guard = OWNER_TX_TEST_LOCK.lock().await;
    let mut tx = owner_tx().await;
    let f = fixtures(&mut tx).await;
    let response_message = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) VALUES ($1, $2, 'ai', 'fixture response') RETURNING id",
    )
    .bind(f.tenant_a)
    .bind(f.conversation_a)
        .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE ai_generations SET response_message_id = $1 WHERE id = $2")
        .bind(response_message)
        .bind(f.generation_a)
        .execute(&mut *tx)
        .await
        .unwrap();

    sqlx::query("DELETE FROM messages WHERE tenant_id = $1 AND id = $2")
        .bind(f.tenant_a)
        .bind(response_message)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (tenant_id, response_message_id): (Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT tenant_id, response_message_id FROM ai_generations WHERE id = $1")
            .bind(f.generation_a)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(tenant_id, f.tenant_a);
    assert_eq!(response_message_id, None);
}

#[tokio::test]
async fn deleting_a_prompt_with_versions_remains_restricted() {
    if !can_run() {
        return;
    }
    let _guard = OWNER_TX_TEST_LOCK.lock().await;
    let mut tx = owner_tx().await;
    let f = fixtures(&mut tx).await;
    sqlx::query(
        "INSERT INTO agent_prompt_versions (tenant_id, prompt_id, version_number, content, created_by_display) VALUES ($1, $2, 1, 'fixture prompt', 'test')",
    )
    .bind(f.tenant_a)
    .bind(f.agent_prompt_a)
        .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("SAVEPOINT moh98_restrict_attempt")
        .execute(&mut *tx)
        .await
        .unwrap();
    let result = sqlx::query("DELETE FROM agent_prompts WHERE tenant_id = $1 AND id = $2")
        .bind(f.tenant_a)
        .bind(f.agent_prompt_a)
        .execute(&mut *tx)
        .await;
    assert_fk_violation(result, "agent_prompt_versions.prompt_id delete restriction");
    sqlx::query("ROLLBACK TO SAVEPOINT moh98_restrict_attempt")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("RELEASE SAVEPOINT moh98_restrict_attempt")
        .execute(&mut *tx)
        .await
        .unwrap();
}

#[tokio::test]
async fn migration_preflight_reports_existing_cross_tenant_rows() {
    const MIGRATION: &str =
        include_str!("../../../../migrations/0092_composite_tenant_foreign_keys.sql");
    if !can_run() {
        return;
    }
    let _guard = OWNER_TX_TEST_LOCK.lock().await;
    let mut tx = owner_tx().await;
    let f = fixtures(&mut tx).await;
    become_owner(&mut tx).await;
    sqlx::query("ALTER TABLE agent_avatar_uploads DROP CONSTRAINT agent_avatar_uploads_tenant_id_agent_id_fkey")
        .execute(&mut *tx)
        .await
        .unwrap();
    become_platform(&mut tx).await;
    sqlx::query(
        "INSERT INTO agent_avatar_uploads (tenant_id, agent_id, content_type, bytes) VALUES ($1, $2, 'image/png', decode('00', 'hex'))",
    )
    .bind(f.tenant_a)
    .bind(f.agent_configuration_b)
    .execute(&mut *tx)
    .await
    .unwrap();

    let start = MIGRATION
        .find("DO $$\nDECLARE\n    ref record;")
        .expect("migration data preflight block");
    let end = start
        + MIGRATION[start..]
            .find("$$;")
            .expect("migration preflight terminator")
        + 3;
    let error = sqlx::query(&MIGRATION[start..end])
        .execute(&mut *tx)
        .await
        .expect_err("the migration preflight must reject the planted cross-tenant row");
    match error {
        sqlx::Error::Database(error) => {
            assert_eq!(error.code().as_deref(), Some("P0001"));
            assert!(
                error
                    .message()
                    .contains("agent_avatar_uploads.agent_id: 1 row(s)"),
                "unexpected preflight message: {}",
                error.message()
            );
        }
        error => panic!("unexpected preflight error: {error}"),
    }
}
