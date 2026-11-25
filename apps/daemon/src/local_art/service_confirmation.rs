//! 确认状态归原Task；仅一个挑战和决定，不创建另一任务表、线程或授权期限。
use super::*;
use loom_capability_runtime::{CapabilityHostError, CommandScope};
use loom_protocol::local_art::service::confirmation as wire;
use std::sync::Condvar;

pub(crate) struct Gate {
    pub scope: CommandScope,
    runtime: SharedCapabilityRuntime,
    generation: u64,
    challenge: wire::Challenge,
    decision: Mutex<Option<bool>>,
    changed: Condvar,
}
impl Gate {
    pub fn new(
        runtime: &SharedCapabilityRuntime,
        command: &str,
        fingerprint: &str,
        expires_at_ms: u64,
    ) -> LocalResult<Self> {
        let generation = runtime.generation();
        let scope = runtime
            .command_scope(command)
            .map_err(|_| fail(409, "local_art_command_changed"))?;
        if runtime.generation() != generation {
            return Err(fail(409, "local_art_command_changed"));
        }
        let challenge = wire::Challenge {
            nonce: Uuid::new_v4().simple().to_string(),
            fingerprint: fingerprint.into(),
            plugin_id: scope.plugin_id.clone(),
            command_id: scope.command_id.clone(),
            package_digest: scope.package_digest.clone(),
            scope_id: scope.scope_id.clone(),
            expires_at_ms,
        };
        challenge
            .validate()
            .map_err(|_| fail(409, "local_art_command_changed"))?;
        Ok(Self {
            scope,
            runtime: runtime.clone(),
            generation,
            challenge,
            decision: Mutex::new(None),
            changed: Condvar::new(),
        })
    }
    pub fn current(&self) -> bool {
        // 不阻塞初始化持有的package锁；任一贡献换代均保守撤销本次待确认/在途调用。
        self.runtime.generation() == self.generation
    }
    pub fn wake(&self) {
        self.changed.notify_all();
    }
    pub fn denied(&self) -> bool {
        self.decision
            .lock()
            .is_ok_and(|value| *value == Some(false))
    }
    pub fn wait(&self, task: &Task) -> Result<(), CapabilityHostError> {
        if !self.scope.requires_user_gesture {
            return Ok(());
        }
        loop {
            if !self.current()
                || !task.lease.valid()
                || task.deadline <= std::time::Instant::now()
                || task
                    .state
                    .lock()
                    .map_or(true, |state| terminal(state.reply.status))
            {
                return Err(CapabilityHostError::Cancelled);
            }
            let value = self
                .decision
                .lock()
                .map_err(|_| CapabilityHostError::Cancelled)?;
            match *value {
                Some(true) => return Ok(()),
                Some(false) => {
                    return Err(CapabilityHostError::Protocol(
                        "user denied confirmation".into(),
                    ))
                }
                None => {
                    let _ = self
                        .changed
                        .wait_timeout(value, Duration::from_millis(20))
                        .map_err(|_| CapabilityHostError::Cancelled)?;
                }
            }
        }
    }
}
impl Tasks {
    fn confirmed_task(&self, owner: &Owner) -> LocalResult<Arc<Task>> {
        self.state
            .lock()
            .map_err(|_| fail(503, "local_art_tasks_unavailable"))?
            .entries
            .iter()
            .find(|entry| &entry.task.owner == owner)
            .map(|entry| entry.task.clone())
            .ok_or_else(|| fail(404, "local_art_call_not_found"))
    }
    pub(in crate::local_art) fn decide(
        &self,
        owner: &Owner,
        lease: &Lease,
        decision: &wire::Decision,
    ) -> LocalResult<()> {
        let task = self.confirmed_task(owner)?;
        let reply = task.snapshot(lease)?;
        let gate = task
            .gate
            .as_ref()
            .ok_or_else(|| fail(409, "local_art_confirmation_scope"))?;
        if terminal(reply.status)
            || !gate.scope.requires_user_gesture
            || !gate.current()
            || task.deadline <= std::time::Instant::now()
            || gate.challenge != decision.challenge
        {
            return Err(fail(409, "local_art_confirmation_scope"));
        }
        let mut value = gate
            .decision
            .lock()
            .map_err(|_| fail(503, "local_art_task_unavailable"))?;
        if value.is_some_and(|prior| prior != decision.approve) {
            return Err(fail(409, "local_art_confirmation_consumed"));
        }
        *value = Some(decision.approve);
        drop(value);
        gate.wake();
        Ok(())
    }
    pub(in crate::local_art) fn confirmation_output(&self, owner: &Owner) -> LocalResult<Value> {
        let task = self.confirmed_task(owner)?;
        let gate = task
            .gate
            .as_ref()
            .ok_or_else(|| fail(409, "local_art_confirmation_scope"))?;
        task.snapshot(&task.lease)?;
        let state = task
            .state
            .lock()
            .map_err(|_| fail(503, "local_art_task_unavailable"))?;
        let confirmation = if !terminal(state.reply.status)
            && gate.scope.requires_user_gesture
            && gate
                .decision
                .lock()
                .map_err(|_| fail(503, "local_art_task_unavailable"))?
                .is_none()
        {
            Some(gate.challenge.clone())
        } else {
            None
        };
        let response = wire::Response {
            protocol_version: wire::PROTOCOL.into(),
            reply: state.reply.clone(),
            image: state.image.as_ref().map(|value| value.image.clone()),
            confirmation,
        };
        response
            .validate(owner, task.image_output)
            .map_err(|_| fail(409, "local_art_confirmation_unavailable"))?;
        serde_json::to_value(response).map_err(|_| fail(500, "local_art_reply_invalid"))
    }
}
