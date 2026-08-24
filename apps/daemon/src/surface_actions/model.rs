// Surface action constants, jobs, coordinator state, and executor data.
const SURFACE_ACTION_WORKERS: usize = 4;
const SURFACE_ACTION_QUEUE_CAPACITY: usize = 64;
const DEFAULT_SURFACE_ACTION_TIMEOUT_MILLIS: u64 = 30_000;
const MAX_SURFACE_ACTION_TIMEOUT_MILLIS: u64 = 120_000;
const SURFACE_ACTION_POLL_MILLIS: u64 = 20;
/// How long a worker waits for an abandoned runner thread to finish before it gives up on
/// reclaiming it. A cancelled or timed-out job must not release its `Serial` lock while the runner
/// it started is still executing, but a thread cannot be stopped from the outside, so the wait has
/// to end somewhere. The runner is bounded by the same timeout as the worker's polling loop, so it
/// normally returns within a poll or two of the loop breaking.
const SURFACE_ACTION_RUNNER_REAP_MILLIS: u64 = 5_000;
/// How many times a submit re-reads the store after resolving a package before it gives up.
///
/// The resolve happens with no lock held, so the instance can be migrated to a different package
/// underneath it. That is rare and self-clearing, but a caller that retried forever would spin
/// against a migration loop, so the attempts are bounded and the last one reports a conflict.
const SURFACE_ACTION_PREPARE_ATTEMPTS: usize = 3;
/// How many Surface manifests are cached before the cache is dropped wholesale.
///
/// Entries are keyed by locked package identity, so an entry can never go stale — it can only pile
/// up as instances migrate to new Art versions. Clearing past the cap keeps the bound without
/// pretending to know which entry is the coldest.
const SURFACE_MANIFEST_CACHE_LIMIT: usize = 64;

pub(crate) type SharedSurfaceActionExecutor = Arc<SurfaceActionExecutor>;

#[derive(Clone)]
struct SurfaceActionJob {
    event: SurfaceEvent,
    ack: SurfaceActionAck,
    action: SurfaceActionDefinition,
    tool: ToolDefinition,
    invocation: SurfaceActionInvocation,
    cancellation: Arc<AtomicBool>,
}

#[derive(Default)]
struct SurfaceActionCoordinator {
    latest_requests: BTreeMap<String, String>,
    reject_reservations: BTreeSet<String>,
    // Weak entries preserve one mutex while any worker holds or waits on it. Dead keys are pruned
    // when the next serial action starts, so instance/action churn cannot grow this table forever.
    serial_locks: BTreeMap<String, Weak<Mutex<()>>>,
    cancellation_tokens: BTreeMap<String, Arc<AtomicBool>>,
}

type SurfaceActionRunner =
    dyn Fn(&SurfaceActionJob) -> Result<Value, SurfaceExecutionError> + Send + Sync + 'static;
type SurfaceToolResolver = dyn Fn(&loom_protocol::SurfaceInstanceDescriptor) -> Result<ToolDefinition, SurfaceStoreError>
    + Send
    + Sync
    + 'static;

pub(crate) struct SurfaceActionExecutor {
    queue: BoundedRequestExecutor<SurfaceActionJob>,
    coordinator: Arc<Mutex<SurfaceActionCoordinator>>,
    surface_instances: SharedSurfaceInstanceStore,
    tool_resolver: Arc<SurfaceToolResolver>,
    /// Surface manifests by locked package identity, so a burst of events against one instance parses
    /// the manifest once instead of once per event. Guarded by its own mutex: it must not be reachable
    /// only while the Surface store lock is held, which is the whole point of caching it.
    manifest_cache: Mutex<BTreeMap<String, Arc<SurfacePackageManifest>>>,
    hook_bridge: SharedHookBridgeRuntime,
}
