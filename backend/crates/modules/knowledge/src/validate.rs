use serde::Serialize;

pub const MAX_TITLE_LENGTH: usize = 200;
pub const MAX_BODY_LENGTH: usize = 100_000;
pub const MAX_TAG_LENGTH: usize = 40;
pub const MAX_TAGS_PER_ITEM: usize = 20;
pub const MAX_DOCUMENT_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    pub field: String,
    pub code: String,
    pub message: String,
}

pub fn validate_title(title: &str) -> Option<ValidationIssue> {
    if title.trim().is_empty() {
        return Some(ValidationIssue {
            field: "title".into(),
            code: "required".into(),
            message: "title is required".into(),
        });
    }
    if title.chars().count() > MAX_TITLE_LENGTH {
        return Some(ValidationIssue {
            field: "title".into(),
            code: "too_long".into(),
            message: format!("title must not exceed {} characters", MAX_TITLE_LENGTH),
        });
    }
    None
}

pub fn sanitize_body(body: &str) -> String {
    ammonia::clean(body)
}

pub fn validate_body(body: &str) -> Option<ValidationIssue> {
    let sanitized = sanitize_body(body);
    if sanitized.chars().count() > MAX_BODY_LENGTH {
        return Some(ValidationIssue {
            field: "body".into(),
            code: "too_long".into(),
            message: format!("body must not exceed {} characters", MAX_BODY_LENGTH),
        });
    }
    None
}

pub fn normalize_tags(raw: &[String]) -> Result<Vec<String>, ValidationIssue> {
    let mut seen = Vec::new();
    for t in raw {
        let trimmed = t.trim().to_lowercase();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().count() > MAX_TAG_LENGTH {
            return Err(ValidationIssue {
                field: "tags".into(),
                code: "too_long".into(),
                message: format!("tag must not exceed {} characters", MAX_TAG_LENGTH),
            });
        }
        if !seen.contains(&trimmed) {
            seen.push(trimmed);
        }
    }
    if seen.len() > MAX_TAGS_PER_ITEM {
        return Err(ValidationIssue {
            field: "tags".into(),
            code: "too_many".into(),
            message: format!("at most {} tags allowed", MAX_TAGS_PER_ITEM),
        });
    }
    Ok(seen)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Article,
    Faq,
    Document,
}

impl ItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Article => "article",
            Self::Faq => "faq",
            Self::Document => "document",
        }
    }
}

impl std::str::FromStr for ItemType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "article" => Ok(Self::Article),
            "faq" => Ok(Self::Faq),
            "document" => Ok(Self::Document),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemStatus {
    Draft,
    Published,
    Archived,
}

impl ItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }
}

impl std::str::FromStr for ItemStatus {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            "archived" => Ok(Self::Archived),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    Illegal { from: ItemStatus, to: ItemStatus },
    BodyRequired,
}

pub fn check_transition(
    from: ItemStatus,
    to: ItemStatus,
    item_type: ItemType,
    body: Option<&str>,
) -> Result<bool, TransitionError> {
    if from == to {
        return Ok(false);
    }

    let allowed = matches!(
        (from, to),
        (ItemStatus::Draft, ItemStatus::Published)
            | (ItemStatus::Published, ItemStatus::Archived)
            | (ItemStatus::Archived, ItemStatus::Draft)
    );

    if !allowed {
        return Err(TransitionError::Illegal { from, to });
    }

    if to == ItemStatus::Published && item_type != ItemType::Document {
        let has_body = match body {
            Some(b) => !b.trim().is_empty(),
            None => false,
        };
        if !has_body {
            return Err(TransitionError::BodyRequired);
        }
    }

    Ok(true)
}

