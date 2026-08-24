//! Cloud request template rendering and path lookup.

use super::*;

/// Find a placeholder that a template declared and substitution did not fill.
///
/// The template's own `{{…}}` tokens are the only ones that count. Looking for `{{` in the rendered
/// text instead would also catch braces that arrived inside an argument's value, which is legitimate
/// content — a caption or a code snippet may well contain them — and used to make the field vanish.
pub(super) fn unresolved_cloud_template_placeholder<'a>(
    template: &'a str,
    rendered: &str,
) -> Option<&'a str> {
    let mut remainder = template;
    while let Some(start) = remainder.find("{{") {
        let after_start = &remainder[start..];
        let Some(end) = after_start.find("}}") else {
            // An unterminated `{{` is not a placeholder; nothing can substitute it and nothing else in
            // the template can close it either.
            return None;
        };
        let placeholder = &after_start[..end + 2];
        if rendered.contains(placeholder) {
            return Some(placeholder);
        }
        remainder = &after_start[end + 2..];
    }
    None
}

pub(super) fn substitute_cloud_template(template: &str, arguments: &serde_json::Value) -> String {
    substitute_cloud_template_with(template, arguments, str::to_owned)
}

/// Substitute the cloud template forms with each argument passed through `render`.
///
/// `render` is where the destination's escaping rule lives: a value going into a URL is
/// percent-encoded, a value going into a plain text body or a multipart field is used as written.
pub(super) fn substitute_cloud_template_with(
    template: &str,
    arguments: &serde_json::Value,
    render: impl Fn(&str) -> String,
) -> String {
    let mut rendered = template.to_owned();
    let Some(arguments) = arguments.as_object() else {
        return rendered;
    };
    for (key, value) in arguments {
        let replacement = render(&scalar_template_value(value));
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}.value}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}.path}}}}"), &replacement);
    }
    rendered
}

/// Percent-encode an argument that is being substituted into an endpoint URL.
///
/// Substitution used to splice the raw value in, so an argument could rewrite the request's
/// authority: an endpoint of `https://api.example.com{{inputs.suffix}}` with a suffix of
/// `@127.0.0.1:8787/` produced a URL whose host was `127.0.0.1` and whose userinfo was
/// `api.example.com`, sending the Art's own credential headers wherever the caller chose. Everything
/// outside the unreserved set is encoded, so a substituted value can no longer end the path, open a
/// query, or introduce userinfo — it can only ever be one path segment or one parameter value.
pub(super) fn percent_encode_cloud_template_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(*byte));
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// The authority a cloud endpoint declares: the text between `://` and the first `/`, `?`, or `#`.
pub(super) fn cloud_endpoint_authority(endpoint: &str) -> Option<&str> {
    let after_scheme = endpoint.split_once("://")?.1;
    let end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    Some(&after_scheme[..end])
}

/// Confirm substitution did not move the endpoint to a different host.
///
/// Percent-encoding already prevents an argument from introducing userinfo or a port, and
/// `validate_outbound_url` re-checks the rendered URL against the declared domains. This is the
/// remaining invariant worth stating outright: when the author wrote a fixed authority, the rendered
/// request has to still carry exactly that authority, whatever the arguments contained.
pub(super) fn validate_rendered_cloud_authority(
    template: &str,
    rendered: &str,
) -> Result<(), String> {
    let Some(declared) = cloud_endpoint_authority(template) else {
        return Ok(());
    };
    if declared.contains("{{") {
        return Ok(());
    }
    let rendered_authority = cloud_endpoint_authority(rendered).unwrap_or_default();
    if rendered_authority == declared {
        return Ok(());
    }
    Err(format!(
        "rendered endpoint authority `{rendered_authority}` does not match the declared authority `{declared}`"
    ))
}

/// Render a JSON-shaped cloud template — the header block, or a JSON request body — by substituting
/// into the parsed document's strings instead of splicing text into the serialized form.
///
/// Splicing let an argument close the string it landed in and add members beside it: a `text`
/// argument of `x","stream":true` turned `{"prompt":"{{inputs.text}}"}` into a two-member object that
/// still parsed, so a caller could set request fields the author never exposed. Substituting after
/// the parse keeps every argument a single string value no matter what punctuation it carries.
///
/// A template that is not itself valid JSON — a placeholder standing in for an unquoted number, say
/// — cannot be parsed before substitution, so it keeps the original splice-then-parse path.
pub(super) fn render_cloud_json_template(
    tool: &ToolDefinition,
    field: &'static str,
    template: &str,
    arguments: &serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    if let Ok(mut document) = serde_json::from_str::<serde_json::Value>(template) {
        substitute_cloud_json_document(&mut document, arguments);
        return Ok(document);
    }
    let rendered = substitute_cloud_template(template, arguments);
    serde_json::from_str::<serde_json::Value>(&rendered).map_err(|source| {
        ToolRegistryError::CloudTemplate {
            id: tool.id.clone(),
            field,
            reason: source.to_string(),
        }
    })
}

pub(super) fn substitute_cloud_json_document(
    document: &mut serde_json::Value,
    arguments: &serde_json::Value,
) {
    match document {
        serde_json::Value::String(value) => {
            *value = substitute_cloud_template(value, arguments);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                substitute_cloud_json_document(value, arguments);
            }
        }
        serde_json::Value::Object(entries) => {
            *entries = entries
                .iter()
                .map(|(key, value)| (substitute_cloud_template(key, arguments), value.clone()))
                .collect();
            for (_, value) in entries.iter_mut() {
                substitute_cloud_json_document(value, arguments);
            }
        }
        _ => {}
    }
}

pub(super) fn header_text_has_control_character(text: &str) -> bool {
    text.chars().any(char::is_control)
}

pub(super) fn scalar_template_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}
