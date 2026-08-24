// Terminal acknowledgement, failure persistence, and bridge broadcast helpers.
fn request_id_for_event(event_id: &str) -> String {
    format!(
        "request:{}",
        event_id.strip_prefix("event:").unwrap_or(event_id)
    )
}

fn finish_cancelled(
    job: &SurfaceActionJob,
    surface_instances: &SharedSurfaceInstanceStore,
    hook_bridge: &SharedHookBridgeRuntime,
) {
    let mut cancelled = job.ack.clone();
    cancelled.status = SurfaceActionStatus::Cancelled;
    persist_ack(surface_instances, &cancelled, true);
    broadcast_ack(hook_bridge, &cancelled);
}

/// Records a failure and persists the `Failed` ack for it.
///
/// Shared by the normal error path and by `SurfaceActionJobGuard::drop`, so a panic produces the
/// same three observable effects as a returned error: a recorded failure on the instance, a terminal
/// ack, and a failure broadcast.
fn finish_failed(
    job: &SurfaceActionJob,
    error: SurfaceExecutionError,
    surface_instances: &SharedSurfaceInstanceStore,
    hook_bridge: &SharedHookBridgeRuntime,
) {
    let failure = SurfaceExecutionFailure {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: job.event.instance_id.clone(),
        request_id: job.ack.request_id.clone(),
        generation: job.event.generation,
        error: error.clone(),
        last_successful_result_revision: None,
    };
    if let Ok(mut store) = surface_instances.lock() {
        let _ = store.record_failure(&job.event.instance_id, failure.clone());
    }
    let mut failed = job.ack.clone();
    failed.status = SurfaceActionStatus::Failed;
    failed.error = Some(error);
    persist_ack(surface_instances, &failed, true);
    broadcast_ack(hook_bridge, &failed);
    broadcast_hook_bridge_json(
        hook_bridge,
        json!({
            "method": SURFACE_EVENT_FAILURE,
            "params": {
                "hookNodeId": hook_node_id(surface_instances, &job.event.instance_id, &job.event.attachment_id),
                "failure": failure,
            }
        }),
    );
}

fn persist_ack(
    surface_instances: &SharedSurfaceInstanceStore,
    ack: &SurfaceActionAck,
    remove_pending: bool,
) {
    if let Ok(mut store) = surface_instances.lock() {
        let _ = store.update_event_ack(ack.clone(), remove_pending);
    }
}

fn broadcast_ack(hook_bridge: &SharedHookBridgeRuntime, ack: &SurfaceActionAck) {
    broadcast_hook_bridge_json(
        hook_bridge,
        json!({ "method": SURFACE_EVENT_ACTION_ACK, "params": ack }),
    );
}

fn broadcast_progress(
    hook_bridge: &SharedHookBridgeRuntime,
    job: &SurfaceActionJob,
    value: Option<f64>,
    stage: &str,
) {
    if !job.action.progress {
        return;
    }
    let progress = SurfaceActionProgress {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: job.event.instance_id.clone(),
        request_id: job.ack.request_id.clone(),
        generation: job.event.generation,
        value,
        stage: Some(stage.to_owned()),
        message_key: None,
    };
    broadcast_hook_bridge_json(
        hook_bridge,
        json!({ "method": SURFACE_EVENT_ACTION_PROGRESS, "params": progress }),
    );
}

fn hook_node_id(
    surface_instances: &SharedSurfaceInstanceStore,
    instance_id: &str,
    attachment_id: &str,
) -> Option<String> {
    surface_instances
        .lock()
        .ok()
        .and_then(|store| store.get(instance_id))
        .and_then(|instance| {
            instance
                .attachments
                .get(attachment_id)
                .map(|attachment| attachment.descriptor.hook_node_id.clone())
        })
}

fn store_execution_error(error: SurfaceStoreError) -> SurfaceExecutionError {
    SurfaceExecutionError {
        code: error.code().to_owned(),
        message: error.to_string(),
        detail: None,
    }
}

fn resource_execution_error(error: SurfaceResourceStoreError) -> SurfaceExecutionError {
    SurfaceExecutionError {
        code: error.code().to_owned(),
        message: error.to_string(),
        detail: None,
    }
}

fn execution_error(code: impl Into<String>, message: impl Into<String>) -> SurfaceExecutionError {
    SurfaceExecutionError {
        code: code.into(),
        message: message.into(),
        detail: None,
    }
}
