//! T067: Quickstart validation stub.
//!
//! Documents the five quickstart scenarios and provides a test runner
//! skeleton that can be wired to a local stack. Each scenario requires:
//!
//! **Prerequisites:**
//! - Running PostgreSQL with migrations applied (`db::run_migrations`)
//! - Running Redis
//! - A configured AI provider (or mock endpoint)
//!
//! ## Scenarios
//!
//! ### Scenario 1: Auto-approved tool
//! 1. Enable `lookup_customer` built-in tool for the tenant (auto-approved)
//! 2. Send a customer message that triggers the tool ("What is my name?")
//! 3. Drive the agent responder (`process_agent_responder_once`)
//! 4. Assert:
//!    - AI reply reflects the customer's profile data
//!    - A `succeeded` `tool_requests` row exists with `duration_ms` populated
//!    - The row persists after reload (re-fetch from DB)
//!
//! ### Scenario 2: Approval-required tool
//! 1. Enable `update_customer_contact` built-in tool (approval-required)
//! 2. Trigger the tool via a customer message
//! 3. Assert:
//!    - Interim holding message is posted ("Let me look into that for you...")
//!    - A `tool_requests` row with `status='awaiting_approval'` exists
//!    - `ai_generations.outcome = 'awaiting_tool_approval'`
//! 4. Approve the request via `POST /tenant/tool-requests/{id}/decide`
//! 5. Drive the agent responder to consume the `ai.tool_decision` event
//! 6. Assert:
//!    - Tool executed, contact field changed
//!    - Follow-up AI reply exists
//! 7. Repeat for deny and expiry paths
//!
//! ### Scenario 3: Failure audit
//! 1. Run one succeeding and one failing execution
//! 2. Assert both show full detail in the timeline
//!    - Succeeded: `status=succeeded`, `duration_ms`, `result` present
//!    - Failed: `status=failed`, `error` present, visually distinct styling
//! 3. Assert customer-facing view shows neither (FR-020)
//!
//! ### Scenario 4: Tenant-defined tool
//! 1. Register a tenant-defined tool via `POST /tenant/tools`
//! 2. Verify it appears in `GET /tenant/tools` with `has_credential:null`
//!    and no credential value anywhere in the response
//! 3. Trigger the AI to call it
//! 4. Assert the endpoint received the expected POST body/header
//! 5. Verify isolation: tenant B's `GET /tenant/tools` does NOT list it
//!
//! ### Scenario 5: Credential confidentiality
//! 1. Register a tenant-defined tool with a credential
//! 2. Execute it (successfully and with a deliberate failure)
//! 3. Assert the credential string appears in none of:
//!    - Settings GET response
//!    - Tool-activity response
//!    - SSE event payloads
//!    - Sanitized error text of the failing call
//!
//! ## Usage
//!
//! ```bash
//! # Set up the local stack
//! docker compose up -d
//!
//! # Set required env vars
//! export DATABASE_URL="postgres://..."
//! export REDIS_URL="redis://..."
//! export REQUIRE_DB_TESTS=1
//!
//! # Run all scenarios
//! cargo test --test quickstart_validation -- --nocapture
//! ```
//!
//! Each test below is a TODO placeholder — uncomment and wire to the
//! specific helpers and assertions once the local stack is available.
//!
//! For now, a smoke-test ensures the module compiles and basic DB
//! connectivity works.

use std::time::Duration;

/// Placeholder: verify the test module loads and can connect to the DB.
#[tokio::test]
async fn quickstart_db_connectivity() {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping quickstart_validation: DATABASE_URL not set");
            return;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping quickstart_validation: DATABASE_URL is unreachable");
        return;
    }
    db::run_migrations(&pool).await.unwrap();
    eprintln!("quickstart_validation: DB connectivity OK");
}
