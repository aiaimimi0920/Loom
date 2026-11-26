//! Each daemon instance owns its task table and worker threads. Retains terminal entries until grant expiry to prevent retry replay; rejects at capacity instead of unbounded queueing.
use super::*;
use loom_capability_runtime::InvocationControl;
use loom_protocol::local_art::service::{Owner, Reply, Status, Submit, PROTOCOL};
use service_lease::Lease;
#[path = "service_confirmation.rs"]
mod confirmation;

const MAX_TASKS: usize = 128;
const MAX_DEVICE_TASKS: usize = 32;
const MAX_RUNNING: usize = 16;
const MAX_DEVICE_RUNNING: usize = 4;
#[derive(Default)]
pub(crate) struct Tasks {
    state: Mutex<Table>,
    image_bytes: Arc<std::sync::atomic::AtomicUsize>,
    preparing: AtomicBool,
}
pub(super) struct Preparation<'a>(&'a AtomicBool);
impl Drop for Preparation<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl Tasks {
    pub(super) fn enter_preparation(&self) -> LocalResult<Preparation<'_>> {
        self.preparing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Preparation(&self.preparing))
            .map_err(|_| fail(429, "local_art_prepare_busy"))
    }
}
#[derive(Default)]
struct Table {
    stopped: bool,
    entries: Vec<Entry>,
}
struct Entry {
    task: Arc<Task>,
    worker: std::thread::JoinHandle<()>,
}
pub(super) struct Task {
    pub lease: Lease,
    pub control: Arc<InvocationControl>,
    owner: Owner,
    fingerprint: String,
    state: Mutex<State>,
    pub image_output: bool,
    pub image_bytes: Arc<std::sync::atomic::AtomicUsize>,
    pub deadline: std::time::Instant,
    pub(super) gate: Option<confirmation::Gate>,
}
struct State {
    reply: Reply,
    progress_sequence: u64,
    image: Option<Arc<service_image_output::Stored>>,
}
fn terminal(status: Status) -> bool {
    matches!(
        status,
        Status::Succeeded | Status::Failed | Status::Cancelled
    )
}
impl Table {
    fn existing(
        &self,
        input: &Submit,
        fingerprint: &str,
        lease: &Lease,
    ) -> LocalResult<Option<Reply>> {
        if self.stopped {
            return Err(fail(503, "local_art_stopping"));
        }
        self.entries
            .iter()
            .find(|entry| same_call(&entry.task.owner, &input.owner))
            .map(|entry| {
                if entry.task.owner != input.owner || entry.task.fingerprint != fingerprint {
                    return Err(fail(409, "local_art_call_conflict"));
                }
                entry.task.snapshot(lease)
            })
            .transpose()
    }
}
impl Tasks {
    pub(super) fn submit(
        &self,
        input: Submit,
        command: String,
        lease: Lease,
        runtime: SharedCapabilityRuntime,
        images: Vec<loom_protocol::local_art::service::images::Image>,
        image_context: service_images::Context,
    ) -> LocalResult<Reply> {
        self.prune();
        let image_output = image_context.image_output;
        let fingerprint = sha256_bytes(
            &serde_json::to_vec(&(
                &input.input,
                input.timeout_ms,
                image_output,
                image_context.confirmable,
                images
                    .iter()
                    .map(|image| sha256_bytes(image.png.as_bytes()))
                    .collect::<Vec<_>>(),
            ))
            .map_err(|_| fail(400, "local_art_service_contract"))?,
        );
        let expires = unix_time_millis()
            .saturating_add(input.timeout_ms)
            .min(lease.expires());
        let deadline = std::time::Instant::now()
            + Duration::from_millis(expires.saturating_sub(unix_time_millis()));
        if let Some(reply) = self
            .state
            .lock()
            .map_err(|_| fail(503, "local_art_tasks_unavailable"))?
            .existing(&input, &fingerprint, &lease)?
        {
            return Ok(reply);
        }
        // `command_scope` may wait for plugin initialization; do not hold the task table lock to block concurrent cancel/shutdown,
        // and never add wait time back onto the deadline. Re-verify the call and capacity after relocking so only one worker spawns.
        let gate = if image_context.confirmable {
            Some(confirmation::Gate::new(
                &runtime,
                &command,
                &fingerprint,
                expires,
            )?)
        } else {
            None
        };
        let mut table = self
            .state
            .lock()
            .map_err(|_| fail(503, "local_art_tasks_unavailable"))?;
        if let Some(reply) = table.existing(&input, &fingerprint, &lease)? {
            return Ok(reply);
        }
        if deadline <= std::time::Instant::now() || !lease.valid() {
            return Err(fail(403, "local_art_service_timeout"));
        }
        let device = &input.owner.binding.device_id;
        let same_device = |e: &&Entry| &e.task.owner.binding.device_id == device;
        if table.entries.len() >= MAX_TASKS
            || table.entries.iter().filter(same_device).count() >= MAX_DEVICE_TASKS
            || table
                .entries
                .iter()
                .filter(|e| !e.worker.is_finished())
                .count()
                >= MAX_RUNNING
            || table
                .entries
                .iter()
                .filter(same_device)
                .filter(|e| !e.worker.is_finished())
                .count()
                >= MAX_DEVICE_RUNNING
        {
            return Err(fail(429, "local_art_service_capacity"));
        }
        let reply = Reply {
            protocol_version: PROTOCOL.into(),
            owner: input.owner.clone(),
            sequence: 1,
            status: Status::Accepted,
            output: None,
            error: None,
            progress: None,
        };
        let task = Arc::new(Task {
            lease,
            control: Arc::new(InvocationControl::new()),
            owner: input.owner,
            fingerprint,
            gate,
            deadline,
            image_output,
            image_bytes: self.image_bytes.clone(),
            state: Mutex::new(State {
                reply: reply.clone(),
                progress_sequence: 0,
                image: None,
            }),
        });
        let worker_task = task.clone();
        let worker = std::thread::Builder::new()
            .name("loom-local-art-service".into())
            .spawn(move || {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    service_execution::run(
                        &worker_task,
                        runtime,
                        command,
                        input.input,
                        images,
                        image_context,
                    );
                }))
                .is_err()
                {
                    worker_task.finish(Status::Failed, None, Some("local_art_worker_failed"));
                }
            })
            .map_err(|_| fail(503, "local_art_worker_unavailable"))?;
        table.entries.push(Entry { task, worker });
        Ok(reply)
    }
    pub(super) fn control(&self, owner: &Owner, lease: &Lease, cancel: bool) -> LocalResult<Reply> {
        let task = self
            .state
            .lock()
            .map_err(|_| fail(503, "local_art_tasks_unavailable"))?
            .entries
            .iter()
            .find(|e| &e.task.owner == owner)
            .map(|e| e.task.clone())
            .ok_or_else(|| fail(404, "local_art_call_not_found"))?;
        task.snapshot(lease)?;
        if cancel {
            task.cancel(None);
        }
        task.snapshot(lease)
    }
    pub(super) fn image_output(&self, owner: &Owner) -> LocalResult<Value> {
        let task = self
            .state
            .lock()
            .map_err(|_| fail(503, "local_art_tasks_unavailable"))?
            .entries
            .iter()
            .find(|e| &e.task.owner == owner)
            .map(|e| e.task.clone())
            .ok_or_else(|| fail(404, "local_art_call_not_found"))?;
        if !task.image_output {
            return Err(fail(409, "local_art_image_output_scope"));
        }
        if !task.lease.valid() {
            task.cancel(Some("local_art_authorization_changed"));
            return Err(fail(403, "local_art_image_output_scope"));
        }
        let state = task
            .state
            .lock()
            .map_err(|_| fail(503, "local_art_task_unavailable"))?;
        let response = loom_protocol::local_art::service::image_output::Response {
            protocol_version: loom_protocol::local_art::service::image_output::PROTOCOL.into(),
            reply: state.reply.clone(),
            image: state.image.as_ref().map(|v| v.image.clone()),
        };
        response
            .validate(owner)
            .map_err(|_| fail(409, "local_art_image_output_unavailable"))?;
        serde_json::to_value(response).map_err(|_| fail(500, "local_art_reply_invalid"))
    }
    pub(crate) fn prune(&self) {
        let tasks = self
            .state
            .lock()
            .map(|table| {
                table
                    .entries
                    .iter()
                    .map(|e| e.task.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for task in tasks {
            if !task.lease.valid() {
                task.cancel(Some("local_art_authorization_changed"));
            }
        }
        let now = unix_time_millis();
        let mut retired = Vec::new();
        if let Ok(mut table) = self.state.lock() {
            let mut index = 0;
            while index < table.entries.len() {
                if table.entries[index].task.lease.expires() <= now
                    && table.entries[index].worker.is_finished()
                {
                    retired.push(table.entries.swap_remove(index));
                } else {
                    index += 1;
                }
            }
        }
        for entry in retired {
            let _ = entry.worker.join();
        }
    }
    pub(crate) fn shutdown(&self) {
        let entries = self
            .state
            .lock()
            .map(|mut table| {
                table.stopped = true;
                std::mem::take(&mut table.entries)
            })
            .unwrap_or_default();
        for entry in &entries {
            entry.task.cancel(None);
        }
        for entry in entries {
            let _ = entry.worker.join();
        }
    }
}
impl Drop for Tasks {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn same_call(a: &Owner, b: &Owner) -> bool {
    a.binding.device_id == b.binding.device_id && a.grant_id == b.grant_id && a.call_id == b.call_id
}
impl Task {
    fn snapshot(&self, lease: &Lease) -> LocalResult<Reply> {
        if !self.lease.same_session(lease) || !self.lease.valid() || !lease.valid() {
            self.cancel(Some("local_art_authorization_changed"));
            return Err(fail(403, "local_art_authorization_changed"));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| fail(503, "local_art_task_unavailable"))?;
        if !terminal(state.reply.status) {
            if let Some(progress) = self.control.progress() {
                if progress.sequence > state.progress_sequence {
                    state.progress_sequence = progress.sequence;
                    state.reply.progress = progress.payload;
                    state.reply.status = Status::Running;
                    state.reply.sequence += 1;
                }
            }
        }
        state
            .reply
            .validate(&self.owner)
            .map_err(|_| fail(500, "local_art_reply_invalid"))?;
        Ok(state.reply.clone())
    }
    pub(super) fn cancel(&self, error: Option<&str>) {
        if let Ok(mut state) = self.state.lock() {
            state.image = None;
            if !terminal(state.reply.status) {
                // The task may still be encoding PNG bytes after the runtime finishes; cancellation must cover that uncommitted window.
                self.control.cancel();
                state.reply.status = if error.is_some() {
                    Status::Failed
                } else {
                    Status::Cancelled
                };
                state.reply.error = error.map(str::to_owned);
                state.reply.sequence += 1;
            }
        }
        if let Some(gate) = &self.gate {
            gate.wake();
        }
    }
    pub(super) fn finish(&self, status: Status, output: Option<Value>, error: Option<&str>) {
        self.finish_with_image(status, output, error, None);
    }
    pub(super) fn finish_with_image(
        &self,
        mut status: Status,
        mut output: Option<Value>,
        mut error: Option<&str>,
        mut image: Option<Arc<service_image_output::Stored>>,
    ) {
        if let Ok(mut state) = self.state.lock() {
            if terminal(state.reply.status) {
                return;
            }
            // Package generation replacement, revocation, or expiry can occur between runtime completion and PNG encoding; re-verify right before commit.
            let invalid = if !self.lease.valid() {
                Some("local_art_authorization_changed")
            } else if self.gate.as_ref().is_some_and(|gate| !gate.current()) {
                Some("local_art_command_changed")
            } else if self.deadline <= std::time::Instant::now() {
                Some("local_art_service_timeout")
            } else {
                None
            };
            if let Some(reason) = invalid {
                status = Status::Failed;
                output = None;
                image = None;
                error = Some(reason);
            }
            if let Some(progress) = self.control.progress() {
                state.reply.progress = progress.payload;
            }
            state.reply.sequence += 1;
            state.reply.status = status;
            state.reply.output = output;
            state.reply.error = error.map(str::to_owned);
            state.image = image;
            if state.reply.validate(&self.owner).is_err() {
                state.reply.status = Status::Failed;
                state.reply.output = None;
                state.reply.error = Some("local_art_result_limit".into());
                state.image = None;
            }
        }
    }
}
