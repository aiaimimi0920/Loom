//! The underlying runtime enforces schema and sensitive-gesture gates; this layer forwards only JSON results without applying workspace effects.
use super::*;
use loom_capability_runtime::{CapabilityHostError, CapabilityInvocation};
use loom_protocol::{local_art::service::Status, CapabilityRuntimeStatus};
use service_tasks::Task;

pub(super) fn run(
    task: &Task,
    runtime: SharedCapabilityRuntime,
    command: String,
    input: Value,
    images: Vec<loom_protocol::local_art::service::images::Image>,
    image_context: service_images::Context,
) {
    if !task.lease.valid() {
        task.cancel(Some("local_art_authorization_changed"));
        return;
    }
    // Supervisor's wait covers initialization as well as command execution. On
    // expiry it cancels, then joins cleanup before publishing a terminal result.
    let (sender, receiver) = mpsc::sync_channel(1);
    let result = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let result = (|| {
                if let Some(gate) = &task.gate {
                    gate.wait(task)?;
                }
                // Staging also counts against the supervisor deadline; leases drop once the runtime drains rather than lingering in the terminal task table.
                let prepared = if images.is_empty() {
                    None
                } else {
                    Some(service_images::prepare(
                        &runtime,
                        &command,
                        images,
                        image_context,
                    )?)
                };
                if !task.lease.valid() {
                    return Err(CapabilityHostError::Cancelled);
                }
                let invocation = CapabilityInvocation {
                    request_id: format!("local-art:{}", Uuid::new_v4()),
                    command_id: command,
                    input,
                    target: None,
                    resource_refs: prepared
                        .as_ref()
                        .map(|p| p.references.clone())
                        .unwrap_or_default(),
                    unit_attachments: vec![],
                    staged_resources: prepared
                        .as_ref()
                        .map(|p| p.staged.resources().to_vec())
                        .unwrap_or_default(),
                    user_gesture_token: None,
                    timeout: Some(
                        task.deadline
                            .saturating_duration_since(std::time::Instant::now()),
                    ),
                };
                match &task.gate {
                    Some(gate) => runtime.invoke_confirmed_controlled(
                        invocation,
                        task.control.clone(),
                        &gate.scope,
                    ),
                    None => runtime.invoke_controlled(invocation, task.control.clone()),
                }
            })();
            let _ = sender.send(result);
        });
        let result = loop {
            if !task.lease.valid() || task.gate.as_ref().is_some_and(|gate| !gate.current()) {
                task.control.cancel();
                if let Some(gate) = &task.gate {
                    gate.wake();
                }
                break Err(CapabilityHostError::Cancelled);
            }
            let remaining = task
                .deadline
                .saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                task.control.cancel();
                if let Some(gate) = &task.gate {
                    gate.wake();
                }
                break Err(CapabilityHostError::Timeout);
            }
            break match receiver.recv_timeout(remaining.min(Duration::from_millis(20))) {
                Ok(result) => result,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => Err(CapabilityHostError::Unavailable("worker stopped".into())),
            };
        };
        if worker.join().is_err() {
            return Err(CapabilityHostError::Unavailable("worker stopped".into()));
        }
        result
    });
    if !task.lease.valid() {
        task.finish(
            Status::Failed,
            None,
            Some("local_art_authorization_changed"),
        );
        return;
    }
    if task.deadline <= std::time::Instant::now() {
        task.finish(Status::Failed, None, Some("local_art_service_timeout"));
        return;
    }
    if let Some(gate) = &task.gate {
        if !gate.current() {
            task.finish(Status::Failed, None, Some("local_art_command_changed"));
            return;
        }
        if gate.denied() {
            task.finish(Status::Failed, None, Some("local_art_confirmation_denied"));
            return;
        }
    }
    match result {
        Ok(output) if output.status == CapabilityRuntimeStatus::Succeeded => {
            let mut payload = output.payload.unwrap_or(Value::Null);
            if payload
                .get("effects")
                .is_some_and(|effects| effects.as_array().is_none_or(|values| !values.is_empty()))
            {
                task.finish(Status::Failed, None, Some("local_art_effects_require_host"));
            } else {
                let output = payload
                    .as_object_mut()
                    .and_then(|object| object.remove("output"))
                    .unwrap_or(payload);
                if task.image_output {
                    match service_image_output::prepare(output, &task.image_bytes) {
                        Ok((metadata, image)) if task.lease.valid() => task.finish_with_image(
                            Status::Succeeded,
                            Some(metadata),
                            None,
                            Some(image),
                        ),
                        Ok(_) => task.cancel(Some("local_art_authorization_changed")),
                        Err(error) => task.finish(Status::Failed, None, Some(error)),
                    }
                } else {
                    task.finish(Status::Succeeded, Some(output), None);
                }
            }
        }
        Ok(output) if output.status == CapabilityRuntimeStatus::Cancelled => {
            task.finish(Status::Cancelled, None, None)
        }
        Err(CapabilityHostError::Cancelled) => task.finish(Status::Cancelled, None, None),
        Err(CapabilityHostError::Timeout) => {
            task.finish(Status::Failed, None, Some("local_art_service_timeout"))
        }
        Err(CapabilityHostError::Busy) => {
            task.finish(Status::Failed, None, Some("local_art_service_busy"))
        }
        _ => task.finish(Status::Failed, None, Some("local_art_service_failed")),
    }
}
