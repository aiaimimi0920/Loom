use super::model::*;
use crate::{json, ParsedHttpRequest, SharedDeviceRegistryStore, Value};

const MAX_BODY: usize = 8 * 1024;

pub(crate) fn handle(
    request: &ParsedHttpRequest,
    path: &str,
    actor: Option<&str>,
    admin: bool,
    registry: &SharedDeviceRegistryStore,
) -> Option<anyhow::Result<(u16, String)>> {
    let report = path == "/v1/otter/locale-report";
    let target = crate::path_id_with_suffix(path, "/v1/devices/", "/locale-requests");
    if !report && target.is_none() {
        return None;
    }
    Some(dispatch(request, actor, admin, registry, report, target))
}

fn dispatch(
    request: &ParsedHttpRequest,
    actor: Option<&str>,
    admin: bool,
    registry: &SharedDeviceRegistryStore,
    report: bool,
    target: Option<&str>,
) -> anyhow::Result<(u16, String)> {
    if request.method != "POST" {
        return error(Error(405, "locale_method_not_allowed"));
    }
    if request.body.len() > MAX_BODY {
        return error(Error(413, "locale_input_too_large"));
    }
    let now = crate::unix_time_millis();
    let mut store = registry
        .lock()
        .map_err(|_| anyhow::anyhow!("device registry unavailable"))?;
    if report {
        let Some(actor) = actor else {
            return error(Error(403, "locale_device_required"));
        };
        let (epoch, authorized_until) = match authority(&store, request, actor, now) {
            Ok(value) => value,
            Err(value) => return error(value),
        };
        let input: Report = match serde_json::from_str(&request.body) {
            Ok(value) => value,
            Err(_) => return error(Error(400, "locale_input_invalid")),
        };
        return match store
            .locales
            .report(actor, epoch, authorized_until, input, now)
        {
            Ok(locale) => {
                let pending = delivery(&locale);
                let mut locale = serde_json::to_value(locale)?;
                locale["request"] = pending;
                Ok((
                    200,
                    json!({
                        "loomId": store.connection_info().loom_id,
                        "deviceId": actor,
                        "locale": locale,
                    })
                    .to_string(),
                ))
            }
            Err(value) => error(value),
        };
    }
    if !admin {
        return error(Error(403, "locale_admin_required"));
    }
    let target = target.expect("locale request target");
    let epoch = match store.authorized_keyed_device(target) {
        Ok(device) => device.session_epoch,
        Err(value) => return crate::device_auth_error_response(value),
    };
    let input: RequestInput = match serde_json::from_str(&request.body) {
        Ok(value) => value,
        Err(_) => return error(Error(400, "locale_input_invalid")),
    };
    match store.locales.request(target, epoch, input, now) {
        Ok((created, locale)) => Ok((
            if created { 202 } else { 200 },
            json!({"locale": locale}).to_string(),
        )),
        Err(value) => error(value),
    }
}

// Recheck the already authenticated session while holding the same lock as the
// mutation. Revocation between top-level dispatch and this handler stays fail-closed.
fn authority(
    registry: &crate::DeviceRegistryStore,
    request: &ParsedHttpRequest,
    actor: &str,
    now: u64,
) -> Result<(u64, u64)> {
    let device = registry
        .authorized_keyed_device(actor)
        .map_err(|_| Error(403, "locale_device_unavailable"))?;
    let token = request
        .authorization_credential("Device")
        .ok_or(Error(403, "locale_device_required"))?;
    let session = registry
        .sessions
        .get(&crate::sha256_bytes(token.as_bytes()))
        .filter(|session| {
            session.device_id == actor
                && session.session_epoch == device.session_epoch
                && session.expires_at_ms > now
                && !session.revoked.load(std::sync::atomic::Ordering::SeqCst)
        })
        .ok_or(Error(403, "locale_device_unavailable"))?;
    Ok((device.session_epoch, session.expires_at_ms))
}

pub(crate) fn request_for(
    registry: &crate::DeviceRegistryStore,
    request: &ParsedHttpRequest,
    actor: &str,
) -> Value {
    let now = crate::unix_time_millis();
    let Ok((epoch, _)) = authority(registry, request, actor, now) else {
        return Value::Null;
    };
    registry
        .locales
        .view(actor, epoch, now)
        .map(|locale| delivery(&locale))
        .unwrap_or(Value::Null)
}

fn delivery(locale: &View) -> Value {
    let Some(request) = locale
        .request
        .as_ref()
        .filter(|request| locale.active && request.status == "pending")
    else {
        return Value::Null;
    };
    json!({
        "requestId": request.input.request_id,
        "receiverGeneration": request.input.receiver_generation,
        "expectedRevision": request.input.expected_revision,
        "language": request.input.language,
        "expiresAt": request.expires_at,
        "validForMs": request.valid_for_ms,
    })
}

fn error(value: Error) -> anyhow::Result<(u16, String)> {
    crate::structured_error(value.0, json!({"code": value.1, "message": value.1}))
}
