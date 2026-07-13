use kernel::{ApiError, ErrorDetail};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::marker::PhantomData;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Customer {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelIdentifier {
    pub id: Uuid,
    pub channel: String,
    pub identifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerListItem {
    pub id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub channels: Vec<String>,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerDetail {
    pub id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub channels: Vec<String>,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub identifiers: Vec<ChannelIdentifier>,
    pub metadata: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Create / Update payloads + validation
// ---------------------------------------------------------------------------

/// Channel vocabulary for `customer_channel_identifiers` rows; mirrors the DB
/// CHECK constraint added in migration 0025 and the dashboard's
/// `channel-badge` fixture.
pub const CHANNELS: &[&str] = &["email", "phone", "web_chat", "whatsapp", "telegram"];

pub const DISPLAY_NAME_MAX: usize = 200;
pub const EMAIL_MAX: usize = 320;
pub const PHONE_MIN_DIGITS: usize = 7;
pub const PHONE_MAX_DIGITS: usize = 15;
pub const IDENTIFIER_MAX: usize = 320;
pub const METADATA_MAX_KEYS: usize = 50;
pub const METADATA_KEY_MAX: usize = 100;
pub const METADATA_VALUE_MAX: usize = 500;

/// Raw channel-identifier entry as supplied on the wire.  The DB normalizes
/// the value (trim + lowercase) but format checks happen before the round
/// trip so the UI gets a clean `422 details[]` per channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelIdentifierInput {
    pub channel: String,
    pub identifier: String,
}

/// Body of `POST /tenant/customers`.  Mirrors the contract in
/// `specs/012-customer-profiles/contracts/rest-api.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CreateCustomerPayload {
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub identifiers: Vec<ChannelIdentifierInput>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// A three-state field that distinguishes an absent field, an explicit JSON
/// `null` (clear), and a present value.  Uses a custom `Deserialize`
/// implementation so serde's default `Option<T>` flattening of `null` →
/// `None` is avoided at the TriState level.
///
/// # States
///
/// | JSON | Rust |
/// |------|------|
/// | field absent (via `#[serde(default)]`) | `TriState::Absent` |
/// | `null` | `TriState::Clear` |
/// | `"value"` | `TriState::Value("value")` |
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriState<T> {
    Absent,
    Clear,
    Value(T),
}

impl<T> TriState<T> {
    pub fn is_absent(&self) -> bool {
        matches!(self, TriState::Absent)
    }
}

impl<T: Serialize> Serialize for TriState<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            TriState::Absent => serializer.serialize_unit(),
            TriState::Clear => serializer.serialize_none(),
            TriState::Value(v) => v.serialize(serializer),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for TriState<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for V<T> {
            type Value = TriState<T>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a value or null")
            }

            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(TriState::Clear)
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(TriState::Clear)
            }

            fn visit_some<D: serde::de::Deserializer<'de>>(
                self,
                de: D,
            ) -> Result<Self::Value, D::Error> {
                T::deserialize(de).map(TriState::Value)
            }
        }
        deserializer.deserialize_option(V::<T>(PhantomData))
    }
}

impl<T> Default for TriState<T> {
    fn default() -> Self {
        TriState::Absent
    }
}

/// Body of `PATCH /tenant/customers/{id}`.  `TriState` for nullable contact
/// fields distinguishes "absent" (no change) from explicit `null` (clear)
/// from a present value.  Identifiers and metadata use replace-the-set
/// semantics — they are present and become the new set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UpdateCustomerPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "TriState::is_absent")]
    pub email: TriState<String>,
    #[serde(default, skip_serializing_if = "TriState::is_absent")]
    pub phone: TriState<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifiers: Option<Vec<ChannelIdentifierInput>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

