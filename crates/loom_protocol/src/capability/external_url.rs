//! Shared fail-closed validation for extension-requested external navigation.

const MAX_EXTERNAL_URL_BYTES: usize = 8 * 1024;

/// Accepts only bounded HTTPS URLs with a non-empty authority and no credentials.
#[must_use]
pub fn is_safe_external_https_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_EXTERNAL_URL_BYTES
        || value.chars().any(|character| {
            character.is_control() || character.is_whitespace() || character == '\\'
        })
    {
        return false;
    }
    let Some(authority_and_path) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = authority_and_path
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    !authority.is_empty() && !authority.contains('@')
}

#[cfg(test)]
mod tests {
    use super::is_safe_external_https_url;

    #[test]
    fn external_navigation_accepts_only_plain_https_urls() {
        assert!(is_safe_external_https_url("https://example.com/path?q=1"));
        assert!(!is_safe_external_https_url("http://example.com"));
        assert!(!is_safe_external_https_url("https://user@example.com"));
        assert!(!is_safe_external_https_url("https:///missing-host"));
        assert!(!is_safe_external_https_url(
            "https://example.com/line\nbreak"
        ));
    }
}
