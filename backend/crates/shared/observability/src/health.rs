use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct HealthReport {
    pub status: HealthStatus,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ready,
    NotReady,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Error,
}

#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &'static str;
    async fn check(&self) -> Result<(), String>;
}

impl IntoResponse for HealthReport {
    fn into_response(self) -> axum::response::Response {
        let status = match self.status {
            HealthStatus::Ready => StatusCode::OK,
            HealthStatus::NotReady => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(self)).into_response()
    }
}

pub async fn liveness() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

pub async fn readiness(checks: Vec<Arc<dyn HealthCheck>>, probe_timeout: Duration) -> HealthReport {
    let mut results = Vec::with_capacity(checks.len());
    for check in &checks {
        let result = tokio::time::timeout(probe_timeout, check.check()).await;
        let name = check.name().to_owned();
        let cr = match result {
            Ok(Ok(())) => CheckResult {
                name,
                status: CheckStatus::Ok,
                error: None,
            },
            Ok(Err(e)) => CheckResult {
                name,
                status: CheckStatus::Error,
                error: Some(e),
            },
            Err(_) => CheckResult {
                name,
                status: CheckStatus::Error,
                error: Some("timed out".to_owned()),
            },
        };
        results.push(cr);
    }
    let all_ok = results.iter().all(|r| r.status == CheckStatus::Ok);
    HealthReport {
        status: if all_ok {
            HealthStatus::Ready
        } else {
            HealthStatus::NotReady
        },
        checks: results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OkCheck(&'static str);
    #[async_trait::async_trait]
    impl HealthCheck for OkCheck {
        fn name(&self) -> &'static str {
            self.0
        }
        async fn check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    struct FailCheck(&'static str);
    #[async_trait::async_trait]
    impl HealthCheck for FailCheck {
        fn name(&self) -> &'static str {
            self.0
        }
        async fn check(&self) -> Result<(), String> {
            Err("something went wrong".to_owned())
        }
    }

    #[tokio::test]
    async fn report_is_ready_when_all_checks_ok() {
        let checks: Vec<Arc<dyn HealthCheck>> =
            vec![Arc::new(OkCheck("a")), Arc::new(OkCheck("b"))];
        let report = readiness(checks, Duration::from_secs(5)).await;
        assert_eq!(report.status, HealthStatus::Ready);
        assert_eq!(report.checks.len(), 2);
        assert_eq!(report.checks[0].status, CheckStatus::Ok);
        assert!(report.checks[0].error.is_none());
    }

    #[tokio::test]
    async fn report_has_error_field_on_failure() {
        let checks: Vec<Arc<dyn HealthCheck>> =
            vec![Arc::new(OkCheck("ok")), Arc::new(FailCheck("fail"))];
        let report = readiness(checks, Duration::from_secs(5)).await;
        assert_eq!(report.status, HealthStatus::NotReady);
        assert_eq!(report.checks[0].status, CheckStatus::Ok);
        assert_eq!(report.checks[1].status, CheckStatus::Error);
        assert_eq!(
            report.checks[1].error.as_deref(),
            Some("something went wrong")
        );
    }
}
