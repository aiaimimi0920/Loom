// HTTP origin and host policy, settings exchange, route response dispatch, and observers.
fn is_reserved_probe(request: &ParsedHttpRequest) -> bool {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    request.method == "GET" && matches!(route_path, "/health" | "/status")
}

enum RouteResponse {
    Text {
        status: u16,
        body: String,
    },
    TextWithHeaders {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    },
    Binary {
        status: u16,
        content_type: String,
        body: Vec<u8>,
    },
}

fn loopback_host_from_authority(authority: &str) -> Option<&str> {
    let authority = authority.trim();
    if authority.is_empty()
        || authority
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return None;
    }
    if let Some(bracketed) = authority.strip_prefix('[') {
        let end = bracketed.find(']')?;
        let host = &bracketed[..end];
        let remainder = &bracketed[end + 1..];
        if !remainder.is_empty()
            && (!remainder.starts_with(':')
                || remainder[1..].is_empty()
                || !remainder[1..].bytes().all(|byte| byte.is_ascii_digit()))
        {
            return None;
        }
        return host
            .parse::<IpAddr>()
            .ok()
            .filter(IpAddr::is_loopback)
            .map(|_| host);
    }
    if authority.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()) {
        return Some(authority);
    }
    let host = match authority.rsplit_once(':') {
        Some((host, port))
            if !host.is_empty()
                && !port.is_empty()
                && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            host
        }
        Some(_) => return None,
        None => authority,
    };
    (host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()))
    .then_some(host)
}

fn origin_matches_host(origin: &str, host: &str) -> bool {
    let Some(authority) = origin
        .trim()
        .strip_prefix("http://")
        .or_else(|| origin.trim().strip_prefix("https://"))
    else {
        return false;
    };
    if authority.contains('/') || authority.contains('?') || authority.contains('#') {
        return false;
    }
    loopback_host_from_authority(authority).is_some() && authority.eq_ignore_ascii_case(host.trim())
}

fn request_requires_json_content_type(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH")
}

fn content_type_is_json(value: &str) -> bool {
    let media_type = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    media_type == "application/json" || media_type.ends_with("+json")
}

fn request_security_error(status: u16, code: &str, message: &str) -> (u16, String) {
    structured_error(status, json!({ "code": code, "message": message }))
        .expect("serialize request security error")
}

fn enforce_request_security(request: &ParsedHttpRequest) -> std::result::Result<(), (u16, String)> {
    if request.header_count("host") != 1 {
        return Err(request_security_error(
            400,
            "invalid_host",
            "exactly one loopback Host header is required",
        ));
    }
    let host = request.header("host").unwrap_or_default();
    if loopback_host_from_authority(host).is_none() {
        return Err(request_security_error(
            400,
            "invalid_host",
            "Loom daemon requests require a loopback or localhost Host header",
        ));
    }
    if request.header_count("origin") > 1 {
        return Err(request_security_error(
            403,
            "origin_denied",
            "multiple Origin headers are not allowed",
        ));
    }
    if let Some(origin) = request.header("origin") {
        if !origin_matches_host(origin, host) {
            return Err(request_security_error(
                403,
                "origin_denied",
                "request Origin must match the Loom daemon loopback origin",
            ));
        }
    }
    if request.header_count("sec-fetch-site") > 1 {
        return Err(request_security_error(
            403,
            "browser_context_denied",
            "multiple Sec-Fetch-Site headers are not allowed",
        ));
    }
    if request.header("sec-fetch-site").is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "same-origin" | "none"
        )
    }) {
        return Err(request_security_error(
            403,
            "browser_context_denied",
            "cross-origin browser requests are not allowed",
        ));
    }
    if request_requires_json_content_type(&request.method) {
        if request.header_count("content-type") != 1
            || !request
                .header("content-type")
                .is_some_and(content_type_is_json)
        {
            return Err(request_security_error(
                415,
                "json_content_type_required",
                "state-changing Loom requests require Content-Type: application/json",
            ));
        }
    }
    Ok(())
}

