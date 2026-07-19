pub fn origin_allowed(
    allowed_domains: &[String],
    origin_header: Option<&str>,
    referer_header: Option<&str>,
) -> bool {
    if allowed_domains.is_empty() {
        return true;
    }

    let host = origin_header
        .and_then(parse_host_from_origin)
        .or_else(|| referer_header.and_then(parse_host_from_url));

    match host {
        Some(h) => allowed_domains
            .iter()
            .any(|entry| domain_matches(entry, &h)),
        None => false,
    }
}

fn parse_host_from_origin(origin: &str) -> Option<String> {
    let without_scheme = origin.split("://").nth(1)?;
    let host = without_scheme.split(':').next()?;
    Some(host.to_lowercase())
}

fn parse_host_from_url(url: &str) -> Option<String> {
    let without_scheme = url.split("://").nth(1)?;
    let without_path = without_scheme.split('/').next()?;
    let host = without_path.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

fn domain_matches(entry: &str, host: &str) -> bool {
    if let Some(suffix) = entry.strip_prefix("*.") {
        let suffix = suffix.to_lowercase();
        if host == suffix {
            return false;
        }
        host.ends_with(&format!(".{}", suffix))
    } else {
        entry.to_lowercase() == host
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list_allows_all() {
        assert!(origin_allowed(&[], Some("https://evil.com"), None));
    }

    #[test]
    fn exact_host_matches() {
        let domains = vec!["example.com".to_string()];
        assert!(origin_allowed(&domains, Some("https://example.com"), None));
    }

    #[test]
    fn exact_host_case_insensitive() {
        let domains = vec!["Example.COM".to_string()];
        assert!(origin_allowed(&domains, Some("https://example.com"), None));
    }

    #[test]
    fn wildcard_subdomain_matches() {
        let domains = vec!["*.example.org".to_string()];
        assert!(origin_allowed(
            &domains,
            Some("https://a.example.org"),
            None
        ));
        assert!(origin_allowed(
            &domains,
            Some("https://a.b.example.org"),
            None
        ));
    }

    #[test]
    fn wildcard_does_not_match_bare_domain() {
        let domains = vec!["*.example.org".to_string()];
        assert!(!origin_allowed(&domains, Some("https://example.org"), None));
    }

    #[test]
    fn missing_origin_when_list_non_empty_returns_false() {
        let domains = vec!["example.com".to_string()];
        assert!(!origin_allowed(&domains, None, None));
    }

    #[test]
    fn falls_back_to_referer() {
        let domains = vec!["example.com".to_string()];
        assert!(origin_allowed(
            &domains,
            None,
            Some("https://example.com/page")
        ));
    }
}
