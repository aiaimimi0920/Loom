//! Header-only admission before a client can upload a privileged large body.
use super::*;

struct RejectionWriter<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
}

impl Write for RejectionWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                "admission response deadline",
            ));
        }
        self.stream
            .set_write_timeout(Some(remaining.min(Duration::from_millis(200))))?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn reject(mut stream: TcpStream, status: u16, body: &str, mut remaining: usize) {
    // Send the error/FIN immediately, including for a client waiting to send its body.
    // Discard eager upload bytes on the bounded read worker, never on the accept loop.
    let deadline = Instant::now() + Duration::from_millis(500);
    if write_response(
        &mut RejectionWriter {
            stream: &mut stream,
            deadline,
        },
        status,
        body,
    )
    .is_err()
    {
        return;
    }
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let mut buffer = [0_u8; HTTP_READ_CHUNK_BYTES];
    while remaining > 0 {
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero()
            || stream
                .set_read_timeout(Some(timeout.min(Duration::from_millis(50))))
                .is_err()
        {
            break;
        }
        let length = remaining.min(buffer.len());
        match stream.read(&mut buffer[..length]) {
            Ok(0) => break,
            Ok(read) => remaining -= read,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
}

pub(super) fn admit(
    request: &ParsedHttpRequest,
    large_body_route: bool,
    runtime: &DaemonRuntime,
) -> std::result::Result<(), (u16, String)> {
    // Small-body and WebSocket routes retain their existing dispatch-owned policy.
    if !large_body_route {
        return Ok(());
    }
    enforce_request_security(request)?;
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
    let admin = request.has_admin_credential(&runtime.auth_token);
    let requires_device = PROJECTION_ROUTES.contains(&path) || PROJECTION_V2_ROUTES.contains(&path);
    if admin && !requires_device {
        return Ok(());
    }
    let device_error = |status, code, message: &str| {
        if admin && requires_device {
            request_security_error(
                403,
                "projection_pairing_required",
                "projection_pairing_required",
            )
        } else {
            request_security_error(status, code, message)
        }
    };
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
            device_error(
                400,
                "device_nonce_missing",
                "authenticated device requests require X-Loom-Device-Nonce",
            )
        })?;
        // Read workers must not wait behind projection decoding or persistence.
        let registry = runtime.device_registry.try_lock().map_err(|_| {
            request_security_error(
                503,
                "device_registry_unavailable",
                "device registry is busy or unavailable; retry later",
            )
        })?;
        // A preflight must not consume the nonce: routing rechecks revocation/expiry and
        // consumes it exactly once, after the complete body has arrived.
        registry
            .validate_device_session(token, nonce)
            .map_err(|error| device_error(error.status, error.code, &error.message))?;
        return Ok(());
    }
    Err(device_error(401, "unauthorized", "large request bodies require a valid Loom administrator or scoped device session credential"))
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
