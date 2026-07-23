use crate::model::ConnectionStatus;

pub fn derive_status(is_active: Option<bool>, recent_outcomes: &[&str]) -> ConnectionStatus {
    match is_active {
        None => ConnectionStatus::NotConnected,
        Some(false) => ConnectionStatus::Disconnected,
        Some(true) => {
            if recent_outcomes.len() >= 3
                && recent_outcomes[0] == "failure"
                && recent_outcomes[1] == "failure"
                && recent_outcomes[2] == "failure"
            {
                ConnectionStatus::Error
            } else {
                ConnectionStatus::Connected
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_connected_when_no_row() {
        assert_eq!(derive_status(None, &[]), ConnectionStatus::NotConnected);
    }

    #[test]
    fn disconnected_when_inactive() {
        assert_eq!(
            derive_status(Some(false), &[]),
            ConnectionStatus::Disconnected
        );
    }

    #[test]
    fn connected_when_active_no_outcomes() {
        assert_eq!(
            derive_status(Some(true), &[]),
            ConnectionStatus::Connected
        );
    }

    #[test]
    fn error_when_three_consecutive_failures() {
        assert_eq!(
            derive_status(Some(true), &["failure", "failure", "failure"]),
            ConnectionStatus::Error
        );
    }

    #[test]
    fn connected_with_only_two_failures() {
        assert_eq!(
            derive_status(Some(true), &["failure", "failure"]),
            ConnectionStatus::Connected
        );
    }

    #[test]
    fn connected_with_failure_failure_success() {
        assert_eq!(
            derive_status(Some(true), &["failure", "failure", "success"]),
            ConnectionStatus::Connected
        );
    }

    #[test]
    fn connected_with_four_failures_first_three_drive_decision() {
        assert_eq!(
            derive_status(Some(true), &["failure", "failure", "failure", "success"]),
            ConnectionStatus::Error
        );
    }

    #[test]
    fn error_with_more_than_three_failures() {
        assert_eq!(
            derive_status(
                Some(true),
                &["failure", "failure", "failure", "failure", "failure"]
            ),
            ConnectionStatus::Error
        );
    }
}
