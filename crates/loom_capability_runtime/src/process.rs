use std::collections::HashMap;
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

type RuntimeResponse = mpsc::Sender<HostResult<CapabilityRuntimeMessage>>;
type PendingResponses = Arc<Mutex<HashMap<String, RuntimeResponse>>>;

struct RuntimeRequest {
    request_id: String,
    message: CapabilityRuntimeMessage,
    response: RuntimeResponse,
}

#[derive(Clone)]
pub(crate) struct RuntimeProcessClient {
    child: Arc<Mutex<ManagedChild>>,
    requests: SyncSender<RuntimeRequest>,
}

pub(crate) struct RuntimeProcess {
    child: Arc<Mutex<ManagedChild>>,
    requests: Option<SyncSender<RuntimeRequest>>,
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
        let (requests, receiver) = mpsc::sync_channel(REQUEST_QUEUE_CAPACITY);
        let ManagedChildPipes {
            stdin,
            stdout,
            stderr: child_stderr,
        } = pipes;
        let writer_pending = Arc::clone(&pending);
        let writer = std::thread::Builder::new()
            .name("loom-capability-runtime-writer".to_owned())
            .spawn(move || request_writer(stdin, receiver, writer_pending))?;
        let reader_pending = Arc::clone(&pending);
        let reader = std::thread::Builder::new()
            .name("loom-capability-runtime-reader".to_owned())
            .spawn(move || response_reader(stdout, reader_pending))?;
        spawn_stderr_drain(child_stderr, Arc::clone(&stderr))?;
        Ok(Self {
            child,
            requests: Some(requests),
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

fn request_writer(
    mut stdin: std::process::ChildStdin,
    receiver: Receiver<RuntimeRequest>,
    pending: PendingResponses,
) {
    for request in receiver {
        let RuntimeRequest {
            request_id,
            message,
            response,
        } = request;
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

fn response_reader(mut stdout: std::process::ChildStdout, pending: PendingResponses) {
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
                let count = intermediate.entry(request_id.clone()).or_default();
                *count = count.saturating_add(1);
                if *count > MAX_INTERMEDIATE_RESPONSES {
                    fail_request(
                        &pending,
                        request_id,
                        "too many intermediate runtime responses",
                    );
                }
            }
            CapabilityRuntimeMessage::Response { ref request_id, .. } => {
                intermediate.remove(request_id);
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
