//! Per-transport cancellation wakeup and identity isolation. Progress frames never enter the final response channel.
use super::*;
use crate::InvocationControl;
use std::sync::mpsc::{Sender, TrySendError};

pub(super) struct RuntimeResponse {
    sender: Sender<HostResult<CapabilityRuntimeMessage>>,
    control: Option<Arc<InvocationControl>>,
}
impl RuntimeResponse {
    pub(super) fn new(
        sender: Sender<HostResult<CapabilityRuntimeMessage>>,
        control: Option<Arc<InvocationControl>>,
    ) -> Self {
        Self { sender, control }
    }
    pub(super) fn send(&self, result: HostResult<CapabilityRuntimeMessage>) -> bool {
        self.sender.send(result).is_ok()
    }
    pub(super) fn publish(
        &self,
        status: CapabilityRuntimeStatus,
        payload: &Option<serde_json::Value>,
    ) -> bool {
        self.control
            .as_ref()
            .is_none_or(|control| control.publish(status, payload))
    }
}

struct Attachment(Option<Arc<InvocationControl>>);
impl Drop for Attachment {
    fn drop(&mut self) {
        if let Some(control) = &self.0 {
            control.detach();
        }
    }
}

impl RuntimeProcessClient {
    pub(crate) fn call(
        &self,
        message: CapabilityRuntimeMessage,
        timeout: Duration,
    ) -> HostResult<CapabilityRuntimeMessage> {
        self.call_controlled(message, timeout, None)
    }

    pub(crate) fn call_controlled(
        &self,
        message: CapabilityRuntimeMessage,
        timeout: Duration,
        control: Option<Arc<InvocationControl>>,
    ) -> HostResult<CapabilityRuntimeMessage> {
        let request_id = message_request_id(&message)?;
        let reservation = request_id.clone();
        let (sender, receiver) = mpsc::channel();
        if let Some(control) = &control {
            control.attach(sender.clone())?;
        }
        let _attachment = Attachment(control.clone());
        let request = RuntimeRequest {
            request_id,
            message,
            response: RuntimeResponse::new(sender, control.clone()),
        };
        match self.requests.try_send(request) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(CapabilityHostError::Busy),
            Err(TrySendError::Disconnected(_)) => {
                return Err(CapabilityHostError::Unavailable(
                    "runtime request writer stopped".into(),
                ))
            }
        }
        let result = match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(CapabilityHostError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(CapabilityHostError::Unavailable(
                "runtime response reader stopped".into(),
            )),
        };
        // Cancellation wins over a buffered final response until the host commits
        // its validated result. Never rely on inflight registration for wakeup.
        let result = match control.as_ref().map(|control| control.check()) {
            Some(Err(error)) => Err(error),
            _ => result,
        };
        if matches!(
            &result,
            Err(CapabilityHostError::Timeout | CapabilityHostError::Cancelled)
        ) {
            if let Ok(mut retired) = self.retired.lock() {
                retired.insert(reservation.clone());
            }
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&reservation);
            }
            // A writer racing retirement can still be in write_all. Killing the
            // owned process unblocks it; host teardown joins both pipe threads.
            terminate_child(&self.child);
        }
        result
    }
    pub(crate) fn terminate(&self) {
        terminate_child(&self.child);
    }
}
