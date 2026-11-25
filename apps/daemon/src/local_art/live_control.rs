//! Native menu intent never joins a viewer, grants JS control or bypasses the original controller/input authority.
use super::*;
use loom_protocol::local_art::live_control::{Operation, Request};
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
    let reducing = input.reducing();
    let acquiring = matches!(input.operation, Operation::Acquire { .. });
    let apply = || {
        s.live_sessions
            .local_art_control(actor, input.operation)
            .map_err(|e| fail(e.status, e.code))
    };
    if reducing {
        // Only release one's own existing controller, never reacquire/input. A revoked Art
        // approval or stopped host must not prevent this reduction; Device authorization remains mandatory.
        live_host::current_device(request, actor, s, &guard, None)?;
        apply()
    } else {
        live_host::authorize(request, actor, s, guard, &grant, acquiring, apply)
    }
}
