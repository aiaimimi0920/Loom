const PROJECTION_V2_ROUTES: &[&str] = &[
    "/v1/projections/v2/context",
    "/v1/projections/v2/create",
    "/v1/projections/v2/inspect",
    "/v1/projections/v2/accept",
    "/v1/projections/v2/update",
    "/v1/projections/v2/read",
    "/v1/projections/v2/unlink",
];

fn handle_projection_v2_route(
    path: &str,
    body: &str,
    actor: Option<&str>,
    root: &Path,
    owner: &ProjectionOwner,
) -> Result<(u16, String)> {
    let Some(actor) = actor else {
        return structured_error(403, json!({"code":"projection_pairing_required"}));
    };
    let operation = match parse_projection_v2_operation(path, body) {
        Ok(operation) => operation,
        Err(error) => return structured_error(error.status, json!({"code":error.code})),
    };
    match owner.execute(root, actor, operation) {
        Ok(response) => Ok((200, serde_json::to_string(&response)?)),
        Err(error) => structured_error(error.status, json!({"code":error.code})),
    }
}

fn parse_projection_v2_operation(
    path: &str,
    body: &str,
) -> std::result::Result<loom_projection::LocalOperation, loom_projection::Error> {
    if body.len() > 6 * 1024 * 1024 {
        return Err(loom_projection::Error {
            status: 413,
            code: "projection_request_budget",
        });
    }
    let invalid = || loom_projection::Error {
        status: 400,
        code: "projection_invalid_request",
    };
    let operation: loom_projection::LocalOperation =
        serde_json::from_str(body).map_err(|_| invalid())?;
    if path.strip_prefix("/v1/projections/v2/") != Some(operation.route()) {
        return Err(invalid());
    }
    Ok(operation)
}

#[cfg(test)]
mod projection_v2_route_tests {
    use super::*;

    #[test]
    fn local_routes_reject_central_operations_unknown_fields_and_path_confusion() {
        assert!(parse_projection_v2_operation(
            "/v1/projections/v2/context",
            r#"{"kind":"context"}"#
        )
        .is_ok());
        for (path, body) in [
            (
                "/v1/projections/v2/read",
                r#"{"kind":"unlink","projectionId":"projection:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
            ),
            (
                "/v1/projections/v2/create",
                r#"{"kind":"create","envelope":{},"width":1,"height":1,"byteLength":1}"#,
            ),
            (
                "/v1/projections/v2/context",
                r#"{"kind":"context","endpoint":{}}"#,
            ),
        ] {
            assert_eq!(
                parse_projection_v2_operation(path, body).unwrap_err().code,
                "projection_invalid_request"
            );
        }
    }
}
