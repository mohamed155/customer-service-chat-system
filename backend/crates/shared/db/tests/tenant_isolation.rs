//! The shared tenant-isolation harness (ADR-004 section 7, gap T5; MOH-47).
//!
//! For every table under forced Row-Level Security this creates one row in each of two tenants
//! and runs `db::testing::isolation::assert_isolated` as `app_runtime`: read by guessed id, list,
//! update, delete and move to the other tenant as the wrong tenant, and read, update, delete and
//! insert with no tenant at all. Rows are created as the bypass role (`db::testing::seed_pool`),
//! which is test setup only; the checks run on the runtime pool.
//!
//! **Adding a table to Row-Level Security means adding it here.** The last test compares the list
//! below with the tables whose policy is forced and fails if they differ in either direction, so a
//! new group (MOH-48, MOH-49, ...) cannot be merged without being covered.
//!
//! Needs the role split (`MIGRATION_DATABASE_URL`); skips without it unless `REQUIRE_DB_TESTS=1`.
use db::testing::isolation::{assert_isolated, forced_rls_tables, Probe};
use sqlx::PgPool;
use uuid::Uuid;

fn can_run() -> bool {
    if std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
        || std::env::var("MIGRATION_DATABASE_URL").is_ok_and(|v| !v.is_empty())
    {
        return true;
    }
    eprintln!("skipping tenant isolation harness: MIGRATION_DATABASE_URL not set");
    false
}

async fn runtime_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    db::lazy_pool(&url, 3, std::time::Duration::from_secs(10))
}

/// One tenant's rows, one per table in the conversations/customers group.
struct Graph {
    tenant: Uuid,
    customer: Uuid,
    identifier: Uuid,
    conversation: Uuid,
    message: Uuid,
    attachment: Uuid,
    citation: Uuid,
    feedback: Uuid,
    escalation: Uuid,
    whatsapp_meta: Uuid,
    // knowledge, AI and tools (MOH-48)
    category: Uuid,
    item: Uuid,
    chunk: Uuid,
    ai_config: Uuid,
    ai_credential: Uuid,
    usage: Uuid,
    generation: Uuid,
    agent: Uuid,
    prompt: Uuid,
    prompt_version: Uuid,
    avatar: Uuid,
    membership: Uuid,
    tool: Uuid,
    tool_policy: Uuid,
    tool_request: Uuid,
    // the remaining tenant-owned tables (MOH-49)
    user: Uuid,
    skill: Uuid,
    invitation: Uuid,
    audit: Uuid,
    outbox: Uuid,
    widget: Uuid,
    widget_session: Uuid,
    notification: Uuid,
    connection: Uuid,
    secret: Uuid,
    delivery: Uuid,
    integration_event: Uuid,
    subscription: Uuid,
    rate_limit: Uuid,
}

async fn scalar_uuid(seed: &PgPool, sql: &str, binds: &[Uuid]) -> Uuid {
    let mut q = sqlx::query_scalar::<_, Uuid>(sqlx::AssertSqlSafe(sql.to_string()));
    for b in binds {
        q = q.bind(*b);
    }
    q.fetch_one(seed)
        .await
        .unwrap_or_else(|e| panic!("seed failed: {sql}: {e}"))
}

