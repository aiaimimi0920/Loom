// Route concurrency classification (`Concurrent` vs `Serialized` under `serialized_route_lock`).

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestConcurrencyClass {
    Concurrent,
    Serialized,
}

fn request_concurrency_class(request: &ParsedHttpRequest) -> RequestConcurrencyClass {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    match (request.method.as_str(), route_path) {
        ("GET", "/health" | "/status" | "/v1/capabilities") => RequestConcurrencyClass::Concurrent,
        ("GET" | "HEAD", "/ui") => RequestConcurrencyClass::Concurrent,
        ("GET" | "HEAD", path) if path.starts_with("/ui/") => RequestConcurrencyClass::Concurrent,
        ("GET", "/v1/ai/status" | "/v1/ai/models") => RequestConcurrencyClass::Concurrent,
        ("GET", "/v1/store/catalog" | "/v1/otter/relay/pending") => {
            RequestConcurrencyClass::Concurrent
        }
        ("POST", "/v1/otter/relay/register" | "/v1/otter/relay/results") => {
            RequestConcurrencyClass::Concurrent
        }
        ("POST", path) if tool_execute_path_id(path).is_some() => {
            RequestConcurrencyClass::Concurrent
        }
        ("GET", "/v1/mcp/registry") => RequestConcurrencyClass::Concurrent,
        ("GET", "/v1/hook-bridge/canvas") => RequestConcurrencyClass::Concurrent,
        ("GET", "/v1/surfaces/stream") => RequestConcurrencyClass::Concurrent,
        ("GET", path) if live_session_suffix_id(path, "/events").is_some() => {
            RequestConcurrencyClass::Concurrent
        }
        ("GET", path) if otter_canvas_preview_node_id("GET", path).is_some() => {
            RequestConcurrencyClass::Concurrent
        }
        ("GET", path) if run_path_id(path).is_some() || run_events_path_id(path).is_some() => {
            RequestConcurrencyClass::Concurrent
        }
        ("POST", "/v1/ai/chat/completions") => RequestConcurrencyClass::Concurrent,
        ("POST", "/v1/asset-library/download" | "/v1/asset-library/download-control") => {
            RequestConcurrencyClass::Concurrent
        }
        ("POST", path)
            if run_action_path_id(path, "stop").is_some()
                || run_action_path_id(path, "retry").is_some() =>
        {
            RequestConcurrencyClass::Concurrent
        }
        ("POST", "/v1/invoke/cancel") => RequestConcurrencyClass::Concurrent,
        ("POST", path) if PROJECTION_V2_ROUTES.contains(&path) => {
            RequestConcurrencyClass::Concurrent
        }
        ("POST", "/v1/invoke") => {
            let valid_capability = serde_json::from_str::<Value>(&request.body)
                .ok()
                .and_then(|body| body.get("capability").cloned())
                .and_then(|capability| capability.as_str().map(str::to_owned))
                .is_some_and(|capability| !capability.trim().is_empty());
            if valid_capability {
                RequestConcurrencyClass::Concurrent
            } else {
                RequestConcurrencyClass::Serialized
            }
        }
        _ => RequestConcurrencyClass::Serialized,
    }
}
