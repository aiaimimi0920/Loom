// Daemon limits, public configuration, help/version, and run-store selection.
const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
// Framework ZIPs may bundle standalone runtimes such as embedded Python. The
// JSON API carries ZIP bytes as base64, so package routes need a larger bounded
// limit while ordinary daemon routes retain the conservative 1 MiB ceiling.
const MAX_PACKAGE_HTTP_BODY_BYTES: usize = 32 * 1024 * 1024;
// MCP packages may include a pinned native runtime. Their decoded ZIP remains
// bounded by loom_mcp; this limit accounts for Base64 and JSON transport overhead.
const MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES: usize = 96 * 1024 * 1024;
const MAX_PYTHON_SOURCE_BYTES: u64 = 512 * 1024;
const MAX_ART_JSON_BYTES: u64 = 512 * 1024;
const MAX_SURFACE_SCENE_BYTES: u64 = 1024 * 1024;
const MAX_SURFACE_JAVASCRIPT_BYTES: u64 = 512 * 1024;
const CAPABILITY_BRAIN_PLAN: &str = "brain.plan";
const CAPABILITY_TEA_TICKET_DECOMPOSE: &str = "tea.ticket.decompose.v1";
const CAPABILITY_TEA_TICKET_EXECUTE: &str = "tea.ticket.execute.v1";
const CAPABILITY_TEA_TICKET_REVIEW: &str = "tea.ticket.review.v1";
const DEFAULT_MCP_REGISTRY_ENDPOINT: &str = "https://registry.modelcontextprotocol.io/v0.1/servers";
const MAX_REGISTRY_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MCP_REGISTRY_CACHE_SCHEMA_VERSION: u32 = 1;
const MCP_REGISTRY_CACHE_FRESH_MILLIS: u64 = 15 * 60 * 1000;
const MCP_REGISTRY_CACHE_MAX_ENTRIES: usize = 64;
const MCP_REGISTRY_FETCH_ATTEMPTS: usize = 2;
const MCP_REGISTRY_RETRY_DELAY: Duration = Duration::from_millis(350);
static MCP_REGISTRY_FETCH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const MAX_ART_STORE_CATALOG_BYTES: usize = 4 * 1024 * 1024;
const MAX_ART_STORE_PACKAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_PUBLISHER_DIRECTORY_BYTES: usize = 256 * 1024;
const MAX_BUNDLED_ART_SHA256_ALLOWLIST_ENTRIES: usize = 4096;
const PUBLISHER_IDENTITY_FILE: &str = "publisher-identity.json";
const PUBLISHER_PRIVATE_KEY_CREDENTIAL: &str = "loom-publisher-signing-key";
const DEFAULT_TEST_PUBLISHER_ID: &str = "L0000000000";
pub const DAEMON_AUTH_TOKEN_FILE: &str = "daemon-token";
const ADMIN_AUTH_COOKIE_NAME: &str = "loom_admin";
#[cfg(test)]
const TEST_DAEMON_AUTH_TOKEN: &str = "loom-daemon-test-admin";

fn invokable_capability_ids() -> [&'static str; 4] {
    [
        CAPABILITY_BRAIN_PLAN,
        CAPABILITY_TEA_TICKET_DECOMPOSE,
        CAPABILITY_TEA_TICKET_EXECUTE,
        CAPABILITY_TEA_TICKET_REVIEW,
    ]
}

