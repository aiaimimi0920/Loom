use super::admission::InvocationAdmission;
use super::resources::validate_staged_resources;
use super::*;
use crate::schema::{validate_command_input, validate_command_output};
use crate::session::response_output;

impl CapabilityRuntimeHost {
    pub fn invoke(
        &self,
        invocation: CapabilityInvocation,
    ) -> HostResult<CapabilityInvocationOutput> {
        let plugin_id = lock(&self.commands)?
            .get(&invocation.command_id)
            .cloned()
            .ok_or_else(|| CapabilityHostError::NotFound(invocation.command_id.clone()))?;
        let _permit = InvocationAdmission::acquire(&self.admission, &self.limits, &plugin_id)?;
        let active_handle = lock(&self.packages)?
            .get(&plugin_id)
            .cloned()
            .ok_or_else(|| CapabilityHostError::Unavailable(plugin_id.clone()))?;
        let mut active = lock(&active_handle)?;
        let command = active
            .effective_contributions
            .commands
            .iter()
            .find(|command| command.id == invocation.command_id)
            .cloned()
            .ok_or_else(|| CapabilityHostError::NotFound(invocation.command_id.clone()))?;
        let requires_user_gesture = command.requires_user_gesture;
        let validators = active.command_schemas.get(&command.id).ok_or_else(|| {
            CapabilityHostError::InvalidPackage("command schema is missing".to_owned())
        })?;
        validate_command_input(
            &active.package,
            &command,
            validators,
            &invocation.input,
            &invocation.resource_refs,
            &invocation.unit_attachments,
        )?;
        validate_staged_resources(&invocation)?;
        if requires_user_gesture {
            self.consume_user_gesture(&plugin_id, &invocation)?;
        }
        if active.package.manifest.entrypoints.service.is_none() {
            return Err(CapabilityHostError::Unavailable(
                "plugin has no service runtime".to_owned(),
            ));
        }
        ensure_process(&mut active, &self.limits)?;
        let request_id = next_request_id();
        let invocation_resource_refs = invocation.resource_refs.clone();
        let message = runtime_request(
            request_id.clone(),
            CapabilityRuntimeMethod::Command,
            json!({
                "commandId": invocation.command_id,
                "input": invocation.input,
                "target": invocation.target,
                "resourceRefs": invocation.resource_refs,
                "unitAttachments": invocation.unit_attachments,
                "stagedResources": invocation.staged_resources,
                "userGesture": requires_user_gesture,
            }),
        );
        let configured_timeout = command
            .timeout_ms
            .map(Duration::from_millis)
            .unwrap_or_else(|| {
                Duration::from_secs(active.package.manifest.resources.timeout_seconds.max(1))
            });
        let timeout = invocation
            .timeout
            .map(|requested| requested.min(configured_timeout))
            .unwrap_or(configured_timeout);
        let process = active
            .process
            .as_ref()
            .expect("process was ensured")
            .client()?;
        // Snapshot what the response path needs so the per-package lock can be
        // released across the blocking call. The package is pinned to the one
        // the input was validated against.
        let package = Arc::clone(&active.package);
        let on_demand = package
            .manifest
            .entrypoints
            .service
            .as_ref()
            .is_some_and(|service| service.process_model == CapabilityProcessModel::OnDemand);
        {
            let mut inflight = lock(&self.inflight)?;
            if inflight.contains_key(&invocation.request_id) {
                return Err(CapabilityHostError::Busy);
            }
            inflight.insert(
                invocation.request_id.clone(),
                InflightInvocation {
                    plugin_id: plugin_id.clone(),
                    runtime_request_id: request_id,
                    process: process.clone(),
                },
            );
        }
        // Holding the package lock here would stall `health`, `contribution_snapshot`,
        // `prune_idle`, `deactivate` and `cancel` for the full command timeout — up to
        // a minute for OCR. The admission permit plus `max_plugin_inflight` already
        // serialise same-plugin invocations, so the lock is not what protects this call.
        // Stamp the dispatch rather than only the completion: `prune_idle` decides purely on
        // `last_used`, so a plugin that had been idle just short of the timeout would have its
        // runtime reaped out from under a request that had only just started.
        active.last_used = Instant::now();
        drop(active);
        let response = process.call(message, timeout);
        lock(&self.inflight)?.remove(&invocation.request_id);
        // Drop the last request sender before failed runtime teardown joins its writer thread.
        drop(process);
        let mut active = lock(&active_handle)?;
        active.last_used = Instant::now();
        let response = match response {
            Ok(response) => {
                active.failures = 0;
                response
            }
            Err(error) => {
                active.process.take();
                record_failure(&mut active);
                return Err(error);
            }
        };
        if on_demand {
            if let Some(mut process) = active.process.take() {
                let _ = call_method(
                    &mut process,
                    CapabilityRuntimeMethod::Deactivate,
                    json!({ "reason": "on_demand_complete" }),
                    Duration::from_secs(2),
                );
            }
        }
        let output = response_output(&package, response)?;
        let validators = active.command_schemas.get(&command.id).ok_or_else(|| {
            CapabilityHostError::InvalidPackage("command schema is missing".to_owned())
        })?;
        validate_command_output(
            &package,
            &command,
            validators,
            &output,
            &invocation_resource_refs,
            requires_user_gesture,
        )?;
        Ok(output)
    }