async fn seed_graph(seed: &PgPool) -> Graph {
    let unique = Uuid::new_v4().simple().to_string();
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind(format!("Isolation {unique}"))
            .bind(format!("iso-{unique}"))
            .fetch_one(seed)
            .await
            .unwrap();
    let customer = scalar_uuid(
        seed,
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'C') RETURNING id",
        &[tenant],
    )
    .await;
    let identifier = scalar_uuid(
        seed,
        &format!(
            "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) \
             VALUES ($1, $2, 'email', 'c-{unique}@example.test') RETURNING id"
        ),
        &[tenant, customer],
    )
    .await;
    let conversation = scalar_uuid(
        seed,
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'widget', 'open') RETURNING id",
        &[tenant, customer],
    )
    .await;
    let message = scalar_uuid(
        seed,
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'hi') RETURNING id",
        &[tenant, conversation],
    )
    .await;
    let attachment = scalar_uuid(
        seed,
        "INSERT INTO message_attachments (tenant_id, message_id, kind) \
         VALUES ($1, $2, 'image') RETURNING id",
        &[tenant, message],
    )
    .await;
    let citation = scalar_uuid(
        seed,
        "INSERT INTO message_citations \
            (tenant_id, message_id, knowledge_item_id, item_title, passage_text, relevance_score, ordinal) \
         VALUES ($1, $2, gen_random_uuid(), 't', 'p', 0.5, 0) RETURNING id",
        &[tenant, message],
    )
    .await;
    let feedback = scalar_uuid(
        seed,
        "INSERT INTO conversation_feedback (tenant_id, conversation_id, channel, rating) \
         VALUES ($1, $2, 'widget', 5) RETURNING id",
        &[tenant, conversation],
    )
    .await;
    let escalation = scalar_uuid(
        seed,
        "INSERT INTO escalations (tenant_id, conversation_id, reason, status) \
         VALUES ($1, $2, 'needs a human', 'queued') RETURNING id",
        &[tenant, conversation],
    )
    .await;
    let whatsapp_meta = scalar_uuid(
        seed,
        "INSERT INTO whatsapp_message_meta (tenant_id, message_id, conversation_id, direction) \
         VALUES ($1, $2, $3, 'inbound') RETURNING id",
        &[tenant, message, conversation],
    )
    .await;
    let category = scalar_uuid(
        seed,
        "INSERT INTO knowledge_categories (tenant_id, name) VALUES ($1, 'Billing') RETURNING id",
        &[tenant],
    )
    .await;
    let item = scalar_uuid(
        seed,
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, status, source, created_by_display) \
         VALUES ($1, 'article', 'Title', 'Body', 'published', 'authored', 'Test') RETURNING id",
        &[tenant],
    )
    .await;
    scalar_uuid(
        seed,
        &format!(
            "INSERT INTO knowledge_documents (tenant_id, item_id, storage_key, original_filename, content_type, size_bytes) \
             VALUES ($1, $2, 'iso/{unique}/f.pdf', 'f.pdf', 'application/pdf', 10) RETURNING item_id"
        ),
        &[tenant, item],
    )
    .await;
    scalar_uuid(
        seed,
        "INSERT INTO knowledge_item_tags (tenant_id, item_id, tag) VALUES ($1, $2, 'billing') RETURNING item_id",
        &[tenant, item],
    )
    .await;
    let chunk = scalar_uuid(
        seed,
        "INSERT INTO knowledge_chunks (tenant_id, item_id, ordinal, content, embedding, content_hash) \
         VALUES ($1, $2, 0, 'chunk', array_fill(0.1::real, ARRAY[1536])::vector, 'h') RETURNING id",
        &[tenant, item],
    )
    .await;
    scalar_uuid(
        seed,
        "INSERT INTO knowledge_index_state (tenant_id, item_id, status) VALUES ($1, $2, 'indexed') RETURNING item_id",
        &[tenant, item],
    )
    .await;
    let ai_config = scalar_uuid(
        seed,
        "INSERT INTO ai_configurations (tenant_id, provider, model) VALUES ($1, 'openai', 'm') RETURNING id",
        &[tenant],
    )
    .await;
    let ai_credential = scalar_uuid(
        seed,
        "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) \
         VALUES ($1, 'openai', decode('01', 'hex'), decode('02', 'hex'), '...abcd') RETURNING id",
        &[tenant],
    )
    .await;
    let usage = scalar_uuid(
        seed,
        "INSERT INTO ai_usage_records (tenant_id, provider, model, status, streamed, latency_ms) \
         VALUES ($1, 'openai', 'm', 'success', false, 5) RETURNING id",
        &[tenant],
    )
    .await;
    let generation = scalar_uuid(
        seed,
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, outcome, latency_ms) \
         VALUES ($1, $2, $3, 'success', 1) RETURNING id",
        &[tenant, conversation, message],
    )
    .await;
    let agent = scalar_uuid(
        seed,
        "INSERT INTO agent_configurations (tenant_id, name) VALUES ($1, 'Agent') RETURNING id",
        &[tenant],
    )
    .await;
    let prompt = scalar_uuid(
        seed,
        "INSERT INTO agent_prompts (tenant_id, active_version) VALUES ($1, 1) RETURNING id",
        &[tenant],
    )
    .await;
    let prompt_version = scalar_uuid(
        seed,
        "INSERT INTO agent_prompt_versions (tenant_id, prompt_id, version_number, content, created_by_display) \
         VALUES ($1, $2, 1, 'You are helpful.', 'Test') RETURNING id",
        &[tenant, prompt],
    )
    .await;
    let avatar = scalar_uuid(
        seed,
        "INSERT INTO agent_avatar_uploads (tenant_id, agent_id, content_type, bytes) \
         VALUES ($1, $2, 'image/png', decode('00', 'hex')) RETURNING id",
        &[tenant, agent],
    )
    .await;
    let user = scalar_uuid(
        seed,
        &format!("INSERT INTO users (email, display_name) VALUES ('iso-{unique}@example.test', 'Iso') RETURNING id"),
        &[],
    )
    .await;
    let membership = scalar_uuid(
        seed,
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'admin') RETURNING id",
        &[tenant, user],
    )
    .await;
    let tool = scalar_uuid(
        seed,
        "INSERT INTO tenant_tools (tenant_id, name, description, input_schema, endpoint_url, created_by_membership_id) \
         VALUES ($1, 'lookup', 'd', '{}'::jsonb, 'https://example.test/t', $2) RETURNING id",
        &[tenant, membership],
    )
    .await;
    let tool_policy = scalar_uuid(
        seed,
        "INSERT INTO tenant_tool_policies (tenant_id, tool_name, updated_by_membership_id) \
         VALUES ($1, 'lookup', $2) RETURNING id",
        &[tenant, membership],
    )
    .await;
    let tool_request = scalar_uuid(
        seed,
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, arguments, status, approval_required, chain_index) \
         VALUES ($1, $2, $3, 'lookup', 'builtin', '{}'::jsonb, 'pending', false, 0) RETURNING id",
        &[tenant, conversation, generation],
    )
    .await;
    let skill = scalar_uuid(
        seed,
        "INSERT INTO skills (tenant_id, name) VALUES ($1, 'Billing') RETURNING id",
        &[tenant],
    )
    .await;
    scalar_uuid(
        seed,
        "INSERT INTO agent_skills (tenant_id, membership_id, skill_id) VALUES ($1, $2, $3) RETURNING skill_id",
        &[tenant, membership, skill],
    )
    .await;
    scalar_uuid(
        seed,
        "INSERT INTO agent_availability (tenant_id, membership_id) VALUES ($1, $2) RETURNING membership_id",
        &[tenant, membership],
    )
    .await;
    let invitation = scalar_uuid(
        seed,
        &format!(
            "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
             VALUES ($1, 'inv-{unique}@example.test', 'agent', 'hash-{unique}', $2, now() + interval '1 day') RETURNING id"
        ),
        &[tenant, user],
    )
    .await;
    let audit = scalar_uuid(
        seed,
        "INSERT INTO audit_logs (tenant_id, action, resource_type, resource_id) \
         VALUES ($1, 'isolation.test', 'tenant', 'x') RETURNING id",
        &[tenant],
    )
    .await;
    let outbox = scalar_uuid(
        seed,
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES (gen_random_uuid(), 'tenant', 'x', $1::uuid::text, 'isolation.test', '{}'::jsonb, now()) RETURNING id",
        &[tenant],
    )
    .await;
    let widget = scalar_uuid(
        seed,
        &format!(
            "INSERT INTO widget_instances (tenant_id, public_id, name) VALUES ($1, 'wgt_{unique}', 'W') RETURNING id"
        ),
        &[tenant],
    )
    .await;
    let widget_session = scalar_uuid(
        seed,
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, expires_at) \
         VALUES ($1, $2, decode(md5(random()::text), 'hex'), now() + interval '1 day') RETURNING id",
        &[tenant, widget],
    )
    .await;
    let notification = scalar_uuid(
        seed,
        &format!(
            "INSERT INTO notifications (tenant_id, recipient_membership_id, kind, title, subject_type, subject_id, dedupe_key) \
             VALUES ($1, $2, 'escalation.new', 'T', 'escalation', $3, 'k-{unique}') RETURNING id"
        ),
        &[tenant, membership, escalation],
    )
    .await;
    let connection = scalar_uuid(
        seed,
        "INSERT INTO integration_connections \
            (tenant_id, catalog_id, webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) \
         VALUES ($1, (SELECT id FROM integration_catalog ORDER BY slug LIMIT 1), \
                 decode(md5(random()::text), 'hex'), decode('01', 'hex'), decode('02', 'hex')) RETURNING id",
        &[tenant],
    )
    .await;
    let secret = scalar_uuid(
        seed,
        "INSERT INTO integration_secrets (tenant_id, connection_id, field_key, ciphertext, nonce, hint) \
         VALUES ($1, $2, 'api_key', decode('01', 'hex'), decode('02', 'hex'), '...abcd') RETURNING id",
        &[tenant, connection],
    )
    .await;
    let delivery = scalar_uuid(
        seed,
        "INSERT INTO integration_webhook_deliveries (tenant_id, connection_id, payload) \
         VALUES ($1, $2, '{}'::jsonb) RETURNING id",
        &[tenant, connection],
    )
    .await;
    let integration_event = scalar_uuid(
        seed,
        "INSERT INTO integration_events (tenant_id, connection_id, event_type, outcome) \
         VALUES ($1, $2, 'connected', 'success') RETURNING id",
        &[tenant, connection],
    )
    .await;
    let subscription = scalar_uuid(
        seed,
        "INSERT INTO tenant_subscriptions (tenant_id, plan_id) \
         VALUES ($1, (SELECT id FROM billing_plans ORDER BY created_at LIMIT 1)) RETURNING id",
        &[tenant],
    )
    .await;
    let rate_limit = scalar_uuid(
        seed,
        "INSERT INTO tenant_rate_limit_overrides (tenant_id, scope, max_requests, window_seconds) \
         VALUES ($1, 'login', 10, 60) RETURNING id",
        &[tenant],
    )
    .await;
    Graph {
        tenant,
        customer,
        identifier,
        conversation,
        message,
        attachment,
        citation,
        feedback,
        escalation,
        whatsapp_meta,
        category,
        item,
        chunk,
        ai_config,
        ai_credential,
        usage,
        generation,
        agent,
        prompt,
        prompt_version,
        avatar,
        membership,
        tool,
        tool_policy,
        tool_request,
        user,
        skill,
        invitation,
        audit,
        outbox,
        widget,
        widget_session,
        notification,
        connection,
        secret,
        delivery,
        integration_event,
        subscription,
        rate_limit,
    }
}

