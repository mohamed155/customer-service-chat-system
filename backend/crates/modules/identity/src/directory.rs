//! What the other modules may ask `identity` about users (MOH-193, ADR-017).
//!
//! `users` is owned by `identity`. A module that needs a person's name, wants to know whether an
//! email is already registered, or creates an account asks here instead of naming the table in its
//! own SQL. Every function is one statement, so it takes any executor: a pool, a transaction, or
//! the connection of a tenant Unit of Work, and it runs inside whatever transaction the caller
//! already holds.
//!
//! `users` is not tenant-owned (a person can belong to several workspaces), so none of these
//! functions takes a tenant. A caller that must only show people of one tenant first gets the
//! user IDs from `tenancy::directory`, which is tenant-scoped, and passes those IDs here. These
//! functions answer for exactly the IDs they are given and never list users on their own.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// What a screen shows about a person: enough to label them, nothing that authenticates them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    pub id: Uuid,
    pub display_name: String,
    pub email: String,
    /// The platform staff role, when the person is platform staff.
    pub platform_role: Option<String>,
    /// Set when the account was deleted. The row stays so old records still show a name.
    pub deleted_at: Option<DateTime<Utc>>,
}

type ProfileRow = (Uuid, String, String, Option<String>, Option<DateTime<Utc>>);

fn profile_from_row(
    (id, display_name, email, platform_role, deleted_at): ProfileRow,
) -> UserProfile {
    UserProfile {
        id,
        display_name,
        email,
        platform_role,
        deleted_at,
    }
}

/// The profile of each of `user_ids` that exists, deleted accounts included (an old audit entry or
/// message still names the person who wrote it). IDs with no row are left out.
pub async fn profiles<'e>(
    executor: impl PgExecutor<'e>,
    user_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, UserProfile>> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, ProfileRow>(
        "SELECT id, display_name, email, platform_role, deleted_at \
         FROM users WHERE id = ANY($1)",
    )
    .bind(user_ids)
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let profile = profile_from_row(row);
            (profile.id, profile)
        })
        .collect())
}

/// The display name of each of `user_ids` that exists.
pub async fn display_names<'e>(
    executor: impl PgExecutor<'e>,
    user_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, String>> {
    Ok(profiles(executor, user_ids)
        .await?
        .into_iter()
        .map(|(id, profile)| (id, profile.display_name))
        .collect())
}

/// Which of `candidates` have a display name or an email that matches the `ILIKE` `pattern` (the
/// caller adds the `%`). `candidates` is how a caller keeps a search inside one tenant: it passes
/// that tenant's member user IDs, so a search never reads the rest of the user table.
pub async fn matching_name_or_email<'e>(
    executor: impl PgExecutor<'e>,
    candidates: &[Uuid],
    pattern: &str,
) -> sqlx::Result<Vec<Uuid>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar(
        "SELECT id FROM users \
         WHERE id = ANY($1) AND (display_name ILIKE $2 OR email ILIKE $2)",
    )
    .bind(candidates)
    .bind(pattern)
    .fetch_all(executor)
    .await
}

/// The ID of the account registered with `email` (exact match, the caller has lowercased it).
pub async fn user_id_by_email<'e>(
    executor: impl PgExecutor<'e>,
    email: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(executor)
        .await
}

/// Whether an account is registered with `email`.
pub async fn email_is_registered<'e>(
    executor: impl PgExecutor<'e>,
    email: &str,
) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(email)
        .fetch_one(executor)
        .await
}

/// Whether a live (not deleted) account has this ID.
pub async fn user_exists<'e>(executor: impl PgExecutor<'e>, user_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND deleted_at IS NULL)")
        .bind(user_id)
        .fetch_one(executor)
        .await
}

/// Whether the user is platform staff. An ID with no row is an error (`RowNotFound`), as the
/// caller has already resolved the user.
pub async fn is_platform_staff<'e>(
    executor: impl PgExecutor<'e>,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT platform_role IS NOT NULL FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(executor)
        .await
}

/// A live account by ID, as the row the principal is built from (`principal_from_row`).
pub async fn principal_row<'e>(
    executor: impl PgExecutor<'e>,
    user_id: Uuid,
) -> sqlx::Result<Option<(Uuid, String, String, Option<String>)>> {
    sqlx::query_as(
        "SELECT id, email, display_name, platform_role FROM users \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(executor)
    .await
}

/// Creates an account and returns its ID. The caller hashes the password first
/// (`identity::password::hash_password`) and maps a unique violation on `email` (SQLSTATE 23505)
/// to its own conflict answer, so the error is returned as it comes.
pub async fn create_user<'e>(
    executor: impl PgExecutor<'e>,
    email: &str,
    display_name: &str,
    password_hash: &str,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(email)
    .bind(display_name)
    .bind(password_hash)
    .fetch_one(executor)
    .await
}
