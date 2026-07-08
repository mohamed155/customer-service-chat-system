use axum::{
    extract::State,
    response::{IntoResponse, Json, Response},
};
use kernel::ApiError;
use serde::Serialize;
use crate::{authorize::fetch_tenant, TenantContext};

#[derive(Serialize)]
pub struct TenantSummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
}

pub async fn get_tenant(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
) -> Response {
    let row = match fetch_tenant(&pool, ctx.tenant_id).await {
        Some(r) => r,
        None => {
            return ApiError::internal_error("Tenant not found after middleware check")
                .into_response()
        }
    };

    Json(TenantSummary {
        id: row.id,
        name: row.name,
        slug: row.slug,
        status: row.status,
    })
    .into_response()
}