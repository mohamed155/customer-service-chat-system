use axum::{
    extract::{Path, Query, State},
    http::{header::SET_COOKIE, StatusCode},
    response::{IntoResponse, Json, Response},
    Extension,
};
use chrono::{Duration, Utc};
use config::AppConfig;
use identity::{password, session, OptionalPrincipal, Principal};
use kernel::{ApiError, ErrorDetail, ErrorEnvelope, Page};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::str::FromStr;
use std::sync::Arc;
use url::Url;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{audit, members, routes, TenantContext};

pub const INVITATION_EXPIRY_DAYS: i64 = 7;

/// Generate a 256-bit random token and return (raw_token, sha256_hex_hash).
fn generate_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw = hex::encode(bytes);
    let hash = hex::encode(Sha256::digest(bytes));
    (raw, hash)
}

fn hash_token(token: &str) -> String {
    let bytes = hex::decode(token).unwrap_or_default();
    hex::encode(Sha256::digest(&bytes))
}

fn parse_token_hash(token: &str) -> Option<String> {
    if token.len() != 64 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    Some(hash_token(token))
}

fn role_rank(role: &authz::TenantRole) -> u8 {
    members::TENANT_ROLE_RANK
        .iter()
        .find(|(r, _)| r == role)
        .map(|(_, rank)| *rank)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateInvitationPayload {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AcceptInvitationPayload {
    pub display_name: Option<String>,
    #[schema(value_type = Option<String>, write_only)]
    pub password: Option<String>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationResponse {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub status: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[schema(value_type = String, example = "queued")]
    pub email_delivery_status: email::EmailDeliveryStatus,
    pub invited_by_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInvitationResponse {
    pub invitation: InvitationResponse,
    pub accept_url: String,
    pub email_sent: bool,
    #[schema(value_type = String, example = "queued")]
    pub email_delivery_status: email::EmailDeliveryStatus,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationListItem {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub status: String,
    pub invited_by_name: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[schema(value_type = String, example = "queued")]
    pub email_delivery_status: email::EmailDeliveryStatus,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationDeliveryResponse {
    #[schema(value_type = String, example = "queued")]
    pub email_delivery_status: email::EmailDeliveryStatus,
}

#[utoipa::path(
    get,
    path = "/tenant/members/invitations/{id}/delivery",
    tag = "members",
    operation_id = "get_member_invitation_delivery",
    summary = "Get the email delivery status of a team-member invitation",
    description = "Return the latest email delivery status (`unconfigured`, `queued`, `sent`, or \
                  `failed`) recorded for the invitation. Requires permission: members.view",
    params(("id" = Uuid, Path, description = "Invitation identifier")),
    responses(
        (status = 200, description = "Delivery status returned.", body = InvitationDeliveryResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Invitation not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_invitation_delivery(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Path(invitation_id): Path<Uuid>,
) -> Response {
    let status: Option<String> = match sqlx::query_scalar(
        "SELECT email_delivery_status FROM tenant_invitations WHERE tenant_id = $1 AND id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(invitation_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(status) => status,
        Err(error) => {
            tracing::error!(%error, "failed to load invitation delivery status");
            return ApiError::internal_error("Failed to load invitation delivery status")
                .into_response();
        }
    };
    let Some(status) = status else {
        return ApiError::not_found("Invitation not found").into_response();
    };
    let email_delivery_status = match status.as_str() {
        "queued" => email::EmailDeliveryStatus::Queued,
        "sent" => email::EmailDeliveryStatus::Sent,
        "failed" => email::EmailDeliveryStatus::Failed("".into()),
        _ => email::EmailDeliveryStatus::Unconfigured,
    };
    Json(InvitationDeliveryResponse {
        email_delivery_status,
    })
    .into_response()
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default, deny_unknown_fields)]
pub struct InvitationQuery {
    pub status: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
}

impl Default for InvitationQuery {
    fn default() -> Self {
        Self {
            status: None,
            limit: 25,
            cursor: None,
        }
    }
}

impl InvitationQuery {
    fn status_filter(&self) -> Result<InvitationStatusFilter, ApiError> {
        match self.status.as_deref() {
            None => Ok(InvitationStatusFilter::Open),
            Some("pending") => Ok(InvitationStatusFilter::Open),
            Some("expired") => Ok(InvitationStatusFilter::ExpiredOnly),
            Some("accepted") => Ok(InvitationStatusFilter::Exact("accepted")),
            Some("revoked") => Ok(InvitationStatusFilter::Exact("revoked")),
            Some(_) => Err(
                ApiError::unprocessable_entity("Validation failed").with_details(vec![
                    ErrorDetail {
                        field: "status".into(),
                        code: "invalid_value".into(),
                        message: "Status must be pending, accepted, revoked, or expired".into(),
                    },
                ]),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvitationStatusFilter {
    Open,
    ExpiredOnly,
    Exact(&'static str),
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInvitationResponse {
    pub tenant_name: String,
    pub email: String,
    pub role: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub account_exists: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AcceptInvitationResponse {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub tenant_name: String,
    pub role: String,
}

// ---------------------------------------------------------------------------
// POST /tenant/members/invitations
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/tenant/members/invitations",
    tag = "members",
    operation_id = "create_member_invitation",
    summary = "Create a team-member invitation",
    description = "Invite a new user to join the current tenant at the given role. A signed accept \
                  URL is generated and the invitation email is enqueued via the outbox. Rank-based \
                  rules apply: the actor must be able to assign the target role; assigning the owner \
                  role additionally requires the `owner.assign` permission. \
                  Requires permission: members.manage",
    request_body = CreateInvitationPayload,
    responses(
        (status = 201, description = "Invitation created.", body = CreateInvitationResponse),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required or insufficient rank.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 409, description = "A pending invitation already exists for this email, or the user is already a member.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (per-field, e.g. invalid email or unknown role).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn create_invitation(
    State(pool): State<sqlx::PgPool>,
    Extension(sender): Extension<Arc<dyn email::EmailSender>>,
    Extension(config): Extension<Arc<AppConfig>>,
    ctx: TenantContext,
    principal: Principal,
    kernel::ApiJson(payload): kernel::ApiJson<CreateInvitationPayload>,
) -> Response {
    let mut details: Vec<ErrorDetail> = Vec::new();

    // ---- validate email ----
    let email = payload.email.trim().to_lowercase();
    if !routes::is_valid_email(&email) {
        details.push(ErrorDetail {
            field: "email".into(),
            code: "invalid_format".into(),
            message: "Invalid email format".into(),
        });
    }

    // ---- validate role ----
    let target_role = match members::parse_role(&payload.role) {
        Ok(r) => r,
        Err(_) => {
            details.push(ErrorDetail {
                field: "role".into(),
                code: "invalid_value".into(),
                message: "Role must be one of: owner, admin, manager, agent, viewer".into(),
            });
            authz::TenantRole::Viewer
        }
    };

    if !details.is_empty() {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .into_response();
    }

    // ---- Determine actor rank ----
    let actor_rank = match &ctx.tenant_role {
        Some(role) => role_rank(role),
        None => match members::staff_effective_rank(&ctx.permissions) {
            Some(r) => r,
            None => {
                return ApiError::unauthorized("Access denied").into_response();
            }
        },
    };

    // ---- Check can-assign (rank-based) ----
    if actor_rank < role_rank(&target_role) {
        return ApiError::unauthorized("Access denied").into_response();
    }

    // ---- Owner assignment additionally requires OwnerAssign permission ----
    if target_role == authz::TenantRole::Owner
        && !ctx.permissions.contains(authz::Permission::OwnerAssign)
    {
        return ApiError::unauthorized("Access denied").into_response();
    }

    // ---- Check existing membership (active or disabled) ----
    let existing_member: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id IN (SELECT id FROM users WHERE email = $2) \
         AND deleted_at IS NULL",
    )
    .bind(ctx.tenant_id)
    .bind(&email)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    if let Some((status,)) = existing_member {
        return ApiError::conflict("User is already a member of this tenant")
            .with_details(vec![ErrorDetail {
                field: "email".into(),
                code: "conflict".into(),
                message: format!("User is already a {} member in this tenant", status),
            }])
            .into_response();
    }

    // ---- Check existing pending invitation (DB unique index backs this up).
    // Only an active, unexpired pending invite blocks reissue: an expired
    // invitation whose 7-day window has passed is superseded automatically
    // below (inside the transaction), so it must not count here. ----
    let existing_pending: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM tenant_invitations \
         WHERE tenant_id = $1 AND email = $2 AND status = 'pending' AND expires_at > now() \
         LIMIT 1",
    )
    .bind(ctx.tenant_id)
    .bind(&email)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    if existing_pending.is_some() {
        return ApiError::conflict("A pending invitation already exists for this email")
            .with_details(vec![ErrorDetail {
                field: "email".into(),
                code: "conflict".into(),
                message: "A pending invitation already exists for this email".into(),
            }])
            .into_response();
    }

    // ---- Generate token ----
    let (raw_token, token_hash) = generate_token();
    let expires_at = Utc::now() + Duration::days(INVITATION_EXPIRY_DAYS);
    let accept_url = match build_accept_url(&config.public_dashboard_url, &raw_token) {
        Ok(url) => url,
        Err(e) => {
            return ApiError::internal_error(format!("Failed to build invitation URL: {e}"))
                .into_response();
        }
    };

    // ---- Transaction: INSERT + audit ----
    let role = payload.role.clone();
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error(format!("Database transaction begin failed: {e}"))
                .into_response();
        }
    };

    // ---- Revoke any stale (expired) pending invitation for this (tenant,
    // email) pair before inserting. An active pending invite was already
    // rejected above (409) so only already-expired rows are touched. ----
    let superseded: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, role FROM tenant_invitations \
         WHERE tenant_id = $1 AND email = $2 AND status = 'pending' AND expires_at <= now() \
         LIMIT 1",
    )
    .bind(ctx.tenant_id)
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    if let Some((old_id, old_role)) = &superseded {
        if let Err(e) = sqlx::query(
            "UPDATE tenant_invitations \
             SET status = 'revoked', revoked_at = now(), revoked_by = $1 \
             WHERE id = $2 AND tenant_id = $3",
        )
        .bind(principal.user_id)
        .bind(old_id)
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await
        {
            return ApiError::internal_error(format!(
                "Failed to supersede expired invitation: {e}"
            ))
            .into_response();
        }

        if let Err(e) = audit::record_member_invitation_revoked(
            &mut tx,
            principal.user_id,
            ctx.tenant_id,
            *old_id,
            &email,
            old_role,
        )
        .await
        {
            return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
        }
    }

    let initial_delivery_status = if sender.is_configured() {
        email::EmailDeliveryStatus::Queued
    } else {
        email::EmailDeliveryStatus::Unconfigured
    };
    let invitation_id: Uuid = match sqlx::query_scalar(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at, email_delivery_status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(&email)
    .bind(&role)
    .bind(&token_hash)
    .bind(principal.user_id)
    .bind(expires_at)
    .bind(initial_delivery_status.as_str())
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        Err(sqlx::Error::Database(dbe)) if dbe.code().as_deref() == Some("23505") => {
            return ApiError::conflict("A pending invitation already exists for this email")
                .with_details(vec![ErrorDetail {
                    field: "email".into(),
                    code: "conflict".into(),
                    message: "A pending invitation already exists for this email".into(),
            }])
                .into_response();
        }
        Err(e) => {
            return ApiError::internal_error(format!("Database insert failed: {e}"))
                .into_response();
        }
    };

    if initial_delivery_status == email::EmailDeliveryStatus::Queued {
        let payload = serde_json::json!({
            "to": email,
            "acceptUrl": accept_url,
        });
        if let Err(e) = sqlx::query(
            "INSERT INTO outbox_events \
             (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
             VALUES ($1, 'tenant_invitation', $2, $3, 'invitation.email_delivery', $4, now())",
        )
        .bind(Uuid::new_v4())
        .bind(invitation_id.to_string())
        .bind(ctx.tenant_id.to_string())
        .bind(payload)
        .execute(&mut *tx)
        .await
        {
            return ApiError::internal_error(format!("Outbox insert failed: {e}")).into_response();
        }
    }

    if let Err(e) = audit::record_member_invited(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        invitation_id,
        &email,
        &role,
    )
    .await
    {
        return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
    }

    if let Err(e) = tx.commit().await {
        return ApiError::internal_error(format!("Transaction commit failed: {e}")).into_response();
    }

    let email_delivery_status = initial_delivery_status;
    let email_sent = email_delivery_status == email::EmailDeliveryStatus::Sent;
    let invited_by_name: String =
        sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
            .bind(principal.user_id)
            .fetch_one(&pool)
            .await
            .unwrap_or_default();

    (
        StatusCode::CREATED,
        Json(CreateInvitationResponse {
            invitation: InvitationResponse {
                id: invitation_id,
                email,
                role,
                status: "pending".into(),
                expires_at,
                created_at: Utc::now(),
                email_delivery_status: email_delivery_status.clone(),
                invited_by_name,
            },
            accept_url,
            email_sent,
            email_delivery_status,
        }),
    )
        .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvitationDeliveryPayload {
    to: String,
    accept_url: String,
}

pub async fn process_invitation_deliveries_once(
    pool: &sqlx::PgPool,
    sender: Arc<dyn email::EmailSender>,
) -> Result<u64, sqlx::Error> {
    const MAX_ATTEMPTS: i32 = 3;
    let mut exhausted_tx = pool.begin().await?;
    let exhausted = sqlx::query(
        "UPDATE outbox_events event SET processed_at = now(), dead_lettered_at = now(), \
         last_error = 'delivery claim expired after maximum attempts', claimed_at = NULL, claim_token = NULL \
         FROM (SELECT id FROM outbox_events \
               WHERE event_type = 'invitation.email_delivery' AND processed_at IS NULL \
                 AND dead_lettered_at IS NULL AND attempts >= $1 \
                 AND (claimed_at IS NULL OR claimed_at <= now() - interval '5 minutes') \
               ORDER BY created_at, id FOR UPDATE SKIP LOCKED LIMIT 1) exhausted \
         WHERE event.id = exhausted.id RETURNING event.aggregate_id, event.tenant_id",
    )
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *exhausted_tx)
    .await?;
    if let Some(row) = exhausted {
        sqlx::query(
            "UPDATE tenant_invitations SET email_delivery_status = 'failed', email_delivery_error = 'Max retries exceeded' \
             WHERE id::text = $1 AND tenant_id::text = $2 AND email_delivery_status = 'queued'",
        )
        .bind(row.get::<String, _>("aggregate_id"))
        .bind(row.get::<String, _>("tenant_id"))
        .execute(&mut *exhausted_tx)
        .await?;
        exhausted_tx.commit().await?;
        return Ok(1);
    }
    exhausted_tx.commit().await?;

    let claim_token = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "UPDATE outbox_events event \
         SET claimed_at = now(), claim_token = $1, attempts = attempts + 1 \
         FROM (SELECT id FROM outbox_events \
               WHERE event_type = 'invitation.email_delivery' \
                 AND processed_at IS NULL AND dead_lettered_at IS NULL \
                 AND available_at <= now() AND attempts < $2 \
                 AND (claimed_at IS NULL OR claimed_at <= now() - interval '5 minutes') \
               ORDER BY available_at, created_at, id FOR UPDATE SKIP LOCKED LIMIT 1) claimable \
         WHERE event.id = claimable.id \
         RETURNING event.id, event.aggregate_id, event.tenant_id, event.payload, event.attempts",
    )
    .bind(claim_token)
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(0);
    };

    tx.commit().await?;

    let event_id: Uuid = row.get("id");
    let attempts: i32 = row.get("attempts");
    let invitation_id = Uuid::parse_str(row.get::<String, _>("aggregate_id").as_str());
    let tenant_id = Uuid::parse_str(row.get::<String, _>("tenant_id").as_str());
    let payload = serde_json::from_value::<InvitationDeliveryPayload>(row.get("payload"));
    let (terminal_status, error) = match (invitation_id, tenant_id, payload) {
        (Ok(invitation_id), Ok(tenant_id), Ok(payload)) => {
            let delivery_status =
                send_invitation_email(sender, payload.to, payload.accept_url).await;
            let err_msg = delivery_status.error_message().map(|s| s.to_string());
            match delivery_status {
                email::EmailDeliveryStatus::Sent => (
                    Some((
                        invitation_id,
                        tenant_id,
                        email::EmailDeliveryStatus::Sent,
                    )),
                    None,
                ),
                email::EmailDeliveryStatus::Failed(_) => {
                    if attempts >= MAX_ATTEMPTS {
                        (
                            Some((
                                invitation_id,
                                tenant_id,
                                email::EmailDeliveryStatus::Failed(
                                    err_msg.clone().unwrap_or_default(),
                                ),
                            )),
                            err_msg,
                        )
                    } else {
                        let mut retry_tx = pool.begin().await?;
                        sqlx::query(
                            "UPDATE outbox_events SET claimed_at = NULL, claim_token = NULL, \
                             available_at = now() + interval '1 second', last_error = $1 \
                             WHERE id = $2 AND claim_token = $3",
                        )
                        .bind(err_msg.as_deref())
                        .bind(event_id)
                        .bind(claim_token)
                        .execute(&mut *retry_tx)
                        .await?;
                        retry_tx.commit().await?;
                        return Ok(1);
                    }
                }
                _ => (None, Some("unexpected delivery status".into())),
            }
        }
        (invitation_id, tenant_id, payload) => {
            let ids = invitation_id
                .as_ref()
                .ok()
                .copied()
                .zip(tenant_id.as_ref().ok().copied());
            let message = format!(
                "poison invitation delivery event: invitation={:?}, tenant={:?}, payload={:?}",
                invitation_id.as_ref().err(),
                tenant_id.as_ref().err(),
                payload.as_ref().err()
            );
            (
                ids.map(|(i, t)| {
                    (
                        i,
                        t,
                        email::EmailDeliveryStatus::Failed(message.clone()),
                    )
                }),
                Some(message),
            )
        }
    };

    let mut finalize_tx = pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE outbox_events SET processed_at = now(), dead_lettered_at = CASE WHEN $1::text IS NULL THEN NULL ELSE now() END, \
         last_error = $1, claimed_at = NULL, claim_token = NULL WHERE id = $2 AND claim_token = $3",
    )
    .bind(error.as_deref())
    .bind(event_id)
    .bind(claim_token)
    .execute(&mut *finalize_tx)
    .await?;
    if updated.rows_affected() == 1 {
        if let Some((invitation_id, tenant_id, status)) = &terminal_status {
            sqlx::query(
                "UPDATE tenant_invitations SET email_delivery_status = $1, email_delivery_error = $4 \
                 WHERE tenant_id = $2 AND id = $3 AND email_delivery_status = 'queued'",
            )
            .bind(status.as_str())
            .bind(tenant_id)
            .bind(invitation_id)
            .bind(status.error_message())
            .execute(&mut *finalize_tx)
            .await?;
        }
    }
    finalize_tx.commit().await?;
    Ok(1)
}

pub async fn run_invitation_delivery_worker(
    pool: sqlx::PgPool,
    sender: Arc<dyn email::EmailSender>,
) {
    loop {
        match process_invitation_deliveries_once(&pool, sender.clone()).await {
            Ok(0) => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
            Ok(_) => {}
            Err(error) => {
                tracing::error!(%error, "invitation delivery worker iteration failed");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

fn build_accept_url(base_url: &str, raw_token: &str) -> Result<String, url::ParseError> {
    let base = Url::parse(base_url)?;
    Ok(base.join(&format!("invite/{raw_token}"))?.to_string())
}

async fn send_invitation_email(
    sender: Arc<dyn email::EmailSender>,
    to: String,
    accept_url: String,
) -> email::EmailDeliveryStatus {
    if !sender.is_configured() {
        return email::EmailDeliveryStatus::Unconfigured;
    }

    let msg = email::EmailMessage {
        to,
        subject: "You've been invited to join a tenant".into(),
        body_text: format!("Click the link to accept:\n{}", accept_url),
        body_html: Some(format!(
            "<p><a href=\"{}\">{}</a></p>",
            accept_url, accept_url
        )),
    };

    let status = sender.send(msg).await;
    if let email::EmailDeliveryStatus::Failed(ref e) = status {
        tracing::error!(error = %e, "failed to send invitation email");
    }
    status
}

// ---------------------------------------------------------------------------
// GET /tenant/members/invitations
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/tenant/members/invitations",
    tag = "members",
    operation_id = "list_member_invitations",
    summary = "List team-member invitations",
    description = "List the invitations issued by the current tenant with cursor-based pagination \
                  and an optional `status` filter (`pending`, `accepted`, `revoked`, or `expired`; \
                  the default is `pending` which also surfaces not-yet-swept expired entries). \
                  Requires permission: members.view",
    params(InvitationQuery),
    responses(
        (status = 200, description = "Page of invitations.", body = Page<InvitationListItem>),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (invalid `status` filter or `limit` range).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_invitations(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Query(params): Query<InvitationQuery>,
) -> Response {
    let mut details: Vec<ErrorDetail> = Vec::new();

    if params.limit == 0 || params.limit > 100 {
        details.push(ErrorDetail {
            field: "limit".into(),
            code: "invalid_range".into(),
            message: "Limit must be between 1 and 100".into(),
        });
    }

    if let Some(ref cursor) = params.cursor {
        let valid_cursor = cursor
            .split_once('_')
            .and_then(|(created_str, id_str)| {
                chrono::DateTime::parse_from_rfc3339(created_str)
                    .ok()
                    .and_then(|_| Uuid::parse_str(id_str).ok())
            })
            .is_some();

        if !valid_cursor {
            details.push(ErrorDetail {
                field: "cursor".into(),
                code: "invalid_value".into(),
                message: "Cursor must be a valid invitation cursor".into(),
            });
        }
    }

    let status_filter = match params.status_filter() {
        Ok(filter) => filter,
        Err(error) => return error.into_response(),
    };

    if !details.is_empty() {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .into_response();
    }

    let limit = params.limit;

    let now = Utc::now();
    let mut where_clauses = vec!["ti.tenant_id = $1".to_string()];
    let mut bind_index = 2usize;

    match status_filter {
        InvitationStatusFilter::Open => {
            where_clauses.push("ti.status IN ('pending', 'expired')".to_string());
        }
        InvitationStatusFilter::ExpiredOnly => {
            where_clauses.push(format!(
                "(ti.status = 'expired' OR (ti.status = 'pending' AND ti.expires_at < ${bind_index}::timestamptz))"
            ));
            bind_index += 1;
        }
        InvitationStatusFilter::Exact(_) => {
            where_clauses.push(format!("ti.status = ${bind_index}"));
            bind_index += 1;
        }
    }

    if params.cursor.is_some() {
        where_clauses.push(format!(
            "(ti.created_at, ti.id) < (${bind_index}::timestamptz, ${bind2}::uuid)",
            bind_index = bind_index,
            bind2 = bind_index + 1,
        ));
        bind_index += 2;
    }

    let sql = format!(
        "SELECT ti.id, ti.email, ti.role, ti.status, ti.email_delivery_status, u.display_name AS invited_by_name, ti.expires_at, ti.created_at \
         FROM tenant_invitations ti \
         JOIN users u ON u.id = ti.invited_by \
         WHERE {} \
         ORDER BY ti.created_at DESC, ti.id DESC \
         LIMIT ${bind_index}",
        where_clauses.join(" AND "),
    );

    let mut query = sqlx::query(&sql).bind(ctx.tenant_id);
    match status_filter {
        InvitationStatusFilter::Open => {}
        InvitationStatusFilter::ExpiredOnly => query = query.bind(now),
        InvitationStatusFilter::Exact(status) => query = query.bind(status),
    }

    if let Some(ref cursor) = params.cursor {
        if let Some((created_str, id_str)) = cursor.split_once('_') {
            if let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(created_str) {
                if let Ok(id) = Uuid::parse_str(id_str) {
                    query = query.bind(created_at);
                    query = query.bind(id);
                }
            }
        }
    }

    query = query.bind(i64::from(limit + 1));

    let rows = match query.fetch_all(&pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "failed to list invitations");
            return ApiError::internal_error("Failed to list invitations").into_response();
        }
    };

    let row_count = rows.len();
    let items: Vec<InvitationListItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| {
            let status: String = row.get("status");
            let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
            let derived_status = if status == "pending" && expires_at < now {
                "expired"
            } else {
                status.as_str()
            };

            InvitationListItem {
                id: row.get("id"),
                email: row.get("email"),
                role: row.get("role"),
                status: derived_status.to_string(),
                invited_by_name: row.get("invited_by_name"),
                expires_at,
                created_at: row.get("created_at"),
                email_delivery_status: match row.get::<String, _>("email_delivery_status").as_str()
                {
                    "queued" => email::EmailDeliveryStatus::Queued,
                    "sent" => email::EmailDeliveryStatus::Sent,
                    "failed" => email::EmailDeliveryStatus::Failed(
                        row.get::<Option<String>, _>("email_delivery_error")
                            .unwrap_or_default(),
                    ),
                    _ => email::EmailDeliveryStatus::Unconfigured,
                },
            }
        })
        .collect();

    let has_more = row_count > limit as usize;
    let next_cursor = items
        .last()
        .map(|last| format!("{}_{}", last.created_at.format("%+"), last.id));

    Json(Page {
        items,
        next_cursor,
        has_more,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// DELETE /tenant/members/invitations/{id}
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/tenant/members/invitations/{id}",
    tag = "members",
    operation_id = "revoke_member_invitation",
    summary = "Revoke a team-member invitation",
    description = "Mark a pending (or not-yet-swept expired) invitation as `revoked`. The \
                  invitation's role must be assignable by the actor's rank. Accepted and already \
                  revoked invitations are terminal and cannot be revoked. \
                  Requires permission: members.manage",
    params(("id" = Uuid, Path, description = "Invitation identifier")),
    responses(
        (status = 204, description = "Invitation revoked."),
        (status = 401, description = "Authentication required or insufficient rank.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Invitation not found.", body = ErrorEnvelope),
        (status = 409, description = "Invitation is not in a revocable state.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn revoke_invitation(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Response {
    // ---- Fetch the invitation (must belong to current tenant) ----
    let row = match sqlx::query_as::<_, (Uuid, String, String, String)>(
        "SELECT id, email, role, status \
         FROM tenant_invitations \
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return ApiError::not_found("Invitation not found").into_response(),
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response()
        }
    };

    let (_inv_id, email, role, status) = row;

    // ---- Pending invitations, including persisted expiry, can be explicitly
    // revoked. Accepted and already-revoked invitations are terminal. ----
    if status != "pending" && status != "expired" {
        return ApiError::conflict("Invitation cannot be revoked").into_response();
    }

    // ---- Check rank: invitation's role must be assignable by actor ----
    let target_role = match authz::TenantRole::from_str(&role) {
        Ok(r) => r,
        Err(_) => {
            return ApiError::internal_error("Invalid role stored on invitation").into_response();
        }
    };

    let actor_rank = match &ctx.tenant_role {
        Some(role) => role_rank(role),
        None => match members::staff_effective_rank(&ctx.permissions) {
            Some(r) => r,
            None => {
                return ApiError::unauthorized("Access denied").into_response();
            }
        },
    };

    if actor_rank < role_rank(&target_role) {
        return ApiError::unauthorized("Access denied").into_response();
    }

    // ---- Transaction: UPDATE + audit ----
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error(format!("Database transaction begin failed: {e}"))
                .into_response();
        }
    };

    let revoked = match sqlx::query(
        "UPDATE tenant_invitations \
         SET status = 'revoked', revoked_at = now(), revoked_by = $1 \
         WHERE id = $2 AND tenant_id = $3 AND status IN ('pending', 'expired')",
    )
    .bind(principal.user_id)
    .bind(id)
    .bind(ctx.tenant_id)
    .execute(&mut *tx)
    .await
    {
        Ok(result) => result.rows_affected(),
        Err(e) => {
            return ApiError::internal_error(format!("Database update failed: {e}"))
                .into_response();
        }
    };

    if revoked == 0 {
        return ApiError::conflict("Invitation cannot be revoked").into_response();
    }

    if let Err(e) = audit::record_member_invitation_revoked(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        id,
        &email,
        &role,
    )
    .await
    {
        return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
    }

    if let Err(e) = tx.commit().await {
        return ApiError::internal_error(format!("Transaction commit failed: {e}")).into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

// ---------------------------------------------------------------------------
// GET /invitations/{token}  (public)
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/invitations/{token}",
    tag = "invitations",
    operation_id = "preview_invitation",
    summary = "Preview an invitation",
    description = "Public endpoint that returns a non-revealing preview of a pending invitation \
                  (tenant name, invitee email, role, and expiry) so the accept page can render \
                  before sign-in. Returns 404 for unknown tokens, soft-deleted or suspended tenants, \
                  and accepted/revoked invitations; returns 410 for expired invitations.",
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 200, description = "Invitation preview returned.", body = PreviewInvitationResponse),
        (status = 404, description = "Invitation not found (or the tenant is not active).", body = ErrorEnvelope),
        (status = 410, description = "Invitation has expired.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(())
)]
pub async fn preview_invitation(
    State(pool): State<sqlx::PgPool>,
    Path(token): Path<String>,
) -> Response {
    let token_hash = match parse_token_hash(&token) {
        Some(hash) => hash,
        None => return ApiError::not_found("Invitation not found").into_response(),
    };

    // `t.deleted_at IS NULL` ensures a soft-deleted tenant is indistinguishable
    // from an unknown token (404), same as a suspended tenant below — the
    // contract requires both to be non-revealing.
    let row = match sqlx::query(
        "SELECT ti.id, ti.email, ti.role, ti.status, ti.expires_at, \
                t.name AS tenant_name, t.status AS tenant_status \
         FROM tenant_invitations ti \
         JOIN tenants t ON t.id = ti.tenant_id AND t.deleted_at IS NULL \
         WHERE ti.token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return ApiError::not_found("Invitation not found").into_response(),
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response()
        }
    };

    let status: String = row.get("status");
    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
    let tenant_status: String = row.get("tenant_status");

    if tenant_status != "active" {
        return ApiError::not_found("Invitation not found").into_response();
    }

    if status == "expired" || (status == "pending" && expires_at < Utc::now()) {
        return ApiError::gone("Invitation has expired").into_response();
    }

    if status != "pending" {
        return ApiError::not_found("Invitation not found").into_response();
    }

    let email: String = row.get("email");
    let role: String = row.get("role");
    let tenant_name: String = row.get("tenant_name");

    // Check if account exists
    let account_exists: bool =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
            .bind(&email)
            .fetch_one(&pool)
            .await
            .unwrap_or(false);

    Json(PreviewInvitationResponse {
        tenant_name,
        email,
        role,
        expires_at,
        account_exists,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// POST /invitations/{token}/accept  (public)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/invitations/{token}/accept",
    tag = "invitations",
    operation_id = "accept_invitation",
    summary = "Accept an invitation",
    description = "Public endpoint that accepts a pending invitation. If the caller is signed in \
                  and the principal's email matches the invitation, a membership is created for the \
                  existing user. Otherwise an anonymous accept creates a new user account (supplying \
                  `displayName` and `password`) and signs them in via a new `app_session` cookie. \
                  Returns 404 for unknown tokens, soft-deleted or suspended tenants, and \
                  accepted/revoked invitations; returns 410 for expired or already-used tokens.",
    params(("token" = String, Path, description = "Invitation token")),
    request_body = AcceptInvitationPayload,
    responses(
        (status = 200, description = "Invitation accepted. The response body is the same `MeResponse` shape returned by `GET /me`.", body = serde_json::Value),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Signed-in principal email does not match the invitation email.", body = ErrorEnvelope),
        (status = 404, description = "Invitation not found (or the tenant is not active).", body = ErrorEnvelope),
        (status = 409, description = "Already a member of this tenant, or an account with this email already exists.", body = ErrorEnvelope),
        (status = 410, description = "Invitation has expired or has already been used.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (per-field, e.g. invalid display name or password).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(())
)]
pub async fn accept_invitation(
    State(pool): State<sqlx::PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
    Path(token): Path<String>,
    maybe_principal: OptionalPrincipal,
    kernel::ApiJson(payload): kernel::ApiJson<AcceptInvitationPayload>,
) -> Response {
    let token_hash = match parse_token_hash(&token) {
        Some(hash) => hash,
        None => return ApiError::not_found("Invitation not found").into_response(),
    };

    // ---- Fetch invitation + tenant info ----
    // `t.deleted_at IS NULL` ensures a soft-deleted tenant is indistinguishable
    // from an unknown token (404), same as a suspended tenant below.
    let row = match sqlx::query(
        "SELECT ti.id, ti.tenant_id, ti.email, ti.role, ti.status, ti.expires_at, \
                ti.accepted_at, t.name AS tenant_name, t.status AS tenant_status \
         FROM tenant_invitations ti \
         JOIN tenants t ON t.id = ti.tenant_id AND t.deleted_at IS NULL \
         WHERE ti.token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return ApiError::not_found("Invitation not found").into_response(),
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response()
        }
    };

    let invitation_id: Uuid = row.get("id");
    let tenant_id: Uuid = row.get("tenant_id");
    let email: String = row.get("email");
    let role: String = row.get("role");
    let status: String = row.get("status");
    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
    let _tenant_name: String = row.get("tenant_name");
    let tenant_status: String = row.get("tenant_status");

    // ---- Guards ----
    if tenant_status != "active" {
        return ApiError::not_found("Invitation not found").into_response();
    }

    if status == "expired" || (status == "pending" && expires_at < Utc::now()) {
        return ApiError::gone("Invitation has expired").into_response();
    }

    if status == "revoked" {
        return ApiError::not_found("Invitation not found").into_response();
    }

    if status != "pending" {
        return ApiError::gone("Invitation has already been used").into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error(format!("Database transaction begin failed: {e}"))
                .into_response();
        }
    };

    let tenant_status_inside: Option<String> = sqlx::query_scalar(
        "SELECT status FROM tenants WHERE id = $1 AND deleted_at IS NULL FOR SHARE",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    if tenant_status_inside.as_deref() != Some("active") {
        return ApiError::not_found("Invitation not found").into_response();
    }

    // If signed in, verify email match
    if let Some(ref principal) = maybe_principal.0 {
        if principal.email.to_lowercase() != email.to_lowercase() {
            return ApiError::unauthorized("Invitation email does not match your account email")
                .into_response();
        }

        // Check membership (disabled or active) -> 409
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM tenant_memberships \
             WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(principal.user_id)
        .fetch_optional(&mut *tx)
        .await
        .unwrap_or(None);

        if let Some((m_status,)) = existing {
            if m_status == "disabled" {
                return ApiError::conflict("Your account is disabled in this tenant")
                    .into_response();
            }
            return ApiError::conflict("You are already a member of this tenant").into_response();
        }

        // ---- Signed-in path: just create membership ----
        let consumed = match sqlx::query(
            "UPDATE tenant_invitations \
             SET status = 'accepted', accepted_at = now(), accepted_user_id = $1 \
             WHERE id = $2 AND tenant_id = $3 AND status = 'pending' AND expires_at > now()",
        )
        .bind(principal.user_id)
        .bind(invitation_id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        {
            Ok(result) => result.rows_affected(),
            Err(e) => {
                return ApiError::internal_error(format!("Database update failed: {e}"))
                    .into_response();
            }
        };

        if consumed == 0 {
            return ApiError::gone("Invitation is no longer valid").into_response();
        }

        if let Err(e) = sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
             VALUES ($1, $2, $3, 'active')",
        )
        .bind(tenant_id)
        .bind(principal.user_id)
        .bind(&role)
        .execute(&mut *tx)
        .await
        {
            return ApiError::internal_error(format!("Membership insert failed: {e}"))
                .into_response();
        }

        if let Err(e) = audit::record_member_invitation_accepted(
            &mut tx,
            principal.user_id,
            tenant_id,
            invitation_id,
            &email,
            &role,
            principal.user_id,
        )
        .await
        {
            return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
        }

        if let Err(e) = tx.commit().await {
            return ApiError::internal_error(format!("Transaction commit failed: {e}"))
                .into_response();
        }

        match routes::build_me_response(
            &pool,
            principal.clone(),
            config.environment == config::Environment::Production,
        )
        .await
        {
            Ok(response) => return Json(response).into_response(),
            Err(error) => return error.into_response(),
        }
    }

    // ---- Anonymous path: create user + membership ----
    let account = match identity::routes::validate_account_creation(
        payload.display_name.as_deref(),
        payload.password.as_deref(),
    ) {
        Ok(account) => account,
        Err(error) => return error.into_response(),
    };

    // Check if account already exists
    let existing_user: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);

    if existing_user.is_some() {
        return ApiError::conflict(
            "An account with this email already exists. Please sign in first.",
        )
        .into_response();
    }

    let password = account.password;
    let display_name = account.display_name;

    // ---- Hash password ----
    let password_hash =
        match tokio::task::spawn_blocking(move || password::hash_password(&password)).await {
            Ok(Ok(hash)) => hash,
            _ => {
                return ApiError::internal_error("Failed to hash password").into_response();
            }
        };

    // ---- Transaction: create user + invitation + membership + audit ----
    let new_user_id: Uuid = match sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&email)
    .bind(&display_name)
    .bind(&password_hash)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        Err(sqlx::Error::Database(dbe)) if dbe.code().as_deref() == Some("23505") => {
            return ApiError::conflict("An account with this email already exists").into_response();
        }
        Err(e) => {
            return ApiError::internal_error(format!("User insert failed: {e}")).into_response();
        }
    };

    let consumed = match sqlx::query(
        "UPDATE tenant_invitations \
         SET status = 'accepted', accepted_at = now(), accepted_user_id = $1 \
         WHERE id = $2 AND tenant_id = $3 AND status = 'pending' AND expires_at > now()",
    )
    .bind(new_user_id)
    .bind(invitation_id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    {
        Ok(result) => result.rows_affected(),
        Err(e) => {
            return ApiError::internal_error(format!("Invitation update failed: {e}"))
                .into_response();
        }
    };

    if consumed == 0 {
        return ApiError::gone("Invitation is no longer valid").into_response();
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, 'active')",
    )
    .bind(tenant_id)
    .bind(new_user_id)
    .bind(&role)
    .execute(&mut *tx)
    .await
    {
        return ApiError::internal_error(format!("Membership insert failed: {e}")).into_response();
    }

    if let Err(e) = audit::record_member_invitation_accepted(
        &mut tx,
        new_user_id,
        tenant_id,
        invitation_id,
        &email,
        &role,
        new_user_id,
    )
    .await
    {
        return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
    }

    if let Err(e) = tx.commit().await {
        return ApiError::internal_error(format!("Transaction commit failed: {e}")).into_response();
    }

    // ---- Issue session cookie ----
    let (jwt, _, _) = match session::issue_token(
        &config.auth_jwt_secret,
        config.auth_session_ttl_seconds,
        new_user_id,
    ) {
        Ok(t) => t,
        Err(e) => {
            return ApiError::internal_error(format!("Session token issuance failed: {e}"))
                .into_response();
        }
    };
    let session_cookie = session::build_session_cookie(&jwt, config.auth_session_ttl_seconds);
    let principal = Principal {
        user_id: new_user_id,
        email,
        display_name,
        platform_role: None,
        invalid_platform_role: false,
    };

    match routes::build_me_response(
        &pool,
        principal,
        config.environment == config::Environment::Production,
    )
    .await
    {
        Ok(response) => ([(SET_COOKIE, session_cookie)], Json(response)).into_response(),
        Err(error) => error.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct FakeSender {
        configured: bool,
        succeeds: bool,
    }

    #[async_trait]
    impl email::EmailSender for FakeSender {
        fn is_configured(&self) -> bool {
            self.configured
        }

        async fn send(
            &self,
            _msg: email::EmailMessage,
        ) -> email::EmailDeliveryStatus {
            if !self.configured {
                email::EmailDeliveryStatus::Unconfigured
            } else if self.succeeds {
                email::EmailDeliveryStatus::Sent
            } else {
                email::EmailDeliveryStatus::Failed("smtp failed".into())
            }
        }
    }

    #[tokio::test]
    async fn send_invitation_email_returns_unconfigured_when_unconfigured() {
        let sender: Arc<dyn email::EmailSender> = Arc::new(FakeSender {
            configured: false,
            succeeds: true,
        });

        assert_eq!(
            send_invitation_email(
                sender,
                "user@example.com".into(),
                "https://app.test/invite/abc".into()
            )
            .await,
            email::EmailDeliveryStatus::Unconfigured
        );
    }

    #[tokio::test]
    async fn send_invitation_email_returns_sent_when_send_succeeds() {
        let sender: Arc<dyn email::EmailSender> = Arc::new(FakeSender {
            configured: true,
            succeeds: true,
        });

        assert_eq!(
            send_invitation_email(
                sender,
                "user@example.com".into(),
                "https://app.test/invite/abc".into()
            )
            .await,
            email::EmailDeliveryStatus::Sent
        );
    }

    #[tokio::test]
    async fn send_invitation_email_returns_failed_when_send_fails() {
        let sender: Arc<dyn email::EmailSender> = Arc::new(FakeSender {
            configured: true,
            succeeds: false,
        });

        let status = send_invitation_email(
            sender,
            "user@example.com".into(),
            "https://app.test/invite/abc".into(),
        )
        .await;
        assert!(matches!(
            status,
            email::EmailDeliveryStatus::Failed(_)
        ));
        assert_eq!(status.error_message(), Some("smtp failed"));
    }

    #[tokio::test]
    async fn failed_status_carries_error_message() {
        let sender: Arc<dyn email::EmailSender> = Arc::new(FakeSender {
            configured: true,
            succeeds: false,
        });

        let status = send_invitation_email(
            sender,
            "user@example.com".into(),
            "https://app.test/invite/abc".into(),
        )
        .await;
        assert_eq!(status.as_str(), "failed");
        assert_eq!(status.error_message(), Some("smtp failed"));
    }

    #[test]
    fn build_accept_url_joins_against_dashboard_base() {
        let url = build_accept_url("https://dashboard.example.com/app/", "abc123")
            .expect("base URL should be valid");

        assert_eq!(url, "https://dashboard.example.com/app/invite/abc123");
    }

    #[test]
    fn parse_token_hash_rejects_malformed_tokens() {
        assert!(parse_token_hash("abc").is_none());
        assert!(parse_token_hash("g".repeat(64).as_str()).is_none());
    }

    #[test]
    fn parse_token_hash_accepts_64_character_hex_tokens() {
        assert!(parse_token_hash(&"a".repeat(64)).is_some());
    }

    /// Concurrent-replacement proof: when two callers simultaneously try to
    /// create an invitation for the same expired (tenant, email) pair, the
    /// transaction — atomic SELECT + UPDATE + INSERT — plus the partial unique
    /// index on `(tenant_id, email) WHERE status = 'pending'` ensure exactly
    /// one succeeds. The loser gets a 23505 (unique-violation) at INSERT time.
    ///
    /// This test requires a real Postgres instance and is excluded from the
    /// default `cargo test` run. Run it explicitly with:
    /// ```sh
    /// DATABASE_URL=postgres://... cargo test concurrent_expired -- --ignored
    /// ```
    #[tokio::test]
    #[ignore]
    async fn concurrent_expired_replacement_only_one_succeeds() {
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for this test");
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("failed to connect to database");

        let tenant_id = Uuid::new_v4();
        let inviter_id = Uuid::new_v4();
        let email = "concurrent@test.com".to_lowercase();

        sqlx::query(
            "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at, status, email_delivery_status) \
             VALUES ($1, $2, 'agent', $3, $4, now() - interval '1 day', 'pending', 'unconfigured')",
        )
        .bind(tenant_id)
        .bind(&email)
        .bind(Uuid::new_v4().to_string())
        .bind(inviter_id)
        .execute(&pool)
        .await
        .unwrap();

        let pool1 = pool.clone();
        let pool2 = pool.clone();
        let email1 = email.clone();
        let email2 = email.clone();

        let (res1, res2) = tokio::join!(
            tokio::spawn(async move {
                let result: std::result::Result<(), sqlx::Error> = async {
                    let mut tx = pool1.begin().await?;

                    let expired: Option<(Uuid,)> = sqlx::query_as(
                        "SELECT id FROM tenant_invitations \
                         WHERE tenant_id = $1 AND email = $2 AND status = 'pending' AND expires_at <= now() \
                         LIMIT 1",
                    )
                    .bind(tenant_id)
                    .bind(&email1)
                    .fetch_optional(&mut *tx)
                    .await?;

                    if let Some((old_id,)) = &expired {
                        sqlx::query(
                            "UPDATE tenant_invitations SET status = 'revoked', revoked_at = now(), revoked_by = $1 WHERE id = $2",
                        )
                        .bind(inviter_id)
                        .bind(old_id)
                        .execute(&mut *tx)
                        .await?;
                    }

                    let (_, new_hash) = generate_token();
                    sqlx::query(
                        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at, email_delivery_status) \
                         VALUES ($1, $2, 'agent', $3, $4, now() + interval '7 days', 'unconfigured')",
                    )
                    .bind(tenant_id)
                    .bind(&email1)
                    .bind(&new_hash)
                    .bind(inviter_id)
                    .execute(&mut *tx)
                    .await?;

                    tx.commit().await
                }
                .await;
                result.is_ok()
            }),
            tokio::spawn(async move {
                let result: std::result::Result<(), sqlx::Error> = async {
                    let mut tx = pool2.begin().await?;

                    let expired: Option<(Uuid,)> = sqlx::query_as(
                        "SELECT id FROM tenant_invitations \
                         WHERE tenant_id = $1 AND email = $2 AND status = 'pending' AND expires_at <= now() \
                         LIMIT 1",
                    )
                    .bind(tenant_id)
                    .bind(&email2)
                    .fetch_optional(&mut *tx)
                    .await?;

                    if let Some((old_id,)) = &expired {
                        sqlx::query(
                            "UPDATE tenant_invitations SET status = 'revoked', revoked_at = now(), revoked_by = $1 WHERE id = $2",
                        )
                        .bind(inviter_id)
                        .bind(old_id)
                        .execute(&mut *tx)
                        .await?;
                    }

                    let (_, new_hash) = generate_token();
                    sqlx::query(
                        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at, email_delivery_status) \
                         VALUES ($1, $2, 'agent', $3, $4, now() + interval '7 days', 'unconfigured')",
                    )
                    .bind(tenant_id)
                    .bind(&email2)
                    .bind(&new_hash)
                    .bind(inviter_id)
                    .execute(&mut *tx)
                    .await?;

                    tx.commit().await
                }
                .await;
                result.is_ok()
            }),
        );

        let successes = [res1.unwrap(), res2.unwrap()];
        assert_eq!(
            successes.iter().filter(|&&s| s).count(),
            1,
            "only one concurrent creator should succeed due to partial unique index"
        );

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tenant_invitations \
             WHERE tenant_id = $1 AND email = $2 AND status = 'pending'",
        )
        .bind(tenant_id)
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        assert_eq!(count, 1, "only one pending invitation should remain");
    }
}
