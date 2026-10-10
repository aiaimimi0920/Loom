//! Header-only admission before a client can upload a privileged large body.
use super::*;

pub(super) fn admit(
    request: &ParsedHttpRequest,
    large_body_route: bool,
    runtime: &DaemonRuntime,
) -> std::result::Result<(), (u16, String)> {
    enforce_request_security(request)?;
    if !large_body_route || request.has_admin_credential(&runtime.auth_token) {
        return Ok(());
    }
    let path = request.path.split('?').next().unwrap_or(&request.path);
    // These two peer endpoints authenticate a pinned-key proof inside the body,
    // not an administrator/session header. Keep their existing 6 MiB byte cap;
    // incremental growth still prevents a declared-length allocation attack.
    if offline_peers::OfflinePeers::body_proof_route(path) {
        return Ok(());
    }
    if offline_peers::OfflinePeers::device_body_route(path) {
        return offline_peers::OfflinePeers::admit_device_body(request, &runtime.device_registry)
            .map_err(|error| request_security_error(error.status, error.code, error.code));
    }
    if path == "/v1/projection-peers/raster-check" {
        return Err(request_security_error(
            403,
            "peer_admin_required",
            "peer_admin_required",
        ));
    }
    if let Some(token) = request.authorization_credential("Device") {
        if !device_session_route_allowed(&request.method, path) {
            return Err(request_security_error(
                403,
                "device_session_scope_denied",
                "device session is not permitted to access this Loom route",
            ));
        }
        let nonce = request.header("x-loom-device-nonce").ok_or_else(|| {
            request_security_error(
                400,
                "device_nonce_missing",
                "authenticated device requests require X-Loom-Device-Nonce",
            )
        })?;
        let registry = runtime.device_registry.lock().map_err(|_| {
            request_security_error(
                503,
                "device_registry_unavailable",
                "device registry is unavailable",
            )
        })?;
        // A preflight must not consume the nonce: routing rechecks revocation/expiry and
        // consumes it exactly once, after the complete body has arrived.
        registry
            .validate_device_session(token, nonce)
            .map_err(|error| request_security_error(error.status, &error.code, &error.message))?;
        return Ok(());
    }
    Err(request_security_error(401, "unauthorized", "large request bodies require a valid Loom administrator or scoped device session credential"))
}

impl DeviceRegistryStore {
    pub(super) fn validate_device_session(
        &self,
        token: &str,
        nonce: &str,
    ) -> std::result::Result<String, DeviceAuthError> {
        validate_device_auth_nonce(nonce)?;
        let session = self
            .sessions
            .get(&sha256_bytes(token.as_bytes()))
            .filter(|session| session.expires_at_ms > unix_time_millis())
            .ok_or_else(|| {
                DeviceAuthError::new(
                    401,
                    "device_session_invalid",
                    "device session is missing or expired",
                )
            })?;
        let device = self.devices.get(&session.device_id).ok_or_else(|| {
            DeviceAuthError::new(401, "device_revoked", "device has been removed")
        })?;
        if device.approval != "approved"
            || !device.enabled
            || device.session_epoch != session.session_epoch
        {
            return Err(DeviceAuthError::new(
                401,
                "device_revoked",
                "device is not approved, is disabled, or its sessions were revoked",
            ));
        }
        if session.used_nonces.len() >= DEVICE_SESSION_MAX_NONCES {
            return Err(DeviceAuthError::new(
                401,
                "device_session_nonce_capacity",
                "device session nonce budget is exhausted; create a new session",
            ));
        }
        if session.used_nonces.contains(nonce) {
            return Err(DeviceAuthError::new(
                409,
                "device_request_replayed",
                "device request nonce has already been used",
            ));
        }
        Ok(session.device_id.clone())
    }
}
