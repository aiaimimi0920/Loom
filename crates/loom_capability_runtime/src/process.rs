use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read as _;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use loom_process::{ManagedChild, ManagedChildPipes, ProcessSpec};
use loom_protocol::{CapabilityRuntimeMessage, CapabilityRuntimeStatus};

use crate::error::{CapabilityHostError, HostResult};
use crate::frame::{read_runtime_frame, write_runtime_frame};

const REQUEST_QUEUE_CAPACITY: usize = 16;
const STDERR_CAPTURE_BYTES: usize = 64 * 1024;
const MAX_INTERMEDIATE_RESPONSES: u16 = 1024;
/// Identities the reader already failed locally. Bounded so a hostile runtime
/// cannot grow host memory by replaying finalisations for retired requests.
const MAX_RETIRED_REQUESTS: usize = 4 * REQUEST_QUEUE_CAPACITY;

type RuntimeResponse = mpsc::Sender<HostResult<CapabilityRuntimeMessage>>;
type PendingResponses = Arc<Mutex<HashMap<String, RuntimeResponse>>>;
type RetiredRequestIds = Arc<Mutex<RetiredRequests>>;

struct RuntimeRequest {
    request_id: String,
    message: CapabilityRuntimeMessage,
    response: RuntimeResponse,
}

#[derive(Clone)]
pub(crate) struct RuntimeProcessClient {
    child: Arc<Mutex<ManagedChild>>,
    requests: SyncSender<RuntimeRequest>,
    pending: PendingResponses,
    retired: RetiredRequestIds,
}

pub(crate) struct RuntimeProcess {
    child: Arc<Mutex<ManagedChild>>,
    requests: Option<SyncSender<RuntimeRequest>>,
    pending: PendingResponses,
    retired: RetiredRequestIds,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    stderr: Arc<Mutex<Vec<u8>>>,
}

impl RuntimeProcess {
    pub(crate) fn spawn(spec: &ProcessSpec) -> HostResult<Self> {
        let (child, pipes) = ManagedChild::spawn(spec)?;
        let child = Arc::new(Mutex::new(child));
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let retired = Arc::new(Mutex::new(RetiredRequests::default()));
        let (requests, receiver) = mpsc::sync_channel(REQUEST_QUEUE_CAPACITY);
        let ManagedChildPipes {
            stdin,
            stdout,
            stderr: child_stderr,
        } = pipes;
        let writer_pending = Arc::clone(&pending);
        let writer_retired = Arc::clone(&retired);
        let writer = std::thread::Builder::new()
            .name("loom-capability-runtime-writer".to_owned())
            .spawn(move || request_writer(stdin, receiver, writer_pending, writer_retired))?;
        let reader_pending = Arc::clone(&pending);
        let reader_retired = Arc::clone(&retired);
        let reader = std::thread::Builder::new()
            .name("loom-capability-runtime-reader".to_owned())
            .spawn(move || response_reader(stdout, reader_pending, reader_retired))?;
        spawn_stderr_drain(child_stderr, Arc::clone(&stderr))?;
        Ok(Self {
            child,
            requests: Some(requests),
            pending,
            retired,
            writer: Some(writer),
            reader: Some(reader),
            stderr,
        })
    }

    pub(crate) fn client(&self) -> HostResult<RuntimeProcessClient> {
        Ok(RuntimeProcessClient {
            child: Arc::clone(&self.child),
            requests: self.requests.as_ref().cloned().ok_or_else(|| {
                CapabilityHostError::Unavailable("runtime process is closed".to_owned())
            })?,
            pending: Arc::clone(&self.pending),
            retired: Arc::clone(&self.retired),
        })
    }

    pub(crate) fn call(
        &self,
        message: CapabilityRuntimeMessage,
        timeout: Duration,
    ) -> HostResult<CapabilityRuntimeMessage> {
        self.client()?.call(message, timeout)
    }

    pub(crate) fn terminate(&mut self) {
        self.requests.take();
        terminate_child(&self.child);
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }

    pub(crate) fn id(&self) -> Option<u32> {
        self.child.lock().ok().map(|child| child.id())
    }

    #[allow(dead_code)]
    pub(crate) fn stderr_excerpt(&self) -> Vec<u8> {
        self.stderr
            .lock()
            .map(|bytes| bytes.clone())
            .unwrap_or_default()
    }
}