/// `validate_create` — run every per-field rule in `data-model.md` /
/// `contracts/rest-api.md` and return a `422 unprocessable_entity` carrying
/// field-level `details[]` if any rule fails.  Otherwise `Ok(())` and the
/// handler proceeds to the DB write inside its transaction.
///
/// Create-only extra rule: at least one of `email`, `phone`, or a non-empty
/// `identifiers` entry must be present (FR-007).  The integration test
/// `create_validates_input_returns_422_with_field_details_and_persists_no_partial_row`
/// asserts the wire-level codes / fields, and the unit tests in this module
/// pin the per-rule behaviour.
pub fn validate_create(payload: &CreateCustomerPayload) -> Result<(), ApiError> {
    let mut details: Vec<ErrorDetail> = Vec::new();

    validate_display_name(Some(&payload.display_name), &mut details);
    validate_email_field(payload.email.as_deref(), "email", &mut details);
    validate_phone_field(payload.phone.as_deref(), "phone", &mut details);
    validate_identifier_set(&payload.identifiers, &mut details);
    validate_metadata(&payload.metadata, "metadata", &mut details);

    if !has_contact_or_identifier(
        payload.email.as_deref(),
        payload.phone.as_deref(),
        &payload.identifiers,
    ) {
        details.push(ErrorDetail {
            field: "<root>".into(),
            code: "missing_contact_or_identifier".into(),
            message: "At least one of email, phone, or a channel identifier is required".into(),
        });
    }

    if details.is_empty() {
        Ok(())
    } else {
        Err(ApiError::unprocessable_entity("Validation failed").with_details(details))
    }
}

/// `validate_update` — same per-field rules as create, but:
///   * `display_name` and the identifier/metadata sets are treated as
///     "present and replace" — if the outer `Option` is `Some`, validate the
///     payload (the empty-identifier set is a valid no-op clear).
///   * The contact fields are triple-state: omitted, present with a value,
///     or present with `None` (explicit clear).  Both present variants must
///     pass format / length validation; the `None` variant is always valid.
///   * No "at least one contact" rule on update — the row already has a
///     contact, and clearing one field while leaving others is allowed.
pub fn validate_update(payload: &UpdateCustomerPayload) -> Result<(), ApiError> {
    let mut details: Vec<ErrorDetail> = Vec::new();

    if let Some(display_name) = payload.display_name.as_deref() {
        validate_display_name(Some(display_name), &mut details);
    }
    if let TriState::Value(v) = &payload.email {
        validate_email_field(Some(v.as_str()), "email", &mut details);
    }
    if let TriState::Value(v) = &payload.phone {
        validate_phone_field(Some(v.as_str()), "phone", &mut details);
    }
    if let Some(identifiers) = payload.identifiers.as_deref() {
        validate_identifier_set(identifiers, &mut details);
    }
    if let Some(metadata) = payload.metadata.as_ref() {
        validate_metadata(metadata, "metadata", &mut details);
    }

    if details.is_empty() {
        Ok(())
    } else {
        Err(ApiError::unprocessable_entity("Validation failed").with_details(details))
    }
}

// ---------------------------------------------------------------------------
// Rule helpers
// ---------------------------------------------------------------------------

fn validate_display_name(value: Option<&str>, details: &mut Vec<ErrorDetail>) {
    let Some(name) = value else {
        details.push(ErrorDetail {
            field: "display_name".into(),
            code: "required".into(),
            message: "Display name is required".into(),
        });
        return;
    };
    let trimmed = name.trim();
    if trimmed.is_empty() {
        details.push(ErrorDetail {
            field: "display_name".into(),
            code: "required".into(),
            message: "Display name is required".into(),
        });
        return;
    }
    if trimmed.chars().count() > DISPLAY_NAME_MAX {
        details.push(ErrorDetail {
            field: "display_name".into(),
            code: "too_long".into(),
            message: format!("Display name must be at most {DISPLAY_NAME_MAX} characters"),
        });
    }
}

