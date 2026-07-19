use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use kernel::ApiError;
use sqlx::PgPool;

use crate::model::FeedbackSummaryDto;
use crate::queries;

#[utoipa::path(
    get,
    path = "/tenant/feedback/summary",
    tag = "conversations",
    operation_id = "get_feedback_summary",
    summary = "Get tenant-wide feedback summary",
    responses(
        (status = 200, description = "Feedback summary.", body = FeedbackSummaryDto),
    ),
)]
pub async fn get_feedback_summary(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
) -> Response {
    let (average_rating, feedback_count) =
        match queries::feedback_summary(&pool, ctx.tenant_id).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(%e, "get_feedback_summary: db error");
                return ApiError::internal_error("Failed to load summary").into_response();
            }
        };

    let average_rating = average_rating.map(|v| (v * 10.0).round() / 10.0);

    (
        StatusCode::OK,
        Json(FeedbackSummaryDto {
            average_rating,
            feedback_count,
        }),
    )
        .into_response()
}
