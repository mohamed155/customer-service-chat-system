use uuid::Uuid;

use customers::queries;

/// Normalize a WhatsApp ID to E.164 format: strip all non-digit characters,
/// then prepend `+`.
pub fn normalize_wa_id(wa_id: &str) -> String {
    let digits: String = wa_id.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("+{digits}")
}

/// Resolve a customer from a WhatsApp ID and optional profile name.
///
/// Resolution order (R8):
/// 1. Try `whatsapp` identifier match → found customer
/// 2. Try `phone` identifier match → attach `whatsapp` identifier to that customer
/// 3. Create new customer with `whatsapp` identifier
pub async fn resolve_customer_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    wa_id: &str,
    profile_name: Option<&str>,
) -> sqlx::Result<Uuid> {
    let normalized = normalize_wa_id(wa_id);

    // Step 1: Try whatsapp identifier
    if let Some(customer_id) = queries::find_customer_by_identifier_in_tx(
        tx, tenant_id, "whatsapp", &normalized,
    )
    .await?
    {
        return Ok(customer_id);
    }

    // Step 2: Try phone identifier, then attach whatsapp
    if let Some(customer_id) = queries::find_customer_by_identifier_in_tx(
        tx, tenant_id, "phone", &normalized,
    )
    .await?
    {
        queries::attach_identifier_in_tx(tx, tenant_id, customer_id, "whatsapp", &normalized)
            .await?;
        return Ok(customer_id);
    }

    // Step 3: Create new customer
    let display_name = profile_name.unwrap_or(&normalized).to_string();
    let customer_id = queries::create_customer_with_identifier_in_tx(
        tx,
        tenant_id,
        &display_name,
        "whatsapp",
        &normalized,
    )
    .await?;

    Ok(customer_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_wa_id_strips_non_digits() {
        assert_eq!(normalize_wa_id("1234567890"), "+1234567890");
        assert_eq!(normalize_wa_id("+1 (555) 123-4567"), "+15551234567");
        assert_eq!(normalize_wa_id("whatsapp: +12345"), "+12345");
    }

    #[test]
    fn test_normalize_wa_id_empty() {
        assert_eq!(normalize_wa_id(""), "+");
    }

    #[test]
    fn test_normalize_wa_id_already_e164() {
        assert_eq!(normalize_wa_id("+15551234567"), "+15551234567");
    }
}