fn validate_email_field(value: Option<&str>, field: &str, details: &mut Vec<ErrorDetail>) {
    let Some(raw) = value else {
        return;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        details.push(ErrorDetail {
            field: field.into(),
            code: "invalid_format".into(),
            message: format!("{field} format is invalid"),
        });
        return;
    }
    if trimmed.chars().count() > EMAIL_MAX {
        details.push(ErrorDetail {
            field: field.into(),
            code: "too_long".into(),
            message: format!("{field} must be at most {EMAIL_MAX} characters"),
        });
        return;
    }
    if !is_valid_email(trimmed) {
        details.push(ErrorDetail {
            field: field.into(),
            code: "invalid_format".into(),
            message: format!("{field} format is invalid"),
        });
    }
}

fn validate_phone_field(value: Option<&str>, field: &str, details: &mut Vec<ErrorDetail>) {
    let Some(raw) = value else {
        return;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        details.push(ErrorDetail {
            field: field.into(),
            code: "invalid_format".into(),
            message: format!("{field} format is invalid"),
        });
        return;
    }
    if !is_valid_phone(trimmed) {
        details.push(ErrorDetail {
            field: field.into(),
            code: "invalid_format".into(),
            message: format!("{field} format is invalid"),
        });
    }
}

fn validate_identifier_set(identifiers: &[ChannelIdentifierInput], details: &mut Vec<ErrorDetail>) {
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for (index, entry) in identifiers.iter().enumerate() {
        validate_channel_value(entry, index, details);

        let channel = canonicalize_channel(&entry.channel);
        let value = entry.identifier.trim();
        let normalized = if channel == "email" {
            value.to_lowercase()
        } else {
            value.to_owned()
        };
        if !seen.insert((channel, normalized)) {
            details.push(ErrorDetail {
                field: format!("identifiers[{index}]"),
                code: "duplicate".into(),
                message: "Duplicate channel identifier in the same payload".into(),
            });
        }
    }
}

fn validate_channel_value(
    entry: &ChannelIdentifierInput,
    index: usize,
    details: &mut Vec<ErrorDetail>,
) {
    let field = format!("identifiers[{index}]");
    let channel = canonicalize_channel(&entry.channel);
    if !CHANNELS.contains(&channel.as_str()) {
        details.push(ErrorDetail {
            field,
            code: "invalid_value".into(),
            message: format!("Channel must be one of: {}", CHANNELS.join(", ")),
        });
        return;
    }

    let value = entry.identifier.trim();
    if value.is_empty() {
        details.push(ErrorDetail {
            field,
            code: "required".into(),
            message: "Identifier value is required".into(),
        });
        return;
    }
    if value.chars().count() > IDENTIFIER_MAX {
        details.push(ErrorDetail {
            field,
            code: "too_long".into(),
            message: format!("Identifier must be at most {IDENTIFIER_MAX} characters"),
        });
        return;
    }

    let ok = match channel.as_str() {
        "email" => is_valid_email(value),
        "phone" | "whatsapp" => is_valid_phone(value),
        // web_chat and telegram allow opaque handles; length is the only
        // format constraint (already enforced above).
        _ => true,
    };
    if !ok {
        details.push(ErrorDetail {
            field,
            code: "invalid_format".into(),
            message: format!("Identifier format is invalid for channel `{channel}`"),
        });
    }
}

fn validate_metadata(
    metadata: &BTreeMap<String, String>,
    field: &str,
    details: &mut Vec<ErrorDetail>,
) {
    if metadata.len() > METADATA_MAX_KEYS {
        details.push(ErrorDetail {
            field: field.into(),
            code: "too_many_keys".into(),
            message: format!("Metadata may contain at most {METADATA_MAX_KEYS} keys"),
        });
        return;
    }
    for (key, value) in metadata {
        let trimmed_key = key.trim();
        if trimmed_key.is_empty() {
            details.push(ErrorDetail {
                field: format!("{field}[<key>]"),
                code: "required".into(),
                message: "Metadata keys must be non-empty".into(),
            });
            continue;
        }
        if trimmed_key.chars().count() > METADATA_KEY_MAX {
            details.push(ErrorDetail {
                field: format!("{field}[{trimmed_key}]"),
                code: "too_long".into(),
                message: format!("Metadata key must be at most {METADATA_KEY_MAX} characters"),
            });
        }
        if value.chars().count() > METADATA_VALUE_MAX {
            details.push(ErrorDetail {
                field: format!("{field}[{trimmed_key}]"),
                code: "too_long".into(),
                message: format!("Metadata value must be at most {METADATA_VALUE_MAX} characters"),
            });
        }
    }
}