/// Every table in the harness: name, the row of a graph, an insert for a tenant, and the ids the
/// insert refers to. Add new tables to this list.
type Row = fn(&Graph) -> Uuid;
type Args = fn(&Graph) -> Vec<Uuid>;
struct Entry {
    table: &'static str,
    /// The column the harness looks rows up by: `id`, or `item_id` for the three tables keyed on
    /// the knowledge item.
    id_column: &'static str,
    row: Row,
    insert_sql: &'static str,
    args: Args,
}

fn entries() -> Vec<Entry> {
    vec![
        Entry {
            table: "customers",
            id_column: "id",
            row: |g| g.customer,
            insert_sql: "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'X')",
            args: |_| vec![],
        },
        Entry {
            table: "customer_channel_identifiers",
            id_column: "id",
            row: |g| g.identifier,
            insert_sql: "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) VALUES ($1, $2, 'phone', 'x')",
            args: |g| vec![g.customer],
        },
        Entry {
            table: "conversations",
            id_column: "id",
            row: |g| g.conversation,
            insert_sql: "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'widget', 'open')",
            args: |g| vec![g.customer],
        },
        Entry {
            table: "messages",
            id_column: "id",
            row: |g| g.message,
            insert_sql: "INSERT INTO messages (tenant_id, conversation_id, kind, body) VALUES ($1, $2, 'customer', 'x')",
            args: |g| vec![g.conversation],
        },
        Entry {
            table: "message_attachments",
            id_column: "id",
            row: |g| g.attachment,
            insert_sql: "INSERT INTO message_attachments (tenant_id, message_id, kind) VALUES ($1, $2, 'image')",
            args: |g| vec![g.message],
        },
        Entry {
            table: "message_citations",
            id_column: "id",
            row: |g| g.citation,
            insert_sql: "INSERT INTO message_citations (tenant_id, message_id, knowledge_item_id, item_title, passage_text, relevance_score, ordinal) VALUES ($1, $2, gen_random_uuid(), 't', 'p', 0.5, 1)",
            args: |g| vec![g.message],
        },
        Entry {
            table: "conversation_feedback",
            id_column: "id",
            row: |g| g.feedback,
            insert_sql: "INSERT INTO conversation_feedback (tenant_id, conversation_id, channel, rating) VALUES ($1, $2, 'widget', 4)",
            args: |g| vec![g.conversation],
        },
        Entry {
            table: "escalations",
            id_column: "id",
            row: |g| g.escalation,
            insert_sql: "INSERT INTO escalations (tenant_id, conversation_id, reason, status) VALUES ($1, $2, 'x', 'queued')",
            args: |g| vec![g.conversation],
        },
        Entry {
            table: "whatsapp_message_meta",
            id_column: "id",
            row: |g| g.whatsapp_meta,
            insert_sql: "INSERT INTO whatsapp_message_meta (tenant_id, message_id, conversation_id, direction) VALUES ($1, $2, $3, 'inbound')",
            args: |g| vec![g.message, g.conversation],
        },
        Entry {
            table: "knowledge_categories",
            id_column: "id",
            row: |g| g.category,
            insert_sql: "INSERT INTO knowledge_categories (tenant_id, name) VALUES ($1, 'X')",
            args: |_| vec![],
        },
        Entry {
            table: "knowledge_items",
            id_column: "id",
            row: |g| g.item,
            insert_sql: "INSERT INTO knowledge_items (tenant_id, item_type, title, body, status, source, created_by_display) VALUES ($1, 'article', 'X', 'b', 'draft', 'authored', 't')",
            args: |_| vec![],
        },
        Entry {
            table: "knowledge_documents",
            id_column: "item_id",
            row: |g| g.item,
            insert_sql: "INSERT INTO knowledge_documents (tenant_id, item_id, storage_key, original_filename, content_type, size_bytes) VALUES ($1, $2, 'k-ins', 'f.pdf', 'application/pdf', 10)",
            args: |g| vec![g.item],
        },
        Entry {
            table: "knowledge_item_tags",
            id_column: "item_id",
            row: |g| g.item,
            insert_sql: "INSERT INTO knowledge_item_tags (tenant_id, item_id, tag) VALUES ($1, $2, 'other')",
            args: |g| vec![g.item],
        },
        Entry {
            table: "knowledge_chunks",
            id_column: "id",
            row: |g| g.chunk,
            insert_sql: "INSERT INTO knowledge_chunks (tenant_id, item_id, ordinal, content, embedding, content_hash) VALUES ($1, $2, 1, 'c', array_fill(0.1::real, ARRAY[1536])::vector, 'h2')",
            args: |g| vec![g.item],
        },
        Entry {
            table: "knowledge_index_state",
            id_column: "item_id",
            row: |g| g.item,
            insert_sql: "INSERT INTO knowledge_index_state (tenant_id, item_id, status) VALUES ($1, $2, 'pending')",
            args: |g| vec![g.item],
        },
        Entry {
            table: "ai_configurations",
            id_column: "id",
            row: |g| g.ai_config,
            insert_sql: "INSERT INTO ai_configurations (tenant_id, provider, model) VALUES ($1, 'openai', 'x')",
            args: |_| vec![],
        },
        Entry {
            table: "ai_credentials",
            id_column: "id",
            row: |g| g.ai_credential,
            insert_sql: "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) VALUES ($1, 'anthropic', decode('01', 'hex'), decode('02', 'hex'), 'x')",
            args: |_| vec![],
        },
        Entry {
            table: "ai_usage_records",
            id_column: "id",
            row: |g| g.usage,
            insert_sql: "INSERT INTO ai_usage_records (tenant_id, provider, model, status, streamed, latency_ms) VALUES ($1, 'openai', 'm', 'success', false, 5)",
            args: |_| vec![],
        },
        Entry {
            table: "ai_generations",
            id_column: "id",
            row: |g| g.generation,
            insert_sql: "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, outcome, latency_ms) VALUES ($1, $2, $3, 'success', 1)",
            args: |g| vec![g.conversation, g.message],
        },
        Entry {
            table: "agent_configurations",
            id_column: "id",
            row: |g| g.agent,
            insert_sql: "INSERT INTO agent_configurations (tenant_id, name) VALUES ($1, 'X')",
            args: |_| vec![],
        },
        Entry {
            table: "agent_prompts",
            id_column: "id",
            row: |g| g.prompt,
            insert_sql: "INSERT INTO agent_prompts (tenant_id, active_version) VALUES ($1, 1)",
            args: |_| vec![],
        },
        Entry {
            table: "agent_prompt_versions",
            id_column: "id",
            row: |g| g.prompt_version,
            insert_sql: "INSERT INTO agent_prompt_versions (tenant_id, prompt_id, version_number, content, created_by_display) VALUES ($1, $2, 2, 'c', 't')",
            args: |g| vec![g.prompt],
        },
        Entry {
            table: "agent_avatar_uploads",
            id_column: "id",
            row: |g| g.avatar,
            insert_sql: "INSERT INTO agent_avatar_uploads (tenant_id, agent_id, content_type, bytes) VALUES ($1, $2, 'image/png', decode('00', 'hex'))",
            args: |g| vec![g.agent],
        },
        Entry {
            table: "tenant_tools",
            id_column: "id",
            row: |g| g.tool,
            insert_sql: "INSERT INTO tenant_tools (tenant_id, name, description, input_schema, endpoint_url, created_by_membership_id) VALUES ($1, 'other', 'd', '{}'::jsonb, 'https://example.test/o', $2)",
            args: |g| vec![g.membership],
        },
        Entry {
            table: "tenant_tool_policies",
            id_column: "id",
            row: |g| g.tool_policy,
            insert_sql: "INSERT INTO tenant_tool_policies (tenant_id, tool_name, updated_by_membership_id) VALUES ($1, 'other', $2)",
            args: |g| vec![g.membership],
        },
        Entry {
            table: "tool_requests",
            id_column: "id",
            row: |g| g.tool_request,
            insert_sql: "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, arguments, status, approval_required, chain_index) VALUES ($1, $2, $3, 'lookup', 'builtin', '{}'::jsonb, 'pending', false, 1)",
            args: |g| vec![g.conversation, g.generation],
        },
        Entry {
            table: "skills",
            id_column: "id",
            row: |g| g.skill,
            insert_sql: "INSERT INTO skills (tenant_id, name) VALUES ($1, 'Other')",
            args: |_| vec![],
        },
        Entry {
            table: "agent_skills",
            id_column: "skill_id",
            row: |g| g.skill,
            insert_sql: "INSERT INTO agent_skills (tenant_id, membership_id, skill_id) VALUES ($1, $2, $3)",
            args: |g| vec![g.membership, g.skill],
        },
        Entry {
            table: "agent_availability",
            id_column: "membership_id",
            row: |g| g.membership,
            insert_sql: "INSERT INTO agent_availability (tenant_id, membership_id) VALUES ($1, $2)",
            args: |g| vec![g.membership],
        },
        Entry {
            table: "tenant_memberships",
            id_column: "id",
            row: |g| g.membership,
            insert_sql: "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent')",
            args: |g| vec![g.user],
        },
        Entry {
            table: "tenant_invitations",
            id_column: "id",
            row: |g| g.invitation,
            insert_sql: "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) VALUES ($1, 'x@example.test', 'agent', 'other-hash', $2, now() + interval '1 day')",
            args: |g| vec![g.user],
        },
        Entry {
            table: "audit_logs",
            id_column: "id",
            row: |g| g.audit,
            insert_sql: "INSERT INTO audit_logs (tenant_id, action, resource_type, resource_id) VALUES ($1, 'isolation.test', 'tenant', 'y')",
            args: |_| vec![],
        },
        Entry {
            table: "outbox_events",
            id_column: "id",
            row: |g| g.outbox,
            insert_sql: "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) VALUES (gen_random_uuid(), 'tenant', 'y', $1::uuid::text, 'isolation.test', '{}'::jsonb, now())",
            args: |_| vec![],
        },
        Entry {
            table: "widget_instances",
            id_column: "id",
            row: |g| g.widget,
            insert_sql: "INSERT INTO widget_instances (tenant_id, public_id, name) VALUES ($1, 'wgt_other', 'W2')",
            args: |_| vec![],
        },
        Entry {
            table: "widget_sessions",
            id_column: "id",
            row: |g| g.widget_session,
            insert_sql: "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, expires_at) VALUES ($1, $2, decode('0a', 'hex'), now() + interval '1 day')",
            args: |g| vec![g.widget],
        },
        Entry {
            table: "notifications",
            id_column: "id",
            row: |g| g.notification,
            insert_sql: "INSERT INTO notifications (tenant_id, recipient_membership_id, kind, title, subject_type, subject_id, dedupe_key) VALUES ($1, $2, 'escalation.new', 'T', 'escalation', $3, 'other')",
            args: |g| vec![g.membership, g.escalation],
        },
        Entry {
            table: "integration_connections",
            id_column: "id",
            row: |g| g.connection,
            insert_sql: "INSERT INTO integration_connections (tenant_id, catalog_id, webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) VALUES ($1, (SELECT id FROM integration_catalog ORDER BY slug LIMIT 1), decode('0b', 'hex'), decode('01', 'hex'), decode('02', 'hex'))",
            args: |_| vec![],
        },
        Entry {
            table: "integration_secrets",
            id_column: "id",
            row: |g| g.secret,
            insert_sql: "INSERT INTO integration_secrets (tenant_id, connection_id, field_key, ciphertext, nonce, hint) VALUES ($1, $2, 'other', decode('01', 'hex'), decode('02', 'hex'), '...wxyz')",
            args: |g| vec![g.connection],
        },
        Entry {
            table: "integration_webhook_deliveries",
            id_column: "id",
            row: |g| g.delivery,
            insert_sql: "INSERT INTO integration_webhook_deliveries (tenant_id, connection_id, payload) VALUES ($1, $2, '{}'::jsonb)",
            args: |g| vec![g.connection],
        },
        Entry {
            table: "integration_events",
            id_column: "id",
            row: |g| g.integration_event,
            insert_sql: "INSERT INTO integration_events (tenant_id, connection_id, event_type, outcome) VALUES ($1, $2, 'connected', 'success')",
            args: |g| vec![g.connection],
        },
        Entry {
            table: "tenant_subscriptions",
            id_column: "id",
            row: |g| g.subscription,
            insert_sql: "INSERT INTO tenant_subscriptions (tenant_id, plan_id) VALUES ($1, (SELECT id FROM billing_plans ORDER BY created_at LIMIT 1))",
            args: |_| vec![],
        },
        Entry {
            table: "tenant_rate_limit_overrides",
            id_column: "id",
            row: |g| g.rate_limit,
            insert_sql: "INSERT INTO tenant_rate_limit_overrides (tenant_id, scope, max_requests, window_seconds) VALUES ($1, 'webhook', 5, 60)",
            args: |_| vec![],
        },
    ]
}