pub fn daemon_help_text() -> &'static str {
    concat!(
        "Usage: loom-daemon [OPTIONS]\n",
        "\n",
        "Options:\n",
        "  -h, --help       Print help\n",
        "  -V, --version    Print version\n",
        "  --manifest-dir <DIR>  Write loom.json discovery manifest to DIR\n",
        "\n",
        "Environment:\n",
        "  LOOM_DAEMON_HOST     Bind host [default: 127.0.0.1]\n",
        "  LOOM_DAEMON_PORT     Bind port [default: 8765]\n",
        "  LOOM_DAEMON_WORKERS  Request worker threads [default: 4]\n",
        "  LOOM_DAEMON_QUEUE_CAPACITY  Queued requests [default: 32]\n",
        "  LOOM_DAEMON_TOKEN    Administrator bearer token; generated and persisted when omitted\n",
        "  LOOM_TLS_TERMINATED  Set to 1 only behind an authenticated TLS terminator for non-loopback binds\n",
        "  LOOM_BUNDLED_ART_SHA256_ALLOWLIST  Comma-separated packaged Art digests supplied by Loom desktop\n",
        "  LOOM_CAPABILITY_MANIFEST_DIR  Directory for loom.json discovery manifest\n",
        "  LOOM_RUN_STORE_PATH  SQLite run evidence path [default: <control-plane>\\runs\\loom-runs.sqlite3]\n",
        "  LOOM_MCP_REGISTRY_ENDPOINT  MCP Registry endpoint override\n",
        "  LOOM_GATEWAY_MODEL     Enable Gateway-backed brain.plan with this model\n",
        "  LOOM_GATEWAY_BASE_URL  Gateway origin [default: http://127.0.0.1:4200]\n",
        "  LOOM_GATEWAY_TOKEN     Optional Gateway bearer token\n",
        "  LOOM_GATEWAY_TIMEOUT_SECS  Gateway request timeout [default: 60]\n",
        "\n",
        "HTTP API:\n",
        "  GET  /health\n",
        "  GET  /status\n",
        "  GET  /v1/capabilities\n",
        "  GET  /v1/surfaces/instances\n",
        "  POST /v1/surfaces/instances\n",
        "  POST /v1/surfaces/attach\n",
        "  GET  /v1/surfaces/stream\n",
        "  GET  /v1/surfaces/instances/{instanceId}\n",
        "  DELETE /v1/surfaces/instances/{instanceId}\n",
        "  POST /v1/surfaces/instances/{instanceId}/attachments\n",
        "  PUT  /v1/surfaces/instances/{instanceId}/snapshot\n",
        "  POST /v1/surfaces/instances/{instanceId}/patch\n",
        "  POST /v1/surfaces/instances/{instanceId}/generation\n",
        "  POST /v1/surfaces/instances/{instanceId}/preview\n",
        "  POST /v1/surfaces/instances/{instanceId}/result\n",
        "  POST /v1/surfaces/instances/{instanceId}/failure\n",
        "  POST /v1/surfaces/instances/{instanceId}/events\n",
        "  POST /v1/surfaces/instances/{instanceId}/migrate\n",
        "  POST /v1/surfaces/instances/{instanceId}/mount\n",
        "  POST /v1/surfaces/actions/cancel\n",
        "  POST /v1/surfaces/confirmations/decision\n",
        "  POST /v1/device-sessions/challenges\n",
        "  POST /v1/device-sessions\n",
        "  POST /v1/invoke\n",
        "  GET  /v1/mcp/servers\n",
        "  GET  /v1/mcp/registry\n",
        "  POST /v1/mcp/test\n",
        "  POST /v1/mcp/call\n",
        "  POST /v1/mcp/package/check\n",
        "  POST /v1/mcp/package/install-plan\n",
        "  PUT  /v1/mcp/servers/{serverId}\n",
        "  DELETE /v1/mcp/servers/{serverId}\n",
        "  GET  /v1/tools\n",
        "  PUT  /v1/tools/{toolId}\n",
        "  DELETE /v1/tools/{toolId}\n",
        "  POST /v1/tools/{toolId}/execute\n",
        "  GET  /v1/tools/enabled\n",
        "  POST /v1/tools/{toolId}/enable\n",
        "  POST /v1/tools/{toolId}/disable\n",
        "  PUT  /v1/tools/{toolId}/defaults\n",
        "  GET  /v1/art-authoring/python/status\n",
        "  GET  /v1/art-authoring/python/arts\n",
        "  POST /v1/art-authoring/source/read\n",
        "  POST /v1/art-authoring/source/read-art-json\n",
        "  POST /v1/art-authoring/source/check-art-json\n",
        "  POST /v1/art-authoring/source/infer-ports\n",
        "  POST /v1/shared-memory/buffers\n",
        "  GET  /v1/shared-memory/buffers\n",
        "  GET  /v1/shared-memory/buffers/{handle}\n",
        "  DELETE /v1/shared-memory/buffers/{handle}\n",
        "  GET  /v1/workflows\n",
        "  GET  /v1/workflows/{workflowId}\n",
        "  PUT  /v1/workflows/{workflowId}\n",
        "  DELETE /v1/workflows/{workflowId}\n",
        "  GET  /v1/hook-bridge/status\n",
        "  GET  /v1/hook-bridge/session\n",
        "  GET  /v1/hook-bridge/canvas\n",
        "  PUT  /v1/hook-bridge/canvas/workflows/{workflowId}\n",
        "  GET  /v1/hook-bridge/canvas/nodes/{nodeId}/preview\n",
        "  POST /v1/hook-bridge/start\n",
        "  POST /v1/hook-bridge/stop\n",
        "  POST /v1/hook-bridge/workflows/instantiate\n",
        "  POST /v1/hook-bridge/workflows/nodes/update\n",
        "  POST /v1/hook-bridge/cache-control\n",
        "  GET  /v1/settings\n",
        "  PUT  /v1/settings\n",
        "  GET  /v1/settings/shortcuts\n",
        "  PUT  /v1/settings/shortcuts/{shortcutId}\n",
        "  GET  /v1/runtime/app-paths\n",
        "  GET  /v1/runtime/autostart\n",
        "  POST /v1/runtime/autostart\n",
        "  POST /v1/runtime/minimize-to-tray\n",
        "  POST /v1/image-helpers/convert\n",
        "  GET  /v1/runs/{runId}\n",
        "  GET  /v1/runs/{runId}/events\n",
    )
}

