const TONE_DIRECTIVES: [(&str, &str); 5] = [
    (
        "professional",
        "Respond in a professional, courteous, and business-appropriate tone.",
    ),
    (
        "friendly",
        "Respond in a warm, approachable, conversational tone.",
    ),
    (
        "casual",
        "Respond in a relaxed, informal, and friendly manner.",
    ),
    (
        "formal",
        "Respond in a formal, precise, and structured tone.",
    ),
    (
        "empathetic",
        "Respond with warmth, understanding, and genuine empathy.",
    ),
];

pub fn compose_system_message(
    agent_name: &str,
    prompt_content: &str,
    tone: &str,
    business_rules: &[String],
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if !prompt_content.is_empty() {
        parts.push(prompt_content.to_string());
    }

    let tone_directive = TONE_DIRECTIVES
        .iter()
        .find(|(key, _)| *key == tone)
        .map(|(_, directive)| *directive)
        .unwrap_or("");

    if !tone_directive.is_empty() {
        parts.push(tone_directive.to_string());
    }

    if !business_rules.is_empty() {
        let mut rules_block = String::from("You must always follow these rules:");
        for (i, rule) in business_rules.iter().enumerate() {
            rules_block.push_str(&format!("\n{}. {}", i + 1, rule));
        }
        parts.push(rules_block);
    }

    let guardrail = format!(
        "You are {}, an AI assistant created to help customers. \
         You must never claim to be a human or impersonate a person.",
        agent_name
    );
    parts.push(guardrail);

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_equality() {
        let rules = vec!["Be polite.".to_string(), "Answer quickly.".to_string()];
        let a = compose_system_message("Alice", "You are helpful.", "friendly", &rules);
        let b = compose_system_message("Alice", "You are helpful.", "friendly", &rules);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_prompt_content_omits_section() {
        let result = compose_system_message("Bob", "", "formal", &[]);
        assert!(!result.contains("You are helpful"));
        assert!(!result.starts_with("\n\n"));
    }

    #[test]
    fn empty_business_rules_omits_header() {
        let result = compose_system_message("Carol", "Hello", "casual", &[]);
        assert!(!result.contains("You must always follow these rules:"));
    }

    #[test]
    fn composed_rendered_prompt_is_deterministic() {
        let content = "You are {{agent_name}}, helping {{customer_name}}.";
        let mut vars = std::collections::HashMap::new();
        vars.insert("agent_name", "SupportBot".to_string());
        vars.insert("customer_name", "Jane".to_string());
        let rendered = crate::prompt_validate::render_prompt(content, &vars);
        let rules = vec!["Be kind.".to_string()];
        let a = compose_system_message("Assistant", &rendered, "friendly", &rules);
        let b = compose_system_message("Assistant", &rendered, "friendly", &rules);
        assert_eq!(a, b);
        assert!(a.contains("SupportBot"));
        assert!(a.contains("Jane"));
    }

    #[test]
    fn each_tone_is_distinct() {
        let tones = ["professional", "friendly", "casual", "formal", "empathetic"];
        let mut outputs: Vec<String> = Vec::new();
        for t in &tones {
            let s = compose_system_message("Test", "Prompt", t, &["Rule".to_string()]);
            outputs.push(s);
        }
        for i in 0..outputs.len() {
            for j in (i + 1)..outputs.len() {
                assert_ne!(
                    outputs[i], outputs[j],
                    "tones {:?} and {:?} produced identical output (one likely missing)",
                    tones[i], tones[j]
                );
            }
        }
    }
}
