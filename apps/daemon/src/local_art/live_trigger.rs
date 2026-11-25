//! 自动触发只接原生宿主明确意图；Preview 仅证明宿主身份，不能隐式创建注册。
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
            // authorize 正持有同一 policy 锁；revision 变化让旧自动授权永久失效，不能撤销后复活。
            let revision = policy::revision_unlocked(s.root)?;
            apply(Some(revision))
        })
    } else {
        // Inspect 不交付 payload；Stop 只减权。原签名/精确宿主/当前 Device 仍是必要条件。
        live_host::current_device(request, actor, s, &guard, None)?;
        apply(None)
    }
}
