//! Device-scoped active immutable descriptor, not a grant. Never serialize package paths or credentials.
use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    art_id: String,
}

fn identity(s: &Services<'_>, art: &str) -> LocalResult<(String, String)> {
    let tool = s
        .tools
        .get_tool(art)
        .map_err(|_| fail(503, "local_art_registry_unavailable"))?
        .filter(|t| t.enabled && t.qualified_id() == art)
        .ok_or_else(|| fail(403, "local_art_disabled"))?;
    let package = tool
        .metadata
        .as_ref()
        .and_then(|m| m.get("artPackage"))
        .ok_or_else(|| fail(409, "local_art_package_unavailable"))?;
    let version = package
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| fail(409, "local_art_package_unavailable"))?;
    let digest = package
        .get("digest")
        .and_then(Value::as_str)
        .ok_or_else(|| fail(409, "local_art_package_unavailable"))?;
    Ok((version.into(), digest.into()))
}
pub(super) fn handle(
    request: &ParsedHttpRequest,
    actor: &str,
    s: &Services<'_>,
    guard: &DeviceGuard,
) -> LocalResult<Value> {
    let input: Input = parse(&request.body)?;
    let active = identity(s, &input.art_id)?;
    let time = unix_time_millis();
    // Approval lookup precedes resolver work; installed or visible in the old catalog is not consent.
    let approval =
        policy::describe_approval(s.root, actor, &input.art_id, &active.0, &active.1, time)?;
    let grant = Grant {
        protocol_version: PROTOCOL.into(),
        grant_id: "description-validation".into(),
        binding: Binding {
            origin: "https://localhost".into(),
            request_id: "description-validation".into(),
            device_id: actor.into(),
            workspace_id: "description-validation".into(),
            instance_id: "description-validation".into(),
            generation: 1,
            art_id: input.art_id,
            art_version: active.0.clone(),
            package_digest: active.1.clone(),
            sdk_version: SDK.into(),
        },
        execution_owner: "otter".into(),
        result_owner: "otter_workspace".into(),
        permissions: approval.permissions.clone(),
        services: approval.services.clone(),
        issued_at_ms: time,
        expires_at_ms: time
            .saturating_add(300_000)
            .min(approval.valid_until_ms)
            .min(guard.expires),
        offline_until_ms: 0,
        source: Source {
            resource_id: format!("sha256:{}", "0".repeat(64)),
            digest: "0".repeat(64),
            size: 1,
        },
    };
    grant
        .validate()
        .map_err(|_| fail(409, "local_art_invalid_descriptor"))?;
    PREPARING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| fail(429, "local_art_prepare_busy"))?;
    let _preparation = Preparation;
    // Reuse exact resolver/trust/dependency/manifest/source admission. No silent scope intersection.
    let _ = source::load(s, &grant)?;
    let _lock = policy::LOCK
        .lock()
        .map_err(|_| fail(503, "local_art_policy_unavailable"))?;
    if policy::approved_unlocked(s.root, &grant, unix_time_millis())? != approval {
        return Err(fail(409, "local_art_approval_changed"));
    }
    let current = device_guard(request, actor, s.devices)?;
    if !Arc::ptr_eq(&guard.revoked, &current.revoked) {
        return Err(fail(403, "local_art_device_changed"));
    }
    if identity(s, &grant.binding.art_id)? != active {
        return Err(fail(409, "local_art_package_changed"));
    }
    if current.revoked.load(Ordering::Acquire) || unix_time_millis() >= grant.expires_at_ms {
        return Err(fail(403, "local_art_expired"));
    }
    Ok(
        json!({"protocolVersion":"loom.art.local.describe.v1","descriptor":{
            "artId":grant.binding.art_id,"artVersion":grant.binding.art_version,
            "packageDigest":grant.binding.package_digest,"sdkVersion":SDK,
            "permissions":grant.permissions,"services":grant.services
        }}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn description_request_rejects_duplicate_unknown_and_oversized_fields() {
        for body in [
            r#"{"artId":"a/b","artId":"a/b"}"#,
            r#"{"artId":"a/b","origin":"https://other"}"#,
        ] {
            assert!(parse::<Input>(body).is_err());
        }
        assert!(parse::<Input>(&format!(r#"{{"artId":"{}"}}"#, "x".repeat(MAX_REQUEST))).is_err());
    }
}
