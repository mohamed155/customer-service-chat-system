use uuid::Uuid;

use crate::agent_config::{EscalationRule, EscalationTrigger};

pub const HUMAN_REQUEST_PHRASES: &[&str] = &[
    "talk to a human",
    "speak to a human",
    "speak to an agent",
    "talk to an agent",
    "real person",
    "human agent",
    "speak to someone",
    "customer service representative",
    "talk to a real person",
    "i want to speak to a human",
];

pub const BASELINE_ESCALATION_REASON: &str = "customer requested a human";
pub const UNCONFIGURED_ESCALATION_REASON: &str = "no AI agent configured";

pub fn matches_human_request(message_body: &str) -> bool {
    let lowered = message_body.to_lowercase();
    HUMAN_REQUEST_PHRASES
        .iter()
        .any(|phrase| lowered.contains(phrase))
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuleMatch {
    None,
    Baseline,
    Tenant {
        rule_id: Uuid,
        rule_name: String,
        required_skill_ids: Vec<Uuid>,
    },
}

pub fn evaluate(message_body: &str, rules: &[EscalationRule]) -> RuleMatch {
    if matches_human_request(message_body) {
        return RuleMatch::Baseline;
    }

    let lowered = message_body.to_lowercase();
    for rule in rules {
        let matches = match &rule.trigger {
            EscalationTrigger::HumanRequest => matches_human_request(message_body),
            EscalationTrigger::TopicKeywords => rule
                .keywords
                .iter()
                .any(|kw| lowered.contains(&kw.to_lowercase())),
        };
        if matches {
            return RuleMatch::Tenant {
                rule_id: rule.id,
                rule_name: rule.name.clone(),
                required_skill_ids: rule.required_skill_ids.clone(),
            };
        }
    }

    RuleMatch::None
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn make_rule(
        id: &str,
        name: &str,
        trigger: EscalationTrigger,
        keywords: Vec<&str>,
    ) -> EscalationRule {
        EscalationRule {
            id: Uuid::parse_str(id).unwrap(),
            name: name.to_string(),
            trigger,
            keywords: keywords.into_iter().map(|s| s.to_string()).collect(),
            required_skill_ids: vec![],
        }
    }

    #[test]
    fn baseline_match_with_no_rules() {
        let result = evaluate("I want to talk to a human", &[]);
        assert_eq!(result, RuleMatch::Baseline);
    }

    #[test]
    fn topic_keyword_match() {
        let rules = vec![make_rule(
            "00000000-0000-0000-0000-000000000001",
            "billing",
            EscalationTrigger::TopicKeywords,
            vec!["billing", "invoice"],
        )];
        let result = evaluate("I have a billing question", &rules);
        assert_eq!(
            result,
            RuleMatch::Tenant {
                rule_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
                rule_name: "billing".to_string(),
                required_skill_ids: vec![],
            }
        );
    }

    #[test]
    fn baseline_takes_priority_over_keyword() {
        let rules = vec![make_rule(
            "00000000-0000-0000-0000-000000000002",
            "billing",
            EscalationTrigger::TopicKeywords,
            vec!["human"],
        )];
        let result = evaluate("I want to talk to a human about billing", &rules);
        assert_eq!(result, RuleMatch::Baseline);
    }

    #[test]
    fn first_rule_wins() {
        let rules = vec![
            make_rule(
                "00000000-0000-0000-0000-000000000003",
                "billing",
                EscalationTrigger::TopicKeywords,
                vec!["billing"],
            ),
            make_rule(
                "00000000-0000-0000-0000-000000000004",
                "support",
                EscalationTrigger::TopicKeywords,
                vec!["support"],
            ),
        ];
        let result = evaluate("I need support for billing", &rules);
        assert_eq!(
            result,
            RuleMatch::Tenant {
                rule_id: Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
                rule_name: "billing".to_string(),
                required_skill_ids: vec![],
            }
        );
    }

    #[test]
    fn keyword_matching_is_case_insensitive() {
        let rules = vec![make_rule(
            "00000000-0000-0000-0000-000000000005",
            "refund",
            EscalationTrigger::TopicKeywords,
            vec!["refund"],
        )];
        let result = evaluate("I want a REFUND please", &rules);
        assert_eq!(
            result,
            RuleMatch::Tenant {
                rule_id: Uuid::parse_str("00000000-0000-0000-0000-000000000005").unwrap(),
                rule_name: "refund".to_string(),
                required_skill_ids: vec![],
            }
        );
    }
}