impl RuntimeProcessClient {
    pub(crate) fn call(
        &self,
        message: CapabilityRuntimeMessage,
        timeout: Duration,
    ) -> HostResult<CapabilityRuntimeMessage> {
        let request_id = message_request_id(&message)?;
        let reservation = request_id.clone();
        let (response, receiver) = mpsc::channel();
        let request = RuntimeRequest {
            request_id,
            message,
            response,
        };
        match self.requests.try_send(request) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(CapabilityHostError::Busy),
            Err(TrySendError::Disconnected(_)) => {
                return Err(CapabilityHostError::Unavailable(
                    "runtime request writer stopped".to_owned(),
                ))
            }
        }
        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Quarantine the identity before releasing its reservation. A
                // buffered response from the dying process must never satisfy a
                // retry that reused the same identity.
                if let Ok(mut retired) = self.retired.lock() {
                    retired.insert(reservation.clone());
                }
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&reservation);
                }
                terminate_child(&self.child);
                Err(CapabilityHostError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(CapabilityHostError::Unavailable(
                "runtime response reader stopped".to_owned(),
            )),
        }
    }

    pub(crate) fn terminate(&self) {
        terminate_child(&self.child);
    }
}

impl Drop for RuntimeProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn request_writer<W: std::io::Write>(
    mut stdin: W,
    receiver: Receiver<RuntimeRequest>,
    pending: PendingResponses,
    retired: RetiredRequestIds,
) {
    for request in receiver {
        let RuntimeRequest {
            request_id,
            message,
            response,
        } = request;
        let is_retired = match retired.lock() {
            Ok(retired) => retired.contains(&request_id),
            Err(_) => {
                let _ = response.send(Err(CapabilityHostError::Unavailable(
                    "runtime retired request map lock poisoned".to_owned(),
                )));
                continue;
            }
        };
        if is_retired {
            let _ = response.send(Err(CapabilityHostError::Protocol(
                "retired runtime request identity".to_owned(),
            )));
            continue;
        }
        let inserted = match pending.lock() {
            Ok(mut pending) if !pending.contains_key(&request_id) => {
                pending.insert(request_id.clone(), response);
                true
            }
            Ok(_) => {
                let _ = response.send(Err(CapabilityHostError::Protocol(
                    "duplicate runtime request identity".to_owned(),
                )));
                false
            }
            Err(_) => {
                let _ = response.send(Err(CapabilityHostError::Unavailable(
                    "runtime response map lock poisoned".to_owned(),
                )));
                false
            }
        };
        if !inserted {
            continue;
        }
        if let Err(error) = write_runtime_frame(&mut stdin, &message) {
            fail_request(&pending, &request_id, &error.to_string());
            break;
        }
    }
}

fn response_reader(
    mut stdout: std::process::ChildStdout,
    pending: PendingResponses,
    retired: RetiredRequestIds,
) {
    let mut intermediate = HashMap::<String, u16>::new();
    loop {
        let message = match read_runtime_frame(&mut stdout) {
            Ok(message) => message,
            Err(error) => {
                fail_all(&pending, &error.to_string());
                return;
            }
        };
        match message {
            CapabilityRuntimeMessage::Event { .. } => {}
            CapabilityRuntimeMessage::Response {
                ref request_id,
                status: CapabilityRuntimeStatus::Accepted | CapabilityRuntimeStatus::Progress,
                ..
            } => {
                // Intermediate frames are only tracked for identities the host
                // actually has in flight. Counting fabricated identities would
                // let a runtime grow this map without bound.
                let request_retired = retired
                    .lock()
                    .map(|retired| retired.contains(request_id))
                    .unwrap_or(true);
                if request_retired || !is_pending(&pending, request_id) {
                    intermediate.remove(request_id);
                    continue;
                }
                let count = intermediate.entry(request_id.clone()).or_default();
                *count = count.saturating_add(1);
                if *count > MAX_INTERMEDIATE_RESPONSES {
                    intermediate.remove(request_id);
                    if let Ok(mut retired) = retired.lock() {
                        retired.insert(request_id.clone());
                    }
                    fail_request(
                        &pending,
                        request_id,
                        "too many intermediate runtime responses",
                    );
                }
            }
            CapabilityRuntimeMessage::Response { ref request_id, .. } => {
                intermediate.remove(request_id);
                // A finalisation for a request this reader already failed is
                // expected, not a protocol breach: drop it without disturbing
                // the other in-flight invocations on this process.
                let request_retired = retired
                    .lock()
                    .map(|mut retired| retired.remove(request_id))
                    .unwrap_or(true);
                if request_retired {
                    continue;
                }
                let response = pending
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.remove(request_id));
                let Some(response) = response else {
                    fail_all(&pending, "runtime returned an unknown request identity");
                    return;
                };
                let _ = response.send(Ok(message));
            }
            CapabilityRuntimeMessage::Request { .. } => {
                fail_all(&pending, "runtime sent a host-only request message");
                return;
            }
        }
    }
}

