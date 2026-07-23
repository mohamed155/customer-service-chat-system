use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_ENV: config::Environment = config::Environment::Test;

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: TEST_ENV,
        cors_allowed_origins: vec![],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: "http://localhost:4200".into(),
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        integration_secrets_key: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    }
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

fn test_app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
    }
}

async fn send_request(pool: sqlx::PgPool, req: Request<Body>) -> axum::response::Response {
    let state = test_app_state(pool);
    let app = router::app(state);
    app.oneshot(req).await.expect("request should succeed")
}

async fn seed_user(pool: &sqlx::PgPool) -> Uuid {
    let email = format!("test_{}@example.com", Uuid::new_v4());
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(&email)
    .bind("Seed User")
    .fetch_one(pool)
    .await
    .expect("seed user")
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!(
        "tenant-{}",
        Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    sqlx::query_scalar::<_, Uuid>("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Seed Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .expect("seed tenant")
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .expect("seed membership")
}

async fn body_bytes(res: &mut axum::response::Response) -> Vec<u8> {
    use http_body_util::BodyExt;
    BodyExt::collect(res.body_mut())
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

async fn body_json(res: &mut axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&body_bytes(res).await).unwrap()
}

/// Seed a tenant-defined tool with a credential, returning its id.
/// The credential value "sk-leaked-key-99999" must NEVER appear in any API response.
async fn seed_tool_with_credential(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> Uuid {
    let master = ai_providers::crypto::MasterKey::from_base64(
        "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
    )
    .unwrap();
    let scope = ai_providers::crypto::aad(Some(tenant_id), "secret_tool");
    let (ct, nonce) = ai_providers::crypto::seal(&master, &scope, "sk-leaked-key-99999").unwrap();
    let ct_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &ct);
    let nonce_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &nonce);

    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenant_tools \
         (tenant_id, name, description, input_schema, endpoint_url, \
          credential_ciphertext, classification, created_by_membership_id) \
         VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7, $8) RETURNING id",
    )
    .bind(tenant_id)
    .bind("secret_tool")
    .bind("A tool with a secret credential")
    .bind(json!({"type": "object", "properties": {}}))
    .bind("https://api.example.com/secret")
    .bind(format!("{ct_b64}||{nonce_b64}"))
    .bind("auto")
    .bind(membership_id)
    .fetch_one(pool)
    .await
    .expect("seed tool with credential")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// After registering a tool with credential and querying via REST,
    /// the raw credential must NOT appear in any API response body.
    #[tokio::test]
    async fn credential_not_exposed_in_get_tools() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool).await;
        let tenant_id = seed_tenant(&pool).await;
        let membership_id = seed_membership(&pool, tenant_id, user_id, "admin").await;
        let _tool_id = seed_tool_with_credential(&pool, tenant_id, membership_id).await;

        // GET /tenant/tools
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        let body_str = serde_json::to_string(&body).unwrap();

        assert!(
            !body_str.contains("sk-leaked-key-99999"),
            "credential must not appear in GET /tenant/tools response"
        );

        // Verify tool is present but hasCredential is null
        let items = body["items"].as_array().unwrap();
        let tool = items.iter().find(|i| i["name"] == "secret_tool");
        assert!(tool.is_some(), "secret_tool should appear in list");
        assert!(
            tool.unwrap()["hasCredential"].is_null(),
            "hasCredential should be null (not exposed)"
        );
    }

    #[tokio::test]
    async fn credential_not_exposed_in_tool_activity() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool).await;
        let tenant_id = seed_tenant(&pool).await;
        let membership_id = seed_membership(&pool, tenant_id, user_id, "admin").await;
        let _tool_id = seed_tool_with_credential(&pool, tenant_id, membership_id).await;

        // Create a conversation and tool request to test tool_activity endpoint
        let conversation_id = Uuid::new_v4();
        let generation_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO conversations (id, tenant_id, customer_id, channel, status) \
             VALUES ($1, $2, $3, 'chat', 'active')",
        )
        .bind(conversation_id)
        .bind(tenant_id)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO ai_generations (id, tenant_id, conversation_id, trigger_message_id, outcome) \
             VALUES ($1, $2, $3, $4, 'success')",
        )
        .bind(generation_id)
        .bind(tenant_id)
        .bind(conversation_id)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .unwrap();

        // Insert a tool request referencing the tenant tool
        sqlx::query(
            "INSERT INTO tool_requests \
             (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
              tenant_tool_id, arguments, status, approval_required, chain_index) \
             VALUES ($1, $2, $3, 'secret_tool', 'tenant', $4, '{}'::jsonb, \
              'succeeded', false, 0)",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .bind(generation_id)
        .bind(_tool_id)
        .execute(&pool)
        .await
        .unwrap();

        // GET /tenant/conversations/{id}/tool-activity
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri(&format!(
                    "/api/v1/tenant/conversations/{conversation_id}/tool-activity"
                ))
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        let body_str = serde_json::to_string(&body).unwrap();

        assert!(
            !body_str.contains("sk-leaked-key-99999"),
            "credential must not appear in tool-activity response"
        );
    }

    #[tokio::test]
    async fn credential_not_exposed_in_error_text() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool).await;
        let tenant_id = seed_tenant(&pool).await;
        let membership_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

        // Create a tool with an invalid credential (tampered ciphertext)
        // The credential value should still NOT leak in any error message
        let master = ai_providers::crypto::MasterKey::from_base64(
            "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
        )
        .unwrap();
        let scope = ai_providers::crypto::aad(Some(tenant_id), "broken_tool");
        let (mut ct, nonce) =
            ai_providers::crypto::seal(&master, &scope, "sk-leaked-key-99999").unwrap();
        // Tamper the ciphertext
        ct[0] ^= 0xff;
        let ct_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &ct);
        let nonce_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &nonce);

        let tool_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenant_tools \
             (tenant_id, name, description, input_schema, endpoint_url, \
              credential_ciphertext, classification, created_by_membership_id) \
             VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7, $8) RETURNING id",
        )
        .bind(tenant_id)
        .bind("broken_tool")
        .bind("Tool with tampered credential")
        .bind(json!({"type": "object", "properties": {}}))
        .bind("https://api.example.com/broken")
        .bind(format!("{ct_b64}||{nonce_b64}"))
        .bind("auto")
        .bind(membership_id)
        .fetch_one(&pool)
        .await
        .expect("seed broken tool");

        // Try executing the tool — it should fail but never include the credential
        let exec_ctx = tools::registry::ToolExecutionCtx {
            tenant_id,
            conversation_id: Uuid::new_v4(),
            pool: pool.clone(),
            master_key: Some(
                ai_providers::crypto::MasterKey::from_base64(
                    "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
                )
                .unwrap(),
            ),
        };
        let resolved = tools::policy::ResolvedTool {
            spec: ai_providers::ToolSpec {
                name: "broken_tool".into(),
                description: "test".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            source: tools::model::ToolSource::Tenant,
            approval_required: false,
            tenant_tool_id: Some(tool_id),
        };

        let outcome = tools::executor::execute(&exec_ctx, &resolved, json!({}), Uuid::nil()).await;
        match outcome {
            tools::executor::ExecutionOutcome::Failed(e) => {
                assert!(
                    !e.contains("sk-leaked-key-99999"),
                    "credential must not leak in error message: {e}"
                );
            }
            other => {
                // Could also be TimedOut if URL resolves or Succeeded in edge case
                // That's OK — just make sure no credential leaked
                let debug_str = format!("{:?}", other);
                assert!(
                    !debug_str.contains("sk-leaked-key-99999"),
                    "credential must not leak in outcome debug: {debug_str}"
                );
            }
        }
    }
}