pub fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .filter(|&c| c != '/' && c != '\\' && c as u32 >= 0x20)
        .collect();

    let truncated: String = sanitized.chars().take(255).collect();

    if truncated.is_empty() {
        "download".to_string()
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_title ---

    #[test]
    fn title_required_when_empty() {
        let issue = validate_title("");
        assert!(issue.is_some());
        assert_eq!(issue.as_ref().unwrap().code, "required");
    }

    #[test]
    fn title_required_when_whitespace() {
        let issue = validate_title("   ");
        assert!(issue.is_some());
        assert_eq!(issue.as_ref().unwrap().code, "required");
    }

    #[test]
    fn title_too_long() {
        let long: String = "a".repeat(MAX_TITLE_LENGTH + 1);
        let issue = validate_title(&long);
        assert!(issue.is_some());
        assert_eq!(issue.as_ref().unwrap().code, "too_long");
    }

    #[test]
    fn title_at_limit_ok() {
        let exact: String = "a".repeat(MAX_TITLE_LENGTH);
        assert!(validate_title(&exact).is_none());
    }

    #[test]
    fn title_valid_ok() {
        assert!(validate_title("Valid Title").is_none());
    }

    // --- sanitize_body ---

    #[test]
    fn sanitizer_strips_script_tags() {
        let result = sanitize_body("<script>alert(1)</script>");
        assert!(!result.contains("<script>"));
        assert!(!result.contains("alert"));
    }

    #[test]
    fn sanitizer_strips_event_handler() {
        let result = sanitize_body("<img onerror=\"alert(1)\" src=x>");
        assert!(!result.contains("onerror"));
    }

    #[test]
    fn sanitizer_preserves_headings() {
        let result = sanitize_body("<h2>Heading</h2>");
        assert!(result.contains("<h2>"));
        assert!(result.contains("Heading"));
    }

    #[test]
    fn sanitizer_preserves_lists() {
        let result = sanitize_body("<ul><li>item</li></ul>");
        assert!(result.contains("<ul>"));
        assert!(result.contains("<li>"));
    }

    #[test]
    fn sanitizer_preserves_links() {
        let result = sanitize_body("<a href=\"https://example.com\">link</a>");
        assert!(result.contains("<a href"));
        assert!(result.contains("link"));
    }

    #[test]
    fn sanitizer_preserves_strong() {
        let result = sanitize_body("<strong>bold</strong>");
        assert!(result.contains("<strong>"));
        assert!(result.contains("bold"));
    }

    // --- validate_body ---

    #[test]
    fn body_too_long() {
        let long: String = "x".repeat(MAX_BODY_LENGTH + 1);
        let issue = validate_body(&long);
        assert!(issue.is_some());
        assert_eq!(issue.as_ref().unwrap().code, "too_long");
    }

    #[test]
    fn body_at_limit_ok() {
        let exact: String = "x".repeat(MAX_BODY_LENGTH);
        assert!(validate_body(&exact).is_none());
    }

    #[test]
    fn body_after_sanitization_shorter_still_ok() {
        let input = format!(
            "{}{}",
            "<script>alert(1)</script>",
            "x".repeat(MAX_BODY_LENGTH - 50)
        );
        // After sanitization the body should be shorter (script stripped), so it should pass
        assert!(validate_body(&input).is_none());
    }

    // --- normalize_tags ---

    #[test]
    fn tags_trim_lowercase_dedupe() {
        let raw = vec![
            "  Foo  ".to_string(),
            "foo".to_string(),
            "BAR".to_string(),
            "bar".to_string(),
        ];
        let result = normalize_tags(&raw).unwrap();
        assert_eq!(result, vec!["foo", "bar"]);
    }

    #[test]
    fn tags_drop_empty() {
        let raw = vec!["".to_string(), "  ".to_string(), "valid".to_string()];
        let result = normalize_tags(&raw).unwrap();
        assert_eq!(result, vec!["valid"]);
    }

    #[test]
    fn tags_too_many() {
        let raw: Vec<String> = (0..=MAX_TAGS_PER_ITEM)
            .map(|i| format!("tag{}", i))
            .collect();
        let err = normalize_tags(&raw).unwrap_err();
        assert_eq!(err.code, "too_many");
    }

    #[test]
    fn tags_at_limit_ok() {
        let raw: Vec<String> = (0..MAX_TAGS_PER_ITEM)
            .map(|i| format!("tag{}", i))
            .collect();
        assert!(normalize_tags(&raw).is_ok());
    }

    #[test]
    fn tags_too_long() {
        let long = "a".repeat(MAX_TAG_LENGTH + 1);
        let raw = vec![long];
        let err = normalize_tags(&raw).unwrap_err();
        assert_eq!(err.code, "too_long");
    }

    #[test]
    fn tags_preserves_first_seen_order() {
        let raw = vec![
            "c".to_string(),
            "a".to_string(),
            "b".to_string(),
            "a".to_string(),
        ];
        let result = normalize_tags(&raw).unwrap();
        assert_eq!(result, vec!["c", "a", "b"]);
    }

    // --- ItemType ---

    #[test]
    fn item_type_as_str() {
        assert_eq!(ItemType::Article.as_str(), "article");
        assert_eq!(ItemType::Faq.as_str(), "faq");
        assert_eq!(ItemType::Document.as_str(), "document");
    }

    #[test]
    fn item_type_from_str() {
        assert_eq!("article".parse::<ItemType>().unwrap(), ItemType::Article);
        assert_eq!("faq".parse::<ItemType>().unwrap(), ItemType::Faq);
        assert_eq!("document".parse::<ItemType>().unwrap(), ItemType::Document);
        assert!("invalid".parse::<ItemType>().is_err());
    }

    // --- ItemStatus ---

    #[test]
    fn item_status_as_str() {
        assert_eq!(ItemStatus::Draft.as_str(), "draft");
        assert_eq!(ItemStatus::Published.as_str(), "published");
        assert_eq!(ItemStatus::Archived.as_str(), "archived");
    }

    #[test]
    fn item_status_from_str() {
        assert_eq!("draft".parse::<ItemStatus>().unwrap(), ItemStatus::Draft);
        assert_eq!(
            "published".parse::<ItemStatus>().unwrap(),
            ItemStatus::Published
        );
        assert_eq!(
            "archived".parse::<ItemStatus>().unwrap(),
            ItemStatus::Archived
        );
        assert!("invalid".parse::<ItemStatus>().is_err());
    }

    // --- check_transition (table-driven) ---

    #[test]
    fn transition_table_article() {
        let pairs = [
            (ItemStatus::Draft, ItemStatus::Draft, Ok(false)),
            (ItemStatus::Draft, ItemStatus::Published, Ok(true)),
            (ItemStatus::Draft, ItemStatus::Archived, Err(())),
            (ItemStatus::Published, ItemStatus::Draft, Err(())),
            (ItemStatus::Published, ItemStatus::Published, Ok(false)),
            (ItemStatus::Published, ItemStatus::Archived, Ok(true)),
            (ItemStatus::Archived, ItemStatus::Draft, Ok(true)),
            (ItemStatus::Archived, ItemStatus::Published, Err(())),
            (ItemStatus::Archived, ItemStatus::Archived, Ok(false)),
        ];

        for (from, to, expected) in &pairs {
            let body = if *to == ItemStatus::Published && *from != *to {
                Some("some body")
            } else {
                None
            };
            let result = check_transition(*from, *to, ItemType::Article, body);
            match expected {
                Ok(noop) => assert_eq!(result, Ok(*noop), "article {:?} -> {:?}", from, to),
                Err(_) => assert!(
                    result.is_err(),
                    "article {:?} -> {:?} should be illegal",
                    from,
                    to
                ),
            }
        }
    }

    #[test]
    fn transition_table_document() {
        let pairs = [
            (ItemStatus::Draft, ItemStatus::Draft, Ok(false)),
            (ItemStatus::Draft, ItemStatus::Published, Ok(true)),
            (ItemStatus::Draft, ItemStatus::Archived, Err(())),
            (ItemStatus::Published, ItemStatus::Draft, Err(())),
            (ItemStatus::Published, ItemStatus::Published, Ok(false)),
            (ItemStatus::Published, ItemStatus::Archived, Ok(true)),
            (ItemStatus::Archived, ItemStatus::Draft, Ok(true)),
            (ItemStatus::Archived, ItemStatus::Published, Err(())),
            (ItemStatus::Archived, ItemStatus::Archived, Ok(false)),
        ];

        for (from, to, expected) in &pairs {
            let result = check_transition(*from, *to, ItemType::Document, None);
            match expected {
                Ok(noop) => assert_eq!(result, Ok(*noop), "document {:?} -> {:?}", from, to),
                Err(_) => assert!(
                    result.is_err(),
                    "document {:?} -> {:?} should be illegal",
                    from,
                    to
                ),
            }
        }
    }

    #[test]
    fn transition_article_publish_requires_body() {
        let result = check_transition(
            ItemStatus::Draft,
            ItemStatus::Published,
            ItemType::Article,
            None,
        );
        assert_eq!(result, Err(TransitionError::BodyRequired));
    }

    #[test]
    fn transition_article_publish_whitespace_body_fails() {
        let result = check_transition(
            ItemStatus::Draft,
            ItemStatus::Published,
            ItemType::Article,
            Some("   "),
        );
        assert_eq!(result, Err(TransitionError::BodyRequired));
    }

    #[test]
    fn transition_document_publish_no_body_ok() {
        let result = check_transition(
            ItemStatus::Draft,
            ItemStatus::Published,
            ItemType::Document,
            None,
        );
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn transition_same_status_returns_false() {
        let result = check_transition(
            ItemStatus::Draft,
            ItemStatus::Draft,
            ItemType::Article,
            None,
        );
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn transition_illegal_error_contains_from_to() {
        let err = check_transition(
            ItemStatus::Published,
            ItemStatus::Draft,
            ItemType::Article,
            None,
        )
        .unwrap_err();
        match err {
            TransitionError::Illegal { from, to } => {
                assert_eq!(from, ItemStatus::Published);
                assert_eq!(to, ItemStatus::Draft);
            }
            _ => panic!("expected Illegal"),
        }
    }

    // --- sanitize_filename ---

    #[test]
    fn filename_strips_path_separators() {
        let result = sanitize_filename("foo/bar\\baz.txt");
        assert_eq!(result, "foobarbaz.txt");
    }

    #[test]
    fn filename_strips_control_chars() {
        let result = sanitize_filename("test\x00file\x1f.txt");
        assert_eq!(result, "testfile.txt");
    }

    #[test]
    fn filename_caps_at_255_chars() {
        let long = "a".repeat(300);
        let result = sanitize_filename(&long);
        assert_eq!(result.chars().count(), 255);
    }

    #[test]
    fn filename_fallback_to_download() {
        let result = sanitize_filename("\x00\x01\x02");
        assert_eq!(result, "download");
    }

    #[test]
    fn filename_preserves_normal_name() {
        let result = sanitize_filename("report.pdf");
        assert_eq!(result, "report.pdf");
    }

    #[test]
    fn filename_allows_spaces_and_hyphens() {
        let result = sanitize_filename("my document (v2).pdf");
        assert_eq!(result, "my document (v2).pdf");
    }
}
