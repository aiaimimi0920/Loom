// HTTP routes for the global AI Gateway relay (/v1/ai/*).
fn route_ai_gateway(
    request: &ParsedHttpRequest,
    route_path: &str,
    admin_authenticated: bool,
    ai_gateway: &SharedAiGateway,
    control_plane_root: &Path,
) -> Option<Result<(u16, String)>> {
    if !route_path.starts_with("/v1/ai/") {
        return None;
    }
    Some(match (request.method.as_str(), route_path) {
        ("GET", "/v1/ai/status") => Ok((
            200,
            serde_json::to_string(&ai_gateway.status()).unwrap_or_else(|_| "{}".to_owned()),
        )),
        ("GET", "/v1/ai/config") => {
            if !admin_authenticated {
                return Some(structured_error(
                    403,
                    json!({
                        "code": "ai_gateway_admin_required",
                        "message": "AI Gateway configuration requires Loom administrator credentials",
                    }),
                ));
            }
            serde_json::to_string(&ai_gateway.status())
                .map(|body| (200, body))
                .map_err(Into::into)
        }
        ("PUT", "/v1/ai/config") => {
            if !admin_authenticated {
                return Some(structured_error(
                    403,
                    json!({
                        "code": "ai_gateway_admin_required",
                        "message": "AI Gateway configuration requires Loom administrator credentials",
                    }),
                ));
            }
            let update: AiGatewayConfigUpdate = match serde_json::from_str(&request.body) {
                Ok(update) => update,
                Err(error) => {
                    return Some(structured_error(
                        400,
                        json!({
                            "code": "invalid_request",
                            "message": format!("invalid AI Gateway configuration payload: {error}"),
                        }),
                    ))
                }
            };
            match ai_gateway.update_config(update, control_plane_root) {
                Ok(status) => serde_json::to_string(&status)
                    .map(|body| (200, body))
                    .map_err(Into::into),
                Err(error) => ai_gateway_error_response(error),
            }
        }
        ("GET", "/v1/ai/models") => match ai_gateway.list_models() {
            Ok(models) => serde_json::to_string(&models)
                .map(|body| (200, body))
                .map_err(Into::into),
            Err(error) => ai_gateway_error_response(error),
        },
        ("POST", "/v1/ai/chat/completions") => {
            let payload: Value = match serde_json::from_str(&request.body) {
                Ok(payload) => payload,
                Err(_) => return Some(bad_request("invalid AI chat completion request JSON")),
            };
            match ai_gateway.chat_completion(payload) {
                Ok(outcome) => serde_json::to_string(&outcome.response)
                    .map(|body| (200, body))
                    .map_err(Into::into),
                Err(error) => ai_gateway_error_response(error),
            }
        }
        _ => structured_error(
            404,
            json!({
                "code": "ai_gateway_route_not_found",
                "message": "unknown /v1/ai endpoint",
            }),
        ),
    })
}

fn ai_gateway_error_response(error: AiGatewayRelayError) -> Result<(u16, String)> {
    match error {
        AiGatewayRelayError::NotConfigured => structured_error(
            503,
            json!({
                "code": "ai_gateway_not_configured",
                "message": error.to_string(),
                "retryable": false,
            }),
        ),
        AiGatewayRelayError::InvalidRequest(message) => structured_error(
            400,
            json!({
                "code": "invalid_ai_request",
                "message": message,
            }),
        ),
        AiGatewayRelayError::InvalidConfig(message) => structured_error(
            400,
            json!({
                "code": "invalid_ai_gateway_config",
                "message": message,
            }),
        ),
        AiGatewayRelayError::Storage(message) => structured_error(
            500,
            json!({
                "code": "ai_gateway_storage_failed",
                "message": truncate_diagnostic(message, 256),
            }),
        ),
        AiGatewayRelayError::Upstream(upstream) => structured_error(
            502,
            json!({
                "code": "ai_gateway_upstream_failed",
                "message": "AI Gateway request failed",
                "diagnostic": truncate_diagnostic(upstream.to_string(), 512),
                "retryable": true,
            }),
        ),
    }
}
