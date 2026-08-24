//! Secret-safe formatting for transport diagnostics.

use super::*;

const REDACTED_SECRET: &str = "[REDACTED_SECRET]";

/// Keep an endpoint useful for diagnosis without exposing path or query credentials.
pub(super) fn remote_endpoint_label(url: &Url) -> String {
    url.origin().ascii_serialization()
}

/// Collect configured values that may be echoed by a child process or remote server.
pub(super) fn collect_sensitive_values<'a>(
    values: impl IntoIterator<Item = &'a String>,
) -> Vec<String> {
    let mut sensitive = Vec::new();
    for value in values {
        let value = value.trim();
        if value.len() < 4 {
            continue;
        }
        sensitive.push(value.to_owned());
        if let Some((scheme, token)) = value.split_once(' ') {
            if matches!(scheme.to_ascii_lowercase().as_str(), "bearer" | "basic")
                && token.trim().len() >= 4
            {
                sensitive.push(token.trim().to_owned());
            }
        }
    }
    sensitive.sort_by_key(|value| std::cmp::Reverse(value.len()));
    sensitive.dedup();
    sensitive
}

/// Replace configured secrets before diagnostic text leaves the transport boundary.
pub(super) fn redact_sensitive_text(text: &str, sensitive_values: &[String]) -> String {
    sensitive_values
        .iter()
        .fold(text.to_owned(), |redacted, value| {
            redacted.replace(value, REDACTED_SECRET)
        })
}
