use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentPromptRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub prompt_kind: String,
    pub active_version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptVersionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub prompt_id: Uuid,
    pub version_number: i32,
    pub content: String,
    pub change_note: Option<String>,
    pub restored_from: Option<i32>,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_display: String,
    pub created_at: DateTime<Utc>,
}

pub async fn active_content(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT v.content FROM agent_prompts p \
         JOIN agent_prompt_versions v ON v.prompt_id = p.id AND v.version_number = p.active_version \
         WHERE p.tenant_id = $1 AND p.prompt_kind = 'system' AND p.deleted_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

pub async fn load_bootstrap(
    pool: &PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Option<(AgentPromptRow, PromptVersionRow)>> {
    let prompt = sqlx::query_as::<_, AgentPromptRow>(
        "SELECT * FROM agent_prompts \
         WHERE tenant_id = $1 AND prompt_kind = 'system' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    let prompt = match prompt {
        Some(p) => p,
        None => return Ok(None),
    };

    let version = sqlx::query_as::<_, PromptVersionRow>(
        "SELECT * FROM agent_prompt_versions \
         WHERE prompt_id = $1 AND version_number = $2",
    )
    .bind(prompt.id)
    .bind(prompt.active_version)
    .fetch_optional(pool)
    .await?;

    let version = match version {
        Some(v) => v,
        None => return Ok(None),
    };

    Ok(Some((prompt, version)))
}

pub async fn load_for_update_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> sqlx::Result<Option<AgentPromptRow>> {
    sqlx::query_as::<_, AgentPromptRow>(
        "SELECT * FROM agent_prompts \
         WHERE tenant_id = $1 AND prompt_kind = 'system' AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
}

#[derive(Debug, Clone, PartialEq)]
pub enum SaveOutcome {
    Created { version: i32, prompt_id: Uuid },
    NoOp { version: i32 },
}

#[derive(Debug)]
pub enum SaveError {
    Conflict { active_version: i32 },
    Db(sqlx::Error),
}

impl From<sqlx::Error> for SaveError {
    fn from(e: sqlx::Error) -> Self {
        SaveError::Db(e)
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn save_version_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    base_version: i32,
    content: &str,
    change_note: Option<&str>,
    actor_user_id: Option<Uuid>,
    actor_display: &str,
    restored_from: Option<i32>,
) -> Result<SaveOutcome, SaveError> {
    let existing = load_for_update_in_tx(tx, tenant_id).await?;

    let (prompt, is_new) = match existing {
        None => {
            if base_version != 0 {
                return Err(SaveError::Conflict { active_version: 0 });
            }
            let inserted = sqlx::query_as::<_, AgentPromptRow>(
                "INSERT INTO agent_prompts (tenant_id, prompt_kind, active_version) \
                 VALUES ($1, 'system', 1) RETURNING *",
            )
            .bind(tenant_id)
            .fetch_one(&mut **tx)
            .await?;
            (inserted, true)
        }
        Some(p) => {
            if base_version != p.active_version {
                return Err(SaveError::Conflict {
                    active_version: p.active_version,
                });
            }
            (p, false)
        }
    };

    if !is_new {
        let current_version = sqlx::query_as::<_, PromptVersionRow>(
            "SELECT * FROM agent_prompt_versions \
             WHERE prompt_id = $1 AND version_number = $2",
        )
        .bind(prompt.id)
        .bind(prompt.active_version)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(ref cv) = current_version {
            if cv.content == content {
                return Ok(SaveOutcome::NoOp {
                    version: prompt.active_version,
                });
            }
        }
    }

    let new_version_number = base_version + 1;

    let prompt_id = prompt.id;

    sqlx::query(
        "INSERT INTO agent_prompt_versions \
         (tenant_id, prompt_id, version_number, content, change_note, restored_from, \
          created_by_user_id, created_by_display) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(tenant_id)
    .bind(prompt_id)
    .bind(new_version_number)
    .bind(content)
    .bind(change_note)
    .bind(restored_from)
    .bind(actor_user_id)
    .bind(actor_display)
    .execute(&mut **tx)
    .await
    .map_err(|e| {
        if let Some(db_err) = e.as_database_error() {
            if db_err.is_unique_violation() {
                return SaveError::Conflict {
                    active_version: prompt.active_version,
                };
            }
        }
        SaveError::Db(e)
    })?;

    if !is_new {
        sqlx::query("UPDATE agent_prompts SET active_version = $1 WHERE id = $2")
            .bind(new_version_number)
            .bind(prompt_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(SaveOutcome::Created {
        version: new_version_number,
        prompt_id,
    })
}

pub async fn list_versions(
    pool: &PgPool,
    tenant_id: Uuid,
    limit: i64,
    before: Option<i32>,
) -> sqlx::Result<(Vec<PromptVersionRow>, bool)> {
    let fetch_limit = limit + 1;

    let rows = match before {
        Some(b) => {
            sqlx::query_as::<_, PromptVersionRow>(
                "SELECT v.* FROM agent_prompt_versions v \
                 JOIN agent_prompts p ON p.id = v.prompt_id \
                 WHERE p.tenant_id = $1 AND p.prompt_kind = 'system' AND p.deleted_at IS NULL \
                 AND v.version_number < $2 \
                 ORDER BY v.version_number DESC \
                 LIMIT $3",
            )
            .bind(tenant_id)
            .bind(b)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, PromptVersionRow>(
                "SELECT v.* FROM agent_prompt_versions v \
                 JOIN agent_prompts p ON p.id = v.prompt_id \
                 WHERE p.tenant_id = $1 AND p.prompt_kind = 'system' AND p.deleted_at IS NULL \
                 ORDER BY v.version_number DESC \
                 LIMIT $2",
            )
            .bind(tenant_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await?
        }
    };

    let has_more = rows.len() > limit as usize;
    let versions = if has_more {
        rows.into_iter().take(limit as usize).collect()
    } else {
        rows
    };

    Ok((versions, has_more))
}

pub async fn get_version(
    pool: &PgPool,
    tenant_id: Uuid,
    version_number: i32,
) -> sqlx::Result<Option<(PromptVersionRow, bool)>> {
    let row = sqlx::query_as::<_, PromptVersionRow>(
        "SELECT v.* FROM agent_prompt_versions v \
         JOIN agent_prompts p ON p.id = v.prompt_id \
         WHERE p.tenant_id = $1 AND p.prompt_kind = 'system' AND p.deleted_at IS NULL \
         AND v.version_number = $2",
    )
    .bind(tenant_id)
    .bind(version_number)
    .fetch_optional(pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let is_active: Option<(i32,)> = sqlx::query_as(
        "SELECT active_version FROM agent_prompts \
         WHERE id = $1 AND active_version = $2 AND deleted_at IS NULL",
    )
    .bind(row.prompt_id)
    .bind(version_number)
    .fetch_optional(pool)
    .await?;

    Ok(Some((row, is_active.is_some())))
}

pub async fn active_version_number(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<Option<i32>> {
    sqlx::query_scalar(
        "SELECT active_version FROM agent_prompts \
         WHERE tenant_id = $1 AND prompt_kind = 'system' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}
