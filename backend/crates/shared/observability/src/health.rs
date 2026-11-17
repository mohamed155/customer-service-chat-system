use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

/// What `/ready` says when a check fails because it did not answer in time.
pub const REASON_TIMED_OUT: &str = "timed out";
/// What `/ready` says when a check reports an error. The error itself is logged, never returned:
/// it can carry a host name, a port or a driver message.
pub const REASON_UNAVAILABLE: &str = "unavailable";
/// What the [`DrainingCheck`] reports once the process has been told to stop.
pub const REASON_SHUTTING_DOWN: &str = "shutting down";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct HealthReport {
    pub status: HealthStatus,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ready,
    NotReady,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    /// A short reason code (`unavailable`, `timed out`, `shutting down`), never the raw error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Error,
}

#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &'static str;
    /// `Err` carries the real error, for the log. `/ready` shows only a reason code (see
    /// [`readiness`]), except for [`REASON_SHUTTING_DOWN`].
    async fn check(&self) -> Result<(), String>;
}

/// A check that fails from the moment the process is told to stop, so a load balancer takes the
/// instance out of rotation before the server stops accepting connections (MOH-81).
pub struct DrainingCheck(Arc<AtomicBool>);

impl DrainingCheck {
    pub fn new(draining: Arc<AtomicBool>) -> Self {
        Self(draining)
    }
}

#[async_trait::async_trait]
impl HealthCheck for DrainingCheck {
    fn name(&self) -> &'static str {
        "shutdown"
    }

    async fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::SeqCst) {
            Err(REASON_SHUTTING_DOWN.to_owned())
        } else {
            Ok(())
        }
    }
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

/// Run every check, each with its own `probe_timeout`, and report the result. The report carries
/// the name and a status per dependency and, for a failure, a short reason code. The error text a
/// check returns is logged with the check's name and not returned: `/ready` is reachable by
/// whoever can reach the port, and a driver message can name a host, a port or a user.
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
            Ok(Err(e)) => {
                let reason = if e == REASON_SHUTTING_DOWN {
                    REASON_SHUTTING_DOWN
                } else {
                    tracing::warn!(check = %name, error = %e, "readiness check failed");
                    REASON_UNAVAILABLE
                };
                CheckResult {
                    name,
                    status: CheckStatus::Error,
                    error: Some(reason.to_owned()),
                }
            }
            Err(_) => {
                tracing::warn!(check = %name, timeout = ?probe_timeout, "readiness check timed out");
                CheckResult {
                    name,
                    status: CheckStatus::Error,
                    error: Some(REASON_TIMED_OUT.to_owned()),
                }
            }
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
        assert_eq!(report.checks[1].error.as_deref(), Some(REASON_UNAVAILABLE));
    }

    struct LeakyCheck;
    #[async_trait::async_trait]
    impl HealthCheck for LeakyCheck {
        fn name(&self) -> &'static str {
            "database"
        }
        async fn check(&self) -> Result<(), String> {
            Err("database error: connect to 10.1.2.3:5432 as app_runtime: password rejected".into())
        }
    }

    #[tokio::test]
    async fn the_raw_error_of_a_check_is_not_in_the_report() {
        let report = readiness(vec![Arc::new(LeakyCheck)], Duration::from_secs(5)).await;
        let body = serde_json::to_string(&report).unwrap();
        assert!(
            !body.contains("10.1.2.3") && !body.contains("password"),
            "{body}"
        );
        assert!(body.contains("\"database\"") && body.contains(REASON_UNAVAILABLE));
    }

    #[tokio::test]
    async fn the_draining_check_fails_once_the_flag_is_set_and_says_why() {
        let flag = Arc::new(AtomicBool::new(false));
        let checks: Vec<Arc<dyn HealthCheck>> = vec![
            Arc::new(OkCheck("db")),
            Arc::new(DrainingCheck::new(flag.clone())),
        ];

        let before = readiness(checks.clone(), Duration::from_secs(5)).await;
        assert_eq!(before.status, HealthStatus::Ready);

        flag.store(true, Ordering::SeqCst);
        let after = readiness(checks, Duration::from_secs(5)).await;
        assert_eq!(after.status, HealthStatus::NotReady);
        assert_eq!(
            after.checks[0].status,
            CheckStatus::Ok,
            "dependencies are still reported"
        );
        assert_eq!(after.checks[1].name, "shutdown");
        assert_eq!(after.checks[1].error.as_deref(), Some(REASON_SHUTTING_DOWN));
    }
}
