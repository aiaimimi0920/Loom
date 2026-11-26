//! Per-invocation cancellation and latest-progress slot; executes no user callbacks, avoids polling, and never builds unbounded progress queues.
use crate::error::{CapabilityHostError as Error, HostResult};
use loom_protocol::{CapabilityRuntimeMessage, CapabilityRuntimeStatus};
use serde_json::Value;
use std::sync::{mpsc::Sender, Mutex};

#[derive(Clone, Debug)]
pub struct InvocationProgress {
    pub sequence: u64,
    pub status: CapabilityRuntimeStatus,
    pub payload: Option<Value>,
}
#[derive(Default)]
struct State {
    claimed: bool,
    cancelled: bool,
    finished: bool,
    response: Option<Sender<HostResult<CapabilityRuntimeMessage>>>,
    progress: Option<InvocationProgress>,
    sequence: u64,
}
#[derive(Default)]
pub struct InvocationControl {
    state: Mutex<State>,
}
pub(crate) struct InvocationGuard<'a>(&'a InvocationControl);
impl Drop for InvocationGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.finished = true;
            state.response = None;
        }
    }
}
impl InvocationControl {
    pub fn new() -> Self {
        Self::default()
    }
    /// May be cancelled before invocation registration; cancellation after a terminal state is a no-op and never alters an already delivered success.
    pub fn cancel(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if state.finished || state.cancelled {
            return false;
        }
        state.cancelled = true;
        if let Some(sender) = &state.response {
            let _ = sender.send(Err(Error::Cancelled));
        }
        true
    }
    pub fn progress(&self) -> Option<InvocationProgress> {
        self.state.lock().ok()?.progress.clone()
    }
    pub(crate) fn begin(&self) -> HostResult<InvocationGuard<'_>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Unavailable("control poisoned".into()))?;
        if state.claimed {
            return Err(Error::Protocol("invocation control already used".into()));
        }
        state.claimed = true;
        if state.cancelled {
            state.finished = true;
            return Err(Error::Cancelled);
        }
        Ok(InvocationGuard(self))
    }
    pub(crate) fn check(&self) -> HostResult<()> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::Unavailable("control poisoned".into()))?;
        if state.cancelled {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
    pub(crate) fn complete(&self) -> HostResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Unavailable("control poisoned".into()))?;
        state.finished = true;
        state.response = None;
        if state.cancelled {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
    pub(crate) fn attach(
        &self,
        sender: Sender<HostResult<CapabilityRuntimeMessage>>,
    ) -> HostResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Unavailable("control poisoned".into()))?;
        if state.cancelled {
            return Err(Error::Cancelled);
        }
        state.response = Some(sender);
        Ok(())
    }
    pub(crate) fn detach(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.response = None;
        }
    }
    pub(crate) fn publish(&self, status: CapabilityRuntimeStatus, payload: &Option<Value>) -> bool {
        // Frames already carry a 4 MiB ceiling; snapshots are further capped at 4 KiB so large payloads are never retained persistently.
        if !matches!(
            status,
            CapabilityRuntimeStatus::Accepted | CapabilityRuntimeStatus::Progress
        ) || payload
            .as_ref()
            .is_some_and(|v| serde_json::to_vec(v).map_or(true, |b| b.len() > 4096))
        {
            return false;
        }
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if state.finished || state.cancelled {
            return true;
        }
        state.sequence += 1;
        state.progress = Some(InvocationProgress {
            sequence: state.sequence,
            status,
            payload: payload.clone(),
        });
        true
    }
}
