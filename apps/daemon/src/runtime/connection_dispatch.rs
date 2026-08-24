// Bounded connection admission, read workers, dispatch, drain, and concurrency classes.
/// Size of the pool that reads requests off accepted sockets.
///
/// Reading used to happen inline on the accept thread, which meant one client sending its request
/// head a byte at a time stalled every other connection — including `/health` — for as long as it
/// cared to keep dribbling. The reads now happen here; the accept thread only sets socket options
/// and hands the socket over.
const CONNECTION_READ_WORKERS: usize = 4;
const CONNECTION_READ_QUEUE_CAPACITY: usize = 64;
/// Keep one reader available for a different network peer even when one address trickles requests.
const CONNECTION_READ_PER_PEER_LIMIT: usize = CONNECTION_READ_WORKERS - 1;

/// Per-`read` timeout. Bounds how long a single syscall can block, not the whole request.
const CONNECTION_READ_TIMEOUT_MILLIS: u64 = 2_000;

/// Write timeout for every accepted socket. Without one, a peer that stops reading its response
/// parks the worker that is writing it forever.
const RESPONSE_WRITE_TIMEOUT_MILLIS: u64 = 30_000;

/// How long the accept loop waits for work when there is nothing to accept and nothing to dispatch.
const ACCEPT_IDLE_WAIT_MILLIS: u64 = 10;

/// Total budget for finishing the requests that were already in flight when shutdown arrived.
///
/// It bounds two things: the backlog the listener accepted but never handed to a reader, and the
/// extra time a reader gives a request whose first bytes had already arrived. Dropping either kind
/// of socket unread makes the platform answer the peer with an RST, which destroys any response
/// that was already on the wire, so both are worth a short wait — but only a short one, since
/// shutdown must not wait on a client that may never finish sending.
const SHUTDOWN_READ_GRACE_MILLIS: u64 = 500;
/// Refused clients may already be uploading. Consume a bounded prefix before the 503 so Windows
/// delivers the response instead of resetting the connection on unread request bytes.
const REFUSAL_READ_GRACE_MILLIS: u64 = 50;

#[derive(Clone)]
struct PeerReadAdmission {
    counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
    limit: usize,
}

impl PeerReadAdmission {
    fn new(limit: usize) -> Self {
        assert!(limit > 0, "per-peer read limit must be positive");
        Self {
            counts: Arc::new(Mutex::new(HashMap::new())),
            limit,
        }
    }

    fn try_acquire(&self, peer: IpAddr) -> Option<PeerReadPermit> {
        let mut counts = self
            .counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = counts.entry(peer).or_default();
        if *count >= self.limit {
            return None;
        }
        *count += 1;
        Some(PeerReadPermit {
            peer,
            counts: Arc::clone(&self.counts),
        })
    }
}

struct PeerReadPermit {
    peer: IpAddr,
    counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl Drop for PeerReadPermit {
    fn drop(&mut self) {
        let mut counts = self
            .counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = counts.get_mut(&self.peer) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.remove(&self.peer);
            }
        }
    }
}

/// A socket handed from the accept thread to a read worker.
///
/// The reply channel travels inside the job so the read stage needs no shared state beyond the
/// draining flag, and so a job only has to be `Send`.
struct ConnectionReadJob {
    stream: TcpStream,
    ready: Sender<ReadyConnection>,
    _peer_permit: PeerReadPermit,
}

/// A socket whose request has been read and is ready to dispatch.
struct ReadyConnection {
    stream: TcpStream,
    outcome: HttpReadOutcome,
}

enum DispatchOutcome {
    Continue,
    Stop,
}