fn settings_token_exchange(request: &ParsedHttpRequest, auth_token: &str) -> Option<RouteResponse> {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    if request.method != "GET"
        || !(route_path == "/settings"
            || route_path
                .strip_prefix("/settings/")
                .is_some_and(|app| !app.is_empty() && !app.contains('/')))
        || request.query_parameter("token").as_deref() != Some(auth_token)
    {
        return None;
    }
    Some(RouteResponse::TextWithHeaders {
        status: 303,
        headers: vec![
            ("Location".to_owned(), route_path.to_owned()),
            (
                "Set-Cookie".to_owned(),
                format!(
                    "{ADMIN_AUTH_COOKIE_NAME}={}; HttpOnly; SameSite=Strict; Path=/",
                    percent_encode_query_value(auth_token)
                ),
            ),
            ("Cache-Control".to_owned(), "no-store".to_owned()),
            ("Referrer-Policy".to_owned(), "no-referrer".to_owned()),
        ],
        body: String::new(),
    })
}

fn route_request(runtime: &DaemonRuntime, request: &ParsedHttpRequest) -> RouteResponse {
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Err((status, body)) = enforce_request_security(request) {
            return Ok(RouteResponse::Text { status, body });
        }
        if let Some(response) = settings_token_exchange(request, &runtime.auth_token) {
            return Ok(response);
        }
        if let Some(digest) = surface_resource_request_digest(&request.method, &request.path) {
            if !request.has_admin_credential(&runtime.auth_token) {
                match authenticate_http_device_session(request, &runtime.device_registry) {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        return structured_error(
                            401,
                            json!({
                                "code": "unauthorized",
                                "message": "missing or invalid Loom administrator or device session credential",
                            }),
                        )
                        .map(|(status, body)| RouteResponse::Text { status, body });
                    }
                    Err(error) => {
                        return device_auth_error_response(error)
                            .map(|(status, body)| RouteResponse::Text { status, body });
                    }
                }
            }
            let lease_id = request.header("x-loom-surface-lease").unwrap_or_default();
            return surface_resource_binary_response(digest, lease_id, &runtime.surface_resources);
        }
        if let Some((workflow_id, node_id)) =
            canvas_workflow_preview_ids(&request.method, &request.path)
        {
            if !request.has_admin_credential(&runtime.auth_token) {
                return structured_error(
                    401,
                    json!({
                        "code": "unauthorized",
                        "message": "missing or invalid Loom bearer token",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            }
            return canvas_workflow_preview_response(
                &workflow_id,
                &node_id,
                &runtime.canvas_workflow_root,
            );
        }
        if let Some(node_id) = hook_canvas_preview_node_id(&request.method, &request.path) {
            if !request.has_admin_credential(&runtime.auth_token) {
                return structured_error(
                    401,
                    json!({
                        "code": "unauthorized",
                        "message": "missing or invalid Loom bearer token",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            }
            return hook_canvas_preview_response(&node_id);
        }
        route_with_runtime(runtime, request)
            .map(|(status, body)| RouteResponse::Text { status, body })
    }));
    match response {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            eprintln!("loom request routing failed: {error:#}");
            let (status, body) = request_worker_failed_response();
            RouteResponse::Text { status, body }
        }
        Err(_) => {
            eprintln!("loom request worker panicked");
            let (status, body) = request_worker_failed_response();
            RouteResponse::Text { status, body }
        }
    }
}

fn write_response_safely(mut stream: TcpStream, status: u16, body: &str) {
    if let Err(error) = write_response(&mut stream, status, body) {
        eprintln!("loom response write failed: {error:#}");
    }
}

fn write_route_response_safely(mut stream: TcpStream, response: RouteResponse) {
    let result = match response {
        RouteResponse::Text { status, body } => write_response(&mut stream, status, &body),
        RouteResponse::TextWithHeaders {
            status,
            headers,
            body,
        } => write_response_with_headers(&mut stream, status, &body, &headers),
        RouteResponse::Binary {
            status,
            content_type,
            body,
        } => write_binary_response(&mut stream, status, &content_type, &body),
    };
    if let Err(error) = result {
        eprintln!("loom response write failed: {error:#}");
    }
}

fn handle_parsed_request(stream: TcpStream, request: ParsedHttpRequest, runtime: &DaemonRuntime) {
    write_route_response_safely(stream, route_request(runtime, &request));
}

fn handle_request_job(job: RequestJob, runtime: &DaemonRuntime) {
    let RequestJob { stream, request } = job;
    let response = match request_concurrency_class(&request) {
        RequestConcurrencyClass::Concurrent => route_request(runtime, &request),
        RequestConcurrencyClass::Serialized => {
            let route_guard = match runtime.serialized_route_lock.lock() {
                Ok(route_guard) => route_guard,
                Err(_) => {
                    eprintln!("loom serialized route lock is poisoned");
                    let (status, body) = request_worker_failed_response();
                    write_response_safely(stream, status, &body);
                    return;
                }
            };
            let observer_guard = serialized_route_observer_guard(runtime);
            let response = route_request(runtime, &request);
            drop(observer_guard);
            drop(route_guard);
            response
        }
    };
    write_route_response_safely(stream, response);
}

fn record_shutdown_observed(runtime: &DaemonRuntime) {
    #[cfg(test)]
    if let Some(observer) = runtime.shutdown_observer.as_ref() {
        observer.record();
    }
    #[cfg(not(test))]
    let _ = runtime;
}

fn record_connection_accepted(runtime: &DaemonRuntime) {
    #[cfg(test)]
    if let Some(observer) = runtime.connection_accept_observer.as_ref() {
        observer.record();
    }
    #[cfg(not(test))]
    let _ = runtime;
}

#[cfg(test)]
fn serialized_route_observer_guard(
    runtime: &DaemonRuntime,
) -> Option<SerializedRouteObserverGuard> {
    runtime
        .serialized_route_observer
        .as_ref()
        .map(SerializedRouteObserver::enter)
}

#[cfg(not(test))]
fn serialized_route_observer_guard(
    _runtime: &DaemonRuntime,
) -> Option<SerializedRouteObserverGuard> {
    None
}

#[cfg(test)]
fn record_request_submission(runtime: &DaemonRuntime) {
    if let Some(observer) = runtime.request_submission_observer.as_ref() {
        observer.record();
    }
}

#[cfg(not(test))]
fn record_request_submission(_runtime: &DaemonRuntime) {}

fn write_local_capability_manifest(
    manifest_dir: &Path,
    address: SocketAddr,
    auth_token: Option<&str>,
) -> Result<()> {
    fs::create_dir_all(manifest_dir)
        .with_context(|| format!("create loom manifest dir {}", manifest_dir.display()))?;
    restrict_sensitive_path_permissions(manifest_dir, true).with_context(|| {
        format!(
            "restrict loom manifest directory permissions {}",
            manifest_dir.display()
        )
    })?;
    let mut transport = json!({
        "type": "http",
        "baseUrl": format!("http://{}", address),
        "auth": "none"
    });
    if let Some(token) = auth_token {
        transport["auth"] = Value::String("bearer".to_owned());
        transport["authToken"] = Value::String(token.to_owned());
    }
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("read system time for loom manifest")?
        .as_secs();
    let manifest = json!({
        "schemaVersion": 1,
        "appId": "loom",
        "displayName": "Loom",
        "version": loom_core::LOOM_VERSION,
        "pid": std::process::id(),
        "transport": transport,
        "capabilities": invokable_capability_ids(),
        "startedAt": started_at
    });
    let path = manifest_dir.join("loom.json");
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    let (temporary, mut file) = create_sensitive_temporary(&path).with_context(|| {
        format!(
            "create loom manifest temporary in {}",
            manifest_dir.display()
        )
    })?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)
            .with_context(|| format!("write loom manifest temporary {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("flush loom manifest temporary {}", temporary.display()))?;
        drop(file);
        restrict_sensitive_path_permissions(&temporary, false).with_context(|| {
            format!(
                "restrict loom manifest temporary permissions {}",
                temporary.display()
            )
        })?;
        if path.is_file() {
            restrict_sensitive_path_permissions(&path, false).with_context(|| {
                format!(
                    "refresh loom manifest permissions before replacement {}",
                    path.display()
                )
            })?;
        }
        replace_sensitive_file(&temporary, &path)
            .with_context(|| format!("atomically replace loom manifest {}", path.display()))?;
        restrict_sensitive_path_permissions(&path, false)
            .with_context(|| format!("restrict loom manifest permissions {}", path.display()))?;
        sync_sensitive_parent(&path)
            .with_context(|| format!("flush loom manifest directory {}", manifest_dir.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(())
}