#[tokio::test]
async fn every_table_under_forced_row_level_security_isolates_tenants() {
    if !can_run() {
        return;
    }
    db::testing::migrate().await;
    let seed = db::testing::seed_pool();
    let runtime = runtime_pool().await;
    let (a, b) = (seed_graph(&seed).await, seed_graph(&seed).await);

    for e in entries() {
        // Each probe inserts the same kind of row for the *other* tenant's ids, so the insert
        // arguments come from tenant A's graph while the tenant bound is also A: with no tenant
        // set the policy refuses it either way.
        let args = (e.args)(&a);
        assert_isolated(
            &runtime,
            &Probe {
                table: e.table,
                id_column: e.id_column,
                tenant_a: a.tenant,
                tenant_b: b.tenant,
                row_a: (e.row)(&a),
                row_b: (e.row)(&b),
                insert_sql: e.insert_sql,
                insert_args: &args,
            },
        )
        .await;
    }
}

/// The harness must cover exactly the tables whose policy is forced: a table added to Row-Level
/// Security without a harness entry (or an entry for a table that is not protected) fails here.
#[tokio::test]
async fn the_harness_covers_exactly_the_tables_with_forced_row_level_security() {
    if !can_run() {
        return;
    }
    db::testing::migrate().await;
    let runtime = runtime_pool().await;
    let mut covered: Vec<String> = entries().iter().map(|e| e.table.to_string()).collect();
    covered.sort();
    let forced = forced_rls_tables(&runtime).await;
    assert_eq!(
        covered, forced,
        "tables with forced Row-Level Security and the harness entries must match: add the new \
         table to `entries()` in backend/crates/shared/db/tests/tenant_isolation.rs"
    );
}