/// Applies the socket options, on the accept thread, before the socket reaches a read worker.
///
/// The write timeout is set here rather than in the read stage because the accept thread itself
/// writes responses — a queue-full 503 goes out on a socket no worker ever touches.
fn prepare_connection(stream: TcpStream) -> Option<TcpStream> {
    let read_timeout = Duration::from_millis(CONNECTION_READ_TIMEOUT_MILLIS);
    let write_timeout = Duration::from_millis(RESPONSE_WRITE_TIMEOUT_MILLIS);
    if let Err(error) = stream.set_nonblocking(false) {
        eprintln!("loom connection setup failed: {error}");
        return None;
    }
    if let Err(error) = stream.set_read_timeout(Some(read_timeout)) {
        eprintln!("loom connection read-timeout setup failed: {error}");
        return None;
    }
    if let Err(error) = stream.set_write_timeout(Some(write_timeout)) {
        eprintln!("loom connection write-timeout setup failed: {error}");
        return None;
    }
    Some(stream)
}

/// Reads whatever the listener has already accepted but not yet handed to a read worker.
///
/// Closing the listener with a connection still sitting in its backlog resets that connection, and a
/// reset destroys a response the peer has not read yet — so a client whose request arrived just
/// before shutdown would see a dropped connection instead of the 503 it was owed. Reading happens
/// inline here rather than on the read stage: the accept thread has nothing left to protect at
/// shutdown, and one shared deadline bounds the whole drain no matter how many sockets are queued.
///
/// The returned connections still have to be dispatched; this only gets them read.
fn drain_accept_backlog(listener: &TcpListener) -> Vec<ReadyConnection> {
    let deadline = Instant::now() + Duration::from_millis(SHUTDOWN_READ_GRACE_MILLIS);
    let abort = AtomicBool::new(false);
    let mut drained = Vec::new();
    while drained.len() < CONNECTION_READ_QUEUE_CAPACITY {
        // The listener is non-blocking, so an empty backlog ends the loop instead of waiting on one.
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(_) => break,
        };
        let mut stream = match prepare_connection(stream) {
            Some(stream) => stream,
            None => continue,
        };
        match read_http_request_until(&mut stream, deadline, &abort) {
            Ok(outcome) => drained.push(ReadyConnection { stream, outcome }),
            Err(error) => eprintln!("loom shutdown drain read failed: {error:#}"),
        }
    }
    drained
}

/// Reads one request on a read worker and returns the socket to the accept loop.
///
/// Once `draining` is set the daemon is shutting down. A queued socket gets one bounded drain window
/// before the 503 is written; this consumes bytes that may already be in the kernel without allowing
/// an idle client to delay shutdown indefinitely. A read already in progress uses the same grace in
/// `read_http_request_until`.
fn read_connection(job: ConnectionReadJob, draining: &AtomicBool) {
    let ConnectionReadJob {
        mut stream,
        ready,
        _peer_permit,
    } = job;
    if draining.load(Ordering::SeqCst) {
        // This job was accepted before shutdown but did not get a reader until after shutdown had
        // started. Its bytes may already be waiting in the socket. Writing a response without first
        // consuming them makes Windows reset the connection and destroys the 503 on the wire. Bound
        // this late drain independently so an idle peer still cannot delay shutdown indefinitely.
        let grace = Duration::from_millis(SHUTDOWN_READ_GRACE_MILLIS);
        let _ = stream.set_read_timeout(Some(grace));
        let _ =
            read_http_request_until(&mut stream, Instant::now() + grace, &AtomicBool::new(false));
        let (status, body) = daemon_shutting_down_response();
        write_response_safely(stream, status, &body);
        return;
    }
    let deadline = Instant::now() + Duration::from_millis(MAX_REQUEST_READ_MILLIS);
    match read_http_request_until(&mut stream, deadline, draining) {
        Ok(outcome) => {
            // A failed send means the accept loop has already stopped receiving, which only happens
            // after shutdown drained the channel. Dropping the socket then is the right answer.
            let _ = ready.send(ReadyConnection { stream, outcome });
        }
        Err(error) => eprintln!("loom request read failed: {error:#}"),
    }
}

