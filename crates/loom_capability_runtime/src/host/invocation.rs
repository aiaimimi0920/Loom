use super::admission::InvocationAdmission;
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
        let active = lock(&self.packages)?
            .get(&plugin_id)
            .cloned()
            .ok_or_else(|| CapabilityHostError::Unavailable(plugin_id.clone()))?;
        let mut active = lock(&active)?;
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
        validate_command_input(validators, &invocation.input, &invocation.resource_refs)?;
        if requires_user_gesture {
            self.consume_user_gesture(&invocation)?;
        }
        if active.package.manifest.entrypoints.service.is_none() {
            return Err(CapabilityHostError::Unavailable(
                "plugin has no service runtime".to_owned(),
            ));
        }
        ensure_process(&mut active, &self.limits)?;
        let request_id = next_request_id();
        let message = runtime_request(
            request_id.clone(),
            CapabilityRuntimeMethod::Command,
            json!({
                "commandId": invocation.command_id,
                "input": invocation.input,
                "target": invocation.target,
                "resourceRefs": invocation.resource_refs,
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
        let response = process.call(message, timeout);
        lock(&self.inflight)?.remove(&invocation.request_id);
        // Drop the last request sender before failed runtime teardown joins its writer thread.
        drop(process);
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
        if active
            .package
            .manifest
            .entrypoints
            .service
            .as_ref()
            .is_some_and(|service| service.process_model == CapabilityProcessModel::OnDemand)
        {
            if let Some(mut process) = active.process.take() {
                let _ = call_method(
                    &mut process,
                    CapabilityRuntimeMethod::Deactivate,
                    json!({ "reason": "on_demand_complete" }),
                    Duration::from_secs(2),
                );
            }
        }
        let output = response_output(&active.package, response)?;
        let validators = active.command_schemas.get(&command.id).ok_or_else(|| {
            CapabilityHostError::InvalidPackage("command schema is missing".to_owned())
        })?;
        validate_command_output(
            &active.package,
            &command,
            validators,
            &output,
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

    fn consume_user_gesture(&self, invocation: &CapabilityInvocation) -> HostResult<()> {
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
        if grant.expires_at <= Instant::now()
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
