use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use kernel::ApiError;
use observability::health::HealthCheck;
use sqlx::PgPool;
use storage::{ObjectStorage, StorageBackend};
use tokio::time::{Duration, timeout};
use tracing;

use crate::health;
use crate::model;
use crate::queries;

#[utoipa::path(
    get,
    path = "/platform/analytics/summary",
    tag = "platform-analytics",
    operation_id = "get_platform_analytics_summary",
    params(
        ("from" = Option<String>, Query, description = "Start date (YYYY-MM-DD)"),
        ("to" = Option<String>, Query, description = "End date (YYYY-MM-DD)"),
    ),
    responses(
        (status = 200, description = "Platform usage summary", body = model::PlatformUsageSummaryDto),
        (status = 422, description = "Invalid date range"),
    )
)]
pub async fn get_platform_analytics_summary(
    State(pool): State<PgPool>,
    Query(query): Query<model::SummaryQuery>,
) -> Response {
    let today = Utc::now().date_naive();
    let range = match model::resolve_range(query, today) {
        Ok(r) => r,
        Err(msg) => return ApiError::unprocessable_entity(msg).into_response(),
    };

    let (tenants_total, tenants_active, tenants_suspended, tenants_created) =
        match queries::tenant_counts(&pool, range.from_ts, range.to_ts).await {
            Ok(row) => row,
            Err(e) => {
                tracing::error!(%e, "failed to load tenant counts");
                return ApiError::internal_error("Failed to load platform summary").into_response();
            }
        };

    let active_convs = match queries::active_conversation_count(&pool).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(%e, "failed to load active conversation count");
            return ApiError::internal_error("Failed to load platform summary").into_response();
        }
    };

    let (total_calls, successful, failed, input_tokens, output_tokens) =
        match queries::ai_usage_totals(&pool, range.from_ts, range.to_ts).await {
            Ok(row) => row,
            Err(e) => {
                tracing::error!(%e, "failed to load AI usage totals");
                return ApiError::internal_error("Failed to load platform summary").into_response();
            }
        };

    let dto = model::PlatformUsageSummaryDto {
        range: model::DateRangeDto {
            from: range.from_date,
            to: range.to_date,
        },
        tenants: model::TenantCountsDto {
            total: tenants_total,
            active: tenants_active,
            suspended: tenants_suspended,
            created_in_range: tenants_created,
        },
        active_conversations: active_convs,
        ai_usage: model::AiUsageTotalsDto {
            total_calls,
            successful_calls: successful,
            failed_calls: failed,
            input_tokens,
            output_tokens,
        },
        generated_at: Utc::now(),
    };

    (StatusCode::OK, Json(dto)).into_response()
}

#[utoipa::path(
    get,
    path = "/platform/health",
    tag = "platform-health",
    operation_id = "get_platform_health",
    responses(
        (status = 200, description = "System health status", body = model::SystemHealthDto),
    )
)]
pub async fn get_platform_health(
    State(pool): State<PgPool>,
    Extension(checks): Extension<Vec<Arc<dyn HealthCheck>>>,
    Extension(storage): Extension<Arc<dyn ObjectStorage>>,
    Extension(backend): Extension<StorageBackend>,
) -> Response {
    let read_at = Utc::now();
    let mut deps = Vec::new();

    for check in &checks {
        let start = Instant::now();
        let result = timeout(Duration::from_secs(5), check.check()).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let (verdict, reason) = match result {
            Ok(Ok(())) => (model::HealthVerdict::Healthy, None),
            Ok(Err(e)) => (model::HealthVerdict::Unhealthy, Some(e)),
            Err(_) => (model::HealthVerdict::Unhealthy, Some("timed out".into())),
        };

        deps.push(model::DependencyHealthDto {
            name: check.name().to_string(),
            verdict,
            reason,
            probe_ms: elapsed_ms,
            detail: None,
        });
    }

    // Queue health
    let start = Instant::now();
    let queue_result = timeout(Duration::from_secs(5), queries::queue_health(&pool)).await;
    let probe_ms = start.elapsed().as_millis() as u64;

    match queue_result {
        Ok(Ok(row)) => {
            let (queue_pending, queue_oldest, queue_stuck) = row;
            let oldest_age = queue_oldest.map(|t| (Utc::now() - t).num_seconds().max(0));
            let (qv, qr) = health::queue_verdict(queue_pending, oldest_age, queue_stuck);
            deps.push(model::DependencyHealthDto {
                name: "queue".into(),
                verdict: qv,
                reason: qr,
                probe_ms,
                detail: Some(model::DependencyDetail::Queue {
                    pending_count: queue_pending,
                    oldest_pending_age_seconds: oldest_age,
                    stuck_retry_count: queue_stuck,
                }),
            });
        }
        Ok(Err(e)) => {
            tracing::error!(%e, "failed to query queue health");
            deps.push(model::DependencyHealthDto {
                name: "queue".into(),
                verdict: model::HealthVerdict::Unhealthy,
                reason: Some(e.to_string()),
                probe_ms,
                detail: Some(model::DependencyDetail::Queue {
                    pending_count: 0,
                    oldest_pending_age_seconds: None,
                    stuck_retry_count: 0,
                }),
            });
        }
        Err(_) => {
            deps.push(model::DependencyHealthDto {
                name: "queue".into(),
                verdict: model::HealthVerdict::Unhealthy,
                reason: Some("timed out".into()),
                probe_ms,
                detail: Some(model::DependencyDetail::Queue {
                    pending_count: 0,
                    oldest_pending_age_seconds: None,
                    stuck_retry_count: 0,
                }),
            });
        }
    }

    // Storage health
    let start = Instant::now();
    let probe_result = timeout(Duration::from_secs(5), storage.probe()).await;
    let probe_ms = start.elapsed().as_millis() as u64;

    let probe_ok = match probe_result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("timed out".into()),
    };
    let (sv, sr) = health::storage_verdict(backend, probe_ok);
    deps.push(model::DependencyHealthDto {
        name: "storage".into(),
        verdict: sv,
        reason: sr,
        probe_ms,
        detail: Some(model::DependencyDetail::Storage {
            backend: match backend.kind {
                storage::StorageKind::S3 => "s3".into(),
                storage::StorageKind::InMemory => "in_memory".into(),
            },
            s3_configured: backend.s3_configured,
        }),
    });

    let verdicts: Vec<_> = deps.iter().map(|d| d.verdict).collect();
    (
        StatusCode::OK,
        Json(model::SystemHealthDto {
            overall: health::overall(&verdicts),
            dependencies: deps,
            read_at,
        }),
    )
        .into_response()
}

