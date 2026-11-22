//! Who a conversation is assigned to, and who is told when that changes.

use uuid::Uuid;

/// The origin of an assignment made by the escalation routing engine. It tells the assignee
/// through its own escalation notification, so the assignment itself stays quiet.
pub const ESCALATION_ORIGIN: &str = "escalations";

/// An assignment that really changes the assignee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssignmentChange {
    pub from: Option<Uuid>,
    pub to: Option<Uuid>,
}

/// What asking for `requested` does to a conversation assigned to `prior`: nothing when it is
/// already assigned that way.
pub fn assignment_change(prior: Option<Uuid>, requested: Option<Uuid>) -> Option<AssignmentChange> {
    (prior != requested).then_some(AssignmentChange {
        from: prior,
        to: requested,
    })
}

/// Whether an assignment made with this `origin` may tell the new assignee about it. The routing
/// engine's assignments do not: it notifies about the escalation instead.
pub fn origin_tells_assignee(origin: &str) -> bool {
    origin != ESCALATION_ORIGIN
}

/// Whether the new assignee is told, given who made the assignment. Nobody needs a notification
/// for assigning themselves; an assignment with no acting member (a system actor) is announced.
pub fn assignee_is_told(actor_membership_id: Option<Uuid>, assigned_membership_id: Uuid) -> bool {
    actor_membership_id != Some(assigned_membership_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigning_the_current_assignee_changes_nothing() {
        let member = Uuid::from_u128(1);
        assert_eq!(assignment_change(Some(member), Some(member)), None);
        assert_eq!(assignment_change(None, None), None);
    }

    #[test]
    fn assigning_unassigning_and_reassigning_are_changes() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        assert_eq!(
            assignment_change(None, Some(a)),
            Some(AssignmentChange {
                from: None,
                to: Some(a)
            })
        );
        assert_eq!(
            assignment_change(Some(a), None),
            Some(AssignmentChange {
                from: Some(a),
                to: None
            })
        );
        assert_eq!(
            assignment_change(Some(a), Some(b)),
            Some(AssignmentChange {
                from: Some(a),
                to: Some(b)
            })
        );
    }

    #[test]
    fn the_routing_engine_does_not_notify_through_the_assignment() {
        assert!(!origin_tells_assignee("escalations"));
        assert!(origin_tells_assignee("conversations"));
        assert!(origin_tells_assignee("tools"));
    }

    #[test]
    fn assigning_yourself_tells_nobody() {
        let me = Uuid::from_u128(1);
        let them = Uuid::from_u128(2);
        assert!(!assignee_is_told(Some(me), me));
        assert!(assignee_is_told(Some(me), them));
    }

    #[test]
    fn an_assignment_without_an_acting_member_is_announced() {
        assert!(assignee_is_told(None, Uuid::from_u128(2)));
    }
}
