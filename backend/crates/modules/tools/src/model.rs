#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource {
    Builtin,
    Tenant,
}

impl ToolSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Tenant => "tenant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    Auto,
    Approval,
}

impl Classification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Approval => "approval",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRequestStatus {
    Pending,
    Refused,
    AwaitingApproval,
    Approved,
    Executing,
    Succeeded,
    Failed,
    TimedOut,
    Denied,
    Expired,
    Cancelled,
}

impl ToolRequestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Refused => "refused",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Approved => "approved",
            Self::Executing => "executing",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Refused
                | Self::Succeeded
                | Self::Failed
                | Self::TimedOut
                | Self::Denied
                | Self::Expired
                | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone)]
pub struct CustomerToolProfile {
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub conversation_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_source_as_str() {
        assert_eq!(ToolSource::Builtin.as_str(), "builtin");
        assert_eq!(ToolSource::Tenant.as_str(), "tenant");
    }

    #[test]
    fn classification_as_str() {
        assert_eq!(Classification::Auto.as_str(), "auto");
        assert_eq!(Classification::Approval.as_str(), "approval");
    }

    #[test]
    fn tool_request_status_as_str() {
        assert_eq!(ToolRequestStatus::Pending.as_str(), "pending");
        assert_eq!(ToolRequestStatus::Refused.as_str(), "refused");
        assert_eq!(
            ToolRequestStatus::AwaitingApproval.as_str(),
            "awaiting_approval"
        );
        assert_eq!(ToolRequestStatus::Approved.as_str(), "approved");
        assert_eq!(ToolRequestStatus::Executing.as_str(), "executing");
        assert_eq!(ToolRequestStatus::Succeeded.as_str(), "succeeded");
        assert_eq!(ToolRequestStatus::Failed.as_str(), "failed");
        assert_eq!(ToolRequestStatus::TimedOut.as_str(), "timed_out");
        assert_eq!(ToolRequestStatus::Denied.as_str(), "denied");
        assert_eq!(ToolRequestStatus::Expired.as_str(), "expired");
        assert_eq!(ToolRequestStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn is_terminal_true_for_terminal_statuses() {
        assert!(ToolRequestStatus::Refused.is_terminal());
        assert!(ToolRequestStatus::Succeeded.is_terminal());
        assert!(ToolRequestStatus::Failed.is_terminal());
        assert!(ToolRequestStatus::TimedOut.is_terminal());
        assert!(ToolRequestStatus::Denied.is_terminal());
        assert!(ToolRequestStatus::Expired.is_terminal());
        assert!(ToolRequestStatus::Cancelled.is_terminal());
    }

    #[test]
    fn is_terminal_false_for_non_terminal_statuses() {
        assert!(!ToolRequestStatus::Pending.is_terminal());
        assert!(!ToolRequestStatus::AwaitingApproval.is_terminal());
        assert!(!ToolRequestStatus::Approved.is_terminal());
        assert!(!ToolRequestStatus::Executing.is_terminal());
    }
}