pub fn daemon_version_text() -> String {
    format!("loom-daemon {}", loom_core::LOOM_VERSION)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RunStoreConfig {
    Memory,
    Sqlite(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonConfig {
    host: String,
    port: u16,
    hook_settings: HookSettings,
    auth_token: Option<String>,
    tls_terminated: bool,
    manifest_dir: Option<PathBuf>,
    mcp_registry_endpoint: String,
    brain_planner: BrainPlannerConfig,
    run_store: RunStoreConfig,
    request_executor: RequestExecutorConfig,
    bundled_art_sha256_allowlist: BTreeSet<String>,
    surface_resource_gc_min_age_ms: u64,
    control_plane_root: Option<PathBuf>,
    configuration_root: Option<PathBuf>,
}

impl DaemonConfig {
    #[must_use]
    pub fn bind_host(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            hook_settings: HookSettings::default(),
            auth_token: {
                #[cfg(test)]
                {
                    Some(TEST_DAEMON_AUTH_TOKEN.to_owned())
                }
                #[cfg(not(test))]
                {
                    None
                }
            },
            tls_terminated: false,
            manifest_dir: None,
            mcp_registry_endpoint: std::env::var("LOOM_MCP_REGISTRY_ENDPOINT")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_MCP_REGISTRY_ENDPOINT.to_owned()),
            brain_planner: BrainPlannerConfig::LocalTemplate,
            run_store: RunStoreConfig::Memory,
            request_executor: RequestExecutorConfig::Inline,
            bundled_art_sha256_allowlist: BTreeSet::new(),
            surface_resource_gc_min_age_ms: DEFAULT_RESOURCE_GC_MIN_AGE_MILLIS,
            control_plane_root: None,
            configuration_root: None,
        }
    }

    #[must_use]
    pub fn localhost(port: u16) -> Self {
        Self::bind_host("127.0.0.1", port)
    }

    #[must_use]
    pub fn with_hook_settings(mut self, hook_settings: HookSettings) -> Self {
        self.hook_settings = hook_settings;
        self
    }

    #[must_use]
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        let token = token.into();
        if !token.trim().is_empty() {
            self.auth_token = Some(token);
        }
        self
    }

    #[must_use]
    pub fn with_tls_termination(mut self, tls_terminated: bool) -> Self {
        self.tls_terminated = tls_terminated;
        self
    }

    #[must_use]
    pub fn with_tls_termination_from_env(self) -> Self {
        let enabled = std::env::var("LOOM_TLS_TERMINATED")
            .ok()
            .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"));
        self.with_tls_termination(enabled)
    }

    #[must_use]
    pub fn with_manifest_dir(mut self, manifest_dir: impl Into<PathBuf>) -> Self {
        self.manifest_dir = Some(manifest_dir.into());
        self
    }

    #[must_use]
    pub fn with_mcp_registry_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        let endpoint = endpoint.into();
        if !endpoint.trim().is_empty() {
            self.mcp_registry_endpoint = endpoint;
        }
        self
    }

    #[must_use]
    pub(crate) fn with_brain_planner(mut self, brain_planner: BrainPlannerConfig) -> Self {
        self.brain_planner = brain_planner;
        self
    }

    pub fn with_brain_planner_from_env(self) -> Result<Self> {
        let brain_planner =
            BrainPlannerConfig::from_env().map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(self.with_brain_planner(brain_planner))
    }

    #[must_use]
    pub fn with_bounded_request_executor(mut self, workers: usize, queue_capacity: usize) -> Self {
        self.request_executor = RequestExecutorConfig::bounded(workers, queue_capacity)
            .expect("bounded request executor configuration must be valid");
        self
    }

    pub fn with_request_executor_from_env(mut self) -> Result<Self> {
        self.request_executor = RequestExecutorConfig::from_env()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(self)
    }

    #[must_use]
    pub fn with_sqlite_run_store(mut self, path: impl Into<PathBuf>) -> Self {
        self.run_store = RunStoreConfig::Sqlite(path.into());
        self
    }

    #[must_use]
    pub fn with_surface_resource_gc_min_age_ms(mut self, min_age_ms: u64) -> Self {
        self.surface_resource_gc_min_age_ms = min_age_ms;
        self
    }

    #[must_use]
    pub fn with_control_plane_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.control_plane_root = Some(root.into());
        self
    }

    #[must_use]
    pub fn with_configuration_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.configuration_root = Some(root.into());
        self
    }

    pub fn with_bundled_art_sha256_allowlist<I, S>(mut self, values: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut allowlist = BTreeSet::new();
        for value in values {
            let value = value.as_ref().trim();
            if value.is_empty() {
                continue;
            }
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                anyhow::bail!("invalid bundled Art SHA-256 allowlist entry `{value}`");
            }
            allowlist.insert(value.to_ascii_lowercase());
            if allowlist.len() > MAX_BUNDLED_ART_SHA256_ALLOWLIST_ENTRIES {
                anyhow::bail!(
                    "bundled Art SHA-256 allowlist exceeds {MAX_BUNDLED_ART_SHA256_ALLOWLIST_ENTRIES} entries"
                );
            }
        }
        self.bundled_art_sha256_allowlist = allowlist;
        Ok(self)
    }
}