/// The platform defaults in `ai_configurations` and `ai_credentials` (rows with no tenant) are
/// hidden from every tenant query: a tenant reaches them only through the audited functions
/// (`xtenant.platform_ai_config` and friends), so a bug in tenant-scoped code cannot read the
/// platform's API keys.
#[tokio::test]
async fn platform_ai_rows_are_invisible_to_tenant_queries() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    db::testing::become_platform(&mut tx).await;
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("Platform rows")
            .bind(format!("plat-{}", Uuid::new_v4().simple()))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    // Replace any platform defaults with known ones (rolled back with the transaction).
    sqlx::query("UPDATE ai_configurations SET deleted_at = now() WHERE tenant_id IS NULL AND deleted_at IS NULL")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE ai_credentials SET deleted_at = now() WHERE tenant_id IS NULL AND deleted_at IS NULL")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO ai_configurations (provider, model) VALUES ('openai', 'platform-model')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_credentials (provider, ciphertext, nonce, key_hint) \
         VALUES ('openai', decode('01', 'hex'), decode('02', 'hex'), '...platform')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    // Control: the bypass role sees them.
    for table in ["ai_configurations", "ai_credentials"] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE tenant_id IS NULL AND deleted_at IS NULL"
        )))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            n, 1,
            "{table}: the platform default must exist for the check to mean anything"
        );
    }

    // As app_runtime with a tenant set, exactly as the application reads.
    db::testing::become_runtime(&mut tx).await;
    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(tenant.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    for table in ["ai_configurations", "ai_credentials"] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE tenant_id IS NULL"
        )))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            n, 0,
            "{table}: a tenant query must not see the platform default"
        );
        let all: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(
            all, 0,
            "{table}: this tenant has no rows of its own, so nothing is visible"
        );
    }
}
