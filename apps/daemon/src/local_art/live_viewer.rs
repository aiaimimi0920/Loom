//! Native user intent joins the original Live store; no Surface fabrication, JS route or control grant.
use super::*;
use loom_protocol::local_art::live_viewer::{Attach, Error};

pub(super) fn attach(
    request: &ParsedHttpRequest,
    actor: &str,
    s: &Services<'_>,
    guard: DeviceGuard,
) -> LocalResult<Value> {
    if request.method != "POST" {
        return Err(fail(405, "local_art_method_not_allowed"));
    }
    let input = Attach::parse(request.body.as_bytes()).map_err(contract_error)?;
    let grant = s
        .issuer
        .verify_local_art(&input.grant)
        .map_err(|_| fail(403, "local_art_invalid_signature"))?;
    input
        .validate(&grant, actor, unix_time_millis())
        .map_err(contract_error)?;
    live_host::authorize(request, actor, s, guard, &grant, true, || {
        let snapshot = s
            .live_sessions
            .attach_viewer_with_policy(actor, input.envelope, input.require_existing_membership)
            .map_err(|error| fail(error.status, error.code))?;
        serde_json::to_value(snapshot).map_err(|_| fail(503, "local_art_live_reply_invalid"))
    })
}
pub(super) fn contract_error(error: Error) -> Failure {
    match error {
        Error::Limit => fail(413, "local_art_live_limit"),
        Error::Identity | Error::Time | Error::Permission => fail(403, "local_art_live_scope"),
        Error::Wire => fail(400, "local_art_live_contract"),
    }
}
