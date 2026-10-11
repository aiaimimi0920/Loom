// Otter bridge process state, broadcast history, and subscriber records.
struct OtterBridgeRuntime {
    discovery: Option<LocalCapabilityManifest>,
    port: Option<u16>,
    shutdown_tx: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
    connected_clients: Arc<AtomicUsize>,
    extension_capable_clients: Arc<AtomicUsize>,
    ocr_text_capable_clients: Arc<AtomicUsize>,
    connections: OtterBridgeConnections,
    broadcast_hub: OtterBridgeBroadcastHub,
    workflow_root: PathBuf,
}

impl OtterBridgeRuntime {
    fn new(workflow_root: PathBuf) -> Self {
        Self {
            discovery: None,
            port: None,
            shutdown_tx: None,
            worker: None,
            connected_clients: Arc::new(AtomicUsize::new(0)),
            extension_capable_clients: Arc::new(AtomicUsize::new(0)),
            ocr_text_capable_clients: Arc::new(AtomicUsize::new(0)),
            connections: OtterBridgeConnections::new(),
            broadcast_hub: OtterBridgeBroadcastHub::new(),
            workflow_root,
        }
    }
}

#[derive(Default)]
struct OtterArtRequestState {
    active_by_request: BTreeMap<OtterArtRequestScope, OtterArtRequestEntry>,
    active_by_node: BTreeMap<OtterArtNodeScope, String>,
    latest_generation_by_node: BTreeMap<OtterArtNodeScope, u64>,
    preview_revision_by_node: BTreeMap<OtterArtNodeScope, u64>,
    result_revision_by_node: BTreeMap<OtterArtNodeScope, u64>,
    terminal_by_request: BTreeMap<OtterArtRequestScope, OtterArtTerminalEntry>,
    terminal_order: VecDeque<OtterArtRequestScope>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OtterArtNodeScope {
    device_id: Option<String>,
    node_id: String,
}

impl OtterArtNodeScope {
    fn new(device_id: Option<&str>, node_id: &str) -> Self {
        Self {
            device_id: device_id.map(str::to_owned),
            node_id: node_id.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OtterArtRequestScope {
    device_id: Option<String>,
    request_id: String,
}

impl OtterArtRequestScope {
    fn new(device_id: Option<&str>, request_id: &str) -> Self {
        Self {
            device_id: device_id.map(str::to_owned),
            request_id: request_id.to_owned(),
        }
    }
}

struct OtterArtRequestEntry {
    node_id: String,
    generation: u64,
    request_fingerprint: String,
    cancellation: Arc<AtomicBool>,
    status: OtterRequestStatus,
    device_id: Option<String>,
    resource_handles: BTreeSet<String>,
    live_resource_handles: BTreeSet<String>,
    result_resource_handles: BTreeSet<String>,
}

struct OtterArtTerminalEntry {
    node_id: String,
    generation: u64,
    request_fingerprint: String,
    response: String,
    resource_handles: BTreeSet<String>,
    live_resource_handles: BTreeSet<String>,
    result_resource_handles: BTreeSet<String>,
}

#[derive(Clone)]
struct OtterBridgeBroadcastHub {
    subscribers: Arc<Mutex<Vec<OtterBridgeSubscriber>>>,
    next_subscriber_id: Arc<AtomicUsize>,
    history: Arc<(Mutex<VecDeque<OtterBridgeHistoryEntry>>, Condvar)>,
    next_sequence: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct OtterBridgeHistoryEntry {
    sequence: usize,
    message: String,
}

const HOOK_BRIDGE_HISTORY_CAPACITY: usize = 2_048;
const HOOK_BRIDGE_POLL_MAX_MESSAGES: usize = 128;
// Cursor zero requests recovery. Reserve one non-message cursor so an initial recovery can advance
// even when no broadcast exists, without skipping the first future broadcast.
const HOOK_BRIDGE_RECOVERY_CURSOR: usize = 1;
const SURFACE_STREAM_WORKERS: usize = 8;
const SURFACE_STREAM_QUEUE_CAPACITY: usize = 128;

fn is_surface_stream_request(request: &ParsedHttpRequest) -> bool {
    request.method == "GET"
        && request
            .path
            .split('?')
            .next()
            .is_some_and(|path| path == "/v1/surfaces/stream")
}

impl OtterBridgeBroadcastHub {
    fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
            next_subscriber_id: Arc::new(AtomicUsize::new(1)),
            history: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())),
            next_sequence: Arc::new(AtomicUsize::new(HOOK_BRIDGE_RECOVERY_CURSOR + 1)),
        }
    }

    fn subscriber_count(&self) -> usize {
        self.subscribers
            .lock()
            .map(|subscribers| subscribers.len())
            .unwrap_or_default()
    }

    fn clear(&self) {
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.clear();
        }
        if let Ok(mut history) = self.history.0.lock() {
            history.clear();
        }
    }

    fn record(&self, broadcasts: &[String]) {
        if broadcasts.is_empty() {
            return;
        }
        let (history_lock, changed) = &*self.history;
        let Ok(mut history) = history_lock.lock() else {
            return;
        };
        let mut bytes: usize = history.iter().map(|entry| entry.message.len()).sum();
        for message in broadcasts {
            let sequence = self.next_sequence.fetch_add(1, Ordering::SeqCst);
            if message.len() > otter_broadcast_queue::MAX_BYTES {
                // A skipped event invalidates cursor continuity; force snapshot recovery.
                history.clear();
                bytes = 0;
                continue;
            }
            while history.len() >= HOOK_BRIDGE_HISTORY_CAPACITY
                || bytes > otter_broadcast_queue::MAX_BYTES - message.len()
            {
                if let Some(removed) = history.pop_front() {
                    bytes -= removed.message.len();
                }
            }
            bytes += message.len();
            history.push_back(OtterBridgeHistoryEntry {
                sequence,
                message: message.clone(),
            });
        }
        changed.notify_all();
    }

    fn wait_after(
        &self,
        after: usize,
        timeout: Duration,
    ) -> (usize, bool, Vec<OtterBridgeHistoryEntry>) {
        let after = after.max(HOOK_BRIDGE_RECOVERY_CURSOR);
        let deadline = Instant::now() + timeout;
        let (history_lock, changed) = &*self.history;
        let Ok(mut history) = history_lock.lock() else {
            return (after, false, Vec::new());
        };
        loop {
            if history.is_empty() {
                let latest = self.next_sequence.load(Ordering::SeqCst).saturating_sub(1);
                if latest > after {
                    return (latest, true, Vec::new());
                }
            }
            let oldest = history.front().map(|entry| entry.sequence);
            let reset = oldest.is_some_and(|oldest| after.saturating_add(1) < oldest);
            let entries = history
                .iter()
                .filter(|entry| reset || entry.sequence > after)
                .take(HOOK_BRIDGE_POLL_MAX_MESSAGES)
                .cloned()
                .collect::<Vec<_>>();
            if !entries.is_empty() {
                let next = entries.last().map(|entry| entry.sequence).unwrap_or(after);
                return (next, reset, entries);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return (after, false, Vec::new());
            }
            let Ok((next_history, timed_out)) = changed.wait_timeout(history, remaining) else {
                return (after, false, Vec::new());
            };
            history = next_history;
            if timed_out.timed_out() {
                return (after, false, Vec::new());
            }
        }
    }
}

#[derive(Clone)]
struct OtterBridgeSubscriber {
    id: usize,
    tx: otter_broadcast_queue::BroadcastSender,
    channels: Vec<String>,
}
