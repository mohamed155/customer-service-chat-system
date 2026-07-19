use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use kernel::ApiError;
use sqlx::PgPool;

use crate::model;
use crate::queries;
use tenancy;

#[utoipa::path(
    get,
    path = "/tenant/analytics/summary",
    tag = "analytics",
    operation_id = "get_analytics_summary",
    summary = "Tenant analytics headline metrics",
    params(
        ("from" = Option<String>, Query, description = "Inclusive UTC start date YYYY-MM-DD"),
        ("to" = Option<String>, Query, description = "Inclusive UTC end date YYYY-MM-DD"),
        ("channel" = Option<String>, Query, description = "Optional channel filter"),
    ),
    responses(
        (status = 200, description = "Analytics summary.", body = model::AnalyticsSummaryDto),
        (status = 422, description = "Invalid query parameters."),
    ),
)]
pub async fn get_analytics_summary(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Query(query): Query<model::AnalyticsQuery>,
) -> Response {
    let resolved = match model::resolve_query(query, chrono::Utc::now().date_naive()) {
        Ok(r) => r,
        Err(msg) => return ApiError::unprocessable_entity(msg).into_response(),
    };

    let (volume, concluded, ai_resolved, handed_off) =
        match queries::conversation_counts(
            &pool,
            ctx.tenant_id,
            resolved.from_ts,
            resolved.to_ts,
            resolved.channel.as_deref(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(%e, "get_analytics_summary: conversation_counts failed");
                return ApiError::internal_error("Failed to load summary").into_response();
            }
        };

    let avg_first = match queries::avg_first_response_seconds(
        &pool,
        ctx.tenant_id,
        resolved.from_ts,
        resolved.to_ts,
        resolved.channel.as_deref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "get_analytics_summary: avg_first_response_seconds failed");
            return ApiError::internal_error("Failed to load summary").into_response();
        }
    };

    let avg_resp = match queries::avg_response_seconds(
        &pool,
        ctx.tenant_id,
        resolved.from_ts,
        resolved.to_ts,
        resolved.channel.as_deref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "get_analytics_summary: avg_response_seconds failed");
            return ApiError::internal_error("Failed to load summary").into_response();
        }
    };

    let (satisfaction_avg, satisfaction_count) =
        match queries::satisfaction(
            &pool,
            ctx.tenant_id,
            resolved.from_ts,
            resolved.to_ts,
            resolved.channel.as_deref(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(%e, "get_analytics_summary: satisfaction failed");
                return ApiError::internal_error("Failed to load summary").into_response();
            }
        };

    let channels = match queries::channel_breakdown(
        &pool,
        ctx.tenant_id,
        resolved.from_ts,
        resolved.to_ts,
    )
    .await
    {
        Ok(rows) => {
            let total: f64 = rows.iter().map(|(_, count)| *count as f64).sum();
            rows.into_iter()
                .map(|(channel, count)| model::ChannelBreakdownItem {
                    share: if total == 0.0 { 0.0 } else { count as f64 / total },
                    conversation_count: count,
                    channel,
                })
                .collect()
        }
        Err(e) => {
            tracing::error!(%e, "get_analytics_summary: channel_breakdown failed");
            return ApiError::internal_error("Failed to load summary").into_response();
        }
    };

    let (total_tokens, unattributed_tokens) = match &resolved.channel {
        Some(channel) => {
            let t = match queries::token_totals_for_channel(
                &pool,
                ctx.tenant_id,
                resolved.from_ts,
                resolved.to_ts,
                channel,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(%e, "get_analytics_summary: token_totals_for_channel failed");
                    return ApiError::internal_error("Failed to load summary").into_response();
                }
            };
            (t, 0)
        }
        None => match queries::token_totals(&pool, ctx.tenant_id, resolved.from_ts, resolved.to_ts)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(%e, "get_analytics_summary: token_totals failed");
                return ApiError::internal_error("Failed to load summary").into_response();
            }
        },
    };

    let ai_resolution_rate = if concluded == 0 {
        None
    } else {
        Some(ai_resolved as f64 / concluded as f64)
    };

    let handoff_rate = if volume == 0 {
        None
    } else {
        Some(handed_off as f64 / volume as f64)
    };

    let satisfaction_avg = satisfaction_avg.map(|v| (v * 10.0).round() / 10.0);

    let dto = model::AnalyticsSummaryDto {
        range: model::DateRangeDto {
            from: resolved.from_date,
            to: resolved.to_date,
        },
        channel: resolved.channel,
        conversation_volume: volume,
        concluded_count: concluded,
        ai_resolution_rate,
        handoff_rate,
        avg_first_response_seconds: avg_first,
        avg_response_seconds: avg_resp,
        satisfaction_avg,
        satisfaction_count,
        total_tokens,
        unattributed_tokens,
        channels,
    };

    (StatusCode::OK, Json(dto)).into_response()
}

#[utoipa::path(
    get,
    path = "/tenant/analytics/timeseries",
    tag = "analytics",
    operation_id = "get_analytics_timeseries",
    summary = "Tenant analytics daily time series",
    params(
        ("from" = Option<String>, Query, description = "Inclusive UTC start date YYYY-MM-DD"),
        ("to" = Option<String>, Query, description = "Inclusive UTC end date YYYY-MM-DD"),
        ("channel" = Option<String>, Query, description = "Optional channel filter"),
    ),
    responses(
        (status = 200, description = "Daily analytics series.", body = model::AnalyticsTimeseriesDto),
        (status = 422, description = "Invalid query parameters."),
    ),
)]
pub async fn get_analytics_timeseries(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Query(query): Query<model::AnalyticsQuery>,
) -> Response {
    let resolved = match model::resolve_query(query, chrono::Utc::now().date_naive()) {
        Ok(r) => r,
        Err(msg) => return ApiError::unprocessable_entity(msg).into_response(),
    };

    let rows = match queries::daily_series(
        &pool,
        ctx.tenant_id,
        resolved.from_date,
        resolved.to_date,
        resolved.from_ts,
        resolved.to_ts,
        resolved.channel.as_deref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "get_analytics_timeseries: daily_series failed");
            return ApiError::internal_error("Failed to load timeseries").into_response();
        }
    };

    let days: Vec<model::TimeseriesDay> = rows
        .into_iter()
        .map(|(date, volume, ai_resolved, handed_off, satisfaction_avg, satisfaction_count, total_tokens)| {
            model::TimeseriesDay {
                date,
                conversation_volume: volume,
                ai_resolved,
                handed_off,
                satisfaction_avg: satisfaction_avg.map(|v| (v * 10.0).round() / 10.0),
                satisfaction_count,
                total_tokens,
            }
        })
        .collect();

    let dto = model::AnalyticsTimeseriesDto {
        range: model::DateRangeDto {
            from: resolved.from_date,
            to: resolved.to_date,
        },
        channel: resolved.channel,
        days,
    };

    (StatusCode::OK, Json(dto)).into_response()
}
