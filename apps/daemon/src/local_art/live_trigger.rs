//! Auto-trigger accepts only explicit native host intent; Preview proves host identity only and never implicitly registers a trigger.
use super::*;
use loom_protocol::local_art::live_trigger::{Operation, Request};
pub(super) fn handle(
    request: &ParsedHttpRequest,
    actor: &str,
    s: &Services<'_>,
    guard: DeviceGuard,
) -> LocalResult<Value> {
    if request.method != "POST" {
        return Err(fail(405, "local_art_method_not_allowed"));
    }
    let input = Request::parse(request.body.as_bytes()).map_err(live_viewer::contract_error)?;
    let grant = s
        .issuer
        .verify_local_art(&input.grant)
        .map_err(|_| fail(403, "local_art_invalid_signature"))?;
    input
        .validate(&grant, actor, unix_time_millis())
        .map_err(live_viewer::contract_error)?;
    let execution = input.operation.requires_execution();
    let configuring = matches!(input.operation, Operation::Configure { .. });
    let apply = |revision| {
        let reply = s
            .live_sessions
            .local_art_trigger(actor, &grant, input.operation, revision)
            .map_err(|error| fail(error.status, error.code))?;
        serde_json::to_value(reply).map_err(|_| fail(503, "local_art_live_reply_invalid"))
    };
    if execution {
        live_host::authorize(request, actor, s, guard, &grant, configuring, || {
            // `authorize` holds the same policy lock; any revision bump permanently invalidates prior auto-trigger grants so revocation cannot be undone.
            let revision = policy::revision_unlocked(s.root)?;
            apply(Some(revision))
        })
    } else {
        // Inspect delivers no payload and Stop only drops privileges; original signature, exact host, and active Device remain required.
        live_host::current_device(request, actor, s, &guard, None)?;
        apply(None)
    }
}