fn drain_and_write_refusal(mut stream: TcpStream, status: u16, body: &str) {
    let grace = Duration::from_millis(REFUSAL_READ_GRACE_MILLIS);
    let _ = stream.set_read_timeout(Some(grace));
    let _ = read_http_request_until(&mut stream, Instant::now() + grace, &AtomicBool::new(false));
    if let Err(error) = write_response(&mut stream, status, body) {
        eprintln!("loom refusal response write failed: {error:#}");
    }
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

/// Hands a parsed request to a bounded executor, answering the caller when it cannot be queued.
fn submit_request_job(
    job: RequestJob,
    executor: &BoundedRequestExecutor<RequestJob>,
    runtime: &DaemonRuntime,
) {
    match executor.try_submit(job) {
        Ok(()) => record_request_submission(runtime),
        Err(SubmitError::Full(job)) => {
            let (status, body) = daemon_busy_response();
            write_response_safely(job.stream, status, &body);
        }
        Err(SubmitError::Closed(job)) => {
            let (status, body) = daemon_shutting_down_response();
            write_response_safely(job.stream, status, &body);
        }
    }
}

/// Routes one already-read connection to whichever executor owns it.
///
/// `Stop` means the accept loop should end: either shutdown was observed while this request was in
/// flight, or it was observed before the request could be queued.
fn dispatch_connection(
    ready: ReadyConnection,
    runtime: &DaemonRuntime,
    executor: &Option<BoundedRequestExecutor<RequestJob>>,
    surface_stream_executor: &BoundedRequestExecutor<RequestJob>,
    shutdown_after_read: bool,
) -> DispatchOutcome {
    let ReadyConnection { stream, outcome } = ready;
    match outcome {
        HttpReadOutcome::Empty => {}
        HttpReadOutcome::Rejected { status, body } => {
            write_response_safely(stream, status, &body);
        }
        HttpReadOutcome::Request(request) => {
            let request = ParsedHttpRequest::from_raw(request);
            let job = RequestJob { stream, request };
            if shutdown_after_read && (executor.is_none() || is_reserved_probe(&job.request)) {
                let (status, body) = daemon_shutting_down_response();
                write_response_safely(job.stream, status, &body);
                return DispatchOutcome::Stop;
            }
            match executor.as_ref() {
                None => handle_request_job(job, runtime),
                Some(_) if is_reserved_probe(&job.request) => {
                    handle_parsed_request(job.stream, job.request, runtime);
                }
                Some(_) if is_surface_stream_request(&job.request) => {
                    submit_request_job(job, surface_stream_executor, runtime);
                }
                Some(request_executor) => submit_request_job(job, request_executor, runtime),
            }
        }
    }
    if shutdown_after_read {
        return DispatchOutcome::Stop;
    }
    DispatchOutcome::Continue
}

/// Closes every intake once shutdown has been observed.
///
/// Closing rather than joining: queued work still runs, but nothing new is taken, and the reader
/// stops reading so that shutdown does not wait on a client that may never finish sending.
fn begin_shutdown(
    runtime: &DaemonRuntime,
    executor: &mut Option<BoundedRequestExecutor<RequestJob>>,
    surface_stream_executor: &mut BoundedRequestExecutor<RequestJob>,
    read_draining: &AtomicBool,
) {
    record_shutdown_observed(runtime);
    if let Some(request_executor) = executor.as_mut() {
        request_executor.close();
    }
    surface_stream_executor.close();
    read_draining.store(true, Ordering::SeqCst);
}

fn route_with_runtime(
    runtime: &DaemonRuntime,
    request: &ParsedHttpRequest,
) -> Result<(u16, String)> {
    runtime_log_debug(format!("{} {}", request.method, request.path));
    route(
        request,
        &runtime.hook_settings,
        &runtime.run_store,
        runtime.run_store_status,
        &runtime.brain_planner,
        &runtime.auth_token,
        runtime.config_registry.as_ref(),
        &runtime.config_store,
        &runtime.mcp_servers,
        &runtime.tool_registry,
        &runtime.workflow_store,
        &runtime.hook_bridge,
        &runtime.device_registry,
        &runtime.surface_instances,
        &runtime.surface_actions,
        &runtime.surface_resources,
        &runtime.settings,
        &runtime.shared_images,
        &runtime.ocr_provider,
        &runtime.settings_base_url,
        &runtime.mcp_registry_endpoint,
        runtime.request_executor_status,
        &runtime.canvas_workflow_root,
        &runtime.framework_registry,
        &runtime.control_plane_root,
        &runtime.bundled_art_sha256_allowlist,
    )
}

struct RequestJob {
    stream: TcpStream,
    request: ParsedHttpRequest,
}

#[cfg(test)]
struct SerializedRouteObserver {
    active: AtomicUsize,
    max_active: AtomicUsize,
    entered: (Mutex<bool>, std::sync::Condvar),
    release: (Mutex<bool>, std::sync::Condvar),
}

#[cfg(test)]
struct RequestSubmissionObserver {
    submitted: Mutex<usize>,
    signal: std::sync::Condvar,
}

#[cfg(test)]
struct DaemonShutdownObserver {
    observed: Mutex<bool>,
    signal: std::sync::Condvar,
}

/// Counts connections the serve loop has accepted and handed to a read worker.
///
/// A test that wants shutdown to land after a connection was accepted but before its request was
/// answered cannot get that ordering from a sleep: the sleep either outlasts the accept, in which case
/// it is wasted time, or it does not, in which case the test measures a different code path than the one
/// it names and reports the mismatch as a timeout on the response read.
#[cfg(test)]
struct ConnectionAcceptObserver {
    accepted: Mutex<usize>,
    signal: std::sync::Condvar,
}

#[cfg(test)]
impl RequestSubmissionObserver {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            submitted: Mutex::new(0),
            signal: std::sync::Condvar::new(),
        })
    }

    fn record(&self) {
        let mut submitted = self.submitted.lock().expect("record request submission");
        *submitted += 1;
        self.signal.notify_all();
    }

    fn wait_for_count(&self, expected: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut submitted = self.submitted.lock().expect("read request submissions");
        while *submitted < expected {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, timeout) = self
                .signal
                .wait_timeout(submitted, remaining)
                .expect("wait request submissions");
            submitted = next;
            if timeout.timed_out() && *submitted < expected {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
impl DaemonShutdownObserver {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            observed: Mutex::new(false),
            signal: std::sync::Condvar::new(),
        })
    }

    fn record(&self) {
        *self.observed.lock().expect("record daemon shutdown") = true;
        self.signal.notify_all();
    }

    fn wait_until_observed(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut observed = self.observed.lock().expect("read daemon shutdown");
        while !*observed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, timeout) = self
                .signal
                .wait_timeout(observed, remaining)
                .expect("wait daemon shutdown");
            observed = next;
            if timeout.timed_out() && !*observed {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
impl ConnectionAcceptObserver {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            accepted: Mutex::new(0),
            signal: std::sync::Condvar::new(),
        })
    }

    fn record(&self) {
        let mut accepted = self.accepted.lock().expect("record accepted connection");
        *accepted += 1;
        self.signal.notify_all();
    }

    fn wait_for_count(&self, expected: usize, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut accepted = self.accepted.lock().expect("read accepted connections");
        while *accepted < expected {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, timeout) = self
                .signal
                .wait_timeout(accepted, remaining)
                .expect("wait accepted connections");
            accepted = next;
            if timeout.timed_out() && *accepted < expected {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
struct SerializedRouteObserverGuard {
    observer: Arc<SerializedRouteObserver>,
}

#[cfg(not(test))]
struct SerializedRouteObserverGuard;

#[cfg(test)]
impl SerializedRouteObserver {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            entered: (Mutex::new(false), std::sync::Condvar::new()),
            release: (Mutex::new(false), std::sync::Condvar::new()),
        })
    }

    fn enter(self: &Arc<Self>) -> SerializedRouteObserverGuard {
        let current = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        let mut observed = self.max_active.load(Ordering::SeqCst);
        while observed < current {
            match self.max_active.compare_exchange(
                observed,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(actual) => observed = actual,
            }
        }

        let (entered_lock, entered_signal) = &self.entered;
        *entered_lock.lock().expect("mark serialized route entry") = true;
        entered_signal.notify_all();

        let (release_lock, release_signal) = &self.release;
        let released = release_lock.lock().expect("wait serialized route release");
        let _ = release_signal
            .wait_timeout_while(released, Duration::from_secs(5), |released| !*released)
            .expect("wait serialized route release");

        SerializedRouteObserverGuard {
            observer: Arc::clone(self),
        }
    }

    fn wait_until_entered(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let (entered_lock, entered_signal) = &self.entered;
        let mut entered = entered_lock.lock().expect("read serialized route entry");
        while !*entered {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, timeout) = entered_signal
                .wait_timeout(entered, remaining)
                .expect("wait serialized route entry");
            entered = next;
            if timeout.timed_out() && !*entered {
                return false;
            }
        }
        true
    }

    fn release(&self) {
        let (release_lock, release_signal) = &self.release;
        *release_lock.lock().expect("release serialized route") = true;
        release_signal.notify_all();
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
impl Drop for SerializedRouteObserverGuard {
    fn drop(&mut self) {
        self.observer.active.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(not(test))]
impl Drop for SerializedRouteObserverGuard {
    fn drop(&mut self) {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestConcurrencyClass {
    Concurrent,
    Serialized,
}

fn request_concurrency_class(request: &ParsedHttpRequest) -> RequestConcurrencyClass {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    match (request.method.as_str(), route_path) {
        ("GET", "/health" | "/status" | "/v1/capabilities") => RequestConcurrencyClass::Concurrent,
        ("GET", "/v1/mcp/registry") => RequestConcurrencyClass::Concurrent,
        ("GET", "/v1/hook-bridge/canvas") => RequestConcurrencyClass::Concurrent,
        // A Surface stream long-poll parks for up to five seconds waiting for something to
        // happen. Serialized, it would hold `serialized_route_lock` for that whole time — and the
        // message that would end the poll early arrives over `POST /v1/surfaces/{id}/events`,
        // itself a serialized route, so the idle poll would be blocking the only thing that could
        // release it. The poll reads no state a serialized route is part-way through mutating: it
        // observes the instance store under that store's own lock and returns.
        ("GET", "/v1/surfaces/stream") => RequestConcurrencyClass::Concurrent,
        ("GET", path) if hook_canvas_preview_node_id("GET", path).is_some() => {
            RequestConcurrencyClass::Concurrent
        }
        ("GET", path) if run_path_id(path).is_some() || run_events_path_id(path).is_some() => {
            RequestConcurrencyClass::Concurrent
        }
        ("POST", "/v1/runs") => RequestConcurrencyClass::Concurrent,
        ("POST", path)
            if run_action_path_id(path, "stop").is_some()
                || run_action_path_id(path, "retry").is_some() =>
        {
            RequestConcurrencyClass::Concurrent
        }
        ("POST", "/v1/invoke") => {
            let capability = serde_json::from_str::<Value>(&request.body)
                .ok()
                .and_then(|body| body.get("capability").cloned())
                .and_then(|capability| capability.as_str().map(str::to_owned));
            match capability.as_deref() {
                Some(CAPABILITY_BRAIN_PLAN | CAPABILITY_TEA_TICKET_DECOMPOSE) => {
                    RequestConcurrencyClass::Concurrent
                }
                _ => RequestConcurrencyClass::Serialized,
            }
        }
        _ => RequestConcurrencyClass::Serialized,
    }
}