fn has_contact_or_identifier(
    email: Option<&str>,
    phone: Option<&str>,
    identifiers: &[ChannelIdentifierInput],
) -> bool {
    email.map(str::trim).filter(|s| !s.is_empty()).is_some()
        || phone.map(str::trim).filter(|s| !s.is_empty()).is_some()
        || identifiers
            .iter()
            .any(|entry| !entry.identifier.trim().is_empty())
}

/// Canonicalize a channel name: trim whitespace and lowercase.
/// This ensures channel comparisons like `channel == "email"` work correctly
/// regardless of how the caller supplied the value.
pub fn canonicalize_channel(channel: &str) -> String {
    channel.trim().to_lowercase()
}

// ---------------------------------------------------------------------------
// Format helpers
// ---------------------------------------------------------------------------

/// `is_valid_email` — RFC 5321-shape validation: single `@`, no whitespace,
/// non-empty local part (≤64 chars), non-empty domain with at least one
/// dot, each domain label ≤63 chars and matching `[A-Za-z0-9-]` with no
/// leading/trailing `-`, and a ≥2-character alphabetic TLD.  Total length is
/// capped at the customer-profile contract's 320-char window (data-model.md
/// / R8) — looser than the RFC 5321 hard 254-byte limit, but the spec is
/// the source of truth for this module.
fn is_valid_email(value: &str) -> bool {
    if value.is_empty() || value.chars().count() > EMAIL_MAX {
        return false;
    }
    if value.bytes().any(|byte| byte < 32 || byte == 127) {
        return false;
    }
    if value.starts_with('@')
        || value.ends_with('@')
        || !value.contains('@')
        || value.contains("@@")
    {
        return false;
    }

    let (local, domain) = match value.split_once('@') {
        Some(parts) => parts,
        None => return false,
    };

    if local.is_empty() || local.len() > 64 {
        return false;
    }
    if local.contains(' ') || local.contains("..") || local.starts_with('.') || local.ends_with('.')
    {
        return false;
    }
    let first = local.chars().next().expect("non-empty");
    let last = local.chars().last().expect("non-empty");
    if !(first.is_alphanumeric() || first == '"') || !(last.is_alphanumeric() || last == '"') {
        return false;
    }

    if domain.is_empty()
        || domain.contains(' ')
        || domain.contains("..")
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return false;
    }

    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    for (index, label) in labels.iter().enumerate() {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
        if index == labels.len() - 1
            && (label.len() < 2 || !label.chars().all(|c| c.is_ascii_alphabetic()))
        {
            return false;
        }
    }
    true
}

/// Strip all non-digit characters from a phone-like value, retaining only
/// an optional leading `+` prefix followed by digits.  Used by the handler
/// to normalize before persistence (E.164 with `+`).
pub fn normalize_phone_digits(value: &str) -> String {
    let has_plus = value.starts_with('+');
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if has_plus {
        format!("+{digits}")
    } else {
        digits
    }
}

