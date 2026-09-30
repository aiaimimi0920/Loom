// Maps workflow-layer failures to the daemon's stable HTTP error envelope.
fn workflow_runtime_error_response(error: WorkflowRuntimeError) -> Result<(u16, String)> {
    structured_error(
        500,
        json!({
            "code": "workflow_runtime_error",
            "message": error.to_string(),
        }),
    )
}

fn workflow_store_error_response(error: WorkflowStoreError) -> Result<(u16, String)> {
    match error {
        WorkflowStoreError::InvalidWorkflowId(id) => structured_error(
            400,
            json!({
                "code": "invalid_workflow_id",
                "message": format!("invalid workflow id `{id}`"),
                "workflow_id": id,
            }),
        ),
        WorkflowStoreError::InvalidWorkflowYaml(message) => structured_error(
            400,
            json!({
                "code": "invalid_workflow",
                "message": message,
            }),
        ),
        WorkflowStoreError::InvalidWorkflowGraph(message) => structured_error(
            400,
            json!({
                "code": "invalid_workflow_graph",
                "message": message,
            }),
        ),
        WorkflowStoreError::NotFound(id) => structured_error(
            404,
            json!({
                "code": "workflow_not_found",
                "message": format!("workflow `{id}` was not found"),
                "workflow_id": id,
            }),
        ),
        WorkflowStoreError::Io(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
        WorkflowStoreError::Json(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
        WorkflowStoreError::Yaml(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
    }
}
