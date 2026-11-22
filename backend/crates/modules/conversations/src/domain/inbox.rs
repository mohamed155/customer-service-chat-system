//! What the inbox can be filtered on, and when a conversation waits for an AI decision.

use uuid::Uuid;

use super::channel::is_known_channel;
use super::status::ConversationStatus;

/// The most conversations or messages one page returns.
pub const MAX_PAGE: u32 = 100;

/// Which statuses the inbox shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    /// Every status (`status=all`).
    All,
    /// One status. An inbox with no `status` parameter shows `Only(Open)`.
    Only(ConversationStatus),
}

/// Whose conversations the inbox shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssigneeFilter {
    /// Assigned or not (no `assignee` parameter).
    Any,
    /// The member who is asking (`assignee=me`).
    Me,
    /// Nobody (`assignee=unassigned`).
    Unassigned,
    /// One member.
    Member(Uuid),
}

/// A filter value the inbox does not accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    Status(String),
    Assignee(String),
    Channel(String),
}

/// The validated filters of one inbox request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxFilter {
    pub status: StatusFilter,
    pub assignee: AssigneeFilter,
    pub channel: Option<String>,
    /// `Some(true)` only conversations with an active escalation, `Some(false)` only those
    /// without.
    pub escalated: Option<bool>,
}

impl InboxFilter {
    /// Validates the raw filter parameters of an inbox request, reporting the first one that is
    /// not acceptable: status, then assignee, then channel.
    pub fn parse(
        status: Option<&str>,
        assignee: Option<&str>,
        channel: Option<&str>,
        escalated: Option<bool>,
    ) -> Result<Self, FilterError> {
        let status = match status {
            None => StatusFilter::Only(ConversationStatus::Open),
            Some("all") => StatusFilter::All,
            Some(name) => StatusFilter::Only(
                ConversationStatus::parse(name)
                    .ok_or_else(|| FilterError::Status(name.to_owned()))?,
            ),
        };
        let assignee = match assignee {
            None => AssigneeFilter::Any,
            Some("me") => AssigneeFilter::Me,
            Some("unassigned") => AssigneeFilter::Unassigned,
            Some(other) => AssigneeFilter::Member(
                Uuid::parse_str(other).map_err(|_| FilterError::Assignee(other.to_owned()))?,
            ),
        };
        if let Some(channel) = channel {
            if !is_known_channel(channel) {
                return Err(FilterError::Channel(channel.to_owned()));
            }
        }
        Ok(Self {
            status,
            assignee,
            channel: channel.map(str::to_owned),
            escalated,
        })
    }

    /// The same filter with `assignee=me` resolved to the member who is asking.
    pub fn for_member(mut self, acting_membership_id: Uuid) -> Self {
        if self.assignee == AssigneeFilter::Me {
            self.assignee = AssigneeFilter::Member(acting_membership_id);
        }
        self
    }
}

/// A page size from a request: at least one, at most [`MAX_PAGE`].
pub fn clamp_page_size(requested: u32) -> i64 {
    i64::from(requested.clamp(1, MAX_PAGE))
}

