//! Opt-in Otter-only source distribution. Device login and package installation alone do not authorize execution.
use super::*;
use loom_protocol::local_art::{Binding, Envelope, Grant, Permission, Source, PROTOCOL, SDK};
mod cached;
mod describe;
mod live_control;
mod live_host;
mod live_trigger;
mod live_viewer;
mod package;
mod policy;
mod service;
mod service_execution;
mod service_image_output;
mod service_images;
mod service_lease;
mod service_tasks;
mod source;
mod source_ports;
mod source_resources;
pub(crate) use service_tasks::Tasks;
const PREFIX: &str = "/v1/art/local/";
const MAX_REQUEST: usize = 16 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Prepare {
    binding: Binding,
    permissions: Vec<Permission>,
    services: Vec<String>,
}
pub(crate) fn device_route(method: &str, path: &str) -> bool {
    (method == "GET" && path == "/v1/art/local/issuer")
        || (method == "POST"
            && matches!(
                path,
                "/v3/art/local/prepare-package"
                    | "/v1/art/local/resource"
                    | "/v1/art/local/live-viewer/attach"
                    | "/v1/art/local/live-viewer/control"
                    | "/v1/art/local/live-trigger"
                    | "/v1/art/local/describe"
                    | "/v1/art/local/service/submit"
                    | "/v1/art/local/service/submit-images"
                    | "/v1/art/local/service/submit-image-output"
                    | "/v1/art/local/service/poll-image-output"
                    | "/v1/art/local/service/submit-confirmable"
                    | "/v1/art/local/service/poll-confirmable"
                    | "/v1/art/local/service/confirm"
                    | "/v1/art/local/service/poll"
                    | "/v1/art/local/service/cancel"
            ))
}
pub(crate) struct Services<'a> {
    pub root: &'a Path,
    pub devices: &'a SharedDeviceRegistryStore,
    pub tools: &'a ToolRegistry,
    pub frameworks: &'a FrameworkRegistry,
    pub issuer: &'a offline_peers::OfflinePeers,
    pub tasks: &'a Tasks,
    pub runtime: &'a SharedCapabilityRuntime,
    pub resources: &'a SharedCapabilityResourceBroker,
    pub surface_resources: &'a SharedSurfaceResourceStore,
    pub live_sessions: &'a SharedLiveSessionStore,
}
pub(crate) fn route(
    request: &ParsedHttpRequest,
    actor: Option<&str>,
    admin: bool,
    services: &Services<'_>,
) -> Option<Result<(u16, String)>> {
    let path = request.path.split('?').next().unwrap_or_default();
    if !path.starts_with(PREFIX) && package::protocol(path).is_none() {
        return None;
    }
    Some(match handle(request, path, actor, admin, services) {
        Ok(value) => serde_json::to_string(&value)
            .map(|body| (200, body))
            .map_err(Into::into),
        Err(error) => structured_error(
            error.status,
            json!({"code":error.code,"message":error.code}),
        ),
    })
}
struct Failure {
    status: u16,
    code: &'static str,
}
type LocalResult<T> = std::result::Result<T, Failure>;
fn fail(status: u16, code: &'static str) -> Failure {
    Failure { status, code }
}
fn parse<T: serde::de::DeserializeOwned>(body: &str) -> LocalResult<T> {
    if body.len() > MAX_REQUEST {
        return Err(fail(413, "local_art_request_too_large"));
    }
    serde_json::from_str(body).map_err(|_| fail(400, "local_art_invalid_request"))
}
fn handle(
    request: &ParsedHttpRequest,
    path: &str,
    actor: Option<&str>,
    admin: bool,
    s: &Services<'_>,
) -> LocalResult<Value> {
    if path == "/v1/art/local/approvals" {
        if !admin {
            return Err(fail(403, "local_art_admin_required"));
        }
        return policy::manage(s.root, request);
    }
    let actor = actor.ok_or_else(|| fail(403, "local_art_device_required"))?;
    let guard = device_guard(request, actor, s.devices)?;
    if path == "/v1/art/local/live-viewer/attach" {
        return live_viewer::attach(request, actor, s, guard);
    }
    if path == "/v1/art/local/live-viewer/control" {
        return live_control::handle(request, actor, s, guard);
    }
    if path == "/v1/art/local/live-trigger" {
        return live_trigger::handle(request, actor, s, guard);
    }
    if request.method == "POST" && path == "/v1/art/local/resource" {
        return package::fetch(request, actor, s, guard);
    }
    if path.starts_with("/v1/art/local/service/") {
        return service::handle(request, path, actor, s, guard);
    }
    if request.method == "POST" && path == "/v1/art/local/describe" {
        return describe::handle(request, actor, s, &guard);
    }
    if request.method == "GET" && path == "/v1/art/local/issuer" {
        return s
            .issuer
            .local_art_issuer()
            .map_err(|_| fail(503, "local_art_issuer_unavailable"));
    }
    let (input, cached) = cached::input(request, path)?;
    if input.binding.device_id != actor {
        return Err(fail(403, "local_art_device_mismatch"));
    }
    let time = unix_time_millis();
    let mut grant = Grant {
        protocol_version: PROTOCOL.into(),
        grant_id: format!("grant:{}", Uuid::new_v4()),
        binding: input.binding,
        execution_owner: "otter".into(),
        result_owner: "otter_workspace".into(),
        permissions: input.permissions,
        services: input.services,
        issued_at_ms: time,
        expires_at_ms: time.saturating_add(300_000),
        offline_until_ms: 0,
        source: Source {
            resource_id: format!("sha256:{}", "0".repeat(64)),
            digest: "0".repeat(64),
            size: 1,
        },
    };
    grant
        .validate()
        .map_err(|_| fail(400, "local_art_invalid_contract"))?;
    let origin = reqwest::Url::parse(&grant.binding.origin)
        .map_err(|_| fail(400, "local_art_invalid_origin"))?;
    if !matches!(origin.scheme(), "http" | "https")
        || origin.origin().ascii_serialization() != grant.binding.origin
    {
        return Err(fail(400, "local_art_invalid_origin"));
    }
    // Do not hold the policy lock while package verification performs bounded disk work.
    let approval = policy::approved(s.root, &grant, time)?;
    let _preparation = s.tasks.enter_preparation()?;
    let loaded = source::resolve(s, &grant)?;
    for entry in &loaded.resources {
        source_resources::read(s, &loaded.directory, entry)?;
    }
    let bytes = &loaded.bytes;
    let hash = sha256_bytes(bytes);
    grant.source = Source {
        resource_id: format!("sha256:{hash}"),
        digest: hash,
        size: bytes.len() as u64,
    };
    // Serialize revocation with issuance; no policy update can slip between the final check and signing.
    let _lock = policy::LOCK
        .lock()
        .map_err(|_| fail(503, "local_art_policy_unavailable"))?;
    if policy::approved_unlocked(s.root, &grant, unix_time_millis())? != approval {
        return Err(fail(409, "local_art_approval_changed"));
    }
    let current = device_guard(request, actor, s.devices)?;
    source::enabled(s, &grant)?;
    if !Arc::ptr_eq(&guard.revoked, &current.revoked) {
        return Err(fail(403, "local_art_device_changed"));
    }
    grant.expires_at_ms = grant
        .expires_at_ms
        .min(approval.valid_until_ms)
        .min(current.expires);
    if approval.offline_ttl_ms > 0 {
        grant.offline_until_ms = grant
            .issued_at_ms
            .saturating_add(approval.offline_ttl_ms)
            .min(grant.expires_at_ms);
    }
    if current.revoked.load(Ordering::Acquire) || unix_time_millis() >= grant.expires_at_ms {
        return Err(fail(403, "local_art_expired"));
    }
    let encoded = s
        .issuer
        .issue_local_art(&grant)
        .map_err(|_| fail(503, "local_art_signing_failed"))?;
    source::enabled(s, &grant)?;
    // A concurrent session revoke after signing must not release a new grant response.
    if current.revoked.load(Ordering::Acquire) || unix_time_millis() >= grant.expires_at_ms {
        return Err(fail(403, "local_art_expired"));
    }
    let envelope: Envelope =
        serde_json::from_slice(&encoded).map_err(|_| fail(503, "local_art_signing_failed"))?;
    let response = cached::response(envelope, bytes, &grant.source.digest, cached.as_deref());
    let proof = package::proof(s, &grant, &loaded, current.public_key.clone())?;
    if current.revoked.load(Ordering::Acquire) || unix_time_millis() >= grant.expires_at_ms {
        return Err(fail(403, "local_art_expired"));
    }
    Ok(package::response(
        response,
        proof,
        loom_protocol::local_art::package::visibility::PREPARE,
    ))
}
struct DeviceGuard {
    expires: u64,
    revoked: Arc<AtomicBool>,
    public_key: Option<String>,
}
fn device_guard(
    request: &ParsedHttpRequest,
    actor: &str,
    devices: &SharedDeviceRegistryStore,
) -> LocalResult<DeviceGuard> {
    let token = request
        .authorization_credential("Device")
        .ok_or_else(|| fail(403, "local_art_device_required"))?;
    let store = devices
        .lock()
        .map_err(|_| fail(503, "device_registry_unavailable"))?;
    let session = store
        .sessions
        .get(&sha256_bytes(token.as_bytes()))
        .ok_or_else(|| fail(403, "local_art_device_revoked"))?;
    let device = store
        .devices
        .get(actor)
        .ok_or_else(|| fail(403, "local_art_device_revoked"))?;
    if session.device_id != actor
        || device.approval != "approved"
        || !device.enabled
        || session.session_epoch != device.session_epoch
        || session.expires_at_ms <= unix_time_millis()
        || session.revoked.load(Ordering::Acquire)
    {
        return Err(fail(403, "local_art_device_revoked"));
    }
    Ok(DeviceGuard {
        expires: session.expires_at_ms,
        revoked: session.revoked.clone(),
        public_key: device.public_key.clone(),
    })
}
