//! Device 专属服务入口；只有当前 issuer、精确 Art 授权和原会话可以读写任务。
use super::*;
use loom_protocol::local_art::service::confirmation;
use loom_protocol::local_art::service::{Control, Submit};

pub(super) fn handle(
    request: &ParsedHttpRequest,
    path: &str,
    actor: &str,
    s: &Services<'_>,
    guard: DeviceGuard,
) -> LocalResult<Value> {
    if request.method != "POST" {
        return Err(fail(405, "local_art_method_not_allowed"));
    }
    if matches!(
        path,
        "/v1/art/local/service/submit"
            | "/v1/art/local/service/submit-images"
            | "/v1/art/local/service/submit-image-output"
            | "/v1/art/local/service/submit-confirmable"
    ) {
        let confirmable = path.ends_with("/submit-confirmable");
        let mut image_output = path.ends_with("/submit-image-output");
        let (input, images) = if confirmable {
            let input =
                confirmation::Request::parse(request.body.as_bytes()).map_err(contract_error)?;
            image_output = input.image_output;
            (input.call, input.images)
        } else if image_output {
            let input = loom_protocol::local_art::service::image_output::Request::parse(
                request.body.as_bytes(),
            )
            .map_err(contract_error)?;
            (input.call, input.images)
        } else if path.ends_with("/submit-images") {
            let input = loom_protocol::local_art::service::images::SubmitImages::parse(
                request.body.as_bytes(),
            )
            .map_err(contract_error)?;
            if input.protocol_version != loom_protocol::local_art::service::images::PROTOCOL {
                return Err(fail(400, "local_art_service_contract"));
            }
            loom_protocol::local_art::service::images::validate_images(&input.images)
                .map_err(contract_error)?;
            (input.call, input.images)
        } else {
            (
                Submit::parse(request.body.as_bytes()).map_err(contract_error)?,
                Vec::new(),
            )
        };
        let grant = s
            .issuer
            .verify_local_art(&input.grant)
            .map_err(|_| fail(403, "local_art_invalid_signature"))?;
        input
            .validate(&grant, actor, unix_time_millis())
            .map_err(contract_error)?;
        let command = command_id(&input.owner.service)?;
        let approval = policy::approved(s.root, &grant, unix_time_millis())?;
        // Repeat immutable source/trust admission before dispatch, not on every poll.
        let _preparation = s.tasks.enter_preparation()?;
        let bytes = source::load(s, &grant)?;
        if bytes.len() as u64 != grant.source.size || sha256_bytes(&bytes) != grant.source.digest {
            return Err(fail(403, "local_art_source_changed"));
        }
        let lease = service_lease::Lease::new(s, grant, approval, guard);
        if !lease.valid() {
            return Err(fail(403, "local_art_authorization_changed"));
        }
        let owner = input.owner.clone();
        let reply = s.tasks.submit(
            input,
            command,
            lease,
            Arc::clone(s.runtime),
            images,
            service_images::Context {
                resources: Arc::clone(s.resources),
                surface_resources: Arc::clone(s.surface_resources),
                image_output,
                confirmable,
            },
        )?;
        if confirmable {
            return s.tasks.confirmation_output(&owner);
        }
        if image_output {
            return s.tasks.image_output(&owner);
        }
        return serde_json::to_value(reply).map_err(|_| fail(500, "local_art_reply_invalid"));
    }
    if !matches!(
        path,
        "/v1/art/local/service/poll"
            | "/v1/art/local/service/cancel"
            | "/v1/art/local/service/poll-image-output"
            | "/v1/art/local/service/poll-confirmable"
            | "/v1/art/local/service/confirm"
    ) {
        return Err(fail(404, "local_art_route_not_found"));
    }
    let confirmed = matches!(
        path,
        "/v1/art/local/service/poll-confirmable" | "/v1/art/local/service/confirm"
    );
    let (input, decision) = if confirmed {
        let input = confirmation::RequestControl::parse(
            request.body.as_bytes(),
            path.ends_with("/confirm"),
        )
        .map_err(contract_error)?;
        (input.control, input.decision)
    } else {
        (
            Control::parse(request.body.as_bytes()).map_err(contract_error)?,
            None,
        )
    };
    let grant = s
        .issuer
        .verify_local_art(&input.grant)
        .map_err(|_| fail(403, "local_art_invalid_signature"))?;
    input
        .validate(&grant, actor, unix_time_millis())
        .map_err(contract_error)?;
    let approval = policy::approved(s.root, &grant, unix_time_millis())?;
    let lease = service_lease::Lease::new(s, grant, approval, guard);
    if !lease.valid() {
        return Err(fail(403, "local_art_authorization_changed"));
    }
    if let Some(decision) = decision {
        s.tasks.decide(&input.owner, &lease, &decision)?;
    }
    let reply = s
        .tasks
        .control(&input.owner, &lease, path.ends_with("/cancel"))?;
    if confirmed {
        return s.tasks.confirmation_output(&input.owner);
    }
    if path.ends_with("/poll-image-output") {
        return s.tasks.image_output(&input.owner);
    }
    serde_json::to_value(reply).map_err(|_| fail(500, "local_art_reply_invalid"))
}
fn contract_error(error: loom_protocol::local_art::service::Error) -> Failure {
    use loom_protocol::local_art::service::Error;
    match error {
        Error::Limit => fail(413, "local_art_service_limit"),
        Error::Identity | Error::Permission | Error::Time => fail(403, "local_art_service_scope"),
        _ => fail(400, "local_art_service_contract"),
    }
}
// Explicit namespaced Plugin command mapping; never reaches the admin/core invoke dispatcher.
fn command_id(service: &str) -> LocalResult<String> {
    let parts = service.split(':').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "capability" || parts[1].is_empty() || parts[2].is_empty() {
        return Err(fail(400, "local_art_service_unsupported"));
    }
    Ok(format!("{}/{}", parts[1], parts[2]))
}
