use sqlx::PgPool;
use uuid::Uuid;

/// Sign-ins usable for access. A missing, revoked, or expired sign-in cannot keep an access
/// session open. The caller supplies a batch of sign-in IDs, never credentials.
pub async fn active_session_ids(pool: &PgPool, ids: &[Uuid]) -> sqlx::Result<Vec<Uuid>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar(
        "SELECT s.id FROM auth_sessions s JOIN users u ON u.id = s.user_id
         WHERE s.id = ANY($1) AND s.revoked_at IS NULL AND u.deleted_at IS NULL
           AND s.idle_expires_at > statement_timestamp()
           AND s.absolute_expires_at > statement_timestamp()",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
}
