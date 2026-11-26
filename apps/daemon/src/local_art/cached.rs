//! Current v3 omits only known source bytes; provides no legacy prepare entrypoint or version negotiation.
use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedPrepare {
    protocol_version: String,
    binding: Binding,
    permissions: Vec<Permission>,
    services: Vec<String>,
    known_source_digest: String,
}
pub(super) fn input(
    request: &ParsedHttpRequest,
    path: &str,
) -> LocalResult<(Prepare, Option<String>)> {
    if request.method != "POST" || path != loom_protocol::local_art::package::visibility::ROUTE {
        return Err(fail(404, "local_art_route_not_found"));
    }
    let input: CachedPrepare = parse(&request.body)?;
    if input.protocol_version != loom_protocol::local_art::package::visibility::PREPARE
        || input.known_source_digest.len() != 64
        || !input
            .known_source_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(fail(400, "local_art_invalid_request"));
    }
    Ok((
        Prepare {
            binding: input.binding,
            permissions: input.permissions,
            services: input.services,
        },
        Some(input.known_source_digest),
    ))
}
pub(super) fn response(
    envelope: Envelope,
    source: &[u8],
    digest: &str,
    cached: Option<&str>,
) -> Value {
    json!({"protocolVersion":loom_protocol::local_art::package::visibility::PREPARE,"grant":envelope,
        "source":if cached == Some(digest) { Value::Null } else { Value::String(BASE64.encode(source)) }})
}