/// Whether the dashboard should flag a conversation as waiting for a decision about AI handling:
/// the tenant has no agent configured, the customer has been sent the automatic acknowledgement,
/// and the conversation is not already being handled by a person or by an available platform AI.
pub fn awaiting_ai_decision(
    agent_configured: bool,
    has_system_ack: bool,
    platform_ai_available: bool,
    ai_handling: Option<&str>,
) -> bool {
    if agent_configured || !has_system_ack {
        return false;
    }
    match ai_handling {
        None => true,
        Some("platform_ai") => !platform_ai_available,
        Some("human") => false,
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(
        status: Option<&str>,
        assignee: Option<&str>,
        channel: Option<&str>,
    ) -> Result<InboxFilter, FilterError> {
        InboxFilter::parse(status, assignee, channel, None)
    }

    #[test]
    fn no_filters_shows_the_open_conversations_of_everyone() {
        let filter = parse(None, None, None).unwrap();
        assert_eq!(filter.status, StatusFilter::Only(ConversationStatus::Open));
        assert_eq!(filter.assignee, AssigneeFilter::Any);
        assert_eq!(filter.channel, None);
        assert_eq!(filter.escalated, None);
    }

    #[test]
    fn all_lifts_the_status_filter() {
        assert_eq!(
            parse(Some("all"), None, None).unwrap().status,
            StatusFilter::All
        );
    }

    #[test]
    fn a_status_must_be_a_status() {
        assert_eq!(
            parse(Some("resolved"), None, None).unwrap().status,
            StatusFilter::Only(ConversationStatus::Resolved)
        );
        assert_eq!(
            parse(Some("done"), None, None),
            Err(FilterError::Status("done".into()))
        );
    }

    #[test]
    fn an_assignee_is_me_unassigned_or_a_membership() {
        let member = Uuid::from_u128(9);
        assert_eq!(
            parse(None, Some("me"), None).unwrap().assignee,
            AssigneeFilter::Me
        );
        assert_eq!(
            parse(None, Some("unassigned"), None).unwrap().assignee,
            AssigneeFilter::Unassigned
        );
        assert_eq!(
            parse(None, Some(&member.to_string()), None)
                .unwrap()
                .assignee,
            AssigneeFilter::Member(member)
        );
        assert_eq!(
            parse(None, Some("bob"), None),
            Err(FilterError::Assignee("bob".into()))
        );
    }

    #[test]
    fn a_channel_must_be_a_known_one() {
        assert_eq!(
            parse(None, None, Some("widget"))
                .unwrap()
                .channel
                .as_deref(),
            Some("widget")
        );
        assert_eq!(
            parse(None, None, Some("fax")),
            Err(FilterError::Channel("fax".into()))
        );
    }

    #[test]
    fn the_first_bad_filter_is_the_one_reported() {
        assert_eq!(
            parse(Some("x"), Some("y"), Some("z")),
            Err(FilterError::Status("x".into()))
        );
        assert_eq!(
            parse(None, Some("y"), Some("z")),
            Err(FilterError::Assignee("y".into()))
        );
    }

    #[test]
    fn me_becomes_the_asking_member_and_nothing_else_changes() {
        let me = Uuid::from_u128(5);
        let filter = parse(None, Some("me"), None).unwrap().for_member(me);
        assert_eq!(filter.assignee, AssigneeFilter::Member(me));
        let other = Uuid::from_u128(6);
        let filter = parse(None, Some(&other.to_string()), None)
            .unwrap()
            .for_member(me);
        assert_eq!(filter.assignee, AssigneeFilter::Member(other));
        let filter = parse(None, Some("unassigned"), None)
            .unwrap()
            .for_member(me);
        assert_eq!(filter.assignee, AssigneeFilter::Unassigned);
    }

    #[test]
    fn a_page_is_between_one_and_a_hundred() {
        assert_eq!(clamp_page_size(0), 1);
        assert_eq!(clamp_page_size(25), 25);
        assert_eq!(clamp_page_size(100), 100);
        assert_eq!(clamp_page_size(5000), 100);
    }

    #[test]
    fn a_configured_agent_decides_for_itself() {
        assert!(!awaiting_ai_decision(true, true, true, None));
        assert!(!awaiting_ai_decision(
            true,
            true,
            false,
            Some("platform_ai")
        ));
    }

    #[test]
    fn without_the_acknowledgement_nothing_waits() {
        assert!(!awaiting_ai_decision(false, false, true, None));
        assert!(!awaiting_ai_decision(false, false, false, Some("other")));
    }

    #[test]
    fn an_unhandled_acknowledged_conversation_waits() {
        assert!(awaiting_ai_decision(false, true, true, None));
        assert!(awaiting_ai_decision(
            false,
            true,
            false,
            Some("unknown-mode")
        ));
    }

    #[test]
    fn platform_ai_waits_only_while_it_is_unavailable() {
        assert!(!awaiting_ai_decision(
            false,
            true,
            true,
            Some("platform_ai")
        ));
        assert!(awaiting_ai_decision(
            false,
            true,
            false,
            Some("platform_ai")
        ));
    }

    #[test]
    fn a_person_handling_it_never_waits() {
        assert!(!awaiting_ai_decision(false, true, false, Some("human")));
        assert!(!awaiting_ai_decision(false, true, true, Some("human")));
    }
}