#[utoipa::path(
    get,
    path = "/platform/ai-providers",
    tag = "platform-health",
    operation_id = "get_platform_ai_provider_status",
    params(
        ("window" = Option<String>, Query, description = "Window preset: 1h, 24h, 7d"),
    ),
    responses(
        (status = 200, description = "AI provider status", body = model::ProviderStatusDto),
        (status = 422, description = "Unknown window value"),
    )
)]
pub async fn get_platform_ai_provider_status(
    State(pool): State<PgPool>,
    Query(query): Query<model::WindowQuery>,
) -> Response {
    let now = Utc::now();
    let (from, to, preset) = match model::resolve_window(query, now) {
        Ok(w) => w,
        Err(msg) => return ApiError::unprocessable_entity(msg).into_response(),
    };

    let metrics_rows = match queries::provider_metrics(&pool, from, to).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(%e, "failed to load provider metrics");
            return ApiError::internal_error("Failed to load provider status").into_response();
        }
    };

    let mut metrics_map: std::collections::HashMap<String, _> = std::collections::HashMap::new();
    for row in metrics_rows {
        metrics_map.insert(row.0.clone(), row);
    }

    let providers: Vec<model::ProviderMetricsDto> = model::PROVIDERS
        .iter()
        .map(|name| {
            if let Some(row) = metrics_map.get(*name) {
                let total = row.1;
                let successes = row.2;
                let failures = row.3;
                let input_tokens = row.4;
                let output_tokens = row.5;
                let p50 = row.6;
                let p95 = row.7;

                let (failure_rate, p50_latency_ms, p95_latency_ms) = if total == 0 {
                    (None, None, None)
                } else {
                    (Some(failures as f64 / total as f64), p50, p95)
                };

                model::ProviderMetricsDto {
                    provider: name.to_string(),
                    verdict: health::provider_verdict(total, failures),
                    total_calls: total,
                    successes,
                    failures,
                    failure_rate,
                    p50_latency_ms,
                    p95_latency_ms,
                    input_tokens,
                    output_tokens,
                    failures_by_category: model::FailuresByCategoryDto {
                        authentication: row.8,
                        rate_limited: row.9,
                        unavailable: row.10,
                        timeout: row.11,
                        invalid_request: row.12,
                    },
                }
            } else {
                model::ProviderMetricsDto {
                    provider: name.to_string(),
                    verdict: health::provider_verdict(0, 0),
                    total_calls: 0,
                    successes: 0,
                    failures: 0,
                    failure_rate: None,
                    p50_latency_ms: None,
                    p95_latency_ms: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    failures_by_category: model::FailuresByCategoryDto {
                        authentication: 0,
                        rate_limited: 0,
                        unavailable: 0,
                        timeout: 0,
                        invalid_request: 0,
                    },
                }
            }
        })
        .collect();

    let recent = match queries::recent_provider_failures(&pool, from, to).await {
        Ok(rows) => rows
            .into_iter()
            .map(|r| model::ProviderFailureDto {
                provider: r.0,
                model: r.1,
                error_category: r.2,
                latency_ms: r.3 as i64,
                occurred_at: r.4,
            })
            .collect(),
        Err(e) => {
            tracing::error!(%e, "failed to load recent provider failures");
            return ApiError::internal_error("Failed to load provider status").into_response();
        }
    };

    (
        StatusCode::OK,
        Json(model::ProviderStatusDto {
            window: model::WindowDto { from, to, preset },
            providers,
            recent_failures: recent,
            generated_at: Utc::now(),
        }),
    )
        .into_response()
}
