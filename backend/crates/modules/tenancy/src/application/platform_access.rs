use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{validate_duration, validate_reason, DurationError, ReasonError};

pub(crate) struct AccessTerms<'a> {
    pub reason: &'a str,
    pub duration_minutes: i32,
}

impl<'a> AccessTerms<'a> {
    pub fn validate(
        reason: Option<&'a str>,
        duration: Option<i32>,
    ) -> Result<Self, (Option<ReasonError>, Option<DurationError>)> {
        match (validate_reason(reason), validate_duration(duration)) {
            (Ok(reason), Ok(duration_minutes)) => Ok(Self {
                reason,
                duration_minutes,
            }),
            (reason, duration) => Err((reason.err(), duration.err())),
        }
    }
}

pub(crate) struct AccessSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub reason: String,
    pub started_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub end_reason: Option<String>,
}

pub(crate) struct SelectedTenant {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan: String,
}
