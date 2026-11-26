// Embedded Web management console routes (`/ui` and `/ui/*`) gated by feature and config.

fn daemon_edition_name() -> &'static str {
    if cfg!(feature = "management-ui") {
        "web"
    } else {
        "api"
    }
}

fn build_management_ui_status(enable_management_ui: bool) -> ManagementUiStatus {
    #[cfg(feature = "management-ui")]
    let prebuilt_bundle = loom_embedded_ui::is_prebuilt_bundle();
    #[cfg(not(feature = "management-ui"))]
    let prebuilt_bundle = false;

    ManagementUiStatus {
        compiled: cfg!(feature = "management-ui"),
        enabled: cfg!(feature = "management-ui") && enable_management_ui,
        prebuilt_bundle,
        mount_path: "/ui/",
    }
}

#[cfg(feature = "management-ui")]
const MANAGEMENT_UI_LOGIN_HTML: &str = concat!(
    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
    "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
    "<title>Loom Management Console Sign In</title>",
    "<style>",
    "body{margin:0;min-height:100vh;display:grid;place-items:center;background:#ebf1f8;",
    "font-family:system-ui,-apple-system,sans-serif;color:#1e293b}",
    ".card{background:#fff;padding:2rem;border-radius:16px;box-shadow:0 12px 32px rgba(15,23,42,.08);",
    "width:min(420px,90vw)}",
    "h1{margin:0 0 .5rem;font-size:1.35rem}p{margin:0 0 1.25rem;color:#475569;font-size:.92rem;line-height:1.5}",
    "input{width:100%;box-sizing:border-box;padding:.7rem .85rem;border:1px solid #cbd5e1;border-radius:10px;",
    "font-size:.95rem;margin-bottom:1rem}",
    "button{width:100%;padding:.75rem;border:0;border-radius:10px;background:#2563eb;color:#fff;",
    "font-weight:600;font-size:.95rem;cursor:pointer}",
    "button:hover{background:#1d4ed8}",
    "</style></head><body><main class=\"card\">",
    "<h1>Loom Web Management Console</h1>",
    "<p>Enter the server <code>daemon-token</code> to establish a secure session (HttpOnly cookie; token is not kept in the URL bar).</p>",
    "<form method=\"get\" action=\"/ui/\">",
    "<input type=\"password\" name=\"token\" placeholder=\"Paste daemon-token\" autocomplete=\"current-password\" required>",
    "<button type=\"submit\">Open Console</button>",
    "</form></main></body></html>"
);

fn route_management_ui(runtime: &DaemonRuntime, request: &ParsedHttpRequest) -> Option<RouteResponse> {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    if route_path != "/ui" && !route_path.starts_with("/ui/") {
        return None;
    }
    if !matches!(request.method.as_str(), "GET" | "HEAD") {
        let (status, body) = request_security_error(
            405,
            "method_not_allowed",
            "management UI only accepts GET or HEAD requests",
        );
        return Some(RouteResponse::Text { status, body });
    }
    #[cfg(not(feature = "management-ui"))]
    {
        let _ = runtime;
        let (status, body) = request_security_error(
            404,
            "management_ui_not_compiled",
            "this headless Loom api edition binary was compiled without management-ui",
        );
        return Some(RouteResponse::Text { status, body });
    }
    #[cfg(feature = "management-ui")]
    {
        if !runtime.enable_management_ui {
            let (status, body) = request_security_error(
                404,
                "management_ui_disabled",
                "Loom Web management UI is disabled by configuration",
            );
            return Some(RouteResponse::Text { status, body });
        }
        if request.query_parameter("token").as_deref() == Some(runtime.auth_token.as_str()) {
            return Some(RouteResponse::TextWithHeaders {
                status: 303,
                headers: vec![
                    ("Location".to_owned(), "/ui/".to_owned()),
                    (
                        "Set-Cookie".to_owned(),
                        format!(
                            "{ADMIN_AUTH_COOKIE_NAME}={}; HttpOnly; SameSite=Strict; Path=/",
                            percent_encode_query_value(&runtime.auth_token)
                        ),
                    ),
                    ("Cache-Control".to_owned(), "no-store".to_owned()),
                    ("Referrer-Policy".to_owned(), "no-referrer".to_owned()),
                ],
                body: String::new(),
            });
        }
        if route_path == "/ui" {
            return Some(RouteResponse::TextWithHeaders {
                status: 303,
                headers: vec![("Location".to_owned(), "/ui/".to_owned())],
                body: String::new(),
            });
        }

        let subpath = route_path.strip_prefix("/ui/").unwrap_or("");
        if !request.has_admin_credential(&runtime.auth_token) {
            let last_segment = subpath.rsplit('/').next().unwrap_or(subpath);
            if subpath.is_empty() || !last_segment.contains('.') {
                return Some(RouteResponse::Binary {
                    status: 200,
                    content_type: "text/html; charset=utf-8".to_owned(),
                    body: if request.method == "HEAD" {
                        Vec::new()
                    } else {
                        MANAGEMENT_UI_LOGIN_HTML.as_bytes().to_vec()
                    },
                });
            }
            let (status, body) = request_security_error(
                401,
                "unauthorized",
                "missing or invalid Loom administrator session for management UI asset",
            );
            return Some(RouteResponse::Text { status, body });
        }

        match loom_embedded_ui::resolve_ui_asset(subpath) {
            Some(asset) => Some(RouteResponse::Binary {
                status: 200,
                content_type: asset.content_type.to_owned(),
                body: if request.method == "HEAD" {
                    Vec::new()
                } else {
                    asset.data.to_vec()
                },
            }),
            None => {
                let (status, body) = request_security_error(
                    404,
                    "ui_asset_not_found",
                    "requested management UI asset does not exist",
                );
                Some(RouteResponse::Text { status, body })
            }
        }
    }
}