/// `is_valid_phone` — optional leading `+` followed by 7–15 digits (the
/// E.164 length window).  Non-digit formatting characters (spaces, dashes,
/// parentheses) are stripped before counting, so `+1 (555) 000-1111` is
/// accepted (13 digits).  This is the platform's "looks like a phone"
/// rule; downstream normalization (storage) is the handler's job.
fn is_valid_phone(value: &str) -> bool {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    let len = digits.len();
    (PHONE_MIN_DIGITS..=PHONE_MAX_DIGITS).contains(&len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details_for<'a>(envelope: &'a ApiError, field: &str) -> Vec<&'a serde_json::Value> {
        envelope
            .details()
            .iter()
            .filter(|detail| detail["field"].as_str() == Some(field))
            .collect()
    }

    fn detail_for<'a>(envelope: &'a ApiError, field: &str) -> Option<&'a serde_json::Value> {
        details_for(envelope, field).into_iter().next()
    }

    fn valid_create() -> CreateCustomerPayload {
        CreateCustomerPayload {
            display_name: "Sara Ali".into(),
            email: Some("sara@example.com".into()),
            phone: Some("+201001234567".into()),
            identifiers: vec![ChannelIdentifierInput {
                channel: "whatsapp".into(),
                identifier: "+201001234567".into(),
            }],
            metadata: BTreeMap::new(),
        }
    }

    // ----- happy path -----

    #[test]
    fn valid_create_payload_passes() {
        let result = validate_create(&valid_create());
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn create_with_only_email_passes() {
        let mut payload = valid_create();
        payload.phone = None;
        payload.identifiers.clear();
        assert!(validate_create(&payload).is_ok());
    }

    #[test]
    fn create_with_only_phone_passes() {
        let mut payload = valid_create();
        payload.email = None;
        payload.identifiers.clear();
        assert!(validate_create(&payload).is_ok());
    }

    #[test]
    fn create_with_only_identifier_passes() {
        let mut payload = valid_create();
        payload.email = None;
        payload.phone = None;
        assert!(validate_create(&payload).is_ok());
    }

    // ----- display_name -----

    #[test]
    fn create_rejects_empty_display_name() {
        let mut payload = valid_create();
        payload.display_name = String::new();
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "display_name").expect("display_name detail");
        assert_eq!(detail["code"].as_str().unwrap(), "required");
    }

    #[test]
    fn create_rejects_whitespace_only_display_name() {
        let mut payload = valid_create();
        payload.display_name = "   ".into();
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "display_name").expect("display_name detail");
        assert_eq!(detail["code"].as_str().unwrap(), "required");
    }

    #[test]
    fn create_accepts_display_name_at_max_length() {
        let mut payload = valid_create();
        payload.display_name = "a".repeat(DISPLAY_NAME_MAX);
        assert!(validate_create(&payload).is_ok());
    }

    #[test]
    fn create_rejects_display_name_one_over_max() {
        let mut payload = valid_create();
        payload.display_name = "a".repeat(DISPLAY_NAME_MAX + 1);
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "display_name").expect("display_name detail");
        assert_eq!(detail["code"].as_str().unwrap(), "too_long");
    }

    // ----- email -----

    #[test]
    fn create_rejects_invalid_email_format() {
        let mut payload = valid_create();
        payload.email = Some("not-an-email".into());
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "email").expect("email detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_format");
    }

    #[test]
    fn create_rejects_email_longer_than_320_chars() {
        let mut payload = valid_create();
        let local = "a".repeat(310);
        payload.email = Some(format!("{local}@example.com"));
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "email").expect("email detail");
        assert_eq!(detail["code"].as_str().unwrap(), "too_long");
    }

    #[test]
    fn create_accepts_email_at_320_chars() {
        let mut payload = valid_create();
        // 64-char local (max) + "@" + 4 * 62-char labels + "." + "com" (3) = 320.
        let local = "a".repeat(64);
        let domain = format!(
            "{}.{}.{}.{}.com",
            "b".repeat(62),
            "c".repeat(62),
            "d".repeat(62),
            "e".repeat(62)
        );
        let email = format!("{local}@{domain}");
        assert_eq!(email.chars().count(), EMAIL_MAX);
        payload.email = Some(email);
        assert!(validate_create(&payload).is_ok());
    }

    // ----- phone -----

    #[test]
    fn create_rejects_non_digit_phone() {
        let mut payload = valid_create();
        payload.phone = Some("abc-not-a-phone".into());
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "phone").expect("phone detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_format");
    }

    #[test]
    fn create_rejects_phone_below_min_digits() {
        let mut payload = valid_create();
        payload.phone = Some("+123456".into());
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "phone").expect("phone detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_format");
    }

    #[test]
    fn create_rejects_phone_above_max_digits() {
        let mut payload = valid_create();
        payload.phone = Some("+1234567890123456".into());
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "phone").expect("phone detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_format");
    }

    #[test]
    fn create_accepts_phone_at_min_and_max_digits() {
        let mut payload = valid_create();
        payload.phone = Some("+1234567".into());
        assert!(validate_create(&payload).is_ok());
        payload.phone = Some("+123456789012345".into());
        assert!(validate_create(&payload).is_ok());
    }

    // ----- identifier set -----

    #[test]
    fn create_rejects_unknown_channel() {
        let mut payload = valid_create();
        payload.identifiers = vec![ChannelIdentifierInput {
            channel: "sms".into(),
            identifier: "+1234567".into(),
        }];
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "identifiers[0]").expect("identifiers[0] detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_value");
    }

    #[test]
    fn create_rejects_empty_identifier_value() {
        let mut payload = valid_create();
        payload.identifiers = vec![ChannelIdentifierInput {
            channel: "telegram".into(),
            identifier: "".into(),
        }];
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "identifiers[0]").expect("identifiers[0] detail");
        assert_eq!(detail["code"].as_str().unwrap(), "required");
    }

    #[test]
    fn create_rejects_identifier_over_320_chars() {
        let mut payload = valid_create();
        payload.identifiers = vec![ChannelIdentifierInput {
            channel: "telegram".into(),
            identifier: "x".repeat(IDENTIFIER_MAX + 1),
        }];
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "identifiers[0]").expect("identifiers[0] detail");
        assert_eq!(detail["code"].as_str().unwrap(), "too_long");
    }

    #[test]
    fn create_rejects_invalid_email_channel_identifier() {
        let mut payload = valid_create();
        payload.identifiers = vec![ChannelIdentifierInput {
            channel: "email".into(),
            identifier: "not-an-email".into(),
        }];
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "identifiers[0]").expect("identifiers[0] detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_format");
    }

    #[test]
    fn create_rejects_invalid_phone_channel_identifier() {
        let mut payload = valid_create();
        payload.identifiers = vec![ChannelIdentifierInput {
            channel: "phone".into(),
            identifier: "not-a-phone".into(),
        }];
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "identifiers[0]").expect("identifiers[0] detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_format");
    }

    #[test]
    fn create_accepts_opaque_web_chat_handle() {
        let mut payload = valid_create();
        payload.identifiers = vec![ChannelIdentifierInput {
            channel: "web_chat".into(),
            identifier: "opaque-handle-123".into(),
        }];
        assert!(validate_create(&payload).is_ok());
    }

    #[test]
    fn create_collects_per_index_identifier_errors() {
        let mut payload = valid_create();
        payload.identifiers = vec![
            ChannelIdentifierInput {
                channel: "email".into(),
                identifier: "good@example.com".into(),
            },
            ChannelIdentifierInput {
                channel: "sms".into(),
                identifier: "+1234567".into(),
            },
        ];
        let error = validate_create(&payload).expect_err("must fail");
        assert!(detail_for(&error, "identifiers[1]").is_some());
        assert!(detail_for(&error, "identifiers[0]").is_none());
    }

    // ----- metadata -----

    #[test]
    fn create_rejects_51_metadata_keys() {
        let mut payload = valid_create();
        let mut metadata = BTreeMap::new();
        for index in 0..(METADATA_MAX_KEYS + 1) {
            metadata.insert(format!("k{index:02}"), "v".into());
        }
        payload.metadata = metadata;
        let error = validate_create(&payload).expect_err("must fail");
        let detail = detail_for(&error, "metadata").expect("metadata detail");
        assert_eq!(detail["code"].as_str().unwrap(), "too_many_keys");
    }

    #[test]
    fn create_accepts_50_metadata_keys() {
        let mut payload = valid_create();
        let mut metadata = BTreeMap::new();
        for index in 0..METADATA_MAX_KEYS {
            metadata.insert(format!("k{index:02}"), "v".into());
        }
        payload.metadata = metadata;
        assert!(validate_create(&payload).is_ok());
    }

    #[test]
    fn create_rejects_metadata_key_over_100_chars() {
        let mut payload = valid_create();
        let mut metadata = BTreeMap::new();
        metadata.insert("k".repeat(METADATA_KEY_MAX + 1), "v".into());
        payload.metadata = metadata;
        let error = validate_create(&payload).expect_err("must fail");
        assert!(
            error
                .details()
                .iter()
                .any(|d| d["code"].as_str() == Some("too_long")
                    && d["field"]
                        .as_str()
                        .map_or(false, |f| f.starts_with("metadata["))),
            "expected too_long on the offending key, got {:?}",
            error.details()
        );
    }

    #[test]
    fn create_rejects_metadata_value_over_500_chars() {
        let mut payload = valid_create();
        let mut metadata = BTreeMap::new();
        metadata.insert("k".into(), "v".repeat(METADATA_VALUE_MAX + 1));
        payload.metadata = metadata;
        let error = validate_create(&payload).expect_err("must fail");
        assert!(
            error
                .details()
                .iter()
                .any(|d| d["code"].as_str() == Some("too_long")
                    && d["field"]
                        .as_str()
                        .map_or(false, |f| f.starts_with("metadata["))),
            "expected too_long on the offending value, got {:?}",
            error.details()
        );
    }

    // ----- "at least one contact" rule -----

    #[test]
    fn create_rejects_payload_with_no_contact_or_identifier() {
        let mut payload = valid_create();
        payload.email = None;
        payload.phone = None;
        payload.identifiers.clear();
        let error = validate_create(&payload).expect_err("must fail");
        assert!(
            error
                .details()
                .iter()
                .any(|d| d["code"].as_str() == Some("missing_contact_or_identifier")),
            "expected missing_contact_or_identifier, got {details:?}",
            details = error.details()
        );
    }

    #[test]
    fn create_collects_multiple_errors_into_one_envelope() {
        let mut payload = valid_create();
        payload.display_name = String::new();
        payload.email = Some("bad".into());
        payload.phone = Some("bad".into());
        payload.identifiers = vec![ChannelIdentifierInput {
            channel: "sms".into(),
            identifier: "x".into(),
        }];
        let error = validate_create(&payload).expect_err("must fail");
        let details = error.details();
        assert!(detail_for(&error, "display_name").is_some());
        assert!(detail_for(&error, "email").is_some());
        assert!(detail_for(&error, "phone").is_some());
        assert!(detail_for(&error, "identifiers[0]").is_some());
        assert!(details.len() >= 4, "got {details:?}");
    }

    // ----- update -----

    #[test]
    fn update_with_only_display_name_passes() {
        let payload = UpdateCustomerPayload {
            display_name: Some("New Name".into()),
            ..Default::default()
        };
        assert!(validate_update(&payload).is_ok());
    }

    #[test]
    fn update_explicit_null_clears_email() {
        let payload = UpdateCustomerPayload {
            email: TriState::Clear,
            ..Default::default()
        };
        assert!(validate_update(&payload).is_ok());
    }

    #[test]
    fn update_rejects_invalid_email_when_present() {
        let payload = UpdateCustomerPayload {
            email: TriState::Value("not-an-email".into()),
            ..Default::default()
        };
        let error = validate_update(&payload).expect_err("must fail");
        let detail = detail_for(&error, "email").expect("email detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_format");
    }

    #[test]
    fn update_rejects_invalid_phone_when_present() {
        let payload = UpdateCustomerPayload {
            phone: TriState::Value("not-a-phone".into()),
            ..Default::default()
        };
        let error = validate_update(&payload).expect_err("must fail");
        let detail = detail_for(&error, "phone").expect("phone detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_format");
    }

    #[test]
    fn update_rejects_too_many_metadata_keys() {
        let mut metadata = BTreeMap::new();
        for index in 0..(METADATA_MAX_KEYS + 1) {
            metadata.insert(format!("k{index:02}"), "v".into());
        }
        let payload = UpdateCustomerPayload {
            metadata: Some(metadata),
            ..Default::default()
        };
        let error = validate_update(&payload).expect_err("must fail");
        let detail = detail_for(&error, "metadata").expect("metadata detail");
        assert_eq!(detail["code"].as_str().unwrap(), "too_many_keys");
    }

    #[test]
    fn update_rejects_invalid_identifier() {
        let payload = UpdateCustomerPayload {
            identifiers: Some(vec![ChannelIdentifierInput {
                channel: "sms".into(),
                identifier: "+1234567".into(),
            }]),
            ..Default::default()
        };
        let error = validate_update(&payload).expect_err("must fail");
        let detail = detail_for(&error, "identifiers[0]").expect("identifiers[0] detail");
        assert_eq!(detail["code"].as_str().unwrap(), "invalid_value");
    }

    #[test]
    fn update_empty_payload_passes() {
        let payload = UpdateCustomerPayload::default();
        assert!(validate_update(&payload).is_ok());
    }

    #[test]
    fn update_rejects_display_name_too_long() {
        let payload = UpdateCustomerPayload {
            display_name: Some("a".repeat(DISPLAY_NAME_MAX + 1)),
            ..Default::default()
        };
        let error = validate_update(&payload).expect_err("must fail");
        let detail = detail_for(&error, "display_name").expect("display_name detail");
        assert_eq!(detail["code"].as_str().unwrap(), "too_long");
    }

    // ----- TriState serde -----

    #[test]
    fn tristate_serde_distinguishes_null_and_absent() {
        let p: UpdateCustomerPayload = serde_json::from_str(r#"{"email": null}"#).unwrap();
        assert_eq!(p.email, TriState::Clear, "null email must deserialize as Clear");
        let p: UpdateCustomerPayload = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(p.email, TriState::Absent, "absent email must deserialize as Absent");
        let p: UpdateCustomerPayload =
            serde_json::from_str(r#"{"email": "test@example.com"}"#).unwrap();
        assert_eq!(
            p.email,
            TriState::Value("test@example.com".into()),
            "present email must deserialize as TriState::Value"
        );
    }

    // ----- is_valid_email / is_valid_phone unit cases -----

    #[test]
    fn is_valid_email_accepts_canonical_forms() {
        for ok in [
            "user@example.com",
            "a.b@c.co",
            "user@sub.example.com",
            "user.name@example.com",
            "user+tag@example.com",
        ] {
            assert!(is_valid_email(ok), "expected {ok} to be valid");
        }
    }

    #[test]
    fn is_valid_email_rejects_malformed() {
        for bad in [
            "",
            "not-an-email",
            "user@",
            "@example.com",
            "user@@example.com",
            "us..er@example.com",
            "user@exa mple.com",
        ] {
            assert!(!is_valid_email(bad), "expected {bad:?} to be invalid");
        }
    }

    #[test]
    fn is_valid_phone_accepts_e164_window() {
        for ok in ["+1234567", "1234567", "+123456789012345"] {
            assert!(is_valid_phone(ok), "expected {ok} to be valid");
        }
    }

    #[test]
    fn is_valid_phone_rejects_outside_window_and_non_digits() {
        for bad in ["+123456", "abc", "+", "1234567890123456", ""] {
            assert!(!is_valid_phone(bad), "expected {bad:?} to be invalid");
        }
    }

}