    pub fn cancel(&self, plugin_id: &str) -> HostResult<()> {
        self.cancel_plugin_inflight(plugin_id)?;
        let active = lock(&self.packages)?
            .get(plugin_id)
            .cloned()
            .ok_or_else(|| CapabilityHostError::NotFound(plugin_id.to_owned()))?;
        let mut active = lock(&active)?;
        if let Some(mut process) = active.process.take() {
            process.terminate();
        }
        active.last_used = Instant::now();
        Ok(())
    }

    /// Requests cooperative cancellation for one invocation and force-terminates on timeout.
    pub fn cancel_request(&self, request_id: &str) -> HostResult<bool> {
        let Some(inflight) = lock(&self.inflight)?.get(request_id).cloned() else {
            return Ok(false);
        };
        let response = inflight.process.call(
            runtime_request(
                next_request_id(),
                CapabilityRuntimeMethod::Cancel,
                json!({ "requestId": inflight.runtime_request_id }),
            ),
            Duration::from_secs(2),
        );
        match response {
            Ok(_) | Err(CapabilityHostError::Timeout) => Ok(true),
            Err(error) => Err(error),
        }
    }

    fn consume_user_gesture(
        &self,
        plugin_id: &str,
        invocation: &CapabilityInvocation,
    ) -> HostResult<()> {
        let token = invocation.user_gesture_token.as_deref().ok_or_else(|| {
            CapabilityHostError::Protocol("command requires a user gesture token".to_owned())
        })?;
        let target = invocation.target.as_ref().map(|target| UserGestureTarget {
            unit_id: target.unit_id.clone(),
            revision: target.revision,
        });
        let grant = lock(&self.gestures)?.remove(token).ok_or_else(|| {
            CapabilityHostError::Protocol("user gesture token is invalid or consumed".to_owned())
        })?;
        // The grant records the owner the command resolved to when the gesture was issued, so
        // checking it keeps the token bound to that plugin even if command ownership moves
        // between issuing and consuming.
        if grant.expires_at <= Instant::now()
            || grant.plugin_id != plugin_id
            || grant.command_id != invocation.command_id
            || grant.target != target
        {
            return Err(CapabilityHostError::Protocol(
                "user gesture token is expired or bound to another target".to_owned(),
            ));
        }
        Ok(())
    }

    pub(super) fn cancel_plugin_inflight(&self, plugin_id: &str) -> HostResult<()> {
        let processes = lock(&self.inflight)?
            .values()
            .filter(|inflight| inflight.plugin_id == plugin_id)
            .map(|inflight| inflight.process.clone())
            .collect::<Vec<_>>();
        for process in processes {
            process.terminate();
        }
        Ok(())
    }
}
