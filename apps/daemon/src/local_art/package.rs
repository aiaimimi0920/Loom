//! 精确grant绑定资源分发；不接受任意路径/URL，也不让资源缓存替代当前设备和审批。
use super::*;
use loom_protocol::local_art::package::{visibility, Package, PROTOCOL, RESOURCE};
pub(super) fn protocol(path: &str) -> Option<&'static str> {
    (path == visibility::ROUTE).then_some(visibility::PREPARE)
}
pub(super) fn proof(
    s: &Services<'_>,
    grant: &Grant,
    loaded: &source::Loaded,
    key: Option<String>,
) -> LocalResult<Value> {
    // 实际字节已在持审批锁前逐项验证；签发不在策略锁内执行磁盘读取。
    let package = Package {
        protocol_version: PROTOCOL.into(),
        grant: grant.clone(),
        device_public_key: if grant.offline_until_ms != 0 {
            key.unwrap_or_default()
        } else {
            String::new()
        },
        resources: loaded
            .resources
            .iter()
            .map(|e| e.resource.clone())
            .collect(),
    };
    s.issuer
        .issue_local_art_visibility_package(
            &loom_protocol::local_art::package::visibility::Package {
                protocol_version: loom_protocol::local_art::package::visibility::PROTOCOL.into(),
                package,
                ports: loaded
                    .ports
                    .clone()
                    .ok_or_else(|| fail(409, "local_art_ports_invalid"))?,
                visibility: loaded
                    .visibility
                    .clone()
                    .ok_or_else(|| fail(409, "local_art_ports_invalid"))?,
            },
        )
        .map_err(|_| fail(503, "local_art_signing_failed"))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    protocol_version: String,
    grant: Envelope,
    resource_id: String,
}
pub(super) fn fetch(
    request: &ParsedHttpRequest,
    actor: &str,
    s: &Services<'_>,
    guard: DeviceGuard,
) -> LocalResult<Value> {
    if request.body.len() > 32 * 1024 {
        return Err(fail(413, "local_art_request_too_large"));
    }
    let input: Request =
        serde_json::from_str(&request.body).map_err(|_| fail(400, "local_art_invalid_request"))?;
    if input.protocol_version != RESOURCE {
        return Err(fail(400, "local_art_invalid_request"));
    }
    let grant = s
        .issuer
        .verify_local_art(&input.grant)
        .map_err(|_| fail(403, "local_art_invalid_signature"))?;
    if grant.binding.device_id != actor
        || unix_time_millis() < grant.issued_at_ms
        || unix_time_millis() >= grant.expires_at_ms
    {
        return Err(fail(403, "local_art_resource_scope"));
    }
    let approval = policy::approved(s.root, &grant, unix_time_millis())?;
    let _preparation = s.tasks.enter_preparation()?;
    let loaded = source::resolve(s, &grant)?;
    if loaded.bytes.len() as u64 != grant.source.size
        || sha256_bytes(&loaded.bytes) != grant.source.digest
    {
        return Err(fail(409, "local_art_source_changed"));
    }
    let entry = loaded
        .resources
        .iter()
        .find(|e| e.resource.id == input.resource_id)
        .ok_or_else(|| fail(404, "local_art_resource_missing"))?;
    let bytes = source_resources::read(s, &loaded.directory, entry)?;
    let lease = service_lease::Lease::new(s, grant.clone(), approval, guard);
    if !lease.valid() {
        return Err(fail(403, "local_art_authorization_changed"));
    }
    Ok(
        json!({"protocolVersion":RESOURCE,"grantId":grant.grant_id,"resourceId":entry.resource.id,"digest":entry.resource.digest,"bytes":BASE64.encode(bytes)}),
    )
}
pub(super) fn response(mut response: Value, proof: Value, protocol: &str) -> Value {
    response["protocolVersion"] = protocol.into();
    response["package"] = proof;
    response
}
