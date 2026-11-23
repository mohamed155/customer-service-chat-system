use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReasonError {
    Required,
    TooShort,
    TooLong,
    InvalidValue,
}

pub(crate) fn validate_reason(input: Option<&str>) -> Result<&str, ReasonError> {
    let reason = input.unwrap_or_default().trim();
    match reason.chars().count() {
        0 => Err(ReasonError::Required),
        1..=9 => Err(ReasonError::TooShort),
        501.. => Err(ReasonError::TooLong),
        _ if reason.contains('\0') => Err(ReasonError::InvalidValue),
        _ => Ok(reason),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DurationError;

pub(crate) fn validate_duration(input: Option<i32>) -> Result<i32, DurationError> {
    let minutes = input.unwrap_or(60);
    if (5..=240).contains(&minutes) {
        Ok(minutes)
    } else {
        Err(DurationError)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccessDenial {
    Expired,
    Required,
}

pub(crate) fn access_denial(
    end_reason: Option<&str>,
    expires_at: DateTime<Utc>,
    observed_at: DateTime<Utc>,
    another_is_open: bool,
) -> Option<AccessDenial> {
    if another_is_open {
        return Some(AccessDenial::Required);
    }
    match end_reason {
        Some("expired") => Some(AccessDenial::Expired),
        Some(_) => Some(AccessDenial::Required),
        None if observed_at >= expires_at => Some(AccessDenial::Expired),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_are_trimmed_and_count_unicode_characters() {
        assert_eq!(validate_reason(None), Err(ReasonError::Required));
        assert_eq!(validate_reason(Some(" \t\n ")), Err(ReasonError::Required));
        assert_eq!(
            validate_reason(Some("123456789")),
            Err(ReasonError::TooShort)
        );
        assert_eq!(validate_reason(Some("  0123456789  ")), Ok("0123456789"));

        let unicode_reason = "界".repeat(500);
        assert_eq!(
            validate_reason(Some(&unicode_reason)),
            Ok(unicode_reason.as_str())
        );
        assert_eq!(
            validate_reason(Some(&"界".repeat(501))),
            Err(ReasonError::TooLong)
        );
        assert_eq!(
            validate_reason(Some("contains \0 a null")),
            Err(ReasonError::InvalidValue)
        );
    }

    #[test]
    fn durations_default_to_an_hour_and_accept_the_inclusive_limits() {
        assert_eq!(validate_duration(None), Ok(60));
        assert_eq!(validate_duration(Some(5)), Ok(5));
        assert_eq!(validate_duration(Some(240)), Ok(240));
        assert_eq!(validate_duration(Some(4)), Err(DurationError));
        assert_eq!(validate_duration(Some(241)), Err(DurationError));
    }

    #[test]
    fn expiry_is_distinguishable_from_other_session_endings() {
        let expiry = DateTime::UNIX_EPOCH;
        let just_before = expiry - chrono::TimeDelta::microseconds(1);
        assert_eq!(access_denial(None, expiry, just_before, false), None);
        assert_eq!(
            access_denial(None, expiry, expiry, false),
            Some(AccessDenial::Expired)
        );
        assert_eq!(
            access_denial(Some("expired"), expiry, expiry, false),
            Some(AccessDenial::Expired)
        );
        assert_eq!(
            access_denial(Some("expired"), expiry, expiry, true),
            Some(AccessDenial::Required)
        );
        for reason in ["ended", "replaced", "signed_out"] {
            assert_eq!(
                access_denial(Some(reason), expiry, expiry, false),
                Some(AccessDenial::Required)
            );
        }
    }
}
