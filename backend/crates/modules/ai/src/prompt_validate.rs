use crate::agent_config::ValidationIssue;
use std::collections::HashMap;

pub struct PromptVariable {
    pub name: &'static str,
    pub description: &'static str,
    pub sample: &'static str,
}

pub const VARIABLES: &[PromptVariable] = &[
    PromptVariable {
        name: "agent_name",
        description: "The AI agent's customer-facing name",
        sample: "Aria",
    },
    PromptVariable {
        name: "tenant_name",
        description: "The tenant's business name",
        sample: "Acme Support",
    },
    PromptVariable {
        name: "customer_name",
        description: "The customer's display name",
        sample: "Jamie Lee",
    },
    PromptVariable {
        name: "channel",
        description: "The conversation's channel",
        sample: "web_chat",
    },
];

pub const MAX_CONTENT_LENGTH: usize = 8000;
pub const MAX_CHANGE_NOTE_LENGTH: usize = 500;

pub const STARTER_PROMPT: &str = "You are {{agent_name}}, the customer \
support assistant for {{tenant_name}}.\n\nHelp {{customer_name}} clearly \
and concisely. If you don't know an answer, say so and offer to connect \
them with a member of the team.";

fn is_valid_variable_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' {
            return false;
        }
    }
    true
}

pub fn validate_prompt(content: &str) -> Result<(), Vec<ValidationIssue>> {
    let mut issues: Vec<ValidationIssue> = Vec::new();

    let trimmed = content.trim();
    if trimmed.is_empty() {
        issues.push(ValidationIssue {
            field: "content".into(),
            code: "required".into(),
            message: "prompt content is required".into(),
        });
    }

    if content.chars().count() > MAX_CONTENT_LENGTH {
        issues.push(ValidationIssue {
            field: "content".into(),
            code: "too_long".into(),
            message: format!(
                "prompt content must not exceed {} characters",
                MAX_CONTENT_LENGTH
            ),
        });
    }

    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_placeholder = false;
    let mut placeholder_start = 0;
    let mut name = String::new();

    while i < len {
        let (offset, ch) = chars[i];

        if !in_placeholder {
            if ch == '{' && i + 1 < len && chars[i + 1].1 == '{' {
                in_placeholder = true;
                placeholder_start = offset;
                name.clear();
                i += 2;
                continue;
            }
            if ch == '}' && i + 1 < len && chars[i + 1].1 == '}' {
                issues.push(ValidationIssue {
                    field: "content".into(),
                    code: "malformed_placeholder".into(),
                    message: format!("stray closing braces at offset {}", offset),
                });
                i += 2;
                continue;
            }
        } else {
            if ch == '}' && i + 1 < len && chars[i + 1].1 == '}' {
                if name.is_empty() {
                    issues.push(ValidationIssue {
                        field: "content".into(),
                        code: "malformed_placeholder".into(),
                        message: format!("empty placeholder at offset {}", placeholder_start),
                    });
                } else if !is_valid_variable_name(&name) {
                    issues.push(ValidationIssue {
                        field: "content".into(),
                        code: "malformed_placeholder".into(),
                        message: format!(
                            "invalid placeholder '{}' at offset {}",
                            name, placeholder_start
                        ),
                    });
                } else if !VARIABLES.iter().any(|v| v.name == name) {
                    issues.push(ValidationIssue {
                        field: "content".into(),
                        code: "unknown_variable".into(),
                        message: format!(
                            "unknown variable '{}' at offset {}",
                            name, placeholder_start
                        ),
                    });
                }
                in_placeholder = false;
                i += 2;
                continue;
            }
            if ch == '{' && i + 1 < len && chars[i + 1].1 == '{' {
                issues.push(ValidationIssue {
                    field: "content".into(),
                    code: "malformed_placeholder".into(),
                    message: format!("unclosed placeholder at offset {}", placeholder_start),
                });
                in_placeholder = true;
                placeholder_start = offset;
                name.clear();
                i += 2;
                continue;
            }
            name.push(ch);
        }

        i += 1;
    }

    if in_placeholder {
        issues.push(ValidationIssue {
            field: "content".into(),
            code: "malformed_placeholder".into(),
            message: format!("unclosed placeholder at offset {}", placeholder_start),
        });
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

pub fn validate_change_note(note: &str) -> Result<(), Vec<ValidationIssue>> {
    if note.chars().count() > MAX_CHANGE_NOTE_LENGTH {
        return Err(vec![ValidationIssue {
            field: "changeNote".into(),
            code: "invalid_length".into(),
            message: format!(
                "change note must not exceed {} characters",
                MAX_CHANGE_NOTE_LENGTH
            ),
        }]);
    }
    Ok(())
}

pub fn render_prompt(content: &str, vars: &HashMap<&str, String>) -> String {
    let mut output = String::new();
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_placeholder = false;
    let mut placeholder_start = 0;
    let mut name = String::new();
    let mut last_end = 0;

    while i < len {
        let (offset, ch) = chars[i];

        if !in_placeholder {
            if ch == '{' && i + 1 < len && chars[i + 1].1 == '{' {
                output.push_str(&content[last_end..offset]);
                in_placeholder = true;
                placeholder_start = offset;
                name.clear();
                i += 2;
                continue;
            }
        } else {
            if ch == '}' && i + 1 < len && chars[i + 1].1 == '}' {
                let end_offset = offset + 2;
                let valid = !name.is_empty() && is_valid_variable_name(&name);

                if valid {
                    if let Some(value) = vars.get(name.as_str()) {
                        output.push_str(value);
                    } else {
                        output.push_str(&content[placeholder_start..end_offset]);
                    }
                } else {
                    output.push_str(&content[placeholder_start..end_offset]);
                }

                in_placeholder = false;
                last_end = end_offset;
                i += 2;
                continue;
            }
            if ch == '{' && i + 1 < len && chars[i + 1].1 == '{' {
                output.push_str(&content[placeholder_start..offset]);
                in_placeholder = true;
                placeholder_start = offset;
                name.clear();
                i += 2;
                continue;
            }
            name.push(ch);
        }

        i += 1;
    }

    if in_placeholder {
        output.push_str(&content[placeholder_start..]);
    } else {
        output.push_str(&content[last_end..]);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_prompt_ok() {
        let content =
            "Hello {{agent_name}} from {{tenant_name}}, serving {{customer_name}} via {{channel}}.";
        let result = validate_prompt(content);
        assert!(result.is_ok());
    }

    #[test]
    fn empty_fails() {
        let result = validate_prompt("");
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "required"));
    }

    #[test]
    fn whitespace_only_fails() {
        let result = validate_prompt("   \n  \t  ");
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "required"));
    }

    #[test]
    fn too_long_fails() {
        let content = "x".repeat(MAX_CONTENT_LENGTH + 1);
        let result = validate_prompt(&content);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "too_long"));
    }

    #[test]
    fn unclosed_placeholder() {
        let result = validate_prompt("{{agent_name");
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "malformed_placeholder"));
    }

    #[test]
    fn stray_closing_braces() {
        let result = validate_prompt("something }} here");
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "malformed_placeholder"));
    }

    #[test]
    fn empty_placeholder_name() {
        let result = validate_prompt("{{}}");
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "malformed_placeholder"));
    }

    #[test]
    fn invalid_placeholder_name() {
        let result = validate_prompt("{{Agent_Name}}");
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "malformed_placeholder"));
    }

    #[test]
    fn whitespace_in_braces() {
        let result = validate_prompt("{{ agent_name }}");
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "malformed_placeholder"));
    }

    #[test]
    fn unknown_variable() {
        let result = validate_prompt("{{business_hours}}");
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.code == "unknown_variable"));
    }

    #[test]
    fn multiple_issues() {
        let content = "{{Invalid!}} {{unknown_var}}";
        let result = validate_prompt(content);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn change_note_500_passes() {
        let note = "a".repeat(MAX_CHANGE_NOTE_LENGTH);
        let result = validate_change_note(&note);
        assert!(result.is_ok());
    }

    #[test]
    fn change_note_501_fails() {
        let note = "a".repeat(MAX_CHANGE_NOTE_LENGTH + 1);
        let result = validate_change_note(&note);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_length");
        assert_eq!(issues[0].field, "changeNote");
    }

    #[test]
    fn literal_single_braces_no_issue() {
        let result = validate_prompt("{agent_name}");
        assert!(result.is_ok());
    }

    #[test]
    fn render_determinism() {
        let content = "Hi {{agent_name}} from {{tenant_name}}";
        let mut vars = HashMap::new();
        vars.insert("agent_name", "Aria".to_string());
        vars.insert("tenant_name", "Acme".to_string());
        let a = render_prompt(content, &vars);
        let b = render_prompt(content, &vars);
        assert_eq!(a, b);
    }

    #[test]
    fn render_injection_safety() {
        let content = "Hello {{agent_name}}";
        let mut vars = HashMap::new();
        vars.insert("agent_name", "{{customer_name}}".to_string());
        let result = render_prompt(content, &vars);
        assert_eq!(result, "Hello {{customer_name}}");
    }

    #[test]
    fn render_passthrough_unknown() {
        let content = "Hours: {{business_hours}}";
        let vars = HashMap::new();
        let result = render_prompt(content, &vars);
        assert_eq!(result, content);
    }

    #[test]
    fn starter_prompt_is_valid() {
        assert!(validate_prompt(STARTER_PROMPT).is_ok());
    }

    #[test]
    fn fixture_validation() {
        let fixture = include_str!(
            "../../../../../specs/018-prompt-management/contracts/prompt-validation-fixture.json"
        );
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(fixture).expect("fixture JSON is valid");
        for (i, case) in cases.iter().enumerate() {
            let content = if let Some(repeat) = case.get("contentRepeat") {
                let unit = repeat["unit"].as_str().unwrap();
                let count = repeat["count"].as_u64().unwrap() as usize;
                unit.repeat(count)
            } else {
                case["content"].as_str().unwrap().to_string()
            };
            let expected: Vec<String> = case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["code"].as_str().unwrap().to_string())
                .collect();
            let result = validate_prompt(&content);
            match &result {
                Ok(()) => assert!(
                    expected.is_empty(),
                    "case {}: expected issues but got Ok",
                    i
                ),
                Err(issues) => {
                    let codes: Vec<&str> = issues.iter().map(|i| i.code.as_str()).collect();
                    assert_eq!(
                        codes.len(),
                        expected.len(),
                        "case {}: issue count mismatch",
                        i
                    );
                    for (j, code) in codes.iter().enumerate() {
                        assert_eq!(*code, expected[j], "case {}: issue {} code mismatch", i, j);
                    }
                }
            }
        }
    }
}