#[derive(Default)]
struct RetiredRequests {
    order: VecDeque<String>,
    ids: HashSet<String>,
}

impl RetiredRequests {
    fn contains(&self, request_id: &str) -> bool {
        self.ids.contains(request_id)
    }

    fn insert(&mut self, request_id: String) {
        if !self.ids.insert(request_id.clone()) {
            return;
        }
        self.order.push_back(request_id);
        while self.order.len() > MAX_RETIRED_REQUESTS {
            if let Some(evicted) = self.order.pop_front() {
                self.ids.remove(&evicted);
            }
        }
    }

    fn remove(&mut self, request_id: &str) -> bool {
        if !self.ids.remove(request_id) {
            return false;
        }
        if let Some(position) = self
            .order
            .iter()
            .position(|candidate| candidate == request_id)
        {
            self.order.remove(position);
        }
        true
    }
}

fn is_pending(pending: &PendingResponses, request_id: &str) -> bool {
    pending
        .lock()
        .map(|pending| pending.contains_key(request_id))
        .unwrap_or(false)
}

fn message_request_id(message: &CapabilityRuntimeMessage) -> HostResult<String> {
    match message {
        CapabilityRuntimeMessage::Request { request_id, .. } => Ok(request_id.clone()),
        _ => Err(CapabilityHostError::Protocol(
            "host can only queue runtime request messages".to_owned(),
        )),
    }
}

fn fail_request(pending: &PendingResponses, request_id: &str, message: &str) {
    let response = pending
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(request_id));
    if let Some(response) = response {
        let _ = response.send(Err(CapabilityHostError::Unavailable(message.to_owned())));
    }
}

fn fail_all(pending: &PendingResponses, message: &str) {
    let responses = pending
        .lock()
        .map(|mut pending| pending.drain().map(|(_, value)| value).collect::<Vec<_>>())
        .unwrap_or_default();
    for response in responses {
        let _ = response.send(Err(CapabilityHostError::Unavailable(message.to_owned())));
    }
}

fn terminate_child(child: &Arc<Mutex<ManagedChild>>) {
    if let Ok(mut child) = child.lock() {
        child.terminate();
    }
}

fn spawn_stderr_drain(
    mut stderr: std::process::ChildStderr,
    capture: Arc<Mutex<Vec<u8>>>,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("loom-capability-runtime-stderr".to_owned())
        .spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(read) = stderr.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                if let Ok(mut bytes) = capture.lock() {
                    let remaining = STDERR_CAPTURE_BYTES.saturating_sub(bytes.len());
                    bytes.extend_from_slice(&buffer[..read.min(remaining)]);
                }
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod process_tests {
    use super::*;
    use loom_protocol::CapabilityRuntimeMethod;
    use serde_json::json;

    #[test]
    fn writer_rejects_a_retired_request_identity() {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let retired = Arc::new(Mutex::new(RetiredRequests::default()));
        retired.lock().unwrap().insert("request-1".to_owned());
        let (requests, receiver) = mpsc::sync_channel(1);
        let (response, result) = mpsc::channel();
        requests
            .send(RuntimeRequest {
                request_id: "request-1".to_owned(),
                message: crate::session::runtime_request(
                    "request-1".to_owned(),
                    CapabilityRuntimeMethod::Command,
                    json!({}),
                ),
                response,
            })
            .unwrap();
        drop(requests);

        request_writer(Vec::<u8>::new(), receiver, Arc::clone(&pending), retired);

        let error = result.recv().unwrap().unwrap_err();
        assert!(matches!(
            error,
            CapabilityHostError::Protocol(message)
                if message == "retired runtime request identity"
        ));
        assert!(pending.lock().unwrap().is_empty());
    }
}