#[must_use]
pub fn default_run_store_path() -> PathBuf {
    std::env::var_os("LOOM_RUN_STORE_PATH")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| {
            default_control_plane_root()
                .join("runs")
                .join("loom-runs.sqlite3")
        })
}

struct DaemonRuntime {
    hook_settings: HookSettings,
    run_store: SharedRunStore,
    auth_token: String,
    config_registry: Arc<ConfigRegistry>,
    config_store: FileDocumentStore,
    mcp_servers: SharedMcpServerStore,
    tool_registry: ToolRegistry,
    workflow_store: WorkflowStore,
    // Root for frozen Hook-canvas workflow snapshots: <root>/canvas-workflows/<id>/
    // holds snapshot.json plus an images/ dir of that workflow's node previews.
    canvas_workflow_root: PathBuf,
    // Tracks which art execution frameworks the user has installed.
    framework_registry: FrameworkRegistry,
    // Control-plane root, for the art install dir (<root>/arts/<id>/).
    control_plane_root: PathBuf,
    bundled_art_sha256_allowlist: BTreeSet<String>,
    hook_bridge: SharedHookBridgeRuntime,
    device_registry: SharedDeviceRegistryStore,
    surface_instances: SharedSurfaceInstanceStore,
    surface_actions: SharedSurfaceActionExecutor,
    surface_resources: SharedSurfaceResourceStore,
    settings: SharedLoomSettingsStore,
    shared_images: SharedImageStoreHandle,
    ocr_provider: OcrProviderHandle,
    settings_base_url: String,
    mcp_registry_endpoint: String,
    brain_planner: SharedBrainPlanner,
    run_store_status: RunStoreStatus,
    request_executor_status: RequestExecutorStatus,
    serialized_route_lock: Mutex<()>,
    #[cfg(test)]
    serialized_route_observer: Option<Arc<SerializedRouteObserver>>,
    #[cfg(test)]
    request_submission_observer: Option<Arc<RequestSubmissionObserver>>,
    #[cfg(test)]
    shutdown_observer: Option<Arc<DaemonShutdownObserver>>,
    #[cfg(test)]
    connection_accept_observer: Option<Arc<ConnectionAcceptObserver>>,
}
