use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use loom_configuration::{
    built_in_registry, default_configuration_root, render_app_settings_page, render_settings_index,
    ConfigRegistry, FileDocumentStore, ManagedAppId, ManagedAppSet, ManagedConfigError,
    ManagedConfigErrorCode,
};
use loom_durable::{
    InMemoryRunEvidenceStore, RunEventDraft, RunEvidenceStore, RunStoreError, RunStoreStatus,
    SqliteRunEvidenceStore,
};
use loom_hook_bridge::{
    ahrp_error_response, ahrp_process_base64_success_response,
    ahrp_process_shared_memory_success_response, arts_updated_broadcast,
    execute_art_node_error_response, execute_art_node_image_success_response,
    execute_art_node_success_response, extract_ahrp_base64_output, extract_execution_text_content,
    handle_request as handle_hook_bridge_request, instantiate_workflow_broadcast,
    legacy_method_names, ocr_image_error_response, ocr_image_success_response, parse_request,
    HookBridgeRequest, HookBridgeRuntimeInput, HOOK_BRIDGE_PORT,
};
use loom_hooks::{HookSettings, HookSettingsSummary};
use loom_mcp::{McpServerConfig, StdioMcpClient};
use loom_shared_image::{SharedImageError, SharedImageFormat, SharedImageInfo, SharedImageStore};
use loom_tool_registry::{
    execute_tool, framework::FrameworkRegistry, ToolDefinition, ToolExecution, ToolRegistry,
    ToolRegistryError, WorkflowExecutionBindings,
};
use loom_workflow_runtime::{execute_tool_with_workflows, WorkflowRuntimeError};
use loom_workflow_store::{WorkflowStore, WorkflowStoreError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod brain_plan;
mod hook_canvas;
mod request_executor;

use brain_plan::{
    build_brain_planner, BrainPlanRequest, BrainPlannerConfig, BrainPlannerStatus,
    SharedBrainPlanner,
};
use request_executor::{
    BoundedRequestExecutor, RequestExecutorConfig, RequestExecutorStatus, SubmitError,
};

const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
const MAX_PYTHON_SOURCE_BYTES: u64 = 512 * 1024;
const MAX_ART_JSON_BYTES: u64 = 512 * 1024;
const CAPABILITY_BRAIN_PLAN: &str = "brain.plan";
const CAPABILITY_TEA_TICKET_DECOMPOSE: &str = "tea.ticket.decompose.v1";
const CAPABILITY_TEA_TICKET_EXECUTE: &str = "tea.ticket.execute.v1";
const CAPABILITY_TEA_TICKET_REVIEW: &str = "tea.ticket.review.v1";
const DEFAULT_MCP_REGISTRY_ENDPOINT: &str = "https://registry.modelcontextprotocol.io/v0/servers";

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
        "  LOOM_DAEMON_TOKEN    Bearer token; required for non-loopback binds\n",
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
        "  POST /v1/invoke\n",
        "  GET  /v1/mcp/servers\n",
        "  GET  /v1/mcp/registry\n",
        "  POST /v1/mcp/test\n",
        "  POST /v1/mcp/package/check\n",
        "  POST /v1/mcp/package/install-plan\n",
        "  PUT  /v1/mcp/servers/{serverId}\n",
        "  DELETE /v1/mcp/servers/{serverId}\n",
        "  GET  /v1/tools\n",
        "  PUT  /v1/tools/{toolId}\n",
        "  DELETE /v1/tools/{toolId}\n",
        "  POST /v1/tools/{toolId}/execute\n",
        "  POST /v1/artloom-compat/mcp/call-tool\n",
        "  GET  /v1/artloom-compat/mcp/registry\n",
        "  GET  /v1/artloom-compat/mcp/servers\n",
        "  POST /v1/artloom-compat/mcp/servers\n",
        "  DELETE /v1/artloom-compat/mcp/servers/{serverId}\n",
        "  GET  /v1/artloom-compat/arts\n",
        "  GET  /v1/artloom-compat/arts/enabled\n",
        "  GET  /v1/artloom-compat/user-arts\n",
        "  GET  /v1/artloom-compat/arts/{artId}\n",
        "  POST /v1/artloom-compat/arts/sync\n",
        "  POST /v1/artloom-compat/arts/broadcast-updated\n",
        "  POST /v1/artloom-compat/native/process-art\n",
        "  POST /v1/artloom-compat/arts/{artId}/enable\n",
        "  POST /v1/artloom-compat/arts/{artId}/disable\n",
        "  PUT  /v1/artloom-compat/arts/{artId}/defaults\n",
        "  GET  /v1/python-arts\n",
        "  GET  /v1/python-arts/{artId}\n",
        "  GET  /v1/python-arts/engine/status\n",
        "  POST /v1/artloom-compat/python/execute-art\n",
        "  POST /v1/artloom-compat/python/process-image\n",
        "  GET  /v1/artloom-compat/python/installed-arts\n",
        "  POST /v1/artloom-compat/python/read-art-json\n",
        "  POST /v1/artloom-compat/python/read-python-file\n",
        "  POST /v1/artloom-compat/python/check-art-json-nearby\n",
        "  POST /v1/python-arts/shader/prefetch\n",
        "  POST /v1/python-arts/source/read\n",
        "  POST /v1/python-arts/source/read-art-json\n",
        "  POST /v1/python-arts/source/check-art-json\n",
        "  POST /v1/python-arts/source/infer-ports\n",
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
        "  GET  /v1/artloom-compat/settings\n",
        "  PUT  /v1/artloom-compat/settings\n",
        "  GET  /v1/artloom-compat/shortcuts\n",
        "  PUT  /v1/artloom-compat/shortcuts/{shortcutId}\n",
        "  GET  /v1/artloom-compat/app-paths\n",
        "  GET  /v1/artloom-compat/ipc/status\n",
        "  POST /v1/artloom-compat/ipc/instantiate-workflow\n",
        "  POST /v1/artloom-compat/ipc/update-workflow-node\n",
        "  POST /v1/artloom-compat/ipc/execute-art-node\n",
        "  GET  /v1/artloom-compat/workflows\n",
        "  PUT  /v1/artloom-compat/workflows/{workflowId}/metadata\n",
        "  PUT  /v1/artloom-compat/workflows/{workflowId}/data\n",
        "  GET  /v1/artloom-compat/workflows/{workflowId}/data\n",
        "  DELETE /v1/artloom-compat/workflows/{workflowId}/data\n",
        "  GET  /v1/artloom-compat/system/autostart\n",
        "  POST /v1/artloom-compat/system/autostart\n",
        "  POST /v1/artloom-compat/system/autostart/enable\n",
        "  POST /v1/artloom-compat/system/autostart/disable\n",
        "  POST /v1/artloom-compat/system/minimize-to-tray\n",
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
    manifest_dir: Option<PathBuf>,
    mcp_registry_endpoint: String,
    brain_planner: BrainPlannerConfig,
    run_store: RunStoreConfig,
    request_executor: RequestExecutorConfig,
}

impl DaemonConfig {
    #[must_use]
    pub fn bind_host(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            hook_settings: HookSettings::default(),
            auth_token: None,
            manifest_dir: None,
            mcp_registry_endpoint: std::env::var("LOOM_MCP_REGISTRY_ENDPOINT")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_MCP_REGISTRY_ENDPOINT.to_owned()),
            brain_planner: BrainPlannerConfig::LocalTemplate,
            run_store: RunStoreConfig::Memory,
            request_executor: RequestExecutorConfig::Inline,
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
    auth_token: Option<String>,
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
    hook_bridge: SharedHookBridgeRuntime,
    artloom_settings: SharedArtLoomCompatSettingsStore,
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
}

pub struct LoomDaemon {
    listener: TcpListener,
    runtime: Arc<DaemonRuntime>,
    request_executor: RequestExecutorConfig,
}

impl LoomDaemon {
    pub fn bind(config: DaemonConfig) -> Result<Self> {
        if config.auth_token.is_none() && !is_loopback_bind_host(&config.host) {
            anyhow::bail!(
                "loom daemon auth token is required when binding non-loopback host {}",
                config.host
            );
        }
        if config.manifest_dir.is_some() && !is_loopback_bind_host(&config.host) {
            anyhow::bail!(
                "loom discovery manifest requires a loopback bind host, got {}",
                config.host
            );
        }
        let brain_planner = build_brain_planner(config.brain_planner)?;
        let listener = TcpListener::bind((config.host.as_str(), config.port))
            .with_context(|| format!("bind loom daemon to {}:{}", config.host, config.port))?;
        listener
            .set_nonblocking(true)
            .context("set daemon listener nonblocking")?;
        let local_addr = listener
            .local_addr()
            .context("read daemon local addr for manifest")?;
        let settings_base_url = std::env::var("LOOM_SETTINGS_BASE_URL")
            .unwrap_or_else(|_| format!("http://{local_addr}/settings"));
        let config_root = std::env::var_os("LOOM_CONFIGURATION_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(default_configuration_root);
        let control_plane_root = std::env::var_os("LOOM_CONTROL_PLANE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(default_control_plane_root);
        let framework_runtime_root = control_plane_root.join("frameworks");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &control_plane_root);
        std::env::set_var("LOOM_FRAMEWORK_RUNTIMES_DIR", &framework_runtime_root);
        let mut run_store: Box<dyn RunEvidenceStore> = match &config.run_store {
            RunStoreConfig::Memory => Box::new(InMemoryRunEvidenceStore::default()),
            RunStoreConfig::Sqlite(path) => {
                Box::new(SqliteRunEvidenceStore::open(path).map_err(|error| {
                    anyhow::anyhow!("open Loom run store `{}`: {error}", path.display())
                })?)
            }
        };
        run_store
            .recover_interrupted_runs()
            .map_err(|error| anyhow::anyhow!("recover Loom run store: {error}"))?;
        let run_store_status = run_store.status();
        if let Some(manifest_dir) = config.manifest_dir.as_deref() {
            write_local_capability_manifest(
                manifest_dir,
                local_addr,
                config.auth_token.as_deref(),
            )?;
        }
        let request_executor = config.request_executor;
        let runtime = DaemonRuntime {
            hook_settings: config.hook_settings,
            run_store: Arc::new(Mutex::new(run_store)),
            auth_token: config.auth_token,
            config_registry: Arc::new(built_in_registry()),
            config_store: FileDocumentStore::new(config_root),
            mcp_servers: Arc::new(Mutex::new(load_persisted_mcp_servers(&control_plane_root))),
            tool_registry: ToolRegistry::new(control_plane_root.join("tools")),
            workflow_store: WorkflowStore::new(control_plane_root.join("workflows")),
            canvas_workflow_root: control_plane_root.join("canvas-workflows"),
            framework_registry: FrameworkRegistry::new(&control_plane_root),
            control_plane_root: control_plane_root.to_path_buf(),
            hook_bridge: Arc::new(Mutex::new(HookBridgeRuntime::new(
                control_plane_root.join("workflows"),
            ))),
            artloom_settings: Arc::new(Mutex::new(ArtLoomCompatSettingsStore::new(
                control_plane_root
                    .join("settings")
                    .join("artloom-compat-settings.json"),
            ))),
            shared_images: Arc::new(Mutex::new(SharedImageStore::new())),
            ocr_provider: Arc::new(Mutex::new(OcrProvider::from_env())),
            settings_base_url,
            mcp_registry_endpoint: config.mcp_registry_endpoint,
            brain_planner,
            run_store_status,
            request_executor_status: request_executor.status(),
            serialized_route_lock: Mutex::new(()),
            #[cfg(test)]
            serialized_route_observer: None,
            #[cfg(test)]
            request_submission_observer: None,
            #[cfg(test)]
            shutdown_observer: None,
        };
        Ok(Self {
            listener,
            runtime: Arc::new(runtime),
            request_executor,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.listener.local_addr().context("read daemon local addr")
    }

    pub fn serve_until(self, shutdown: Receiver<()>) -> Result<()> {
        let worker_runtime = Arc::clone(&self.runtime);
        let mut executor = match self.request_executor {
            RequestExecutorConfig::Inline => None,
            RequestExecutorConfig::Bounded {
                workers,
                queue_capacity,
            } => Some(BoundedRequestExecutor::new(
                "loom-request",
                workers,
                queue_capacity,
                move |job: RequestJob| handle_request_job(job, &worker_runtime),
            )?),
        };

        let serve_result: Result<()> = loop {
            if shutdown.try_recv().is_ok() {
                record_shutdown_observed(&self.runtime);
                if let Some(request_executor) = executor.as_mut() {
                    request_executor.close();
                }
                break Ok(());
            }

            match self.listener.accept() {
                Ok((stream, _)) => {
                    let Some((stream, outcome)) = read_connection(stream) else {
                        continue;
                    };
                    let shutdown_after_read = shutdown.try_recv().is_ok();
                    if shutdown_after_read {
                        record_shutdown_observed(&self.runtime);
                        if let Some(request_executor) = executor.as_mut() {
                            request_executor.close();
                        }
                    }
                    match outcome {
                        HttpReadOutcome::Empty => {}
                        HttpReadOutcome::Rejected { status, body } => {
                            write_response_safely(stream, status, &body);
                        }
                        HttpReadOutcome::Request(request) => {
                            let request = ParsedHttpRequest::from_raw(&request);
                            let job = RequestJob { stream, request };
                            if shutdown_after_read
                                && (executor.is_none() || is_reserved_probe(&job.request))
                            {
                                let (status, body) = daemon_shutting_down_response();
                                write_response_safely(job.stream, status, &body);
                                break Ok(());
                            }
                            if executor.is_none() {
                                handle_request_job(job, &self.runtime);
                                continue;
                            }
                            if is_reserved_probe(&job.request) {
                                handle_parsed_request(job.stream, job.request, &self.runtime);
                                continue;
                            }
                            let request_executor =
                                executor.as_ref().expect("bounded executor is available");
                            match request_executor.try_submit(job) {
                                Ok(()) => record_request_submission(&self.runtime),
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
                    }
                    if shutdown_after_read {
                        break Ok(());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => break Err(error).context("accept daemon connection"),
            }
        };

        let shutdown_result = executor
            .as_mut()
            .map(BoundedRequestExecutor::shutdown)
            .transpose();
        if let Err(error) = serve_result {
            let _ = shutdown_result;
            return Err(error);
        }
        shutdown_result.context("shutdown Loom request executor")?;
        Ok(())
    }
}

fn read_connection(mut stream: TcpStream) -> Option<(TcpStream, HttpReadOutcome)> {
    if let Err(error) = stream.set_nonblocking(false) {
        eprintln!("loom connection setup failed: {error}");
        return None;
    }
    if let Err(error) = stream.set_read_timeout(Some(Duration::from_secs(2))) {
        eprintln!("loom connection read-timeout setup failed: {error}");
        return None;
    }
    match read_http_request(&mut stream) {
        Ok(outcome) => Some((stream, outcome)),
        Err(error) => {
            eprintln!("loom request read failed: {error:#}");
            None
        }
    }
}

fn route_with_runtime(
    runtime: &DaemonRuntime,
    request: &ParsedHttpRequest,
) -> Result<(u16, String)> {
    route(
        request,
        &runtime.hook_settings,
        &runtime.run_store,
        runtime.run_store_status,
        &runtime.brain_planner,
        runtime.auth_token.as_deref(),
        runtime.config_registry.as_ref(),
        &runtime.config_store,
        &runtime.mcp_servers,
        &runtime.tool_registry,
        &runtime.workflow_store,
        &runtime.hook_bridge,
        &runtime.artloom_settings,
        &runtime.shared_images,
        &runtime.ocr_provider,
        &runtime.settings_base_url,
        &runtime.mcp_registry_endpoint,
        runtime.request_executor_status,
        &runtime.canvas_workflow_root,
        &runtime.framework_registry,
        &runtime.control_plane_root,
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
        ("GET", "/v1/hook-bridge/canvas") => RequestConcurrencyClass::Concurrent,
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

fn is_reserved_probe(request: &ParsedHttpRequest) -> bool {
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    request.method == "GET" && matches!(route_path, "/health" | "/status")
}

enum RouteResponse {
    Text {
        status: u16,
        body: String,
    },
    Binary {
        status: u16,
        content_type: &'static str,
        body: Vec<u8>,
    },
}

fn route_request(runtime: &DaemonRuntime, request: &ParsedHttpRequest) -> RouteResponse {
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some((workflow_id, node_id)) =
            canvas_workflow_preview_ids(&request.method, &request.path)
        {
            if let Some(token) = runtime.auth_token.as_deref() {
                if !request.has_bearer(token) {
                    return structured_error(
                        401,
                        json!({
                            "code": "unauthorized",
                            "message": "missing or invalid Loom bearer token",
                        }),
                    )
                    .map(|(status, body)| RouteResponse::Text { status, body });
                }
            }
            return canvas_workflow_preview_response(
                &workflow_id,
                &node_id,
                &runtime.canvas_workflow_root,
            );
        }
        if let Some(node_id) = hook_canvas_preview_node_id(&request.method, &request.path) {
            if let Some(token) = runtime.auth_token.as_deref() {
                if !request.has_bearer(token) {
                    return structured_error(
                        401,
                        json!({
                            "code": "unauthorized",
                            "message": "missing or invalid Loom bearer token",
                        }),
                    )
                    .map(|(status, body)| RouteResponse::Text { status, body });
                }
            }
            return hook_canvas_preview_response(&node_id);
        }
        route_with_runtime(runtime, request)
            .map(|(status, body)| RouteResponse::Text { status, body })
    }));
    match response {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            eprintln!("loom request routing failed: {error:#}");
            let (status, body) = request_worker_failed_response();
            RouteResponse::Text { status, body }
        }
        Err(_) => {
            eprintln!("loom request worker panicked");
            let (status, body) = request_worker_failed_response();
            RouteResponse::Text { status, body }
        }
    }
}

fn write_response_safely(mut stream: TcpStream, status: u16, body: &str) {
    if let Err(error) = write_response(&mut stream, status, body) {
        eprintln!("loom response write failed: {error:#}");
    }
}

fn write_route_response_safely(mut stream: TcpStream, response: RouteResponse) {
    let result = match response {
        RouteResponse::Text { status, body } => write_response(&mut stream, status, &body),
        RouteResponse::Binary {
            status,
            content_type,
            body,
        } => write_binary_response(&mut stream, status, content_type, &body),
    };
    if let Err(error) = result {
        eprintln!("loom response write failed: {error:#}");
    }
}

fn handle_parsed_request(stream: TcpStream, request: ParsedHttpRequest, runtime: &DaemonRuntime) {
    write_route_response_safely(stream, route_request(runtime, &request));
}

fn handle_request_job(job: RequestJob, runtime: &DaemonRuntime) {
    let RequestJob { stream, request } = job;
    let response = match request_concurrency_class(&request) {
        RequestConcurrencyClass::Concurrent => route_request(runtime, &request),
        RequestConcurrencyClass::Serialized => {
            let route_guard = match runtime.serialized_route_lock.lock() {
                Ok(route_guard) => route_guard,
                Err(_) => {
                    eprintln!("loom serialized route lock is poisoned");
                    let (status, body) = request_worker_failed_response();
                    write_response_safely(stream, status, &body);
                    return;
                }
            };
            let observer_guard = serialized_route_observer_guard(runtime);
            let response = route_request(runtime, &request);
            drop(observer_guard);
            drop(route_guard);
            response
        }
    };
    write_route_response_safely(stream, response);
}

fn record_shutdown_observed(runtime: &DaemonRuntime) {
    #[cfg(test)]
    if let Some(observer) = runtime.shutdown_observer.as_ref() {
        observer.record();
    }
    #[cfg(not(test))]
    let _ = runtime;
}

#[cfg(test)]
fn serialized_route_observer_guard(
    runtime: &DaemonRuntime,
) -> Option<SerializedRouteObserverGuard> {
    runtime
        .serialized_route_observer
        .as_ref()
        .map(SerializedRouteObserver::enter)
}

#[cfg(not(test))]
fn serialized_route_observer_guard(
    _runtime: &DaemonRuntime,
) -> Option<SerializedRouteObserverGuard> {
    None
}

#[cfg(test)]
fn record_request_submission(runtime: &DaemonRuntime) {
    if let Some(observer) = runtime.request_submission_observer.as_ref() {
        observer.record();
    }
}

#[cfg(not(test))]
fn record_request_submission(_runtime: &DaemonRuntime) {}

fn write_local_capability_manifest(
    manifest_dir: &Path,
    address: SocketAddr,
    auth_token: Option<&str>,
) -> Result<()> {
    fs::create_dir_all(manifest_dir)
        .with_context(|| format!("create loom manifest dir {}", manifest_dir.display()))?;
    let mut transport = json!({
        "type": "http",
        "baseUrl": format!("http://{}", address),
        "auth": "none"
    });
    if let Some(token) = auth_token {
        transport["auth"] = Value::String("bearer".to_owned());
        transport["authToken"] = Value::String(token.to_owned());
    }
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("read system time for loom manifest")?
        .as_secs();
    let manifest = json!({
        "schemaVersion": 1,
        "appId": "loom",
        "displayName": "Loom",
        "version": loom_core::LOOM_VERSION,
        "pid": std::process::id(),
        "transport": transport,
        "capabilities": invokable_capability_ids(),
        "startedAt": started_at
    });
    fs::write(
        manifest_dir.join("loom.json"),
        serde_json::to_string_pretty(&manifest)?,
    )
    .with_context(|| format!("write loom manifest in {}", manifest_dir.display()))?;
    Ok(())
}

fn is_loopback_bind_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn default_control_plane_root() -> PathBuf {
    if let Some(path) = std::env::var_os("LOOM_CONTROL_PLANE_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return path;
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.join("Loom").join("control-plane"))
        .unwrap_or_else(|| PathBuf::from(".runtime").join("loom").join("control-plane"))
}

fn mcp_server_store_path(control_plane_root: &Path) -> PathBuf {
    control_plane_root.join("mcp").join("servers.json")
}

fn normalize_mcp_server_config(mut server: McpServerConfig) -> McpServerConfig {
    if is_npx_command(&server.command) && server_uses_brave_search_package(&server.args) {
        server.args = vec![
            "-y".to_owned(),
            "@brave/brave-search-mcp-server".to_owned(),
            "--transport".to_owned(),
            "stdio".to_owned(),
        ];
    }
    server
}

fn is_npx_command(command: &str) -> bool {
    Path::new(command)
        .file_stem()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("npx"))
}

fn server_uses_brave_search_package(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.trim(),
            "github:brave/brave-search-mcp-server" | "@brave/brave-search-mcp-server"
        )
    })
}

fn load_persisted_mcp_servers(control_plane_root: &Path) -> HashMap<String, McpServerConfig> {
    let path = mcp_server_store_path(control_plane_root);
    let Some(content) = fs::read_to_string(path).ok() else {
        return HashMap::new();
    };

    let parsed = serde_json::from_str::<Vec<McpServerConfig>>(&content)
        .ok()
        .or_else(|| {
            serde_json::from_str::<Value>(&content)
                .ok()
                .and_then(|value| value.get("servers").cloned())
                .and_then(|servers| serde_json::from_value::<Vec<McpServerConfig>>(servers).ok())
        });

    parsed
        .unwrap_or_default()
        .into_iter()
        .map(normalize_mcp_server_config)
        .map(|server| (server.id.clone(), server))
        .collect()
}

fn persist_mcp_servers_snapshot(
    path: &Path,
    servers: &HashMap<String, McpServerConfig>,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create MCP server store dir {}", parent.display()))?;
    }
    let mut ordered = servers.values().cloned().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.id.cmp(&right.id));
    fs::write(path, serde_json::to_string_pretty(&ordered)?)
        .with_context(|| format!("write MCP server store {}", path.display()))?;
    Ok(())
}

enum HttpReadOutcome {
    Empty,
    Request(String),
    Rejected { status: u16, body: String },
}

fn read_http_request(stream: &mut impl Read) -> Result<HttpReadOutcome> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 512];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) if request.is_empty() => return Ok(HttpReadOutcome::Empty),
            Ok(0) => break,
            Ok(bytes) => {
                request.extend_from_slice(&buffer[..bytes]);
                if request_exceeds_size_limit(&request) {
                    return Ok(payload_too_large_response());
                }
                if request_has_full_body(&request) {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) && request.is_empty() =>
            {
                return Ok(HttpReadOutcome::Empty);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                break;
            }
            Err(error) => return Err(error).context("read daemon request"),
        }
    }

    Ok(HttpReadOutcome::Request(
        String::from_utf8_lossy(&request).to_string(),
    ))
}

fn payload_too_large_response() -> HttpReadOutcome {
    HttpReadOutcome::Rejected {
        status: 413,
        body: json!({
            "error": {
                "code": "payload_too_large",
                "message": "request body is too large"
            },
            "status": "failed"
        })
        .to_string(),
    }
}

fn request_exceeds_size_limit(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return request.len() > MAX_HTTP_HEADER_BYTES;
    };
    if header_end > MAX_HTTP_HEADER_BYTES {
        return true;
    }

    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let content_length = content_length(&header_text);
    let body_start = header_end + 4;
    content_length > MAX_HTTP_BODY_BYTES
        || request.len().saturating_sub(body_start) > MAX_HTTP_BODY_BYTES
}

fn request_has_full_body(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let content_length = content_length(&header_text);
    let body_start = header_end + 4;
    request.len().saturating_sub(body_start) >= content_length
}

fn content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

#[derive(Debug)]
struct ParsedHttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl ParsedHttpRequest {
    fn from_raw(raw: &str) -> Self {
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let mut lines = head.lines();
        let mut request_line = lines.next().unwrap_or("").split_whitespace();
        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_string(), value.trim().to_string()))
            })
            .collect();
        Self {
            method: request_line.next().unwrap_or("GET").to_string(),
            path: request_line.next().unwrap_or("/").to_string(),
            headers,
            body: body.to_string(),
        }
    }

    fn has_bearer(&self, token: &str) -> bool {
        self.headers.iter().any(|(name, value)| {
            let mut parts = value.split_whitespace();
            name.eq_ignore_ascii_case("authorization")
                && parts
                    .next()
                    .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer"))
                && parts.next() == Some(token)
                && parts.next().is_none()
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    pid: u32,
    executable_path: String,
}

#[derive(Serialize)]
struct StatusResponse {
    status: &'static str,
    modules: Vec<ModuleStatus>,
    hooks: HookSettingsSummary,
    brain_planner: BrainPlannerStatus,
    run_store: RunStoreStatus,
    #[serde(rename = "requestExecutor")]
    request_executor: RequestExecutorStatus,
}

#[derive(Serialize)]
struct ModuleStatus {
    name: &'static str,
    version: &'static str,
    initialized: bool,
}

#[derive(Debug, Deserialize)]
struct StartRunRequest {
    ticket: TicketRequest,
}

#[derive(Debug, Deserialize)]
struct TicketRequest {
    id: String,
    title: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunActionRequest {
    run: Value,
}

#[derive(Debug, Deserialize)]
struct InvokeCapabilityRequest {
    #[serde(rename = "requestId")]
    request_id: String,
    caller: String,
    capability: String,
    #[serde(default)]
    input: Value,
}

#[derive(Debug, Deserialize)]
struct PutWorkflowRequest {
    #[serde(alias = "yaml")]
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveHookCanvasWorkflowRequest {
    selected_node_id: String,
    workflow_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameCanvasWorkflowRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ExecuteToolRequest {
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameworkPackageRequest {
    #[serde(default, alias = "zip_base64")]
    zip_base64: String,
}

#[derive(Debug, Deserialize)]
struct PythonSourceReadRequest {
    #[serde(default, alias = "filePath")]
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PythonArtJsonReadRequest {
    #[serde(default, alias = "art_path", alias = "path")]
    art_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PythonNearbyArtJsonRequest {
    #[serde(default, alias = "python_path", alias = "path")]
    python_path: String,
}

#[derive(Debug, Deserialize)]
struct PythonInferPortsRequest {
    #[serde(default)]
    code: String,
    #[serde(default, alias = "filePath")]
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PythonShaderPrefetchRequest {
    #[serde(default, alias = "art_id")]
    art_id: String,
    #[serde(default, alias = "art_path")]
    art_path: Option<String>,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtLoomCompatPythonExecuteArtRequest {
    #[serde(default, alias = "art_id")]
    art_id: String,
    #[serde(default, alias = "art_path")]
    art_path: Option<String>,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtLoomCompatPythonProcessImageRequest {
    #[serde(default, alias = "art_id")]
    art_id: String,
    #[serde(default, alias = "art_path")]
    art_path: Option<String>,
    #[serde(default, alias = "input_base64")]
    input_base64: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct StartHookBridgeRequest {
    port: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpPackageCheckRequest {
    #[serde(default, alias = "module_name", alias = "module")]
    module_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpPackageInstallPlanRequest {
    #[serde(default, alias = "package_name", alias = "package")]
    package_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtLoomCompatCallMcpToolRequest {
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default, alias = "tool_name")]
    tool_name: String,
    #[serde(default, alias = "tool_args")]
    tool_args: Value,
}

#[derive(Debug, Deserialize)]
struct ArtLoomCompatToggleRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtLoomCompatNativeProcessArtRequest {
    #[serde(default, alias = "art_id")]
    art_id: String,
    #[serde(default, alias = "input_base64")]
    input_base64: String,
    #[serde(default)]
    params: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtLoomCompatInstantiateWorkflowRequest {
    #[serde(default)]
    nodes: Vec<Value>,
    #[serde(default)]
    edges: Vec<Value>,
    #[serde(default)]
    mode: String,
    #[serde(default, alias = "workflow_id")]
    workflow_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtLoomCompatUpdateWorkflowNodeRequest {
    #[serde(default, alias = "workflow_id")]
    workflow_id: String,
    #[serde(default, alias = "node_id")]
    node_id: String,
    #[serde(default)]
    param: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtLoomCompatExecuteArtNodeRequest {
    #[serde(default, alias = "node_id")]
    node_id: String,
    #[serde(default, alias = "art_id")]
    art_id: String,
    #[serde(default, alias = "input_base64")]
    input_base64: Option<String>,
    #[serde(default)]
    params: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct SharedMemoryCreateBufferRequest {
    width: u32,
    height: u32,
    #[serde(default = "default_shared_memory_channels")]
    channels: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageHelperConvertRequest {
    source_type: String,
    target_type: String,
    data: Option<Value>,
    path: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

type SharedRunStore = Arc<Mutex<Box<dyn RunEvidenceStore>>>;
type SharedMcpServerStore = Arc<Mutex<HashMap<String, McpServerConfig>>>;
type SharedHookBridgeRuntime = Arc<Mutex<HookBridgeRuntime>>;
type SharedImageStoreHandle = Arc<Mutex<SharedImageStore>>;
type OcrProviderHandle = Arc<Mutex<OcrProvider>>;
type SharedArtLoomCompatSettingsStore = Arc<Mutex<ArtLoomCompatSettingsStore>>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ArtLoomShortcutConfig {
    id: String,
    label: String,
    keys: String,
    enabled: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ArtLoomGeneralSettings {
    theme: String,
    language: String,
    auto_start: bool,
    minimize_to_tray: bool,
    enable_tray_icon: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ArtLoomSystemPreferences {
    auto_check_updates: bool,
    enable_run_log: bool,
    run_as_admin: bool,
    record_screenshot_history: bool,
    history_retention: String,
    enable_proxy: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ArtLoomEngineSettings {
    comfyui_url: String,
    python_interpreter: String,
    virtual_env_path: String,
    compute_device: String,
    vram_reservation_gb: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ArtLoomQuickBinding {
    id: String,
    art: String,
    key: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ArtLoomCompatSettings {
    general: ArtLoomGeneralSettings,
    system: ArtLoomSystemPreferences,
    engine: ArtLoomEngineSettings,
    #[serde(default)]
    quick_bindings: Vec<ArtLoomQuickBinding>,
    #[serde(default)]
    shortcuts: HashMap<String, ArtLoomShortcutConfig>,
}

impl Default for ArtLoomCompatSettings {
    fn default() -> Self {
        let mut shortcuts = HashMap::new();
        for shortcut in default_artloom_shortcuts() {
            shortcuts.insert(shortcut.id.clone(), shortcut);
        }
        Self {
            general: ArtLoomGeneralSettings {
                theme: "system".to_owned(),
                language: "zh-Hans".to_owned(),
                auto_start: false,
                minimize_to_tray: true,
                enable_tray_icon: true,
            },
            system: ArtLoomSystemPreferences {
                auto_check_updates: true,
                enable_run_log: true,
                run_as_admin: false,
                record_screenshot_history: true,
                history_retention: "7d".to_owned(),
                enable_proxy: false,
            },
            engine: ArtLoomEngineSettings {
                comfyui_url: "http://127.0.0.1:8188".to_owned(),
                python_interpreter: "python.exe".to_owned(),
                virtual_env_path: "./venv".to_owned(),
                compute_device: "0".to_owned(),
                vram_reservation_gb: 12,
            },
            quick_bindings: vec![ArtLoomQuickBinding {
                id: "1".to_owned(),
                art: "ComfyUI Workflow".to_owned(),
                key: "Ctrl+Shift+1".to_owned(),
            }],
            shortcuts,
        }
    }
}

struct ArtLoomCompatSettingsStore {
    path: PathBuf,
    settings: ArtLoomCompatSettings,
}

impl ArtLoomCompatSettingsStore {
    fn new(path: PathBuf) -> Self {
        let settings = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<ArtLoomCompatSettings>(&content).ok())
            .unwrap_or_default();
        Self { path, settings }
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create ArtLoom compat settings dir {}", parent.display())
            })?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(&self.settings)?)
            .with_context(|| format!("write ArtLoom compat settings {}", self.path.display()))?;
        Ok(())
    }
}

fn default_artloom_shortcuts() -> Vec<ArtLoomShortcutConfig> {
    [
        ("cancel", "Cancel / Deselect", "Escape"),
        ("capture", "Screenshot", "Ctrl+1"),
        ("copy_unit", "Copy Unit", "Ctrl+C"),
        ("paste_unit", "Paste Unit", "Ctrl+V"),
        ("save_image", "Save Image", "Ctrl+S"),
        ("toggle_ocr", "Toggle OCR", "Alt+2"),
        ("toggle_translation", "Toggle Translation", "Alt+3"),
    ]
    .into_iter()
    .map(|(id, label, keys)| ArtLoomShortcutConfig {
        id: id.to_owned(),
        label: label.to_owned(),
        keys: keys.to_owned(),
        enabled: true,
    })
    .collect()
}

#[derive(Debug)]
enum OcrProvider {
    Unavailable,
    Fixture { text: String },
    Real { engine: loom_ocr::OcrEngine },
}

impl OcrProvider {
    fn from_env() -> Self {
        if let Some(text) = std::env::var("LOOM_OCR_FIXTURE_TEXT")
            .ok()
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
        {
            return Self::Fixture { text };
        }

        match loom_ocr::discover_default_model_set() {
            Ok(Some(model_set)) => match loom_ocr::OcrEngine::new(model_set) {
                Ok(engine) => Self::Real { engine },
                Err(_) => Self::Unavailable,
            },
            Ok(None) | Err(_) => Self::Unavailable,
        }
    }

    fn is_available(&self) -> bool {
        matches!(self, Self::Fixture { .. } | Self::Real { .. })
    }
}

struct HookBridgeRuntime {
    port: Option<u16>,
    shutdown_tx: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
    connected_clients: Arc<AtomicUsize>,
    broadcast_hub: HookBridgeBroadcastHub,
    workflow_root: PathBuf,
}

impl HookBridgeRuntime {
    fn new(workflow_root: PathBuf) -> Self {
        Self {
            port: None,
            shutdown_tx: None,
            worker: None,
            connected_clients: Arc::new(AtomicUsize::new(0)),
            broadcast_hub: HookBridgeBroadcastHub::new(),
            workflow_root,
        }
    }
}

#[derive(Clone)]
struct HookBridgeBroadcastHub {
    subscribers: Arc<Mutex<Vec<HookBridgeSubscriber>>>,
    next_subscriber_id: Arc<AtomicUsize>,
}

impl HookBridgeBroadcastHub {
    fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
            next_subscriber_id: Arc::new(AtomicUsize::new(1)),
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
    }
}

#[derive(Clone)]
struct HookBridgeSubscriber {
    id: usize,
    tx: Sender<String>,
    channels: Vec<String>,
}

fn route(
    request: &ParsedHttpRequest,
    hook_settings: &HookSettings,
    run_store: &SharedRunStore,
    run_store_status: RunStoreStatus,
    brain_planner: &SharedBrainPlanner,
    auth_token: Option<&str>,
    config_registry: &ConfigRegistry,
    config_store: &FileDocumentStore,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    hook_bridge: &SharedHookBridgeRuntime,
    artloom_settings: &SharedArtLoomCompatSettingsStore,
    shared_images: &SharedImageStoreHandle,
    ocr_provider: &OcrProviderHandle,
    settings_base_url: &str,
    mcp_registry_endpoint: &str,
    request_executor: RequestExecutorStatus,
    canvas_workflow_root: &Path,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    if let Some(token) = auth_token {
        let requires_auth = request.path != "/health";
        if requires_auth && !request.has_bearer(token) {
            return structured_error(
                401,
                json!({
                    "code": "unauthorized",
                    "message": "missing or invalid Loom bearer token",
                }),
            );
        }
    }

    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());

    match (request.method.as_str(), route_path) {
        ("GET", "/health") => Ok((
            200,
            serde_json::to_string(&HealthResponse {
                status: "ok",
                version: env!("CARGO_PKG_VERSION"),
                pid: std::process::id(),
                executable_path: std::env::current_exe()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
            })?,
        )),
        ("GET", "/status") => Ok((
            200,
            serde_json::to_string(&StatusResponse {
                status: "ready",
                modules: module_statuses(),
                hooks: hook_settings.summary(),
                brain_planner: brain_planner.status(),
                run_store: run_store_status,
                request_executor,
            })?,
        )),
        ("GET", "/v1/configuration/claims") if configuration_claim_app(&request.path).is_some() => {
            configuration_claim(
                configuration_claim_app(&request.path).expect("checked path"),
                config_registry,
                settings_base_url,
            )
        }
        ("GET", "/settings") => settings_index(config_registry, config_store),
        ("GET", path) if app_from_path(path, "/settings/").is_some() => settings_app(
            app_from_path(path, "/settings/").expect("checked app path"),
            config_registry,
            config_store,
        ),
        ("GET", path) if app_from_path(path, "/v1/configuration/apps/").is_some() => {
            get_managed_config(
                app_from_path(path, "/v1/configuration/apps/").expect("checked app path"),
                config_registry,
                config_store,
            )
        }
        ("PUT", path) if app_from_path(path, "/v1/configuration/apps/").is_some() => {
            put_managed_config(
                app_from_path(path, "/v1/configuration/apps/").expect("checked app path"),
                &request.body,
                config_registry,
                config_store,
            )
        }
        ("GET", "/v1/capabilities") => capabilities(),
        ("GET", "/v1/mcp/servers") => list_mcp_servers(mcp_servers),
        ("GET", "/v1/mcp/registry") => fetch_mcp_registry(&request.path, mcp_registry_endpoint),
        ("POST", "/v1/mcp/test") => test_mcp_connection(&request.body),
        ("POST", "/v1/mcp/package/check") => check_mcp_package_installed(&request.body),
        ("POST", "/v1/mcp/package/install-plan") => build_mcp_package_install_plan(&request.body),
        ("POST", "/v1/artloom-compat/mcp/call-tool") => artloom_compat_call_mcp_tool(&request.body),
        ("GET", "/v1/artloom-compat/mcp/registry") => {
            artloom_compat_fetch_mcp_registry(&request.path, mcp_registry_endpoint)
        }
        ("GET", "/v1/artloom-compat/mcp/servers") => artloom_compat_list_mcp_servers(mcp_servers),
        ("POST", "/v1/artloom-compat/mcp/servers") => artloom_compat_save_mcp_server(
            &request.body,
            mcp_servers,
            &mcp_server_store_path(control_plane_root),
        ),
        ("DELETE", path) if path_id(path, "/v1/artloom-compat/mcp/servers/").is_some() => {
            artloom_compat_delete_mcp_server(
                path_id(path, "/v1/artloom-compat/mcp/servers/").expect("checked path"),
                mcp_servers,
                &mcp_server_store_path(control_plane_root),
            )
        }
        ("PUT", path) if path_id(path, "/v1/mcp/servers/").is_some() => put_mcp_server(
            path_id(path, "/v1/mcp/servers/").expect("checked path"),
            &request.body,
            mcp_servers,
            &mcp_server_store_path(control_plane_root),
        ),
        ("DELETE", path) if path_id(path, "/v1/mcp/servers/").is_some() => delete_mcp_server(
            path_id(path, "/v1/mcp/servers/").expect("checked path"),
            mcp_servers,
            &mcp_server_store_path(control_plane_root),
        ),
        ("GET", "/v1/tools") => list_tools(tool_registry),
        ("GET", "/v1/frameworks") => list_frameworks(framework_registry),
        ("POST", "/v1/frameworks/install") => {
            install_framework_package(&request.body, framework_registry)
        }
        ("POST", "/v1/arts/install") => install_art(
            &request.body,
            tool_registry,
            framework_registry,
            control_plane_root,
            hook_bridge,
        ),
        ("GET", path) if path.split('?').next() == Some("/v1/arts/store/catalog") => {
            fetch_art_store_catalog(path)
        }
        ("POST", "/v1/arts/store/install") => install_art_from_store(
            &request.body,
            tool_registry,
            framework_registry,
            control_plane_root,
            hook_bridge,
        ),
        ("GET", path) if path_id_with_suffix(path, "/v1/arts/", "/package").is_some() => {
            package_art(
                path_id_with_suffix(path, "/v1/arts/", "/package").expect("checked path"),
                tool_registry,
                control_plane_root,
            )
        }
        ("POST", "/v1/arts/store/publish") => {
            publish_art_to_store(&request.body, tool_registry, control_plane_root)
        }
        ("POST", path) if path_id_with_suffix(path, "/v1/frameworks/", "/install").is_some() => {
            install_framework(
                path_id_with_suffix(path, "/v1/frameworks/", "/install").expect("checked path"),
                framework_registry,
            )
        }
        ("POST", path) if path_id_with_suffix(path, "/v1/frameworks/", "/enable").is_some() => {
            set_framework_enabled(
                path_id_with_suffix(path, "/v1/frameworks/", "/enable").expect("checked path"),
                true,
                framework_registry,
            )
        }
        ("POST", path) if path_id_with_suffix(path, "/v1/frameworks/", "/disable").is_some() => {
            set_framework_enabled(
                path_id_with_suffix(path, "/v1/frameworks/", "/disable").expect("checked path"),
                false,
                framework_registry,
            )
        }
        ("POST", path) if path_id_with_suffix(path, "/v1/frameworks/", "/upgrade").is_some() => {
            upgrade_framework_package(
                path_id_with_suffix(path, "/v1/frameworks/", "/upgrade").expect("checked path"),
                &request.body,
                framework_registry,
            )
        }
        ("POST", path) if path_id_with_suffix(path, "/v1/frameworks/", "/uninstall").is_some() => {
            uninstall_framework(
                path_id_with_suffix(path, "/v1/frameworks/", "/uninstall").expect("checked path"),
                framework_registry,
            )
        }
        ("GET", path) if path_id_with_suffix(path, "/v1/tools/", "/readiness").is_some() => {
            tool_readiness(
                path_id_with_suffix(path, "/v1/tools/", "/readiness").expect("checked path"),
                tool_registry,
                framework_registry,
            )
        }
        ("PUT", path) if path_id(path, "/v1/tools/").is_some() => put_tool(
            path_id(path, "/v1/tools/").expect("checked path"),
            &request.body,
            tool_registry,
            hook_bridge,
        ),
        ("DELETE", path) if path_id(path, "/v1/tools/").is_some() => delete_tool(
            path_id(path, "/v1/tools/").expect("checked path"),
            tool_registry,
            hook_bridge,
        ),
        ("POST", path) if tool_execute_path_id(path).is_some() => execute_registered_tool(
            tool_execute_path_id(path).expect("checked path"),
            &request.body,
            mcp_servers,
            tool_registry,
            workflow_store,
            framework_registry,
        ),
        ("GET", "/v1/artloom-compat/arts") => list_artloom_compat_arts("list_arts", tool_registry),
        ("GET", "/v1/artloom-compat/arts/enabled") => {
            list_enabled_artloom_compat_arts(tool_registry)
        }
        ("GET", "/v1/artloom-compat/user-arts") => {
            list_artloom_compat_arts("get_user_arts", tool_registry)
        }
        ("POST", "/v1/artloom-compat/arts/sync") => {
            sync_artloom_compat_arts(&request.body, tool_registry, hook_bridge)
        }
        ("POST", "/v1/artloom-compat/arts/broadcast-updated") => {
            broadcast_artloom_compat_arts_updated(hook_bridge)
        }
        ("POST", "/v1/artloom-compat/native/process-art") => {
            artloom_compat_native_process_art(&request.body)
        }
        ("GET", path) if path_id(path, "/v1/artloom-compat/arts/").is_some() => {
            get_artloom_compat_art(
                path_id(path, "/v1/artloom-compat/arts/").expect("checked path"),
                tool_registry,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/artloom-compat/arts/", "/enable").is_some() =>
        {
            set_artloom_compat_art_enabled(
                path_id_with_suffix(path, "/v1/artloom-compat/arts/", "/enable")
                    .expect("checked path"),
                true,
                tool_registry,
                hook_bridge,
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/artloom-compat/arts/", "/disable").is_some() =>
        {
            set_artloom_compat_art_enabled(
                path_id_with_suffix(path, "/v1/artloom-compat/arts/", "/disable")
                    .expect("checked path"),
                false,
                tool_registry,
                hook_bridge,
            )
        }
        ("PUT", path)
            if path_id_with_suffix(path, "/v1/artloom-compat/arts/", "/defaults").is_some() =>
        {
            update_artloom_compat_art_defaults(
                path_id_with_suffix(path, "/v1/artloom-compat/arts/", "/defaults")
                    .expect("checked path"),
                &request.body,
                tool_registry,
                hook_bridge,
            )
        }
        ("GET", "/v1/python-arts/engine/status") => python_engine_status(),
        ("POST", "/v1/artloom-compat/python/execute-art") => {
            artloom_compat_execute_python_art(&request.body)
        }
        ("POST", "/v1/artloom-compat/python/process-image") => {
            artloom_compat_python_process_image(&request.body)
        }
        ("GET", "/v1/artloom-compat/python/installed-arts") => {
            list_artloom_compat_installed_python_arts()
        }
        ("POST", "/v1/artloom-compat/python/read-art-json") => {
            artloom_compat_read_art_json(&request.body)
        }
        ("POST", "/v1/artloom-compat/python/read-python-file") => {
            artloom_compat_read_python_file(&request.body)
        }
        ("POST", "/v1/artloom-compat/python/check-art-json-nearby") => {
            artloom_compat_check_art_json_nearby(&request.body)
        }
        ("POST", "/v1/python-arts/shader/prefetch") => prefetch_python_art_shader(&request.body),
        ("GET", "/v1/python-arts") => list_python_arts(),
        ("GET", path) if path_id(path, "/v1/python-arts/").is_some() => {
            get_python_art(path_id(path, "/v1/python-arts/").expect("checked path"))
        }
        ("POST", "/v1/python-arts/source/read") => read_python_art_source(&request.body),
        ("POST", "/v1/python-arts/source/read-art-json") => read_python_art_json(&request.body),
        ("POST", "/v1/python-arts/source/check-art-json") => {
            check_python_art_json_nearby(&request.body)
        }
        ("POST", "/v1/python-arts/source/infer-ports") => infer_python_art_ports(&request.body),
        ("GET", "/v1/shared-memory/buffers") => list_shared_memory_buffers(shared_images),
        ("POST", "/v1/shared-memory/buffers") => {
            create_shared_memory_buffer(&request.body, shared_images)
        }
        ("GET", path) if path_id(path, "/v1/shared-memory/buffers/").is_some() => {
            get_shared_memory_buffer_info(
                path_id(path, "/v1/shared-memory/buffers/").expect("checked path"),
                shared_images,
            )
        }
        ("DELETE", path) if path_id(path, "/v1/shared-memory/buffers/").is_some() => {
            release_shared_memory_buffer(
                path_id(path, "/v1/shared-memory/buffers/").expect("checked path"),
                shared_images,
            )
        }
        ("GET", "/v1/shared-images") => list_shared_images(shared_images),
        ("POST", "/v1/shared-images") => create_shared_image(&request.body, shared_images),
        ("POST", "/v1/image-helpers/convert") => convert_image_helper(&request.body),
        ("GET", path) if path_id(path, "/v1/shared-images/").is_some() => get_shared_image(
            path_id(path, "/v1/shared-images/").expect("checked path"),
            shared_images,
        ),
        ("DELETE", path) if path_id(path, "/v1/shared-images/").is_some() => delete_shared_image(
            path_id(path, "/v1/shared-images/").expect("checked path"),
            shared_images,
        ),
        ("GET", "/v1/artloom-compat/settings") => get_artloom_compat_settings(artloom_settings),
        ("PUT", "/v1/artloom-compat/settings") => {
            put_artloom_compat_settings(&request.body, artloom_settings)
        }
        ("GET", "/v1/artloom-compat/shortcuts") => get_artloom_compat_shortcuts(artloom_settings),
        ("PUT", path) if path_id(path, "/v1/artloom-compat/shortcuts/").is_some() => {
            put_artloom_compat_shortcut(
                path_id(path, "/v1/artloom-compat/shortcuts/").expect("checked path"),
                &request.body,
                artloom_settings,
            )
        }
        ("GET", "/v1/artloom-compat/app-paths") => get_artloom_compat_app_paths(),
        ("GET", "/v1/artloom-compat/ipc/status") => artloom_compat_ipc_status(hook_bridge),
        ("POST", "/v1/artloom-compat/ipc/instantiate-workflow") => {
            artloom_compat_instantiate_workflow(&request.body, hook_bridge)
        }
        ("POST", "/v1/artloom-compat/ipc/update-workflow-node") => {
            artloom_compat_update_workflow_node(&request.body, hook_bridge, tool_registry)
        }
        ("POST", "/v1/artloom-compat/ipc/execute-art-node") => artloom_compat_execute_art_node(
            &request.body,
            mcp_servers,
            tool_registry,
            workflow_store,
        ),
        ("GET", "/v1/artloom-compat/system/autostart") => {
            get_artloom_compat_autostart(artloom_settings)
        }
        ("POST", "/v1/artloom-compat/system/autostart") => {
            set_artloom_compat_autostart(&request.body, artloom_settings)
        }
        ("POST", "/v1/artloom-compat/system/autostart/enable") => {
            set_artloom_compat_autostart_preference("enable_autostart", true, artloom_settings)
        }
        ("POST", "/v1/artloom-compat/system/autostart/disable") => {
            set_artloom_compat_autostart_preference("disable_autostart", false, artloom_settings)
        }
        ("POST", "/v1/artloom-compat/system/minimize-to-tray") => {
            set_artloom_compat_minimize_to_tray(&request.body, artloom_settings)
        }
        ("GET", "/v1/artloom-compat/workflows") => list_artloom_compat_workflows(workflow_store),
        ("PUT", path)
            if path_id_with_suffix(path, "/v1/artloom-compat/workflows/", "/metadata")
                .is_some() =>
        {
            save_artloom_compat_workflow_metadata(
                path_id_with_suffix(path, "/v1/artloom-compat/workflows/", "/metadata")
                    .expect("checked path"),
                &request.body,
                workflow_store,
            )
        }
        ("PUT", path)
            if path_id_with_suffix(path, "/v1/artloom-compat/workflows/", "/data").is_some() =>
        {
            save_artloom_compat_workflow_data(
                path_id_with_suffix(path, "/v1/artloom-compat/workflows/", "/data")
                    .expect("checked path"),
                &request.body,
                workflow_store,
            )
        }
        ("GET", path)
            if path_id_with_suffix(path, "/v1/artloom-compat/workflows/", "/data").is_some() =>
        {
            load_artloom_compat_workflow_data(
                path_id_with_suffix(path, "/v1/artloom-compat/workflows/", "/data")
                    .expect("checked path"),
                workflow_store,
            )
        }
        ("DELETE", path)
            if path_id_with_suffix(path, "/v1/artloom-compat/workflows/", "/data").is_some() =>
        {
            delete_artloom_compat_workflow_data(
                path_id_with_suffix(path, "/v1/artloom-compat/workflows/", "/data")
                    .expect("checked path"),
                workflow_store,
            )
        }
        ("GET", "/v1/workflows") => list_workflows(workflow_store),
        ("GET", path) if path_id(path, "/v1/workflows/").is_some() => get_workflow(
            path_id(path, "/v1/workflows/").expect("checked path"),
            workflow_store,
        ),
        ("PUT", path) if path_id(path, "/v1/workflows/").is_some() => put_workflow(
            path_id(path, "/v1/workflows/").expect("checked path"),
            &request.body,
            workflow_store,
        ),
        ("DELETE", path) if path_id(path, "/v1/workflows/").is_some() => delete_workflow(
            path_id(path, "/v1/workflows/").expect("checked path"),
            workflow_store,
        ),
        ("GET", "/v1/hook-bridge/status") => hook_bridge_status(hook_bridge),
        ("GET", "/v1/hook-bridge/session") => hook_bridge_session(hook_bridge),
        ("GET", "/v1/hook-bridge/canvas") => hook_canvas_snapshot(),
        ("GET", "/v1/hook-bridge/canvas/workflows") => list_canvas_workflows(canvas_workflow_root),
        ("GET", path) if path_id(path, "/v1/hook-bridge/canvas/workflows/").is_some() => {
            get_canvas_workflow_snapshot(
                path_id(path, "/v1/hook-bridge/canvas/workflows/").expect("checked path"),
                canvas_workflow_root,
            )
        }
        ("PUT", path)
            if path_id_with_suffix(path, "/v1/hook-bridge/canvas/workflows/", "/rename")
                .is_some() =>
        {
            rename_canvas_workflow(
                path_id_with_suffix(path, "/v1/hook-bridge/canvas/workflows/", "/rename")
                    .expect("checked path"),
                &request.body,
                canvas_workflow_root,
            )
        }
        ("PUT", path) if path_id(path, "/v1/hook-bridge/canvas/workflows/").is_some() => {
            save_hook_canvas_workflow(
                path_id(path, "/v1/hook-bridge/canvas/workflows/").expect("checked path"),
                &request.body,
                workflow_store,
                canvas_workflow_root,
            )
        }
        ("DELETE", path) if path_id(path, "/v1/hook-bridge/canvas/workflows/").is_some() => {
            delete_canvas_workflow(
                path_id(path, "/v1/hook-bridge/canvas/workflows/").expect("checked path"),
                canvas_workflow_root,
            )
        }
        ("POST", "/v1/hook-bridge/start") => start_hook_bridge(
            &request.body,
            hook_bridge,
            mcp_servers,
            tool_registry,
            workflow_store,
            artloom_settings,
            shared_images,
            ocr_provider,
        ),
        ("POST", "/v1/hook-bridge/stop") => stop_hook_bridge(hook_bridge),
        ("POST", "/v1/runs") => start_tea_run(&request.body, run_store),
        ("POST", "/v1/invoke") => invoke_capability(&request.body, run_store, brain_planner),
        ("GET", path) if run_events_path_id(path).is_some() => {
            get_run_events(run_events_path_id(path).expect("checked path"), run_store)
        }
        ("GET", path) if run_path_id(path).is_some() => {
            get_run(run_path_id(path).expect("checked path"), run_store)
        }
        ("POST", path) if run_action_path_id(path, "stop").is_some() => run_action(
            run_action_path_id(path, "stop").expect("checked path"),
            &request.body,
            "stopped",
            run_store,
        ),
        ("POST", path) if run_action_path_id(path, "retry").is_some() => run_action(
            run_action_path_id(path, "retry").expect("checked path"),
            &request.body,
            "retrying",
            run_store,
        ),
        _ => structured_error(
            404,
            json!({
                "code": "not_found",
                "message": "Loom endpoint was not found",
            }),
        ),
    }
}

fn configuration_claim(
    app: &str,
    registry: &ConfigRegistry,
    settings_base_url: &str,
) -> Result<(u16, String)> {
    let app_id = match app.parse::<ManagedAppId>() {
        Ok(app_id) => app_id,
        Err(error) => {
            return structured_error(
                404,
                json!({
                    "code": managed_config_error_code(error.code()),
                    "message": error.message(),
                }),
            );
        }
    };
    let Some(adapter) = registry.get(app_id) else {
        return structured_error(
            404,
            json!({
                "code": "unknown_app",
                "message": format!("unknown managed app: {app_id}"),
            }),
        );
    };
    let managed = managed_app_set().contains(app_id);
    let panel_url =
        managed.then(|| format!("{}/{}", settings_base_url.trim_end_matches('/'), app_id));

    Ok((
        200,
        serde_json::to_string(&json!({
            "app": app_id,
            "managed": managed,
            "owner": if managed { "loom" } else { app_id.as_str() },
            "source": if managed { "loom-managed" } else { "local" },
            "panel_url": panel_url,
            "reason": if managed { "Loom manages this app configuration" } else { "Loom has not claimed this app configuration" },
            "schema_version": adapter.schema_version(),
        }))?,
    ))
}

fn managed_app_set() -> ManagedAppSet {
    ManagedAppSet::parse(&std::env::var("LOOM_MANAGED_CONFIG_APPS").unwrap_or_default())
}

fn app_from_path(path: &str, prefix: &str) -> Option<ManagedAppId> {
    path.strip_prefix(prefix)?.parse().ok()
}

fn path_id<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let id = path.strip_prefix(prefix)?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn path_id_with_suffix<'a>(path: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let id = path.strip_prefix(prefix)?.strip_suffix(suffix)?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn tool_execute_path_id(path: &str) -> Option<&str> {
    let id = path.strip_prefix("/v1/tools/")?.strip_suffix("/execute")?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn query_value(path: &str, name: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (percent_decode(key) == name).then(|| percent_decode(value))
    })
}

fn build_mcp_registry_url(
    endpoint: &str,
    search: Option<&str>,
    limit: Option<u32>,
    cursor: Option<&str>,
) -> String {
    let safe_limit = limit.unwrap_or(60).clamp(1, 100);
    let mut pairs = vec![format!("limit={safe_limit}")];
    if let Some(search_text) = search.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push(format!("search={}", percent_encode(search_text)));
    }
    if let Some(cursor_text) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push(format!("cursor={}", percent_encode(cursor_text)));
    }
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    format!(
        "{}{separator}{}",
        endpoint.trim_end_matches('&'),
        pairs.join("&")
    )
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&decoded).to_string()
}

#[derive(Debug, Deserialize)]
struct PutManagedConfigRequest {
    expected_revision: u64,
    config: Value,
}

fn settings_index(registry: &ConfigRegistry, store: &FileDocumentStore) -> Result<(u16, String)> {
    let managed = managed_app_set();
    let mut documents = Vec::new();
    for app in managed.managed_apps() {
        let (document, _) = match store.read_or_create(app, registry) {
            Ok(document) => document,
            Err(error) => return managed_config_error_response(error),
        };
        documents.push(document);
    }
    Ok((200, render_settings_index(registry, &managed, &documents)))
}

fn settings_app(
    app: ManagedAppId,
    registry: &ConfigRegistry,
    store: &FileDocumentStore,
) -> Result<(u16, String)> {
    if !managed_app_set().contains(app) {
        return managed_config_error_response(ManagedConfigError::new(
            ManagedConfigErrorCode::AppNotManaged,
            format!("{app} is not managed by Loom"),
        ));
    }
    let (document, _) = match store.read_or_create(app, registry) {
        Ok(document) => document,
        Err(error) => return managed_config_error_response(error),
    };
    Ok((200, render_app_settings_page(registry, app, &document)))
}

fn get_managed_config(
    app: ManagedAppId,
    registry: &ConfigRegistry,
    store: &FileDocumentStore,
) -> Result<(u16, String)> {
    if !managed_app_set().contains(app) {
        return managed_config_error_response(ManagedConfigError::new(
            ManagedConfigErrorCode::AppNotManaged,
            format!("{app} is not managed by Loom"),
        ));
    }
    let adapter = registry.get(app).expect("registered adapter");
    let (document, created) = match store.read_or_create(app, registry) {
        Ok(document) => document,
        Err(error) => return managed_config_error_response(error),
    };
    let config = document.config.clone();
    let ui_sections = adapter.ui_sections(&config);
    Ok((
        200,
        serde_json::to_string(&json!({
            "app": app,
            "owner": "loom",
            "source": "loom-managed",
            "writable": true,
            "created": created,
            "document": document.metadata(),
            "config": config,
            "ui": {
                "title": adapter.display_name(),
                "sections": ui_sections,
            }
        }))?,
    ))
}

fn put_managed_config(
    app: ManagedAppId,
    body: &str,
    registry: &ConfigRegistry,
    store: &FileDocumentStore,
) -> Result<(u16, String)> {
    if !managed_app_set().contains(app) {
        return managed_config_error_response(ManagedConfigError::new(
            ManagedConfigErrorCode::AppNotManaged,
            format!("{app} is not managed by Loom"),
        ));
    }
    let request = match serde_json::from_str::<PutManagedConfigRequest>(body) {
        Ok(request) => request,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_request",
                    "message": error.to_string(),
                }),
            );
        }
    };
    let document =
        match store.write_validated(app, request.expected_revision, request.config, registry) {
            Ok(document) => document,
            Err(error) => return managed_config_error_response(error),
        };
    let config = document.config.clone();
    Ok((
        200,
        serde_json::to_string(&json!({
            "ok": true,
            "app": app,
            "owner": "loom",
            "source": "loom-managed",
            "writable": true,
            "created": false,
            "document": document.metadata(),
            "config": config,
            "validation": { "errors": [] },
        }))?,
    ))
}

fn managed_config_error_response(error: ManagedConfigError) -> Result<(u16, String)> {
    let status = match error.code() {
        ManagedConfigErrorCode::UnknownApp => 404,
        ManagedConfigErrorCode::AppNotManaged => 409,
        ManagedConfigErrorCode::InvalidConfiguration => 400,
        ManagedConfigErrorCode::RevisionConflict => 409,
        ManagedConfigErrorCode::StorageError => 500,
    };
    structured_error(
        status,
        json!({
            "code": managed_config_error_code(error.code()),
            "message": error.message(),
            "validation": { "errors": error.validation_errors() },
        }),
    )
}

fn managed_config_error_code(code: ManagedConfigErrorCode) -> &'static str {
    match code {
        ManagedConfigErrorCode::UnknownApp => "unknown_app",
        ManagedConfigErrorCode::AppNotManaged => "app_not_managed",
        ManagedConfigErrorCode::InvalidConfiguration => "invalid_configuration",
        ManagedConfigErrorCode::RevisionConflict => "revision_conflict",
        ManagedConfigErrorCode::StorageError => "storage_error",
    }
}

fn configuration_claim_app(path: &str) -> Option<&str> {
    let query = path.strip_prefix("/v1/configuration/claims?")?;
    query.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        (name == "app" && !value.is_empty()).then_some(value)
    })
}

fn capabilities() -> Result<(u16, String)> {
    Ok((
        200,
        serde_json::to_string(&json!({
            "capabilities": [
                {
                    "id": CAPABILITY_BRAIN_PLAN,
                    "mode": "run",
                    "description": "Create a concise Loom-side execution plan from a goal and optional constraints.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "goal": { "type": "string" },
                            "constraints": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        },
                        "required": ["goal"]
                    }
                },
                {
                    "id": CAPABILITY_TEA_TICKET_DECOMPOSE,
                    "mode": "run",
                    "description": "Use Loom reasoning to generate a Tea work-order decomposition proposal without mutating Tea ticket state.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "schema_version": { "type": "integer" },
                            "request_id": { "type": "string" },
                            "ticket": { "type": "object" },
                            "comments": { "type": "array" },
                            "policy": { "type": "object" },
                            "context": { "type": "object" }
                        },
                        "required": ["schema_version", "request_id", "ticket", "policy", "context"]
                    }
                },
                {
                    "id": CAPABILITY_TEA_TICKET_EXECUTE,
                    "mode": "run",
                    "description": "Execute an approved Tea plan through Loom runtime and return run evidence.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "schema_version": { "type": "integer" },
                            "request_id": { "type": "string" },
                            "ticket": { "type": "object" },
                            "approved_plan_id": { "type": "string" },
                            "plan": { "type": "object" },
                            "policy": { "type": "object" }
                        },
                        "required": ["schema_version", "request_id", "ticket", "approved_plan_id", "plan", "policy"]
                    }
                },
                {
                    "id": CAPABILITY_TEA_TICKET_REVIEW,
                    "mode": "run",
                    "description": "Review Tea execution evidence and return a review suggestion without changing Tea state.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "schema_version": { "type": "integer" },
                            "request_id": { "type": "string" },
                            "ticket": { "type": "object" },
                            "evidence": { "type": "object" }
                        },
                        "required": ["schema_version", "request_id", "ticket", "evidence"]
                    }
                }
            ]
        }))?,
    ))
}

fn list_mcp_servers(mcp_servers: &SharedMcpServerStore) -> Result<(u16, String)> {
    let mut servers = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    servers.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((200, serde_json::to_string(&json!({ "servers": servers }))?))
}

fn put_mcp_server(
    path_id: &str,
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    store_path: &Path,
) -> Result<(u16, String)> {
    let server = match serde_json::from_str::<McpServerConfig>(body) {
        Ok(server) => server,
        Err(error) => return invalid_request(error.to_string()),
    };
    let server = normalize_mcp_server_config(server);
    if server.id != path_id {
        return id_mismatch("server", path_id, &server.id);
    }
    if server.name.trim().is_empty() || server.command.trim().is_empty() {
        return invalid_request("MCP server name and command are required");
    }

    {
        let mut guard = mcp_servers
            .lock()
            .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
        let previous = guard.insert(server.id.clone(), server.clone());
        if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
            match previous {
                Some(previous_server) => {
                    guard.insert(server.id.clone(), previous_server);
                }
                None => {
                    guard.remove(&server.id);
                }
            }
            return Err(error);
        }
    }

    Ok((200, serde_json::to_string(&json!({ "server": server }))?))
}

fn delete_mcp_server(
    path_id: &str,
    mcp_servers: &SharedMcpServerStore,
    store_path: &Path,
) -> Result<(u16, String)> {
    {
        let mut guard = mcp_servers
            .lock()
            .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
        let Some(removed) = guard.remove(path_id) else {
            return structured_error(
                404,
                json!({
                    "code": "mcp_server_not_found",
                    "message": format!("MCP server `{path_id}` was not found"),
                    "server_id": path_id,
                }),
            );
        };
        if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
            guard.insert(path_id.to_owned(), removed);
            return Err(error);
        }
    }

    Ok((
        200,
        serde_json::to_string(&json!({ "serverId": path_id, "deleted": true }))?,
    ))
}

fn artloom_compat_list_mcp_servers(mcp_servers: &SharedMcpServerStore) -> Result<(u16, String)> {
    let mut servers = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    servers.sort_by(|left, right| left.id.cmp(&right.id));
    let count = servers.len();
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "get_mcp_servers",
            "servers": servers,
            "count": count,
        }))?,
    ))
}

fn artloom_compat_save_mcp_server(
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    store_path: &Path,
) -> Result<(u16, String)> {
    let server = match serde_json::from_str::<McpServerConfig>(body) {
        Ok(server) => server,
        Err(error) => return invalid_request(error.to_string()),
    };
    let server = normalize_mcp_server_config(server);
    if server.id.trim().is_empty() {
        return invalid_request("MCP server id is required");
    }
    if server.name.trim().is_empty() || server.command.trim().is_empty() {
        return invalid_request("MCP server name and command are required");
    }

    {
        let mut guard = mcp_servers
            .lock()
            .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
        let previous = guard.insert(server.id.clone(), server.clone());
        if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
            match previous {
                Some(previous_server) => {
                    guard.insert(server.id.clone(), previous_server);
                }
                None => {
                    guard.remove(&server.id);
                }
            }
            return Err(error);
        }
    }

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "save_mcp_server",
            "message": "Saved successfully",
            "server": server,
        }))?,
    ))
}

fn artloom_compat_delete_mcp_server(
    path_id: &str,
    mcp_servers: &SharedMcpServerStore,
    store_path: &Path,
) -> Result<(u16, String)> {
    {
        let mut guard = mcp_servers
            .lock()
            .map_err(|_| anyhow::anyhow!("lock MCP server store"))?;
        let Some(removed) = guard.remove(path_id) else {
            return structured_error(
                404,
                json!({
                    "compatCommand": "delete_mcp_server",
                    "code": "mcp_server_not_found",
                    "message": "Server not found",
                    "server_id": path_id,
                }),
            );
        };
        if let Err(error) = persist_mcp_servers_snapshot(store_path, &guard) {
            guard.insert(path_id.to_owned(), removed);
            return Err(error);
        }
    }

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "delete_mcp_server",
            "serverId": path_id,
            "deleted": true,
            "message": "Deleted successfully",
        }))?,
    ))
}

fn artloom_compat_fetch_mcp_registry(path: &str, endpoint: &str) -> Result<(u16, String)> {
    let (status, body) = fetch_mcp_registry(path, endpoint)?;
    if status != 200 {
        return Ok((status, body));
    }
    let mut value =
        serde_json::from_str::<Value>(&body).unwrap_or_else(|_| json!({ "data": body }));
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "compatCommand".to_owned(),
            Value::String("fetch_mcp_registry".to_owned()),
        );
    } else {
        value = json!({
            "compatCommand": "fetch_mcp_registry",
            "data": value,
        });
    }
    Ok((200, serde_json::to_string(&value)?))
}

fn fetch_mcp_registry(path: &str, endpoint: &str) -> Result<(u16, String)> {
    let search = query_value(path, "search");
    let cursor = query_value(path, "cursor");
    let limit = query_value(path, "limit").and_then(|value| value.parse::<u32>().ok());
    let url = build_mcp_registry_url(endpoint, search.as_deref(), limit, cursor.as_deref());

    let client = reqwest::blocking::Client::builder()
        .user_agent("Loom/0.1 MCP Registry Client")
        .timeout(Duration::from_secs(20))
        .build()
        .context("build MCP Registry client")?;
    let response = match client.get(&url).send() {
        Ok(response) => response,
        Err(error) => {
            return structured_error(
                502,
                json!({
                    "code": "mcp_registry_unavailable",
                    "message": format!("failed to fetch MCP Registry: {error}"),
                    "url": url,
                }),
            );
        }
    };
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return structured_error(
            502,
            json!({
                "code": "mcp_registry_error",
                "message": format!("MCP Registry returned HTTP {status}: {body}"),
                "url": url,
            }),
        );
    }
    let value = match response.json::<Value>() {
        Ok(value) => value,
        Err(error) => {
            return structured_error(
                502,
                json!({
                    "code": "mcp_registry_invalid_json",
                    "message": format!("MCP Registry returned invalid JSON: {error}"),
                    "url": url,
                }),
            );
        }
    };

    Ok((200, serde_json::to_string(&value)?))
}

fn test_mcp_connection(body: &str) -> Result<(u16, String)> {
    let config: McpServerConfig = match serde_json::from_str(body) {
        Ok(config) => config,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "compatCommand": "test_mcp_connection",
                    "code": "invalid_mcp_server",
                    "message": format!("invalid MCP server config: {error}"),
                }),
            );
        }
    };
    let config = normalize_mcp_server_config(config);

    let mut client = match StdioMcpClient::spawn(&config) {
        Ok(client) => client,
        Err(error) => {
            return Ok((
                200,
                serde_json::to_string(&json!({
                    "compatCommand": "test_mcp_connection",
                    "success": false,
                    "tools": [],
                    "error": error.to_string(),
                }))?,
            ));
        }
    };
    let server_info = match client.initialize() {
        Ok(server_info) => server_info,
        Err(error) => {
            return Ok((
                200,
                serde_json::to_string(&json!({
                    "compatCommand": "test_mcp_connection",
                    "success": false,
                    "tools": [],
                    "error": error.to_string(),
                }))?,
            ));
        }
    };
    let tools_result = match client.list_tools() {
        Ok(tools) => tools,
        Err(error) => {
            return Ok((
                200,
                serde_json::to_string(&json!({
                    "compatCommand": "test_mcp_connection",
                    "success": false,
                    "tools": [],
                    "server_info": server_info,
                    "serverInfo": server_info,
                    "error": error.to_string(),
                }))?,
            ));
        }
    };
    let tools = tools_result
        .get("tools")
        .cloned()
        .unwrap_or_else(|| json!([]));

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "test_mcp_connection",
            "success": true,
            "tools": tools,
            "server_info": server_info,
            "serverInfo": server_info,
        }))?,
    ))
}

fn artloom_compat_call_mcp_tool(body: &str) -> Result<(u16, String)> {
    let request: ArtLoomCompatCallMcpToolRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let command = request.command.trim();
    let tool_name = request.tool_name.trim();
    if command.is_empty() {
        return invalid_request("command is required");
    }
    if tool_name.is_empty() {
        return invalid_request("toolName is required");
    }

    let config = McpServerConfig {
        id: "artloom-compat-direct".to_owned(),
        name: "ArtLoom Compat Direct MCP".to_owned(),
        description: "One-shot ArtLoom call_mcp_tool compatibility server".to_owned(),
        command: command.to_owned(),
        args: request.args,
        env: request.env,
        enabled: true,
    };
    let mut client = match StdioMcpClient::spawn(&config) {
        Ok(client) => client,
        Err(error) => {
            return structured_error(
                502,
                json!({
                    "compatCommand": "call_mcp_tool",
                    "code": "mcp_spawn_failed",
                    "message": error.to_string(),
                }),
            );
        }
    };
    if let Err(error) = client.initialize() {
        return structured_error(
            502,
            json!({
                "compatCommand": "call_mcp_tool",
                "code": "mcp_initialize_failed",
                "message": error.to_string(),
            }),
        );
    }
    let result = match client.call_tool(tool_name, request.tool_args) {
        Ok(result) => result,
        Err(error) => {
            return structured_error(
                502,
                json!({
                    "compatCommand": "call_mcp_tool",
                    "code": "mcp_tool_call_failed",
                    "message": error.to_string(),
                }),
            );
        }
    };

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "call_mcp_tool",
            "status": "succeeded",
            "jsonrpc": "2.0",
            "id": 3,
            "result": result,
        }))?,
    ))
}

fn check_mcp_package_installed(body: &str) -> Result<(u16, String)> {
    let request: McpPackageCheckRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let module_name = request.module_name.trim();
    if !is_safe_python_module_name(module_name) {
        return invalid_request("moduleName must be a dotted Python module identifier");
    }

    let python = resolve_mcp_package_python();
    let output = Command::new(&python)
        .args(["-c", &format!("import {module_name}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let value = match output {
        Ok(output) => json!({
            "compatCommand": "check_mcp_package_installed",
            "installed": output.status.success(),
            "module": module_name,
            "python": python.to_string_lossy(),
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
        }),
        Err(error) => json!({
            "compatCommand": "check_mcp_package_installed",
            "installed": false,
            "module": module_name,
            "python": python.to_string_lossy(),
            "error": error.to_string(),
        }),
    };

    Ok((200, serde_json::to_string(&value)?))
}

fn build_mcp_package_install_plan(body: &str) -> Result<(u16, String)> {
    let request: McpPackageInstallPlanRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let package_name = request.package_name.trim();
    if !is_safe_python_package_name(package_name) {
        return invalid_request("packageName must be a safe pip package specifier");
    }
    let python = resolve_mcp_package_python();
    let command = vec![
        python.to_string_lossy().to_string(),
        "-m".to_owned(),
        "pip".to_owned(),
        "install".to_owned(),
        "--upgrade".to_owned(),
        package_name.to_owned(),
    ];

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "install_mcp_package",
            "package": package_name,
            "sideEffect": false,
            "mode": "safe-preview",
            "command": command,
            "message": "Install plan prepared. Loom does not run arbitrary package installation from this compatibility preview.",
        }))?,
    ))
}

fn resolve_mcp_package_python() -> PathBuf {
    if let Some(path) = std::env::var_os("LOOM_PYTHON").map(PathBuf::from) {
        return path;
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(base) = current_exe.parent() {
            let packaged = base.join("bin").join("python-embed").join("python.exe");
            if packaged.exists() {
                return packaged;
            }
        }
    }
    PathBuf::from(if cfg!(windows) {
        "python.exe"
    } else {
        "python3"
    })
}

fn is_safe_python_module_name(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            let mut chars = segment.chars();
            let Some(first) = chars.next() else {
                return false;
            };
            (first == '_' || first.is_ascii_alphabetic())
                && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        })
}

fn is_safe_python_package_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '[' | ']' | '='))
}

fn list_tools(tool_registry: &ToolRegistry) -> Result<(u16, String)> {
    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools,
        Err(error) => return tool_registry_error_response(error),
    };
    Ok((200, serde_json::to_string(&json!({ "tools": tools }))?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallArtRequest {
    /// The art package zip, base64-encoded (data URL or raw base64).
    zip_base64: String,
}

// Install an art package (zip) into the registry: extracts to <root>/arts/<id>/,
// checks the framework is ready, and registers the ToolDefinition.
fn install_art(
    body: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<InstallArtRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let zip_bytes = match loom_image_io::decode_data_url_bytes(&request.zip_base64) {
        Ok(bytes) => bytes,
        Err(error) => return invalid_request(format!("decode art package: {error}")),
    };
    match loom_tool_registry::install::install_art_from_zip(
        &zip_bytes,
        control_plane_root,
        framework_registry,
        tool_registry,
    ) {
        Ok(report) => {
            let _ = broadcast_artloom_compat_arts_updated(hook_bridge);
            Ok((200, serde_json::to_string(&json!({ "report": report }))?))
        }
        Err(loom_tool_registry::install::ArtInstallError::FrameworkNotReady {
            art_id,
            framework,
            reason,
        }) => structured_error(
            409,
            json!({
                "code": "framework_not_ready",
                "message": format!("art `{art_id}` 需要框架 `{framework}`（未{reason}），请先安装该框架"),
                "framework": framework,
            }),
        ),
        Err(loom_tool_registry::install::ArtInstallError::InvalidArtId(id)) => {
            invalid_request(format!("invalid art id `{id}`"))
        }
        Err(error) => structured_error(
            400,
            json!({ "code": "art_install_failed", "message": error.to_string() }),
        ),
    }
}

// Resolve the remote art store base URL: explicit `?store=`/`store` field wins,
// else the LOOM_ART_STORE_URL env var.
fn resolve_art_store_url(explicit: Option<&str>) -> Option<String> {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            std::env::var("LOOM_ART_STORE_URL")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
}

fn art_store_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .user_agent("Loom/0.1 Art Store Client")
        .timeout(Duration::from_secs(30))
        .build()
        .context("build art store client")
}

// Proxy the remote art store catalog (GET {store}/catalog).
fn fetch_art_store_catalog(path: &str) -> Result<(u16, String)> {
    let Some(store) = resolve_art_store_url(query_value(path, "store").as_deref()) else {
        return structured_error(
            400,
            json!({ "code": "art_store_not_configured", "message": "未配置 art 商店地址（LOOM_ART_STORE_URL 或 ?store=）" }),
        );
    };
    let url = format!("{}/catalog", store.trim_end_matches('/'));
    let client = art_store_client()?;
    match client.get(&url).send().and_then(|r| r.error_for_status()) {
        Ok(response) => {
            let body = response.text().unwrap_or_default();
            Ok((200, body))
        }
        Err(error) => structured_error(
            502,
            json!({ "code": "art_store_unavailable", "message": format!("获取 art 商店目录失败：{error}"), "url": url }),
        ),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallFromStoreRequest {
    art_id: String,
    #[serde(default)]
    store: Option<String>,
}

// Install an art (and its dependents) from the remote store: fetch the root art
// zip, then recursively fetch/install dependent arts by id.
fn install_art_from_store(
    body: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<InstallFromStoreRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let Some(store) = resolve_art_store_url(request.store.as_deref()) else {
        return structured_error(
            400,
            json!({ "code": "art_store_not_configured", "message": "未配置 art 商店地址" }),
        );
    };
    let store = store.trim_end_matches('/').to_owned();
    let client = match art_store_client() {
        Ok(client) => client,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "internal", "message": error.to_string() }),
            )
        }
    };
    let fetch_zip =
        |id: &str| -> std::result::Result<Vec<u8>, loom_tool_registry::install::ArtInstallError> {
            let url = format!("{store}/arts/{}.zip", id);
            let response = client
                .get(&url)
                .send()
                .and_then(|r| r.error_for_status())
                .map_err(|error| {
                    loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                        "fetch `{id}` from store: {error}"
                    ))
                })?;
            response.bytes().map(|b| b.to_vec()).map_err(|error| {
                loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                    "read `{id}` bytes: {error}"
                ))
            })
        };

    let root_zip = match fetch_zip(&request.art_id) {
        Ok(bytes) => bytes,
        Err(error) => {
            return structured_error(
                502,
                json!({ "code": "art_store_unavailable", "message": error.to_string() }),
            )
        }
    };
    match loom_tool_registry::install::install_art_recursive(
        &root_zip,
        control_plane_root,
        framework_registry,
        tool_registry,
        &fetch_zip,
    ) {
        Ok(reports) => {
            let _ = broadcast_artloom_compat_arts_updated(hook_bridge);
            Ok((200, serde_json::to_string(&json!({ "reports": reports }))?))
        }
        Err(loom_tool_registry::install::ArtInstallError::FrameworkNotReady {
            art_id,
            framework,
            reason,
        }) => structured_error(
            409,
            json!({
                "code": "framework_not_ready",
                "message": format!("art `{art_id}` 需要框架 `{framework}`（未{reason}），请先安装该框架"),
                "framework": framework,
            }),
        ),
        Err(error) => structured_error(
            400,
            json!({ "code": "art_install_failed", "message": error.to_string() }),
        ),
    }
}

// Package an installed art into a zip (manifest + its resource dir), returned as
// a base64 data URL so the frontend can export/save it.
fn package_art(
    id: &str,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let tool = match tool_registry.get_tool(id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({ "code": "tool_not_found", "message": format!("art `{id}` 不存在") }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let art_dir = control_plane_root.join("arts").join(id);
    match loom_tool_registry::install::package_art_to_zip(&tool, &art_dir) {
        Ok(bytes) => {
            use base64::Engine as _;
            let data = format!(
                "data:application/zip;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            );
            Ok((
                200,
                serde_json::to_string(&json!({ "artId": id, "zipBase64": data }))?,
            ))
        }
        Err(error) => structured_error(
            500,
            json!({ "code": "art_package_failed", "message": error.to_string() }),
        ),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishArtRequest {
    art_id: String,
    #[serde(default)]
    store: Option<String>,
}

// Publish a local art to the remote store: package it, then POST the zip to
// {store}/publish.
fn publish_art_to_store(
    body: &str,
    tool_registry: &ToolRegistry,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PublishArtRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let Some(store) = resolve_art_store_url(request.store.as_deref()) else {
        return structured_error(
            400,
            json!({ "code": "art_store_not_configured", "message": "未配置 art 商店地址" }),
        );
    };
    let tool = match tool_registry.get_tool(&request.art_id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({ "code": "tool_not_found", "message": format!("art `{}` 不存在", request.art_id) }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let art_dir = control_plane_root.join("arts").join(&request.art_id);
    let zip = match loom_tool_registry::install::package_art_to_zip(&tool, &art_dir) {
        Ok(bytes) => bytes,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "art_package_failed", "message": error.to_string() }),
            )
        }
    };
    let url = format!("{}/publish", store.trim_end_matches('/'));
    let client = match art_store_client() {
        Ok(client) => client,
        Err(error) => {
            return structured_error(
                500,
                json!({ "code": "internal", "message": error.to_string() }),
            )
        }
    };
    match client
        .post(&url)
        .header("Content-Type", "application/zip")
        .header("X-Art-Id", &request.art_id)
        .body(zip)
        .send()
        .and_then(|r| r.error_for_status())
    {
        Ok(_) => Ok((
            200,
            serde_json::to_string(&json!({ "artId": request.art_id, "published": true }))?,
        )),
        Err(error) => structured_error(
            502,
            json!({ "code": "art_store_publish_failed", "message": format!("发布失败：{error}"), "url": url }),
        ),
    }
}

fn list_frameworks(framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    let frameworks = framework_registry.statuses();
    Ok((
        200,
        serde_json::to_string(&json!({ "frameworks": frameworks }))?,
    ))
}

fn install_framework(id: &str, framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    match framework_registry.install(id) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_install_failed", id),
    }
}

fn uninstall_framework(id: &str, framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    match framework_registry.uninstall(id) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_uninstall_failed", id),
    }
}

fn install_framework_package(
    body: &str,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let package = match decode_framework_package_request(body) {
        Ok(package) => package,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_framework_package_request",
                    "message": error.to_string()
                }),
            )
        }
    };
    match framework_registry.install_framework_package_from_zip(&package) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_install_failed", "package"),
    }
}

fn upgrade_framework_package(
    id: &str,
    body: &str,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let package = match decode_framework_package_request(body) {
        Ok(package) => package,
        Err(error) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_framework_package_request",
                    "message": error.to_string()
                }),
            )
        }
    };
    match framework_registry.upgrade_framework_package(id, &package) {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(error, "framework_upgrade_failed", id),
    }
}

fn set_framework_enabled(
    id: &str,
    enabled: bool,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let result = if enabled {
        framework_registry.enable(id)
    } else {
        framework_registry.disable(id)
    };
    match result {
        Ok(status) => Ok((200, serde_json::to_string(&json!({ "framework": status }))?)),
        Err(error) => framework_error_response(
            error,
            if enabled {
                "framework_enable_failed"
            } else {
                "framework_disable_failed"
            },
            id,
        ),
    }
}

fn decode_framework_package_request(body: &str) -> Result<Vec<u8>> {
    let request: FrameworkPackageRequest = serde_json::from_str(body)
        .map_err(|error| anyhow::anyhow!("invalid framework package request: {error}"))?;
    let encoded = request.zip_base64.trim();
    if encoded.is_empty() {
        return Err(anyhow::anyhow!("zipBase64 is required"));
    }
    let encoded = encoded
        .strip_prefix("data:application/zip;base64,")
        .unwrap_or(encoded);
    BASE64
        .decode(encoded)
        .map_err(|error| anyhow::anyhow!("invalid zipBase64: {error}"))
}

fn framework_error_response(
    error: loom_tool_registry::framework::FrameworkError,
    operation_code: &str,
    id: &str,
) -> Result<(u16, String)> {
    use loom_tool_registry::framework::FrameworkError;
    match error {
        FrameworkError::UnknownFramework(unknown_id) => structured_error(
            404,
            json!({
                "code": "unknown_framework",
                "message": format!("未知框架 `{unknown_id}`")
            }),
        ),
        FrameworkError::FrameworkNotInstalled(framework_id) => structured_error(
            409,
            json!({
                "code": "framework_not_installed",
                "message": format!("框架 `{framework_id}` 未安装")
            }),
        ),
        FrameworkError::InvalidPackage {
            id: package_id,
            reason,
        } => structured_error(
            400,
            json!({
                "code": "invalid_framework_package",
                "message": format!("框架包 `{package_id}` 无效：{reason}")
            }),
        ),
        other => structured_error(
            500,
            json!({
                "code": operation_code,
                "framework": id,
                "message": other.to_string()
            }),
        ),
    }
}

// Report whether an art is runnable: its framework must be installed + ready.
fn tool_readiness(
    id: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let tool = match tool_registry.get_tool(id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({ "code": "tool_not_found", "message": format!("工具 `{id}` 不存在") }),
            )
        }
        Err(error) => return tool_registry_error_response(error),
    };
    let framework_id = loom_tool_registry::framework::framework_id_for_execution(&tool.execution);
    let installed = framework_registry.is_installed(framework_id);
    let (ready, detail) = if !tool.enabled {
        (false, "Art 已禁用".to_owned())
    } else if installed {
        framework_registry.readiness(framework_id)
    } else {
        (false, "框架未安装".to_owned())
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "toolId": id,
            "framework": framework_id,
            "frameworkInstalled": installed,
            "toolEnabled": tool.enabled,
            "ready": ready,
            "detail": detail,
        }))?,
    ))
}

fn put_tool(
    path_id: &str,
    body: &str,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let tool = match serde_json::from_str::<ToolDefinition>(body) {
        Ok(tool) => tool,
        Err(error) => return invalid_request(error.to_string()),
    };
    if tool.id != path_id {
        return id_mismatch("tool", path_id, &tool.id);
    }

    let saved = match tool_registry.save_tool(tool) {
        Ok(saved) => saved,
        Err(error) => return tool_registry_error_response(error),
    };
    broadcast_hook_bridge_json(hook_bridge, arts_updated_broadcast());
    Ok((200, serde_json::to_string(&json!({ "tool": saved }))?))
}

fn delete_tool(
    path_id: &str,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let deleted = match tool_registry.delete_tool(path_id) {
        Ok(deleted) => deleted,
        Err(error) => return tool_registry_error_response(error),
    };

    if !deleted {
        return structured_error(
            404,
            json!({
                "code": "tool_not_found",
                "message": format!("tool `{path_id}` was not found"),
                "tool_id": path_id,
            }),
        );
    }

    broadcast_hook_bridge_json(hook_bridge, arts_updated_broadcast());
    Ok((
        200,
        serde_json::to_string(&json!({ "toolId": path_id, "deleted": true }))?,
    ))
}

fn list_artloom_compat_arts(
    compat_command: &str,
    tool_registry: &ToolRegistry,
) -> Result<(u16, String)> {
    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools,
        Err(error) => return tool_registry_error_response(error),
    };
    let compat_tools: Vec<ToolDefinition> = tools
        .into_iter()
        .filter(is_artloom_compat_visible_tool)
        .collect();
    let arts: Vec<Value> = compat_tools
        .iter()
        .map(|tool| {
            if compat_command == "get_user_arts" {
                artloom_frontend_user_art_json(tool)
            } else {
                artloom_compat_art_json(tool)
            }
        })
        .collect();
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": compat_command,
            "arts": arts,
            "tools": compat_tools,
            "count": arts.len(),
        }))?,
    ))
}

fn list_enabled_artloom_compat_arts(tool_registry: &ToolRegistry) -> Result<(u16, String)> {
    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools,
        Err(error) => return tool_registry_error_response(error),
    };
    let compat_tools: Vec<ToolDefinition> = tools
        .into_iter()
        .filter(|tool| is_artloom_compat_visible_tool(tool) && tool.enabled)
        .collect();
    let arts: Vec<Value> = compat_tools.iter().map(artloom_compat_art_json).collect();
    let count = arts.len();
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "get_enabled_arts",
            "type": "arts",
            "data": arts.clone(),
            "arts": arts,
            "tools": compat_tools,
            "count": count,
        }))?,
    ))
}

fn sync_artloom_compat_arts(
    body: &str,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => return invalid_request(error.to_string()),
    };

    let response = match sync_artloom_compat_arts_value(&request, tool_registry) {
        Ok(response) => response,
        Err(response) => return response,
    };
    broadcast_hook_bridge_json(hook_bridge, arts_updated_broadcast());
    Ok((200, serde_json::to_string(&response)?))
}

fn broadcast_artloom_compat_arts_updated(
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    broadcast_hook_bridge_json(hook_bridge, arts_updated_broadcast());
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "broadcast_arts_updated",
            "broadcasted": true,
        }))?,
    ))
}

fn artloom_compat_native_process_art(body: &str) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request = match serde_json::from_str::<ArtLoomCompatNativeProcessArtRequest>(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    if request.art_id.trim().is_empty() {
        return invalid_request("native_process_art requires art_id");
    }
    if request.input_base64.trim().is_empty() {
        return invalid_request("native_process_art requires input_base64");
    }

    let result = loom_native_image::process_art(
        request.art_id.trim(),
        request.input_base64.trim(),
        request.params,
    );

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "native_process_art",
            "success": result.success,
            "output_base64": result.output_base64,
            "error": result.error,
            "processing_time_ms": result.processing_time_ms,
        }))?,
    ))
}

fn sync_artloom_compat_arts_value(
    request: &Value,
    tool_registry: &ToolRegistry,
) -> std::result::Result<Value, Result<(u16, String)>> {
    if let Some(arts) = request.get("arts").and_then(Value::as_array) {
        let existing_tools = match tool_registry.list_tools() {
            Ok(tools) => tools,
            Err(error) => return Err(tool_registry_error_response(error)),
        };

        let compat_ids: Vec<String> = existing_tools
            .iter()
            .filter(|tool| is_artloom_sync_managed_tool(tool))
            .map(|tool| tool.id.clone())
            .collect();

        for tool_id in compat_ids {
            if let Err(error) = tool_registry.delete_tool(&tool_id) {
                return Err(tool_registry_error_response(error));
            }
        }

        let mut synced_count = 0usize;
        let mut preserved_count = 0usize;
        for art in arts {
            let tool = match artloom_sync_art_to_tool(art) {
                Ok(tool) => tool,
                Err(response) => return Err(response),
            };
            if existing_tools
                .iter()
                .any(|existing| existing.id == tool.id && is_artloom_loom_local_tool(existing))
            {
                preserved_count += 1;
                continue;
            }
            match tool_registry.save_tool(tool) {
                Ok(_) => synced_count += 1,
                Err(error) => return Err(tool_registry_error_response(error)),
            }
        }

        let tools = match tool_registry.list_tools() {
            Ok(tools) => tools,
            Err(error) => return Err(tool_registry_error_response(error)),
        };
        let compat_tools: Vec<ToolDefinition> = tools
            .into_iter()
            .filter(is_artloom_compat_visible_tool)
            .collect();
        let compat_arts: Vec<Value> = compat_tools.iter().map(artloom_compat_art_json).collect();

        return Ok(json!({
            "compatCommand": "sync_user_arts",
            "synced": true,
            "sideEffect": true,
            "syncedCount": synced_count,
            "preservedCount": preserved_count,
            "arts": compat_arts,
            "tools": compat_tools,
            "count": compat_arts.len(),
            "message": if preserved_count > 0 {
                "Imported ArtLoom compat Arts into the Loom registry, preserved non-compat Loom tools, and kept Loom-local compat Arts as the source of truth on id collisions."
            } else {
                "Imported ArtLoom compat Arts into the Loom registry and preserved non-compat Loom tools."
            },
        }));
    }

    let tools = match tool_registry.list_tools() {
        Ok(tools) => tools,
        Err(error) => return Err(tool_registry_error_response(error)),
    };
    let compat_tools: Vec<ToolDefinition> = tools
        .into_iter()
        .filter(is_artloom_compat_visible_tool)
        .collect();
    let arts: Vec<Value> = compat_tools.iter().map(artloom_compat_art_json).collect();
    Ok(json!({
        "compatCommand": "sync_user_arts",
        "synced": true,
        "sideEffect": false,
        "arts": arts,
        "tools": compat_tools,
        "count": arts.len(),
        "message": "Loom registry is the source of truth for ArtLoom compat Arts; sync_user_arts only mirrors the current compat Arts and broadcasts arts_updated.",
    }))
}

fn get_artloom_compat_art(art_id: &str, tool_registry: &ToolRegistry) -> Result<(u16, String)> {
    let tool = match get_artloom_tool(art_id, tool_registry) {
        Ok(tool) => tool,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "get_art",
            "art": artloom_compat_art_json(&tool),
            "tool": tool,
        }))?,
    ))
}

fn set_artloom_compat_art_enabled(
    art_id: &str,
    enabled: bool,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let mut tool = match get_artloom_tool(art_id, tool_registry) {
        Ok(tool) => tool,
        Err(response) => return response,
    };
    tool.enabled = enabled;
    let saved = match tool_registry.save_tool(tool) {
        Ok(saved) => saved,
        Err(error) => return tool_registry_error_response(error),
    };
    broadcast_hook_bridge_json(hook_bridge, arts_updated_broadcast());
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": if enabled { "enable_art" } else { "disable_art" },
            "artId": art_id,
            "enabled": enabled,
            "art": artloom_compat_art_json(&saved),
            "tool": saved,
        }))?,
    ))
}

fn update_artloom_compat_art_defaults(
    art_id: &str,
    body: &str,
    tool_registry: &ToolRegistry,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut tool = match get_artloom_tool(art_id, tool_registry) {
        Ok(tool) => tool,
        Err(response) => return response,
    };
    apply_artloom_defaults_update(&mut tool, &request);
    let saved = match tool_registry.save_tool(tool) {
        Ok(saved) => saved,
        Err(error) => return tool_registry_error_response(error),
    };
    broadcast_hook_bridge_json(hook_bridge, arts_updated_broadcast());
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "update_art_defaults",
            "artId": art_id,
            "art": artloom_compat_art_json(&saved),
            "tool": saved,
        }))?,
    ))
}

fn get_artloom_tool(
    tool_id: &str,
    tool_registry: &ToolRegistry,
) -> std::result::Result<ToolDefinition, Result<(u16, String)>> {
    match tool_registry.get_tool(tool_id) {
        Ok(Some(tool)) if is_artloom_compat_visible_tool(&tool) => Ok(tool),
        Ok(Some(_)) => Err(tool_not_found_response(tool_id)),
        Ok(None) => Err(tool_not_found_response(tool_id)),
        Err(error) => Err(tool_registry_error_response(error)),
    }
}

fn tool_not_found_response(tool_id: &str) -> Result<(u16, String)> {
    structured_error(
        404,
        json!({
            "code": "tool_not_found",
            "message": format!("tool `{tool_id}` was not found"),
            "tool_id": tool_id,
        }),
    )
}

fn artloom_compat_art_json(tool: &ToolDefinition) -> Value {
    json!({
        "id": &tool.id,
        "art_id": &tool.id,
        "name": &tool.name,
        "label": &tool.name,
        "description": &tool.description,
        "icon": artloom_compat_icon(tool),
        "enabled": tool.enabled,
        "auto_process": artloom_compat_auto_process(tool),
        "execution_type": artloom_compat_execution_type(tool),
        "execution": artloom_compat_execution(tool),
        "inputs": &tool.inputs,
        "outputs": &tool.outputs,
        "params": &tool.params,
        "defaults": artloom_compat_defaults_json(tool),
        "metadata": &tool.metadata,
    })
}

fn artloom_frontend_user_art_json(tool: &ToolDefinition) -> Value {
    json!({
        "id": &tool.id,
        "name": &tool.name,
        "description": &tool.description,
        "category": "Adapter",
        "version": "1.0.0",
        "author": "User",
        "status": if tool.enabled { "active" } else { "inactive" },
        "iconColor": artloom_compat_icon(tool),
        "downloads": 0,
        "owned": true,
        "executionType": artloom_frontend_execution_type(tool),
        "execution": artloom_frontend_execution(tool),
        "autoProcess": artloom_compat_auto_process(tool),
        "inputs": &tool.inputs,
        "outputs": artloom_frontend_outputs(tool),
    })
}

fn artloom_compat_defaults_json(tool: &ToolDefinition) -> Value {
    tool.metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("artloomCompat"))
        .and_then(Value::as_object)
        .and_then(|compat| compat.get("defaults"))
        .cloned()
        .unwrap_or_else(|| artloom_param_defaults_json(tool))
}

fn artloom_param_defaults_json(tool: &ToolDefinition) -> Value {
    let mut defaults = serde_json::Map::new();
    for param in &tool.params {
        let Some(param_object) = param.as_object() else {
            continue;
        };
        let key = param_object
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| param_object.get("name").and_then(Value::as_str));
        let Some(key) = key else {
            continue;
        };
        if let Some(default) = param_object.get("default") {
            defaults.insert(key.to_owned(), default.clone());
        }
    }
    Value::Object(defaults)
}

fn artloom_compat_icon(tool: &ToolDefinition) -> Value {
    tool.metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("artloomCompat"))
        .and_then(Value::as_object)
        .and_then(|compat| compat.get("icon"))
        .cloned()
        // Empty string (not null) so string-typed consumers like Hook's
        // ArtDefinition.icon deserialize cleanly.
        .unwrap_or_else(|| Value::String(String::new()))
}

fn artloom_compat_auto_process(tool: &ToolDefinition) -> bool {
    tool.metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("artloomCompat"))
        .and_then(Value::as_object)
        .and_then(|compat| compat.get("autoProcess"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn artloom_frontend_execution_type(tool: &ToolDefinition) -> Value {
    artloom_compat_execution_type(tool)
}

fn artloom_compat_execution_type(tool: &ToolDefinition) -> Value {
    tool.metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("artloomCompat"))
        .and_then(Value::as_object)
        .and_then(|compat| compat.get("executionType"))
        .cloned()
        .unwrap_or_else(|| json!(artloom_execution_type_name(&tool.execution)))
}

fn artloom_frontend_execution(tool: &ToolDefinition) -> Value {
    artloom_compat_execution(tool)
}

fn artloom_compat_execution(tool: &ToolDefinition) -> Value {
    tool.metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("artloomCompat"))
        .and_then(Value::as_object)
        .and_then(|compat| compat.get("execution"))
        .cloned()
        .unwrap_or_else(|| json!(&tool.execution))
}

fn artloom_frontend_outputs(tool: &ToolDefinition) -> Value {
    artloom_frontend_execution(tool)
        .as_object()
        .and_then(|execution| execution.get("outputs"))
        .cloned()
        .unwrap_or_else(|| json!(&tool.outputs))
}

fn artloom_execution_type_name(execution: &ToolExecution) -> &'static str {
    match execution {
        ToolExecution::CliWrapper { .. } => "cli_wrapper",
        ToolExecution::CloudApi { .. } => "cloud_api",
        ToolExecution::Script { .. } => "script",
        ToolExecution::PythonArt { .. } => "python_art",
        ToolExecution::Mcp { .. } => "mcp",
        ToolExecution::Workflow { .. } => "workflow",
        ToolExecution::FrameworkArt { .. } => "framework_art",
    }
}

fn artloom_compat_metadata(
    icon: Option<Value>,
    auto_process: bool,
    defaults: Option<Value>,
    legacy_execution_type: Option<String>,
    legacy_execution: Option<Value>,
) -> Value {
    let mut compat = serde_json::Map::new();
    compat.insert("source".to_owned(), json!("artloom-compat"));
    compat.insert("managedBy".to_owned(), json!("sync_user_arts"));
    if let Some(icon) = icon {
        compat.insert("icon".to_owned(), icon);
    }
    if auto_process {
        compat.insert("autoProcess".to_owned(), json!(true));
    }
    if let Some(defaults) = defaults {
        compat.insert("defaults".to_owned(), defaults);
    }
    if let Some(execution_type) = legacy_execution_type {
        compat.insert("executionType".to_owned(), Value::String(execution_type));
    }
    if let Some(execution) = legacy_execution {
        compat.insert("execution".to_owned(), execution);
    }

    let mut root = serde_json::Map::new();
    root.insert("artloomCompat".to_owned(), Value::Object(compat));
    Value::Object(root)
}

fn artloom_compat_source(tool: &ToolDefinition) -> Option<&str> {
    tool.metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("artloomCompat"))
        .and_then(Value::as_object)
        .and_then(|compat| compat.get("source"))
        .and_then(Value::as_str)
}

fn is_artloom_compat_visible_tool(tool: &ToolDefinition) -> bool {
    matches!(
        artloom_compat_source(tool),
        Some("artloom-compat") | Some("loom-local")
    )
}

fn is_artloom_sync_managed_tool(tool: &ToolDefinition) -> bool {
    artloom_compat_source(tool) == Some("artloom-compat")
}

fn is_artloom_loom_local_tool(tool: &ToolDefinition) -> bool {
    artloom_compat_source(tool) == Some("loom-local")
}

fn artloom_object_str<'a>(
    object: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

fn artloom_value_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    value.as_object().and_then(|object| {
        keys.iter()
            .find_map(|key| object.get(*key).and_then(Value::as_str))
    })
}

fn artloom_legacy_execution_type(
    object: &serde_json::Map<String, Value>,
    execution: &Value,
) -> Option<String> {
    artloom_object_str(object, &["execution_type", "executionType"])
        .or_else(|| artloom_value_str(execution, &["type"]))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn artloom_legacy_cli_args(args: Option<&Value>) -> Vec<String> {
    match args {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(template)) => split_legacy_args_template(template),
        _ => Vec::new(),
    }
}

fn split_legacy_args_template(template: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for character in template.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }

        if character == '\\' {
            escaped = true;
            continue;
        }

        if matches!(character, '"' | '\'') {
            if quote == Some(character) {
                quote = None;
                continue;
            }
            if quote.is_none() {
                quote = Some(character);
                continue;
            }
        }

        if quote.is_none() && character.is_whitespace() {
            if !current.is_empty() {
                args.push(std::mem::take(&mut current));
            }
            continue;
        }

        current.push(character);
    }

    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        args.push(current);
    }

    args
}

// Convert an ArtLoom `headers` value into the JSON-string form the cloud_api
// executor expects (`serde_json::from_str::<HashMap<String,String>>`). ArtLoom
// sends headers as an object (`{"x-api-key":"{api_key}"}`) with `{api_key}`
// placeholders resolved from the execution's `api_key` field; the executor
// wants a JSON string. A value already a string is passed through untouched.
fn artloom_headers_to_json_string(execution: &Value) -> Option<String> {
    let headers = execution.get("headers")?;
    if let Some(text) = headers.as_str() {
        let trimmed = text.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_owned());
    }
    let object = headers.as_object()?;
    let api_key = execution
        .get("api_key")
        .or_else(|| execution.get("apiKey"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut map = serde_json::Map::new();
    for (name, value) in object {
        let Some(raw) = value.as_str() else {
            continue;
        };
        // Resolve the ArtLoom `{api_key}` placeholder against the execution's key.
        let resolved = raw.replace("{api_key}", api_key);
        map.insert(name.clone(), Value::String(resolved));
    }
    if map.is_empty() {
        return None;
    }
    Some(Value::Object(map).to_string())
}

// Convert an ArtLoom `body` value into the JSON-string form the cloud_api
// executor expects: a flat `{ "<field>": "<template-or-value>" }` map encoded
// as a JSON string. ArtLoom sends `body` as an array of field descriptors:
//   - the input image field (`execution_type: image_buffer` / `source: input`)
//     becomes `"<name>": "{{inputs.input.path}}"` so the executor uploads the
//     temp input file (the `.path}}` suffix marks it a file field);
//   - every other field becomes `"<name>": "<default>"` (falling back to a
//     `{{name}}` template so a runtime arg can fill it).
// A value already a JSON string is passed through untouched.
fn artloom_body_to_json_string(execution: &Value) -> Option<String> {
    let body = execution.get("body")?;
    if let Some(text) = body.as_str() {
        let trimmed = text.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_owned());
    }
    let array = body.as_array()?;
    let mut map = serde_json::Map::new();
    for field in array {
        let Some(field_object) = field.as_object() else {
            continue;
        };
        let Some(name) = field_object.get("name").and_then(Value::as_str) else {
            continue;
        };
        let execution_type = field_object
            .get("execution_type")
            .or_else(|| field_object.get("executionType"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let source = field_object
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let is_input_image = execution_type == "image_buffer"
            || execution_type == "image_path"
            || execution_type == "image_base64"
            || source == "input";
        let template = if is_input_image {
            // `.path}}` suffix makes the executor treat it as a file upload.
            "{{inputs.input.path}}".to_owned()
        } else if let Some(default) = field_object.get("default").and_then(Value::as_str) {
            default.to_owned()
        } else {
            format!("{{{{{name}}}}}")
        };
        map.insert(name.to_owned(), Value::String(template));
    }
    if map.is_empty() {
        return None;
    }
    Some(Value::Object(map).to_string())
}

fn artloom_sync_execution_to_tool_execution(
    tool_id: &str,
    object: &serde_json::Map<String, Value>,
) -> std::result::Result<ToolExecution, Result<(u16, String)>> {
    let execution = object.get("execution").ok_or_else(|| {
        invalid_request(format!(
            "sync_user_arts art `{tool_id}` is missing `execution`"
        ))
    })?;

    match serde_json::from_value::<ToolExecution>(execution.clone()) {
        Ok(execution) => return Ok(execution),
        Err(parse_error) => {
            let Some(execution_type) = artloom_legacy_execution_type(object, execution) else {
                return Err(invalid_request(format!(
                    "sync_user_arts art `{tool_id}` has invalid `execution`: {parse_error}"
                )));
            };

            let execution_object = execution.as_object().ok_or_else(|| {
                invalid_request(format!(
                    "sync_user_arts art `{tool_id}` has invalid `execution`: expected object"
                ))
            })?;

            match execution_type.as_str() {
                "cli_wrapper" | "cli" => Ok(ToolExecution::CliWrapper {
                    command: artloom_value_str(execution, &["command"])
                        .unwrap_or_default()
                        .to_owned(),
                    args: artloom_legacy_cli_args(execution_object.get("args")),
                }),
                "cloud_api" => Ok(ToolExecution::CloudApi {
                    endpoint: artloom_value_str(execution, &["endpoint", "url"])
                        .unwrap_or_default()
                        .to_owned(),
                    method: artloom_value_str(execution, &["method"])
                        .unwrap_or("POST")
                        .to_owned(),
                    content_type: artloom_value_str(execution, &["contentType", "content_type"])
                        .map(str::to_owned),
                    headers: artloom_headers_to_json_string(execution),
                    body: artloom_body_to_json_string(execution),
                }),
                "mcp" => Ok(ToolExecution::Mcp {
                    server_id: artloom_value_str(execution, &["serverId", "server_id", "server"])
                        .unwrap_or_default()
                        .to_owned(),
                    tool_name: artloom_value_str(execution, &["toolName", "tool_name"])
                        .unwrap_or_default()
                        .to_owned(),
                }),
                "workflow" => {
                    let workflow_bindings = execution_object
                        .get("workflowBindings")
                        .or_else(|| execution_object.get("workflow_bindings"))
                        .cloned()
                        .and_then(|value| {
                            serde_json::from_value::<WorkflowExecutionBindings>(value).ok()
                        });
                    Ok(ToolExecution::Workflow {
                        workflow_id: artloom_value_str(execution, &["workflowId", "workflow_id"])
                            .unwrap_or_default()
                            .to_owned(),
                        workflow_bindings,
                    })
                }
                "script" | "python" | "shader" => {
                    let script_path = artloom_value_str(
                        execution,
                        &["path", "scriptPath", "script_path", "pythonPath", "python_path"],
                    );
                    if let Some(script_path) = script_path {
                        return Ok(ToolExecution::Script {
                            path: script_path.to_owned(),
                        });
                    }

                    Ok(ToolExecution::PythonArt {
                        art_id: artloom_value_str(execution, &["artId", "art_id"])
                            .unwrap_or(tool_id)
                            .to_owned(),
                        art_path: artloom_value_str(execution, &["artPath", "art_path"])
                            .map(str::to_owned),
                    })
                }
                other => Err(invalid_request(format!(
                    "sync_user_arts art `{tool_id}` has unsupported legacy `execution_type`: {other}"
                ))),
            }
        }
    }
}

fn artloom_sync_art_to_tool(
    art: &Value,
) -> std::result::Result<ToolDefinition, Result<(u16, String)>> {
    let object = match art.as_object() {
        Some(object) => object,
        None => {
            return Err(invalid_request(
                "sync_user_arts expects each art to be a JSON object".to_owned(),
            ))
        }
    };

    let tool_id = object
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| object.get("art_id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            invalid_request("sync_user_arts art is missing a non-empty `id`".to_owned())
        })?;

    let tool_name = object
        .get("label")
        .and_then(Value::as_str)
        .or_else(|| object.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(tool_id);

    let tool_description = object
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let execution = artloom_sync_execution_to_tool_execution(tool_id, object)?;
    let legacy_execution_type = object
        .get("execution")
        .and_then(|execution| artloom_legacy_execution_type(object, execution));
    let legacy_execution = object.get("execution").cloned();

    let mut tool = ToolDefinition::new(tool_id, tool_name, tool_description, execution);
    tool.enabled = object
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    tool.inputs = object
        .get("inputs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    tool.outputs = object
        .get("outputs")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| {
            object
                .get("execution")
                .and_then(Value::as_object)
                .and_then(|execution| execution.get("outputs"))
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default();
    tool.params = object
        .get("params")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    apply_artloom_defaults_update(&mut tool, art);
    let auto_process = object
        .get("autoProcess")
        .and_then(Value::as_bool)
        .or_else(|| object.get("auto_process").and_then(Value::as_bool))
        .unwrap_or(false);
    tool.metadata = Some(artloom_compat_metadata(
        object
            .get("icon")
            .cloned()
            .or_else(|| object.get("iconColor").cloned()),
        auto_process,
        object.get("defaults").cloned(),
        legacy_execution_type,
        legacy_execution,
    ));

    match tool.validate() {
        Ok(()) => Ok(tool),
        Err(error) => Err(tool_registry_error_response(error)),
    }
}

fn set_artloom_compat_defaults_metadata(tool: &mut ToolDefinition, defaults: Value) {
    let metadata = tool
        .metadata
        .get_or_insert_with(|| json!({}))
        .as_object_mut();
    let Some(metadata) = metadata else {
        tool.metadata = Some(json!({ "artloomCompat": { "defaults": defaults } }));
        return;
    };
    let compat = metadata
        .entry("artloomCompat".to_owned())
        .or_insert_with(|| json!({}))
        .as_object_mut();
    let Some(compat) = compat else {
        metadata.insert("artloomCompat".to_owned(), json!({ "defaults": defaults }));
        return;
    };
    compat
        .entry("source".to_owned())
        .or_insert_with(|| json!("artloom-compat"));
    compat
        .entry("managedBy".to_owned())
        .or_insert_with(|| json!("sync_user_arts"));
    compat.insert("defaults".to_owned(), defaults);
}

fn apply_artloom_defaults_update(tool: &mut ToolDefinition, request: &Value) {
    if let Some(params) = request.get("params").and_then(Value::as_array) {
        tool.params = params.clone();
    }
    if let Some(inputs) = request.get("inputs").and_then(Value::as_array) {
        tool.inputs = inputs.clone();
    }
    if let Some(outputs) = request.get("outputs").and_then(Value::as_array) {
        tool.outputs = outputs.clone();
    }

    let defaults = request
        .get("defaults")
        .and_then(Value::as_object)
        .or_else(|| request.as_object());
    let Some(defaults) = defaults else {
        return;
    };
    if defaults.is_empty() {
        return;
    }

    set_artloom_compat_defaults_metadata(tool, Value::Object(defaults.clone()));

    if tool.params.is_empty() {
        tool.params = defaults
            .iter()
            .map(|(key, value)| {
                json!({
                    "id": key,
                    "name": key,
                    "default": value,
                })
            })
            .collect();
        return;
    }

    for param in &mut tool.params {
        let Some(param_object) = param.as_object_mut() else {
            continue;
        };
        let key = param_object
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| param_object.get("name").and_then(Value::as_str))
            .or_else(|| param_object.get("key").and_then(Value::as_str))
            .map(str::to_owned);
        if let Some(key) = key {
            if let Some(default_value) = defaults.get(&key) {
                param_object.insert("default".to_owned(), default_value.clone());
            }
        }
    }
}

fn list_python_arts() -> Result<(u16, String)> {
    let arts = collect_python_arts();
    Ok((200, serde_json::to_string(&json!({ "arts": arts }))?))
}

fn list_artloom_compat_installed_python_arts() -> Result<(u16, String)> {
    let arts = collect_python_arts();
    let count = arts.len();
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "list_installed_arts",
            "arts": arts,
            "count": count,
        }))?,
    ))
}

fn get_python_art(art_id: &str) -> Result<(u16, String)> {
    if let Some(art) = collect_python_arts().into_iter().find(|art| {
        art.get("art_id")
            .and_then(Value::as_str)
            .is_some_and(|candidate| candidate == art_id)
    }) {
        return Ok((200, serde_json::to_string(&json!({ "art": art }))?));
    }

    structured_error(
        404,
        json!({
            "code": "python_art_not_found",
            "message": format!("Python Art `{art_id}` was not found"),
            "art_id": art_id,
        }),
    )
}

fn python_engine_status() -> Result<(u16, String)> {
    let python = resolve_mcp_package_python();
    let launcher_path = resolve_python_launcher_path();
    let arts_dirs = python_arts_dirs();
    let available = launcher_path.as_ref().is_some_and(|path| path.is_file())
        && Command::new(&python)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "python_engine_status",
            "available": available,
            "python_exe": python.to_string_lossy(),
            "pythonExe": python.to_string_lossy(),
            "launcher_path": launcher_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            "launcherPath": launcher_path.as_ref().map(|path| path.to_string_lossy().to_string()),
            "launcherAvailable": launcher_path.as_ref().is_some_and(|path| path.is_file()),
            "arts_dir": arts_dirs
                .first()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            "artsDirs": arts_dirs
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            "installedArtCount": collect_python_arts().len(),
        }))?,
    ))
}

fn artloom_compat_execute_python_art(body: &str) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request: ArtLoomCompatPythonExecuteArtRequest = match serde_json::from_str(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let art_id = request.art_id.trim();
    if art_id.is_empty() {
        return invalid_request("execute_python_art requires art_id");
    }
    let params = match request.params {
        Value::Object(params) => Value::Object(params),
        Value::Null => json!({}),
        _ => return invalid_request("execute_python_art params must be a JSON object"),
    };

    match execute_python_art_raw(art_id, params, request.art_path.as_deref()) {
        Ok(mut response) => {
            if let Some(object) = response.as_object_mut() {
                object.insert(
                    "compatCommand".to_owned(),
                    Value::String("execute_python_art".to_owned()),
                );
            }
            Ok((200, serde_json::to_string(&response)?))
        }
        Err(message) => structured_error(
            500,
            json!({
                "code": "python_art_execution_failed",
                "message": message,
                "compatCommand": "execute_python_art",
                "art_id": art_id,
            }),
        ),
    }
}

fn artloom_compat_python_process_image(body: &str) -> Result<(u16, String)> {
    let started = Instant::now();
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request: ArtLoomCompatPythonProcessImageRequest = match serde_json::from_str(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let art_id = request.art_id.trim();
    if art_id.is_empty() {
        return invalid_request("python_process_image requires art_id");
    }
    if request.input_base64.trim().is_empty() {
        return invalid_request("python_process_image requires input_base64");
    }
    let mut params = match request.params {
        Value::Object(params) => params,
        Value::Null => serde_json::Map::new(),
        _ => return invalid_request("python_process_image params must be a JSON object"),
    };

    let image_bytes = match loom_image_io::decode_data_url_bytes(&request.input_base64) {
        Ok(bytes) => bytes,
        Err(error) => return invalid_request(format!("Failed to decode Base64: {error}")),
    };
    let temp_dir = std::env::temp_dir().join("loom_artloom_python");
    if let Err(error) = fs::create_dir_all(&temp_dir) {
        return artloom_python_process_image_result(
            false,
            None,
            None,
            started.elapsed().as_millis(),
            Some(format!("Failed to create temp directory: {error}")),
        );
    }
    let request_id = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let input_path = temp_dir.join(format!("{request_id}_input.png"));
    let output_path = temp_dir.join(format!("{request_id}_output.png"));
    if let Err(error) = fs::write(&input_path, image_bytes) {
        return artloom_python_process_image_result(
            false,
            None,
            None,
            started.elapsed().as_millis(),
            Some(format!("Failed to save input image: {error}")),
        );
    }

    params.insert(
        "input_path".to_owned(),
        Value::String(input_path.to_string_lossy().to_string()),
    );
    params.insert(
        "output_path".to_owned(),
        Value::String(output_path.to_string_lossy().to_string()),
    );

    let execution =
        execute_python_art_raw(art_id, Value::Object(params), request.art_path.as_deref());
    let processing_time_ms = started.elapsed().as_millis();
    let response = match execution {
        Ok(response) if response.get("status").and_then(Value::as_i64) == Some(200) => {
            if output_path.is_file() {
                match loom_image_io::read_image_path_as_data_url(&output_path) {
                    Ok(output_base64) => artloom_python_process_image_result(
                        true,
                        Some(output_base64),
                        Some(output_path.to_string_lossy().to_string()),
                        processing_time_ms,
                        None,
                    ),
                    Err(error) => artloom_python_process_image_result(
                        false,
                        None,
                        None,
                        processing_time_ms,
                        Some(format!("Failed to read output image: {error}")),
                    ),
                }
            } else {
                artloom_python_process_image_result(
                    false,
                    None,
                    None,
                    processing_time_ms,
                    Some("Output image was not created by plugin".to_owned()),
                )
            }
        }
        Ok(response) => artloom_python_process_image_result(
            false,
            None,
            None,
            processing_time_ms,
            Some(python_response_error_message(&response)),
        ),
        Err(error) => {
            artloom_python_process_image_result(false, None, None, processing_time_ms, Some(error))
        }
    };
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_path);
    response
}

fn artloom_python_process_image_result(
    success: bool,
    output_base64: Option<String>,
    output_path: Option<String>,
    processing_time_ms: u128,
    error: Option<String>,
) -> Result<(u16, String)> {
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "python_process_image",
            "success": success,
            "output_base64": output_base64,
            "output_path": output_path,
            "processing_time_ms": u64::try_from(processing_time_ms).unwrap_or(u64::MAX),
            "error": error,
        }))?,
    ))
}

fn python_response_error_message(response: &Value) -> String {
    response
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| "Python Art execution failed".to_owned())
}

fn execute_python_art_raw(
    art_id: &str,
    params: Value,
    art_path: Option<&str>,
) -> std::result::Result<Value, String> {
    let launcher_path =
        resolve_python_launcher_path().ok_or_else(|| "Python Art launcher not found".to_owned())?;
    let plugin_path = resolve_python_art_plugin_path(art_id, art_path)
        .ok_or_else(|| format!("Python Art `{art_id}` was not found"))?;
    let base_dir = launcher_path
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let request = json!({
        "request_id": format!("loom-python-art-compat-{art_id}"),
        "art_id": art_id,
        "plugin_path": plugin_path.to_string_lossy(),
        "params": params,
    });
    let output = Command::new(resolve_mcp_package_python())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg(&launcher_path)
        .arg(request.to_string())
        .current_dir(base_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Failed to spawn Python process: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stdout.is_empty() {
        return Err(format!(
            "Python process produced no output. Exit code: {:?}, stderr: {}",
            output.status.code(),
            stderr
        ));
    }
    serde_json::from_str::<Value>(&stdout)
        .map_err(|error| format!("Failed to parse Python response: {error}. Output: {stdout}"))
}

fn resolve_python_art_plugin_path(art_id: &str, art_path: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = art_path.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Some(path);
        }
    }
    collect_python_arts().into_iter().find_map(|art| {
        (art.get("art_id").and_then(Value::as_str) == Some(art_id))
            .then(|| art.get("path").and_then(Value::as_str).map(PathBuf::from))
            .flatten()
    })
}

fn artloom_compat_read_python_file(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonSourceReadRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let path = request.path.trim();
    if path.is_empty() {
        return invalid_request("filePath is required");
    }
    let (path, content) = match read_python_source_file(Path::new(path)) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "read_python_file",
            "filePath": display_path(&path),
            "path": display_path(&path),
            "content": content,
            "bytes": content.len(),
        }))?,
    ))
}

fn artloom_compat_read_art_json(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonArtJsonReadRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let art_path = request.art_path.trim();
    if art_path.is_empty() {
        return invalid_request("artPath is required");
    }
    let art_json_path = resolve_art_json_path(Path::new(art_path));
    let (path, art_json) = match read_art_json_file(&art_json_path) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "read_art_json",
            "artJsonPath": display_path(&path),
            "artJson": art_json,
        }))?,
    ))
}

fn artloom_compat_check_art_json_nearby(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonNearbyArtJsonRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let python_path = request.python_path.trim();
    if python_path.is_empty() {
        return invalid_request("pythonPath is required");
    }
    let (python_path, _) = match read_python_source_file(Path::new(python_path)) {
        Ok(result) => result,
        Err(response) => return response,
    };
    let Some(parent) = python_path.parent() else {
        return invalid_request("pythonPath must have a parent directory");
    };
    let art_json_path = parent.join("art.json");
    if !art_json_path.is_file() {
        return Ok((
            200,
            serde_json::to_string(&json!({
                "compatCommand": "check_art_json_nearby",
                "found": false,
                "pythonPath": display_path(&python_path),
                "artJson": Value::Null,
            }))?,
        ));
    }
    let (path, art_json) = match read_art_json_file(&art_json_path) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "check_art_json_nearby",
            "found": true,
            "pythonPath": display_path(&python_path),
            "artJsonPath": display_path(&path),
            "artJson": art_json,
        }))?,
    ))
}

fn prefetch_python_art_shader(body: &str) -> Result<(u16, String)> {
    let request: PythonShaderPrefetchRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let art_id = request.art_id.trim();
    if art_id.is_empty() {
        return invalid_request("artId is required");
    }

    let mut params = request
        .params
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);
    params
        .entry("output_mode".to_owned())
        .or_insert_with(|| json!("shader"));
    params
        .entry("mode".to_owned())
        .or_insert_with(|| json!("shader"));
    params
        .entry("reference_path".to_owned())
        .or_insert_with(|| json!(""));

    let tool = ToolDefinition::new(
        format!("prefetch-shader-{art_id}"),
        format!("Prefetch shader {art_id}"),
        "ArtLoom prefetch_shader compatibility probe",
        ToolExecution::PythonArt {
            art_id: art_id.to_owned(),
            art_path: request
                .art_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        },
    );
    let result = match execute_tool(&tool, &[], Value::Object(params)) {
        Ok(result) => result,
        Err(error) => return tool_registry_error_response(error),
    };
    let result = unwrap_prefetch_shader_payload(result);

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "prefetch_shader",
            "artId": art_id,
            "result": result,
        }))?,
    ))
}

fn unwrap_prefetch_shader_payload(result: Value) -> Value {
    if result.get("type").and_then(Value::as_str) == Some("shader") {
        return result;
    }

    let Some(text) = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
    else {
        return result;
    };

    let Ok(parsed) = serde_json::from_str::<Value>(text) else {
        return result;
    };

    if parsed.get("type").and_then(Value::as_str) == Some("shader")
        && parsed
            .get("vertex_shader")
            .and_then(Value::as_str)
            .is_some()
        && parsed
            .get("fragment_shader")
            .and_then(Value::as_str)
            .is_some()
    {
        parsed
    } else {
        result
    }
}

fn resolve_python_launcher_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        candidates.push(exe_dir.join("python").join("Launcher.py"));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("python").join("Launcher.py"));
        candidates.push(
            current_dir
                .join("Loom")
                .join("resources")
                .join("python")
                .join("Launcher.py"),
        );
    }
    for ancestor in PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors() {
        candidates.push(
            ancestor
                .join("resources")
                .join("python")
                .join("Launcher.py"),
        );
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn read_python_art_source(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonSourceReadRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let path = request.path.trim();
    if path.is_empty() {
        return invalid_request("path is required");
    }
    let (path, content) = match read_python_source_file(Path::new(path)) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "path": display_path(&path),
            "content": content,
            "bytes": content.len(),
        }))?,
    ))
}

fn read_python_art_json(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonArtJsonReadRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let art_path = request.art_path.trim();
    if art_path.is_empty() {
        return invalid_request("artPath is required");
    }
    let art_json_path = resolve_art_json_path(Path::new(art_path));
    let (path, art_json) = match read_art_json_file(&art_json_path) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "artJsonPath": display_path(&path),
            "artJson": art_json,
        }))?,
    ))
}

fn check_python_art_json_nearby(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonNearbyArtJsonRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let python_path = request.python_path.trim();
    if python_path.is_empty() {
        return invalid_request("pythonPath is required");
    }
    let (python_path, _) = match read_python_source_file(Path::new(python_path)) {
        Ok(result) => result,
        Err(response) => return response,
    };
    let Some(parent) = python_path.parent() else {
        return invalid_request("pythonPath must have a parent directory");
    };
    let art_json_path = parent.join("art.json");
    if !art_json_path.is_file() {
        return Ok((
            200,
            serde_json::to_string(&json!({
                "found": false,
                "pythonPath": display_path(&python_path),
            }))?,
        ));
    }
    let (path, art_json) = match read_art_json_file(&art_json_path) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "found": true,
            "pythonPath": display_path(&python_path),
            "artJsonPath": display_path(&path),
            "artJson": art_json,
        }))?,
    ))
}

fn infer_python_art_ports(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonInferPortsRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let (source_path, code) = if request.code.trim().is_empty() {
        let path = request.path.trim();
        if path.is_empty() {
            return invalid_request("code or path is required");
        }
        let (path, code) = match read_python_source_file(Path::new(path)) {
            Ok(result) => result,
            Err(response) => return response,
        };
        (Some(path), code)
    } else {
        if request.code.len() as u64 > MAX_PYTHON_SOURCE_BYTES {
            return structured_error(
                413,
                json!({
                    "code": "python_source_too_large",
                    "message": format!("Python source exceeds {MAX_PYTHON_SOURCE_BYTES} bytes"),
                }),
            );
        }
        (None, request.code)
    };
    let (inputs, outputs) = infer_python_ports_from_code(&code);
    Ok((
        200,
        serde_json::to_string(&json!({
            "path": source_path.map(|path| display_path(&path)),
            "inputs": inputs,
            "outputs": outputs,
        }))?,
    ))
}

fn read_python_source_file(
    path: &Path,
) -> std::result::Result<(PathBuf, String), Result<(u16, String)>> {
    let path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) => {
            return Err(structured_error(
                404,
                json!({
                    "code": "python_source_not_found",
                    "message": format!("Python source file was not found: {error}"),
                }),
            ));
        }
    };
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("py"))
    {
        return Err(invalid_request("path must point to a .py file"));
    }
    read_text_file_limited(&path, MAX_PYTHON_SOURCE_BYTES, "python_source_too_large")
}

fn read_art_json_file(path: &Path) -> std::result::Result<(PathBuf, Value), Result<(u16, String)>> {
    let path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) => {
            return Err(structured_error(
                404,
                json!({
                    "code": "art_json_not_found",
                    "message": format!("art.json was not found: {error}"),
                }),
            ));
        }
    };
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| !name.eq_ignore_ascii_case("art.json"))
    {
        return Err(invalid_request(
            "artPath must point to an art.json file or an Art directory",
        ));
    }
    let (path, content) = read_text_file_limited(&path, MAX_ART_JSON_BYTES, "art_json_too_large")?;
    let art_json = match serde_json::from_str::<Value>(&content) {
        Ok(json) => json,
        Err(error) => {
            return Err(invalid_request(format!(
                "failed to parse art.json: {error}"
            )));
        }
    };
    Ok((path, art_json))
}

fn read_text_file_limited(
    path: &Path,
    max_bytes: u64,
    too_large_code: &str,
) -> std::result::Result<(PathBuf, String), Result<(u16, String)>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(structured_error(
                404,
                json!({
                    "code": "file_not_found",
                    "message": format!("file was not found: {error}"),
                }),
            ));
        }
    };
    if !metadata.is_file() {
        return Err(invalid_request("path must point to a file"));
    }
    if metadata.len() > max_bytes {
        return Err(structured_error(
            413,
            json!({
                "code": too_large_code,
                "message": format!("file exceeds {max_bytes} bytes"),
                "bytes": metadata.len(),
            }),
        ));
    }
    match fs::read_to_string(path) {
        Ok(content) => Ok((path.to_path_buf(), content)),
        Err(error) => Err(structured_error(
            400,
            json!({
                "code": "file_read_failed",
                "message": format!("failed to read UTF-8 text file: {error}"),
            }),
        )),
    }
}

fn resolve_art_json_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("art.json")
    } else {
        path.to_path_buf()
    }
}

fn display_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{stripped}");
    }
    raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_owned()
}

fn infer_python_ports_from_code(code: &str) -> (Vec<Value>, Vec<Value>) {
    let mut inputs = Vec::<String>::new();
    collect_python_arg_names(code, "args.get(", &mut inputs);
    collect_python_arg_names(code, "args[", &mut inputs);

    let outputs = collect_python_return_object_keys(code);

    (
        inputs
            .into_iter()
            .map(|name| python_port_json(&name))
            .collect(),
        outputs
            .into_iter()
            .map(|name| python_port_json(&name))
            .collect(),
    )
}

fn collect_python_arg_names(code: &str, marker: &str, names: &mut Vec<String>) {
    let mut rest = code;
    while let Some(index) = rest.find(marker) {
        let after_marker = rest[index + marker.len()..].trim_start();
        let Some(quote) = after_marker
            .chars()
            .next()
            .filter(|quote| *quote == '"' || *quote == '\'')
        else {
            rest = &rest[index + marker.len()..];
            continue;
        };
        let after_quote = &after_marker[quote.len_utf8()..];
        if let Some(end_index) = after_quote.find(quote) {
            let name = &after_quote[..end_index];
            if is_python_identifier_like(name) && !names.iter().any(|existing| existing == name) {
                names.push(name.to_owned());
            }
            rest = &after_quote[end_index + quote.len_utf8()..];
        } else {
            break;
        }
    }
}

fn collect_python_return_object_keys(code: &str) -> Vec<String> {
    let Some(return_index) = code.find("return") else {
        return Vec::new();
    };
    let after_return = &code[return_index..];
    let Some(open_index) = after_return.find('{') else {
        return Vec::new();
    };
    let after_open = &after_return[open_index + 1..];
    let Some(close_index) = after_open.find('}') else {
        return Vec::new();
    };
    let object_body = &after_open[..close_index];
    let mut names = Vec::<String>::new();
    let mut rest = object_body;
    while let Some(quote_index) = rest.find(['"', '\'']) {
        let quote = rest[quote_index..]
            .chars()
            .next()
            .expect("quote char after find");
        let after_quote = &rest[quote_index + quote.len_utf8()..];
        let Some(end_index) = after_quote.find(quote) else {
            break;
        };
        let name = &after_quote[..end_index];
        let after_name = after_quote[end_index + quote.len_utf8()..].trim_start();
        if after_name.starts_with(':')
            && is_python_identifier_like(name)
            && !names.iter().any(|existing| existing == name)
        {
            names.push(name.to_owned());
        }
        rest = after_name;
    }
    names
}

fn is_python_identifier_like(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn python_port_json(name: &str) -> Value {
    let (ui_type, execution_type) = infer_python_port_type(name);
    json!({
        "name": name,
        "label": name,
        "type": ui_type,
        "execution_type": execution_type,
        "executionType": execution_type,
    })
}

fn infer_python_port_type(name: &str) -> (&'static str, &'static str) {
    let lower = name.to_ascii_lowercase();
    if [
        "path",
        "image",
        "file",
        "input",
        "output",
        "source",
        "reference",
        "result",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return ("image", "image_path");
    }
    if ["factor", "ratio", "strength", "alpha", "blend", "scale"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return ("float", "number");
    }
    if ["count", "num", "size", "clusters", "width", "height", "n_"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return ("int", "number");
    }
    ("string", "string")
}

fn collect_python_arts() -> Vec<Value> {
    let mut seen = HashMap::<String, ()>::new();
    let mut arts = Vec::new();
    for arts_dir in python_arts_dirs() {
        let Ok(entries) = fs::read_dir(&arts_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let art_json_path = path.join("art.json");
            if !art_json_path.is_file() {
                continue;
            }
            let Ok(content) = fs::read_to_string(&art_json_path) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<Value>(&content) else {
                continue;
            };
            let art_id = json
                .get("art_id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            if seen.contains_key(&art_id) {
                continue;
            }
            seen.insert(art_id.clone(), ());
            let label = json
                .get("label")
                .and_then(Value::as_str)
                .or_else(|| json.get("name").and_then(Value::as_str))
                .unwrap_or_else(|| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Python Art")
                });
            let description = json
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let version = json
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("1.0.0");
            arts.push(json!({
                "path": path.to_string_lossy(),
                "art_json_path": art_json_path.to_string_lossy(),
                "art_id": art_id,
                "label": label,
                "description": description,
                "version": version,
                "definition": json,
            }));
        }
    }
    arts.sort_by(|left, right| {
        let left_label = left
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right_label = right
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default();
        left_label.cmp(right_label)
    });
    arts
}

fn python_arts_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir.join("python").join("Arts"));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        dirs.push(current_dir.join("python").join("Arts"));
        dirs.push(
            current_dir
                .join("Loom")
                .join("resources")
                .join("python")
                .join("Arts"),
        );
    }
    for ancestor in PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors() {
        dirs.push(ancestor.join("resources").join("python").join("Arts"));
    }
    dirs
}

fn execute_registered_tool(
    tool_id: &str,
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    framework_registry: &FrameworkRegistry,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request = match serde_json::from_str::<ExecuteToolRequest>(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let tool = match tool_registry.get_tool(tool_id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            return structured_error(
                404,
                json!({
                    "code": "tool_not_found",
                    "message": format!("tool `{tool_id}` was not found"),
                    "tool_id": tool_id,
                }),
            );
        }
        Err(error) => return tool_registry_error_response(error),
    };
    if let ToolExecution::FrameworkArt { framework } = &tool.execution {
        if !tool.enabled {
            return structured_error(
                409,
                json!({
                    "code": "art_disabled",
                    "message": format!("Art {tool_id} 已禁用"),
                    "artId": tool_id,
                }),
            );
        }
        let (ready, detail) = framework_registry.readiness(framework);
        if !ready {
            return structured_error(
                409,
                json!({
                    "code": "framework_not_ready",
                    "message": format!("Art {tool_id} 的框架 {framework} 不可运行：{detail}"),
                    "framework": framework,
                    "artId": tool_id,
                }),
            );
        }
    }
    let servers = mcp_servers
        .lock()
        .map_err(|_| anyhow::anyhow!("lock MCP server store"))?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let result = match execute_tool_with_workflows(
        &tool,
        &servers,
        workflow_store,
        tool_registry,
        request.arguments,
    ) {
        Ok(result) => result,
        Err(error) => return workflow_runtime_error_response(error),
    };

    Ok((
        200,
        serde_json::to_string(&json!({
            "toolId": tool_id,
            "status": "succeeded",
            "result": result,
        }))?,
    ))
}

fn list_shared_images(shared_images: &SharedImageStoreHandle) -> Result<(u16, String)> {
    let images = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .list();
    Ok((200, serde_json::to_string(&json!({ "images": images }))?))
}

fn default_shared_memory_channels() -> u32 {
    4
}

fn list_shared_memory_buffers(shared_images: &SharedImageStoreHandle) -> Result<(u16, String)> {
    let images = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .list();
    let buffers: Vec<Value> = images.iter().map(shared_memory_buffer_info_json).collect();
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "shm_list_buffers",
            "buffers": buffers,
            "images": images,
        }))?,
    ))
}

fn create_shared_memory_buffer(
    body: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let request: SharedMemoryCreateBufferRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    if request.channels != 4 {
        return invalid_request(
            "Loom shared-memory compatibility buffers require rgba8 channels=4",
        );
    }
    let size = match rgba8_buffer_size(request.width, request.height) {
        Ok(size) => size,
        Err(message) => return invalid_request(message),
    };
    let data = vec![0_u8; size];
    let image = match shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .create_rgba8(request.width, request.height, data)
    {
        Ok(image) => image,
        Err(error) => return shared_image_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "shm_create_buffer",
            "handle": &image.handle,
            "handle_name": &image.handle,
            "buffer": shared_memory_buffer_info_json(&image),
            "image": &image,
        }))?,
    ))
}

fn get_shared_memory_buffer_info(
    handle: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let store = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?;
    let Some(image) = store.get(handle) else {
        return shared_image_error_response(SharedImageError::NotFound(handle.to_owned()));
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "shm_get_buffer_info",
            "handle": handle,
            "buffer": shared_memory_buffer_info_json(&image),
            "image": &image,
        }))?,
    ))
}

fn release_shared_memory_buffer(
    handle: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let released = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .release(handle);
    if !released {
        return shared_image_error_response(SharedImageError::NotFound(handle.to_owned()));
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "shm_release_buffer",
            "handle": handle,
            "released": true,
            "deleted": true,
        }))?,
    ))
}

fn shared_memory_buffer_info_json(info: &SharedImageInfo) -> Value {
    json!({
        "handle": &info.handle,
        "handle_name": &info.handle,
        "size": info.size,
        "width": info.width,
        "height": info.height,
        "format": shared_image_format_name(&info.format),
        "ref_count": 1,
    })
}

fn shared_image_format_name(format: &SharedImageFormat) -> &'static str {
    match format {
        SharedImageFormat::Rgba8 => "rgba8",
    }
}

fn rgba8_buffer_size(width: u32, height: u32) -> std::result::Result<usize, String> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "shared-memory dimensions overflow".to_owned())?;
    usize::try_from(pixels).map_err(|_| "shared-memory buffer is too large".to_owned())
}

fn create_shared_image(
    body: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<Value>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut store = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?;

    let image = if let Some(data_url) = request.get("dataBase64").and_then(Value::as_str) {
        match store.create_from_data_url(data_url) {
            Ok(image) => image,
            Err(error) => return shared_image_error_response(error),
        }
    } else {
        let Some(width) = value_u32(&request, "width") else {
            return invalid_request("shared image width is required");
        };
        let Some(height) = value_u32(&request, "height") else {
            return invalid_request("shared image height is required");
        };
        let format = request
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("rgba8");
        if format != "rgba8" {
            return invalid_request(format!("unsupported shared image format: {format}"));
        }
        let Some(data) = request.get("data").and_then(Value::as_array) else {
            return invalid_request("shared image data array is required");
        };
        let mut bytes = Vec::with_capacity(data.len());
        for value in data {
            let Some(byte) = value.as_u64().and_then(|value| u8::try_from(value).ok()) else {
                return invalid_request("shared image data must contain bytes");
            };
            bytes.push(byte);
        }
        match store.create_rgba8(width, height, bytes) {
            Ok(image) => image,
            Err(error) => return shared_image_error_response(error),
        }
    };

    Ok((200, serde_json::to_string(&json!({ "image": image }))?))
}

fn get_shared_image(handle: &str, shared_images: &SharedImageStoreHandle) -> Result<(u16, String)> {
    let store = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?;
    let Some(image) = store.get(handle) else {
        return shared_image_error_response(SharedImageError::NotFound(handle.to_owned()));
    };
    let data = match store.read_rgba8(handle) {
        Ok(data) => data,
        Err(error) => return shared_image_error_response(error),
    };
    let data_base64 = match store.read_png_data_url(handle) {
        Ok(data_url) => data_url,
        Err(error) => return shared_image_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "image": image,
            "data": data,
            "dataBase64": data_base64,
        }))?,
    ))
}

fn delete_shared_image(
    handle: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let deleted = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .release(handle);
    if !deleted {
        return shared_image_error_response(SharedImageError::NotFound(handle.to_owned()));
    }
    Ok((200, serde_json::to_string(&json!({ "deleted": true }))?))
}

fn convert_image_helper(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<ImageHelperConvertRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let source_type = request.source_type.as_str();
    let target_type = request.target_type.as_str();

    match (source_type, target_type) {
        ("image_base64", "image_buffer") => {
            let data = match request.data.as_ref().and_then(Value::as_str) {
                Some(data) => data,
                None => return invalid_request("image_base64 data is required"),
            };
            let rgba = match loom_image_io::decode_image_base64_to_rgba8(data) {
                Ok(rgba) => rgba,
                Err(error) => return image_io_error_response(error),
            };
            image_buffer_response(rgba)
        }
        ("image_base64", "image_base64") => {
            let data = match request.data.as_ref().and_then(Value::as_str) {
                Some(data) => data,
                None => return invalid_request("image_base64 data is required"),
            };
            let rgba = match loom_image_io::decode_image_base64_to_rgba8(data) {
                Ok(rgba) => rgba,
                Err(error) => return image_io_error_response(error),
            };
            let data_base64 =
                match loom_image_io::rgba8_to_png_data_url(rgba.width, rgba.height, &rgba.data) {
                    Ok(data_url) => data_url,
                    Err(error) => return image_io_error_response(error),
                };
            Ok((
                200,
                serde_json::to_string(&json!({ "dataBase64": data_base64 }))?,
            ))
        }
        ("image_path", "image_base64") => {
            let path = match request.path.as_deref() {
                Some(path) => path,
                None => return invalid_request("image_path path is required"),
            };
            let data_base64 = match loom_image_io::read_image_path_as_data_url(path) {
                Ok(data_url) => data_url,
                Err(error) => return image_io_error_response(error),
            };
            Ok((
                200,
                serde_json::to_string(&json!({ "dataBase64": data_base64 }))?,
            ))
        }
        ("image_path", "image_buffer") => {
            let path = match request.path.as_deref() {
                Some(path) => path,
                None => return invalid_request("image_path path is required"),
            };
            let data_url = match loom_image_io::read_image_path_as_data_url(path) {
                Ok(data_url) => data_url,
                Err(error) => return image_io_error_response(error),
            };
            let rgba = match loom_image_io::decode_image_base64_to_rgba8(&data_url) {
                Ok(rgba) => rgba,
                Err(error) => return image_io_error_response(error),
            };
            image_buffer_response(rgba)
        }
        ("image_buffer", "image_base64") => {
            let Some(width) = request.width else {
                return invalid_request("image_buffer width is required");
            };
            let Some(height) = request.height else {
                return invalid_request("image_buffer height is required");
            };
            let data = match request.data.as_ref().and_then(value_byte_array) {
                Some(data) => data,
                None => return invalid_request("image_buffer data array is required"),
            };
            let data_base64 = match loom_image_io::rgba8_to_png_data_url(width, height, &data) {
                Ok(data_url) => data_url,
                Err(error) => return image_io_error_response(error),
            };
            Ok((
                200,
                serde_json::to_string(&json!({ "dataBase64": data_base64 }))?,
            ))
        }
        ("image_buffer", "image_buffer") => {
            let Some(width) = request.width else {
                return invalid_request("image_buffer width is required");
            };
            let Some(height) = request.height else {
                return invalid_request("image_buffer height is required");
            };
            let data = match request.data.as_ref().and_then(value_byte_array) {
                Some(data) => data,
                None => return invalid_request("image_buffer data array is required"),
            };
            let size = data.len();
            Ok((
                200,
                serde_json::to_string(&json!({
                    "image": {
                        "width": width,
                        "height": height,
                        "format": "rgba8",
                        "size": size
                    },
                    "data": data
                }))?,
            ))
        }
        _ => invalid_request(format!(
            "unsupported image helper conversion: {source_type} to {target_type}"
        )),
    }
}

fn image_buffer_response(rgba: loom_image_io::RgbaImageData) -> Result<(u16, String)> {
    Ok((
        200,
        serde_json::to_string(&json!({
            "image": {
                "width": rgba.width,
                "height": rgba.height,
                "format": rgba.format,
                "size": rgba.size
            },
            "data": rgba.data
        }))?,
    ))
}

fn value_byte_array(value: &Value) -> Option<Vec<u8>> {
    value.as_array().map(|values| {
        values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|byte| u8::try_from(byte).ok())
                    .ok_or(())
            })
            .collect::<Result<Vec<_>, _>>()
            .ok()
    })?
}

fn image_io_error_response(error: loom_image_io::ImageIoError) -> Result<(u16, String)> {
    invalid_request(error.to_string())
}

fn value_u32(value: &Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn shared_image_error_response(error: SharedImageError) -> Result<(u16, String)> {
    match error {
        SharedImageError::NotFound(handle) => structured_error(
            404,
            json!({
                "code": "shared_image_not_found",
                "message": format!("shared image `{handle}` was not found"),
                "handle": handle,
            }),
        ),
        SharedImageError::Platform(message) => structured_error(
            500,
            json!({
                "code": "shared_image_platform_error",
                "message": message,
            }),
        ),
        other => invalid_request(other.to_string()),
    }
}

fn get_artloom_compat_settings(
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> Result<(u16, String)> {
    let store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock ArtLoom compat settings"))?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "get_settings",
            "settings": store.settings,
        }))?,
    ))
}

fn put_artloom_compat_settings(
    body: &str,
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> Result<(u16, String)> {
    let settings: ArtLoomCompatSettings = match serde_json::from_str(body) {
        Ok(settings) => settings,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock ArtLoom compat settings"))?;
    store.settings = settings.clone();
    store.save()?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "update_settings",
            "settings": settings,
            "saved": true,
        }))?,
    ))
}

fn get_artloom_compat_shortcuts(
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> Result<(u16, String)> {
    let store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock ArtLoom compat settings"))?;
    let mut shortcuts = store
        .settings
        .shortcuts
        .values()
        .cloned()
        .collect::<Vec<_>>();
    shortcuts.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "get_shortcuts",
            "shortcuts": shortcuts,
        }))?,
    ))
}

fn put_artloom_compat_shortcut(
    path_id: &str,
    body: &str,
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> Result<(u16, String)> {
    let shortcut: ArtLoomShortcutConfig = match serde_json::from_str(body) {
        Ok(shortcut) => shortcut,
        Err(error) => return invalid_request(error.to_string()),
    };
    if shortcut.id != path_id {
        return id_mismatch("shortcut", path_id, &shortcut.id);
    }
    let mut store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock ArtLoom compat settings"))?;
    store
        .settings
        .shortcuts
        .insert(shortcut.id.clone(), shortcut.clone());
    store.save()?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "update_shortcut",
            "shortcut": shortcut,
            "saved": true,
        }))?,
    ))
}

fn get_artloom_compat_app_paths() -> Result<(u16, String)> {
    let data_dir = std::env::var_os("LOOM_CONTROL_PLANE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(default_control_plane_root);
    let config_dir = data_dir.join("settings");
    let log_dir = std::env::var_os("LOOM_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("logs"));
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "get_app_paths",
            "dataDir": data_dir.to_string_lossy(),
            "configDir": config_dir.to_string_lossy(),
            "logDir": log_dir.to_string_lossy(),
        }))?,
    ))
}

fn get_artloom_compat_autostart(
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> Result<(u16, String)> {
    let store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock ArtLoom compat settings"))?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "is_autostart_enabled",
            "enabled": store.settings.general.auto_start,
            "sideEffect": false,
            "mode": "compat-preview",
        }))?,
    ))
}

fn set_artloom_compat_autostart(
    body: &str,
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> Result<(u16, String)> {
    let request: ArtLoomCompatToggleRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    set_artloom_compat_autostart_preference("set_autostart", request.enabled, settings_store)
}

fn set_artloom_compat_autostart_preference(
    compat_command: &str,
    enabled: bool,
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> Result<(u16, String)> {
    let mut store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock ArtLoom compat settings"))?;
    store.settings.general.auto_start = enabled;
    store.save()?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": compat_command,
            "enabled": enabled,
            "sideEffect": false,
            "mode": "compat-preview",
            "message": "Loom saved the requested autostart preference but did not mutate Windows startup entries from the compatibility endpoint.",
        }))?,
    ))
}

fn set_artloom_compat_minimize_to_tray(
    body: &str,
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> Result<(u16, String)> {
    let request: ArtLoomCompatToggleRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut store = settings_store
        .lock()
        .map_err(|_| anyhow::anyhow!("lock ArtLoom compat settings"))?;
    store.settings.general.minimize_to_tray = request.enabled;
    store.save()?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "set_minimize_to_tray",
            "enabled": request.enabled,
            "sideEffect": false,
            "mode": "compat-preview",
        }))?,
    ))
}

fn list_artloom_compat_workflows(workflow_store: &WorkflowStore) -> Result<(u16, String)> {
    let workflows = match workflow_store.list_workflows() {
        Ok(workflows) => workflows,
        Err(error) => return workflow_store_error_response(error),
    };
    let workflows: Vec<Value> = workflows
        .iter()
        .map(artloom_compat_workflow_metadata_json)
        .collect();
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "list_workflows",
            "workflows": workflows,
        }))?,
    ))
}

fn save_artloom_compat_workflow_metadata(
    workflow_id: &str,
    body: &str,
    workflow_store: &WorkflowStore,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request: Value = match serde_json::from_str(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let Some(object) = request.as_object() else {
        return invalid_request("save_workflow_metadata body must be a JSON object");
    };
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(workflow_id);

    if workflow_store.load_workflow(workflow_id).is_err() {
        let placeholder = format!(
            "name: {}\nnodes: []\n",
            serde_json::to_string(name).unwrap_or_else(|_| format!("{name:?}"))
        );
        if let Err(error) = workflow_store.save_workflow(workflow_id, &placeholder) {
            return workflow_store_error_response(error);
        }
    }

    let node_count = workflow_store
        .list_workflows()
        .ok()
        .and_then(|workflows| {
            workflows
                .into_iter()
                .find(|workflow| workflow.id == workflow_id)
        })
        .map(|workflow| workflow.node_count)
        .or_else(|| {
            object
                .get("node_count")
                .and_then(Value::as_u64)
                .map(|count| count as usize)
        })
        .or_else(|| {
            object
                .get("nodeCount")
                .and_then(Value::as_u64)
                .map(|count| count as usize)
        })
        .unwrap_or(0);
    let updated_at = object
        .get("updated_at")
        .or_else(|| object.get("updatedAt"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(artloom_workflow_timestamp);
    let created_at = object
        .get("created_at")
        .or_else(|| object.get("createdAt"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&updated_at)
        .to_owned();
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("draft");
    let tags = object.get("tags").cloned().unwrap_or_else(|| json!([]));
    let workflow = artloom_compat_workflow_metadata_value(
        workflow_id,
        name,
        description,
        &created_at,
        &updated_at,
        status,
        node_count,
        tags,
    );

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "save_workflow_metadata",
            "workflow": workflow,
        }))?,
    ))
}

fn save_artloom_compat_workflow_data(
    workflow_id: &str,
    body: &str,
    workflow_store: &WorkflowStore,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PutWorkflowRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let workflow = match workflow_store.save_workflow(workflow_id, &request.data) {
        Ok(workflow) => workflow,
        Err(error) => return workflow_store_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "save_workflow_data",
            "workflowId": workflow_id,
            "saved": true,
            "workflow": artloom_compat_workflow_metadata_json(&workflow),
        }))?,
    ))
}

fn load_artloom_compat_workflow_data(
    workflow_id: &str,
    workflow_store: &WorkflowStore,
) -> Result<(u16, String)> {
    let data = match workflow_store.load_workflow(workflow_id) {
        Ok(data) => data,
        Err(error) => return workflow_store_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "load_workflow_data",
            "workflowId": workflow_id,
            "data": data,
        }))?,
    ))
}

fn delete_artloom_compat_workflow_data(
    workflow_id: &str,
    workflow_store: &WorkflowStore,
) -> Result<(u16, String)> {
    if let Err(error) = workflow_store.load_workflow(workflow_id) {
        return workflow_store_error_response(error);
    }
    if let Err(error) = workflow_store.delete_workflow(workflow_id) {
        return workflow_store_error_response(error);
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "delete_workflow_data",
            "workflowId": workflow_id,
            "deleted": true,
        }))?,
    ))
}

fn artloom_compat_workflow_metadata_json(
    workflow: &loom_workflow_store::WorkflowMetadata,
) -> Value {
    let updated_at = if workflow.updated_at.trim().is_empty() {
        artloom_workflow_timestamp()
    } else {
        workflow.updated_at.clone()
    };
    artloom_compat_workflow_metadata_value(
        &workflow.id,
        &workflow.name,
        "",
        &updated_at,
        &updated_at,
        "draft",
        workflow.node_count,
        json!([]),
    )
}

fn artloom_compat_workflow_metadata_value(
    id: &str,
    name: &str,
    description: &str,
    created_at: &str,
    updated_at: &str,
    status: &str,
    node_count: usize,
    tags: Value,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "description": description,
        "created_at": created_at,
        "createdAt": created_at,
        "updated_at": updated_at,
        "updatedAt": updated_at,
        "status": status,
        "node_count": node_count,
        "nodeCount": node_count,
        "last_run_at": Value::Null,
        "lastRunAt": Value::Null,
        "tags": tags,
    })
}

fn artloom_workflow_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

fn list_workflows(workflow_store: &WorkflowStore) -> Result<(u16, String)> {
    let workflows = match workflow_store.list_workflows() {
        Ok(workflows) => workflows,
        Err(error) => return workflow_store_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({ "workflows": workflows }))?,
    ))
}

fn get_workflow(path_id: &str, workflow_store: &WorkflowStore) -> Result<(u16, String)> {
    let data = match workflow_store.load_workflow(path_id) {
        Ok(data) => data,
        Err(error) => return workflow_store_error_response(error),
    };
    let metadata = workflow_store.list_workflows().ok().and_then(|workflows| {
        workflows
            .into_iter()
            .find(|workflow| workflow.id == path_id)
    });
    let workflow = match metadata {
        Some(metadata) => {
            let mut value = serde_json::to_value(metadata)?;
            if let Some(object) = value.as_object_mut() {
                object.insert("data".to_owned(), json!(data));
            }
            value
        }
        None => json!({
            "id": path_id,
            "name": path_id,
            "nodeCount": 0,
            "updatedAt": "",
            "data": data,
        }),
    };

    Ok((
        200,
        serde_json::to_string(&json!({ "workflow": workflow }))?,
    ))
}

fn put_workflow(
    path_id: &str,
    body: &str,
    workflow_store: &WorkflowStore,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PutWorkflowRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let workflow = match workflow_store.save_workflow(path_id, &request.data) {
        Ok(workflow) => workflow,
        Err(error) => return workflow_store_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({ "workflow": workflow }))?,
    ))
}

fn canvas_workflow_dir(root: &Path, id: &str) -> Option<PathBuf> {
    // The id becomes a directory name, so reject anything that could escape the
    // canvas-workflow root or is not a plain slug.
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return None;
    }
    // Reject `..` traversal, hidden (leading-dot) names, and trailing-dot names
    // (Windows silently strips a trailing dot, aliasing `foo.` to `foo`).
    if trimmed.contains("..") || trimmed.starts_with('.') || trimmed.ends_with('.') {
        return None;
    }
    Some(root.join(trimmed))
}

fn canvas_workflow_preview_ext(source: &hook_canvas::HookCanvasPreviewSource) -> &'static str {
    // Sniff the extension from the source bytes so the saved file keeps a usable
    // type; default to png which the content-type sniffer also accepts.
    match source {
        hook_canvas::HookCanvasPreviewSource::File(path) => {
            match path.extension().and_then(|ext| ext.to_str()) {
                Some(ext)
                    if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") =>
                {
                    "jpg"
                }
                Some(ext) if ext.eq_ignore_ascii_case("webp") => "webp",
                _ => "png",
            }
        }
        hook_canvas::HookCanvasPreviewSource::DataUrl(_) => "png",
    }
}

fn save_hook_canvas_workflow(
    path_id: &str,
    body: &str,
    workflow_store: &WorkflowStore,
    canvas_workflow_root: &Path,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<SaveHookCanvasWorkflowRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    if request.selected_node_id.trim().is_empty() {
        return invalid_request("selectedNodeId is required");
    }

    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, path_id) else {
        return invalid_request("invalid workflow id");
    };

    let document = match load_active_hook_canvas_document() {
        Ok(document) => document,
        Err(error) => {
            eprintln!("loom Hook canvas workflow export failed: {error:#}");
            return structured_error(
                500,
                json!({
                    "code": "hook_canvas_error",
                    "message": "Hook canvas workflow export is temporarily unavailable",
                }),
            );
        }
    };

    let workflow_name = request
        .workflow_name
        .clone()
        .unwrap_or_else(|| path_id.to_owned());

    // Topology YAML (kept for the existing workflow store / studio).
    let data = match document
        .export_workflow_yaml_for_selected_node(request.selected_node_id.trim(), &workflow_name)
    {
        Ok(data) => data,
        Err(hook_canvas::HookCanvasWorkflowExportError::NodeNotFound(node_id)) => {
            return structured_error(
                404,
                json!({
                    "code": "hook_canvas_node_not_found",
                    "message": format!("Hook canvas node `{node_id}` was not found"),
                }),
            );
        }
    };

    // Frozen full snapshot (geometry + crop) scoped to the selected component.
    let component =
        match document.component_snapshot_for_selected_node(request.selected_node_id.trim()) {
            Ok(component) => component,
            Err(hook_canvas::HookCanvasWorkflowExportError::NodeNotFound(node_id)) => {
                return structured_error(
                    404,
                    json!({
                        "code": "hook_canvas_node_not_found",
                        "message": format!("Hook canvas node `{node_id}` was not found"),
                    }),
                );
            }
        };

    // Persist image copies for each member node, then rewrite preview URLs to the
    // saved-workflow preview route so the frozen snapshot renders without the
    // live Hook session.
    let images_dir = workflow_dir.join("images");
    if let Err(error) = fs::create_dir_all(&images_dir) {
        eprintln!("loom canvas workflow image dir failed: {error:#}");
        return structured_error(
            500,
            json!({
                "code": "canvas_workflow_write_failed",
                "message": "Unable to persist canvas workflow images",
            }),
        );
    }

    let mut saved_previews: HashMap<String, String> = HashMap::new();
    for (node_id, source) in &component.previews {
        let bytes = match source {
            hook_canvas::HookCanvasPreviewSource::DataUrl(data_url) => {
                match loom_image_io::decode_data_url_bytes(data_url) {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                }
            }
            hook_canvas::HookCanvasPreviewSource::File(path) => match fs::read(path) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            },
        };
        if bytes.len() as u64 > hook_canvas::MAX_PREVIEW_BYTES {
            continue;
        }
        let ext = canvas_workflow_preview_ext(source);
        let file_name = format!("{}.{ext}", sanitize_preview_file_stem(node_id));
        if fs::write(images_dir.join(&file_name), &bytes).is_ok() {
            saved_previews.insert(node_id.clone(), file_name);
        }
    }

    // Rewrite the frozen snapshot's preview URLs to the saved-workflow route.
    let mut snapshot = component.snapshot;
    for node in &mut snapshot.nodes {
        if saved_previews.contains_key(&node.id) {
            node.preview_available = true;
            node.preview_url = Some(format!(
                "/v1/hook-bridge/canvas/workflows/{}/nodes/{}/preview",
                percent_encode_path_segment(path_id),
                percent_encode_path_segment(&node.id),
            ));
        } else {
            node.preview_available = false;
            node.preview_url = None;
        }
    }

    let node_count = snapshot.nodes.len();
    let snapshot_json = match serde_json::to_string(&snapshot) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("loom canvas workflow snapshot serialize failed: {error:#}");
            return structured_error(
                500,
                json!({
                    "code": "canvas_workflow_write_failed",
                    "message": "Unable to serialize canvas workflow snapshot",
                }),
            );
        }
    };
    let saved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let meta = json!({
        "id": path_id,
        "name": workflow_name,
        "nodeCount": node_count,
        "savedAt": saved_at,
    });
    if fs::write(workflow_dir.join("snapshot.json"), &snapshot_json).is_err()
        || fs::write(workflow_dir.join("meta.json"), meta.to_string()).is_err()
    {
        return structured_error(
            500,
            json!({
                "code": "canvas_workflow_write_failed",
                "message": "Unable to persist canvas workflow snapshot",
            }),
        );
    }

    // Keep the topology in the existing workflow store too (studio compatibility).
    let workflow = match workflow_store.save_workflow(path_id, &data) {
        Ok(workflow) => workflow,
        Err(error) => return workflow_store_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "workflow": workflow,
            "sourceNodeId": request.selected_node_id,
            "workflowName": workflow_name,
            "canvasWorkflow": meta,
        }))?,
    ))
}

fn sanitize_preview_file_stem(node_id: &str) -> String {
    let mut stem = String::with_capacity(node_id.len());
    for ch in node_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            stem.push(ch);
        } else {
            stem.push('_');
        }
    }
    if stem.is_empty() {
        "node".to_owned()
    } else {
        stem
    }
}

fn percent_encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn list_canvas_workflows(canvas_workflow_root: &Path) -> Result<(u16, String)> {
    let mut workflows: Vec<Value> = Vec::new();
    if let Ok(entries) = fs::read_dir(canvas_workflow_root) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let meta_path = entry.path().join("meta.json");
            if let Ok(text) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<Value>(&text) {
                    workflows.push(meta);
                }
            }
        }
    }
    workflows.sort_by(|a, b| {
        // savedAt is written as epoch-millis u64; sort newest first.
        let sa = a.get("savedAt").and_then(Value::as_u64).unwrap_or(0);
        let sb = b.get("savedAt").and_then(Value::as_u64).unwrap_or(0);
        sb.cmp(&sa)
    });
    Ok((
        200,
        serde_json::to_string(&json!({ "workflows": workflows }))?,
    ))
}

fn get_canvas_workflow_snapshot(
    path_id: &str,
    canvas_workflow_root: &Path,
) -> Result<(u16, String)> {
    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, path_id) else {
        return invalid_request("invalid workflow id");
    };
    match fs::read_to_string(workflow_dir.join("snapshot.json")) {
        Ok(json) => Ok((200, json)),
        Err(_) => structured_error(
            404,
            json!({
                "code": "canvas_workflow_not_found",
                "message": "Canvas workflow was not found",
            }),
        ),
    }
}

fn canvas_workflow_preview_response(
    workflow_id: &str,
    node_id: &str,
    canvas_workflow_root: &Path,
) -> Result<RouteResponse> {
    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, workflow_id) else {
        return structured_error(
            400,
            json!({ "code": "invalid_request", "message": "invalid workflow id" }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    };
    let images_dir = workflow_dir.join("images");
    let Ok(canonical_images) = fs::canonicalize(&images_dir) else {
        return structured_error(
            404,
            json!({ "code": "preview_not_found", "message": "Hook canvas preview was not found" }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    };
    let stem = sanitize_preview_file_stem(node_id);
    // Try known extensions for this node stem.
    for ext in ["png", "jpg", "jpeg", "webp"] {
        let candidate = images_dir.join(format!("{stem}.{ext}"));
        let Ok(canonical) = fs::canonicalize(&candidate) else {
            continue;
        };
        if !canonical.starts_with(&canonical_images) || !canonical.is_file() {
            continue;
        }
        if let Ok(bytes) = fs::read(&canonical) {
            return hook_canvas_preview_binary_response(bytes);
        }
    }
    structured_error(
        404,
        json!({ "code": "preview_not_found", "message": "Hook canvas preview was not found" }),
    )
    .map(|(status, body)| RouteResponse::Text { status, body })
}

fn delete_canvas_workflow(path_id: &str, canvas_workflow_root: &Path) -> Result<(u16, String)> {
    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, path_id) else {
        return invalid_request("invalid workflow id");
    };
    if !workflow_dir.is_dir() {
        return structured_error(
            404,
            json!({
                "code": "canvas_workflow_not_found",
                "message": "Canvas workflow was not found",
            }),
        );
    }
    if fs::remove_dir_all(&workflow_dir).is_err() {
        return structured_error(
            500,
            json!({
                "code": "canvas_workflow_delete_failed",
                "message": "Unable to delete canvas workflow",
            }),
        );
    }
    Ok((
        200,
        serde_json::to_string(&json!({ "workflowId": path_id, "deleted": true }))?,
    ))
}

// Rename a frozen canvas workflow by updating the `name` field in its meta.json.
// The id (directory name) is stable; only the display name changes.
fn rename_canvas_workflow(
    path_id: &str,
    body: &str,
    canvas_workflow_root: &Path,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<RenameCanvasWorkflowRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let name = request.name.trim();
    if name.is_empty() {
        return invalid_request("name is required");
    }
    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, path_id) else {
        return invalid_request("invalid workflow id");
    };
    let meta_path = workflow_dir.join("meta.json");
    let Ok(text) = fs::read_to_string(&meta_path) else {
        return structured_error(
            404,
            json!({
                "code": "canvas_workflow_not_found",
                "message": "Canvas workflow was not found",
            }),
        );
    };
    let mut meta = match serde_json::from_str::<Value>(&text) {
        Ok(meta) => meta,
        Err(_) => json!({ "id": path_id }),
    };
    if let Some(object) = meta.as_object_mut() {
        object.insert("name".to_owned(), json!(name));
    }
    if fs::write(&meta_path, meta.to_string()).is_err() {
        return structured_error(
            500,
            json!({
                "code": "canvas_workflow_rename_failed",
                "message": "Unable to rename canvas workflow",
            }),
        );
    }
    Ok((200, serde_json::to_string(&meta)?))
}

fn delete_workflow(path_id: &str, workflow_store: &WorkflowStore) -> Result<(u16, String)> {
    if let Err(error) = workflow_store.load_workflow(path_id) {
        return workflow_store_error_response(error);
    }
    if let Err(error) = workflow_store.delete_workflow(path_id) {
        return workflow_store_error_response(error);
    }

    Ok((
        200,
        serde_json::to_string(&json!({ "workflowId": path_id, "deleted": true }))?,
    ))
}

fn hook_bridge_status(hook_bridge: &SharedHookBridgeRuntime) -> Result<(u16, String)> {
    let runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    Ok((
        200,
        serde_json::to_string(&hook_bridge_status_json(&runtime))?,
    ))
}

fn artloom_compat_ipc_status(hook_bridge: &SharedHookBridgeRuntime) -> Result<(u16, String)> {
    let runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    let mut status = hook_bridge_status_json(&runtime);
    if let Some(object) = status.as_object_mut() {
        object.insert("compatCommand".to_owned(), json!("get_ipc_status"));
        object.insert(
            "ipcPort".to_owned(),
            json!(runtime.port.unwrap_or(HOOK_BRIDGE_PORT)),
        );
    }
    Ok((200, serde_json::to_string(&status)?))
}

fn artloom_compat_instantiate_workflow(
    body: &str,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request: ArtLoomCompatInstantiateWorkflowRequest = match serde_json::from_str(request_body)
    {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mode = if request.mode.trim().is_empty() {
        "reference".to_owned()
    } else {
        request.mode.trim().to_owned()
    };
    let broadcast = instantiate_workflow_broadcast(
        request.nodes.clone(),
        request.edges.clone(),
        mode,
        request.workflow_id.clone(),
    );
    let serialized = serde_json::to_string(&broadcast)?;
    let (hub, subscribed_clients) = {
        let runtime = hook_bridge
            .lock()
            .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
        (
            runtime.broadcast_hub.clone(),
            runtime.broadcast_hub.subscriber_count(),
        )
    };
    if subscribed_clients == 0 {
        return structured_error(
            409,
            json!({
                "compatCommand": "instantiate_workflow",
                "code": "no_art_hook_client",
                "message": "No ArtHook desktop client is connected to receive workflow instantiation",
            }),
        );
    }
    broadcast_hook_bridge_messages(&hub, &[serialized]);

    Ok((
        200,
        serde_json::to_string(&json!({
            "compatCommand": "instantiate_workflow",
            "type": "success",
            "method": "art_hook/instantiate",
            "broadcasted": true,
            "subscribedClients": subscribed_clients,
            "params": broadcast.get("params").cloned().unwrap_or_else(|| json!({})),
        }))?,
    ))
}

fn artloom_compat_update_workflow_node(
    body: &str,
    hook_bridge: &SharedHookBridgeRuntime,
    tool_registry: &ToolRegistry,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request: ArtLoomCompatUpdateWorkflowNodeRequest = match serde_json::from_str(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let workflow_id = request.workflow_id.trim();
    let node_id = request.node_id.trim();
    let param = request.param.trim();
    if workflow_id.is_empty() {
        return invalid_request("update_workflow_node requires workflow_id");
    }
    if node_id.is_empty() {
        return invalid_request("update_workflow_node requires node_id");
    }
    if param.is_empty() {
        return invalid_request("update_workflow_node requires param");
    }

    let (workflow_root, broadcast_hub) = {
        let runtime = hook_bridge
            .lock()
            .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
        (runtime.workflow_root.clone(), runtime.broadcast_hub.clone())
    };
    let tools = tool_registry
        .list_tools()
        .unwrap_or_default()
        .into_iter()
        .filter(is_artloom_compat_visible_tool)
        .map(|tool| artloom_compat_art_json(&tool))
        .collect::<Vec<_>>();
    let input = HookBridgeRuntimeInput::new(tools, workflow_root);
    let result = handle_hook_bridge_request(
        HookBridgeRequest::UpdateWorkflowNode {
            workflow_id: workflow_id.to_owned(),
            node_id: node_id.to_owned(),
            param: param.to_owned(),
            value: request.value.clone(),
        },
        input,
    )
    .map_err(|error| anyhow::anyhow!("update Hook workflow node: {error}"))?;

    let success = result.response.get("type").and_then(Value::as_str) == Some("success");
    if success && is_hook_live_workflow_id(workflow_id) {
        let mut patch = HookCanvasPersistPatch::default();
        patch
            .param_updates
            .push((param.to_owned(), request.value.clone()));
        let _ = persist_hook_canvas_live_node_patch(node_id, &patch);
    }

    let broadcasts = result
        .broadcasts
        .iter()
        .filter_map(|broadcast| serde_json::to_string(broadcast).ok())
        .collect::<Vec<_>>();
    broadcast_hook_bridge_messages(&broadcast_hub, &broadcasts);

    let mut response = result.response;
    if let Some(object) = response.as_object_mut() {
        object.insert(
            "compatCommand".to_owned(),
            Value::String("update_workflow_node".to_owned()),
        );
        object.insert(
            "workflowId".to_owned(),
            Value::String(workflow_id.to_owned()),
        );
        object.insert("nodeId".to_owned(), Value::String(node_id.to_owned()));
        object.insert("param".to_owned(), Value::String(param.to_owned()));
        object.insert("value".to_owned(), request.value);
    }
    Ok((200, serde_json::to_string(&response)?))
}

fn artloom_compat_execute_art_node(
    body: &str,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request: ArtLoomCompatExecuteArtNodeRequest = match serde_json::from_str(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let node_id = request.node_id.trim();
    let art_id = request.art_id.trim();
    if node_id.is_empty() {
        return invalid_request("execute_art_node requires node_id");
    }
    if art_id.is_empty() {
        return invalid_request("execute_art_node requires art_id");
    }
    let result = execute_hook_bridge_art_node(
        node_id,
        art_id,
        request.input_base64,
        request.params,
        mcp_servers,
        tool_registry,
        workflow_store,
    );
    let mut response = serde_json::from_str::<Value>(&result.response)
        .unwrap_or_else(|_| json!({ "type": "error", "data": { "message": result.response } }));
    if let Some(object) = response.as_object_mut() {
        object.insert(
            "compatCommand".to_owned(),
            Value::String("execute_art_node".to_owned()),
        );
    }
    Ok((200, serde_json::to_string(&response)?))
}

fn hook_bridge_session(hook_bridge: &SharedHookBridgeRuntime) -> Result<(u16, String)> {
    let runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    let (session_path, available, session, error) = read_arthook_session_snapshot();
    Ok((
        200,
        serde_json::to_string(&json!({
            "method": "read_arthook_session",
            "compatCommand": "read_arthook_session",
            "running": runtime.worker.is_some(),
            "port": runtime.port.unwrap_or(HOOK_BRIDGE_PORT),
            "connectedClients": runtime.connected_clients.load(Ordering::SeqCst),
            "subscribedClients": runtime.broadcast_hub.subscriber_count(),
            "protocol": "artloom-compat",
            "sessionPath": session_path.to_string_lossy(),
            "available": available,
            "error": error,
            "session": session,
        }))?,
    ))
}

fn hook_canvas_snapshot() -> Result<(u16, String)> {
    match load_active_hook_canvas_document() {
        Ok(document) => Ok((200, serde_json::to_string(&document.snapshot)?)),
        Err(error) => {
            eprintln!("loom Hook canvas snapshot failed: {error:#}");
            structured_error(
                500,
                json!({
                    "code": "hook_canvas_error",
                    "message": "Hook canvas snapshot is temporarily unavailable",
                }),
            )
        }
    }
}

fn hook_canvas_preview_node_id(method: &str, path: &str) -> Option<String> {
    if method != "GET" {
        return None;
    }
    let path = path.split('?').next().unwrap_or(path);
    let encoded_id = path_id_with_suffix(path, "/v1/hook-bridge/canvas/nodes/", "/preview")?;
    Some(percent_decode(encoded_id))
}

// Parse `/v1/hook-bridge/canvas/workflows/{workflowId}/nodes/{nodeId}/preview`
// into (workflowId, nodeId), both percent-decoded. Returns None for other paths.
fn canvas_workflow_preview_ids(method: &str, path: &str) -> Option<(String, String)> {
    if method != "GET" {
        return None;
    }
    let path = path.split('?').next().unwrap_or(path);
    let rest = path.strip_prefix("/v1/hook-bridge/canvas/workflows/")?;
    let rest = rest.strip_suffix("/preview")?;
    let (encoded_workflow, encoded_node) = rest.split_once("/nodes/")?;
    if encoded_workflow.is_empty() || encoded_node.is_empty() {
        return None;
    }
    Some((
        percent_decode(encoded_workflow),
        percent_decode(encoded_node),
    ))
}

fn hook_canvas_preview_response(node_id: &str) -> Result<RouteResponse> {
    let document = match load_active_hook_canvas_document() {
        Ok(document) => document,
        Err(error) => {
            eprintln!("loom Hook canvas preview snapshot failed: {error:#}");
            return structured_error(
                500,
                json!({
                    "code": "hook_canvas_error",
                    "message": "Hook canvas preview is temporarily unavailable",
                }),
            )
            .map(|(status, body)| RouteResponse::Text { status, body });
        }
    };
    let Some(source) = document.preview_source(node_id) else {
        return structured_error(
            404,
            json!({
                "code": "preview_not_found",
                "message": "Hook canvas preview was not found",
            }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    };
    match source {
        hook_canvas::HookCanvasPreviewSource::DataUrl(data_url) => {
            let body = match loom_image_io::decode_data_url_bytes(data_url) {
                Ok(body) => body,
                Err(_) => {
                    return structured_error(
                        415,
                        json!({
                            "code": "unsupported_preview_type",
                            "message": "Hook canvas preview is not a supported image",
                        }),
                    )
                    .map(|(status, body)| RouteResponse::Text { status, body });
                }
            };
            return hook_canvas_preview_binary_response(body);
        }
        hook_canvas::HookCanvasPreviewSource::File(path) => {
            let preview_roots = document.preview_roots();
            if preview_roots.is_empty() {
                return structured_error(
                    404,
                    json!({
                        "code": "preview_not_found",
                        "message": "Hook canvas preview was not found",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            }
            let Ok(canonical_path) = fs::canonicalize(path) else {
                return structured_error(
                    404,
                    json!({
                        "code": "preview_not_found",
                        "message": "Hook canvas preview was not found",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            };
            // The preview file must be a regular file inside one of the roots the
            // document already validated its node images against.
            let within_root = preview_roots.iter().any(|root| {
                fs::canonicalize(root)
                    .map(|canonical_root| canonical_path.starts_with(&canonical_root))
                    .unwrap_or(false)
            });
            if !canonical_path.is_file() || !within_root {
                return structured_error(
                    404,
                    json!({
                        "code": "preview_not_found",
                        "message": "Hook canvas preview was not found",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            }
            let metadata = match fs::metadata(&canonical_path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return structured_error(
                        404,
                        json!({
                            "code": "preview_not_found",
                            "message": "Hook canvas preview was not found",
                        }),
                    )
                    .map(|(status, body)| RouteResponse::Text { status, body });
                }
                Err(error) => return Err(error).context("read Hook canvas preview metadata"),
            };
            if metadata.len() > hook_canvas::MAX_PREVIEW_BYTES {
                return structured_error(
                    413,
                    json!({
                        "code": "preview_too_large",
                        "message": "Hook canvas preview exceeds the size limit",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            }
            let body = match fs::read(&canonical_path) {
                Ok(body) => body,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return structured_error(
                        404,
                        json!({
                            "code": "preview_not_found",
                            "message": "Hook canvas preview was not found",
                        }),
                    )
                    .map(|(status, body)| RouteResponse::Text { status, body });
                }
                Err(error) => return Err(error).context("read Hook canvas preview bytes"),
            };
            hook_canvas_preview_binary_response(body)
        }
    }
}

fn hook_canvas_preview_binary_response(body: Vec<u8>) -> Result<RouteResponse> {
    if body.len() as u64 > hook_canvas::MAX_PREVIEW_BYTES {
        return structured_error(
            413,
            json!({
                "code": "preview_too_large",
                "message": "Hook canvas preview exceeds the size limit",
            }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    }
    let Some(content_type) = hook_canvas_preview_content_type(&body) else {
        return structured_error(
            415,
            json!({
                "code": "unsupported_preview_type",
                "message": "Hook canvas preview is not a supported image",
            }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    };
    Ok(RouteResponse::Binary {
        status: 200,
        content_type,
        body,
    })
}

fn hook_canvas_preview_content_type(body: &[u8]) -> Option<&'static str> {
    if body.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if body.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if body.len() >= 12 && &body[..4] == b"RIFF" && &body[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

fn read_arthook_session_snapshot() -> (PathBuf, bool, Value, Option<String>) {
    let session_path = arthook_session_path();
    if !session_path.exists() {
        return (
            session_path,
            false,
            json!({ "stickers": [], "links": [] }),
            Some("Session file not found".to_owned()),
        );
    }
    match fs::read_to_string(&session_path) {
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Ok(session) => (session_path, true, session, None),
            Err(error) => (
                session_path,
                false,
                json!({ "stickers": [], "links": [] }),
                Some(format!("Invalid ArtHook session JSON: {error}")),
            ),
        },
        Err(error) => (
            session_path,
            false,
            json!({ "stickers": [], "links": [] }),
            Some(format!("Unable to read ArtHook session: {error}")),
        ),
    }
}

// Hook has shipped under several Tauri identifiers over time. Its own runtime
// resolves the active app-data directory by falling back from the current
// identifier to a legacy directory that still holds user state, so the file the
// daemon must read is not always `com.vmjcv.arthook-next`. Loom mirrors that by
// scanning every known identifier and selecting the session file Hook is
// actually writing (the most recently modified one).
const HOOK_SESSION_IDENTIFIERS: &[&str] = &[
    "com.yamiyu.hook",
    "com.vmjcv.hook",
    "io.github.aiaimimi0920.hook",
    "com.vmjcv.arthook-next",
    "com.vmjcv.arthook",
];
const HOOK_LIVE_WORKFLOW_ID: &str = "hook-live";
const LEGACY_HOOK_LIVE_WORKFLOW_ID: &str = "arthook-live";

#[derive(Clone, Debug)]
struct HookLiveWorkflowSnapshot {
    source_path: PathBuf,
    bytes: Vec<u8>,
    root: Value,
    updated_at: Option<String>,
}

#[derive(Clone, Debug)]
struct HookCanvasRuntimeNodeState {
    status: String,
    error_message: Option<String>,
    preview_data_url: Option<String>,
    preview_cache_token: Option<String>,
    result_candidates: Vec<hook_canvas::HookCanvasResultCandidate>,
    selected_result_index: Option<usize>,
}

#[derive(Default)]
struct HookCanvasPersistPatch {
    param_updates: Vec<(String, Value)>,
    image_search_metadata: Option<Option<Value>>,
    preview_data_url: Option<Option<String>>,
}

static HOOK_LIVE_WORKFLOW_SNAPSHOTS: OnceLock<Mutex<HashMap<String, HookLiveWorkflowSnapshot>>> =
    OnceLock::new();
static HOOK_CANVAS_RUNTIME_STATUSES: OnceLock<Mutex<HashMap<String, HookCanvasRuntimeNodeState>>> =
    OnceLock::new();

fn hook_live_workflow_snapshots() -> &'static Mutex<HashMap<String, HookLiveWorkflowSnapshot>> {
    HOOK_LIVE_WORKFLOW_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hook_canvas_runtime_statuses() -> &'static Mutex<HashMap<String, HookCanvasRuntimeNodeState>> {
    HOOK_CANVAS_RUNTIME_STATUSES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_hook_live_workflow_id(workflow_id: &str) -> bool {
    workflow_id == HOOK_LIVE_WORKFLOW_ID || workflow_id == LEGACY_HOOK_LIVE_WORKFLOW_ID
}

fn clear_hook_canvas_runtime_state() {
    if let Ok(mut snapshots) = hook_live_workflow_snapshots().lock() {
        snapshots.clear();
    }
    if let Ok(mut statuses) = hook_canvas_runtime_statuses().lock() {
        statuses.clear();
    }
}

fn store_hook_live_workflow_snapshot(source_path: &Path, workflow_id: &str, snapshot: &Value) {
    let mut root = snapshot.clone();
    let Some(object) = root.as_object_mut() else {
        return;
    };
    if !object.contains_key("workflowId") && !object.contains_key("workflow_id") {
        object.insert(
            "workflowId".to_owned(),
            Value::String(workflow_id.to_owned()),
        );
    }
    let Ok(bytes) = serde_json::to_vec(&root) else {
        return;
    };
    let Ok(mut snapshots) = hook_live_workflow_snapshots().lock() else {
        return;
    };
    snapshots.insert(
        workflow_id.to_owned(),
        HookLiveWorkflowSnapshot {
            source_path: source_path.to_path_buf(),
            bytes,
            root,
            updated_at: Some(artloom_workflow_timestamp()),
        },
    );
}

fn hook_canvas_root_nodes_mut(root: &mut Value) -> Option<&mut Vec<Value>> {
    let key = {
        let object = root.as_object()?;
        if object
            .get("stickers")
            .and_then(Value::as_array)
            .is_some_and(|nodes| !nodes.is_empty())
        {
            "stickers"
        } else if object
            .get("nodes")
            .and_then(Value::as_array)
            .is_some_and(|nodes| !nodes.is_empty())
        {
            "nodes"
        } else if object.get("units").and_then(Value::as_array).is_some() {
            "units"
        } else if object.get("nodes").and_then(Value::as_array).is_some() {
            "nodes"
        } else if object.get("stickers").and_then(Value::as_array).is_some() {
            "stickers"
        } else {
            return None;
        }
    };
    root.get_mut(key).and_then(Value::as_array_mut)
}

fn hook_canvas_node_mut<'a>(root: &'a mut Value, node_id: &str) -> Option<&'a mut Value> {
    hook_canvas_root_nodes_mut(root)?
        .iter_mut()
        .find(|node| node.get("id").and_then(Value::as_str) == Some(node_id))
}

fn hook_canvas_node_field_owner_mut<'a>(
    node: &'a mut Value,
    field: &str,
) -> Option<&'a mut serde_json::Map<String, Value>> {
    let use_top_level = node.get(field).is_some() || node.get("data").is_none();
    if use_top_level {
        return node.as_object_mut();
    }
    let object = node.as_object_mut()?;
    let data = object.entry("data").or_insert_with(|| json!({}));
    if !data.is_object() {
        *data = json!({});
    }
    data.as_object_mut()
}

fn hook_canvas_set_node_param(node: &mut Value, param: &str, value: Value) -> bool {
    let Some(owner) = hook_canvas_node_field_owner_mut(node, "params") else {
        return false;
    };
    let params = owner.entry("params").or_insert_with(|| json!({}));
    if !params.is_object() {
        *params = json!({});
    }
    params
        .as_object_mut()
        .expect("params object")
        .insert(param.to_owned(), value);
    true
}

fn hook_canvas_set_image_search_metadata(node: &mut Value, metadata: Value) -> bool {
    let Some(owner) = hook_canvas_node_field_owner_mut(node, "loomMetadata") else {
        return false;
    };
    let loom_metadata = owner.entry("loomMetadata").or_insert_with(|| json!({}));
    if !loom_metadata.is_object() {
        *loom_metadata = json!({});
    }
    let generic_metadata = json!({
        "kind": "image.candidates",
        "items": metadata
            .get("candidates")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "selectedIndex": metadata.get("selectedIndex").cloned().unwrap_or(Value::Null),
    });
    let object = loom_metadata.as_object_mut().expect("loomMetadata object");
    object.insert("candidates".to_owned(), generic_metadata);
    object.insert("imageSearch".to_owned(), metadata);
    true
}

fn hook_canvas_set_preview_data_url(node: &mut Value, preview_data_url: String) -> bool {
    let Some(owner) = hook_canvas_node_field_owner_mut(node, "previewSrc") else {
        return false;
    };
    owner.insert("previewSrc".to_owned(), Value::String(preview_data_url));
    true
}

fn apply_hook_canvas_persist_patch(
    root: &mut Value,
    node_id: &str,
    patch: &HookCanvasPersistPatch,
) -> bool {
    let Some(node) = hook_canvas_node_mut(root, node_id) else {
        return false;
    };
    let mut changed = false;
    for (param, value) in &patch.param_updates {
        changed |= hook_canvas_set_node_param(node, param, value.clone());
    }
    if let Some(metadata) = &patch.image_search_metadata {
        match metadata {
            Some(metadata) => {
                changed |= hook_canvas_set_image_search_metadata(node, metadata.clone());
            }
            None => {
                changed |= hook_canvas_set_image_search_metadata(node, Value::Null);
            }
        }
    }
    if let Some(preview_data_url) = &patch.preview_data_url {
        match preview_data_url {
            Some(preview_data_url) => {
                changed |= hook_canvas_set_preview_data_url(node, preview_data_url.clone());
            }
            None => {
                changed |= hook_canvas_set_preview_data_url(node, String::new());
            }
        }
    }
    changed
}

fn write_hook_canvas_root(path: &Path, root: &Value) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(root).context("serialize Hook canvas root")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create Hook canvas dir `{}`", parent.display()))?;
    }
    fs::write(path, &bytes)
        .with_context(|| format!("write Hook canvas root `{}`", path.display()))?;
    Ok(bytes)
}

fn persist_hook_canvas_live_node_patch(node_id: &str, patch: &HookCanvasPersistPatch) -> bool {
    let mut updated = false;
    if let Ok(mut snapshots) = hook_live_workflow_snapshots().lock() {
        for workflow_id in [HOOK_LIVE_WORKFLOW_ID, LEGACY_HOOK_LIVE_WORKFLOW_ID] {
            let Some(snapshot) = snapshots.get_mut(workflow_id) else {
                continue;
            };
            if !apply_hook_canvas_persist_patch(&mut snapshot.root, node_id, patch) {
                continue;
            }
            match write_hook_canvas_root(&snapshot.source_path, &snapshot.root) {
                Ok(bytes) => {
                    snapshot.bytes = bytes;
                    snapshot.updated_at = Some(artloom_workflow_timestamp());
                    updated = true;
                }
                Err(error) => {
                    eprintln!(
                        "loom Hook canvas live-node persist failed for `{node_id}` at `{}`: {error:#}",
                        snapshot.source_path.display(),
                    );
                    updated = true;
                }
            }
        }
    }
    if updated {
        return true;
    }

    let session_path = arthook_session_path();
    let Ok(content) = fs::read_to_string(&session_path) else {
        return false;
    };
    let Ok(mut root) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    if !apply_hook_canvas_persist_patch(&mut root, node_id, patch) {
        return false;
    }
    match write_hook_canvas_root(&session_path, &root) {
        Ok(_) => true,
        Err(error) => {
            eprintln!(
                "loom Hook canvas session persist failed for `{node_id}` at `{}`: {error:#}",
                session_path.display(),
            );
            true
        }
    }
}

fn load_hook_live_workflow_document() -> Option<hook_canvas::HookCanvasDocument> {
    let snapshots = hook_live_workflow_snapshots().lock().ok()?;
    let snapshot = snapshots
        .get(HOOK_LIVE_WORKFLOW_ID)
        .or_else(|| snapshots.get(LEGACY_HOOK_LIVE_WORKFLOW_ID))?;
    Some(hook_canvas::HookCanvasDocument::from_serialized_root(
        &snapshot.source_path,
        snapshot.bytes.clone(),
        snapshot.root.clone(),
        snapshot.updated_at.clone(),
    ))
}

fn hook_canvas_overlay_revision(snapshot: &hook_canvas::HookCanvasSnapshot) -> Option<String> {
    let statuses = hook_canvas_runtime_statuses().lock().ok()?;
    let mut tokens = snapshot
        .nodes
        .iter()
        .filter_map(|node| {
            statuses.get(&node.id).map(|state| {
                format!(
                    "{}:{}:{:?}:{:?}:{:?}:{:?}",
                    node.id,
                    state.status,
                    state.error_message,
                    state.preview_cache_token,
                    state.selected_result_index,
                    state
                        .result_candidates
                        .iter()
                        .map(|candidate| (&candidate.index, &candidate.image_url))
                        .collect::<Vec<_>>()
                )
            })
        })
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }
    tokens.sort();
    let mut hasher = DefaultHasher::new();
    for token in tokens {
        token.hash(&mut hasher);
    }
    Some(format!("{:016x}", hasher.finish()))
}

fn apply_hook_canvas_runtime_overlays(document: &mut hook_canvas::HookCanvasDocument) {
    let Some(statuses) = hook_canvas_runtime_statuses().lock().ok() else {
        return;
    };
    for node in &mut document.snapshot.nodes {
        let Some(state) = statuses.get(&node.id) else {
            continue;
        };
        node.status = state.status.clone();
        node.error_message = state.error_message.clone();
        node.result_candidates = state.result_candidates.clone();
        node.selected_result_index = state.selected_result_index;
    }
    let preview_overrides = statuses
        .iter()
        .filter_map(|(node_id, state)| {
            state.preview_data_url.as_ref().map(|data_url| {
                (
                    node_id.clone(),
                    data_url.clone(),
                    state.preview_cache_token.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    drop(statuses);
    for (node_id, data_url, cache_token) in preview_overrides {
        document.override_preview_source(
            &node_id,
            hook_canvas::HookCanvasPreviewSource::DataUrl(data_url),
            cache_token.as_deref(),
        );
    }
    if let Some(overlay_revision) = hook_canvas_overlay_revision(&document.snapshot) {
        document.snapshot.revision =
            format!("{}-rt-{overlay_revision}", document.snapshot.revision);
    }
}

fn load_active_hook_canvas_document() -> Result<hook_canvas::HookCanvasDocument> {
    let mut document = match load_hook_live_workflow_document() {
        Some(document) => document,
        None => hook_canvas::HookCanvasDocument::read(&arthook_session_path())?,
    };
    apply_hook_canvas_runtime_overlays(&mut document);
    Ok(document)
}

fn set_hook_canvas_runtime_status(node_id: &str, status: &str, error_message: Option<String>) {
    if let Ok(mut statuses) = hook_canvas_runtime_statuses().lock() {
        let state = statuses
            .entry(node_id.to_owned())
            .or_insert(HookCanvasRuntimeNodeState {
                status: String::new(),
                error_message: None,
                preview_data_url: None,
                preview_cache_token: None,
                result_candidates: Vec::new(),
                selected_result_index: None,
            });
        state.status = status.to_owned();
        state.error_message = error_message;
    }
}

fn hook_canvas_preview_cache_token(data_url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    data_url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn set_hook_canvas_runtime_preview(node_id: &str, preview_data_url: Option<String>) {
    if let Ok(mut statuses) = hook_canvas_runtime_statuses().lock() {
        let state = statuses
            .entry(node_id.to_owned())
            .or_insert(HookCanvasRuntimeNodeState {
                status: String::new(),
                error_message: None,
                preview_data_url: None,
                preview_cache_token: None,
                result_candidates: Vec::new(),
                selected_result_index: None,
            });
        state.preview_cache_token = preview_data_url
            .as_deref()
            .map(hook_canvas_preview_cache_token);
        state.preview_data_url = preview_data_url;
    }
}

fn clear_hook_canvas_runtime_preview(node_id: &str) {
    set_hook_canvas_runtime_preview(node_id, None);
}

fn set_hook_canvas_runtime_result_candidates(
    node_id: &str,
    candidates: Vec<hook_canvas::HookCanvasResultCandidate>,
    selected_result_index: Option<usize>,
) {
    if let Ok(mut statuses) = hook_canvas_runtime_statuses().lock() {
        let state = statuses
            .entry(node_id.to_owned())
            .or_insert(HookCanvasRuntimeNodeState {
                status: String::new(),
                error_message: None,
                preview_data_url: None,
                preview_cache_token: None,
                result_candidates: Vec::new(),
                selected_result_index: None,
            });
        state.result_candidates = candidates;
        state.selected_result_index = selected_result_index;
    }
}

fn clear_hook_canvas_runtime_result_candidates(node_id: &str) {
    set_hook_canvas_runtime_result_candidates(node_id, Vec::new(), None);
}

fn extract_hook_canvas_result_candidates(
    execution_result: &Value,
) -> Option<(Vec<hook_canvas::HookCanvasResultCandidate>, Option<usize>)> {
    let loom_metadata = execution_result.get("loomMetadata")?;
    let metadata = loom_metadata
        .get("candidates")
        .or_else(|| loom_metadata.get("imageSearch"))?;
    let candidates = metadata
        .get("items")
        .or_else(|| metadata.get("candidates"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let image_url = item.get("imageUrl").and_then(Value::as_str)?.to_owned();
                    Some(hook_canvas::HookCanvasResultCandidate {
                        index: item.get("index").and_then(Value::as_u64).unwrap_or(0) as usize,
                        title: item.get("title").and_then(Value::as_str).map(str::to_owned),
                        image_url,
                        thumbnail: item
                            .get("thumbnail")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        preview: item
                            .get("preview")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        thumbnail_url: item
                            .get("thumbnailUrl")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        source_page_url: item
                            .get("sourcePageUrl")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        width: item.get("width").and_then(Value::as_u64),
                        height: item.get("height").and_then(Value::as_u64),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let selected_result_index = metadata
        .get("selectedIndex")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    Some((candidates, selected_result_index))
}

fn hook_canvas_image_search_metadata_value(
    candidates: &[hook_canvas::HookCanvasResultCandidate],
    selected_result_index: Option<usize>,
) -> Value {
    json!({
        "candidates": candidates
            .iter()
            .map(|candidate| {
                json!({
                    "index": candidate.index,
                    "title": candidate.title,
                    "imageUrl": candidate.image_url,
                    "thumbnail": candidate.thumbnail,
                    "preview": candidate.preview,
                    "thumbnailUrl": candidate.thumbnail_url,
                    "sourcePageUrl": candidate.source_page_url,
                    "width": candidate.width,
                    "height": candidate.height,
                })
            })
            .collect::<Vec<_>>(),
        "selectedIndex": selected_result_index,
    })
}

fn attach_ahrp_image_search_metadata(response: &mut Value, metadata: Value) {
    if response.get("data").is_none() {
        if let Some(object) = response.as_object_mut() {
            object.insert("data".to_owned(), json!({}));
        }
    }
    let Some(data) = response.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    let generic_metadata = json!({
        "kind": "image.candidates",
        "items": metadata
            .get("candidates")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "selectedIndex": metadata.get("selectedIndex").cloned().unwrap_or(Value::Null),
    });
    data.insert(
        "loomMetadata".to_owned(),
        json!({
            "candidates": generic_metadata,
            "imageSearch": metadata,
        }),
    );
}

fn persist_hook_canvas_image_search_state(
    node_id: &str,
    candidates: &[hook_canvas::HookCanvasResultCandidate],
    selected_result_index: Option<usize>,
    preview_data_url: Option<&str>,
) {
    let mut patch = HookCanvasPersistPatch {
        image_search_metadata: Some(Some(hook_canvas_image_search_metadata_value(
            candidates,
            selected_result_index,
        ))),
        ..HookCanvasPersistPatch::default()
    };
    if let Some(selected_result_index) = selected_result_index {
        patch
            .param_updates
            .push(("result_index".to_owned(), json!(selected_result_index)));
    }
    if let Some(preview_data_url) = preview_data_url {
        patch.preview_data_url = Some(Some(preview_data_url.to_owned()));
    }
    let _ = persist_hook_canvas_live_node_patch(node_id, &patch);
}

fn finalize_execute_art_node_runtime_status(node_id: &str, response: &Value) {
    match response.get("type").and_then(Value::as_str) {
        Some("success") => set_hook_canvas_runtime_status(node_id, "ready", None),
        _ => set_hook_canvas_runtime_status(
            node_id,
            "error",
            response
                .get("data")
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        ),
    }
}

fn finalize_ahrp_runtime_status(node_id: Option<&str>, response: &Value) {
    let Some(node_id) = node_id else {
        return;
    };
    match response.get("status").and_then(Value::as_str) {
        Some("Success") => set_hook_canvas_runtime_status(node_id, "ready", None),
        _ => set_hook_canvas_runtime_status(
            node_id,
            "error",
            response
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ),
    }
}

fn params_match_score(node_params: &Value, params: &BTreeMap<String, Value>) -> usize {
    let Some(object) = node_params.as_object() else {
        return 0;
    };
    params
        .iter()
        .filter(|(key, value)| {
            object
                .get(*key)
                .is_some_and(|candidate| candidate == *value)
        })
        .count()
}

fn resolve_hook_runtime_node_id_from_document(
    document: &hook_canvas::HookCanvasDocument,
    art_id: &str,
    params: &BTreeMap<String, Value>,
) -> Option<String> {
    let candidates = document
        .snapshot
        .nodes
        .iter()
        .filter(|node| node.art_id.as_deref() == Some(art_id))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0].id.clone());
    }

    let mut scored = candidates
        .iter()
        .map(|node| ((*node).id.clone(), params_match_score(&node.params, params)))
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let best = scored.first()?;
    let second_score = scored.get(1).map(|entry| entry.1).unwrap_or(0);
    if best.1 == 0 || best.1 == second_score {
        return None;
    }
    Some(best.0.clone())
}

fn resolve_hook_runtime_node_id(art_id: &str, params: &BTreeMap<String, Value>) -> Option<String> {
    if let Some(document) = load_hook_live_workflow_document() {
        if let Some(node_id) = resolve_hook_runtime_node_id_from_document(&document, art_id, params)
        {
            return Some(node_id);
        }
    }
    let document = hook_canvas::HookCanvasDocument::read(&arthook_session_path()).ok()?;
    resolve_hook_runtime_node_id_from_document(&document, art_id, params)
}

fn arthook_session_path() -> PathBuf {
    // An explicit full-path override wins so isolated smokes and advanced setups
    // can point Loom at a specific session file.
    if let Some(path) = std::env::var_os("LOOM_HOOK_SESSION_PATH") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    let appdata = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    // An explicit identifier-directory override still resolves `session.json`
    // beneath it.
    if let Some(dir) = std::env::var_os("LOOM_HOOK_APPDATA_DIR") {
        let dir = PathBuf::from(dir);
        if !dir.as_os_str().is_empty() {
            return dir.join("session.json");
        }
    }
    resolve_hook_session_path(&appdata)
}

fn resolve_hook_session_path(appdata: &Path) -> PathBuf {
    let default_path = appdata.join("com.vmjcv.arthook-next").join("session.json");
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for identifier in HOOK_SESSION_IDENTIFIERS {
        let candidate = appdata.join(identifier).join("session.json");
        let Ok(metadata) = fs::metadata(&candidate) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        let is_newer = match &best {
            Some((best_modified, _)) => modified > *best_modified,
            None => true,
        };
        if is_newer {
            best = Some((modified, candidate));
        }
    }
    best.map(|(_, path)| path).unwrap_or(default_path)
}

fn start_hook_bridge(
    body: &str,
    hook_bridge: &SharedHookBridgeRuntime,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    artloom_settings: &SharedArtLoomCompatSettingsStore,
    shared_images: &SharedImageStoreHandle,
    ocr_provider: &OcrProviderHandle,
) -> Result<(u16, String)> {
    let request_body = if body.trim().is_empty() { "{}" } else { body };
    let request = match serde_json::from_str::<StartHookBridgeRequest>(request_body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let requested_port = request.port.unwrap_or(HOOK_BRIDGE_PORT);
    let mut runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    if runtime.worker.is_some() {
        return structured_error(
            409,
            json!({
                "code": "hook_bridge_running",
                "message": "Hook bridge is already running",
            }),
        );
    }
    clear_hook_canvas_runtime_state();

    let listener = match TcpListener::bind(("127.0.0.1", requested_port)) {
        Ok(listener) => listener,
        Err(error) => {
            return structured_error(
                409,
                json!({
                    "code": "hook_bridge_bind_failed",
                    "message": error.to_string(),
                    "port": requested_port,
                }),
            );
        }
    };
    let assigned_port = listener
        .local_addr()
        .context("read hook bridge local address")?
        .port();
    listener
        .set_nonblocking(true)
        .context("set hook bridge listener nonblocking")?;
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let connected_clients = Arc::clone(&runtime.connected_clients);
    connected_clients.store(0, Ordering::SeqCst);
    runtime.broadcast_hub.clear();
    let broadcast_hub = runtime.broadcast_hub.clone();
    let worker_mcp_servers = Arc::clone(mcp_servers);
    let worker_tool_registry = tool_registry.clone();
    let worker_workflow_store = workflow_store.clone();
    let worker_artloom_settings = Arc::clone(artloom_settings);
    let worker_shared_images = Arc::clone(shared_images);
    let worker_ocr_provider = Arc::clone(ocr_provider);
    let workflow_root = runtime.workflow_root.clone();
    let worker = thread::spawn(move || {
        run_hook_bridge_websocket_server(
            listener,
            shutdown_rx,
            connected_clients,
            broadcast_hub,
            worker_mcp_servers,
            worker_tool_registry,
            worker_workflow_store,
            worker_artloom_settings,
            worker_shared_images,
            worker_ocr_provider,
            workflow_root,
        );
    });
    runtime.shutdown_tx = Some(shutdown_tx);
    runtime.worker = Some(worker);
    runtime.port = Some(assigned_port);

    Ok((
        200,
        serde_json::to_string(&hook_bridge_status_json(&runtime))?,
    ))
}

fn stop_hook_bridge(hook_bridge: &SharedHookBridgeRuntime) -> Result<(u16, String)> {
    let mut runtime = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("lock hook bridge runtime"))?;
    if let Some(shutdown_tx) = runtime.shutdown_tx.take() {
        let _ = shutdown_tx.send(());
    }
    if let Some(worker) = runtime.worker.take() {
        let _ = worker.join();
    }
    runtime.connected_clients.store(0, Ordering::SeqCst);
    runtime.broadcast_hub.clear();
    runtime.port = None;
    clear_hook_canvas_runtime_state();

    Ok((
        200,
        serde_json::to_string(&hook_bridge_status_json(&runtime))?,
    ))
}

fn hook_bridge_status_json(runtime: &HookBridgeRuntime) -> Value {
    let running = runtime.worker.is_some();
    json!({
        "running": running,
        "port": runtime.port.unwrap_or(HOOK_BRIDGE_PORT),
        "connectedClients": runtime.connected_clients.load(Ordering::SeqCst),
        "subscribedClients": runtime.broadcast_hub.subscriber_count(),
        "protocol": "artloom-compat",
        "sessionMethod": "read_arthook_session",
        "methods": legacy_method_names(),
    })
}

fn run_hook_bridge_websocket_server(
    listener: TcpListener,
    shutdown_rx: Receiver<()>,
    connected_clients: Arc<AtomicUsize>,
    broadcast_hub: HookBridgeBroadcastHub,
    mcp_servers: SharedMcpServerStore,
    tool_registry: ToolRegistry,
    workflow_store: WorkflowStore,
    artloom_settings: SharedArtLoomCompatSettingsStore,
    shared_images: SharedImageStoreHandle,
    ocr_provider: OcrProviderHandle,
    workflow_root: PathBuf,
) {
    loop {
        if shutdown_rx.try_recv().is_ok() {
            return;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let connected_clients = Arc::clone(&connected_clients);
                let broadcast_hub = broadcast_hub.clone();
                let mcp_servers = Arc::clone(&mcp_servers);
                let tool_registry = tool_registry.clone();
                let workflow_store = workflow_store.clone();
                let artloom_settings = Arc::clone(&artloom_settings);
                let shared_images = Arc::clone(&shared_images);
                let ocr_provider = Arc::clone(&ocr_provider);
                let workflow_root = workflow_root.clone();
                thread::spawn(move || {
                    handle_hook_bridge_websocket_connection(
                        stream,
                        connected_clients,
                        broadcast_hub,
                        mcp_servers,
                        tool_registry,
                        workflow_store,
                        artloom_settings,
                        shared_images,
                        ocr_provider,
                        workflow_root,
                    );
                });
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return,
        }
    }
}

fn handle_hook_bridge_websocket_connection(
    stream: std::net::TcpStream,
    connected_clients: Arc<AtomicUsize>,
    broadcast_hub: HookBridgeBroadcastHub,
    mcp_servers: SharedMcpServerStore,
    tool_registry: ToolRegistry,
    workflow_store: WorkflowStore,
    artloom_settings: SharedArtLoomCompatSettingsStore,
    shared_images: SharedImageStoreHandle,
    ocr_provider: OcrProviderHandle,
    workflow_root: PathBuf,
) {
    let _ = stream.set_nonblocking(false);
    let Ok(mut websocket) = tungstenite::accept(stream) else {
        return;
    };
    let _ = websocket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)));
    connected_clients.fetch_add(1, Ordering::SeqCst);
    let _guard = ConnectedClientGuard { connected_clients };
    let mut subscription_rx: Option<Receiver<String>> = None;
    let mut _subscription_guard: Option<HookBridgeSubscriptionGuard> = None;

    loop {
        if let Some(rx) = &subscription_rx {
            if !drain_hook_bridge_broadcasts(&mut websocket, rx) {
                break;
            }
        }

        let message = match websocket.read() {
            Ok(message) => message,
            Err(error) if hook_bridge_read_timed_out(&error) => continue,
            Err(_) => break,
        };
        match message {
            tungstenite::Message::Text(text) => {
                let result = handle_hook_bridge_websocket_text(
                    &text,
                    &mcp_servers,
                    &tool_registry,
                    &workflow_store,
                    &artloom_settings,
                    &shared_images,
                    &ocr_provider,
                    &workflow_root,
                );
                if result.subscription_channels.is_some() && subscription_rx.is_none() {
                    let (rx, guard) = register_hook_bridge_subscription(
                        &broadcast_hub,
                        result.subscription_channels.clone().unwrap_or_default(),
                    );
                    subscription_rx = Some(rx);
                    _subscription_guard = Some(guard);
                }
                if websocket
                    .send(tungstenite::Message::Text(result.response))
                    .is_err()
                {
                    break;
                }
                broadcast_hook_bridge_messages(&broadcast_hub, &result.broadcasts);
            }
            tungstenite::Message::Ping(data) => {
                let _ = websocket.send(tungstenite::Message::Pong(data));
            }
            tungstenite::Message::Close(close) => {
                let _ = websocket.close(close);
                break;
            }
            _ => {}
        }
    }
}

struct HookBridgeSubscriptionGuard {
    id: usize,
    subscribers: Arc<Mutex<Vec<HookBridgeSubscriber>>>,
}

impl Drop for HookBridgeSubscriptionGuard {
    fn drop(&mut self) {
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.retain(|subscriber| subscriber.id != self.id);
        }
    }
}

struct ConnectedClientGuard {
    connected_clients: Arc<AtomicUsize>,
}

impl Drop for ConnectedClientGuard {
    fn drop(&mut self) {
        self.connected_clients.fetch_sub(1, Ordering::SeqCst);
    }
}

struct HookBridgeWebSocketTextResult {
    response: String,
    broadcasts: Vec<String>,
    subscription_channels: Option<Vec<String>>,
}

impl HookBridgeWebSocketTextResult {
    fn response(response: String) -> Self {
        Self {
            response,
            broadcasts: Vec::new(),
            subscription_channels: None,
        }
    }
}

#[derive(Clone, Copy)]
enum AhrpOutputPreference {
    Base64,
    SharedMemory,
}

fn handle_hook_bridge_websocket_text(
    text: &str,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    artloom_settings: &SharedArtLoomCompatSettingsStore,
    shared_images: &SharedImageStoreHandle,
    ocr_provider: &OcrProviderHandle,
    workflow_root: &Path,
) -> HookBridgeWebSocketTextResult {
    let request = match parse_request(text) {
        Ok(request) => request,
        Err(error) => {
            return HookBridgeWebSocketTextResult::response(hook_bridge_error_json(format!(
                "Invalid request: {error}"
            )))
        }
    };
    let subscription_channels = match &request {
        HookBridgeRequest::Subscribe { channels } => Some(channels.clone()),
        _ => None,
    };
    let workflow_node_param_persist = match &request {
        HookBridgeRequest::UpdateWorkflowNode {
            workflow_id,
            node_id,
            param,
            value,
        } if is_hook_live_workflow_id(workflow_id) => {
            Some((node_id.clone(), param.clone(), value.clone()))
        }
        _ => None,
    };
    if let HookBridgeRequest::SyncUserArts { arts }
    | HookBridgeRequest::SyncUserArtsNamespaced { arts } = request
    {
        return execute_hook_bridge_sync_user_arts(arts, tool_registry);
    }
    if let HookBridgeRequest::UpdateArtParam {
        art_id,
        param_id,
        value,
    } = request
    {
        return execute_hook_bridge_update_art_param(&art_id, &param_id, value, tool_registry);
    }
    if let HookBridgeRequest::GetSettings = request {
        return execute_hook_bridge_get_settings(artloom_settings);
    }
    if let HookBridgeRequest::GetShortcuts = request {
        return execute_hook_bridge_get_shortcuts(artloom_settings);
    }
    if let HookBridgeRequest::SyncShortcuts = request {
        return execute_hook_bridge_sync_shortcuts(artloom_settings);
    }
    if let HookBridgeRequest::TranslateText { text, target_lang } = request {
        return execute_hook_bridge_translate_text(&text, &target_lang);
    }
    if let HookBridgeRequest::OcrImage { image_base64 } = request {
        return execute_hook_bridge_ocr_image(&image_base64, ocr_provider);
    }
    if let HookBridgeRequest::ExecuteArtNode {
        node_id,
        art_id,
        input_base64,
        params,
    } = request
    {
        return execute_hook_bridge_art_node(
            &node_id,
            &art_id,
            input_base64,
            params,
            mcp_servers,
            tool_registry,
            workflow_store,
        );
    }
    if let HookBridgeRequest::Process {
        request_id,
        art_id,
        input,
        params,
        input_images,
        disabled_params,
    } = request
    {
        return execute_hook_bridge_ahrp_process(
            &request_id,
            &art_id,
            input,
            params,
            input_images,
            disabled_params,
            mcp_servers,
            tool_registry,
            workflow_store,
            shared_images,
        );
    }
    if let HookBridgeRequest::OverwriteWorkflow {
        workflow_id,
        snapshot,
    } = &request
    {
        if is_hook_live_workflow_id(workflow_id) {
            store_hook_live_workflow_snapshot(&arthook_session_path(), workflow_id, snapshot);
        }
    }
    let tools = tool_registry
        .list_tools()
        .unwrap_or_default()
        .into_iter()
        .filter(is_artloom_compat_visible_tool)
        .map(|tool| artloom_compat_art_json(&tool))
        .collect::<Vec<_>>();
    let ocr_available = ocr_provider
        .lock()
        .map(|provider| provider.is_available())
        .unwrap_or(false);
    let input = HookBridgeRuntimeInput::new(tools, workflow_root.to_path_buf())
        .with_ocr_available(ocr_available);
    let result = match handle_hook_bridge_request(request, input) {
        Ok(result) => result,
        Err(error) => {
            return HookBridgeWebSocketTextResult::response(hook_bridge_error_json(
                error.to_string(),
            ))
        }
    };
    if result.response.get("type").and_then(Value::as_str) == Some("success") {
        if let Some((node_id, param, value)) = workflow_node_param_persist {
            let mut patch = HookCanvasPersistPatch::default();
            patch.param_updates.push((param, value));
            let _ = persist_hook_canvas_live_node_patch(&node_id, &patch);
        }
    }
    let response = serde_json::to_string(&result.response)
        .unwrap_or_else(|_| hook_bridge_error_json("Serialization failed"));
    let broadcasts = result
        .broadcasts
        .into_iter()
        .filter_map(|broadcast| serde_json::to_string(&broadcast).ok())
        .collect();
    HookBridgeWebSocketTextResult {
        response,
        broadcasts,
        subscription_channels,
    }
}

fn execute_hook_bridge_sync_user_arts(
    arts: Vec<Value>,
    tool_registry: &ToolRegistry,
) -> HookBridgeWebSocketTextResult {
    let request = json!({ "arts": arts });
    match sync_artloom_compat_arts_value(&request, tool_registry) {
        Ok(data) => HookBridgeWebSocketTextResult {
            response: json!({
                "type": "success",
                "data": data,
            })
            .to_string(),
            broadcasts: vec![arts_updated_broadcast().to_string()],
            subscription_channels: None,
        },
        Err(response) => {
            HookBridgeWebSocketTextResult::response(hook_bridge_control_error_json(response))
        }
    }
}

fn execute_hook_bridge_update_art_param(
    art_id: &str,
    param_id: &str,
    value: Value,
    tool_registry: &ToolRegistry,
) -> HookBridgeWebSocketTextResult {
    let mut tool = match get_artloom_tool(art_id, tool_registry) {
        Ok(tool) => tool,
        Err(response) => {
            return HookBridgeWebSocketTextResult::response(hook_bridge_control_error_json(
                response,
            ))
        }
    };

    apply_artloom_defaults_update(&mut tool, &json!({ param_id: value.clone() }));
    let saved = match tool_registry.save_tool(tool) {
        Ok(saved) => saved,
        Err(error) => {
            return HookBridgeWebSocketTextResult::response(hook_bridge_control_error_json(
                tool_registry_error_response(error),
            ))
        }
    };

    HookBridgeWebSocketTextResult {
        response: json!({
            "type": "success",
            "data": {
                "compatCommand": "update_art_param",
                "art_id": art_id,
                "param_id": param_id,
                "value": value,
                "art": artloom_compat_art_json(&saved),
                "tool": saved,
            }
        })
        .to_string(),
        broadcasts: vec![arts_updated_broadcast().to_string()],
        subscription_channels: None,
    }
}

fn execute_hook_bridge_get_settings(
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> HookBridgeWebSocketTextResult {
    let store = match settings_store.lock() {
        Ok(store) => store,
        Err(_) => {
            return HookBridgeWebSocketTextResult::response(hook_bridge_error_json(
                "lock ArtLoom compat settings",
            ))
        }
    };

    HookBridgeWebSocketTextResult::response(
        json!({
            "type": "settings",
            "data": store.settings,
        })
        .to_string(),
    )
}

fn execute_hook_bridge_get_shortcuts(
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> HookBridgeWebSocketTextResult {
    match artloom_compat_shortcuts_value(settings_store) {
        Ok(shortcuts) => HookBridgeWebSocketTextResult::response(
            json!({
                "type": "shortcuts",
                "data": shortcuts,
            })
            .to_string(),
        ),
        Err(error) => HookBridgeWebSocketTextResult::response(hook_bridge_error_json(error)),
    }
}

fn execute_hook_bridge_sync_shortcuts(
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> HookBridgeWebSocketTextResult {
    match artloom_compat_shortcuts_value(settings_store) {
        Ok(shortcuts) => HookBridgeWebSocketTextResult::response(
            json!({
                "type": "shortcuts",
                "data": shortcuts,
            })
            .to_string(),
        ),
        Err(error) => HookBridgeWebSocketTextResult::response(hook_bridge_error_json(error)),
    }
}

fn execute_hook_bridge_translate_text(
    text: &str,
    target_lang: &str,
) -> HookBridgeWebSocketTextResult {
    match translate_text_via_provider(text, target_lang) {
        Ok(Some(translated_text)) => HookBridgeWebSocketTextResult::response(
            json!({
                "type": "success",
                "data": {
                    "translated_text": translated_text,
                    "target_lang": target_lang,
                    "source": "loom-translate-provider"
                }
            })
            .to_string(),
        ),
        Ok(None) => HookBridgeWebSocketTextResult::response(
            json!({
                "type": "success",
                "data": {
                    "translated_text": text,
                    "target_lang": target_lang,
                    "source": "loom-hook-bridge-compat"
                }
            })
            .to_string(),
        ),
        Err(error) => HookBridgeWebSocketTextResult::response(hook_bridge_error_json(error)),
    }
}

fn translate_text_via_provider(
    text: &str,
    target_lang: &str,
) -> std::result::Result<Option<String>, String> {
    let endpoint = match std::env::var("LOOM_TRANSLATE_ENDPOINT") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return Ok(None),
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| format!("build translate provider client: {error}"))?;
    let response = client
        .post(endpoint)
        .json(&json!({
            "text": text,
            "target_lang": target_lang,
            "source_lang": "auto"
        }))
        .send()
        .map_err(|error| format!("translate provider request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("read translate provider response: {error}"))?;
    if !status.is_success() {
        return Err(format!("translate provider returned {status}: {body}"));
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| format!("translate provider returned invalid JSON: {error}"))?;
    value
        .get("translated_text")
        .or_else(|| value.get("data"))
        .or_else(|| value.get("translation"))
        .and_then(Value::as_str)
        .map(|translated| Some(translated.to_owned()))
        .ok_or_else(|| "translate provider response missing translated text".to_owned())
}

fn artloom_compat_shortcuts_value(
    settings_store: &SharedArtLoomCompatSettingsStore,
) -> std::result::Result<Vec<ArtLoomShortcutConfig>, String> {
    let store = settings_store
        .lock()
        .map_err(|_| "lock ArtLoom compat settings".to_owned())?;
    let mut shortcuts = store
        .settings
        .shortcuts
        .values()
        .cloned()
        .collect::<Vec<_>>();
    shortcuts.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(shortcuts)
}

fn hook_bridge_control_error_json(response: Result<(u16, String)>) -> String {
    let message = match response {
        Ok((_status, body)) => serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or(body),
        Err(error) => error.to_string(),
    };
    hook_bridge_error_json(message)
}

fn execute_hook_bridge_ocr_image(
    image_base64: &str,
    ocr_provider: &OcrProviderHandle,
) -> HookBridgeWebSocketTextResult {
    let mut provider = match ocr_provider.lock() {
        Ok(provider) => provider,
        Err(_) => {
            return HookBridgeWebSocketTextResult::response(
                ocr_image_error_response("OCR enhancement unavailable").to_string(),
            )
        }
    };

    match &mut *provider {
        OcrProvider::Unavailable => HookBridgeWebSocketTextResult::response(
            ocr_image_error_response("OCR enhancement unavailable").to_string(),
        ),
        OcrProvider::Fixture { text } => {
            let rgba = match loom_image_io::decode_image_base64_to_rgba8(image_base64) {
                Ok(rgba) => rgba,
                Err(error) => {
                    return HookBridgeWebSocketTextResult::response(
                        ocr_image_error_response(error.to_string()).to_string(),
                    )
                }
            };

            HookBridgeWebSocketTextResult::response(
                ocr_image_success_response(text, rgba.width, rgba.height).to_string(),
            )
        }
        OcrProvider::Real { engine } => {
            let image_bytes = match loom_image_io::decode_data_url_bytes(image_base64) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return HookBridgeWebSocketTextResult::response(
                        ocr_image_error_response(error.to_string()).to_string(),
                    )
                }
            };
            let result = match engine.detect_image_bytes(&image_bytes, false) {
                Ok(result) => result,
                Err(error) => {
                    return HookBridgeWebSocketTextResult::response(
                        ocr_image_error_response(error.to_string()).to_string(),
                    )
                }
            };
            let response = serde_json::json!({
                "type": "success",
                "data": result,
            });

            HookBridgeWebSocketTextResult::response(response.to_string())
        }
    }
}

fn execute_hook_bridge_art_node(
    node_id: &str,
    art_id: &str,
    input_base64: Option<String>,
    params: std::collections::BTreeMap<String, Value>,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
) -> HookBridgeWebSocketTextResult {
    let started = Instant::now();
    set_hook_canvas_runtime_status(node_id, "processing", None);
    let tool = match tool_registry.get_tool(art_id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            if loom_native_image::is_native_art_id(art_id) {
                return execute_hook_bridge_native_art_node(node_id, art_id, input_base64, params);
            }
            let message = format!("Art definition not found: {art_id}");
            set_hook_canvas_runtime_status(node_id, "error", Some(message.clone()));
            return HookBridgeWebSocketTextResult::response(
                execute_art_node_error_response(message).to_string(),
            );
        }
        Err(error) => {
            set_hook_canvas_runtime_status(node_id, "error", Some(error.to_string()));
            return HookBridgeWebSocketTextResult::response(
                execute_art_node_error_response(error.to_string()).to_string(),
            );
        }
    };

    let servers = match mcp_servers.lock() {
        Ok(servers) => servers.values().cloned().collect::<Vec<_>>(),
        Err(_) => {
            set_hook_canvas_runtime_status(
                node_id,
                "error",
                Some("lock MCP server store".to_owned()),
            );
            return HookBridgeWebSocketTextResult::response(
                execute_art_node_error_response("lock MCP server store").to_string(),
            );
        }
    };

    let mut arguments = serde_json::Map::new();
    for (key, value) in params {
        arguments.insert(key, value);
    }
    let mut temporary_input_files = Vec::new();
    if let Some(input) = input_base64.filter(|value| !value.is_empty()) {
        arguments
            .entry("input_base64".to_owned())
            .or_insert_with(|| Value::String(input.clone()));
        if matches!(&tool.execution, ToolExecution::CloudApi { .. }) {
            if let Some(input_path) = write_hook_bridge_cloud_input_file(&input) {
                let temporary_input_file = input_path.clone();
                let input_path = input_path.to_string_lossy().into_owned();
                arguments
                    .entry("input".to_owned())
                    .or_insert_with(|| Value::String(input_path.clone()));
                arguments
                    .entry("image".to_owned())
                    .or_insert_with(|| Value::String(input_path));
                temporary_input_files.push(temporary_input_file);
            } else {
                arguments
                    .entry("input".to_owned())
                    .or_insert_with(|| Value::String(input));
            }
        } else {
            arguments
                .entry("input".to_owned())
                .or_insert_with(|| Value::String(input));
        }
    }

    let response = match execute_tool_with_workflows(
        &tool,
        &servers,
        workflow_store,
        tool_registry,
        Value::Object(arguments),
    ) {
        Ok(result) => {
            let image_search_state = extract_hook_canvas_result_candidates(&result);
            if let Some((candidates, selected_result_index)) = image_search_state.clone() {
                set_hook_canvas_runtime_result_candidates(
                    node_id,
                    candidates,
                    selected_result_index,
                );
            } else {
                clear_hook_canvas_runtime_result_candidates(node_id);
            }
            match extract_ahrp_base64_output(&result) {
                Some(output_base64) => {
                    if let Some((candidates, selected_result_index)) = image_search_state.as_ref() {
                        persist_hook_canvas_image_search_state(
                            node_id,
                            candidates,
                            *selected_result_index,
                            Some(output_base64.as_str()),
                        );
                    }
                    set_hook_canvas_runtime_preview(node_id, Some(output_base64.clone()));
                    execute_art_node_image_success_response(
                        node_id,
                        &output_base64,
                        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    )
                }
                None => {
                    clear_hook_canvas_runtime_preview(node_id);
                    execute_art_node_success_response(
                        node_id,
                        result,
                        started.elapsed().as_millis(),
                    )
                }
            }
        }
        Err(error) => {
            clear_hook_canvas_runtime_preview(node_id);
            clear_hook_canvas_runtime_result_candidates(node_id);
            execute_art_node_error_response(error.to_string())
        }
    };
    for path in temporary_input_files {
        let _ = fs::remove_file(path);
    }
    finalize_execute_art_node_runtime_status(node_id, &response);
    HookBridgeWebSocketTextResult::response(response.to_string())
}

fn write_hook_bridge_cloud_input_file(input_base64: &str) -> Option<PathBuf> {
    let bytes = loom_image_io::decode_data_url_bytes(input_base64).ok()?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "loom-cloud-input-{}-{timestamp}.png",
        std::process::id()
    ));
    fs::write(&path, bytes).ok()?;
    Some(path)
}

fn execute_hook_bridge_native_art_node(
    node_id: &str,
    art_id: &str,
    input_base64: Option<String>,
    params: std::collections::BTreeMap<String, Value>,
) -> HookBridgeWebSocketTextResult {
    set_hook_canvas_runtime_status(node_id, "processing", None);
    let Some(input_base64) = input_base64.filter(|value| !value.is_empty()) else {
        set_hook_canvas_runtime_status(
            node_id,
            "error",
            Some("Native image filter input_base64 is required".to_owned()),
        );
        return HookBridgeWebSocketTextResult::response(
            execute_art_node_error_response("Native image filter input_base64 is required")
                .to_string(),
        );
    };
    let params = params.into_iter().collect::<HashMap<_, _>>();
    let result = loom_native_image::process_art(art_id, &input_base64, params);
    let response = if result.success {
        match result.output_base64 {
            Some(output_base64) => {
                set_hook_canvas_runtime_preview(node_id, Some(output_base64.clone()));
                execute_art_node_image_success_response(
                    node_id,
                    &output_base64,
                    result.processing_time_ms,
                )
            }
            None => {
                clear_hook_canvas_runtime_preview(node_id);
                execute_art_node_error_response("Native image filter produced no output")
            }
        }
    } else {
        clear_hook_canvas_runtime_preview(node_id);
        execute_art_node_error_response(
            result
                .error
                .unwrap_or_else(|| "Native image filter failed".to_owned()),
        )
    };
    finalize_execute_art_node_runtime_status(node_id, &response);
    HookBridgeWebSocketTextResult::response(response.to_string())
}

struct PreparedAhrpInput {
    input_base64: String,
    width: u64,
    height: u64,
    tool_input: Value,
    output_preference: AhrpOutputPreference,
}

fn prepare_hook_bridge_ahrp_input(
    request_id: &str,
    input: Value,
    shared_images: &SharedImageStoreHandle,
) -> Result<PreparedAhrpInput, Value> {
    let input_type = input.get("type").and_then(Value::as_str).unwrap_or("");
    match input_type {
        "base64" => prepare_hook_bridge_base64_ahrp_input(request_id, input),
        "shared_memory" => {
            prepare_hook_bridge_shared_memory_ahrp_input(request_id, input, shared_images)
        }
        _ => Err(ahrp_error_response(
            request_id,
            "BadRequest",
            format!("Unsupported AHRP input type: {input_type}"),
        )),
    }
}

fn prepare_hook_bridge_base64_ahrp_input(
    request_id: &str,
    input: Value,
) -> Result<PreparedAhrpInput, Value> {
    let Some(input_base64) = input
        .get("data")
        .and_then(Value::as_str)
        .filter(|data| !data.is_empty())
        .map(str::to_owned)
    else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP base64 input missing data",
        ));
    };
    let Some(width) = input.get("width").and_then(Value::as_u64) else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP base64 input missing width",
        ));
    };
    let Some(height) = input.get("height").and_then(Value::as_u64) else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP base64 input missing height",
        ));
    };

    Ok(PreparedAhrpInput {
        input_base64,
        width,
        height,
        tool_input: input,
        output_preference: AhrpOutputPreference::Base64,
    })
}

fn prepare_hook_bridge_shared_memory_ahrp_input(
    request_id: &str,
    input: Value,
    shared_images: &SharedImageStoreHandle,
) -> Result<PreparedAhrpInput, Value> {
    let Some(handle) = input
        .get("handle")
        .and_then(Value::as_str)
        .filter(|handle| !handle.is_empty())
    else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP shared_memory input missing handle",
        ));
    };
    let Some(size) = input
        .get("size")
        .and_then(Value::as_u64)
        .and_then(|size| usize::try_from(size).ok())
    else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP shared_memory input missing size",
        ));
    };
    let Some(width) = input.get("width").and_then(Value::as_u64) else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP shared_memory input missing width",
        ));
    };
    let Some(height) = input.get("height").and_then(Value::as_u64) else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP shared_memory input missing height",
        ));
    };
    let format = input
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("rgba8");
    if format != "rgba8" {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            format!("Unsupported AHRP shared_memory format: {format}"),
        ));
    }
    let Some(expected_size) = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
    else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP shared_memory dimensions overflow",
        ));
    };
    if size != expected_size {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            format!("AHRP shared_memory size mismatch: expected {expected_size}, got {size}"),
        ));
    }

    let bytes = match shared_images
        .lock()
        .map_err(|_| ahrp_error_response(request_id, "InternalError", "lock shared image store"))?
        .read_rgba8_or_open(handle, size)
    {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(ahrp_error_response(
                request_id,
                "BadRequest",
                error.to_string(),
            ))
        }
    };
    let Ok(width_u32) = u32::try_from(width) else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP shared_memory width exceeds u32",
        ));
    };
    let Ok(height_u32) = u32::try_from(height) else {
        return Err(ahrp_error_response(
            request_id,
            "BadRequest",
            "AHRP shared_memory height exceeds u32",
        ));
    };
    let input_base64 = match loom_shared_image::rgba8_to_png_data_url(width_u32, height_u32, bytes)
    {
        Ok(data_url) => data_url,
        Err(error) => {
            return Err(ahrp_error_response(
                request_id,
                "BadRequest",
                error.to_string(),
            ))
        }
    };
    let tool_input = json!({
        "type": "base64",
        "data": input_base64,
        "width": width,
        "height": height,
        "format": "rgba8",
    });

    Ok(PreparedAhrpInput {
        input_base64,
        width,
        height,
        tool_input,
        output_preference: AhrpOutputPreference::SharedMemory,
    })
}

fn ahrp_process_output_response(
    request_id: &str,
    output_base64: &str,
    width: u64,
    height: u64,
    processing_time_ms: u128,
    output_preference: AhrpOutputPreference,
    shared_images: &SharedImageStoreHandle,
) -> Value {
    match output_preference {
        AhrpOutputPreference::Base64 => ahrp_process_base64_success_response(
            request_id,
            output_base64,
            width,
            height,
            processing_time_ms,
        ),
        AhrpOutputPreference::SharedMemory => {
            let image = match shared_images
                .lock()
                .map_err(|_| "lock shared image store".to_owned())
                .and_then(|mut store| {
                    store
                        .create_from_data_url(output_base64)
                        .map_err(|error| error.to_string())
                }) {
                Ok(image) => image,
                Err(error) => return ahrp_error_response(request_id, "EngineError", error),
            };
            let format = match image.format {
                SharedImageFormat::Rgba8 => "rgba8",
            };
            ahrp_process_shared_memory_success_response(
                request_id,
                &image.handle,
                image.size,
                u64::from(image.width),
                u64::from(image.height),
                format,
                processing_time_ms,
            )
        }
    }
}

fn hook_bridge_argument_value_is_blank(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Object(object) => object.is_empty(),
        _ => false,
    }
}

fn execute_hook_bridge_ahrp_process(
    request_id: &str,
    art_id: &str,
    input: Value,
    params: std::collections::BTreeMap<String, Value>,
    input_images: std::collections::BTreeMap<String, Value>,
    disabled_params: Vec<String>,
    mcp_servers: &SharedMcpServerStore,
    tool_registry: &ToolRegistry,
    workflow_store: &WorkflowStore,
    shared_images: &SharedImageStoreHandle,
) -> HookBridgeWebSocketTextResult {
    let started = Instant::now();
    let matched_node_id = resolve_hook_runtime_node_id(art_id, &params);
    if let Some(node_id) = matched_node_id.as_deref() {
        set_hook_canvas_runtime_status(node_id, "processing", None);
    }
    let prepared_input = match prepare_hook_bridge_ahrp_input(request_id, input, shared_images) {
        Ok(input) => input,
        Err(response) => {
            finalize_ahrp_runtime_status(matched_node_id.as_deref(), &response);
            return HookBridgeWebSocketTextResult::response(response.to_string());
        }
    };

    let disabled = disabled_params
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let filtered_params = params
        .into_iter()
        .filter(|(key, _)| !disabled.contains(key))
        .collect::<std::collections::BTreeMap<_, _>>();

    let tool = match tool_registry.get_tool(art_id) {
        Ok(Some(tool)) => tool,
        Ok(None) => {
            if loom_native_image::is_native_art_id(art_id) {
                let result = execute_hook_bridge_native_ahrp_process(
                    request_id,
                    art_id,
                    &prepared_input.input_base64,
                    filtered_params,
                    prepared_input.width,
                    prepared_input.height,
                    prepared_input.output_preference,
                    matched_node_id.as_deref(),
                    shared_images,
                );
                if let Ok(response) = serde_json::from_str::<Value>(&result.response) {
                    finalize_ahrp_runtime_status(matched_node_id.as_deref(), &response);
                }
                return result;
            }
            let response = ahrp_error_response(
                request_id,
                "NotFound",
                format!("Art definition not found: {art_id}"),
            );
            finalize_ahrp_runtime_status(matched_node_id.as_deref(), &response);
            return HookBridgeWebSocketTextResult::response(response.to_string());
        }
        Err(error) => {
            let response = ahrp_error_response(request_id, "InternalError", error.to_string());
            finalize_ahrp_runtime_status(matched_node_id.as_deref(), &response);
            return HookBridgeWebSocketTextResult::response(response.to_string());
        }
    };

    let servers = match mcp_servers.lock() {
        Ok(servers) => servers.values().cloned().collect::<Vec<_>>(),
        Err(_) => {
            let response =
                ahrp_error_response(request_id, "InternalError", "lock MCP server store");
            finalize_ahrp_runtime_status(matched_node_id.as_deref(), &response);
            return HookBridgeWebSocketTextResult::response(response.to_string());
        }
    };

    let mut arguments = serde_json::Map::new();
    for (key, value) in filtered_params {
        arguments.insert(key, value);
    }
    for (key, value) in input_images {
        if key.trim().is_empty() || hook_bridge_argument_value_is_blank(&value) {
            continue;
        }
        arguments.insert(key, value);
    }
    arguments
        .entry("input_base64".to_owned())
        .or_insert_with(|| Value::String(prepared_input.input_base64.clone()));
    // For cloud APIs the input must be a real file path so multipart file fields
    // (e.g. `{{inputs.input.path}}`) upload the actual image. Without this the
    // template resolves to the raw AHRP input object and PhotoRoom-style APIs
    // reject the call with "missing_image". Mirrors the HTTP execute_art_node
    // path in `execute_hook_bridge_art_node`.
    let mut temporary_input_files = Vec::new();
    if matches!(&tool.execution, ToolExecution::CloudApi { .. }) {
        if let Some(input_path) = write_hook_bridge_cloud_input_file(&prepared_input.input_base64) {
            let temporary_input_file = input_path.clone();
            let input_path = input_path.to_string_lossy().into_owned();
            arguments
                .entry("input".to_owned())
                .or_insert_with(|| Value::String(input_path.clone()));
            arguments
                .entry("image".to_owned())
                .or_insert_with(|| Value::String(input_path));
            temporary_input_files.push(temporary_input_file);
        } else {
            arguments
                .entry("input".to_owned())
                .or_insert(prepared_input.tool_input);
        }
    } else {
        arguments
            .entry("input".to_owned())
            .or_insert(prepared_input.tool_input);
    }

    let response = match execute_tool_with_workflows(
        &tool,
        &servers,
        workflow_store,
        tool_registry,
        Value::Object(arguments),
    ) {
        Ok(result) => {
            let image_search_state = extract_hook_canvas_result_candidates(&result);
            if let Some(node_id) = matched_node_id.as_deref() {
                if let Some((candidates, selected_result_index)) = image_search_state.clone() {
                    set_hook_canvas_runtime_result_candidates(
                        node_id,
                        candidates,
                        selected_result_index,
                    );
                } else {
                    clear_hook_canvas_runtime_result_candidates(node_id);
                }
            }
            match extract_ahrp_base64_output(&result) {
                Some(output) => {
                    if let Some(node_id) = matched_node_id.as_deref() {
                        if let Some((candidates, selected_result_index)) =
                            image_search_state.as_ref()
                        {
                            persist_hook_canvas_image_search_state(
                                node_id,
                                candidates,
                                *selected_result_index,
                                Some(output.as_str()),
                            );
                        }
                        set_hook_canvas_runtime_preview(node_id, Some(output.clone()));
                    }
                    let mut response = ahrp_process_output_response(
                        request_id,
                        &output,
                        prepared_input.width,
                        prepared_input.height,
                        started.elapsed().as_millis(),
                        prepared_input.output_preference,
                        shared_images,
                    );
                    if let Some((candidates, selected_result_index)) = image_search_state.as_ref() {
                        attach_ahrp_image_search_metadata(
                            &mut response,
                            hook_canvas_image_search_metadata_value(
                                candidates,
                                *selected_result_index,
                            ),
                        );
                    }
                    response
                }
                None => {
                    if let Some(node_id) = matched_node_id.as_deref() {
                        clear_hook_canvas_runtime_preview(node_id);
                    }
                    let detail = extract_execution_text_content(&result)
                        .map(|text| text.trim().to_owned())
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| {
                            "MCP tool response contained no usable image data".to_owned()
                        });
                    let mut response = ahrp_error_response(request_id, "EngineError", detail);
                    if let Some((candidates, selected_result_index)) = image_search_state.as_ref() {
                        attach_ahrp_image_search_metadata(
                            &mut response,
                            hook_canvas_image_search_metadata_value(
                                candidates,
                                *selected_result_index,
                            ),
                        );
                    }
                    response
                }
            }
        }
        Err(error) => {
            if let Some(node_id) = matched_node_id.as_deref() {
                clear_hook_canvas_runtime_preview(node_id);
                clear_hook_canvas_runtime_result_candidates(node_id);
            }
            ahrp_error_response(request_id, "EngineError", error.to_string())
        }
    };
    for path in temporary_input_files {
        let _ = fs::remove_file(path);
    }
    finalize_ahrp_runtime_status(matched_node_id.as_deref(), &response);
    HookBridgeWebSocketTextResult::response(response.to_string())
}

fn execute_hook_bridge_native_ahrp_process(
    request_id: &str,
    art_id: &str,
    input_base64: &str,
    params: std::collections::BTreeMap<String, Value>,
    width: u64,
    height: u64,
    output_preference: AhrpOutputPreference,
    matched_node_id: Option<&str>,
    shared_images: &SharedImageStoreHandle,
) -> HookBridgeWebSocketTextResult {
    let result = loom_native_image::process_art(
        art_id,
        input_base64,
        params.into_iter().collect::<HashMap<_, _>>(),
    );
    let response = if result.success {
        match result.output_base64 {
            Some(output_base64) => {
                if let Some(node_id) = matched_node_id {
                    set_hook_canvas_runtime_preview(node_id, Some(output_base64.clone()));
                }
                ahrp_process_output_response(
                    request_id,
                    &output_base64,
                    width,
                    height,
                    result.processing_time_ms as u128,
                    output_preference,
                    shared_images,
                )
            }
            None => {
                if let Some(node_id) = matched_node_id {
                    clear_hook_canvas_runtime_preview(node_id);
                }
                ahrp_error_response(
                    request_id,
                    "EngineError",
                    "Native image filter produced no output",
                )
            }
        }
    } else {
        if let Some(node_id) = matched_node_id {
            clear_hook_canvas_runtime_preview(node_id);
        }
        ahrp_error_response(
            request_id,
            "EngineError",
            result
                .error
                .unwrap_or_else(|| "Native image filter failed".to_owned()),
        )
    };
    HookBridgeWebSocketTextResult::response(response.to_string())
}

fn register_hook_bridge_subscription(
    hub: &HookBridgeBroadcastHub,
    channels: Vec<String>,
) -> (Receiver<String>, HookBridgeSubscriptionGuard) {
    let (tx, rx) = mpsc::channel();
    let id = hub.next_subscriber_id.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut subscribers) = hub.subscribers.lock() {
        subscribers.push(HookBridgeSubscriber { id, tx, channels });
    }
    (
        rx,
        HookBridgeSubscriptionGuard {
            id,
            subscribers: Arc::clone(&hub.subscribers),
        },
    )
}

fn drain_hook_bridge_broadcasts(
    websocket: &mut tungstenite::WebSocket<std::net::TcpStream>,
    rx: &Receiver<String>,
) -> bool {
    loop {
        match rx.try_recv() {
            Ok(message) => {
                if websocket.send(tungstenite::Message::Text(message)).is_err() {
                    return false;
                }
            }
            Err(mpsc::TryRecvError::Empty) => return true,
            Err(mpsc::TryRecvError::Disconnected) => return false,
        }
    }
}

fn broadcast_hook_bridge_messages(hub: &HookBridgeBroadcastHub, broadcasts: &[String]) {
    if broadcasts.is_empty() {
        return;
    }
    let Ok(mut subscribers) = hub.subscribers.lock() else {
        return;
    };
    subscribers.retain(|subscriber| {
        broadcasts.iter().all(|broadcast| {
            if !subscriber_accepts_broadcast(subscriber, broadcast) {
                return true;
            }
            subscriber.tx.send(broadcast.clone()).is_ok()
        })
    });
}

fn broadcast_hook_bridge_json(hook_bridge: &SharedHookBridgeRuntime, broadcast: Value) {
    let serialized = match serde_json::to_string(&broadcast) {
        Ok(serialized) => serialized,
        Err(_) => return,
    };
    let hub = match hook_bridge.lock() {
        Ok(runtime) => runtime.broadcast_hub.clone(),
        Err(_) => return,
    };
    broadcast_hook_bridge_messages(&hub, &[serialized]);
}

fn subscriber_accepts_broadcast(subscriber: &HookBridgeSubscriber, broadcast: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(broadcast) else {
        return false;
    };
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return false;
    };
    subscriber
        .channels
        .iter()
        .any(|channel| channel_accepts_method(channel, method))
}

fn channel_accepts_method(channel: &str, method: &str) -> bool {
    let channel = channel.trim();
    if channel.is_empty() {
        return false;
    }
    method == channel
        || method
            .strip_prefix(channel)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn hook_bridge_read_timed_out(error: &tungstenite::Error) -> bool {
    matches!(
        error,
        tungstenite::Error::Io(io_error)
            if matches!(io_error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
    )
}

fn hook_bridge_error_json(message: impl Into<String>) -> String {
    serde_json::json!({
        "type": "error",
        "data": {
            "message": message.into()
        }
    })
    .to_string()
}

fn invalid_request(message: impl Into<String>) -> Result<(u16, String)> {
    structured_error(
        400,
        json!({
            "code": "invalid_request",
            "message": message.into(),
        }),
    )
}

fn id_mismatch(kind: &str, path_id: &str, body_id: &str) -> Result<(u16, String)> {
    structured_error(
        400,
        json!({
            "code": "id_mismatch",
            "message": format!("path {kind} id `{path_id}` does not match body id `{body_id}`"),
            "path_id": path_id,
            "body_id": body_id,
        }),
    )
}

fn tool_registry_error_response(error: ToolRegistryError) -> Result<(u16, String)> {
    match error {
        ToolRegistryError::InvalidToolDefinition { id, reason } => structured_error(
            400,
            json!({
                "code": "invalid_tool",
                "message": reason,
                "tool_id": id,
            }),
        ),
        ToolRegistryError::ExecutionRejected { id } => structured_error(
            400,
            json!({
                "code": "tool_disabled",
                "message": format!("tool `{id}` is disabled"),
                "tool_id": id,
            }),
        ),
        ToolRegistryError::UnsupportedExecution { id, execution_type } => structured_error(
            400,
            json!({
                "code": "unsupported_tool_execution",
                "message": format!("tool `{id}` execution type `{execution_type}` is not supported"),
                "tool_id": id,
                "execution_type": execution_type,
            }),
        ),
        ToolRegistryError::MissingMcpServer { tool_id, server_id } => structured_error(
            404,
            json!({
                "code": "mcp_server_not_found",
                "message": format!("MCP server `{server_id}` for tool `{tool_id}` was not found or is disabled"),
                "tool_id": tool_id,
                "server_id": server_id,
            }),
        ),
        ToolRegistryError::CliWrapperFailed { id, reason } => structured_error(
            500,
            json!({
                "code": "cli_wrapper_failed",
                "message": reason,
                "tool_id": id,
            }),
        ),
        ToolRegistryError::Mcp(error) => structured_error(
            500,
            json!({
                "code": "mcp_execution_error",
                "message": error.to_string(),
            }),
        ),
        ToolRegistryError::ScriptNotFound { id, path } => structured_error(
            404,
            json!({
                "code": "script_not_found",
                "message": format!("script `{path}` for tool `{id}` was not found"),
                "tool_id": id,
                "path": path,
            }),
        ),
        ToolRegistryError::ScriptSpawn { id, path, source } => structured_error(
            500,
            json!({
                "code": "script_execution_error",
                "message": source.to_string(),
                "tool_id": id,
                "path": path,
            }),
        ),
        ToolRegistryError::ScriptTimedOut {
            id,
            path,
            timeout_ms,
        } => structured_error(
            500,
            json!({
                "code": "script_execution_timeout",
                "message": format!("script timed out after {timeout_ms}ms"),
                "tool_id": id,
                "path": path,
                "timeout_ms": timeout_ms,
            }),
        ),
        ToolRegistryError::ScriptFailed {
            id,
            path,
            code,
            stderr,
        } => structured_error(
            500,
            json!({
                "code": "script_execution_error",
                "message": format!("script exited with code {code:?}: {stderr}"),
                "tool_id": id,
                "path": path,
                "exit_code": code,
            }),
        ),
        ToolRegistryError::ScriptEmptyStdout { id, path } => structured_error(
            500,
            json!({
                "code": "script_execution_error",
                "message": "script returned no stdout",
                "tool_id": id,
                "path": path,
            }),
        ),
        ToolRegistryError::ScriptJson {
            id,
            path,
            source,
            stdout,
        } => structured_error(
            500,
            json!({
                "code": "script_execution_error",
                "message": format!("script returned invalid JSON: {source}"),
                "tool_id": id,
                "path": path,
                "stdout": stdout,
            }),
        ),
        ToolRegistryError::PythonArtNotFound { id, art_id } => structured_error(
            404,
            json!({
                "code": "python_art_not_found",
                "message": format!("Python Art `{art_id}` for tool `{id}` was not found"),
                "tool_id": id,
                "art_id": art_id,
            }),
        ),
        ToolRegistryError::PythonArtLauncherNotFound { id } => structured_error(
            500,
            json!({
                "code": "python_art_launcher_not_found",
                "message": format!("Python Art launcher for tool `{id}` was not found"),
                "tool_id": id,
            }),
        ),
        ToolRegistryError::PythonArtSpawn { id, art_id, source } => structured_error(
            500,
            json!({
                "code": "python_art_execution_error",
                "message": source.to_string(),
                "tool_id": id,
                "art_id": art_id,
            }),
        ),
        ToolRegistryError::PythonArtFailed {
            id,
            art_id,
            code,
            stderr,
        } => structured_error(
            500,
            json!({
                "code": "python_art_execution_error",
                "message": format!("Python Art exited with code {code:?}: {stderr}"),
                "tool_id": id,
                "art_id": art_id,
                "exit_code": code,
            }),
        ),
        ToolRegistryError::PythonArtEmptyStdout { id, art_id } => structured_error(
            500,
            json!({
                "code": "python_art_execution_error",
                "message": "Python Art returned no stdout",
                "tool_id": id,
                "art_id": art_id,
            }),
        ),
        ToolRegistryError::PythonArtJson {
            id,
            art_id,
            source,
            stdout,
        } => structured_error(
            500,
            json!({
                "code": "python_art_execution_error",
                "message": format!("Python Art returned invalid JSON: {source}"),
                "tool_id": id,
                "art_id": art_id,
                "stdout": stdout,
            }),
        ),
        ToolRegistryError::PythonArtStatus {
            id,
            art_id,
            status,
            message,
        } => structured_error(
            500,
            json!({
                "code": "python_art_execution_error",
                "message": message,
                "tool_id": id,
                "art_id": art_id,
                "status": status,
            }),
        ),
        ToolRegistryError::CloudInvalidMethod { id, method } => structured_error(
            400,
            json!({
                "code": "cloud_api_error",
                "message": format!("cloud API method `{method}` is not supported"),
                "tool_id": id,
                "method": method,
            }),
        ),
        ToolRegistryError::CloudRequest {
            id,
            endpoint,
            source,
        } => structured_error(
            500,
            json!({
                "code": "cloud_api_error",
                "message": source.to_string(),
                "tool_id": id,
                "endpoint": endpoint,
            }),
        ),
        ToolRegistryError::CloudHttpStatus {
            id,
            endpoint,
            status,
            body,
        } => structured_error(
            500,
            json!({
                "code": "cloud_api_error",
                "message": format!("cloud API returned HTTP {status}: {body}"),
                "tool_id": id,
                "endpoint": endpoint,
                "status": status,
            }),
        ),
        ToolRegistryError::CloudJson {
            id,
            endpoint,
            source,
            body,
        } => structured_error(
            500,
            json!({
                "code": "cloud_api_error",
                "message": format!("cloud API returned invalid JSON: {source}"),
                "tool_id": id,
                "endpoint": endpoint,
                "body": body,
            }),
        ),
        ToolRegistryError::CloudTemplate { id, field, reason } => structured_error(
            400,
            json!({
                "code": "cloud_api_error",
                "message": format!("cloud API {field} template is invalid: {reason}"),
                "tool_id": id,
                "field": field,
            }),
        ),
        ToolRegistryError::FrameworkPackageNotFound {
            id,
            framework,
            path,
        } => structured_error(
            404,
            json!({
                "code": "framework_package_not_found",
                "message": format!("framework package `{framework}` for tool `{id}` was not found"),
                "tool_id": id,
                "framework": framework,
                "path": path,
            }),
        ),
        ToolRegistryError::FrameworkArtDirectoryNotFound { id, path } => structured_error(
            404,
            json!({
                "code": "framework_art_directory_not_found",
                "message": format!("framework Art directory for tool `{id}` was not found"),
                "tool_id": id,
                "path": path,
            }),
        ),
        ToolRegistryError::FrameworkProcessSpawn {
            id,
            framework,
            reason,
        } => structured_error(
            500,
            json!({
                "code": "framework_process_spawn_error",
                "message": reason,
                "tool_id": id,
                "framework": framework,
            }),
        ),
        ToolRegistryError::FrameworkProcessTimeout {
            id,
            framework,
            timeout_ms,
        } => structured_error(
            504,
            json!({
                "code": "framework_process_timeout",
                "message": format!("framework process timed out after {timeout_ms}ms"),
                "tool_id": id,
                "framework": framework,
                "timeoutMs": timeout_ms,
            }),
        ),
        ToolRegistryError::FrameworkProcessIo {
            id,
            framework,
            reason,
        } => structured_error(
            500,
            json!({
                "code": "framework_process_io_error",
                "message": reason,
                "tool_id": id,
                "framework": framework,
            }),
        ),
        ToolRegistryError::FrameworkProcessProtocol {
            id,
            framework,
            reason,
        } => structured_error(
            502,
            json!({
                "code": "framework_process_protocol_error",
                "message": reason,
                "tool_id": id,
                "framework": framework,
            }),
        ),
        ToolRegistryError::FrameworkProcessFailed {
            id,
            framework,
            code,
            message,
            detail,
        } => structured_error(
            500,
            json!({
                "code": "framework_execution_error",
                "message": message,
                "detail": detail,
                "frameworkCode": code,
                "tool_id": id,
                "framework": framework,
            }),
        ),
        ToolRegistryError::Io(error) => structured_error(
            500,
            json!({
                "code": "tool_registry_error",
                "message": error.to_string(),
            }),
        ),
        ToolRegistryError::Json(error) => structured_error(
            500,
            json!({
                "code": "tool_registry_error",
                "message": error.to_string(),
            }),
        ),
    }
}

fn workflow_runtime_error_response(error: WorkflowRuntimeError) -> Result<(u16, String)> {
    structured_error(
        500,
        json!({
            "code": "workflow_runtime_error",
            "message": error.to_string(),
        }),
    )
}

fn workflow_store_error_response(error: WorkflowStoreError) -> Result<(u16, String)> {
    match error {
        WorkflowStoreError::InvalidWorkflowId(id) => structured_error(
            400,
            json!({
                "code": "invalid_workflow_id",
                "message": format!("invalid workflow id `{id}`"),
                "workflow_id": id,
            }),
        ),
        WorkflowStoreError::InvalidWorkflowYaml(message) => structured_error(
            400,
            json!({
                "code": "invalid_workflow",
                "message": message,
            }),
        ),
        WorkflowStoreError::NotFound(id) => structured_error(
            404,
            json!({
                "code": "workflow_not_found",
                "message": format!("workflow `{id}` was not found"),
                "workflow_id": id,
            }),
        ),
        WorkflowStoreError::Io(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
        WorkflowStoreError::Json(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
        WorkflowStoreError::Yaml(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
    }
}

fn invoke_capability(
    body: &str,
    run_store: &SharedRunStore,
    brain_planner: &SharedBrainPlanner,
) -> Result<(u16, String)> {
    let Ok(request) = serde_json::from_str::<InvokeCapabilityRequest>(body) else {
        return bad_request("invalid invoke request");
    };
    if request.request_id.trim().is_empty() {
        return bad_request("invalid invoke request: requestId is required");
    }
    if request.caller.trim().is_empty() {
        return invoke_error(
            400,
            Some(&request.request_id),
            "invalid_request",
            "caller is required",
            json!({}),
        );
    }
    match request.capability.as_str() {
        CAPABILITY_BRAIN_PLAN => invoke_brain_plan(request, run_store, brain_planner),
        CAPABILITY_TEA_TICKET_DECOMPOSE => invoke_tea_ticket_decompose(request, run_store),
        _ => invoke_error(
            404,
            Some(&request.request_id),
            "unknown_capability",
            &format!("unknown capability `{}`", request.capability),
            json!({
                "capability": request.capability,
            }),
        ),
    }
}

fn invoke_brain_plan(
    request: InvokeCapabilityRequest,
    run_store: &SharedRunStore,
    brain_planner: &SharedBrainPlanner,
) -> Result<(u16, String)> {
    let InvokeCapabilityRequest {
        request_id, input, ..
    } = request;
    let run_id = loom_core::RunId::new().to_string();
    let goal = input
        .get("goal")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(goal) = goal else {
        return invoke_error(
            400,
            Some(&request_id),
            "invalid_input",
            "brain.plan input.goal is required",
            json!({
                "capability": CAPABILITY_BRAIN_PLAN,
            }),
        );
    };

    let constraints = input
        .get("constraints")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let context = input.get("context").cloned();
    let session_id = loom_core::SessionId::new().to_string();
    let mut run = json!({
        "id": run_id,
        "capability": CAPABILITY_BRAIN_PLAN,
        "loom_session_id": session_id,
        "status": "running",
        "input": input,
    });

    let started = match RunEventDraft::new(
        "run_started",
        json!({
            "capability": CAPABILITY_BRAIN_PLAN,
            "status": "running",
        }),
    ) {
        Ok(event) => event,
        Err(error) => return run_store_failed(error),
    };
    {
        let mut store = match lock_run_store(run_store) {
            Ok(store) => store,
            Err(error) => return run_store_failed(error),
        };
        if let Err(error) = store.insert_run(run.clone(), vec![started]) {
            return run_store_failed(error);
        }
    }

    let planning = brain_planner.plan(BrainPlanRequest {
        goal: goal.to_owned(),
        constraints,
        context,
    });
    match planning {
        Ok(result) => {
            let planner = json!({
                "source": result.source.as_str(),
                "model": result.model,
            });
            let output = json!({
                "summary": result.summary,
                "steps": result.steps,
                "planner": planner,
            });
            run["status"] = json!("succeeded");
            run["output"] = output.clone();

            let completed = match RunEventDraft::new(
                "capability_completed",
                json!({
                    "capability": CAPABILITY_BRAIN_PLAN,
                    "status": "succeeded",
                    "planner": planner,
                }),
            ) {
                Ok(event) => event,
                Err(error) => return run_store_failed(error),
            };
            let mut store = match lock_run_store(run_store) {
                Ok(store) => store,
                Err(error) => return run_store_failed(error),
            };
            if let Err(error) = store.transition_run(run.clone(), completed) {
                return run_store_failed(error);
            }
            Ok((
                200,
                serde_json::to_string(&json!({
                    "requestId": request_id,
                    "status": "succeeded",
                    "output": {
                        "runId": run_id,
                        "run": run,
                        "summary": output["summary"].clone(),
                        "steps": output["steps"].clone(),
                        "planner": output["planner"].clone(),
                    }
                }))?,
            ))
        }
        Err(error) => {
            let planner_status: BrainPlannerStatus = brain_planner.status();
            let planner = json!({
                "source": planner_status.mode,
                "model": planner_status.model,
            });
            let run_error = json!({
                "code": "gateway_planner_failed",
                "message": "Gateway-backed planning failed",
                "diagnostic": truncate_diagnostic(error.to_string(), 512),
            });
            run["status"] = json!("failed");
            run["planner"] = planner.clone();
            run["error"] = run_error.clone();

            let failed = match RunEventDraft::new(
                "capability_failed",
                json!({
                    "capability": CAPABILITY_BRAIN_PLAN,
                    "status": "failed",
                    "planner": planner,
                    "error": {
                        "code": "gateway_planner_failed",
                    },
                }),
            ) {
                Ok(event) => event,
                Err(error) => return run_store_failed(error),
            };
            let mut store = match lock_run_store(run_store) {
                Ok(store) => store,
                Err(error) => return run_store_failed(error),
            };
            if let Err(error) = store.transition_run(run, failed) {
                return run_store_failed(error);
            }
            invoke_error(
                502,
                Some(&request_id),
                "gateway_planner_failed",
                "Gateway-backed planning failed",
                json!({
                    "capability": CAPABILITY_BRAIN_PLAN,
                    "runId": run_id,
                }),
            )
        }
    }
}

fn invoke_tea_ticket_decompose(
    request: InvokeCapabilityRequest,
    run_store: &SharedRunStore,
) -> Result<(u16, String)> {
    let Some(ticket) = request.input.get("ticket").and_then(Value::as_object) else {
        return invoke_error(
            400,
            Some(&request.request_id),
            "invalid_input",
            "tea.ticket.decompose.v1 input.ticket is required",
            json!({
                "capability": CAPABILITY_TEA_TICKET_DECOMPOSE,
            }),
        );
    };
    let ticket_title = ticket
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Tea ticket");
    let ticket_description = ticket
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let risk = if ticket_description.len() > 240 {
        "high"
    } else if ticket_description.len() < 12 {
        "medium"
    } else {
        "medium"
    };
    let proposal_id = loom_core::RunId::new().to_string();
    let proposal = json!({
        "schema_version": 1,
        "proposal_id": proposal_id,
        "analysis": {
            "intent": "engineering_work_order",
            "target_components": ["Tea"],
            "target_paths": [],
            "constraints": [
                "Loom must not mutate Tea ticket state directly",
                "Tea validates, stores, and governs decomposition records"
            ],
            "acceptance_criteria": [
                "analysis and plan are returned as a proposal",
                "Tea commits accepted records into its own timeline",
                "verification evidence is attached before human acceptance"
            ],
            "missing_context": if ticket_description.is_empty() {
                json!(["ticket description is empty"])
            } else {
                json!([])
            },
            "risk_assessment": risk,
            "confidence": 0.82,
            "recommended_policy": "human_before_execute",
            "recommended_workflow": "loom.tea_ticket_decompose.v1"
        },
        "plan": {
            "summary": format!("Decompose Tea work order: {ticket_title}"),
            "steps": [
                {
                    "id": "inspect-context",
                    "title": "Inspect context",
                    "description": "Read the Tea ticket snapshot, comments, policy, and available workspace context."
                },
                {
                    "id": "propose-plan",
                    "title": "Propose plan",
                    "description": "Generate a bounded plan that Tea can validate and store before execution."
                },
                {
                    "id": "validate",
                    "title": "Validate",
                    "description": "Define concrete verification commands and evidence requirements for human review."
                }
            ],
            "required_tools": ["loom.run"],
            "expected_artifacts": ["Tea analysis record", "Tea plan record", "Loom run evidence"],
            "validation_strategy": ["Tea stores the returned analysis and plan", "Loom does not write Tea state directly"],
            "rollback_strategy": ["leave the Tea ticket blocked with proposal evidence"],
            "requires_approval_before_execute": true
        },
        "requires_human_review": true,
        "notes": []
    });
    let run_id = loom_core::RunId::new().to_string();
    let output = json!({
        "proposal": proposal,
        "summary": format!("Tea decomposition proposal prepared for {ticket_title}")
    });
    let run = json!({
        "id": run_id,
        "capability": CAPABILITY_TEA_TICKET_DECOMPOSE,
        "loom_session_id": loom_core::SessionId::new().to_string(),
        "status": "succeeded",
        "input": request.input,
        "output": output.clone()
    });

    let events = match (
        RunEventDraft::new(
            "run_started",
            json!({
                "capability": CAPABILITY_TEA_TICKET_DECOMPOSE,
                "status": "running",
            }),
        ),
        RunEventDraft::new(
            "capability_completed",
            json!({
                "capability": CAPABILITY_TEA_TICKET_DECOMPOSE,
                "status": "succeeded",
            }),
        ),
    ) {
        (Ok(started), Ok(completed)) => vec![started, completed],
        (Err(error), _) | (_, Err(error)) => return run_store_failed(error),
    };
    let mut store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    if let Err(error) = store.insert_run(run.clone(), events) {
        return run_store_failed(error);
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "requestId": request.request_id,
            "status": "succeeded",
            "output": {
                "runId": run_id,
                "run": run,
                "proposal": output["proposal"].clone(),
                "summary": output["summary"].clone(),
            }
        }))?,
    ))
}

fn get_run(run_id: &str, run_store: &SharedRunStore) -> Result<(u16, String)> {
    let store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    let run = match store.get_run(run_id) {
        Ok(Some(run)) => run,
        Ok(None) => return run_not_found(run_id),
        Err(error) => return run_store_failed(error),
    };
    Ok((200, serde_json::to_string(&run)?))
}

fn get_run_events(run_id: &str, run_store: &SharedRunStore) -> Result<(u16, String)> {
    let store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    let events = match store.get_events(run_id) {
        Ok(Some(events)) => events,
        Ok(None) => return run_not_found(run_id),
        Err(error) => return run_store_failed(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "run_id": run_id,
            "events": events,
        }))?,
    ))
}

fn start_tea_run(body: &str, run_store: &SharedRunStore) -> Result<(u16, String)> {
    let Ok(request) = serde_json::from_str::<StartRunRequest>(body) else {
        return bad_request("invalid run request");
    };
    let ticket = request.ticket;
    let run = json!({
        "id": loom_core::RunId::new().to_string(),
        "ticket_id": ticket.id,
        "loom_session_id": loom_core::SessionId::new().to_string(),
        "status": "succeeded",
        "evidence": {
            "summary": format!(
                "loom daemon run completed for {}",
                ticket.title
                    .as_deref()
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or("Tea ticket")
            ),
            "commands": [],
            "artifacts": [
                "loom-daemon:http-run-contract"
            ],
            "risks": ticket
                .description
                .as_deref()
                .filter(|description| !description.trim().is_empty())
                .map(|description| vec![format!("request context length: {} bytes", description.len())])
                .unwrap_or_default()
        }
    });
    let events = match (
        RunEventDraft::new(
            "run_started",
            json!({
                "source": "tea",
                "status": "running",
            }),
        ),
        RunEventDraft::new(
            "run_finished",
            json!({
                "source": "tea",
                "status": "succeeded",
            }),
        ),
    ) {
        (Ok(started), Ok(finished)) => vec![started, finished],
        (Err(error), _) | (_, Err(error)) => return run_store_failed(error),
    };
    let mut store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    if let Err(error) = store.insert_run(run.clone(), events) {
        return run_store_failed(error);
    }
    Ok((200, serde_json::to_string(&run)?))
}

fn run_action(
    path_run_id: &str,
    body: &str,
    status: &str,
    run_store: &SharedRunStore,
) -> Result<(u16, String)> {
    let Ok(request) = serde_json::from_str::<RunActionRequest>(body) else {
        return bad_request("invalid run action request");
    };
    let Some(body_run_id) = request.run.get("id").and_then(Value::as_str) else {
        return structured_error(
            400,
            json!({
                "code": "invalid_run_action_request",
                "message": "run action request requires run.id",
            }),
        );
    };
    if body_run_id != path_run_id {
        return structured_error(
            400,
            json!({
                "code": "run_id_mismatch",
                "message": format!(
                    "path run id `{path_run_id}` does not match body run id `{body_run_id}`"
                ),
                "path_run_id": path_run_id,
                "body_run_id": body_run_id,
            }),
        );
    }

    let mut store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    let mut run = match store.get_run(path_run_id) {
        Ok(Some(run)) => run,
        Ok(None) => return run_not_found(path_run_id),
        Err(error) => return run_store_failed(error),
    };
    run["status"] = json!(status);
    let event = match RunEventDraft::new(
        "run_action",
        json!({
            "action": status,
            "status": status,
        }),
    ) {
        Ok(event) => event,
        Err(error) => return run_store_failed(error),
    };
    if let Err(error) = store.transition_run(run.clone(), event) {
        return run_store_failed(error);
    }
    Ok((200, serde_json::to_string(&run)?))
}

fn bad_request(message: &'static str) -> Result<(u16, String)> {
    Ok((
        400,
        serde_json::to_string(&json!({
            "status": "failed",
            "error": {
                "code": "invalid_request",
                "message": message,
            }
        }))?,
    ))
}

fn structured_error(status: u16, error: Value) -> Result<(u16, String)> {
    Ok((status, serde_json::to_string(&json!({ "error": error }))?))
}

fn request_worker_failed_response() -> (u16, String) {
    structured_error(
        500,
        json!({
            "code": "request_worker_failed",
            "message": "Loom could not complete the request"
        }),
    )
    .expect("serialize request worker failure response")
}

fn daemon_busy_response() -> (u16, String) {
    structured_error(
        503,
        json!({
            "code": "daemon_busy",
            "message": "Loom daemon request queue is full",
            "retryable": true,
        }),
    )
    .expect("serialize daemon busy response")
}

fn daemon_shutting_down_response() -> (u16, String) {
    structured_error(
        503,
        json!({
            "code": "daemon_shutting_down",
            "message": "Loom daemon is shutting down",
            "retryable": true,
        }),
    )
    .expect("serialize daemon shutdown response")
}

fn invoke_error(
    status: u16,
    request_id: Option<&str>,
    code: &str,
    message: &str,
    fields: Value,
) -> Result<(u16, String)> {
    let mut error = json!({
        "code": code,
        "message": message,
    });
    merge_object_fields(&mut error, fields);
    Ok((
        status,
        serde_json::to_string(&json!({
            "requestId": request_id.unwrap_or_default(),
            "status": "failed",
            "error": error,
        }))?,
    ))
}

fn truncate_diagnostic(diagnostic: String, max_bytes: usize) -> String {
    if diagnostic.len() <= max_bytes {
        return diagnostic;
    }
    if max_bytes <= 3 {
        return diagnostic.chars().take(max_bytes).collect::<String>();
    }
    let mut end = max_bytes - 3;
    while end > 0 && !diagnostic.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &diagnostic[..end])
}

fn run_not_found(run_id: &str) -> Result<(u16, String)> {
    structured_error(
        404,
        json!({
            "code": "run_not_found",
            "message": format!("run `{run_id}` was not found"),
            "run_id": run_id,
        }),
    )
}

fn run_store_failed(error: RunStoreError) -> Result<(u16, String)> {
    eprintln!("loom run store operation failed: {error}");
    structured_error(
        500,
        json!({
            "code": "run_store_failed",
            "message": "Loom run evidence could not be stored"
        }),
    )
}

fn lock_run_store(
    run_store: &SharedRunStore,
) -> std::result::Result<std::sync::MutexGuard<'_, Box<dyn RunEvidenceStore>>, RunStoreError> {
    run_store
        .lock()
        .map_err(|_| RunStoreError::Integrity("run store lock poisoned".to_owned()))
}

fn merge_object_fields(target: &mut Value, fields: Value) {
    let (Some(target), Some(fields)) = (target.as_object_mut(), fields.as_object()) else {
        return;
    };
    for (key, value) in fields {
        target.insert(key.clone(), value.clone());
    }
}

fn run_path_id(path: &str) -> Option<&str> {
    let run_id = path.strip_prefix("/v1/runs/")?;
    (!run_id.is_empty() && !run_id.contains('/')).then_some(run_id)
}

fn run_events_path_id(path: &str) -> Option<&str> {
    let run_id = path.strip_prefix("/v1/runs/")?.strip_suffix("/events")?;
    (!run_id.is_empty() && !run_id.contains('/')).then_some(run_id)
}

fn run_action_path_id<'a>(path: &'a str, action: &str) -> Option<&'a str> {
    let suffix = match action {
        "stop" => "/stop",
        "retry" => "/retry",
        _ => return None,
    };
    let run_id = path.strip_prefix("/v1/runs/")?.strip_suffix(suffix)?;
    (!run_id.is_empty() && !run_id.contains('/')).then_some(run_id)
}

fn module_statuses() -> Vec<ModuleStatus> {
    vec![
        ModuleStatus {
            name: "core",
            version: loom_core::LOOM_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "durable",
            version: loom_durable::LOOM_DURABLE_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "agent",
            version: loom_agent::LOOM_AGENT_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "workflow",
            version: loom_workflow::LOOM_WORKFLOW_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "memory",
            version: loom_memory::LOOM_MEMORY_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "sandbox",
            version: loom_sandbox::LOOM_SANDBOX_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "gateway",
            version: loom_gateway::LOOM_GATEWAY_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "hooks",
            version: loom_hooks::LOOM_HOOKS_VERSION,
            initialized: true,
        },
    ]
}

fn write_response(stream: &mut impl Write, status: u16, body: &str) -> Result<()> {
    let reason = response_reason(status);
    let content_type = if body.trim_start().starts_with("<!doctype html") {
        "text/html; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .context("write daemon response")
}

fn write_binary_response(
    stream: &mut impl Write,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let reason = response_reason(status);
    // Preview URLs carry a content version token, so a changed image always
    // arrives under a new URL. Force revalidation anyway so an in-place image
    // update is never masked by an aggressive WebView/browser cache.
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .context("write daemon binary response")?;
    stream.write_all(body).context("write daemon binary body")
}

fn response_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
    use std::fs;
    use std::io::{BufRead, Cursor, Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::{mpsc, Arc, Condvar, Mutex};
    use std::thread;
    use std::time::Duration;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn framework_package_zip(id: &str, version: &str) -> Vec<u8> {
        let command = match id {
            "cli_wrapper" => "runtime/loom-framework-cli-wrapper.exe",
            "cloud_api" => "runtime/loom-framework-cloud-api.exe",
            "script" => "runtime/loom-framework-script.exe",
            "python_art" => "runtime/loom-framework-python-art.exe",
            "mcp" => "runtime/loom-framework-mcp.exe",
            "workflow" => "runtime/loom-framework-workflow.exe",
            other => panic!("unsupported test framework: {other}"),
        };
        let manifest = serde_json::json!({
            "id": id,
            "name": format!("{id} daemon test framework"),
            "description": "daemon framework package test",
            "version": version,
            "protocolVersion": "loom.framework.v1",
            "platforms": ["windows-x64"],
            "entry": { "kind": "process", "command": command, "args": ["--stdio"] },
            "permissions": ["process.spawn"],
            "artExecution": {
                "requestSchema": "loom.art.execute.v1",
                "responseSchema": "loom.art.result.v1"
            }
        });
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer
                .start_file("framework.manifest.json", options)
                .expect("manifest entry");
            writer
                .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
                .expect("manifest bytes");
            writer.start_file(command, options).expect("runtime entry");
            writer
                .write_all(b"MZ-test-framework")
                .expect("runtime bytes");
            writer.finish().expect("finish package");
        }
        bytes
    }

    #[test]
    fn framework_package_routes_cover_install_upgrade_disable_enable_uninstall() {
        let root = unique_temp_dir("framework-package-routes");
        let registry = FrameworkRegistry::new(&root);

        let install_body = serde_json::to_string(&json!({
            "zipBase64": format!(
                "data:application/zip;base64,{}",
                BASE64.encode(framework_package_zip("script", "1.0.0"))
            )
        }))
        .expect("install body");
        let (status, body) = install_framework_package(&install_body, &registry).expect("install");
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_str::<Value>(&body).expect("install response")["framework"]["version"],
            "1.0.0"
        );

        let (status, body) = set_framework_enabled("script", false, &registry).expect("disable");
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_str::<Value>(&body).expect("disable response")["framework"]["enabled"],
            false
        );

        let (status, body) = set_framework_enabled("script", true, &registry).expect("enable");
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_str::<Value>(&body).expect("enable response")["framework"]["ready"],
            true
        );

        let upgrade_body = serde_json::to_string(&json!({
            "zipBase64": BASE64.encode(framework_package_zip("script", "2.0.0"))
        }))
        .expect("upgrade body");
        let (status, body) =
            upgrade_framework_package("script", &upgrade_body, &registry).expect("upgrade");
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_str::<Value>(&body).expect("upgrade response")["framework"]["version"],
            "2.0.0"
        );

        let (status, body) = uninstall_framework("script", &registry).expect("uninstall");
        assert_eq!(status, 200);
        assert_eq!(
            serde_json::from_str::<Value>(&body).expect("uninstall response")["framework"]
                ["installed"],
            false
        );
        assert!(!root.join("frameworks").join("script").exists());

        fs::remove_dir_all(&root).ok();
    }

    // Regression: ArtLoom sends cloud_api `headers` as an object (with a
    // `{api_key}` placeholder) and `body` as an array of field descriptors. The
    // executor expects both as JSON strings; the old converter used a
    // string-only extractor and silently dropped them, so RemoveBG uploaded no
    // image and PhotoRoom answered "missing_image". These lock the conversion.
    #[test]
    fn artloom_cloud_headers_object_becomes_json_string_with_api_key_resolved() {
        let execution = serde_json::json!({
            "type": "cloud_api",
            "method": "POST",
            "content_type": "multipart/form-data",
            "endpoint": "https://sdk.photoroom.com/v1/segment",
            "api_key": "sk_test_123",
            "headers": { "x-api-key": "{api_key}" }
        });
        let rendered = artloom_headers_to_json_string(&execution).expect("headers json");
        let parsed: std::collections::HashMap<String, String> =
            serde_json::from_str(&rendered).expect("headers parse as string map");
        assert_eq!(
            parsed.get("x-api-key").map(String::as_str),
            Some("sk_test_123")
        );
    }

    #[test]
    fn artloom_cloud_body_array_becomes_json_string_with_image_file_field() {
        let execution = serde_json::json!({
            "type": "cloud_api",
            "body": [
                { "name": "image_file", "execution_type": "image_buffer", "source": "input" },
                { "name": "format", "default": "png" }
            ]
        });
        let rendered = artloom_body_to_json_string(&execution).expect("body json");
        let parsed: std::collections::HashMap<String, String> =
            serde_json::from_str(&rendered).expect("body parse as string map");
        // Input image field must carry a `.path}}` template so the executor
        // uploads the temp input file as a multipart file part.
        assert_eq!(
            parsed.get("image_file").map(String::as_str),
            Some("{{inputs.input.path}}")
        );
        assert_eq!(parsed.get("format").map(String::as_str), Some("png"));
    }

    #[test]
    fn artloom_cloud_headers_and_body_pass_through_existing_json_strings() {
        let execution = serde_json::json!({
            "type": "cloud_api",
            "headers": "{\"X-Trace\":\"abc\"}",
            "body": "{\"file\":\"{{inputs.image.path}}\"}"
        });
        assert_eq!(
            artloom_headers_to_json_string(&execution).as_deref(),
            Some("{\"X-Trace\":\"abc\"}")
        );
        assert_eq!(
            artloom_body_to_json_string(&execution).as_deref(),
            Some("{\"file\":\"{{inputs.image.path}}\"}")
        );
    }

    const CONCURRENCY_GATE_TIMEOUT: Duration = Duration::from_secs(5);

    fn wait_for_test_gate(gate: &Arc<(Mutex<bool>, Condvar)>, timeout: Duration) -> bool {
        let (gate_lock, gate_signal) = &**gate;
        let released = gate_lock.lock().expect("read test gate");
        let (released, _) = gate_signal
            .wait_timeout_while(released, timeout, |released| !*released)
            .expect("wait test gate");
        *released
    }

    fn release_test_gate(gate: &Arc<(Mutex<bool>, Condvar)>) {
        let (gate_lock, gate_signal) = &**gate;
        *gate_lock.lock().expect("release test gate") = true;
        gate_signal.notify_all();
    }

    struct BlockingBrainPlanner {
        entered: Arc<(Mutex<bool>, Condvar)>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    impl brain_plan::BrainPlanner for BlockingBrainPlanner {
        fn plan(
            &self,
            _request: BrainPlanRequest,
        ) -> std::result::Result<brain_plan::BrainPlanResult, brain_plan::BrainPlannerError>
        {
            let (entered_lock, entered_signal) = &*self.entered;
            *entered_lock.lock().expect("enter planner") = true;
            entered_signal.notify_all();
            if !wait_for_test_gate(&self.release, CONCURRENCY_GATE_TIMEOUT) {
                return Err(brain_plan::BrainPlannerError::InvalidModelOutput(
                    "fixture release timed out".to_owned(),
                ));
            }
            Ok(brain_plan::BrainPlanResult {
                summary: "concurrent plan".to_owned(),
                steps: vec!["complete".to_owned()],
                source: brain_plan::BrainPlanSource::Gateway,
                model: Some("fixture-model".to_owned()),
            })
        }

        fn status(&self) -> BrainPlannerStatus {
            BrainPlannerStatus {
                mode: "gateway",
                configured: true,
                model: Some("fixture-model".to_owned()),
                timeout_seconds: Some(30),
            }
        }
    }

    struct CountingBlockingBrainPlanner {
        entered: Arc<(Mutex<usize>, Condvar)>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    impl brain_plan::BrainPlanner for CountingBlockingBrainPlanner {
        fn plan(
            &self,
            _request: BrainPlanRequest,
        ) -> std::result::Result<brain_plan::BrainPlanResult, brain_plan::BrainPlannerError>
        {
            let (entered_lock, entered_signal) = &*self.entered;
            *entered_lock.lock().expect("count planner entry") += 1;
            entered_signal.notify_all();
            if !wait_for_test_gate(&self.release, CONCURRENCY_GATE_TIMEOUT) {
                return Err(brain_plan::BrainPlannerError::InvalidModelOutput(
                    "fixture release timed out".to_owned(),
                ));
            }
            Ok(brain_plan::BrainPlanResult {
                summary: "concurrent plan".to_owned(),
                steps: vec!["complete".to_owned()],
                source: brain_plan::BrainPlanSource::Gateway,
                model: Some("fixture-model".to_owned()),
            })
        }

        fn status(&self) -> BrainPlannerStatus {
            BrainPlannerStatus {
                mode: "gateway",
                configured: true,
                model: Some("fixture-model".to_owned()),
                timeout_seconds: Some(30),
            }
        }
    }

    struct ConcurrencyTestFixture {
        releases: Vec<Box<dyn Fn() + Send + Sync>>,
        shutdown_tx: Option<mpsc::Sender<()>>,
        clients: Vec<thread::JoinHandle<()>>,
        server: Option<thread::JoinHandle<Result<()>>>,
    }

    impl ConcurrencyTestFixture {
        fn new(shutdown_tx: mpsc::Sender<()>, server: thread::JoinHandle<Result<()>>) -> Self {
            Self {
                releases: Vec::new(),
                shutdown_tx: Some(shutdown_tx),
                clients: Vec::new(),
                server: Some(server),
            }
        }

        fn add_release_action<F>(&mut self, action: F)
        where
            F: Fn() + Send + Sync + 'static,
        {
            self.releases.push(Box::new(action));
        }

        fn add_release_gate(&mut self, gate: Arc<(Mutex<bool>, Condvar)>) {
            self.add_release_action(move || release_test_gate(&gate));
        }

        fn release_gates(&self) {
            for release in &self.releases {
                release();
            }
        }

        fn request_shutdown(&mut self) {
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                let _ = shutdown_tx.send(());
            }
        }

        fn spawn_client<T, F>(&mut self, task: F) -> mpsc::Receiver<T>
        where
            T: Send + 'static,
            F: FnOnce() -> T + Send + 'static,
        {
            let (result_tx, result_rx) = mpsc::channel();
            self.clients.push(thread::spawn(move || {
                let _ = result_tx.send(task());
            }));
            result_rx
        }

        fn join_clients(&mut self) -> Result<()> {
            let mut client_panicked = false;
            for client in self.clients.drain(..) {
                if client.join().is_err() {
                    client_panicked = true;
                }
            }
            if client_panicked {
                anyhow::bail!("Loom concurrency fixture client thread panicked");
            }
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            self.release_gates();
            self.request_shutdown();
            let clients_result = self.join_clients();
            let Some(server) = self.server.take() else {
                return clients_result;
            };
            let server_result = server
                .join()
                .map_err(|_| anyhow::anyhow!("Loom concurrency fixture server thread panicked"))?;
            clients_result?;
            server_result
        }
    }

    impl Drop for ConcurrencyTestFixture {
        fn drop(&mut self) {
            self.release_gates();
            self.request_shutdown();
            for client in self.clients.drain(..) {
                let _ = client.join();
            }
            if let Some(server) = self.server.take() {
                let _ = server.join();
            }
        }
    }

    #[test]
    fn concurrency_fixture_requests_shutdown_before_joining_clients() {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let (client_release_tx, client_release_rx) = mpsc::channel();
        let server = thread::spawn(move || -> Result<()> {
            shutdown_rx
                .recv_timeout(Duration::from_secs(3))
                .context("wait fixture shutdown")?;
            let _ = client_release_tx.send(());
            Ok(())
        });
        let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
        let client_result_rx = fixture.spawn_client(move || {
            client_release_rx
                .recv_timeout(Duration::from_secs(2))
                .is_ok()
        });

        let (finish_tx, finish_rx) = mpsc::channel();
        let finish_thread = thread::spawn(move || {
            let _ = finish_tx.send(fixture.finish());
        });
        let early_result = finish_rx.recv_timeout(Duration::from_millis(750));
        let completed_before_client_timeout = early_result.is_ok();
        let finish_result = early_result.unwrap_or_else(|_| {
            finish_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("fixture finish after client timeout")
        });
        let client_unblocked_by_shutdown = client_result_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("fixture client result");
        finish_thread.join().expect("fixture finish thread");

        finish_result.expect("finish fixture");
        assert!(
            completed_before_client_timeout,
            "fixture joined clients before requesting daemon shutdown"
        );
        assert!(
            client_unblocked_by_shutdown,
            "daemon shutdown did not unblock the fixture client"
        );
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "loom-daemon-contract-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn canonical_test_path(path: impl AsRef<Path>) -> PathBuf {
        fs::canonicalize(path).expect("canonicalize test path")
    }

    fn write_hook_session(appdata: &Path, identifier: &str, contents: &str) -> PathBuf {
        let dir = appdata.join(identifier);
        fs::create_dir_all(&dir).expect("create hook identifier dir");
        let path = dir.join("session.json");
        fs::write(&path, contents).expect("write hook session");
        path
    }

    fn test_daemon_runtime_from_config(
        control_plane_root: &Path,
        config: DaemonConfig,
    ) -> DaemonRuntime {
        let run_store: Box<dyn RunEvidenceStore> = match config.run_store {
            RunStoreConfig::Memory => Box::new(InMemoryRunEvidenceStore::default()),
            RunStoreConfig::Sqlite(path) => {
                Box::new(SqliteRunEvidenceStore::open(path).expect("open test sqlite run store"))
            }
        };
        let run_store_status = run_store.status();
        let brain_planner = build_brain_planner(config.brain_planner).expect("build test planner");
        let config_root = std::env::var_os("LOOM_CONFIGURATION_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| control_plane_root.join("config"));
        let settings_base_url = std::env::var("LOOM_SETTINGS_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:0/settings".to_owned());
        DaemonRuntime {
            hook_settings: config.hook_settings,
            run_store: Arc::new(Mutex::new(run_store)),
            auth_token: config.auth_token,
            config_registry: Arc::new(built_in_registry()),
            config_store: FileDocumentStore::new(config_root),
            mcp_servers: Arc::new(Mutex::new(load_persisted_mcp_servers(control_plane_root))),
            tool_registry: ToolRegistry::new(control_plane_root.join("tools")),
            workflow_store: WorkflowStore::new(control_plane_root.join("workflows")),
            canvas_workflow_root: control_plane_root.join("canvas-workflows"),
            framework_registry: FrameworkRegistry::new(&control_plane_root),
            control_plane_root: control_plane_root.to_path_buf(),
            hook_bridge: Arc::new(Mutex::new(HookBridgeRuntime::new(
                control_plane_root.join("workflows"),
            ))),
            artloom_settings: Arc::new(Mutex::new(ArtLoomCompatSettingsStore::new(
                control_plane_root
                    .join("settings")
                    .join("artloom-compat-settings.json"),
            ))),
            shared_images: Arc::new(Mutex::new(SharedImageStore::new())),
            ocr_provider: Arc::new(Mutex::new(OcrProvider::from_env())),
            settings_base_url,
            mcp_registry_endpoint: config.mcp_registry_endpoint,
            brain_planner,
            run_store_status,
            request_executor_status: config.request_executor.status(),
            serialized_route_lock: Mutex::new(()),
            #[cfg(test)]
            serialized_route_observer: None,
            #[cfg(test)]
            request_submission_observer: None,
            #[cfg(test)]
            shutdown_observer: None,
        }
    }

    fn test_daemon_runtime(control_plane_root: &Path, auth_token: Option<&str>) -> DaemonRuntime {
        let mut config = DaemonConfig::localhost(0);
        if let Some(token) = auth_token {
            config = config.with_bearer_token(token);
        }
        test_daemon_runtime_from_config(control_plane_root, config)
    }

    fn parsed_request(
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> ParsedHttpRequest {
        ParsedHttpRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            body: body.unwrap_or_default().to_owned(),
        }
    }

    fn expect_text_route_response(response: RouteResponse, expected_status: u16) -> String {
        match response {
            RouteResponse::Text { status, body } => {
                assert_eq!(status, expected_status);
                body
            }
            RouteResponse::Binary { .. } => {
                panic!("expected text response with status {expected_status}")
            }
        }
    }

    fn expect_json_text_route_response(
        response: RouteResponse,
        expected_status: u16,
    ) -> serde_json::Value {
        let body = expect_text_route_response(response, expected_status);
        serde_json::from_str(&body).expect("json route body")
    }

    fn expect_json_result_response(
        response: Result<(u16, String)>,
        expected_status: u16,
    ) -> serde_json::Value {
        let (status, body) = response.expect("route result");
        assert_eq!(status, expected_status);
        serde_json::from_str(&body).expect("json result body")
    }

    fn start_test_hook_bridge(runtime: &DaemonRuntime, body: &str) -> serde_json::Value {
        expect_json_result_response(
            start_hook_bridge(
                body,
                &runtime.hook_bridge,
                &runtime.mcp_servers,
                &runtime.tool_registry,
                &runtime.workflow_store,
                &runtime.artloom_settings,
                &runtime.shared_images,
                &runtime.ocr_provider,
            ),
            200,
        )
    }

    fn stop_test_hook_bridge(runtime: &DaemonRuntime) -> serde_json::Value {
        expect_json_result_response(stop_hook_bridge(&runtime.hook_bridge), 200)
    }

    fn hook_bridge_status_value(runtime: &DaemonRuntime) -> serde_json::Value {
        expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200)
    }

    fn run_hook_bridge_text(runtime: &DaemonRuntime, request: &str) -> serde_json::Value {
        let workflow_root = runtime
            .hook_bridge
            .lock()
            .expect("lock hook bridge runtime for test")
            .workflow_root
            .clone();
        let result = handle_hook_bridge_websocket_text(
            request,
            &runtime.mcp_servers,
            &runtime.tool_registry,
            &runtime.workflow_store,
            &runtime.artloom_settings,
            &runtime.shared_images,
            &runtime.ocr_provider,
            &workflow_root,
        );
        serde_json::from_str(&result.response).expect("hook bridge json response")
    }

    fn expect_binary_route_response(
        response: RouteResponse,
        expected_status: u16,
        expected_content_type: &'static str,
    ) -> Vec<u8> {
        match response {
            RouteResponse::Binary {
                status,
                content_type,
                body,
            } => {
                assert_eq!(status, expected_status);
                assert_eq!(content_type, expected_content_type);
                body
            }
            RouteResponse::Text { .. } => {
                panic!("expected binary response with status {expected_status}")
            }
        }
    }

    #[test]
    fn resolve_hook_session_path_prefers_the_most_recently_written_identifier() {
        let appdata = unique_temp_dir("hook-session-resolution");

        // A stale legacy directory (the previously hardcoded default) plus the
        // directory Hook is actually writing under a newer identifier.
        let stale = write_hook_session(&appdata, "com.vmjcv.arthook-next", r#"{"stickers":[]}"#);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let current = write_hook_session(
            &appdata,
            "com.vmjcv.hook",
            r#"{"stickers":[{"id":"live","type":"sticker"}]}"#,
        );

        let resolved = resolve_hook_session_path(&appdata);

        assert_eq!(resolved, current);
        assert_ne!(resolved, stale);
    }

    #[test]
    fn resolve_hook_session_path_falls_back_to_default_when_no_session_exists() {
        let appdata = unique_temp_dir("hook-session-missing");

        let resolved = resolve_hook_session_path(&appdata);

        assert_eq!(
            resolved,
            appdata.join("com.vmjcv.arthook-next").join("session.json")
        );
    }

    fn start_daemon_with_store(
        path: &Path,
        brain_planner: BrainPlannerConfig,
    ) -> (u16, mpsc::Sender<()>, thread::JoinHandle<Result<()>>) {
        let daemon = LoomDaemon::bind(
            DaemonConfig::localhost(0)
                .with_sqlite_run_store(path)
                .with_brain_planner(brain_planner),
        )
        .expect("bind daemon with SQLite store");
        let port = daemon.local_addr().expect("local address").port();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
        (port, shutdown_tx, server)
    }

    #[derive(Debug)]
    struct FailingRunEvidenceStore;

    struct CountingRunEvidenceStore {
        inner: InMemoryRunEvidenceStore,
        insert_count: Arc<AtomicUsize>,
    }

    impl CountingRunEvidenceStore {
        fn new(insert_count: Arc<AtomicUsize>) -> Self {
            Self {
                inner: InMemoryRunEvidenceStore::default(),
                insert_count,
            }
        }
    }

    impl RunEvidenceStore for CountingRunEvidenceStore {
        fn insert_run(
            &mut self,
            run: Value,
            events: Vec<RunEventDraft>,
        ) -> loom_durable::RunStoreResult<()> {
            self.insert_count.fetch_add(1, Ordering::SeqCst);
            self.inner.insert_run(run, events)
        }

        fn transition_run(
            &mut self,
            run: Value,
            event: RunEventDraft,
        ) -> loom_durable::RunStoreResult<()> {
            self.inner.transition_run(run, event)
        }

        fn get_run(&self, run_id: &str) -> loom_durable::RunStoreResult<Option<Value>> {
            self.inner.get_run(run_id)
        }

        fn get_events(&self, run_id: &str) -> loom_durable::RunStoreResult<Option<Vec<Value>>> {
            self.inner.get_events(run_id)
        }

        fn recover_interrupted_runs(&mut self) -> loom_durable::RunStoreResult<usize> {
            self.inner.recover_interrupted_runs()
        }

        fn status(&self) -> RunStoreStatus {
            self.inner.status()
        }
    }

    impl RunEvidenceStore for FailingRunEvidenceStore {
        fn insert_run(
            &mut self,
            _run: Value,
            _events: Vec<RunEventDraft>,
        ) -> loom_durable::RunStoreResult<()> {
            Err(RunStoreError::Integrity("fixture failure".to_owned()))
        }

        fn transition_run(
            &mut self,
            _run: Value,
            _event: RunEventDraft,
        ) -> loom_durable::RunStoreResult<()> {
            Err(RunStoreError::Integrity("fixture failure".to_owned()))
        }

        fn get_run(&self, _run_id: &str) -> loom_durable::RunStoreResult<Option<Value>> {
            Err(RunStoreError::Integrity("fixture failure".to_owned()))
        }

        fn get_events(&self, _run_id: &str) -> loom_durable::RunStoreResult<Option<Vec<Value>>> {
            Err(RunStoreError::Integrity("fixture failure".to_owned()))
        }

        fn recover_interrupted_runs(&mut self) -> loom_durable::RunStoreResult<usize> {
            Err(RunStoreError::Integrity("fixture failure".to_owned()))
        }

        fn status(&self) -> RunStoreStatus {
            RunStoreStatus {
                mode: "memory",
                persistent: false,
            }
        }
    }

    #[test]
    fn daemon_help_and_version_are_available_without_binding_a_port() {
        let help = daemon_help_text();
        assert!(help.contains("Usage: loom-daemon"));
        assert!(help.contains("LOOM_DAEMON_HOST"));
        assert!(help.contains("LOOM_DAEMON_PORT"));
        assert!(help.contains("--manifest-dir"));
        assert!(help.contains("LOOM_CAPABILITY_MANIFEST_DIR"));
        assert!(help.contains("LOOM_RUN_STORE_PATH"));
        assert!(help.contains("LOOM_DAEMON_WORKERS"));
        assert!(help.contains("worker threads [default: 4]"));
        assert!(help.contains("LOOM_DAEMON_QUEUE_CAPACITY"));
        assert!(help.contains("Queued requests [default: 32]"));
        assert!(help.contains("/v1/invoke"));

        assert_eq!(
            daemon_version_text(),
            format!("loom-daemon {}", loom_core::LOOM_VERSION)
        );
    }

    #[test]
    fn json_http_responses_declare_utf8_charset() {
        let mut response = Vec::new();

        write_response(&mut response, 200, r#"{"name":"Hook 实时工作流"}"#)
            .expect("write response");
        let response = String::from_utf8(response).expect("utf8 response");

        assert!(response.contains("Content-Type: application/json; charset=utf-8"));
        assert!(response.contains(r#""name":"Hook 实时工作流""#));
    }

    #[test]
    fn daemon_serves_health_and_module_status_on_configured_isolated_port() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let health = http_get(address.port(), "/health");
        assert!(health.contains("200 OK"));
        assert!(health.contains("\"status\":\"ok\""));

        let status = http_get(address.port(), "/status");
        assert!(status.contains("200 OK"));
        assert!(status.contains("\"status\":\"ready\""));
        assert!(status.contains("\"name\":\"core\""));
        assert!(status.contains("\"name\":\"gateway\""));
        assert!(status.contains("\"name\":\"hooks\""));

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_reports_brain_planner_status_by_default() {
        let root = unique_temp_dir("status-brain-planner-default");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let status = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
            200,
        );

        assert_eq!(status["brain_planner"]["mode"], "local_template");
        assert_eq!(status["brain_planner"]["configured"], false);
        assert!(status["brain_planner"].get("model").is_none());
        assert!(status["brain_planner"].get("timeout_seconds").is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_reports_inline_request_executor_by_default() {
        let root = unique_temp_dir("status-inline-request-executor");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let status = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
            200,
        );
        assert_eq!(status["requestExecutor"]["mode"], "inline");
        assert_eq!(status["requestExecutor"]["workers"], 1);
        assert_eq!(status["requestExecutor"]["queueCapacity"], 0);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_reports_explicit_bounded_request_executor() {
        let root = unique_temp_dir("status-bounded-request-executor");
        let runtime = test_daemon_runtime_from_config(
            &root,
            DaemonConfig::localhost(0).with_bounded_request_executor(2, 3),
        );
        let status = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
            200,
        );
        assert_eq!(status["requestExecutor"]["mode"], "bounded_workers");
        assert_eq!(status["requestExecutor"]["workers"], 2);
        assert_eq!(status["requestExecutor"]["queueCapacity"], 3);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_runtime_remains_available_across_sequential_routes() {
        let daemon =
            LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(2, 4))
                .expect("bind daemon");
        let port = daemon.local_addr().expect("address").port();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

        assert_eq!(http_json_get(port, "/health")["status"], "ok");
        assert_eq!(http_json_get(port, "/status")["status"], "ready");
        assert!(http_json_get(port, "/v1/capabilities")["capabilities"].is_array());

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server").expect("serve");
    }

    #[test]
    fn daemon_serves_probes_while_brain_plan_is_blocked() {
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let mut daemon =
            LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(2, 4))
                .expect("bind daemon");
        Arc::get_mut(&mut daemon.runtime)
            .expect("exclusive daemon runtime")
            .brain_planner = Arc::new(BlockingBrainPlanner {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        });
        let port = daemon.local_addr().expect("address").port();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
        let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
        fixture.add_release_gate(Arc::clone(&release));

        let invoke_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"concurrent-plan","caller":"test","capability":"brain.plan","input":{"goal":"block planner"}}"#,
                ),
            )
        });
        assert!(
            wait_for_test_gate(&entered, Duration::from_millis(750)),
            "planner did not enter before the deadline"
        );

        let probes_rx = fixture.spawn_client(move || {
            let health = http_get(port, "/health");
            let status = http_get(port, "/status");
            (health, status)
        });
        let probes_before_release = probes_rx.recv_timeout(Duration::from_millis(750));
        let probes_returned_while_blocked = probes_before_release.is_ok();

        fixture.release_gates();
        let invoke = invoke_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("invoke response after release");
        let (health, status) = probes_before_release.unwrap_or_else(|_| {
            probes_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("probe responses after release")
        });
        fixture.finish().expect("serve");
        assert!(
            probes_returned_while_blocked,
            "health and status did not return while planning was blocked"
        );
        assert!(health.starts_with("HTTP/1.1 200 OK"));
        assert!(status.starts_with("HTTP/1.1 200 OK"));
        assert!(invoke.starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn daemon_runs_approved_capabilities_concurrently() {
        let entered = Arc::new((Mutex::new(0_usize), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let mut daemon =
            LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(2, 2))
                .expect("bind daemon");
        Arc::get_mut(&mut daemon.runtime)
            .expect("exclusive daemon runtime")
            .brain_planner = Arc::new(CountingBlockingBrainPlanner {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        });
        let port = daemon.local_addr().expect("address").port();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
        let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
        fixture.add_release_gate(Arc::clone(&release));

        let first_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"overlap-first","caller":"test","capability":"brain.plan","input":{"goal":"overlap first"}}"#,
                ),
            )
        });
        let second_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"overlap-second","caller":"test","capability":"brain.plan","input":{"goal":"overlap second"}}"#,
                ),
            )
        });

        let (entered_lock, entered_signal) = &*entered;
        let entered_count = entered_lock.lock().expect("read planner entries");
        let (entered_count, _) = entered_signal
            .wait_timeout_while(entered_count, Duration::from_millis(750), |count| {
                *count < 2
            })
            .expect("wait planner entries");
        let overlapped = *entered_count >= 2;
        drop(entered_count);

        fixture.release_gates();
        let first_response = first_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("first capability response");
        let second_response = second_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("second capability response");
        fixture.finish().expect("serve");
        assert!(overlapped, "approved capabilities did not overlap");
        assert!(first_response.starts_with("HTTP/1.1 200 OK"));
        assert!(second_response.starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn daemon_returns_busy_when_request_queue_is_full() {
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let submissions = RequestSubmissionObserver::new();
        let mut daemon =
            LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(1, 1))
                .expect("bind daemon");
        let inserted_runs = Arc::new(AtomicUsize::new(0));
        let runtime = Arc::get_mut(&mut daemon.runtime).expect("exclusive daemon runtime");
        runtime.brain_planner = Arc::new(BlockingBrainPlanner {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        });
        runtime.run_store = Arc::new(Mutex::new(Box::new(CountingRunEvidenceStore::new(
            Arc::clone(&inserted_runs),
        ))));
        runtime.request_submission_observer = Some(Arc::clone(&submissions));
        let port = daemon.local_addr().expect("address").port();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
        let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
        fixture.add_release_gate(Arc::clone(&release));

        let first_body =
            r#"{"requestId":"queue-first","caller":"test","capability":"brain.plan","input":{"goal":"queue first"}}"#
                .to_owned();
        let first_rx = fixture
            .spawn_client(move || http_request(port, "POST", "/v1/invoke", Some(&first_body)));
        assert!(
            wait_for_test_gate(&entered, Duration::from_millis(750)),
            "planner did not enter before the deadline"
        );

        let second_body =
            r#"{"requestId":"queue-second","caller":"test","capability":"brain.plan","input":{"goal":"queue second"}}"#
                .to_owned();
        let second_rx = fixture
            .spawn_client(move || http_request(port, "POST", "/v1/invoke", Some(&second_body)));
        let second_submitted = submissions.wait_for_count(2);

        let third_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"queue-third","caller":"test","capability":"brain.plan","input":{"goal":"queue third"}}"#,
                ),
            )
        });
        let third_response_before_release = third_rx.recv_timeout(Duration::from_millis(750)).ok();
        let third_returned_before_release = third_response_before_release.is_some();

        fixture.release_gates();
        let first_response = first_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("first client response");
        let second_response = second_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("second client response");
        let third_response = third_response_before_release.unwrap_or_else(|| {
            third_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("third response after release")
        });

        let health = http_get(port, "/health");
        fixture.finish().expect("serve");

        assert!(
            third_returned_before_release,
            "third request did not receive an overload response before release"
        );
        assert!(
            second_submitted,
            "second request was not submitted to the queue"
        );
        assert!(third_response.starts_with("HTTP/1.1 503 Service Unavailable"));
        let third_body = response_json_body(&third_response);
        assert_eq!(third_body["error"]["code"], "daemon_busy");
        assert_eq!(third_body["error"]["retryable"], true);
        assert!(!third_body.to_string().contains("queue-third"));
        assert!(first_response.starts_with("HTTP/1.1 200 OK"));
        assert!(second_response.starts_with("HTTP/1.1 200 OK"));
        assert!(!first_response.contains("queue-third"));
        assert!(!second_response.contains("queue-third"));
        assert_eq!(
            inserted_runs.load(Ordering::SeqCst),
            2,
            "overloaded request created run evidence"
        );
        assert!(health.starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn daemon_shutdown_drains_active_and_queued_requests() {
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let submissions = RequestSubmissionObserver::new();
        let shutdown_observer = DaemonShutdownObserver::new();
        let mut daemon =
            LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(1, 1))
                .expect("bind daemon");
        let runtime = Arc::get_mut(&mut daemon.runtime).expect("exclusive daemon runtime");
        runtime.brain_planner = Arc::new(BlockingBrainPlanner {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        });
        runtime.request_submission_observer = Some(Arc::clone(&submissions));
        runtime.shutdown_observer = Some(Arc::clone(&shutdown_observer));
        let port = daemon.local_addr().expect("address").port();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let (server_done_tx, server_done_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let result = daemon.serve_until(shutdown_rx);
            server_done_tx.send(()).expect("report server completion");
            result
        });
        let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
        fixture.add_release_gate(Arc::clone(&release));

        let first_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"drain-first","caller":"test","capability":"brain.plan","input":{"goal":"drain first"}}"#,
                ),
            )
        });
        assert!(
            wait_for_test_gate(&entered, Duration::from_millis(750)),
            "planner did not enter before the deadline"
        );

        let second_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"drain-second","caller":"test","capability":"brain.plan","input":{"goal":"drain second"}}"#,
                ),
            )
        });
        let second_submitted = submissions.wait_for_count(2);
        fixture.request_shutdown();
        assert!(
            shutdown_observer.wait_until_observed(Duration::from_secs(3)),
            "serve loop did not observe shutdown before the deadline"
        );
        let stopped_before_release = server_done_rx
            .recv_timeout(Duration::from_millis(250))
            .is_ok();

        fixture.release_gates();
        let first_response = first_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("first drained request");
        let second_response = second_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("second drained request");
        fixture.finish().expect("serve");
        assert!(
            !stopped_before_release,
            "daemon returned before active work was released"
        );
        assert!(
            second_submitted,
            "second request was not queued before shutdown"
        );
        assert!(first_response.starts_with("HTTP/1.1 200 OK"));
        assert!(second_response.starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn daemon_returns_shutting_down_for_request_accepted_before_shutdown() {
        let daemon =
            LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(1, 1))
                .expect("bind daemon");
        let port = daemon.local_addr().expect("address").port();
        let mut client = TcpStream::connect(("127.0.0.1", port)).expect("connect client");
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("set client timeout");
        let body = r#"{"requestId":"shutdown-race","caller":"test","capability":"brain.plan","input":{"goal":"shutdown race"}}"#;
        write!(
            client,
            "POST /v1/invoke HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("write partial request");

        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
        thread::sleep(Duration::from_millis(100));
        shutdown_tx.send(()).expect("request shutdown");
        client
            .write_all(body.as_bytes())
            .expect("complete request body");
        client
            .shutdown(Shutdown::Write)
            .expect("close client write side");

        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .expect("read shutdown response");
        server.join().expect("server thread").expect("serve daemon");

        assert!(response.starts_with("HTTP/1.1 503 Service Unavailable"));
        let body = response_json_body(&response);
        assert_eq!(body["error"]["code"], "daemon_shutting_down");
        assert_eq!(body["error"]["retryable"], true);
    }

    #[test]
    fn daemon_shutting_down_response_is_retryable_service_unavailable() {
        let (status, body) = daemon_shutting_down_response();
        assert_eq!(status, 503);
        let body: Value = serde_json::from_str(&body).expect("shutdown response json");
        assert_eq!(
            body,
            serde_json::json!({
                "error": {
                    "code": "daemon_shutting_down",
                    "message": "Loom daemon is shutting down",
                    "retryable": true,
                }
            })
        );

        let mut response = Vec::new();
        write_response(&mut response, status, &body.to_string()).expect("write shutdown response");
        let response = String::from_utf8(response).expect("utf8 response");
        assert!(response.starts_with("HTTP/1.1 503 Service Unavailable"));
    }

    #[test]
    fn serialized_routes_do_not_overlap_while_probes_remain_available() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("serialized-routes");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);

        let observer = SerializedRouteObserver::new();
        let mut daemon =
            LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(3, 4))
                .expect("bind daemon");
        Arc::get_mut(&mut daemon.runtime)
            .expect("exclusive daemon runtime")
            .serialized_route_observer = Some(Arc::clone(&observer));
        let port = daemon.local_addr().expect("address").port();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
        let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
        let observer_for_cleanup = Arc::clone(&observer);
        fixture.add_release_action(move || observer_for_cleanup.release());

        let first_rx = fixture.spawn_client(move || http_get(port, "/v1/workflows"));
        assert!(
            observer.wait_until_entered(Duration::from_millis(750)),
            "serialized route did not enter before the deadline"
        );
        let second_rx = fixture.spawn_client(move || http_get(port, "/v1/workflows"));

        let health = http_get(port, "/health");
        assert!(health.starts_with("HTTP/1.1 200 OK"));

        fixture.release_gates();
        let first_response = first_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("first serialized route");
        let second_response = second_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("second serialized route");
        fixture.finish().expect("serve");
        assert!(first_response.starts_with("HTTP/1.1 200 OK"));
        assert!(second_response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(observer.max_active(), 1);
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup serialized routes root");
    }

    #[test]
    fn serialized_route_observer_wait_is_bounded() {
        let observer = SerializedRouteObserver::new();
        assert!(!observer.wait_until_entered(Duration::from_millis(25)));
    }

    #[test]
    fn request_concurrency_classification_is_conservative() {
        let request = |method: &str, path: &str, capability: Option<&str>| ParsedHttpRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            headers: Vec::new(),
            body: capability
                .map(|capability| serde_json::json!({ "capability": capability }).to_string())
                .unwrap_or_default(),
        };

        let concurrent = [
            ("GET", "/health", None),
            ("GET", "/status", None),
            ("GET", "/v1/capabilities", None),
            ("GET", "/v1/hook-bridge/canvas", None),
            ("GET", "/v1/hook-bridge/canvas/nodes/capture/preview", None),
            ("GET", "/v1/runs/run-1", None),
            ("GET", "/v1/runs/run-1/events", None),
            ("POST", "/v1/invoke", Some("brain.plan")),
            ("POST", "/v1/invoke", Some("tea.ticket.decompose.v1")),
        ];
        for (method, path, capability) in concurrent {
            assert_eq!(
                request_concurrency_class(&request(method, path, capability)),
                RequestConcurrencyClass::Concurrent,
                "expected concurrent classification for {method} {path} {capability:?}"
            );
        }

        let serialized = [
            ("GET", "/v1/workflows", None),
            ("PUT", "/v1/workflows/workflow-1", None),
            ("POST", "/v1/tools/tool-1/execute", None),
            ("POST", "/v1/invoke", Some("future.capability")),
        ];
        for (method, path, capability) in serialized {
            assert_eq!(
                request_concurrency_class(&request(method, path, capability)),
                RequestConcurrencyClass::Serialized,
                "expected serialized classification for {method} {path} {capability:?}"
            );
        }

        let invalid_invoke = ParsedHttpRequest {
            method: "POST".to_owned(),
            path: "/v1/invoke".to_owned(),
            headers: Vec::new(),
            body: "not-json".to_owned(),
        };
        assert_eq!(
            request_concurrency_class(&invalid_invoke),
            RequestConcurrencyClass::Serialized
        );
    }

    #[test]
    fn daemon_reports_in_memory_run_store_by_default() {
        let root = unique_temp_dir("status-run-store-memory");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let status = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
            200,
        );

        assert_eq!(status["run_store"]["mode"], "memory");
        assert_eq!(status["run_store"]["persistent"], false);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_reports_explicit_sqlite_run_store() {
        let root = unique_temp_dir("sqlite-status");
        let path = root.join("runs").join("loom-runs.sqlite3");
        let runtime = test_daemon_runtime_from_config(
            &root,
            DaemonConfig::localhost(0).with_sqlite_run_store(&path),
        );
        let status = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
            200,
        );

        assert_eq!(status["run_store"]["mode"], "sqlite");
        assert_eq!(status["run_store"]["persistent"], true);
        assert!(status["run_store"].get("path").is_none());
        assert!(!status
            .to_string()
            .contains(&path.to_string_lossy().to_string()));
        drop(runtime);
        assert!(path.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_reports_configured_gateway_brain_planner_without_auth_token() {
        let config = DaemonConfig::localhost(0).with_brain_planner(
            brain_plan::BrainPlannerConfig::Gateway(brain_plan::GatewayPlannerConfig {
                base_url: "http://127.0.0.1:4200".to_owned(),
                auth_token: Some("do-not-expose".to_owned()),
                model: "test-model".to_owned(),
                timeout: Duration::from_secs(12),
            }),
        );
        let root = unique_temp_dir("status-gateway-brain-planner");
        let runtime = test_daemon_runtime_from_config(&root, config);
        let status = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
            200,
        );

        assert_eq!(status["brain_planner"]["mode"], "gateway");
        assert_eq!(status["brain_planner"]["configured"], true);
        assert_eq!(status["brain_planner"]["model"], "test-model");
        assert_eq!(status["brain_planner"]["timeout_seconds"], 12);
        assert!(status["brain_planner"].get("auth_token").is_none());
        assert!(!status.to_string().contains("do-not-expose"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_rejects_invalid_gateway_timeout_configuration() {
        let mut values = std::collections::HashMap::new();
        values.insert("LOOM_GATEWAY_MODEL".to_owned(), "test-model".to_owned());
        values.insert("LOOM_GATEWAY_TIMEOUT_SECS".to_owned(), "301".to_owned());

        let error = brain_plan::BrainPlannerConfig::from_lookup(|name| values.get(name).cloned())
            .expect_err("invalid timeout must be rejected");

        assert_eq!(
            error,
            brain_plan::BrainPlannerConfigError::TimeoutOutOfRange(301)
        );
    }

    #[test]
    fn daemon_ignores_empty_probe_before_serving_real_request() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        TcpStream::connect(("127.0.0.1", address.port())).expect("empty probe");
        let health = http_get(address.port(), "/health");

        assert!(health.contains("200 OK"));
        assert!(health.contains("\"status\":\"ok\""));

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn status_reports_hook_settings_summary() {
        let hook_settings = loom_hooks::HookSettings::enabled(vec![
            loom_hooks::HookRule::new(loom_hooks::HookEventKind::RunStarted)
                .with_target(loom_hooks::HookTarget::memory("runs")),
            loom_hooks::HookRule::new(loom_hooks::HookEventKind::RunStopped)
                .with_target(loom_hooks::HookTarget::memory("finished")),
        ]);
        let root = unique_temp_dir("status-hook-settings-summary");
        let runtime = test_daemon_runtime_from_config(
            &root,
            DaemonConfig::localhost(0).with_hook_settings(hook_settings),
        );
        let status = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
            200,
        );

        assert_eq!(status["hooks"]["enabled"], true);
        assert_eq!(status["hooks"]["ruleCount"], 2);
        assert_eq!(status["hooks"]["targetCount"], 2);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_serves_tea_run_contract() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let ticket_id = "11111111-1111-4111-8111-111111111111";
        let run_body = http_post(
            address.port(),
            "/v1/runs",
            &format!(
                r#"{{
                    "ticket": {{
                        "id": "{ticket_id}",
                        "title": "Tea integration smoke",
                        "description": "Run a Tea-created work order through Loom."
                    }}
                }}"#
            ),
        );
        assert!(run_body.contains("\"ticket_id\":\"11111111-1111-4111-8111-111111111111\""));
        assert!(run_body.contains("\"status\":\"succeeded\""));
        assert!(run_body.contains("\"loom_session_id\""));
        assert!(run_body.contains("loom daemon run completed"));

        let run: serde_json::Value = serde_json::from_str(&run_body).expect("run json");
        let run_id = run["id"].as_str().expect("run id");

        let stopped = http_post(
            address.port(),
            &format!("/v1/runs/{run_id}/stop"),
            &format!(r#"{{"run":{run_body}}}"#),
        );
        assert!(stopped.contains("\"status\":\"stopped\""));

        let retrying = http_post(
            address.port(),
            &format!("/v1/runs/{run_id}/retry"),
            &format!(r#"{{"run":{run_body}}}"#),
        );
        assert!(retrying.contains("\"status\":\"retrying\""));

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_lists_brain_plan_capability() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let response = http_json_get(address.port(), "/v1/capabilities");

        assert_eq!(response["capabilities"][0]["id"], "brain.plan");
        assert_eq!(response["capabilities"][0]["mode"], "run");
        assert!(response["capabilities"][0]["description"]
            .as_str()
            .expect("description")
            .contains("plan"));
        let capability_ids = response["capabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .map(|capability| capability["id"].as_str().expect("capability id").to_owned())
            .collect::<Vec<_>>();
        assert!(capability_ids.contains(&"brain.plan".to_owned()));
        assert!(capability_ids.contains(&"tea.ticket.decompose.v1".to_owned()));
        assert!(capability_ids.contains(&"tea.ticket.execute.v1".to_owned()));
        assert!(capability_ids.contains(&"tea.ticket.review.v1".to_owned()));

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_reads_and_writes_mcp_servers() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("control-plane-mcp");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let empty = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/mcp/servers", &[], None),
            ),
            200,
        );
        assert_eq!(empty["servers"], serde_json::json!([]));

        let saved = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/mcp/servers/brave",
                    &[],
                    Some(
                        r#"{
              "id": "brave",
              "name": "Brave Search",
              "command": "npx",
              "args": ["-y", "@brave/brave-search-mcp-server"],
              "env": { "BRAVE_API_KEY": "test-key" },
              "enabled": true
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved["server"]["id"], "brave");
        assert_eq!(saved["server"]["name"], "Brave Search");
        assert_eq!(saved["server"]["args"][1], "@brave/brave-search-mcp-server");

        let listed = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/mcp/servers", &[], None),
            ),
            200,
        );
        assert_eq!(listed["servers"].as_array().expect("servers").len(), 1);
        assert_eq!(listed["servers"][0]["id"], "brave");

        let deleted = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("DELETE", "/v1/mcp/servers/brave", &[], None),
            ),
            200,
        );
        assert_eq!(deleted["deleted"], true);
        let listed_after_delete = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/mcp/servers", &[], None),
            ),
            200,
        );
        assert_eq!(
            listed_after_delete["servers"]
                .as_array()
                .expect("servers")
                .len(),
            0
        );

        drop(runtime);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn daemon_persists_mcp_servers_across_runtime_reloads() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("control-plane-mcp-persist");
        let fixture = current_test_binary_mcp_fixture_config();

        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let saved = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/mcp/servers/fixture",
                    &[],
                    Some(&fixture.to_string()),
                ),
            ),
            200,
        );
        assert_eq!(saved["server"]["id"], "fixture");
        drop(runtime);

        let reloaded_runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let reloaded = expect_json_text_route_response(
            route_request(
                &reloaded_runtime,
                &parsed_request("GET", "/v1/mcp/servers", &[], None),
            ),
            200,
        );
        assert_eq!(
            reloaded["servers"].as_array().expect("servers").len(),
            1,
            "persisted MCP server should reload from disk"
        );
        assert_eq!(reloaded["servers"][0]["id"], "fixture");

        let deleted = expect_json_text_route_response(
            route_request(
                &reloaded_runtime,
                &parsed_request("DELETE", "/v1/mcp/servers/fixture", &[], None),
            ),
            200,
        );
        assert_eq!(deleted["deleted"], true);
        drop(reloaded_runtime);

        let deleted_runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let empty = expect_json_text_route_response(
            route_request(
                &deleted_runtime,
                &parsed_request("GET", "/v1/mcp/servers", &[], None),
            ),
            200,
        );
        assert_eq!(empty["servers"], serde_json::json!([]));

        drop(deleted_runtime);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normalize_mcp_server_config_rewrites_legacy_brave_github_source_to_npm_transport_stdio() {
        let normalized = normalize_mcp_server_config(McpServerConfig {
            id: "479008f0-bd4f-483e-8598-39fbae54a117".to_owned(),
            name: "Brave Search".to_owned(),
            description: "legacy brave fixture".to_owned(),
            command: "npx".to_owned(),
            args: vec![
                "-y".to_owned(),
                "github:brave/brave-search-mcp-server".to_owned(),
            ],
            env: BTreeMap::new(),
            enabled: true,
        });

        assert_eq!(
            normalized.args,
            vec![
                "-y".to_owned(),
                "@brave/brave-search-mcp-server".to_owned(),
                "--transport".to_owned(),
                "stdio".to_owned(),
            ]
        );
    }

    #[test]
    fn normalize_mcp_server_config_leaves_unrelated_servers_unchanged() {
        let server = McpServerConfig {
            id: "fixture".to_owned(),
            name: "Fixture".to_owned(),
            description: "test fixture".to_owned(),
            command: "python".to_owned(),
            args: vec!["server.py".to_owned()],
            env: BTreeMap::new(),
            enabled: true,
        };

        assert_eq!(normalize_mcp_server_config(server.clone()), server);
    }

    #[test]
    fn daemon_exposes_artloom_mcp_server_store_command_aliases() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let registry_fixture = McpRegistryFixture::start();
        let previous_registry_endpoint = std::env::var("LOOM_MCP_REGISTRY_ENDPOINT").ok();
        let root = unique_temp_dir("artloom-mcp-server-store");
        std::env::set_var(
            "LOOM_MCP_REGISTRY_ENDPOINT",
            registry_fixture.url("/v0/servers"),
        );
        let config = DaemonConfig::localhost(0);
        restore_env("LOOM_MCP_REGISTRY_ENDPOINT", previous_registry_endpoint);
        let runtime = test_daemon_runtime_from_config(&root, config);

        let empty = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/mcp/servers", &[], None),
            ),
            200,
        );
        assert_eq!(empty["compatCommand"], "get_mcp_servers");
        assert_eq!(empty["servers"], serde_json::json!([]));

        let saved = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/mcp/servers",
                    &[],
                    Some(
                        r#"{
              "id": "compat-mcp",
              "name": "Compat MCP",
              "description": "Old ArtLoom MCP server store fixture",
              "command": "powershell.exe",
              "args": ["-NoProfile"],
              "env": { "COMPAT": "1" },
              "enabled": true
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved["compatCommand"], "save_mcp_server");
        assert_eq!(saved["message"], "Saved successfully");
        assert_eq!(saved["server"]["id"], "compat-mcp");

        let listed = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/mcp/servers", &[], None),
            ),
            200,
        );
        assert_eq!(listed["compatCommand"], "get_mcp_servers");
        assert_eq!(listed["servers"][0]["id"], "compat-mcp");

        let registry = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "GET",
                    "/v1/artloom-compat/mcp/registry?search=fixture&limit=250&cursor=cursor-1",
                    &[],
                    None,
                ),
            ),
            200,
        );
        assert_eq!(registry["compatCommand"], "fetch_mcp_registry");
        assert_eq!(
            registry["servers"][0]["server"]["name"],
            "io.modelcontextprotocol/fixture"
        );
        assert!(registry_fixture.request_path().contains("limit=100"));

        let deleted = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "DELETE",
                    "/v1/artloom-compat/mcp/servers/compat-mcp",
                    &[],
                    None,
                ),
            ),
            200,
        );
        assert_eq!(deleted["compatCommand"], "delete_mcp_server");
        assert_eq!(deleted["message"], "Deleted successfully");
        assert_eq!(deleted["deleted"], true);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup ArtLoom MCP server store root");
    }

    #[test]
    fn daemon_exposes_mcp_registry_and_connection_test_contracts() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let registry_fixture = McpRegistryFixture::start();
        let previous_registry_endpoint = std::env::var("LOOM_MCP_REGISTRY_ENDPOINT").ok();
        std::env::set_var(
            "LOOM_MCP_REGISTRY_ENDPOINT",
            registry_fixture.url("/v0/servers"),
        );
        let root = unique_temp_dir("mcp-registry-contracts");
        let config = DaemonConfig::localhost(0);
        restore_env("LOOM_MCP_REGISTRY_ENDPOINT", previous_registry_endpoint);
        let runtime = test_daemon_runtime_from_config(&root, config);

        let registry = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "GET",
                    "/v1/mcp/registry?search=fixture&limit=250&cursor=cursor-1",
                    &[],
                    None,
                ),
            ),
            200,
        );
        assert_eq!(
            registry["servers"][0]["server"]["name"],
            "io.modelcontextprotocol/fixture"
        );
        assert!(registry_fixture.request_path().contains("limit=100"));
        assert!(registry_fixture.request_path().contains("search=fixture"));
        assert!(registry_fixture.request_path().contains("cursor=cursor-1"));

        let request_body = current_test_binary_mcp_fixture_config().to_string();
        let test_result = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("POST", "/v1/mcp/test", &[], Some(&request_body)),
            ),
            200,
        );
        assert_eq!(test_result["success"], true);
        assert_eq!(test_result["compatCommand"], "test_mcp_connection");
        assert_eq!(test_result["tools"][0]["name"], "echo");
        assert_eq!(
            test_result["server_info"]["serverInfo"]["name"],
            "daemon-fixture"
        );

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup mcp registry root");
    }

    #[test]
    fn daemon_exposes_safe_mcp_package_compatibility_contracts() {
        let install_plan = build_mcp_package_install_plan(r#"{"packageName":"mcp-server-demo"}"#)
            .expect("install plan");
        assert_eq!(install_plan.0, 200);
        let install_plan =
            serde_json::from_str::<Value>(&install_plan.1).expect("install plan json");
        assert_eq!(install_plan["compatCommand"], "install_mcp_package");
        assert_eq!(install_plan["package"], "mcp-server-demo");
        assert_eq!(install_plan["sideEffect"], false);
        assert_eq!(install_plan["command"][1], "-m");
        assert_eq!(install_plan["command"][2], "pip");

        let rejected = build_mcp_package_install_plan(r#"{"packageName":"demo;rm"}"#)
            .expect("invalid plan response");
        assert_eq!(rejected.0, 400);

        let check = check_mcp_package_installed(r#"{"moduleName":"json"}"#).expect("check module");
        assert_eq!(check.0, 200);
        let check = serde_json::from_str::<Value>(&check.1).expect("check json");
        assert_eq!(check["compatCommand"], "check_mcp_package_installed");
        assert_eq!(check["module"], "json");
    }

    #[test]
    fn daemon_exposes_python_art_source_import_helpers() {
        let root = unique_temp_dir("python-art-source");
        let art_dir = root.join("Art_SourceFixture");
        fs::create_dir_all(&art_dir).expect("create source art dir");
        let python_path = art_dir.join("main.py");
        let art_json_path = art_dir.join("art.json");
        fs::write(
            &python_path,
            r#"
def run(args):
    input_image = args.get("input_image")
    strength = args["strength"]
    return {"result_path": input_image, "confidence": strength}
"#,
        )
        .expect("write python source fixture");
        fs::write(
            &art_json_path,
            r#"{
  "art_id": "source_fixture",
  "label": "Source Fixture",
  "description": "Nearby art.json fixture",
  "signature": {
    "inputs": [{"id": "input_image", "label": "Input image", "type": "Image"}],
    "outputs": [{"id": "result_path", "label": "Result path", "type": "Image"}]
  },
  "variables": [{"id": "strength", "label": "Strength", "widget": "slider", "default": 0.75}]
}"#,
        )
        .expect("write art json fixture");

        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let source = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/python-arts/source/read",
                    &[],
                    Some(&serde_json::json!({ "path": python_path }).to_string()),
                ),
            ),
            200,
        );
        assert_eq!(
            canonical_test_path(source["path"].as_str().expect("source path")),
            canonical_test_path(&python_path)
        );
        assert!(source["content"]
            .as_str()
            .expect("source content")
            .contains("args.get(\"input_image\")"));

        let nearby = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/python-arts/source/check-art-json",
                    &[],
                    Some(&serde_json::json!({ "pythonPath": python_path }).to_string()),
                ),
            ),
            200,
        );
        assert_eq!(nearby["found"], true);
        assert_eq!(nearby["artJson"]["label"], "Source Fixture");
        assert_eq!(
            canonical_test_path(
                nearby["artJsonPath"]
                    .as_str()
                    .expect("nearby art json path")
            ),
            canonical_test_path(&art_json_path)
        );

        let art_json = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/python-arts/source/read-art-json",
                    &[],
                    Some(&serde_json::json!({ "artPath": art_dir }).to_string()),
                ),
            ),
            200,
        );
        assert_eq!(art_json["artJson"]["art_id"], "source_fixture");

        let inferred = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/python-arts/source/infer-ports",
                    &[],
                    Some(&serde_json::json!({ "path": python_path }).to_string()),
                ),
            ),
            200,
        );
        assert_eq!(inferred["inputs"][0]["name"], "input_image");
        assert_eq!(inferred["inputs"][0]["execution_type"], "image_path");
        assert_eq!(inferred["inputs"][1]["name"], "strength");
        assert_eq!(inferred["inputs"][1]["execution_type"], "number");
        assert_eq!(inferred["outputs"][0]["name"], "result_path");
        assert_eq!(inferred["outputs"][0]["execution_type"], "image_path");
        assert_eq!(inferred["outputs"][1]["name"], "confidence");
        assert_eq!(inferred["outputs"][1]["execution_type"], "string");

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup source fixture root");
    }

    #[test]
    fn daemon_exposes_artloom_python_source_command_aliases() {
        let root = unique_temp_dir("artloom-python-source-aliases");
        let art_json_path = root.join("art.json");
        let source_path = root.join("fixture.py");
        fs::write(
            &art_json_path,
            r#"{"art_id":"fixture_source_alias","label":"Fixture Source Alias"}"#,
        )
        .expect("write art json");
        fs::write(&source_path, "def main(args):\n    return args\n").expect("write python source");

        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let installed = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/python/installed-arts", &[], None),
            ),
            200,
        );
        assert_eq!(installed["compatCommand"], "list_installed_arts");
        assert!(installed["arts"].as_array().is_some());

        let read_source = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/python/read-python-file",
                    &[],
                    Some(
                        &serde_json::json!({
                            "filePath": source_path,
                        })
                        .to_string(),
                    ),
                ),
            ),
            200,
        );
        assert_eq!(read_source["compatCommand"], "read_python_file");
        assert!(read_source["content"]
            .as_str()
            .expect("source content")
            .contains("def main"));

        let nearby = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/python/check-art-json-nearby",
                    &[],
                    Some(
                        &serde_json::json!({
                            "pythonPath": source_path,
                        })
                        .to_string(),
                    ),
                ),
            ),
            200,
        );
        assert_eq!(nearby["compatCommand"], "check_art_json_nearby");
        assert_eq!(nearby["found"], true);
        assert_eq!(nearby["artJson"]["art_id"], "fixture_source_alias");

        let art_json = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/python/read-art-json",
                    &[],
                    Some(
                        &serde_json::json!({
                            "artPath": root,
                        })
                        .to_string(),
                    ),
                ),
            ),
            200,
        );
        assert_eq!(art_json["compatCommand"], "read_art_json");
        assert_eq!(art_json["artJson"]["label"], "Fixture Source Alias");

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup python source aliases root");
    }

    #[test]
    fn daemon_exposes_artloom_execute_python_art_command_alias() {
        let root = unique_temp_dir("artloom-execute-python-art");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let response = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/python/execute-art",
                    &[],
                    Some(r#"{"artId":"loom_echo","params":{"text":"direct compat"}}"#),
                ),
            ),
            200,
        );

        assert_eq!(response["compatCommand"], "execute_python_art");
        assert_eq!(response["status"], 200, "response={response}");
        assert_eq!(
            response["data"]["content"][0]["text"],
            "python art saw direct compat"
        );
        assert!(response["request_id"].as_str().is_some());

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup python art root");
    }

    #[test]
    fn daemon_exposes_artloom_python_process_image_command_alias() {
        let root = unique_temp_dir("python-process-image");
        let art_dir = root.join("Art_CopyImage");
        fs::create_dir_all(&art_dir).expect("create python image art dir");
        fs::write(
            art_dir.join("main.py"),
            r#"
import shutil

def main(args):
    shutil.copyfile(args["input_path"], args["output_path"])
    return {"copied": True}
"#,
        )
        .expect("write python image art");

        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let response = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/python/process-image",
                    &[],
                    Some(
                        &serde_json::json!({
                            "artId": "copy_image",
                            "artPath": art_dir,
                            "inputBase64": test_png_base64(),
                            "params": {}
                        })
                        .to_string(),
                    ),
                ),
            ),
            200,
        );

        assert_eq!(response["compatCommand"], "python_process_image");
        assert_eq!(response["success"], true, "response={response}");
        assert!(response["output_base64"]
            .as_str()
            .expect("python process image output")
            .starts_with("data:image/png;base64,"));
        assert!(response["output_path"].as_str().is_some());
        assert!(response["processing_time_ms"].as_u64().is_some());
        assert!(response["error"].is_null());

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup python image art root");
    }

    #[test]
    fn unwrap_prefetch_shader_payload_promotes_text_wrapped_shader_json() {
        let wrapped = serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "{\"type\":\"shader\",\"vertex_shader\":\"void main(){}\",\"fragment_shader\":\"void main(){ }\",\"uniforms\":{\"strength\":42.0},\"textures\":{\"lut\":\"data:image/png;base64,AAAA\"}}"
                }
            ]
        });

        let unwrapped = unwrap_prefetch_shader_payload(wrapped);

        assert_eq!(unwrapped["type"], "shader");
        assert_eq!(unwrapped["vertex_shader"], "void main(){}");
        assert_eq!(unwrapped["fragment_shader"], "void main(){ }");
        assert_eq!(unwrapped["uniforms"]["strength"], 42.0);
        assert_eq!(unwrapped["textures"]["lut"], "data:image/png;base64,AAAA");
        assert!(unwrapped.get("content").is_none());
    }

    #[test]
    fn daemon_recovers_trailing_tool_registry_data_before_listing_tools() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("control-plane-trailing-tools");
        let tools_root = root.join("tools");
        fs::create_dir_all(&tools_root).expect("create tool registry root");
        fs::write(
            tools_root.join("tools.json"),
            r#"[
              {
                "id": "recovered-tool",
                "name": "Recovered Tool",
                "description": "Recover a valid tool array with trailing delimiters",
                "enabled": true,
                "execution": {
                  "type": "cli_wrapper",
                  "command": "echo",
                  "args": ["ok"]
                }
              }
            ]  }
              }
            ]"#,
        )
        .expect("write corrupted tool registry");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let body = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/v1/tools", &[], None)),
            200,
        );
        assert_eq!(body["tools"][0]["id"], "recovered-tool");

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup recovered registry root");
    }

    #[test]
    fn daemon_reads_and_writes_tool_and_workflow_contracts() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("control-plane-tools-workflows");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/paint-flow",
                    &[],
                    Some(
                        r#"{
              "id": "paint-flow",
              "name": "Paint Flow",
              "description": "Run a saved workflow",
              "enabled": true,
              "execution": { "type": "workflow", "workflowId": "wf-1" }
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["tool"]["id"], "paint-flow");
        assert_eq!(saved_tool["tool"]["execution"]["type"], "workflow");

        let tools = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/v1/tools", &[], None)),
            200,
        );
        assert_eq!(tools["tools"].as_array().expect("tools").len(), 1);
        assert_eq!(tools["tools"][0]["id"], "paint-flow");

        let saved_workflow = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/workflows/wf-1",
                    &[],
                    Some(
                        r#"{"data":"name: Paint Flow\nnodes:\n  - id: prompt\n    uses: text.prompt\n"}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_workflow["workflow"]["id"], "wf-1");
        assert_eq!(saved_workflow["workflow"]["name"], "Paint Flow");
        assert_eq!(saved_workflow["workflow"]["nodeCount"], 1);

        let workflows = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/v1/workflows", &[], None)),
            200,
        );
        assert_eq!(
            workflows["workflows"].as_array().expect("workflows").len(),
            1
        );
        assert_eq!(workflows["workflows"][0]["id"], "wf-1");

        let loaded_workflow = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/workflows/wf-1", &[], None),
            ),
            200,
        );
        assert_eq!(loaded_workflow["workflow"]["id"], "wf-1");
        assert!(loaded_workflow["workflow"]["data"]
            .as_str()
            .expect("workflow data")
            .contains("name: Paint Flow"));

        let deleted_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("DELETE", "/v1/tools/paint-flow", &[], None),
            ),
            200,
        );
        assert_eq!(deleted_tool["deleted"], true);
        let tools_after_delete = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/v1/tools", &[], None)),
            200,
        );
        assert_eq!(
            tools_after_delete["tools"].as_array().expect("tools").len(),
            0
        );

        let deleted_workflow = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("DELETE", "/v1/workflows/wf-1", &[], None),
            ),
            200,
        );
        assert_eq!(deleted_workflow["deleted"], true);
        let workflows_after_delete = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/v1/workflows", &[], None)),
            200,
        );
        assert_eq!(
            workflows_after_delete["workflows"]
                .as_array()
                .expect("workflows")
                .len(),
            0
        );

        drop(runtime);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn daemon_exposes_artloom_workflow_store_command_aliases() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("artloom-workflow-store");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let saved_metadata = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/artloom-compat/workflows/compat-workflow/metadata",
                    &[],
                    Some(
                        r#"{
              "id": "compat-workflow",
              "name": "Compat Workflow",
              "description": "Old metadata path",
              "created_at": "1",
              "updated_at": "",
              "status": "draft",
              "node_count": 0,
              "tags": ["compat"]
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_metadata["compatCommand"], "save_workflow_metadata");
        assert_eq!(saved_metadata["workflow"]["id"], "compat-workflow");
        assert_eq!(saved_metadata["workflow"]["name"], "Compat Workflow");
        assert_eq!(
            saved_metadata["workflow"]["description"],
            "Old metadata path"
        );
        assert_eq!(saved_metadata["workflow"]["status"], "draft");
        assert_eq!(saved_metadata["workflow"]["tags"][0], "compat");

        let saved_data = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/artloom-compat/workflows/compat-workflow/data",
                    &[],
                    Some(
                        r#"{"data":"name: Compat Workflow\nnodes:\n  - id: prompt\n    uses: text.prompt\n"}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_data["compatCommand"], "save_workflow_data");
        assert_eq!(saved_data["workflowId"], "compat-workflow");
        assert_eq!(saved_data["saved"], true);

        let listed = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/workflows", &[], None),
            ),
            200,
        );
        assert_eq!(listed["compatCommand"], "list_workflows");
        assert_eq!(listed["workflows"][0]["id"], "compat-workflow");
        assert_eq!(listed["workflows"][0]["name"], "Compat Workflow");
        assert_eq!(listed["workflows"][0]["node_count"], 1);
        assert_eq!(listed["workflows"][0]["status"], "draft");

        let loaded_data = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "GET",
                    "/v1/artloom-compat/workflows/compat-workflow/data",
                    &[],
                    None,
                ),
            ),
            200,
        );
        assert_eq!(loaded_data["compatCommand"], "load_workflow_data");
        assert_eq!(loaded_data["workflowId"], "compat-workflow");
        assert!(loaded_data["data"]
            .as_str()
            .expect("loaded workflow data")
            .contains("uses: text.prompt"));

        let deleted = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "DELETE",
                    "/v1/artloom-compat/workflows/compat-workflow/data",
                    &[],
                    None,
                ),
            ),
            200,
        );
        assert_eq!(deleted["compatCommand"], "delete_workflow_data");
        assert_eq!(deleted["workflowId"], "compat-workflow");
        assert_eq!(deleted["deleted"], true);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup ArtLoom workflow store root");
    }

    #[test]
    fn daemon_executes_mcp_backed_tool_contract() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("mcp-backed-tool");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let fixture = current_test_binary_mcp_fixture_config();

        let saved_server = http_json_put(
            address.port(),
            "/v1/mcp/servers/fixture",
            &fixture.to_string(),
        );
        assert_eq!(saved_server["server"]["id"], "fixture");

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-echo",
            r#"{
              "id": "fixture-echo",
              "name": "Fixture Echo",
              "description": "Execute fixture MCP echo",
              "enabled": true,
              "execution": {
                "type": "mcp",
                "serverId": "fixture",
                "toolName": "echo"
              }
            }"#,
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-echo");
        assert_eq!(saved_tool["tool"]["execution"]["type"], "mcp");

        let executed = http_json_post(
            address.port(),
            "/v1/tools/fixture-echo/execute",
            r#"{"arguments":{"text":"hello daemon"}}"#,
        );
        assert_eq!(executed["toolId"], "fixture-echo");
        assert_eq!(executed["status"], "succeeded");
        assert_eq!(executed["result"]["content"][0]["type"], "text");
        assert_eq!(executed["result"]["content"][0]["text"], "hello daemon");

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup mcp-backed tool root");
    }

    #[test]
    fn daemon_exposes_artloom_call_mcp_tool_command_alias() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("artloom-call-mcp-tool");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let fixture = current_test_binary_mcp_fixture_config();

        let response = http_json_post(
            address.port(),
            "/v1/artloom-compat/mcp/call-tool",
            &serde_json::json!({
                "command": fixture["command"].as_str().expect("fixture command"),
                "args": fixture["args"].clone(),
                "env": fixture["env"].clone(),
                "toolName": "echo",
                "toolArgs": {
                    "text": "direct mcp compat"
                }
            })
            .to_string(),
        );

        assert_eq!(response["compatCommand"], "call_mcp_tool");
        assert_eq!(response["status"], "succeeded");
        assert_eq!(response["result"]["content"][0]["type"], "text");
        assert_eq!(
            response["result"]["content"][0]["text"],
            "direct mcp compat"
        );

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup ArtLoom call_mcp_tool root");
    }

    #[test]
    fn daemon_executes_script_backed_tool_contract() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("script-tool");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = write_daemon_script_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-script",
            &serde_json::json!({
                "id": "fixture-script",
                "name": "Fixture Script",
                "description": "Execute fixture script",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-script");
        assert_eq!(saved_tool["tool"]["execution"]["type"], "script");

        let executed = http_json_post(
            address.port(),
            "/v1/tools/fixture-script/execute",
            r#"{"arguments":{"text":"hello script daemon"}}"#,
        );
        assert_eq!(executed["toolId"], "fixture-script");
        assert_eq!(executed["status"], "succeeded");
        assert_eq!(executed["result"]["content"][0]["type"], "text");
        assert_eq!(
            executed["result"]["content"][0]["text"],
            "script saw hello script daemon"
        );

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup script tool root");
    }

    #[test]
    fn daemon_executes_workflow_backed_tool_contract() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("workflow-tool");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = write_daemon_script_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_script = http_json_put(
            address.port(),
            "/v1/tools/fixture-script",
            &serde_json::json!({
                "id": "fixture-script",
                "name": "Fixture Script",
                "description": "Execute fixture script",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_script["tool"]["id"], "fixture-script");

        let workflow_yaml = r#"name: Runtime Flow
nodes:
  - id: prompt
    uses: fixture-script
    with:
      text: hello workflow daemon
"#;
        let saved_workflow = http_json_put(
            address.port(),
            "/v1/workflows/runtime-flow",
            &serde_json::json!({ "data": workflow_yaml }).to_string(),
        );
        assert_eq!(saved_workflow["workflow"]["id"], "runtime-flow");

        let saved_workflow_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-workflow",
            r#"{
              "id": "fixture-workflow",
              "name": "Fixture Workflow",
              "description": "Execute fixture workflow",
              "enabled": true,
              "execution": {
                "type": "workflow",
                "workflowId": "runtime-flow"
              }
            }"#,
        );
        assert_eq!(saved_workflow_tool["tool"]["id"], "fixture-workflow");

        let executed = http_json_post(
            address.port(),
            "/v1/tools/fixture-workflow/execute",
            r#"{"arguments":{}}"#,
        );
        assert_eq!(executed["toolId"], "fixture-workflow");
        assert_eq!(executed["status"], "succeeded");
        assert_eq!(executed["result"]["content"][0]["type"], "text");
        assert_eq!(
            executed["result"]["content"][0]["text"],
            "script saw hello workflow daemon"
        );

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup workflow tool root");
    }

    #[test]
    fn daemon_executes_cloud_api_backed_tool_contract() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("cloud-tool");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let fixture = CloudApiFixture::start(CloudApiFixtureMode::Text);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-cloud",
            &serde_json::json!({
                "id": "fixture-cloud",
                "name": "Fixture Cloud",
                "description": "Execute fixture cloud API",
                "enabled": true,
                "execution": {
                    "type": "cloud_api",
                    "endpoint": fixture.url("/text"),
                    "method": "POST"
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-cloud");
        assert_eq!(saved_tool["tool"]["execution"]["type"], "cloud_api");

        let executed = http_json_post(
            address.port(),
            "/v1/tools/fixture-cloud/execute",
            r#"{"arguments":{"prompt":"hello daemon cloud"}}"#,
        );
        assert_eq!(executed["toolId"], "fixture-cloud");
        assert_eq!(executed["status"], "succeeded");
        assert_eq!(executed["result"]["content"][0]["type"], "text");
        assert_eq!(
            executed["result"]["content"][0]["text"],
            "cloud saw hello daemon cloud"
        );

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup cloud tool root");
    }

    #[test]
    fn daemon_tool_readiness_reports_downloaded_python_art_runtime() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("python-art-readiness");
        mark_framework_installed(&root, "python_art");
        provision_test_python_art_runtime(&root);
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/fixture-python-readiness",
                    &[],
                    Some(
                        r#"{
              "id": "fixture-python-readiness",
              "name": "Fixture Python Readiness",
              "description": "Report framework readiness from a downloaded runtime",
              "enabled": true,
              "execution": {
                "type": "python_art",
                "artId": "fixture_python_art",
                "artPath": "python/Arts/FixturePythonReadiness"
              }
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-python-readiness");

        let readiness = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "GET",
                    "/v1/tools/fixture-python-readiness/readiness",
                    &[],
                    None,
                ),
            ),
            200,
        );
        assert_eq!(readiness["toolId"], "fixture-python-readiness");
        assert_eq!(readiness["framework"], "python_art");
        assert_eq!(readiness["frameworkInstalled"], true);
        assert_eq!(readiness["ready"], true, "response={readiness}");
        let detail = readiness["detail"]
            .as_str()
            .expect("python_art readiness detail")
            .replace('\\', "/");
        assert!(detail.contains("python_art"), "response={readiness}");

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup python art readiness root");
    }

    #[test]
    fn daemon_reports_hook_bridge_status_contract() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let appdata_root = unique_temp_dir("empty-arthook-appdata");
        let control_plane_root = unique_temp_dir("hook-bridge-status-control-plane");
        let previous_appdata = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata_root);

        let runtime =
            test_daemon_runtime_from_config(&control_plane_root, DaemonConfig::localhost(0));

        let status = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/hook-bridge/status", &[], None),
            ),
            200,
        );
        assert_eq!(status["running"], false);
        assert_eq!(status["port"], 19820);
        assert_eq!(status["connectedClients"], 0);
        assert!(status["methods"].as_array().expect("methods").contains(
            &serde_json::Value::String("art_loom/update_workflow_node".to_owned())
        ));
        assert!(status["methods"].as_array().expect("methods").contains(
            &serde_json::Value::String("art_hook/instantiate".to_owned())
        ));
        assert!(status["methods"].as_array().expect("methods").contains(
            &serde_json::Value::String("read_arthook_session".to_owned())
        ));
        assert_eq!(status["sessionMethod"], "read_arthook_session");

        let session = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/hook-bridge/session", &[], None),
            ),
            200,
        );
        assert_eq!(session["method"], "read_arthook_session");
        assert_eq!(
            session["session"]["stickers"]
                .as_array()
                .expect("stickers")
                .len(),
            0
        );
        assert_eq!(
            session["session"]["links"].as_array().expect("links").len(),
            0
        );

        restore_env("APPDATA", previous_appdata);
        fs::remove_dir_all(appdata_root).expect("cleanup appdata root");
        fs::remove_dir_all(control_plane_root).expect("cleanup control plane");
    }

    #[test]
    fn daemon_exposes_hook_canvas_snapshot_contract() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_hook_canvas_runtime_state();
        let appdata = unique_temp_dir("hook-canvas-appdata");
        let session_dir = appdata.join("com.vmjcv.arthook-next");
        let images = session_dir.join("images");
        fs::create_dir_all(&images).expect("create session images");
        fs::write(
            session_dir.join("session.json"),
            r#"{"stickers":[{"id":"capture node","type":"sticker","src":"images/capture.png","x":20,"y":30,"w":320,"h":180}],"links":[]}"#,
        )
        .expect("write Hook session");
        fs::write(images.join("capture.png"), test_png_bytes()).expect("write preview");
        let previous = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);

        let (status, body) = hook_canvas_snapshot().expect("hook canvas snapshot");
        assert_eq!(status, 200);
        let canvas = serde_json::from_str::<serde_json::Value>(&body).expect("snapshot json");
        assert_eq!(canvas["available"], true);
        assert_eq!(canvas["nodes"][0]["id"], "capture node");
        assert_eq!(canvas["nodes"][0]["kind"], "screenshot");
        let preview_url = canvas["nodes"][0]["previewUrl"]
            .as_str()
            .expect("preview url string");
        assert!(
            preview_url.starts_with("/v1/hook-bridge/canvas/nodes/capture%20node/preview?v="),
            "unexpected preview url: {preview_url}"
        );
        assert!(daemon_help_text().contains("GET  /v1/hook-bridge/canvas"));
        assert!(daemon_help_text().contains("GET  /v1/hook-bridge/canvas/nodes/{nodeId}/preview"));
        restore_env("APPDATA", previous);
        clear_hook_canvas_runtime_state();
        fs::remove_dir_all(appdata).expect("cleanup");
    }

    #[test]
    fn daemon_hook_canvas_prefers_live_hook_workflow_snapshot_when_available() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_hook_canvas_runtime_state();
        let root = unique_temp_dir("hook-canvas-live-workflow");
        let appdata = unique_temp_dir("hook-canvas-live-workflow-appdata");
        let previous_appdata = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let workflow = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": "art_loom/overwrite_workflow",
                "params": {
                    "workflow_id": HOOK_LIVE_WORKFLOW_ID,
                    "snapshot": {
                        "name": "Hook Live",
                        "nodes": [
                            {
                                "id": "capture",
                                "type": "screenshot",
                                "position": { "x": 20, "y": 30 },
                                "measured": { "width": 80, "height": 80 },
                                "data": {
                                    "src": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
                                    "w": 80,
                                    "h": 80
                                }
                            },
                            {
                                "id": "missing-art-node",
                                "type": "art",
                                "position": { "x": 160, "y": 40 },
                                "measured": { "width": 90, "height": 90 },
                                "data": {
                                    "artId": "missing-art",
                                    "previewSrc": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
                                    "params": { "strength": 61 },
                                    "w": 90,
                                    "h": 90
                                }
                            }
                        ],
                        "edges": [
                            {
                                "id": "edge-1",
                                "source": "capture",
                                "target": "missing-art-node",
                                "sourceHandle": "output",
                                "targetHandle": "input"
                            }
                        ]
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(workflow["type"], "success", "response={workflow}");

        let (status, body) = hook_canvas_snapshot().expect("hook canvas snapshot");
        assert_eq!(status, 200);
        let canvas = serde_json::from_str::<serde_json::Value>(&body).expect("snapshot json");
        assert_eq!(canvas["available"], true);
        assert_eq!(canvas["workflowId"], HOOK_LIVE_WORKFLOW_ID);
        assert_eq!(canvas["nodes"].as_array().expect("nodes").len(), 2);
        assert_eq!(canvas["nodes"][1]["id"], "missing-art-node");
        assert_eq!(canvas["nodes"][1]["kind"], "art");
        assert_eq!(canvas["nodes"][1]["status"], "ready");
        assert_eq!(canvas["nodes"][1]["previewAvailable"], true);

        restore_env("APPDATA", previous_appdata);
        clear_hook_canvas_runtime_state();
        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(appdata).expect("cleanup appdata");
    }

    #[test]
    fn daemon_hook_canvas_marks_live_art_node_error_after_ahrp_failure() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_hook_canvas_runtime_state();
        let root = unique_temp_dir("hook-canvas-live-error");
        let appdata = unique_temp_dir("hook-canvas-live-error-appdata");
        let previous_appdata = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let overwrite = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": "art_loom/overwrite_workflow",
                "params": {
                    "workflow_id": HOOK_LIVE_WORKFLOW_ID,
                    "snapshot": {
                        "name": "Hook Live",
                        "nodes": [
                            {
                                "id": "capture",
                                "type": "screenshot",
                                "position": { "x": 20, "y": 30 },
                                "measured": { "width": 80, "height": 80 },
                                "data": {
                                    "src": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
                                    "w": 80,
                                    "h": 80
                                }
                            },
                            {
                                "id": "missing-art-node",
                                "type": "art",
                                "position": { "x": 160, "y": 40 },
                                "measured": { "width": 90, "height": 90 },
                                "data": {
                                    "artId": "missing-art",
                                    "previewSrc": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
                                    "params": { "strength": 61 },
                                    "w": 90,
                                    "h": 90
                                }
                            }
                        ],
                        "edges": [
                            {
                                "id": "edge-1",
                                "source": "capture",
                                "target": "missing-art-node",
                                "sourceHandle": "output",
                                "targetHandle": "input"
                            }
                        ]
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(overwrite["type"], "success", "response={overwrite}");

        let before = expect_json_result_response(hook_canvas_snapshot(), 200);
        assert_eq!(before["nodes"][1]["status"], "ready");
        let before_revision = before["revision"]
            .as_str()
            .expect("before revision")
            .to_owned();

        let failure = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": "art/process",
                "params": {
                    "request_id": "req-missing-art",
                    "art_id": "missing-art",
                    "input": {
                        "type": "base64",
                        "data": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
                        "width": 1,
                        "height": 1,
                        "format": "rgba8"
                    },
                    "params": {
                        "strength": 61
                    },
                    "disabled_params": []
                }
            })
            .to_string(),
        );
        assert_eq!(failure["status"], "NotFound", "response={failure}");

        let after = expect_json_result_response(hook_canvas_snapshot(), 200);
        assert_eq!(after["nodes"][1]["status"], "error");
        assert_eq!(
            after["nodes"][1]["errorMessage"],
            "Art definition not found: missing-art"
        );
        assert_eq!(after["nodes"][1]["previewAvailable"], true);
        let after_revision = after["revision"].as_str().expect("after revision");
        assert_ne!(after_revision, before_revision);
        assert!(
            after_revision.contains("-rt-"),
            "expected runtime overlay revision suffix, got {after_revision}"
        );

        restore_env("APPDATA", previous_appdata);
        clear_hook_canvas_runtime_state();
        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(appdata).expect("cleanup appdata");
    }

    #[test]
    fn daemon_hook_canvas_overrides_blank_live_art_preview_with_runtime_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_hook_canvas_runtime_state();
        let root = unique_temp_dir("hook-canvas-runtime-preview");
        let appdata = unique_temp_dir("hook-canvas-runtime-preview-appdata");
        let previous_appdata = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        #[cfg(windows)]
        let execution = ToolExecution::CliWrapper {
            command: "powershell.exe".to_owned(),
            args: vec![
                "-NoProfile".to_owned(),
                "-Command".to_owned(),
                "Copy-Item -LiteralPath '{{input}}' -Destination '{{output}}' -Force".to_owned(),
            ],
        };
        #[cfg(not(windows))]
        let execution = ToolExecution::CliWrapper {
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                "cp \"$1\" \"$2\"".to_owned(),
                "loom-cli-wrapper".to_owned(),
                "{{input}}".to_owned(),
                "{{output}}".to_owned(),
            ],
        };
        let tool = ToolDefinition::new(
            "fixture-image-compress-preview",
            "Fixture Image Preview",
            "Return image output for preview overlay tests.",
            execution,
        );
        runtime
            .tool_registry
            .save_tool(tool)
            .expect("save preview fixture tool");

        let black_preview = {
            let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(1, 1, vec![0, 0, 0, 255])
                .expect("black preview image");
            let mut png = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(image)
                .write_to(&mut png, ImageFormat::Png)
                .expect("encode black preview");
            format!("data:image/png;base64,{}", BASE64.encode(png.into_inner()))
        };

        let overwrite = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": "art_loom/overwrite_workflow",
                "params": {
                    "workflow_id": HOOK_LIVE_WORKFLOW_ID,
                    "snapshot": {
                        "name": "Hook Live",
                        "nodes": [
                            {
                                "id": "capture",
                                "type": "screenshot",
                                "position": { "x": 20, "y": 30 },
                                "measured": { "width": 80, "height": 80 },
                                "data": {
                                    "src": test_png_base64(),
                                    "w": 80,
                                    "h": 80
                                }
                            },
                            {
                                "id": "compress-art",
                                "type": "art",
                                "position": { "x": 160, "y": 40 },
                                "measured": { "width": 90, "height": 90 },
                                "data": {
                                    "artId": "fixture-image-compress-preview",
                                    "previewSrc": black_preview,
                                    "params": { "level_num": 2 },
                                    "w": 90,
                                    "h": 90
                                }
                            }
                        ],
                        "edges": [
                            {
                                "id": "edge-1",
                                "source": "capture",
                                "target": "compress-art",
                                "sourceHandle": "output",
                                "targetHandle": "input"
                            }
                        ]
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(overwrite["type"], "success", "response={overwrite}");

        let before = expect_binary_route_response(
            hook_canvas_preview_response("compress-art").expect("initial blank preview"),
            200,
            "image/png",
        );
        assert_ne!(
            before,
            test_png_bytes(),
            "test requires the stored Hook preview to be blank or otherwise not equal to the real output image"
        );

        let success = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": "art/process",
                "params": {
                    "request_id": "req-runtime-preview",
                    "art_id": "fixture-image-compress-preview",
                    "input": {
                        "type": "base64",
                        "data": test_png_base64(),
                        "width": 1,
                        "height": 1,
                        "format": "rgba8"
                    },
                    "params": {
                        "level_num": 2
                    },
                    "disabled_params": []
                }
            })
            .to_string(),
        );
        assert_eq!(success["status"], "Success", "response={success}");

        let after_snapshot = expect_json_result_response(hook_canvas_snapshot(), 200);
        assert_eq!(after_snapshot["nodes"][1]["status"], "ready");
        let preview_url = after_snapshot["nodes"][1]["previewUrl"]
            .as_str()
            .expect("preview url");
        assert!(
            preview_url.contains("?v="),
            "runtime preview overlay should cache-bust preview urls: {preview_url}"
        );

        let after = expect_binary_route_response(
            hook_canvas_preview_response("compress-art").expect("runtime preview"),
            200,
            "image/png",
        );
        assert_eq!(
            after,
            test_png_bytes(),
            "successful runtime image output must override the blank Hook preview payload"
        );

        restore_env("APPDATA", previous_appdata);
        clear_hook_canvas_runtime_state();
        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(appdata).expect("cleanup appdata");
    }

    #[test]
    fn daemon_can_save_a_hook_canvas_component_directly_as_a_workflow() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_hook_canvas_runtime_state();
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let previous_appdata = std::env::var("APPDATA").ok();
        let root = unique_temp_dir("hook-canvas-save-workflow");
        let appdata = unique_temp_dir("hook-canvas-save-workflow-appdata");
        let session_dir = appdata.join("com.vmjcv.arthook-next");
        let images = session_dir.join("images");
        fs::create_dir_all(&images).expect("create session images");
        fs::write(
            session_dir.join("session.json"),
            r#"{
              "stickers": [
                {"id":"a","type":"sticker","src":"images/a.png","x":0,"y":0,"w":80,"h":80},
                {"id":"b","type":"art","artId":"resize","src":"images/b.png","x":200,"y":0,"w":80,"h":80},
                {"id":"c","type":"art","artId":"resize","src":"images/c.png","x":400,"y":0,"w":80,"h":80},
                {"id":"lonely","type":"sticker","src":"images/lonely.png","x":0,"y":200,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"a","toUnitId":"b"},
                {"id":"e2","fromUnitId":"b","toUnitId":"c"}
              ]
            }"#,
        )
        .expect("write Hook session");
        for name in ["a.png", "b.png", "c.png", "lonely.png"] {
            fs::write(images.join(name), test_png_bytes()).expect("write preview");
        }
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        std::env::set_var("APPDATA", &appdata);

        let workflow_store = WorkflowStore::new(root.join("workflows"));
        let canvas_workflow_root = root.join("canvas-workflows");
        let (status, body) = save_hook_canvas_workflow(
            "hook-export",
            r#"{"selectedNodeId":"a","workflowName":"Hook Export"}"#,
            &workflow_store,
            &canvas_workflow_root,
        )
        .expect("save Hook canvas workflow");
        assert_eq!(status, 200);
        let saved = serde_json::from_str::<serde_json::Value>(&body).expect("saved workflow json");
        assert_eq!(saved["workflow"]["id"], "hook-export");
        assert_eq!(saved["workflow"]["name"], "Hook Export");

        let loaded = workflow_store
            .load_workflow("hook-export")
            .expect("load saved workflow");
        let data = loaded;
        assert!(data.contains("name: 'Hook Export'"));
        assert!(data.contains("- id: a"));
        assert!(data.contains("- id: resize"));
        assert!(data.contains("- id: resize-2"));
        assert!(data.contains("needs: [a]"));
        assert!(data.contains("needs: [resize]"));
        assert!(!data.contains("lonely"));

        // Renaming updates the display name in meta.json without changing the id.
        let canvas_root = root.join("canvas-workflows");
        let (rename_status, rename_body) =
            rename_canvas_workflow("hook-export", r#"{"name":"Renamed Flow"}"#, &canvas_root)
                .expect("rename canvas workflow");
        assert_eq!(rename_status, 200);
        let renamed = serde_json::from_str::<serde_json::Value>(&rename_body).expect("rename json");
        assert_eq!(renamed["id"], "hook-export");
        assert_eq!(renamed["name"], "Renamed Flow");
        let (_, list_body) = list_canvas_workflows(&canvas_root).expect("list canvas workflows");
        let list_json = serde_json::from_str::<serde_json::Value>(&list_body).expect("list json");
        assert!(list_json["workflows"]
            .as_array()
            .expect("workflows array")
            .iter()
            .any(|w| w["id"] == "hook-export" && w["name"] == "Renamed Flow"));

        // Deleting removes the frozen snapshot directory.
        let (delete_status, _) =
            delete_canvas_workflow("hook-export", &canvas_root).expect("delete canvas workflow");
        assert_eq!(delete_status, 200);
        let (_, after_body) = list_canvas_workflows(&canvas_root).expect("list after delete");
        let after_json =
            serde_json::from_str::<serde_json::Value>(&after_body).expect("after json");
        assert!(after_json["workflows"]
            .as_array()
            .expect("workflows array")
            .iter()
            .all(|w| w["id"] != "hook-export"));

        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        restore_env("APPDATA", previous_appdata);
        clear_hook_canvas_runtime_state();
        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(appdata).expect("cleanup appdata");
    }

    #[test]
    fn daemon_serves_only_registered_hook_canvas_preview_images() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let appdata = unique_temp_dir("hook-canvas-preview-appdata");
        let session_dir = appdata.join("com.vmjcv.arthook-next");
        let images = session_dir.join("images");
        fs::create_dir_all(&images).expect("create session images");
        let png = test_png_bytes();
        fs::write(images.join("capture.png"), &png).expect("write registered preview");
        fs::write(appdata.join("outside.png"), &png).expect("write outside preview");
        fs::write(
            session_dir.join("session.json"),
            r#"{
              "stickers": [
                {"id":"capture node","type":"sticker","src":"images/capture.png","x":20,"y":30,"w":320,"h":180},
                {"id":"escape","type":"sticker","src":"../outside.png","x":400,"y":30,"w":320,"h":180}
              ],
              "links": []
            }"#,
        )
        .expect("write Hook session");
        let previous = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);

        let registered = hook_canvas_preview_response("capture node").expect("registered preview");
        let registered_body = expect_binary_route_response(registered, 200, "image/png");
        assert_eq!(registered_body, png);

        for node_id in ["unknown", "escape"] {
            let response =
                hook_canvas_preview_response(node_id).expect("preview not found response");
            let body = expect_text_route_response(response, 404);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&body).expect("preview error json")
                    ["error"]["code"],
                "preview_not_found"
            );
        }

        restore_env("APPDATA", previous);
        fs::remove_dir_all(appdata).expect("cleanup");
    }

    #[test]
    fn daemon_prefers_data_url_hook_canvas_preview_over_file_backed_src() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let appdata = unique_temp_dir("hook-canvas-preview-data-url");
        let session_dir = appdata.join("com.vmjcv.arthook-next");
        let images = session_dir.join("images");
        fs::create_dir_all(&images).expect("create session images");

        let preferred_png = test_png_bytes();
        let preferred_data_url = test_png_base64();

        let fallback_png = {
            let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(1, 1, vec![200, 100, 50, 255])
                .expect("fallback png image");
            let mut bytes = Vec::new();
            DynamicImage::ImageRgba8(image)
                .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
                .expect("encode fallback png");
            bytes
        };
        fs::write(images.join("original.png"), &fallback_png).expect("write fallback image");
        fs::write(
            session_dir.join("session.json"),
            format!(
                r#"{{
                  "stickers": [
                    {{
                      "id":"capture",
                      "type":"sticker",
                      "src":"images/original.png",
                      "previewSrc":"{}"
                    }}
                  ],
                  "links": []
                }}"#,
                preferred_data_url
            ),
        )
        .expect("write session");

        let previous = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);

        let response = hook_canvas_preview_response("capture").expect("data-url preview response");
        let body = expect_binary_route_response(response, 200, "image/png");
        assert_eq!(
            body, preferred_png,
            "daemon should serve previewSrc data URL instead of falling back to src",
        );

        restore_env("APPDATA", previous);
        fs::remove_dir_all(appdata).expect("cleanup");
    }

    #[test]
    fn daemon_validates_hook_canvas_preview_type_and_size() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let appdata = unique_temp_dir("hook-canvas-preview-validation");
        let session_dir = appdata.join("com.vmjcv.arthook-next");
        let images = session_dir.join("images");
        fs::create_dir_all(&images).expect("create session images");
        fs::write(images.join("unsupported.bin"), b"not-an-image").expect("write unsupported");
        let oversized_path = images.join("oversized.png");
        fs::File::create(&oversized_path)
            .expect("create oversized")
            .set_len(hook_canvas::MAX_PREVIEW_BYTES + 1)
            .expect("size oversized");
        fs::write(images.join("pixel.jpg"), [0xff, 0xd8, 0xff, 0xe0]).expect("write jpeg");
        fs::write(images.join("pixel.webp"), b"RIFF\x04\x00\x00\x00WEBP").expect("write webp");
        fs::write(
            session_dir.join("session.json"),
            r#"{
              "stickers": [
                {"id":"unsupported","type":"sticker","src":"images/unsupported.bin"},
                {"id":"oversized","type":"sticker","src":"images/oversized.png"},
                {"id":"jpeg","type":"sticker","src":"images/pixel.jpg"},
                {"id":"webp","type":"sticker","src":"images/pixel.webp"}
              ],
              "links": []
            }"#,
        )
        .expect("write Hook session");
        let previous = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);

        let unsupported =
            hook_canvas_preview_response("unsupported").expect("unsupported preview response");
        expect_text_route_response(unsupported, 415);

        let oversized =
            hook_canvas_preview_response("oversized").expect("oversized preview response");
        expect_text_route_response(oversized, 413);

        for (node_id, expected_type) in [("jpeg", "image/jpeg"), ("webp", "image/webp")] {
            let response = hook_canvas_preview_response(node_id)
                .unwrap_or_else(|error| panic!("preview response for {node_id}: {error:#}"));
            expect_binary_route_response(response, 200, expected_type);
        }

        restore_env("APPDATA", previous);
        fs::remove_dir_all(appdata).expect("cleanup");
    }

    #[test]
    fn daemon_preserves_auth_and_structured_errors_for_hook_canvas_routes() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let appdata = unique_temp_dir("hook-canvas-auth-appdata");
        let control_plane_root = unique_temp_dir("hook-canvas-auth-control-plane");
        let session_dir = appdata.join("com.vmjcv.arthook-next");
        let images = session_dir.join("images");
        fs::create_dir_all(&images).expect("create session images");
        fs::write(images.join("capture.png"), test_png_bytes()).expect("write preview");
        fs::write(
            session_dir.join("session.json"),
            r#"{"stickers":[{"id":"capture","type":"sticker","src":"images/capture.png"}],"links":[]}"#,
        )
        .expect("write Hook session");
        let previous = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);
        let runtime = test_daemon_runtime(&control_plane_root, Some("canvas-secret"));

        let unauthorized = route_request(
            &runtime,
            &parsed_request(
                "GET",
                "/v1/hook-bridge/canvas/nodes/capture/preview",
                &[],
                None,
            ),
        );
        let unauthorized_body = expect_text_route_response(unauthorized, 401);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&unauthorized_body)
                .expect("unauthorized json")["error"]["code"],
            "unauthorized"
        );

        let authorized = route_request(
            &runtime,
            &parsed_request(
                "GET",
                "/v1/hook-bridge/canvas/nodes/capture/preview",
                &[("Authorization", "Bearer canvas-secret")],
                None,
            ),
        );
        expect_binary_route_response(authorized, 200, "image/png");

        fs::write(session_dir.join("session.json"), "{not-json").expect("corrupt session");
        let malformed_runtime = test_daemon_runtime(&control_plane_root, None);
        let response = route_request(
            &malformed_runtime,
            &parsed_request("GET", "/v1/hook-bridge/canvas", &[], None),
        );
        let body = expect_text_route_response(response, 500);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).expect("malformed session json")
                ["error"]["code"],
            "hook_canvas_error"
        );

        restore_env("APPDATA", previous);
        fs::remove_dir_all(appdata).expect("cleanup");
        fs::remove_dir_all(control_plane_root).expect("cleanup control plane");
    }

    #[test]
    fn daemon_exposes_artloom_settings_shortcuts_and_safe_system_contracts() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("artloom-settings");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let settings = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/settings", &[], None),
            ),
            200,
        );
        assert_eq!(settings["compatCommand"], "get_settings");
        assert_eq!(settings["settings"]["general"]["theme"], "system");
        assert_eq!(
            settings["settings"]["engine"]["comfyui_url"],
            "http://127.0.0.1:8188"
        );

        let updated_settings = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/artloom-compat/settings",
                    &[],
                    Some(
                        r#"{"general":{"theme":"dark","language":"en","auto_start":false,"minimize_to_tray":true,"enable_tray_icon":true},"system":{"auto_check_updates":true,"enable_run_log":true,"run_as_admin":false,"record_screenshot_history":true,"history_retention":"7d","enable_proxy":false},"engine":{"comfyui_url":"http://127.0.0.1:8188","python_interpreter":"python.exe","virtual_env_path":"./venv","compute_device":"0","vram_reservation_gb":12},"quick_bindings":[{"id":"1","art":"ComfyUI Workflow","key":"Ctrl+Shift+1"}],"shortcuts":{}}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(updated_settings["compatCommand"], "update_settings");
        assert_eq!(updated_settings["settings"]["general"]["theme"], "dark");

        let shortcut = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/artloom-compat/shortcuts/capture",
                    &[],
                    Some(
                        r#"{"id":"capture","label":"Screenshot","keys":"Ctrl+Alt+1","enabled":true}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(shortcut["compatCommand"], "update_shortcut");
        assert_eq!(shortcut["shortcut"]["keys"], "Ctrl+Alt+1");

        let shortcuts = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/shortcuts", &[], None),
            ),
            200,
        );
        assert_eq!(shortcuts["compatCommand"], "get_shortcuts");
        assert_eq!(shortcuts["shortcuts"][0]["id"], "capture");
        assert_eq!(shortcuts["shortcuts"][0]["keys"], "Ctrl+Alt+1");

        let started = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("POST", "/v1/hook-bridge/start", &[], Some(r#"{"port":0}"#)),
            ),
            200,
        );
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"get_settings"}"#.to_owned(),
            ))
            .expect("send get_settings");
        let hook_settings = read_hook_bridge_json(&mut socket);
        assert_eq!(hook_settings["type"], "settings");
        assert_eq!(hook_settings["data"]["general"]["theme"], "dark");

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"get_shortcuts"}"#.to_owned(),
            ))
            .expect("send get_shortcuts");
        let hook_shortcuts = read_hook_bridge_json(&mut socket);
        assert_eq!(hook_shortcuts["type"], "shortcuts");
        assert_eq!(hook_shortcuts["data"][0]["id"], "capture");
        assert_eq!(hook_shortcuts["data"][0]["keys"], "Ctrl+Alt+1");

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"sync_shortcuts"}"#.to_owned(),
            ))
            .expect("send sync_shortcuts");
        let hook_synced = read_hook_bridge_json(&mut socket);
        assert_eq!(hook_synced["type"], "shortcuts");
        assert_eq!(hook_synced["data"][0]["keys"], "Ctrl+Alt+1");

        drop(socket);
        let stopped = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("POST", "/v1/hook-bridge/stop", &[], Some("{}")),
            ),
            200,
        );
        assert_eq!(stopped["running"], false);

        let default_autostart = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/system/autostart", &[], None),
            ),
            200,
        );
        assert_eq!(default_autostart["compatCommand"], "is_autostart_enabled");
        assert_eq!(default_autostart["enabled"], false);
        assert_eq!(default_autostart["sideEffect"], false);

        let autostart = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/system/autostart",
                    &[],
                    Some(r#"{"enabled":true}"#),
                ),
            ),
            200,
        );
        assert_eq!(autostart["compatCommand"], "set_autostart");
        assert_eq!(autostart["enabled"], true);
        assert_eq!(autostart["sideEffect"], false);

        let updated_autostart = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/system/autostart", &[], None),
            ),
            200,
        );
        assert_eq!(updated_autostart["compatCommand"], "is_autostart_enabled");
        assert_eq!(updated_autostart["enabled"], true);
        assert_eq!(updated_autostart["sideEffect"], false);

        let disabled_autostart = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/system/autostart/disable",
                    &[],
                    Some("{}"),
                ),
            ),
            200,
        );
        assert_eq!(disabled_autostart["compatCommand"], "disable_autostart");
        assert_eq!(disabled_autostart["enabled"], false);
        assert_eq!(disabled_autostart["sideEffect"], false);

        let enabled_autostart = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/system/autostart/enable",
                    &[],
                    Some("{}"),
                ),
            ),
            200,
        );
        assert_eq!(enabled_autostart["compatCommand"], "enable_autostart");
        assert_eq!(enabled_autostart["enabled"], true);
        assert_eq!(enabled_autostart["sideEffect"], false);

        let tray = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/system/minimize-to-tray",
                    &[],
                    Some(r#"{"enabled":false}"#),
                ),
            ),
            200,
        );
        assert_eq!(tray["compatCommand"], "set_minimize_to_tray");
        assert_eq!(tray["enabled"], false);
        assert_eq!(tray["sideEffect"], false);

        let paths = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/app-paths", &[], None),
            ),
            200,
        );
        assert_eq!(paths["compatCommand"], "get_app_paths");

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup settings root");
    }

    #[test]
    fn daemon_exposes_artloom_registry_ipc_and_shared_memory_aliases() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("artloom-compat-aliases");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let sync_body = serde_json::json!({
            "arts": [{
                "id": "compat-art",
                "name": "Compat Art",
                "description": "ArtLoom registry alias fixture",
                "iconColor": "#52c41a",
                "enabled": true,
                "execution_type": "cli_wrapper",
                "execution": {
                    "command": "echo",
                    "args": "{{inputs.image.path}} --out {{outputs.result.path}}",
                    "outputs": [{ "name": "result", "type": "image" }]
                },
                "autoProcess": true,
                "defaults": { "seed": 1234 },
                "inputs": [{ "name": "image", "type": "image" }],
                "params": [{ "id": "strength", "default": 0.1 }]
            }]
        })
        .to_string();
        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/sync",
                    &[],
                    Some(&sync_body),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["syncedCount"], 1);
        assert_eq!(saved_tool["arts"][0]["art_id"], "compat-art");

        let arts = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/arts", &[], None),
            ),
            200,
        );
        assert_eq!(arts["compatCommand"], "list_arts");
        assert_eq!(arts["count"], 1);
        assert_eq!(arts["arts"][0]["art_id"], "compat-art");
        assert_eq!(arts["arts"][0]["auto_process"], true);
        assert_eq!(arts["arts"][0]["defaults"]["seed"], 1234);

        let user_arts = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/user-arts", &[], None),
            ),
            200,
        );
        assert_eq!(user_arts["compatCommand"], "get_user_arts");
        assert_eq!(user_arts["arts"][0]["id"], "compat-art");
        assert_eq!(user_arts["arts"][0]["name"], "Compat Art");
        assert_eq!(
            user_arts["arts"][0]["description"],
            "ArtLoom registry alias fixture"
        );
        assert_eq!(user_arts["arts"][0]["category"], "Adapter");
        assert_eq!(user_arts["arts"][0]["version"], "1.0.0");
        assert_eq!(user_arts["arts"][0]["author"], "User");
        assert_eq!(user_arts["arts"][0]["status"], "active");
        assert_eq!(user_arts["arts"][0]["iconColor"], "#52c41a");
        assert_eq!(user_arts["arts"][0]["downloads"], 0);
        assert_eq!(user_arts["arts"][0]["owned"], true);
        assert_eq!(user_arts["arts"][0]["executionType"], "cli_wrapper");
        assert_eq!(
            user_arts["arts"][0]["execution"]["args"],
            "{{inputs.image.path}} --out {{outputs.result.path}}"
        );
        assert_eq!(user_arts["arts"][0]["autoProcess"], true);
        assert_eq!(user_arts["arts"][0]["inputs"][0]["name"], "image");
        assert_eq!(user_arts["arts"][0]["outputs"][0]["name"], "result");
        assert!(user_arts["arts"][0].get("art_id").is_none());

        let art = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/arts/compat-art", &[], None),
            ),
            200,
        );
        assert_eq!(art["compatCommand"], "get_art");
        assert_eq!(art["art"]["enabled"], true);
        assert_eq!(art["art"]["auto_process"], true);
        assert_eq!(art["art"]["defaults"]["seed"], 1234);

        let enabled_arts = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/arts/enabled", &[], None),
            ),
            200,
        );
        assert_eq!(enabled_arts["compatCommand"], "get_enabled_arts");
        assert_eq!(enabled_arts["count"], 1);
        assert_eq!(enabled_arts["arts"][0]["id"], "compat-art");

        let disabled = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/compat-art/disable",
                    &[],
                    Some("{}"),
                ),
            ),
            200,
        );
        assert_eq!(disabled["compatCommand"], "disable_art");
        assert_eq!(disabled["enabled"], false);

        let enabled_after_disable = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/arts/enabled", &[], None),
            ),
            200,
        );
        assert_eq!(enabled_after_disable["compatCommand"], "get_enabled_arts");
        assert_eq!(enabled_after_disable["count"], 0);

        let enabled = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/compat-art/enable",
                    &[],
                    Some("{}"),
                ),
            ),
            200,
        );
        assert_eq!(enabled["compatCommand"], "enable_art");
        assert_eq!(enabled["enabled"], true);

        let defaults = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/artloom-compat/arts/compat-art/defaults",
                    &[],
                    Some(r#"{"defaults":{"strength":0.8}}"#),
                ),
            ),
            200,
        );
        assert_eq!(defaults["compatCommand"], "update_art_defaults");
        assert_eq!(defaults["tool"]["params"][0]["default"], 0.8);

        let synced = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("POST", "/v1/artloom-compat/arts/sync", &[], Some("{}")),
            ),
            200,
        );
        assert_eq!(synced["compatCommand"], "sync_user_arts");
        assert_eq!(synced["synced"], true);
        assert_eq!(synced["sideEffect"], false);
        assert!(synced["message"]
            .as_str()
            .expect("sync message")
            .contains("source of truth"));

        let ipc = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/ipc/status", &[], None),
            ),
            200,
        );
        assert_eq!(ipc["compatCommand"], "get_ipc_status");
        assert_eq!(ipc["protocol"], "artloom-compat");

        let created_buffer = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/shared-memory/buffers",
                    &[],
                    Some(r#"{"width":1,"height":1,"channels":4}"#),
                ),
            ),
            200,
        );
        assert_eq!(created_buffer["compatCommand"], "shm_create_buffer");
        let handle = created_buffer["handle"]
            .as_str()
            .expect("shared memory handle");

        let buffers = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/shared-memory/buffers", &[], None),
            ),
            200,
        );
        assert_eq!(buffers["compatCommand"], "shm_list_buffers");
        assert_eq!(buffers["buffers"][0]["handle_name"], handle);
        assert_eq!(buffers["buffers"][0]["format"], "rgba8");
        assert_eq!(buffers["buffers"][0]["ref_count"], 1);

        let info = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "GET",
                    &format!("/v1/shared-memory/buffers/{handle}"),
                    &[],
                    None,
                ),
            ),
            200,
        );
        assert_eq!(info["compatCommand"], "shm_get_buffer_info");
        assert_eq!(info["buffer"]["handle_name"], handle);

        let released = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "DELETE",
                    &format!("/v1/shared-memory/buffers/{handle}"),
                    &[],
                    None,
                ),
            ),
            200,
        );
        assert_eq!(released["compatCommand"], "shm_release_buffer");
        assert_eq!(released["released"], true);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup alias root");
    }

    #[test]
    fn daemon_sync_user_arts_imports_payload_and_preserves_non_compat_tools() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("artloom-compat-import");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let regular_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/loom-tool",
                    &[],
                    Some(
                        r#"{"id":"loom-tool","name":"Loom Tool","description":"native loom tool","enabled":true,"execution":{"type":"workflow","workflowId":"wf-native"}}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(regular_tool["tool"]["id"], "loom-tool");

        let old_compat_body = serde_json::json!({
            "arts": [{
                "id": "compat-old",
                "label": "Compat Old",
                "description": "old compat art",
                "enabled": true,
                "execution": { "type": "cli_wrapper", "command": "echo", "args": ["old"] },
                "params": [{ "id": "strength", "default": 0.1 }]
            }]
        })
        .to_string();
        let old_compat = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/sync",
                    &[],
                    Some(&old_compat_body),
                ),
            ),
            200,
        );
        assert_eq!(old_compat["syncedCount"], 1);
        assert_eq!(old_compat["arts"][0]["art_id"], "compat-old");

        let sync_body = serde_json::json!({
            "arts": [{
                "id": "compat-new",
                "label": "Compat New",
                "description": "new compat art",
                "icon": "#52c41a",
                "enabled": true,
                "execution_type": "cli_wrapper",
                "execution": { "type": "cli_wrapper", "command": "echo", "args": ["new"] },
                "inputs": [
                    { "name": "prompt", "label": "Prompt", "type": "string", "execution_type": "string", "default": "hello" }
                ],
                "params": [
                    { "id": "strength", "label": "Strength", "widget": "slider", "default": 0.8, "min": 0.0, "max": 1.0, "step": 0.1 }
                ],
                "defaults": { "strength": 0.8 }
            }]
        })
        .to_string();
        let synced = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/sync",
                    &[],
                    Some(&sync_body),
                ),
            ),
            200,
        );
        assert_eq!(synced["compatCommand"], "sync_user_arts");
        assert_eq!(synced["sideEffect"], true);
        assert_eq!(synced["syncedCount"], 1);

        let tools = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/v1/tools", &[], None)),
            200,
        );
        let listed = tools["tools"].as_array().expect("tools array");
        assert!(listed.iter().any(|tool| tool["id"] == "loom-tool"));
        assert!(listed.iter().any(|tool| tool["id"] == "compat-new"));
        assert!(!listed.iter().any(|tool| tool["id"] == "compat-old"));

        let arts = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/arts", &[], None),
            ),
            200,
        );
        let listed_arts = arts["arts"].as_array().expect("arts array");
        assert_eq!(listed_arts.len(), 1);
        assert_eq!(listed_arts[0]["art_id"], "compat-new");
        assert_eq!(listed_arts[0]["inputs"][0]["name"], "prompt");
        assert_eq!(listed_arts[0]["params"][0]["default"], 0.8);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup import root");
    }

    #[test]
    fn daemon_hook_bridge_runtime_start_status_stop() {
        let root = unique_temp_dir("hook-bridge-runtime");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let stopped = expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200);
        assert_eq!(stopped["running"], false);
        assert_eq!(stopped["connectedClients"], 0);

        let started = expect_json_result_response(
            start_hook_bridge(
                r#"{"port":0}"#,
                &runtime.hook_bridge,
                &runtime.mcp_servers,
                &runtime.tool_registry,
                &runtime.workflow_store,
                &runtime.artloom_settings,
                &runtime.shared_images,
                &runtime.ocr_provider,
            ),
            200,
        );
        assert_eq!(started["running"], true);
        assert!(started["port"].as_u64().expect("assigned bridge port") > 0);
        assert_eq!(started["connectedClients"], 0);
        assert_eq!(started["protocol"], "artloom-compat");

        let running = expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200);
        assert_eq!(running["running"], true);
        assert_eq!(running["port"], started["port"]);

        let duplicate_start = expect_json_result_response(
            start_hook_bridge(
                r#"{"port":0}"#,
                &runtime.hook_bridge,
                &runtime.mcp_servers,
                &runtime.tool_registry,
                &runtime.workflow_store,
                &runtime.artloom_settings,
                &runtime.shared_images,
                &runtime.ocr_provider,
            ),
            409,
        );
        assert_eq!(duplicate_start["error"]["code"], "hook_bridge_running");

        let stopped_again =
            expect_json_result_response(stop_hook_bridge(&runtime.hook_bridge), 200);
        assert_eq!(stopped_again["running"], false);
        assert_eq!(stopped_again["connectedClients"], 0);
        assert_eq!(stopped_again["port"], 19820);

        let final_status =
            expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200);
        assert_eq!(final_status["running"], false);
        assert_eq!(final_status["port"], 19820);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup hook bridge root");
    }

    #[test]
    fn daemon_hook_bridge_accepts_websocket_handshake_request() {
        let root = unique_temp_dir("hook-bridge-handshake");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let started = expect_json_result_response(
            start_hook_bridge(
                r#"{"port":0}"#,
                &runtime.hook_bridge,
                &runtime.mcp_servers,
                &runtime.tool_registry,
                &runtime.workflow_store,
                &runtime.artloom_settings,
                &runtime.shared_images,
                &runtime.ocr_provider,
            ),
            200,
        );
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let stream =
            TcpStream::connect(("127.0.0.1", bridge_port)).expect("connect bridge tcp socket");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set websocket read timeout");
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .expect("set websocket write timeout");
        let (mut socket, _) = tungstenite::client(format!("ws://127.0.0.1:{bridge_port}"), stream)
            .expect("connect bridge websocket");

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"handshake","params":{"client_version":"0.4.2"}}"#.to_owned(),
            ))
            .expect("send handshake");
        let response = socket.read().expect("read handshake response");
        let response = response.into_text().expect("text response");
        let response: serde_json::Value = serde_json::from_str(&response).expect("response json");

        assert_eq!(response["type"], "handshake");
        assert_eq!(response["data"]["server_version"], "0.1.0");
        assert!(response["data"]["session_id"].as_str().is_some());

        let running = expect_json_result_response(hook_bridge_status(&runtime.hook_bridge), 200);
        assert_eq!(running["running"], true);
        assert!(
            running["connectedClients"]
                .as_u64()
                .expect("connected clients")
                >= 1
        );

        drop(socket);
        let stopped = expect_json_result_response(stop_hook_bridge(&runtime.hook_bridge), 200);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup hook bridge root");
    }

    #[test]
    fn daemon_artloom_sync_without_defaults_does_not_treat_art_shape_as_defaults() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("artloom-compat-no-defaults");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let sync_body = serde_json::json!({
            "arts": [{
                "id": "compat-no-defaults",
                "name": "Compat No Defaults",
                "description": "Old sync_user_arts payload without independent defaults",
                "iconColor": "#52c41a",
                "enabled": true,
                "execution_type": "cli_wrapper",
                "execution": {
                    "command": "echo",
                    "args": "{{inputs.prompt.value}}",
                    "outputs": [{ "name": "result", "type": "text" }]
                },
                "inputs": [{ "name": "prompt", "type": "text" }],
                "params": [{ "id": "strength", "default": 0.2 }]
            }]
        })
        .to_string();
        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/sync",
                    &[],
                    Some(&sync_body),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["syncedCount"], 1);

        let arts = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/artloom-compat/arts", &[], None),
            ),
            200,
        );
        assert_eq!(arts["compatCommand"], "list_arts");
        assert_eq!(arts["arts"][0]["defaults"], json!({ "strength": 0.2 }));
        assert!(arts["arts"][0]["defaults"].get("id").is_none());
        assert!(arts["arts"][0]["defaults"].get("execution").is_none());
        assert!(arts["arts"][0]["defaults"].get("params").is_none());

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup settings root");
    }

    #[test]
    fn daemon_hook_bridge_sync_user_arts_imports_hook_payload() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("hook-bridge-sync-user-arts");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let saved_native_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/loom-tool",
                    &[],
                    Some(
                        r#"{"id":"loom-tool","name":"Loom Tool","description":"native loom tool","enabled":true,"execution":{"type":"workflow","workflowId":"wf-native"}}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_native_tool["tool"]["id"], "loom-tool");

        let started = expect_json_result_response(
            start_hook_bridge(
                r#"{"port":0}"#,
                &runtime.hook_bridge,
                &runtime.mcp_servers,
                &runtime.tool_registry,
                &runtime.workflow_store,
                &runtime.artloom_settings,
                &runtime.shared_images,
                &runtime.ocr_provider,
            ),
            200,
        );
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;

        let mut socket = connect_hook_bridge_websocket(bridge_port);
        let request = serde_json::json!({
            "method": "sync_user_arts",
            "params": {
                "arts": [{
                    "id": "hook-art",
                    "name": "Hook Art",
                    "description": "Synced from Hook",
                    "iconColor": "#52c41a",
                    "enabled": true,
                    "execution_type": "cli_wrapper",
                    "execution": {
                        "command": "echo",
                        "args": "{{inputs.prompt.value}}",
                        "outputs": [{ "name": "result", "type": "text" }]
                    },
                    "inputs": [{ "name": "prompt", "label": "Prompt", "type": "string" }],
                    "params": [{ "id": "strength", "default": 0.7 }]
                }]
            }
        });
        socket
            .send(tungstenite::Message::Text(request.to_string()))
            .expect("send sync_user_arts");
        let response = read_hook_bridge_json(&mut socket);
        assert_eq!(response["type"], "success");
        assert_eq!(response["data"]["compatCommand"], "sync_user_arts");
        assert_eq!(response["data"]["sideEffect"], true);
        assert_eq!(response["data"]["syncedCount"], 1);

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"list_arts"}"#.to_owned(),
            ))
            .expect("send list_arts");
        let listed = read_hook_bridge_json(&mut socket);
        assert_eq!(listed["type"], "arts");
        assert_eq!(listed["data"].as_array().expect("arts").len(), 1);
        assert_eq!(listed["data"][0]["art_id"], "hook-art");
        assert_eq!(listed["data"][0]["icon"], "#52c41a");
        assert_eq!(listed["data"][0]["execution_type"], "cli_wrapper");
        assert_eq!(
            listed["data"][0]["execution"]["args"],
            "{{inputs.prompt.value}}"
        );
        assert_eq!(
            listed["data"][0]["execution"]["outputs"][0]["name"],
            "result"
        );
        assert_eq!(listed["data"][0]["inputs"][0]["name"], "prompt");

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"update_art_param","params":{"art_id":"hook-art","param_id":"strength","value":0.9}}"#
                    .to_owned(),
            ))
            .expect("send update_art_param");
        let updated_param = read_hook_bridge_json(&mut socket);
        assert_eq!(updated_param["type"], "success");
        assert_eq!(updated_param["data"]["compatCommand"], "update_art_param");
        assert_eq!(updated_param["data"]["art_id"], "hook-art");
        assert_eq!(updated_param["data"]["param_id"], "strength");

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"list_arts"}"#.to_owned(),
            ))
            .expect("send list_arts after update_art_param");
        let relisted = read_hook_bridge_json(&mut socket);
        assert_eq!(relisted["type"], "arts");
        assert_eq!(relisted["data"][0]["defaults"]["strength"], 0.9);
        assert_eq!(relisted["data"][0]["params"][0]["default"], 0.9);

        let tools = expect_json_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/v1/tools", &[], None)),
            200,
        );
        let listed_tools = tools["tools"].as_array().expect("tools array");
        assert!(listed_tools.iter().any(|tool| tool["id"] == "loom-tool"));
        assert!(listed_tools.iter().any(|tool| tool["id"] == "hook-art"));

        drop(socket);
        let stopped = expect_json_result_response(stop_hook_bridge(&runtime.hook_bridge), 200);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup hook sync root");
    }

    #[test]
    fn daemon_hook_bridge_sync_user_arts_preserves_loom_local_compat_art_on_id_collision() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("hook-bridge-sync-loom-local-collision");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let art_id = "custom-1770146354922";
        let local_command = root
            .join("arts")
            .join(art_id)
            .join("bin")
            .join("pingo.exe")
            .to_string_lossy()
            .to_string();
        let local_tool: ToolDefinition = serde_json::from_value(json!({
            "id": art_id,
            "name": "图片压缩",
            "description": "使用 Pingo 对 PNG/JPEG/WebP/APNG 图片执行本地压缩",
            "enabled": true,
            "execution": {
                "type": "cli_wrapper",
                "command": local_command,
                "args": [
                    "-s{{level_num}}",
                    "-quality={{quality_num}}",
                    "{{-lossless}}",
                    "{{output}}"
                ]
            },
            "outputs": [{
                "name": "output",
                "label": "output",
                "type": "image",
                "execution_type": "image_buffer"
            }],
            "params": [
                {
                    "id": "level_num",
                    "label": "压缩级别",
                    "widget": "slider",
                    "default": 2,
                    "min": 1,
                    "max": 4,
                    "step": 1,
                    "data_type": "number"
                },
                {
                    "id": "quality_num",
                    "label": "质量",
                    "widget": "slider",
                    "default": 90,
                    "min": 60,
                    "max": 100,
                    "step": 1,
                    "data_type": "number"
                },
                {
                    "id": "lossless",
                    "label": "无损压缩",
                    "widget": "checkbox",
                    "default": false,
                    "data_type": "bool"
                }
            ],
            "metadata": {
                "dependencies": {
                    "framework": "cli_wrapper",
                    "binaries": [{
                        "name": "bin/pingo.exe",
                        "sha256": "abc"
                    }]
                },
                "artloomCompat": {
                    "defaults": {},
                    "executionType": "cli_wrapper",
                    "icon": "#52c41a",
                    "source": "loom-local",
                    "execution": {
                        "command": local_command,
                        "args": "-s{{level_num}} -quality={{quality_num}} {{-lossless}} {{output}}",
                        "outputs": [{
                            "name": "output",
                            "label": "output",
                            "type": "image",
                            "execution_type": "image_buffer"
                        }],
                        "sourceType": "installed"
                    }
                }
            }
        }))
        .expect("local tool definition");
        runtime
            .tool_registry
            .save_tool(local_tool)
            .expect("save loom-local compat art");

        let started = expect_json_result_response(
            start_hook_bridge(
                r#"{"port":0}"#,
                &runtime.hook_bridge,
                &runtime.mcp_servers,
                &runtime.tool_registry,
                &runtime.workflow_store,
                &runtime.artloom_settings,
                &runtime.shared_images,
                &runtime.ocr_provider,
            ),
            200,
        );
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;

        let mut socket = connect_hook_bridge_websocket(bridge_port);
        let request = serde_json::json!({
            "method": "sync_user_arts",
            "params": {
                "arts": [{
                    "id": art_id,
                    "name": "图片压缩",
                    "description": "",
                    "enabled": true,
                    "execution_type": "cli_wrapper",
                    "execution": {
                        "command": "\"C:\\Users\\vmjcv\\Downloads\\pingo-win64\\pingo.exe\"",
                        "args": "-s{{level_num}} -quality={{quality_num}} {{-lossless}} {{output}}",
                        "outputs": [{
                            "captureMode": "explicit_path",
                            "execution_type": "image_path",
                            "filename": "{{input}}",
                            "label": "output",
                            "name": "output",
                            "type": "image"
                        }]
                    },
                    "outputs": [{
                        "captureMode": "explicit_path",
                        "execution_type": "image_path",
                        "filename": "{{input}}",
                        "label": "output",
                        "name": "output",
                        "type": "image"
                    }],
                    "params": [
                        {
                            "data_type": "number",
                            "default": "2",
                            "id": "level_num",
                            "label": "level_num",
                            "max": 9.0,
                            "min": 1.0,
                            "step": 1.0,
                            "widget": "slider"
                        },
                        {
                            "data_type": "number",
                            "default": "90",
                            "id": "quality_num",
                            "label": "quality_num",
                            "max": 100.0,
                            "min": 60.0,
                            "step": 1.0,
                            "widget": "slider"
                        },
                        {
                            "data_type": "string",
                            "default": "-lossless",
                            "id": "lossless",
                            "label": "lossless",
                            "widget": "text"
                        }
                    ]
                }]
            }
        });
        socket
            .send(tungstenite::Message::Text(request.to_string()))
            .expect("send colliding sync_user_arts");
        let response = read_hook_bridge_json(&mut socket);
        assert_eq!(response["type"], "success");
        assert_eq!(response["data"]["compatCommand"], "sync_user_arts");
        assert_eq!(response["data"]["sideEffect"], true);
        assert_eq!(response["data"]["syncedCount"], 0);
        assert_eq!(response["data"]["count"], 1);
        assert_eq!(response["data"]["arts"][0]["id"], art_id);
        assert_eq!(
            response["data"]["arts"][0]["execution"]["command"],
            local_command
        );
        assert_eq!(
            response["data"]["arts"][0]["outputs"][0]["execution_type"],
            "image_buffer"
        );
        assert_eq!(
            response["data"]["arts"][0]["params"][2]["data_type"],
            "bool"
        );

        let saved = runtime
            .tool_registry
            .get_tool(art_id)
            .expect("get saved art")
            .expect("art exists");
        match saved.execution {
            ToolExecution::CliWrapper { command, args } => {
                assert_eq!(command, local_command);
                assert_eq!(
                    args,
                    vec![
                        "-s{{level_num}}".to_owned(),
                        "-quality={{quality_num}}".to_owned(),
                        "{{-lossless}}".to_owned(),
                        "{{output}}".to_owned(),
                    ]
                );
            }
            other => panic!("expected cli_wrapper execution, got {other:?}"),
        }
        assert_eq!(saved.outputs[0]["execution_type"], "image_buffer");
        assert_eq!(saved.params[2]["data_type"], "bool");

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"list_arts"}"#.to_owned(),
            ))
            .expect("send list_arts");
        let listed = read_hook_bridge_json(&mut socket);
        assert_eq!(listed["type"], "arts");
        assert_eq!(listed["data"].as_array().expect("arts").len(), 1);
        assert_eq!(listed["data"][0]["art_id"], art_id);
        assert_eq!(listed["data"][0]["execution"]["command"], local_command);
        assert_eq!(
            listed["data"][0]["outputs"][0]["execution_type"],
            "image_buffer"
        );

        drop(socket);
        let stopped = expect_json_result_response(stop_hook_bridge(&runtime.hook_bridge), 200);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup hook sync collision root");
    }

    #[test]
    fn daemon_hook_bridge_fans_out_broadcasts_to_subscribed_websocket_clients() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("hook-bridge-fanout");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;

        let mut subscriber = connect_hook_bridge_websocket(bridge_port);
        subscriber
            .send(tungstenite::Message::Text(
                r#"{"method":"subscribe","params":{"channels":["art_hook"]}}"#.to_owned(),
            ))
            .expect("send subscribe");
        let subscribe_response = read_hook_bridge_json(&mut subscriber);
        assert_eq!(subscribe_response["type"], "success");
        assert_eq!(subscribe_response["data"]["subscribed"], true);

        let mut publisher = connect_hook_bridge_websocket(bridge_port);
        publisher
            .send(tungstenite::Message::Text(
                r#"{"method":"art_loom/instantiate_workflow","params":{"nodes":[{"id":"prompt"}],"edges":[{"source":"prompt","target":"out"}],"mode":"reference","workflow_id":"wf-broadcast"}}"#.to_owned(),
            ))
            .expect("send instantiate workflow");
        let publish_response = read_hook_bridge_json(&mut publisher);
        assert_eq!(publish_response["type"], "success");

        let broadcast = read_hook_bridge_json(&mut subscriber);
        assert_eq!(broadcast["method"], "art_hook/instantiate");
        assert_eq!(broadcast["params"]["workflow_id"], "wf-broadcast");
        assert_eq!(broadcast["params"]["nodes"][0]["id"], "prompt");
        assert_eq!(broadcast["params"]["edges"][0]["target"], "out");

        let running = hook_bridge_status_value(&runtime);
        assert!(
            running["subscribedClients"]
                .as_u64()
                .expect("subscribed clients")
                >= 1
        );

        drop(publisher);
        drop(subscriber);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup Hook Bridge fanout root");
    }

    #[test]
    fn daemon_exposes_artloom_ipc_workflow_command_aliases() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("artloom-ipc-workflow-aliases");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut subscriber = connect_hook_bridge_websocket(bridge_port);
        subscriber
            .send(tungstenite::Message::Text(
                r#"{"method":"subscribe","params":{"channels":["art_hook"]}}"#.to_owned(),
            ))
            .expect("send subscribe");
        let subscribe_response = read_hook_bridge_json(&mut subscriber);
        assert_eq!(subscribe_response["type"], "success");

        let instantiated = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/ipc/instantiate-workflow",
                    &[],
                    Some(
                        r#"{"nodes":[{"id":"compat-node"}],"edges":[{"source":"compat-node","target":"compat-output"}],"mode":"reference","workflowId":"compat-workflow"}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(instantiated["compatCommand"], "instantiate_workflow");
        assert_eq!(instantiated["type"], "success");
        assert_eq!(instantiated["method"], "art_hook/instantiate");

        let broadcast = read_hook_bridge_json(&mut subscriber);
        assert_eq!(broadcast["method"], "art_hook/instantiate");
        assert_eq!(broadcast["params"]["workflow_id"], "compat-workflow");
        assert_eq!(broadcast["params"]["nodes"][0]["id"], "compat-node");

        let executed = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/ipc/execute-art-node",
                    &[],
                    Some(
                        &serde_json::json!({
                            "nodeId": "compat-native-node",
                            "artId": "core.image.invert",
                            "inputBase64": test_png_base64(),
                            "params": {}
                        })
                        .to_string(),
                    ),
                ),
            ),
            200,
        );
        assert_eq!(executed["compatCommand"], "execute_art_node");
        assert_eq!(executed["type"], "success", "response={executed}");
        assert_eq!(executed["data"]["node_id"], "compat-native-node");
        assert_eq!(executed["data"]["success"], true);
        assert!(executed["data"]["output_base64"]
            .as_str()
            .expect("output_base64")
            .starts_with("data:image/png;base64,"));

        drop(subscriber);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup ArtLoom IPC workflow aliases root");
    }

    #[test]
    fn daemon_broadcasts_arts_updated_after_tool_and_artloom_registry_mutations() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("artloom-compat-broadcasts");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;

        let mut subscriber = connect_hook_bridge_websocket(bridge_port);
        subscriber
            .send(tungstenite::Message::Text(
                r#"{"method":"subscribe","params":{"channels":["art_loom/arts_updated"]}}"#
                    .to_owned(),
            ))
            .expect("send subscribe");
        let subscribe_response = read_hook_bridge_json(&mut subscriber);
        assert_eq!(subscribe_response["type"], "success");
        assert_eq!(subscribe_response["data"]["subscribed"], true);

        let explicit_broadcast = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/broadcast-updated",
                    &[],
                    Some("{}"),
                ),
            ),
            200,
        );
        assert_eq!(
            explicit_broadcast["compatCommand"],
            "broadcast_arts_updated"
        );
        assert_eq!(explicit_broadcast["broadcasted"], true);
        let explicit_broadcast_event = read_hook_bridge_json(&mut subscriber);
        assert_eq!(explicit_broadcast_event["method"], "art_loom/arts_updated");

        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/native-tool",
                    &[],
                    Some(
                        r#"{"id":"native-tool","name":"Native Tool","description":"broadcast fixture","enabled":true,"execution":{"type":"cli_wrapper","command":"echo","args":["native"]}}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["tool"]["id"], "native-tool");
        let put_broadcast = read_hook_bridge_json(&mut subscriber);
        assert_eq!(put_broadcast["method"], "art_loom/arts_updated");

        let sync_body = serde_json::json!({
            "arts": [{
                "id": "compat-art",
                "label": "Compat Art",
                "description": "broadcast compat fixture",
                "enabled": true,
                "execution": { "type": "cli_wrapper", "command": "echo", "args": ["ok"] },
                "params": [{ "id": "strength", "default": 0.1 }]
            }]
        })
        .to_string();
        let imported = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/sync",
                    &[],
                    Some(&sync_body),
                ),
            ),
            200,
        );
        assert_eq!(imported["compatCommand"], "sync_user_arts");
        let import_broadcast = read_hook_bridge_json(&mut subscriber);
        assert_eq!(import_broadcast["method"], "art_loom/arts_updated");

        let disabled = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/arts/compat-art/disable",
                    &[],
                    Some("{}"),
                ),
            ),
            200,
        );
        assert_eq!(disabled["compatCommand"], "disable_art");
        let disable_broadcast = read_hook_bridge_json(&mut subscriber);
        assert_eq!(disable_broadcast["method"], "art_loom/arts_updated");

        let defaults = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/artloom-compat/arts/compat-art/defaults",
                    &[],
                    Some(r#"{"defaults":{"strength":0.8}}"#),
                ),
            ),
            200,
        );
        assert_eq!(defaults["compatCommand"], "update_art_defaults");
        let defaults_broadcast = read_hook_bridge_json(&mut subscriber);
        assert_eq!(defaults_broadcast["method"], "art_loom/arts_updated");

        let mirrored = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("POST", "/v1/artloom-compat/arts/sync", &[], Some("{}")),
            ),
            200,
        );
        assert_eq!(mirrored["compatCommand"], "sync_user_arts");
        let mirror_broadcast = read_hook_bridge_json(&mut subscriber);
        assert_eq!(mirror_broadcast["method"], "art_loom/arts_updated");

        let deleted_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("DELETE", "/v1/tools/native-tool", &[], None),
            ),
            200,
        );
        assert_eq!(deleted_tool["deleted"], true);
        let delete_broadcast = read_hook_bridge_json(&mut subscriber);
        assert_eq!(delete_broadcast["method"], "art_loom/arts_updated");

        drop(subscriber);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup broadcast root");
    }

    #[test]
    fn daemon_hook_bridge_filters_broadcasts_by_subscribed_channel() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("hook-bridge-channel-filter");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;

        let mut subscriber = connect_hook_bridge_websocket(bridge_port);
        subscriber
            .send(tungstenite::Message::Text(
                r#"{"method":"subscribe","params":{"channels":["art_hook/instantiate"]}}"#
                    .to_owned(),
            ))
            .expect("send subscribe");
        let subscribe_response = read_hook_bridge_json(&mut subscriber);
        assert_eq!(subscribe_response["type"], "success");
        assert_eq!(subscribe_response["data"]["subscribed"], true);

        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/filter-art",
                    &[],
                    Some(
                        r#"{"id":"filter-art","name":"Filter Art","description":"channel filter fixture","enabled":true,"execution":{"type":"cli_wrapper","command":"echo","args":["ok"]},"params":[{"id":"strength","default":0.1}]}"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["tool"]["id"], "filter-art");

        subscriber
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(400)))
            .expect("shrink websocket read timeout");
        let read_result = subscriber.read();
        match read_result {
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            other => panic!("expected timeout without unrelated broadcast, got {other:?}"),
        }

        drop(subscriber);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup channel filter root");
    }

    #[test]
    fn daemon_hook_bridge_executes_mcp_backed_art_node() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("mcp-art-node");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let fixture = current_test_binary_mcp_fixture_config();

        let saved_server = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/mcp/servers/fixture",
                    &[],
                    Some(&fixture.to_string()),
                ),
            ),
            200,
        );
        assert_eq!(saved_server["server"]["id"], "fixture");

        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/fixture-art",
                    &[],
                    Some(
                        r#"{
              "id": "fixture-art",
              "name": "Fixture Art",
              "description": "Execute fixture MCP through Hook bridge",
              "enabled": true,
              "execution": {
                "type": "mcp",
                "serverId": "fixture",
                "toolName": "echo"
              }
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-art");

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"art_loom/execute_art_node","params":{"node_id":"node-mcp","art_id":"fixture-art","input_base64":"data:text/plain;base64,aW5wdXQ=","params":{"text":"execute art node runtime"}}}"#.to_owned(),
            ))
            .expect("send execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-mcp");
        assert_eq!(response["data"]["output_text"], "execute art node runtime");

        drop(socket);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup mcp art node root");
    }

    #[test]
    fn daemon_hook_bridge_executes_mcp_image_search_art_node_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("mcp-image-search-art-node");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let image_data = test_png_base64();
        let image_fixture = HttpImageFixture::start(
            "image/png",
            loom_image_io::decode_data_url_bytes(&image_data).expect("decode test image"),
        );
        let fixture = current_test_binary_mcp_fixture_config_with_env(&[(
            "LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL",
            image_fixture.url("/fixture.png"),
        )]);

        let saved_server = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/mcp/servers/fixture",
                    &[],
                    Some(&fixture.to_string()),
                ),
            ),
            200,
        );
        assert_eq!(saved_server["server"]["id"], "fixture");

        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/fixture-image-search-art",
                    &[],
                    Some(
                        r#"{
              "id": "fixture-image-search-art",
              "name": "图片搜索",
              "description": "Execute fixture MCP image search through Hook bridge",
              "enabled": true,
              "execution": {
                "type": "mcp",
                "serverId": "fixture",
                "toolName": "brave_image_search"
              },
              "outputs": [
                {
                  "name": "output",
                  "label": "output",
                  "type": "image",
                  "execution_type": "image_buffer"
                }
              ]
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-image-search-art");

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-mcp-image-search",
                        "art_id": "fixture-image-search-art",
                        "params": {
                            "query": "fixture cat",
                            "count": 1,
                            "safesearch": "off",
                            "spellcheck": true
                        }
                    }
                })
                .to_string(),
            ))
            .expect("send MCP image-search execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-mcp-image-search");
        assert_eq!(response["data"]["output_base64"], image_data);

        drop(socket);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup mcp image-search art node root");
    }

    #[test]
    fn daemon_hook_canvas_surfaces_mcp_image_search_candidates_and_selection() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_hook_canvas_runtime_state();
        let root = unique_temp_dir("mcp-image-search-canvas");
        let appdata = unique_temp_dir("mcp-image-search-canvas-appdata");
        let previous_appdata = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let first_image_data = test_png_base64();
        let second_image_data =
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAEElEQVR4AQEFAPr/AAoUHv8BpAE8tOS4KAAAAABJRU5ErkJggg==";
        let first_fixture = HttpImageFixture::start(
            "image/png",
            loom_image_io::decode_data_url_bytes(&first_image_data).expect("decode first image"),
        );
        let second_fixture = HttpImageFixture::start(
            "image/png",
            loom_image_io::decode_data_url_bytes(second_image_data).expect("decode second image"),
        );
        let fixture = current_test_binary_mcp_fixture_config_with_env(&[
            (
                "LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL",
                first_fixture.url("/fixture-a.png"),
            ),
            (
                "LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL_ALT",
                second_fixture.url("/fixture-b.png"),
            ),
        ]);

        let saved_server = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/mcp/servers/fixture",
                    &[],
                    Some(&fixture.to_string()),
                ),
            ),
            200,
        );
        assert_eq!(saved_server["server"]["id"], "fixture");

        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/fixture-image-search-art-canvas",
                    &[],
                    Some(
                        r#"{
              "id": "fixture-image-search-art-canvas",
              "name": "图片搜索",
              "description": "Execute fixture MCP image search for Hook canvas state",
              "enabled": true,
              "execution": {
                "type": "mcp",
                "serverId": "fixture",
                "toolName": "brave_image_search"
              },
              "outputs": [
                {
                  "name": "output",
                  "label": "output",
                  "type": "image",
                  "execution_type": "image_buffer"
                }
              ],
              "params": [
                { "id": "query", "default": "fixture cat" },
                { "id": "count", "default": 2 },
                { "id": "result_index", "default": 0 }
              ]
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-image-search-art-canvas");

        let overwrite = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": "art_loom/overwrite_workflow",
                "params": {
                    "workflow_id": HOOK_LIVE_WORKFLOW_ID,
                    "snapshot": {
                        "name": "Hook Live",
                        "nodes": [
                            {
                                "id": "image-search-node",
                                "type": "art",
                                "position": { "x": 160, "y": 40 },
                                "measured": { "width": 90, "height": 90 },
                                "data": {
                                    "artId": "fixture-image-search-art-canvas",
                                    "params": {
                                        "query": "fixture cat",
                                        "count": 2
                                    },
                                    "w": 90,
                                    "h": 90
                                }
                            }
                        ],
                        "edges": []
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(overwrite["type"], "success", "response={overwrite}");

        let success = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": "art_loom/execute_art_node",
                "params": {
                    "node_id": "image-search-node",
                    "art_id": "fixture-image-search-art-canvas",
                    "params": {
                        "query": "fixture cat",
                        "count": 2,
                        "result_index": 1
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(success["type"], "success", "response={success}");
        assert_eq!(success["data"]["output_base64"], second_image_data);

        let after_snapshot = expect_json_result_response(hook_canvas_snapshot(), 200);
        assert_eq!(after_snapshot["nodes"][0]["selectedResultIndex"], 1);
        assert_eq!(
            after_snapshot["nodes"][0]["resultCandidates"][0]["index"],
            0
        );
        assert_eq!(
            after_snapshot["nodes"][0]["resultCandidates"][1]["index"],
            1
        );
        assert_eq!(after_snapshot["nodes"][0]["previewAvailable"], true);

        clear_hook_canvas_runtime_state();

        let persisted_snapshot = expect_json_result_response(hook_canvas_snapshot(), 200);
        assert_eq!(persisted_snapshot["available"], true);
        assert_eq!(persisted_snapshot["nodes"][0]["id"], "image-search-node");
        assert_eq!(persisted_snapshot["nodes"][0]["params"]["result_index"], 1);
        assert_eq!(persisted_snapshot["nodes"][0]["selectedResultIndex"], 1);
        assert_eq!(
            persisted_snapshot["nodes"][0]["resultCandidates"][0]["index"],
            0
        );
        assert_eq!(
            persisted_snapshot["nodes"][0]["resultCandidates"][1]["index"],
            1
        );
        assert_eq!(persisted_snapshot["nodes"][0]["previewAvailable"], true);

        let persisted_preview = expect_binary_route_response(
            hook_canvas_preview_response("image-search-node").expect("persisted preview"),
            200,
            "image/png",
        );
        assert_eq!(
            persisted_preview,
            loom_image_io::decode_data_url_bytes(second_image_data)
                .expect("decode persisted preview"),
        );

        restore_env("APPDATA", previous_appdata);
        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup mcp image search canvas root");
        fs::remove_dir_all(appdata).expect("cleanup mcp image search canvas appdata");
    }

    #[test]
    fn daemon_artloom_update_workflow_node_route_persists_live_hook_params() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_hook_canvas_runtime_state();
        let root = unique_temp_dir("hook-live-update-workflow-node");
        let appdata = unique_temp_dir("hook-live-update-workflow-node-appdata");
        let previous_appdata = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &appdata);
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let overwrite = run_hook_bridge_text(
            &runtime,
            &json!({
                "method": "art_loom/overwrite_workflow",
                "params": {
                    "workflow_id": HOOK_LIVE_WORKFLOW_ID,
                    "snapshot": {
                        "name": "Hook Live",
                        "nodes": [
                            {
                                "id": "image-search-node",
                                "type": "art",
                                "position": { "x": 160, "y": 40 },
                                "measured": { "width": 90, "height": 90 },
                                "data": {
                                    "artId": "fixture-image-search-art-canvas",
                                    "params": {
                                        "query": "fixture cat",
                                        "count": 2
                                    },
                                    "w": 90,
                                    "h": 90
                                }
                            }
                        ],
                        "edges": []
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(overwrite["type"], "success", "response={overwrite}");

        let updated = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/ipc/update-workflow-node",
                    &[],
                    Some(
                        r#"{
                            "workflowId": "hook-live",
                            "nodeId": "image-search-node",
                            "param": "result_index",
                            "value": 1
                        }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(updated["compatCommand"], "update_workflow_node");
        assert_eq!(updated["type"], "success");

        clear_hook_canvas_runtime_state();

        let persisted_snapshot = expect_json_result_response(hook_canvas_snapshot(), 200);
        assert_eq!(persisted_snapshot["available"], true);
        assert_eq!(persisted_snapshot["nodes"][0]["params"]["result_index"], 1);
        assert_eq!(persisted_snapshot["nodes"][0]["selectedResultIndex"], 1);

        restore_env("APPDATA", previous_appdata);
        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup hook live update workflow node root");
        fs::remove_dir_all(appdata).expect("cleanup hook live update workflow node appdata");
    }

    #[test]
    fn daemon_hook_bridge_executes_ahrp_process_through_mcp_tool() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("mcp-ahrp-process");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let fixture = current_test_binary_mcp_fixture_config();
        let image_data = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";

        let saved_server = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/mcp/servers/fixture",
                    &[],
                    Some(&fixture.to_string()),
                ),
            ),
            200,
        );
        assert_eq!(saved_server["server"]["id"], "fixture");

        let saved_tool = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/tools/fixture-process-art",
                    &[],
                    Some(
                        r#"{
              "id": "fixture-process-art",
              "name": "Fixture Process Art",
              "description": "Execute fixture MCP through AHRP process",
              "enabled": true,
              "execution": {
                "type": "mcp",
                "serverId": "fixture",
                "toolName": "echo"
              }
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-process-art");

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-ahrp-mcp",
                        "art_id": "fixture-process-art",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {
                            "text": image_data,
                            "ignored": "remove me"
                        },
                        "disabled_params": ["ignored"]
                    }
                })
                .to_string(),
            ))
            .expect("send ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["request_id"], "req-ahrp-mcp");
        assert_eq!(response["status"], "Success");
        assert_eq!(response["data"]["type"], "result");
        assert_eq!(response["data"]["output"]["type"], "base64");
        assert_eq!(response["data"]["output"]["data"], image_data);
        assert_eq!(response["data"]["output"]["width"], 1);
        assert_eq!(response["data"]["output"]["height"], 1);
        assert!(response["data"]["processing_time_ms"].as_u64().is_some());

        drop(socket);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup mcp ahrp process root");
    }

    #[test]
    fn daemon_hook_bridge_executes_ahrp_process_mcp_image_search_with_legacy_hook_params() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("mcp-ahrp-process-legacy-image-search");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let image_data = test_png_base64();
        let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let alternate_image_data =
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAEElEQVR4AQEFAPr/AAoUHv8BpAE8tOS4KAAAAABJRU5ErkJggg==";
        let alternate_image_fixture = HttpImageFixture::start(
            "image/png",
            loom_image_io::decode_data_url_bytes(alternate_image_data)
                .expect("decode alternate image"),
        );
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let fixture = current_test_binary_mcp_fixture_config_with_env(&[
            (
                "LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL",
                image_fixture.url("/fixture-a.png"),
            ),
            (
                "LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL_ALT",
                alternate_image_fixture.url("/fixture-b.png"),
            ),
        ]);
        http_json_put(
            address.port(),
            "/v1/mcp/servers/fixture",
            &fixture.to_string(),
        );

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-process-image-search-legacy",
            &serde_json::json!({
                "id": "fixture-process-image-search-legacy",
                "name": "Fixture Process Image Search Legacy",
                "description": "Execute MCP image search through the real Hook art/process path with legacy Hook params",
                "enabled": true,
                "execution": {
                    "type": "mcp",
                    "serverId": "fixture",
                    "toolName": "brave_image_search"
                },
                "outputs": [
                    {
                        "name": "output",
                        "label": "output",
                        "type": "image",
                        "execution_type": "image_buffer"
                    }
                ]
            })
            .to_string(),
        );
        assert_eq!(
            saved_tool["tool"]["id"],
            "fixture-process-image-search-legacy"
        );

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-ahrp-mcp-legacy-image-search",
                        "art_id": "fixture-process-image-search-legacy",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {
                            "query": "fixture cat",
                            "count": "2",
                            "search_lang": "ZH",
                            "spellcheck": "true"
                        },
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send legacy hook mcp image search ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["request_id"], "req-ahrp-mcp-legacy-image-search");
        assert_eq!(response["status"], "Success", "response={response}");
        assert_eq!(response["data"]["type"], "result", "response={response}");
        assert_eq!(
            response["data"]["output"]["type"], "base64",
            "response={response}"
        );
        assert_eq!(
            response["data"]["output"]["data"],
            fixture_image_base64(),
            "response={response}"
        );
        assert_eq!(
            response["data"]["loomMetadata"]["imageSearch"]["selectedIndex"], 0,
            "response={response}"
        );
        assert_eq!(
            response["data"]["loomMetadata"]["imageSearch"]["candidates"]
                .as_array()
                .map(Vec::len),
            Some(2),
            "response={response}"
        );
        assert_eq!(
            response["data"]["loomMetadata"]["imageSearch"]["candidates"][0]["imageUrl"],
            image_fixture.url("/fixture-a.png"),
            "response={response}"
        );
        assert_eq!(
            response["data"]["loomMetadata"]["imageSearch"]["candidates"][1]["imageUrl"],
            alternate_image_fixture.url("/fixture-b.png"),
            "response={response}"
        );
        assert_eq!(
            response["data"]["loomMetadata"]["candidates"]["kind"], "image.candidates",
            "response={response}"
        );
        assert_eq!(
            response["data"]["loomMetadata"]["candidates"]["items"]
                .as_array()
                .map(Vec::len),
            Some(2),
            "response={response}"
        );

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup mcp legacy image search ahrp process root");
    }

    #[test]
    fn daemon_hook_bridge_reports_friendly_message_when_mcp_image_search_returns_no_images() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("mcp-ahrp-process-empty-image-search");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let image_data = test_png_base64();
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let fixture = current_test_binary_mcp_fixture_config();
        http_json_put(
            address.port(),
            "/v1/mcp/servers/fixture",
            &fixture.to_string(),
        );
        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-process-image-search-empty",
            &serde_json::json!({
                "id": "fixture-process-image-search-empty",
                "name": "Fixture Process Image Search Empty",
                "description": "Surface a friendly message when image search yields no usable image",
                "enabled": true,
                "execution": {
                    "type": "mcp",
                    "serverId": "fixture",
                    "toolName": "brave_image_search"
                },
                "outputs": [
                    {
                        "name": "output",
                        "label": "output",
                        "type": "image",
                        "execution_type": "image_buffer"
                    }
                ]
            })
            .to_string(),
        );
        assert_eq!(
            saved_tool["tool"]["id"],
            "fixture-process-image-search-empty"
        );

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-ahrp-mcp-empty-image-search",
                        "art_id": "fixture-process-image-search-empty",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {
                            "query": "offensive fixture",
                            "count": "3",
                            "search_lang": "ZH",
                            "spellcheck": "true"
                        },
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send empty-image MCP image search ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["request_id"], "req-ahrp-mcp-empty-image-search");
        assert_eq!(response["status"], "EngineError", "response={response}");
        assert_eq!(
            response["error"],
            "图片搜索未返回可用结果：搜索服务将该查询判定为可能敏感，请尝试更换关键词。",
            "response={response}"
        );

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup empty-image mcp image search root");
    }

    #[test]
    fn daemon_hook_bridge_retains_image_search_candidates_when_mcp_candidate_download_fails() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("mcp-ahrp-process-candidate-download-failure");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let image_data = test_png_base64();
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let fixture = current_test_binary_mcp_fixture_config_with_env(&[(
            "LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL",
            "http://127.0.0.1:9/broken.jpg".to_owned(),
        )]);
        http_json_put(
            address.port(),
            "/v1/mcp/servers/fixture",
            &fixture.to_string(),
        );
        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-process-image-search-download-failure",
            &serde_json::json!({
                "id": "fixture-process-image-search-download-failure",
                "name": "Fixture Process Image Search Download Failure",
                "description": "Keep image-search candidates in the Hook bridge error payload when Loom cannot download them server-side",
                "enabled": true,
                "execution": {
                    "type": "mcp",
                    "serverId": "fixture",
                    "toolName": "brave_image_search"
                },
                "outputs": [
                    {
                        "name": "output",
                        "label": "output",
                        "type": "image",
                        "execution_type": "image_buffer"
                    }
                ]
            })
            .to_string(),
        );
        assert_eq!(
            saved_tool["tool"]["id"],
            "fixture-process-image-search-download-failure"
        );

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-ahrp-mcp-candidate-download-failure",
                        "art_id": "fixture-process-image-search-download-failure",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {
                            "query": "fixture cat",
                            "count": "1",
                            "search_lang": "ZH",
                            "spellcheck": "true"
                        },
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send candidate-download-failure MCP image search ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(
            response["request_id"],
            "req-ahrp-mcp-candidate-download-failure"
        );
        assert_eq!(response["status"], "EngineError", "response={response}");
        assert_eq!(
            response["error"], "图片搜索已返回候选结果，但图片下载失败，请稍后重试。",
            "response={response}"
        );
        assert_eq!(
            response["data"]["loomMetadata"]["imageSearch"]["candidates"][0]["imageUrl"],
            "http://127.0.0.1:9/broken.jpg",
            "response={response}"
        );

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup candidate-download-failure mcp image search root");
    }

    #[test]
    fn daemon_registered_tool_executes_realshape_mcp_image_search_with_legacy_hook_params() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("mcp-direct-legacy-image-search-realshape");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let fixture = current_test_binary_mcp_fixture_config_with_env(&[(
            "LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL",
            image_fixture.url("/fixture.png"),
        )]);
        http_json_put(
            address.port(),
            "/v1/mcp/servers/fixture",
            &fixture.to_string(),
        );

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-image-search-realshape",
            &serde_json::json!({
                "id": "fixture-image-search-realshape",
                "name": "Fixture Image Search Realshape",
                "description": "Execute MCP image search through the direct tool route with Brave-like string-only search_lang schema",
                "enabled": true,
                "execution": {
                    "type": "mcp",
                    "serverId": "fixture",
                    "toolName": "brave_image_search_realshape"
                },
                "outputs": [
                    {
                        "name": "output",
                        "label": "output",
                        "type": "image",
                        "execution_type": "image_buffer"
                    }
                ]
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-image-search-realshape");

        let executed = http_json_post(
            address.port(),
            "/v1/tools/fixture-image-search-realshape/execute",
            r#"{"arguments":{"query":"fixture cat","count":"1","search_lang":"ZH","spellcheck":"true"}}"#,
        );
        assert_eq!(executed["toolId"], "fixture-image-search-realshape");
        assert_eq!(executed["status"], "succeeded");
        assert_eq!(executed["result"]["content"][0]["type"], "image");
        assert_eq!(
            executed["result"]["content"][0]["data"],
            fixture_image_base64()
        );

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup mcp direct legacy image search realshape root");
    }

    #[test]
    fn daemon_hook_bridge_executes_cli_wrapper_art_node_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("cli-wrapper-art-node");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let image_data = test_png_base64();

        #[cfg(windows)]
        let execution = serde_json::json!({
            "type": "cli_wrapper",
            "command": "powershell.exe",
            "args": [
                "-NoProfile",
                "-Command",
                "Copy-Item -LiteralPath '{{input}}' -Destination '{{output}}' -Force"
            ]
        });
        #[cfg(not(windows))]
        let execution = serde_json::json!({
            "type": "cli_wrapper",
            "command": "sh",
            "args": [
                "-c",
                "cp \"$1\" \"$2\"",
                "loom-cli-wrapper",
                "{{input}}",
                "{{output}}"
            ]
        });

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-cli-wrapper-art",
            &serde_json::json!({
                "id": "fixture-cli-wrapper-art",
                "name": "Fixture CLI Wrapper Art",
                "description": "Execute fixture cli_wrapper Art through Hook bridge",
                "enabled": true,
                "execution": execution
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-cli-wrapper-art");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-cli-wrapper",
                        "art_id": "fixture-cli-wrapper-art",
                        "input_base64": image_data,
                        "params": {}
                    }
                })
                .to_string(),
            ))
            .expect("send cli_wrapper execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-cli-wrapper");
        assert_eq!(response["data"]["output_base64"], image_data);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup cli wrapper art node root");
    }

    #[test]
    fn daemon_hook_bridge_executes_python_art_art_node_text_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let previous_runtime_root = std::env::var("LOOM_FRAMEWORK_RUNTIMES_DIR").ok();
        let root = unique_temp_dir("python-art-node");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        mark_framework_installed(&root, "python_art");
        let runtime_root = provision_test_python_art_runtime(&root);
        let art_path = write_daemon_python_art_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-python-art-node",
            &serde_json::json!({
                "id": "fixture-python-art-node",
                "name": "Fixture Python Art Node",
                "description": "Execute fixture python_art through Hook bridge",
                "enabled": true,
                "execution": {
                    "type": "python_art",
                    "artId": "fixture_python_art",
                    "artPath": art_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-python-art-node");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-python-art",
                        "art_id": "fixture-python-art-node",
                        "params": {
                            "text": "hook python art"
                        }
                    }
                })
                .to_string(),
            ))
            .expect("send python_art execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-python-art");
        assert_eq!(
            response["data"]["output_text"],
            "python art saw hook python art"
        );
        assert!(
            runtime_root
                .join("python-embed")
                .join("python.exe")
                .is_file(),
            "python_art runtime marker missing"
        );

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        restore_env("LOOM_FRAMEWORK_RUNTIMES_DIR", previous_runtime_root);
        fs::remove_dir_all(root).expect("cleanup python art node root");
    }

    #[test]
    fn daemon_hook_bridge_executes_native_art_node_without_registry_tool() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("native-art-node");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let image_data = test_png_base64();

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-native",
                        "art_id": "core.image.invert",
                        "input_base64": image_data,
                        "params": {}
                    }
                })
                .to_string(),
            ))
            .expect("send native execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-native");
        assert!(response["data"]["output_base64"]
            .as_str()
            .expect("native output")
            .starts_with("data:image/png;base64,"));

        drop(socket);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup native art node root");
    }

    #[test]
    fn daemon_hook_bridge_executes_native_art_ahrp_process_without_registry_tool() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("native-ahrp-process");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let image_data = test_png_base64();

        let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-native-process",
                        "art_id": "core.image.invert",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {},
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send native ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(
            response["request_id"], "req-native-process",
            "response={response}"
        );
        assert_eq!(response["status"], "Success");
        assert_eq!(response["data"]["type"], "result");
        assert_eq!(response["data"]["output"]["type"], "base64");
        assert!(response["data"]["output"]["data"]
            .as_str()
            .expect("native ahrp output")
            .starts_with("data:image/png;base64,"));
        assert_eq!(response["data"]["output"]["width"], 1);
        assert_eq!(response["data"]["output"]["height"], 1);

        drop(socket);
        let stopped = stop_test_hook_bridge(&runtime);
        assert_eq!(stopped["running"], false);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup native ahrp process root");
    }

    #[test]
    fn daemon_exposes_artloom_native_process_art_command_alias() {
        let root = unique_temp_dir("native-process-art-alias");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let image_data = test_png_base64();

        let response = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/artloom-compat/native/process-art",
                    &[],
                    Some(
                        &serde_json::json!({
                            "artId": "core.image.invert",
                            "inputBase64": image_data,
                            "params": {}
                        })
                        .to_string(),
                    ),
                ),
            ),
            200,
        );

        assert_eq!(response["compatCommand"], "native_process_art");
        assert_eq!(response["success"], true);
        assert!(response["output_base64"]
            .as_str()
            .expect("native compat output")
            .starts_with("data:image/png;base64,"));
        assert!(response["error"].is_null());
        assert!(response["processing_time_ms"].as_u64().is_some());

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup native compat root");
    }

    #[test]
    fn daemon_shared_image_api_create_list_get_delete_contract() {
        let root = unique_temp_dir("shared-images-api");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let created = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/shared-images",
                    &[],
                    Some(r#"{"width":1,"height":1,"format":"rgba8","data":[10,20,30,255]}"#),
                ),
            ),
            200,
        );
        let handle = created["image"]["handle"]
            .as_str()
            .expect("created shared image handle")
            .to_owned();

        assert_eq!(created["image"]["size"], 4);
        assert_eq!(created["image"]["width"], 1);
        assert_eq!(created["image"]["height"], 1);
        assert_eq!(created["image"]["format"], "rgba8");

        let listed = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/shared-images", &[], None),
            ),
            200,
        );
        assert_eq!(listed["images"].as_array().expect("images").len(), 1);
        assert_eq!(listed["images"][0]["handle"], handle);

        let fetched = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", &format!("/v1/shared-images/{handle}"), &[], None),
            ),
            200,
        );
        assert_eq!(fetched["image"]["handle"], handle);
        assert_eq!(fetched["data"], serde_json::json!([10, 20, 30, 255]));
        assert!(fetched["dataBase64"]
            .as_str()
            .expect("png data URL")
            .starts_with("data:image/png;base64,"));

        let deleted = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("DELETE", &format!("/v1/shared-images/{handle}"), &[], None),
            ),
            200,
        );
        assert_eq!(deleted["deleted"], true);
        let listed = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/shared-images", &[], None),
            ),
            200,
        );
        assert!(listed["images"].as_array().expect("images").is_empty());

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup shared images root");
    }

    #[test]
    fn daemon_image_helper_converts_base64_to_rgba_buffer() {
        let root = unique_temp_dir("image-helper-base64");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let response = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/image-helpers/convert",
                    &[],
                    Some(
                        &serde_json::json!({
                            "sourceType": "image_base64",
                            "targetType": "image_buffer",
                            "data": test_png_base64()
                        })
                        .to_string(),
                    ),
                ),
            ),
            200,
        );

        assert_eq!(response["image"]["width"], 1);
        assert_eq!(response["image"]["height"], 1);
        assert_eq!(response["image"]["format"], "rgba8");
        assert_eq!(response["image"]["size"], 4);
        assert_eq!(response["data"], serde_json::json!([10, 20, 30, 255]));

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup image helper base64 root");
    }

    #[test]
    fn daemon_image_helper_converts_path_to_base64() {
        let root = unique_temp_dir("image-helper-path");
        let image_path = root.join("pixel.png");
        let data_url = test_png_base64();
        fs::write(
            &image_path,
            BASE64
                .decode(data_url.split_once(',').expect("data URL").1)
                .expect("decode test png"),
        )
        .expect("write image fixture");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let response = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "POST",
                    "/v1/image-helpers/convert",
                    &[],
                    Some(
                        &serde_json::json!({
                            "sourceType": "image_path",
                            "targetType": "image_base64",
                            "path": image_path.display().to_string()
                        })
                        .to_string(),
                    ),
                ),
            ),
            200,
        );

        assert!(response["dataBase64"]
            .as_str()
            .expect("data URL")
            .starts_with("data:image/png;base64,"));
        let rgba = loom_image_io::decode_image_base64_to_rgba8(
            response["dataBase64"].as_str().expect("data URL"),
        )
        .expect("decode converted path");
        assert_eq!(rgba.data, vec![10, 20, 30, 255]);

        drop(runtime);
        fs::remove_dir_all(root).expect("cleanup image helper root");
    }

    #[test]
    fn daemon_hook_bridge_ocr_image_fixture_provider_returns_success() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_fixture = std::env::var("LOOM_OCR_FIXTURE_TEXT").ok();
        std::env::set_var("LOOM_OCR_FIXTURE_TEXT", "hello loom ocr");
        let root = unique_temp_dir("ocr-fixture");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

        let capabilities = run_hook_bridge_text(
            &runtime,
            &serde_json::json!({
                "method": "art_loom/get_capabilities"
            })
            .to_string(),
        );
        assert_eq!(capabilities["data"]["ocr"], true);

        let response = run_hook_bridge_text(
            &runtime,
            &serde_json::json!({
                "method": "art_loom/ocr_image",
                "params": {
                    "image_base64": test_png_base64()
                }
            })
            .to_string(),
        );

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["fullText"], "hello loom ocr");
        assert_eq!(response["data"]["width"], 1);
        assert_eq!(response["data"]["height"], 1);
        assert_eq!(response["data"]["textBlocks"][0]["text"], "hello loom ocr");

        drop(runtime);
        restore_env("LOOM_OCR_FIXTURE_TEXT", previous_fixture);
        fs::remove_dir_all(root).expect("cleanup ocr fixture root");
    }

    #[test]
    fn daemon_hook_bridge_translate_text_uses_configured_provider() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let fixture = TranslateFixture::start();
        let previous_endpoint = std::env::var("LOOM_TRANSLATE_ENDPOINT").ok();
        std::env::set_var("LOOM_TRANSLATE_ENDPOINT", fixture.url("/translate"));
        let root = unique_temp_dir("translate-provider");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let response = run_hook_bridge_text(
            &runtime,
            &serde_json::json!({
                "method": "art_loom/translate_text",
                "params": {
                    "text": "hello loom",
                    "target_lang": "zh"
                }
            })
            .to_string(),
        );

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(
            response["data"]["translated_text"],
            "translated:hello loom:zh"
        );
        assert_eq!(response["data"]["source"], "loom-translate-provider");
        let request = fixture.request();
        assert!(request.starts_with("POST /translate HTTP/1.1"));
        assert!(request.contains(r#""text":"hello loom""#));
        assert!(request.contains(r#""target_lang":"zh""#));

        drop(runtime);
        restore_env("LOOM_TRANSLATE_ENDPOINT", previous_endpoint);
        fs::remove_dir_all(root).expect("cleanup translate provider root");
    }

    #[test]
    #[cfg_attr(
        not(windows),
        ignore = "packaged OCR validation requires the bundled Windows ONNX Runtime"
    )]
    fn daemon_hook_bridge_ocr_image_real_provider_returns_success() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_fixture = std::env::var("LOOM_OCR_FIXTURE_TEXT").ok();
        let previous_model_dir = std::env::var("LOOM_OCR_MODEL_DIR").ok();
        std::env::remove_var("LOOM_OCR_FIXTURE_TEXT");
        std::env::set_var("LOOM_OCR_MODEL_DIR", workspace_ocr_resources());
        let root = unique_temp_dir("ocr-real");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/get_capabilities"
                })
                .to_string(),
            ))
            .expect("send capabilities request");
        let capabilities = read_hook_bridge_json(&mut socket);
        assert_eq!(capabilities["data"]["ocr"], true);

        let image_data = packaged_ocr_fixture_base64();
        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/ocr_image",
                    "params": {
                        "image_base64": image_data
                    }
                })
                .to_string(),
            ))
            .expect("send ocr request");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert!(
            !response["data"]["fullText"]
                .as_str()
                .expect("fullText")
                .trim()
                .is_empty(),
            "response={response}"
        );
        assert!(
            !response["data"]["textBlocks"]
                .as_array()
                .expect("textBlocks")
                .is_empty(),
            "response={response}"
        );
        assert_eq!(response["data"]["width"], 678);
        assert_eq!(response["data"]["height"], 108);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        restore_env("LOOM_OCR_MODEL_DIR", previous_model_dir);
        restore_env("LOOM_OCR_FIXTURE_TEXT", previous_fixture);
        fs::remove_dir_all(root).expect("cleanup ocr real root");
    }

    #[test]
    fn daemon_hook_bridge_ocr_image_unavailable_by_default() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_fixture = std::env::var("LOOM_OCR_FIXTURE_TEXT").ok();
        let previous_model_dir = std::env::var("LOOM_OCR_MODEL_DIR").ok();
        std::env::remove_var("LOOM_OCR_FIXTURE_TEXT");
        let root = unique_temp_dir("ocr-unavailable");
        let empty_model_dir = root.join("empty-ocr-models");
        fs::create_dir_all(&empty_model_dir).expect("create empty model dir");
        std::env::set_var("LOOM_OCR_MODEL_DIR", &empty_model_dir);
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let response = run_hook_bridge_text(
            &runtime,
            &serde_json::json!({
                "method": "art_loom/ocr_image",
                "params": {
                    "image_base64": test_png_base64()
                }
            })
            .to_string(),
        );

        assert_eq!(response["type"], "error");
        assert_eq!(response["data"]["message"], "OCR enhancement unavailable");

        drop(runtime);
        restore_env("LOOM_OCR_MODEL_DIR", previous_model_dir);
        restore_env("LOOM_OCR_FIXTURE_TEXT", previous_fixture);
        fs::remove_dir_all(root).expect("cleanup ocr unavailable root");
    }

    #[test]
    fn daemon_hook_bridge_executes_shared_memory_ahrp_process() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("shared-memory-ahrp-process");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let created = http_json_post(
            address.port(),
            "/v1/shared-images",
            r#"{"width":1,"height":1,"format":"rgba8","data":[10,20,30,255]}"#,
        );
        let handle = created["image"]["handle"]
            .as_str()
            .expect("shared input handle")
            .to_owned();

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-shared-memory-process",
                        "art_id": "core.image.invert",
                        "input": {
                            "type": "shared_memory",
                            "handle": handle,
                            "size": 4,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {},
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send shared memory ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["request_id"], "req-shared-memory-process");
        assert_eq!(response["status"], "Success", "response={response}");
        assert_eq!(response["data"]["type"], "result");
        assert_eq!(response["data"]["output"]["type"], "shared_memory");
        assert_eq!(response["data"]["output"]["width"], 1);
        assert_eq!(response["data"]["output"]["height"], 1);
        assert_eq!(response["data"]["output"]["size"], 4);
        assert_eq!(response["data"]["output"]["format"], "rgba8");
        let output_handle = response["data"]["output"]["handle"]
            .as_str()
            .expect("output shared memory handle");

        let output = http_json_get(
            address.port(),
            &format!("/v1/shared-images/{output_handle}"),
        );
        assert_eq!(output["data"], serde_json::json!([245, 235, 225, 255]));

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup shared memory ahrp process root");
    }

    #[test]
    fn daemon_hook_bridge_executes_script_art_node_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("script-art-node");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = write_daemon_script_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let image_data = test_png_base64();

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-art",
            &serde_json::json!({
                "id": "fixture-script-art",
                "name": "Fixture Script Art",
                "description": "Execute fixture script Art through Hook bridge",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-script-art");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-script",
                        "art_id": "fixture-script-art",
                        "input_base64": image_data,
                        "params": {}
                    }
                })
                .to_string(),
            ))
            .expect("send script execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-script");
        assert_eq!(response["data"]["output_base64"], image_data);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup script art node root");
    }

    #[test]
    fn daemon_hook_bridge_executes_script_image_blend_art_node() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("script-image-blend-art-node");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = workspace_image_blend_script_path();
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let input_image = test_color_png_base64([240, 60, 0, 255]);
        let reference_image = test_color_png_base64([40, 160, 200, 255]);

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-image-blend",
            &serde_json::json!({
                "id": "fixture-script-image-blend",
                "name": "Fixture Script Image Blend",
                "description": "Blend two images through script",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                },
                "inputs": [
                    { "name": "input", "label": "源图", "type": "image", "execution_type": "image_buffer" },
                    { "name": "reference", "label": "参考图", "type": "image", "execution_type": "image_buffer" }
                ],
                "outputs": [
                    { "name": "output", "label": "结果", "type": "image", "execution_type": "image_buffer" }
                ],
                "params": [
                    { "id": "reference", "label": "参考图", "widget": "image_link", "default": "", "data_type": "image_path", "disabled": false },
                    { "id": "mix_ratio", "label": "混合比例", "widget": "slider", "default": 25, "min": 0, "max": 100, "step": 1, "data_type": "number", "disabled": false }
                ]
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-script-image-blend");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-script-blend",
                        "art_id": "fixture-script-image-blend",
                        "input_base64": input_image,
                        "params": {
                            "reference": reference_image,
                            "mix_ratio": 25
                        }
                    }
                })
                .to_string(),
            ))
            .expect("send script image blend execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-script-blend");
        let output = loom_image_io::decode_image_base64_to_rgba8(
            response["data"]["output_base64"]
                .as_str()
                .expect("blend output_base64"),
        )
        .expect("decode script image blend output");
        assert_eq!(output.width, 1);
        assert_eq!(output.height, 1);
        assert_eq!(output.data, vec![190, 85, 50, 255]);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup script image blend art node root");
    }

    #[cfg(windows)]
    #[test]
    fn daemon_hook_bridge_executes_script_image_blend_art_node_with_large_payload_and_valid_images()
    {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("script-image-blend-art-node-large-images");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = workspace_image_blend_script_path();
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let input_image = test_color_png_base64([240, 60, 0, 255]);
        let reference_image = test_color_png_base64([40, 160, 200, 255]);
        let debug_padding = "x".repeat(40_000);

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-image-blend-large-images",
            &serde_json::json!({
                "id": "fixture-script-image-blend-large-images",
                "name": "Fixture Script Image Blend Large Images",
                "description": "Blend two large images through script",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                },
                "inputs": [
                    { "name": "input", "label": "源图", "type": "image", "execution_type": "image_buffer" },
                    { "name": "reference", "label": "参考图", "type": "image", "execution_type": "image_buffer", "exposePort": true }
                ],
                "outputs": [
                    { "name": "output", "label": "结果", "type": "image", "execution_type": "image_buffer" }
                ],
                "params": [
                    { "id": "reference", "label": "参考图", "widget": "image_link", "default": "", "data_type": "image_path", "disabled": false },
                    { "id": "mix_ratio", "label": "混合比例", "widget": "slider", "default": 50, "min": 0, "max": 100, "step": 1, "data_type": "number", "disabled": false }
                ]
            })
            .to_string(),
        );
        assert_eq!(
            saved_tool["tool"]["id"],
            "fixture-script-image-blend-large-images"
        );

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-script-blend-large",
                        "art_id": "fixture-script-image-blend-large-images",
                        "input_base64": input_image,
                        "params": {
                            "reference": reference_image,
                            "mix_ratio": 50,
                            "debug_padding": debug_padding
                        }
                    }
                })
                .to_string(),
            ))
            .expect("send large payload script blend execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-script-blend-large");
        let output = loom_image_io::decode_image_base64_to_rgba8(
            response["data"]["output_base64"]
                .as_str()
                .expect("large payload blend output_base64"),
        )
        .expect("decode large payload script image blend output");
        assert_eq!(output.width, 1);
        assert_eq!(output.height, 1);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup large valid image blend art node root");
    }

    #[cfg(windows)]
    #[test]
    fn daemon_hook_bridge_executes_script_art_node_with_large_payload() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("script-art-node-large-payload");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = write_daemon_script_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let image_data = format!("data:image/png;base64,{}", "A".repeat(40_000));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-art-large-payload",
            &serde_json::json!({
                "id": "fixture-script-art-large-payload",
                "name": "Fixture Script Art Large Payload",
                "description": "Execute fixture script Art through Hook bridge with a large payload",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-script-art-large-payload");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-script-large",
                        "art_id": "fixture-script-art-large-payload",
                        "input_base64": image_data,
                        "params": {}
                    }
                })
                .to_string(),
            ))
            .expect("send script execute art node with large payload");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-script-large");
        let output = response["data"]["output_base64"]
            .as_str()
            .expect("large payload output_base64");
        assert!(output.starts_with("data:image/png;base64,"));
        assert_eq!(output.len(), "data:image/png;base64,".len() + 40_000);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup large payload script art node root");
    }

    #[test]
    fn daemon_hook_bridge_executes_script_ahrp_process_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("script-ahrp-process");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = write_daemon_script_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let image_data = test_png_base64();

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-process",
            &serde_json::json!({
                "id": "fixture-script-process",
                "name": "Fixture Script Process",
                "description": "Execute fixture script through AHRP process",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-script-process");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-script-process",
                        "art_id": "fixture-script-process",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {},
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send script ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(
            response["request_id"], "req-script-process",
            "response={response}"
        );
        assert_eq!(response["status"], "Success");
        assert_eq!(response["data"]["type"], "result");
        assert_eq!(response["data"]["output"]["type"], "base64");
        assert_eq!(response["data"]["output"]["data"], image_data);
        assert_eq!(response["data"]["output"]["width"], 1);
        assert_eq!(response["data"]["output"]["height"], 1);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup script ahrp process root");
    }

    #[test]
    fn daemon_hook_bridge_process_uses_explicit_auxiliary_input_images_for_script_blend() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("script-ahrp-process-image-blend");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = workspace_image_blend_script_path();
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let input_image = test_color_png_base64([240, 60, 0, 255]);
        let reference_image = test_color_png_base64([40, 160, 200, 255]);

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-process-blend",
            &serde_json::json!({
                "id": "fixture-script-process-blend",
                "name": "Fixture Script Process Blend",
                "description": "Execute image blend script through the real art/process route",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                },
                "inputs": [
                    { "name": "input", "label": "源图", "type": "image", "execution_type": "image_buffer" },
                    { "name": "reference", "label": "参考图", "type": "image", "execution_type": "image_buffer", "exposePort": true }
                ],
                "outputs": [
                    { "name": "output", "label": "结果", "type": "image", "execution_type": "image_buffer" }
                ],
                "params": [
                    { "id": "reference", "label": "参考图", "widget": "image_link", "default": "", "data_type": "image_path", "disabled": false },
                    { "id": "mix_ratio", "label": "混合比例", "widget": "slider", "default": 25, "min": 0, "max": 100, "step": 1, "data_type": "number", "disabled": false }
                ]
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-script-process-blend");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-script-process-blend",
                        "art_id": "fixture-script-process-blend",
                        "input": {
                            "type": "base64",
                            "data": input_image,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {
                            "reference": "",
                            "mix_ratio": 25
                        },
                        "input_images": {
                            "reference": reference_image
                        },
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send script blend ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(
            response["request_id"], "req-script-process-blend",
            "response={response}"
        );
        assert_eq!(response["status"], "Success");
        let output = loom_image_io::decode_image_base64_to_rgba8(
            response["data"]["output"]["data"]
                .as_str()
                .expect("script blend process output data"),
        )
        .expect("decode script blend process output");
        assert_eq!(output.width, 1);
        assert_eq!(output.height, 1);
        assert_eq!(output.data, vec![190, 85, 50, 255]);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup script blend ahrp process root");
    }

    #[cfg(windows)]
    #[test]
    fn daemon_hook_bridge_process_executes_image_blend_compress_workflow_art() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("image-blend-compress-workflow-ahrp");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let workflow_yaml =
            fs::read_to_string(workspace_image_blend_compress_resource("workflow.yaml"))
                .expect("read image blend compress workflow");
        let workflow_manifest =
            fs::read_to_string(workspace_image_blend_compress_resource("manifest.json"))
                .expect("read image blend compress manifest");
        let blend_script = workspace_image_blend_script_path();
        let (compress_script, compress_evidence) = write_daemon_cli_image_copy_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let input_image = test_color_png_base64([240, 60, 0, 255]);
        let reference_image = test_color_png_base64([40, 160, 200, 255]);

        let saved_blend = http_json_put(
            address.port(),
            "/v1/tools/custom-image-blend-script",
            &serde_json::json!({
                "id": "custom-image-blend-script",
                "name": "Fixture Image Blend",
                "description": "Production image blend script child",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": blend_script.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_blend["tool"]["id"], "custom-image-blend-script");
        let saved_compress = http_json_put(
            address.port(),
            "/v1/tools/custom-1770146354922",
            &serde_json::json!({
                "id": "custom-1770146354922",
                "name": "Fixture Image Compress",
                "description": "Deterministic cli_wrapper child",
                "enabled": true,
                "execution": {
                    "type": "cli_wrapper",
                    "command": "powershell.exe",
                    "args": [
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                        compress_script.display().to_string(),
                        "-InputPath",
                        "{{input}}",
                        "-OutputPath",
                        "{{output}}",
                        "-Quality",
                        "{{quality_num}}",
                        "-EvidencePath",
                        compress_evidence.display().to_string()
                    ]
                }
            })
            .to_string(),
        );
        assert_eq!(saved_compress["tool"]["id"], "custom-1770146354922");
        let saved_workflow = http_json_put(
            address.port(),
            "/v1/workflows/image-blend-compress-workflow",
            &serde_json::json!({ "data": workflow_yaml }).to_string(),
        );
        assert_eq!(
            saved_workflow["workflow"]["id"],
            "image-blend-compress-workflow"
        );
        let saved_workflow_art = http_json_put(
            address.port(),
            "/v1/tools/custom-image-blend-compress-workflow",
            &workflow_manifest,
        );
        assert_eq!(
            saved_workflow_art["tool"]["id"],
            "custom-image-blend-compress-workflow"
        );

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);
        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-image-blend-compress-workflow",
                        "art_id": "custom-image-blend-compress-workflow",
                        "input": {
                            "type": "base64",
                            "data": input_image,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {
                            "mix_ratio": 25,
                            "quality_num": 73
                        },
                        "input_images": {
                            "reference": reference_image
                        },
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send image blend compress workflow process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(
            response["request_id"], "req-image-blend-compress-workflow",
            "response={response}"
        );
        assert_eq!(response["status"], "Success", "response={response}");
        assert_eq!(response["data"]["output"]["type"], "base64");
        let output = loom_image_io::decode_image_base64_to_rgba8(
            response["data"]["output"]["data"]
                .as_str()
                .expect("workflow process output data"),
        )
        .expect("decode workflow process output");
        assert_eq!(output.width, 1);
        assert_eq!(output.height, 1);
        assert_eq!(output.data, vec![190, 85, 50, 255]);
        assert_eq!(
            fs::read_to_string(&compress_evidence).expect("read compression evidence"),
            "73"
        );

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);
        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup image blend compress workflow root");
    }

    #[test]
    fn daemon_hook_bridge_executes_workflow_art_node_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("workflow-art-node");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = write_daemon_script_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let image_data = test_png_base64();

        let saved_script = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-art",
            &serde_json::json!({
                "id": "fixture-script-art",
                "name": "Fixture Script Art",
                "description": "Execute fixture script Art through workflow",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_script["tool"]["id"], "fixture-script-art");
        let workflow_yaml = r#"name: Workflow Art
nodes:
  - id: image
    uses: fixture-script-art
"#;
        let saved_workflow = http_json_put(
            address.port(),
            "/v1/workflows/runtime-art-flow",
            &serde_json::json!({ "data": workflow_yaml }).to_string(),
        );
        assert_eq!(saved_workflow["workflow"]["id"], "runtime-art-flow");
        let saved_workflow_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-workflow-art",
            r#"{
              "id": "fixture-workflow-art",
              "name": "Fixture Workflow Art",
              "description": "Execute fixture workflow Art",
              "enabled": true,
              "execution": {
                "type": "workflow",
                "workflowId": "runtime-art-flow"
              }
            }"#,
        );
        assert_eq!(saved_workflow_tool["tool"]["id"], "fixture-workflow-art");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-workflow",
                        "art_id": "fixture-workflow-art",
                        "input_base64": image_data,
                        "params": {}
                    }
                })
                .to_string(),
            ))
            .expect("send workflow execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-workflow");
        assert_eq!(response["data"]["output_base64"], image_data);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup workflow art node root");
    }

    #[test]
    fn daemon_hook_bridge_executes_workflow_ahrp_process_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("workflow-ahrp-process");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = write_daemon_script_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let image_data = test_png_base64();

        let saved_script = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-process",
            &serde_json::json!({
                "id": "fixture-script-process",
                "name": "Fixture Script Process",
                "description": "Execute fixture script through workflow",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_script["tool"]["id"], "fixture-script-process");
        let workflow_yaml = r#"name: Workflow AHRP
nodes:
  - id: image
    uses: fixture-script-process
"#;
        let saved_workflow = http_json_put(
            address.port(),
            "/v1/workflows/runtime-ahrp-flow",
            &serde_json::json!({ "data": workflow_yaml }).to_string(),
        );
        assert_eq!(saved_workflow["workflow"]["id"], "runtime-ahrp-flow");
        let saved_workflow_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-workflow-process",
            r#"{
              "id": "fixture-workflow-process",
              "name": "Fixture Workflow Process",
              "description": "Execute fixture workflow through AHRP process",
              "enabled": true,
              "execution": {
                "type": "workflow",
                "workflowId": "runtime-ahrp-flow"
              }
            }"#,
        );
        assert_eq!(
            saved_workflow_tool["tool"]["id"],
            "fixture-workflow-process"
        );

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-workflow-process",
                        "art_id": "fixture-workflow-process",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {},
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send workflow ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(
            response["request_id"], "req-workflow-process",
            "response={response}"
        );
        assert_eq!(response["status"], "Success");
        assert_eq!(response["data"]["type"], "result");
        assert_eq!(response["data"]["output"]["type"], "base64");
        assert_eq!(response["data"]["output"]["data"], image_data);
        assert_eq!(response["data"]["output"]["width"], 1);
        assert_eq!(response["data"]["output"]["height"], 1);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup workflow ahrp process root");
    }

    #[test]
    fn daemon_hook_bridge_executes_script_shader_text_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("script-shader");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let script_path = write_daemon_script_fixture(&root);
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-script-shader",
            &serde_json::json!({
                "id": "fixture-script-shader",
                "name": "Fixture Script Shader",
                "description": "Return shader code through script",
                "enabled": true,
                "execution": {
                    "type": "script",
                    "path": script_path.display().to_string()
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-script-shader");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                r#"{"method":"art_loom/execute_art_node","params":{"node_id":"node-shader","art_id":"fixture-script-shader","params":{"mode":"shader"}}}"#.to_owned(),
            ))
            .expect("send script shader art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-shader");
        assert_eq!(
            response["data"]["output_text"],
            "void fragment() { COLOR = vec4(1.0); }"
        );

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup script shader root");
    }

    #[test]
    fn daemon_hook_bridge_executes_cloud_api_art_node_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("cloud-art-node");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let image_data = test_png_base64();
        let fixture = CloudApiFixture::start(CloudApiFixtureMode::Image(image_data.clone()));
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-cloud-art",
            &serde_json::json!({
                "id": "fixture-cloud-art",
                "name": "Fixture Cloud Art",
                "description": "Execute fixture cloud Art through Hook bridge",
                "enabled": true,
                "execution": {
                    "type": "cloud_api",
                    "endpoint": fixture.url("/image"),
                    "method": "POST"
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-cloud-art");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-cloud",
                        "art_id": "fixture-cloud-art",
                        "input_base64": image_data,
                        "params": {}
                    }
                })
                .to_string(),
            ))
            .expect("send cloud execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-cloud");
        assert_eq!(response["data"]["output_base64"], image_data);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup cloud art node root");
    }

    #[test]
    fn daemon_hook_bridge_executes_cloud_api_multipart_art_node_with_input_file() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("cloud-multipart-art-node");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let image_data = test_png_base64();
        let fixture =
            CloudApiFixture::start(CloudApiFixtureMode::MultipartImage(image_data.clone()));
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-cloud-multipart-art",
            &serde_json::json!({
                "id": "fixture-cloud-multipart-art",
                "name": "Fixture Cloud Multipart Art",
                "description": "Execute old ArtLoom-style multipart cloud Art through Hook bridge",
                "enabled": true,
                "execution": {
                    "type": "cloud_api",
                    "url": fixture.url("/multipart/{{inputs.route.value}}"),
                    "method": "POST",
                    "contentType": "multipart/form-data",
                    "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\"}",
                    "body": "{\"file\":\"{{inputs.input.path}}\",\"prompt\":\"{{inputs.prompt.value}}\"}"
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-cloud-multipart-art");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art_loom/execute_art_node",
                    "params": {
                        "node_id": "node-cloud-multipart",
                        "art_id": "fixture-cloud-multipart-art",
                        "input_base64": image_data,
                        "params": {
                            "route": "image",
                            "trace": "trace-bridge",
                            "prompt": "hello cloud multipart"
                        }
                    }
                })
                .to_string(),
            ))
            .expect("send multipart cloud execute art node");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(response["type"], "success", "response={response}");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-cloud-multipart");
        assert_eq!(response["data"]["output_base64"], image_data);

        let request = fixture.request();
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /multipart/image HTTP/1.1"));
        assert!(request_lower.contains("x-trace: trace-bridge"));
        assert!(request_lower.contains("content-type: multipart/form-data; boundary="));
        assert!(request.contains("name=\"file\""));
        assert!(request.contains("filename=\"loom-cloud-input-"));
        assert!(request.contains("name=\"prompt\""));
        assert!(request.contains("\r\nhello cloud multipart\r\n"));
        assert!(!request.contains("{{"));

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup cloud multipart art node root");
    }

    // Regression: the real Hook path is `art/process` (not `art_loom/execute_art_node`).
    // For cloud_api multipart arts it must write the AHRP input to a temp file and
    // bind it to `input`/`image` so `{{inputs.input.path}}` uploads the real image.
    // Before the fix this path bound `input` to the raw AHRP input object, so the
    // template rendered a JSON blob and PhotoRoom-style APIs answered "missing_image".
    #[test]
    fn daemon_hook_bridge_ahrp_process_cloud_multipart_uploads_input_file() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("cloud-multipart-ahrp-process");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let image_data = test_png_base64();
        let fixture =
            CloudApiFixture::start(CloudApiFixtureMode::MultipartImage(image_data.clone()));
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-cloud-multipart-process",
            &serde_json::json!({
                "id": "fixture-cloud-multipart-process",
                "name": "Fixture Cloud Multipart Process",
                "description": "Execute multipart cloud Art through the AHRP process path",
                "enabled": true,
                "execution": {
                    "type": "cloud_api",
                    "url": fixture.url("/multipart/segment"),
                    "method": "POST",
                    "contentType": "multipart/form-data",
                    "headers": "{\"x-api-key\":\"sk_test\"}",
                    "body": "{\"image_file\":\"{{inputs.input}}\",\"format\":\"png\"}"
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-cloud-multipart-process");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-cloud-multipart-process",
                        "art_id": "fixture-cloud-multipart-process",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {},
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send cloud multipart ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(
            response["request_id"], "req-cloud-multipart-process",
            "response={response}"
        );
        assert_eq!(response["status"], "Success", "response={response}");

        let request = fixture.request();
        // The image field must be a real file upload, not the AHRP input JSON.
        assert!(request.contains("name=\"image_file\""));
        assert!(request.contains("filename=\"loom-cloud-input-"));
        assert!(request.contains("name=\"format\""));
        assert!(request.contains("\r\npng\r\n"));
        // No unrendered template and no leaked AHRP input object.
        assert!(!request.contains("{{"));
        assert!(!request.contains("\"type\":\"base64\""));

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup cloud multipart ahrp process root");
    }

    #[test]
    fn daemon_hook_bridge_executes_cloud_api_ahrp_process_image_output() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = unique_temp_dir("cloud-ahrp-process");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
        let image_data = test_png_base64();
        let fixture = CloudApiFixture::start(CloudApiFixtureMode::Image(image_data.clone()));
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let saved_tool = http_json_put(
            address.port(),
            "/v1/tools/fixture-cloud-process",
            &serde_json::json!({
                "id": "fixture-cloud-process",
                "name": "Fixture Cloud Process",
                "description": "Execute fixture cloud through AHRP process",
                "enabled": true,
                "execution": {
                    "type": "cloud_api",
                    "endpoint": fixture.url("/image"),
                    "method": "POST"
                }
            })
            .to_string(),
        );
        assert_eq!(saved_tool["tool"]["id"], "fixture-cloud-process");

        let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
        let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
        let mut socket = connect_hook_bridge_websocket(bridge_port);

        socket
            .send(tungstenite::Message::Text(
                serde_json::json!({
                    "method": "art/process",
                    "params": {
                        "request_id": "req-cloud-process",
                        "art_id": "fixture-cloud-process",
                        "input": {
                            "type": "base64",
                            "data": image_data,
                            "width": 1,
                            "height": 1,
                            "format": "rgba8"
                        },
                        "params": {},
                        "disabled_params": []
                    }
                })
                .to_string(),
            ))
            .expect("send cloud ahrp process");
        let response = read_hook_bridge_json(&mut socket);

        assert_eq!(
            response["request_id"], "req-cloud-process",
            "response={response}"
        );
        assert_eq!(response["status"], "Success");
        assert_eq!(response["data"]["type"], "result");
        assert_eq!(response["data"]["output"]["type"], "base64");
        assert_eq!(response["data"]["output"]["data"], image_data);
        assert_eq!(response["data"]["output"]["width"], 1);
        assert_eq!(response["data"]["output"]["height"], 1);

        drop(socket);
        let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
        assert_eq!(stopped["running"], false);

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        fs::remove_dir_all(root).expect("cleanup cloud ahrp process root");
    }

    fn connect_hook_bridge_websocket(bridge_port: u16) -> tungstenite::WebSocket<TcpStream> {
        let stream =
            TcpStream::connect(("127.0.0.1", bridge_port)).expect("connect bridge tcp socket");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set websocket read timeout");
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .expect("set websocket write timeout");
        tungstenite::client(format!("ws://127.0.0.1:{bridge_port}"), stream)
            .expect("connect bridge websocket")
            .0
    }

    fn read_hook_bridge_json(socket: &mut tungstenite::WebSocket<TcpStream>) -> serde_json::Value {
        let response = socket.read().expect("read websocket frame");
        let response = response.into_text().expect("text frame");
        serde_json::from_str(&response).expect("response json")
    }

    fn test_png_base64() -> String {
        format!("data:image/png;base64,{}", BASE64.encode(test_png_bytes()))
    }

    fn test_color_png_base64(rgba: [u8; 4]) -> String {
        loom_image_io::rgba8_to_png_data_url(1, 1, &rgba).expect("encode colored test png")
    }
    fn fixture_image_base64() -> String {
        "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
            .to_owned()
    }

    fn fixture_image_bytes() -> Vec<u8> {
        loom_image_io::decode_data_url_bytes(&fixture_image_base64())
            .expect("decode fixture image data url")
    }

    fn test_png_bytes() -> Vec<u8> {
        let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(1, 1, vec![10, 20, 30, 255])
            .expect("test png image");
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut png, ImageFormat::Png)
            .expect("encode test png");
        png.into_inner()
    }

    fn packaged_ocr_fixture_base64() -> String {
        let image = fs::read(
            workspace_ocr_resources()
                .join("fixtures")
                .join("test_1.png"),
        )
        .expect("read packaged OCR fixture");
        format!("data:image/png;base64,{}", BASE64.encode(image))
    }

    fn workspace_ocr_resources() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let path = candidate.join("resources").join("ocr");
                path.join("ch_PP-OCRv4_det_infer.onnx")
                    .exists()
                    .then_some(path)
            })
            .expect("locate Loom/resources/ocr")
    }

    fn workspace_image_blend_script_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let script = candidate
                    .join("resources")
                    .join("script-arts")
                    .join("image-blend")
                    .join("main.ps1");
                script.exists().then_some(script)
            })
            .expect("locate Loom/resources/script-arts/image-blend/main.ps1")
    }

    fn workspace_image_blend_compress_resource(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let path = candidate
                    .join("resources")
                    .join("workflow-arts")
                    .join("image-blend-compress")
                    .join(name);
                path.exists().then_some(path)
            })
            .unwrap_or_else(|| panic!("locate image-blend-compress resource `{name}`"))
    }

    #[cfg(windows)]
    fn write_daemon_cli_image_copy_fixture(root: &Path) -> (PathBuf, PathBuf) {
        let script_path = root.join("fixture-compress.ps1");
        let evidence_path = root.join("compress-evidence.txt");
        let source = r#"
param(
    [Parameter(Mandatory = $true)][string]$InputPath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [Parameter(Mandatory = $true)][int]$Quality,
    [Parameter(Mandatory = $true)][string]$EvidencePath
)
$ErrorActionPreference = "Stop"
Copy-Item -LiteralPath $InputPath -Destination $OutputPath -Force
[System.IO.File]::WriteAllText(
    $EvidencePath,
    [string]$Quality,
    [System.Text.UTF8Encoding]::new($false)
)
"#;
        fs::write(&script_path, source).expect("write daemon CLI image copy fixture");
        (script_path, evidence_path)
    }

    fn workspace_python_embed_resources() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let path = candidate.join("resources").join("python-embed");
                path.join("python.exe").exists().then_some(path)
            })
            .expect("locate Loom/resources/python-embed")
    }

    fn workspace_python_resources() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let path = candidate.join("resources").join("python");
                path.join("Launcher.py").exists().then_some(path)
            })
            .expect("locate Loom/resources/python")
    }

    fn copy_dir_recursive(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("create recursive copy destination");
        for entry in fs::read_dir(source).expect("read recursive copy source") {
            let entry = entry.expect("read recursive copy entry");
            let source_path = entry.path();
            let destination_path = destination.join(entry.file_name());
            if source_path.is_dir() {
                copy_dir_recursive(&source_path, &destination_path);
            } else {
                if let Some(parent) = destination_path.parent() {
                    fs::create_dir_all(parent).expect("create recursive copy parent");
                }
                fs::copy(&source_path, &destination_path).expect("copy recursive file");
            }
        }
    }

    fn mark_framework_installed(root: &Path, id: &str) {
        let mut ids = vec![
            "cli_wrapper".to_owned(),
            "cloud_api".to_owned(),
            "script".to_owned(),
            "workflow".to_owned(),
        ];
        if !ids.iter().any(|candidate| candidate == id) {
            ids.push(id.to_owned());
        }
        let registry = FrameworkRegistry::new(root);
        for framework_id in ids {
            install_test_framework_package(&registry, &framework_id);
        }
    }

    fn install_test_framework_package(registry: &FrameworkRegistry, id: &str) {
        registry
            .install_framework_package_from_zip(&framework_package_zip(id, "0.1.0"))
            .expect("install daemon test framework");
    }

    fn provision_test_python_art_runtime(root: &Path) -> PathBuf {
        let runtime_root = root.join("frameworks").join("python_art");
        let python_embed_root = runtime_root.join("python-embed");
        let python_root = runtime_root.join("python");
        copy_dir_recursive(&workspace_python_embed_resources(), &python_embed_root);
        fs::create_dir_all(&python_root).expect("create python_art runtime python dir");
        fs::copy(
            workspace_python_resources().join("Launcher.py"),
            python_root.join("Launcher.py"),
        )
        .expect("copy python_art launcher");
        runtime_root
    }

    fn write_daemon_python_art_fixture(root: &Path) -> PathBuf {
        let art_dir = root.join("fixture-python-art");
        fs::create_dir_all(&art_dir).expect("create python art fixture dir");
        fs::write(
            art_dir.join("art.json"),
            r#"{
  "art_id": "fixture_python_art",
  "label": "Fixture Python Art",
  "description": "Python Art fixture for daemon Hook bridge tests.",
  "version": "1.0.0",
  "execution": {
    "engine": "python",
    "entry": "main.py"
  },
  "signature": {
    "inputs": [
      {
        "id": "text",
        "label": "Text",
        "type": "String"
      }
    ],
    "outputs": [
      {
        "id": "text",
        "label": "Text",
        "type": "String"
      }
    ]
  },
  "variables": []
}
"#,
        )
        .expect("write python art fixture art.json");
        fs::write(
            art_dir.join("main.py"),
            r#"#!/usr/bin/env python3
import sys


def main(args):
    return {
        "content": [
            {
                "type": "text",
                "text": f"python art saw {args.get('text', '')}",
            }
        ],
        "pythonExecutable": sys.executable,
    }
"#,
        )
        .expect("write python art fixture main.py");
        art_dir
    }

    fn write_daemon_script_fixture(root: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let script_path = root.join("fixture-script-art.ps1");
            let source = r#"
$ErrorActionPreference = "Stop"
$payload = $args[0] | ConvertFrom-Json
$arguments = $payload.arguments
if ($arguments.mode -eq "shader") {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "text"
                text = "void fragment() { COLOR = vec4(1.0); }"
            }
        )
    }
} elseif ($arguments.input_base64) {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "image"
                data = [string]$arguments.input_base64
                mimeType = "image/png"
            }
        )
    }
} elseif ($arguments.input -and $arguments.input.data) {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "image"
                data = [string]$arguments.input.data
                mimeType = "image/png"
            }
        )
    }
} else {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "text"
                text = "script saw $($arguments.text)"
            }
        )
    }
}
[Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
            fs::write(&script_path, source).expect("write PowerShell daemon script fixture");
            script_path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;

            let script_path = root.join("fixture-script-art.sh");
            let source = r#"#!/usr/bin/env sh
python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
arguments = payload.get("arguments", {})
if arguments.get("mode") == "shader":
    response = {
        "content": [
            {
                "type": "text",
                "text": "void fragment() { COLOR = vec4(1.0); }",
            }
        ]
    }
elif arguments.get("input_base64"):
    response = {
        "content": [
            {
                "type": "image",
                "data": arguments["input_base64"],
                "mimeType": "image/png",
            }
        ]
    }
elif arguments.get("input", {}).get("data"):
    response = {
        "content": [
            {
                "type": "image",
                "data": arguments["input"]["data"],
                "mimeType": "image/png",
            }
        ]
    }
else:
    response = {
        "content": [
            {
                "type": "text",
                "text": "script saw " + str(arguments.get("text", "")),
            }
        ]
    }
print(json.dumps(response))
PY
"#;
            fs::write(&script_path, source).expect("write shell daemon script fixture");
            let mut permissions = fs::metadata(&script_path)
                .expect("daemon script fixture metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&script_path, permissions)
                .expect("make daemon shell fixture executable");
            script_path
        }
    }

    struct GatewayBrainPlanFixture {
        port: u16,
        worker: Option<thread::JoinHandle<()>>,
        captured_request: Arc<Mutex<Option<String>>>,
    }

    impl GatewayBrainPlanFixture {
        fn start(status: &'static str, body: String) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind Gateway fixture");
            let port = listener
                .local_addr()
                .expect("Gateway fixture address")
                .port();
            let captured_request = Arc::new(Mutex::new(None));
            let worker_captured_request = Arc::clone(&captured_request);
            let worker = thread::spawn(move || {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let request = read_gateway_fixture_request(&mut stream);
                *worker_captured_request
                    .lock()
                    .expect("lock Gateway request capture") = Some(request);
                write_cloud_fixture_response(&mut stream, status, "application/json", &body);
            });
            Self {
                port,
                worker: Some(worker),
                captured_request,
            }
        }

        fn base_url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        fn request(&self) -> String {
            self.captured_request
                .lock()
                .expect("lock Gateway request capture")
                .clone()
                .expect("captured Gateway request")
        }
    }

    impl Drop for GatewayBrainPlanFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    fn read_gateway_fixture_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set Gateway fixture read timeout");
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let bytes = stream
                .read(&mut chunk)
                .expect("read Gateway fixture request");
            if bytes == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..bytes]);
            let Some(header_end) = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| position + 4)
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(|| {
                        value
                            .trim()
                            .parse::<usize>()
                            .expect("Gateway content length")
                    })
                })
                .unwrap_or(0);
            if request.len() >= header_end + content_length {
                break;
            }
        }
        String::from_utf8(request).expect("Gateway fixture request UTF-8")
    }

    enum CloudApiFixtureMode {
        Text,
        Image(String),
        MultipartImage(String),
    }

    struct CloudApiFixture {
        port: u16,
        worker: Option<thread::JoinHandle<()>>,
        captured_request: Arc<Mutex<Option<String>>>,
    }

    impl CloudApiFixture {
        fn start(mode: CloudApiFixtureMode) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind cloud API fixture");
            let port = listener
                .local_addr()
                .expect("cloud API fixture address")
                .port();
            let captured_request = Arc::new(Mutex::new(None));
            let worker_captured_request = Arc::clone(&captured_request);
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept cloud API fixture request");
                let request = read_cloud_fixture_request(&mut stream);
                *worker_captured_request
                    .lock()
                    .expect("lock cloud request capture") = Some(request.clone());
                let Some((_, body)) = request.split_once("\r\n\r\n") else {
                    return;
                };
                let prompt = serde_json::from_str::<serde_json::Value>(body)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("prompt")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_default();
                let response = match mode {
                    CloudApiFixtureMode::Text => serde_json::json!({
                        "content": [
                            {
                                "type": "text",
                                "text": format!("cloud saw {prompt}")
                            }
                        ]
                    }),
                    CloudApiFixtureMode::Image(image_data) => serde_json::json!({
                        "content": [
                            {
                                "type": "image",
                                "data": image_data,
                                "mimeType": "image/png"
                            }
                        ]
                    }),
                    CloudApiFixtureMode::MultipartImage(image_data) => serde_json::json!({
                        "content": [
                            {
                                "type": "image",
                                "data": image_data,
                                "mimeType": "image/png"
                            }
                        ]
                    }),
                };
                write_cloud_fixture_response(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    &response.to_string(),
                );
            });
            Self {
                port,
                worker: Some(worker),
                captured_request,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }

        fn request(&self) -> String {
            self.captured_request
                .lock()
                .expect("lock cloud request capture")
                .clone()
                .expect("captured cloud request")
        }
    }

    impl Drop for CloudApiFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    struct HttpImageFixture {
        port: u16,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl HttpImageFixture {
        fn start(content_type: &'static str, body: Vec<u8>) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP image fixture");
            let port = listener
                .local_addr()
                .expect("HTTP image fixture address")
                .port();
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener
                    .accept()
                    .expect("accept HTTP image fixture request");
                let _ = read_cloud_fixture_request(&mut stream);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            });
            Self {
                port,
                worker: Some(worker),
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }
    }

    impl Drop for HttpImageFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    fn read_cloud_fixture_request(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 8192];
        // Read headers first (until CRLFCRLF), then the body per Content-Length.
        // A single 8 KB read stops at the header boundary because reqwest sends
        // the multipart body in a later packet, so multipart assertions (file
        // parts, field values) would miss the body without this.
        loop {
            let read = stream.read(&mut chunk).expect("read cloud fixture request");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&buffer);
            if let Some(header_end) = text.find("\r\n\r\n") {
                let headers = &text[..header_end];
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.trim()
                            .eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                let body_start = header_end + 4;
                if buffer.len() >= body_start + content_length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&buffer).to_string()
    }

    fn write_cloud_fixture_response(
        stream: &mut TcpStream,
        status: &str,
        content_type: &str,
        body: &str,
    ) {
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.flush();
    }

    struct TranslateFixture {
        port: u16,
        worker: Option<thread::JoinHandle<()>>,
        captured_request: Arc<Mutex<Option<String>>>,
    }

    impl TranslateFixture {
        fn start() -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind translate fixture");
            let port = listener
                .local_addr()
                .expect("translate fixture address")
                .port();
            let captured_request = Arc::new(Mutex::new(None));
            let worker_captured_request = Arc::clone(&captured_request);
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept translate fixture request");
                let request = read_cloud_fixture_request(&mut stream);
                *worker_captured_request
                    .lock()
                    .expect("lock translate request capture") = Some(request.clone());
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or("{}");
                let payload = serde_json::from_str::<Value>(body).unwrap_or_else(|_| json!({}));
                let text = payload
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let target_lang = payload
                    .get("target_lang")
                    .or_else(|| payload.get("targetLang"))
                    .and_then(Value::as_str)
                    .unwrap_or("auto");
                let response = json!({
                    "code": 200,
                    "data": format!("translated:{text}:{target_lang}")
                });
                write_cloud_fixture_response(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    &response.to_string(),
                );
            });
            Self {
                port,
                worker: Some(worker),
                captured_request,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }

        fn request(&self) -> String {
            self.captured_request
                .lock()
                .expect("lock translate request capture")
                .clone()
                .expect("captured translate request")
        }
    }

    impl Drop for TranslateFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    struct McpRegistryFixture {
        port: u16,
        worker: Option<thread::JoinHandle<()>>,
        request_path: Arc<Mutex<Option<String>>>,
    }

    impl McpRegistryFixture {
        fn start() -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind MCP registry fixture");
            let port = listener
                .local_addr()
                .expect("MCP registry fixture address")
                .port();
            let request_path = Arc::new(Mutex::new(None));
            let worker_request_path = Arc::clone(&request_path);
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept MCP registry request");
                let request = read_cloud_fixture_request(&mut stream);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_owned();
                *worker_request_path
                    .lock()
                    .expect("lock MCP registry request path") = Some(path);
                write_cloud_fixture_response(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    r#"{"servers":[{"server":{"name":"io.modelcontextprotocol/fixture","title":"Fixture MCP","description":"Fixture registry server","packages":[{"registryType":"npm","identifier":"@fixture/mcp","version":"1.0.0","transport":{"type":"stdio"},"runtimeArguments":[{"value":"-y"}],"environmentVariables":[{"name":"FIXTURE_API_KEY","isRequired":true}]}]},"_meta":{"io.modelcontextprotocol.registry/official":{"status":"active","isLatest":true,"updatedAt":"2026-06-12T00:00:00Z"}}}],"metadata":{"count":1}}"#,
                );
            });
            Self {
                port,
                worker: Some(worker),
                request_path,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }

        fn request_path(&self) -> String {
            self.request_path
                .lock()
                .expect("lock MCP registry request path")
                .clone()
                .expect("captured MCP registry request path")
        }
    }

    impl Drop for McpRegistryFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    #[test]
    fn daemon_mcp_fixture_server() {
        if std::env::var("LOOM_DAEMON_MCP_FIXTURE_SERVER")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }

        run_mcp_fixture_server();
        std::process::exit(0);
    }

    #[test]
    fn daemon_writes_local_capability_manifest_when_configured() {
        let temp_dir = unique_temp_dir("manifest");
        let manifest_dir = temp_dir.join("capabilities");
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_manifest_dir(&manifest_dir))
            .expect("bind daemon");
        let address = daemon.local_addr().expect("local address");

        let manifest_path = manifest_dir.join("loom.json");
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read loom manifest"))
                .expect("valid loom manifest json");

        assert_eq!(manifest["schemaVersion"], 1);
        assert_eq!(manifest["appId"], "loom");
        assert_eq!(manifest["displayName"], "Loom");
        assert_eq!(manifest["version"], loom_core::LOOM_VERSION);
        assert!(manifest["pid"].as_u64().expect("pid") > 0);
        assert_eq!(manifest["transport"]["type"], "http");
        assert_eq!(
            manifest["transport"]["baseUrl"],
            format!("http://127.0.0.1:{}", address.port())
        );
        assert_eq!(manifest["transport"]["auth"], "none");
        assert!(manifest["transport"].get("authToken").is_none());
        assert!(manifest["capabilities"]
            .as_array()
            .expect("capabilities")
            .contains(&serde_json::Value::String("brain.plan".to_owned())));
        assert!(manifest["capabilities"]
            .as_array()
            .expect("capabilities")
            .contains(&serde_json::Value::String(
                "tea.ticket.decompose.v1".to_owned()
            )));
        assert!(manifest["capabilities"]
            .as_array()
            .expect("capabilities")
            .contains(&serde_json::Value::String(
                "tea.ticket.execute.v1".to_owned()
            )));
        assert!(manifest["capabilities"]
            .as_array()
            .expect("capabilities")
            .contains(&serde_json::Value::String(
                "tea.ticket.review.v1".to_owned()
            )));
        assert!(manifest["startedAt"].as_u64().is_some() || manifest["startedAt"].is_string());
    }

    #[test]
    fn daemon_writes_bearer_local_capability_manifest_and_requires_auth_when_configured() {
        let temp_dir = unique_temp_dir("bearer-manifest");
        let manifest_dir = temp_dir.join("capabilities");
        let daemon = LoomDaemon::bind(
            DaemonConfig::localhost(0)
                .with_bearer_token("local-token")
                .with_manifest_dir(&manifest_dir),
        )
        .expect("bind tokenized daemon");
        let address = daemon.local_addr().expect("local address");

        let manifest_path = manifest_dir.join("loom.json");
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read loom manifest"))
                .expect("valid loom manifest json");
        assert_eq!(manifest["transport"]["type"], "http");
        assert_eq!(
            manifest["transport"]["baseUrl"],
            format!("http://127.0.0.1:{}", address.port())
        );
        assert_eq!(manifest["transport"]["auth"], "bearer");
        assert_eq!(manifest["transport"]["authToken"], "local-token");
        assert!(manifest["capabilities"]
            .as_array()
            .expect("capabilities")
            .contains(&serde_json::Value::String("brain.plan".to_owned())));
        assert!(manifest["capabilities"]
            .as_array()
            .expect("capabilities")
            .contains(&serde_json::Value::String(
                "tea.ticket.decompose.v1".to_owned()
            )));

        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let public_health = http_get(address.port(), "/health");
        assert!(
            public_health.starts_with("HTTP/1.1 200 OK"),
            "public_health={public_health}"
        );

        let unauthorized_capabilities = http_get(address.port(), "/v1/capabilities");
        assert!(
            unauthorized_capabilities.starts_with("HTTP/1.1 401 Unauthorized"),
            "unauthorized_capabilities={unauthorized_capabilities}"
        );

        let invoke_body = r#"{
            "requestId":"loom-bearer-manifest-1",
            "caller":"hook",
            "capability":"brain.plan",
            "input":{"goal":"token protected manifest"}
        }"#;
        let unauthorized_invoke =
            http_request(address.port(), "POST", "/v1/invoke", Some(invoke_body));
        assert!(
            unauthorized_invoke.starts_with("HTTP/1.1 401 Unauthorized"),
            "unauthorized_invoke={unauthorized_invoke}"
        );

        let authorized_invoke = http_request_with_bearer(
            address.port(),
            "POST",
            "/v1/invoke",
            Some(invoke_body),
            "local-token",
        );
        assert!(
            authorized_invoke.starts_with("HTTP/1.1 200 OK"),
            "authorized_invoke={authorized_invoke}"
        );
        let authorized_body = response_json_body(&authorized_invoke);
        assert_eq!(authorized_body["requestId"], "loom-bearer-manifest-1");
        assert_eq!(authorized_body["status"], "succeeded");
        assert!(authorized_body["output"]["runId"].as_str().is_some());

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn root_contract_loom_manifest_fixture_matches_current_invokable_capabilities() {
        let fixture: serde_json::Value =
            serde_json::from_str(&shared_local_capability_example("loom-manifest.json"))
                .expect("root contract Loom manifest fixture json");
        let capabilities = fixture["capabilities"]
            .as_array()
            .expect("fixture capabilities")
            .iter()
            .map(|capability| capability.as_str().expect("capability string").to_owned())
            .collect::<Vec<_>>();

        assert_eq!(fixture["schemaVersion"], 1);
        assert_eq!(fixture["appId"], "loom");
        assert_eq!(fixture["displayName"], "Loom");
        assert_eq!(fixture["transport"]["type"], "http");
        assert_eq!(fixture["transport"]["auth"], "none");
        assert_eq!(
            capabilities,
            vec![
                "brain.plan".to_owned(),
                "tea.ticket.decompose.v1".to_owned(),
                "tea.ticket.execute.v1".to_owned(),
                "tea.ticket.review.v1".to_owned(),
            ]
        );
    }

    #[test]
    fn daemon_reports_default_tea_configuration_claim() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
        let previous_base = std::env::var("LOOM_SETTINGS_BASE_URL").ok();
        std::env::remove_var("LOOM_MANAGED_CONFIG_APPS");
        std::env::remove_var("LOOM_SETTINGS_BASE_URL");
        let root = unique_temp_dir("claims-tea-default");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let response = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/configuration/claims?app=tea", &[], None),
            ),
            200,
        );

        assert_eq!(response["app"], "tea");
        assert_eq!(response["managed"], false);
        assert_eq!(response["panel_url"], serde_json::Value::Null);

        restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
        restore_env("LOOM_SETTINGS_BASE_URL", previous_base);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_reports_managed_tea_configuration_claim_from_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
        let previous_base = std::env::var("LOOM_SETTINGS_BASE_URL").ok();
        std::env::set_var("LOOM_MANAGED_CONFIG_APPS", "hook,tea,talk");
        std::env::set_var("LOOM_SETTINGS_BASE_URL", "http://127.0.0.1:8765/settings");

        let root = unique_temp_dir("claims-tea-managed");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let response = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/configuration/claims?app=tea", &[], None),
            ),
            200,
        );

        assert_eq!(response["app"], "tea");
        assert_eq!(response["managed"], true);
        assert_eq!(response["panel_url"], "http://127.0.0.1:8765/settings/tea");

        restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
        restore_env("LOOM_SETTINGS_BASE_URL", previous_base);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_claim_response_includes_owner_source_and_schema_version() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
        let previous_base = std::env::var("LOOM_SETTINGS_BASE_URL").ok();
        std::env::set_var("LOOM_MANAGED_CONFIG_APPS", "tea");
        std::env::set_var("LOOM_SETTINGS_BASE_URL", "http://127.0.0.1:8765/settings");
        let root = unique_temp_dir("claims-tea-owner-source");
        let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
        let response = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/configuration/claims?app=tea", &[], None),
            ),
            200,
        );

        assert_eq!(response["app"], "tea");
        assert_eq!(response["managed"], true);
        assert_eq!(response["owner"], "loom");
        assert_eq!(response["source"], "loom-managed");
        assert_eq!(response["schema_version"], 1);
        assert_eq!(response["panel_url"], "http://127.0.0.1:8765/settings/tea");

        restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
        restore_env("LOOM_SETTINGS_BASE_URL", previous_base);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_configuration_api_reads_writes_and_rejects_stale_revisions() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
        let previous_root = std::env::var("LOOM_CONFIGURATION_ROOT").ok();
        let root =
            std::env::temp_dir().join(format!("loom-daemon-config-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::env::set_var("LOOM_MANAGED_CONFIG_APPS", "tea");
        std::env::set_var("LOOM_CONFIGURATION_ROOT", &root);
        let control_plane_root = unique_temp_dir("configuration-api-control-plane");
        let runtime =
            test_daemon_runtime_from_config(&control_plane_root, DaemonConfig::localhost(0));

        let first = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request("GET", "/v1/configuration/apps/tea", &[], None),
            ),
            200,
        );
        assert_eq!(first["source"], "loom-managed");
        assert_eq!(first["created"], true);
        assert_eq!(first["document"]["revision"], 1);

        let write = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/configuration/apps/tea",
                    &[],
                    Some(
                        r#"{
              "expected_revision": 1,
              "config": {
                "notifications_enabled": false,
                "human_ticket_default_approval_policy": "manual_only",
                "hook_ticket_default_approval_policy": "plan_only"
              }
            }"#,
                    ),
                ),
            ),
            200,
        );
        assert_eq!(write["ok"], true);
        assert_eq!(write["document"]["revision"], 2);

        let stale = expect_json_text_route_response(
            route_request(
                &runtime,
                &parsed_request(
                    "PUT",
                    "/v1/configuration/apps/tea",
                    &[],
                    Some(r#"{"expected_revision":1,"config":{"notifications_enabled":true}}"#),
                ),
            ),
            409,
        );
        assert_eq!(stale["error"]["code"], "revision_conflict");

        drop(runtime);
        fs::remove_dir_all(control_plane_root).expect("cleanup control plane");
        let _ = std::fs::remove_dir_all(&root);
        restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
        restore_env("LOOM_CONFIGURATION_ROOT", previous_root);
    }

    #[test]
    fn daemon_settings_pages_render_real_html() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_apps = std::env::var("LOOM_MANAGED_CONFIG_APPS").ok();
        let previous_root = std::env::var("LOOM_CONFIGURATION_ROOT").ok();
        let root =
            std::env::temp_dir().join(format!("loom-daemon-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::env::set_var("LOOM_MANAGED_CONFIG_APPS", "tea,hook");
        std::env::set_var("LOOM_CONFIGURATION_ROOT", &root);
        let control_plane_root = unique_temp_dir("settings-pages-control-plane");
        let runtime =
            test_daemon_runtime_from_config(&control_plane_root, DaemonConfig::localhost(0));

        let index_body = expect_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/settings", &[], None)),
            200,
        );
        let mut index_http = Vec::new();
        write_response(&mut index_http, 200, &index_body).expect("write settings index");
        let index = String::from_utf8(index_http).expect("settings index http");
        assert!(
            index.contains("Content-Type: text/html; charset=utf-8"),
            "index should be served as browser-renderable HTML: {index}"
        );
        assert!(index.contains("Loom Settings"));
        assert!(index.contains("/settings/tea"));

        let tea_body = expect_text_route_response(
            route_request(&runtime, &parsed_request("GET", "/settings/tea", &[], None)),
            200,
        );
        let mut tea_http = Vec::new();
        write_response(&mut tea_http, 200, &tea_body).expect("write tea settings");
        let tea = String::from_utf8(tea_http).expect("tea settings http");
        assert!(
            tea.contains("Content-Type: text/html; charset=utf-8"),
            "app settings should be served as browser-renderable HTML: {tea}"
        );
        assert!(tea.contains("Tea Settings"));
        assert!(tea.contains("expected_revision"));

        drop(runtime);
        let _ = std::fs::remove_dir_all(&control_plane_root);
        let _ = std::fs::remove_dir_all(&root);
        restore_env("LOOM_MANAGED_CONFIG_APPS", previous_apps);
        restore_env("LOOM_CONFIGURATION_ROOT", previous_root);
    }

    #[test]
    fn daemon_invokes_brain_plan_and_serves_run_and_events() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let invoke = http_json_post(
            address.port(),
            "/v1/invoke",
            r#"{
                "requestId": "loom-request-1",
                "caller": "hook",
                "capability": "brain.plan",
                "input": {
                    "goal": "Plan Loom capability API tests",
                    "constraints": ["preserve Tea run contract"]
                }
            }"#,
        );

        assert_eq!(invoke["requestId"], "loom-request-1");
        assert_eq!(invoke["status"], "succeeded");
        assert_eq!(invoke["output"]["run"]["capability"], "brain.plan");
        assert_eq!(
            invoke["output"]["run"]["input"]["goal"],
            "Plan Loom capability API tests"
        );
        assert!(invoke["output"]["summary"]
            .as_str()
            .expect("summary")
            .contains("Plan Loom capability API tests"));
        let run_id = invoke["output"]["runId"].as_str().expect("run id");

        let stored_run = http_json_get(address.port(), &format!("/v1/runs/{run_id}"));
        assert_eq!(stored_run, invoke["output"]["run"]);

        let events = http_json_get(address.port(), &format!("/v1/runs/{run_id}/events"));
        assert_eq!(events["run_id"], run_id);
        assert_eq!(events["events"].as_array().expect("events").len(), 2);
        assert_eq!(events["events"][0]["kind"], "run_started");
        assert_eq!(events["events"][0]["run_id"], run_id);
        assert_eq!(events["events"][1]["kind"], "capability_completed");
        assert_eq!(events["events"][1]["run_id"], run_id);
        assert_eq!(events["events"][1]["planner"]["source"], "local_template");
        assert_eq!(invoke["output"]["planner"]["source"], "local_template");

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_invokes_gateway_brain_plan_and_forwards_input() {
        let plan_content = serde_json::json!({
            "summary": "Gateway plan",
            "steps": ["inspect", "execute"]
        })
        .to_string();
        let fixture = GatewayBrainPlanFixture::start(
            "200 OK",
            serde_json::json!({
                "model": "resolved-model",
                "choices": [{
                    "message": { "content": plan_content }
                }]
            })
            .to_string(),
        );
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_brain_planner(
            brain_plan::BrainPlannerConfig::Gateway(brain_plan::GatewayPlannerConfig {
                base_url: fixture.base_url(),
                auth_token: Some("test-token".to_owned()),
                model: "planner-model".to_owned(),
                timeout: Duration::from_secs(5),
            }),
        ))
        .expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let invoke = http_json_post(
            address.port(),
            "/v1/invoke",
            &serde_json::json!({
                "requestId": "loom-request-gateway-success",
                "caller": "hook",
                "capability": "brain.plan",
                "input": {
                    "goal": "Plan Gateway-backed smoke",
                    "constraints": ["preserve run contract", 42, "  "],
                    "context": {"release": "candidate-2"}
                }
            })
            .to_string(),
        );

        assert_eq!(invoke["status"], "succeeded");
        assert_eq!(invoke["output"]["summary"], "Gateway plan");
        assert_eq!(
            invoke["output"]["steps"],
            serde_json::json!(["inspect", "execute"])
        );
        assert_eq!(invoke["output"]["planner"]["source"], "gateway");
        assert_eq!(invoke["output"]["planner"]["model"], "resolved-model");
        assert_eq!(invoke["output"]["run"]["status"], "succeeded");

        let run_id = invoke["output"]["runId"].as_str().expect("run id");
        let stored_run = http_json_get(address.port(), &format!("/v1/runs/{run_id}"));
        assert_eq!(stored_run["status"], "succeeded");
        assert_eq!(stored_run["output"]["planner"]["source"], "gateway");
        let events = http_json_get(address.port(), &format!("/v1/runs/{run_id}/events"));
        assert_eq!(events["events"].as_array().expect("events").len(), 2);
        assert_eq!(events["events"][0]["kind"], "run_started");
        assert_eq!(events["events"][1]["kind"], "capability_completed");
        assert_eq!(events["events"][1]["planner"]["source"], "gateway");

        let gateway_request = fixture.request();
        assert!(gateway_request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(gateway_request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-token"));
        let gateway_body = gateway_request
            .split_once("\r\n\r\n")
            .expect("Gateway request body")
            .1;
        let gateway_payload: serde_json::Value =
            serde_json::from_str(gateway_body).expect("Gateway request JSON");
        assert_eq!(gateway_payload["model"], "planner-model");
        let user_content = gateway_payload["messages"][1]["content"]
            .as_str()
            .expect("Gateway user content");
        let user_payload: serde_json::Value =
            serde_json::from_str(user_content).expect("Gateway user JSON");
        assert_eq!(user_payload["goal"], "Plan Gateway-backed smoke");
        assert_eq!(
            user_payload["constraints"],
            serde_json::json!(["preserve run contract"])
        );
        assert_eq!(
            user_payload["context"],
            serde_json::json!({"release": "candidate-2"})
        );
        assert_eq!(
            user_payload.as_object().expect("Gateway user object").len(),
            3
        );
        assert!(user_content.contains("Plan Gateway-backed smoke"));
        assert!(!user_content.contains("test-token"));
        assert!(!user_content.contains("LOOM_GATEWAY_BASE_URL"));

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_records_failed_gateway_brain_plan_with_run_evidence() {
        let fixture = GatewayBrainPlanFixture::start(
            "503 Service Unavailable",
            serde_json::json!({
                "error": {
                    "code": "gateway_unavailable",
                    "message": format!("fixture Gateway is unavailable {}", "x".repeat(700))
                }
            })
            .to_string(),
        );
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_brain_planner(
            brain_plan::BrainPlannerConfig::Gateway(brain_plan::GatewayPlannerConfig {
                base_url: fixture.base_url(),
                auth_token: Some("failure-secret".to_owned()),
                model: "failure-model".to_owned(),
                timeout: Duration::from_secs(5),
            }),
        ))
        .expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let response = http_request(
            address.port(),
            "POST",
            "/v1/invoke",
            Some(
                r#"{
                    "requestId":"loom-request-gateway-failure",
                    "caller":"hook",
                    "capability":"brain.plan",
                    "input":{
                        "goal":"record Gateway failure",
                        "context":{"raw":"must not become diagnostic"}
                    }
                }"#,
            ),
        );

        assert!(
            response.starts_with("HTTP/1.1 502 Bad Gateway"),
            "response={response}"
        );
        let body = response_json_body(&response);
        assert_eq!(body["status"], "failed");
        assert_eq!(body["error"]["code"], "gateway_planner_failed");
        let run_id = body["error"]["runId"].as_str().expect("failed run id");
        assert!(!response.contains("failure-secret"));

        let stored_run = http_json_get(address.port(), &format!("/v1/runs/{run_id}"));
        assert_eq!(stored_run["status"], "failed");
        assert_eq!(stored_run["error"]["code"], "gateway_planner_failed");
        let diagnostic = stored_run["error"]["diagnostic"]
            .as_str()
            .expect("failed run diagnostic");
        assert!(diagnostic.len() <= 512, "diagnostic={diagnostic}");
        assert!(!diagnostic.contains("must not become diagnostic"));
        assert!(!stored_run.to_string().contains("failure-secret"));
        let events = http_json_get(address.port(), &format!("/v1/runs/{run_id}/events"));
        assert_eq!(events["events"].as_array().expect("events").len(), 2);
        assert_eq!(events["events"][0]["kind"], "run_started");
        assert_eq!(events["events"][1]["kind"], "capability_failed");
        assert_eq!(events["events"][1]["planner"]["source"], "gateway");
        assert_eq!(
            events["events"][1]["error"]["code"],
            "gateway_planner_failed"
        );

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_reads_brain_plan_run_after_restart() {
        let root = unique_temp_dir("run-restart");
        let path = root.join("runs.sqlite3");
        let (port, shutdown_tx, server) =
            start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);

        let invoke = http_json_post(
            port,
            "/v1/invoke",
            r#"{"requestId":"persist-1","caller":"hook","capability":"brain.plan","input":{"goal":"survive restart"}}"#,
        );
        let run_id = invoke["output"]["runId"]
            .as_str()
            .expect("run id")
            .to_owned();
        shutdown_tx.send(()).expect("shutdown first daemon");
        server.join().expect("first daemon").expect("first serve");

        let (port, shutdown_tx, server) =
            start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);
        let run = http_json_get(port, &format!("/v1/runs/{run_id}"));
        let events = http_json_get(port, &format!("/v1/runs/{run_id}/events"));
        assert_eq!(run["status"], "succeeded");
        assert_eq!(run["input"]["goal"], "survive restart");
        assert_eq!(events["events"][0]["kind"], "run_started");
        assert_eq!(events["events"][1]["kind"], "capability_completed");
        shutdown_tx.send(()).expect("shutdown second daemon");
        server.join().expect("second daemon").expect("second serve");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_reads_failed_gateway_run_after_restart() {
        let root = unique_temp_dir("gateway-run-restart");
        let path = root.join("runs.sqlite3");
        let planner = BrainPlannerConfig::Gateway(brain_plan::GatewayPlannerConfig {
            base_url: "http://127.0.0.1:9".to_owned(),
            auth_token: Some("restart-secret".to_owned()),
            model: "restart-model".to_owned(),
            timeout: Duration::from_secs(1),
        });
        let (port, shutdown_tx, server) = start_daemon_with_store(&path, planner);

        let response = http_request(
            port,
            "POST",
            "/v1/invoke",
            Some(
                r#"{"requestId":"persist-gateway-failure","caller":"hook","capability":"brain.plan","input":{"goal":"persist gateway failure"}}"#,
            ),
        );
        assert!(response.starts_with("HTTP/1.1 502 Bad Gateway"));
        let body = response_json_body(&response);
        let run_id = body["error"]["runId"].as_str().expect("failed run id");
        assert!(!response.contains("restart-secret"));
        shutdown_tx.send(()).expect("shutdown first daemon");
        server.join().expect("first daemon").expect("first serve");

        let (port, shutdown_tx, server) =
            start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);
        let run = http_json_get(port, &format!("/v1/runs/{run_id}"));
        let events = http_json_get(port, &format!("/v1/runs/{run_id}/events"));
        assert_eq!(run["status"], "failed");
        assert_eq!(run["error"]["code"], "gateway_planner_failed");
        assert!(!run.to_string().contains("restart-secret"));
        assert_eq!(events["events"][1]["kind"], "capability_failed");
        shutdown_tx.send(()).expect("shutdown second daemon");
        server.join().expect("second daemon").expect("second serve");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn daemon_recovers_preexisting_running_run_after_bind() {
        let root = unique_temp_dir("running-run-recovery");
        let path = root.join("runs.sqlite3");
        {
            let mut store = SqliteRunEvidenceStore::open(&path).expect("open seed store");
            store
                .insert_run(
                    json!({
                        "id": "preseeded-running",
                        "capability": "brain.plan",
                        "loom_session_id": "session-preseeded",
                        "status": "running",
                        "input": { "goal": "recover me" }
                    }),
                    vec![RunEventDraft::new(
                        "run_started",
                        json!({ "capability": "brain.plan", "status": "running" }),
                    )
                    .expect("start event")],
                )
                .expect("seed running run");
        }

        let (port, shutdown_tx, server) =
            start_daemon_with_store(&path, BrainPlannerConfig::LocalTemplate);
        let run = http_json_get(port, "/v1/runs/preseeded-running");
        let events = http_json_get(port, "/v1/runs/preseeded-running/events");
        assert_eq!(run["status"], "failed");
        assert_eq!(run["error"]["code"], "daemon_restarted");
        assert_eq!(run["input"]["goal"], "recover me");
        assert_eq!(events["events"].as_array().expect("events").len(), 2);
        assert_eq!(events["events"][1]["kind"], "run_interrupted");
        assert_eq!(events["events"][1]["error"]["code"], "daemon_restarted");
        shutdown_tx.send(()).expect("shutdown daemon");
        server.join().expect("daemon").expect("serve");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn run_store_failure_returns_safe_http_error_without_stopping_daemon() {
        let mut daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        Arc::get_mut(&mut daemon.runtime)
            .expect("exclusive daemon runtime")
            .run_store = Arc::new(Mutex::new(Box::new(FailingRunEvidenceStore)));
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

        let response = http_request(
            address.port(),
            "POST",
            "/v1/invoke",
            Some(
                r#"{"requestId":"run-store-failure","caller":"hook","capability":"brain.plan","input":{"goal":"fail store"}}"#,
            ),
        );
        assert!(response.starts_with("HTTP/1.1 500 Internal Server Error"));
        let body = response_json_body(&response);
        assert_eq!(body["error"]["code"], "run_store_failed");
        assert!(!body.to_string().contains("fixture failure"));

        let health = http_request(address.port(), "GET", "/health", None);
        assert!(health.starts_with("HTTP/1.1 200 OK"));
        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server").expect("serve");
    }

    #[test]
    fn daemon_invokes_tea_ticket_decompose_capability() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let input: serde_json::Value = serde_json::from_str(&shared_tea_brain_provider_example(
            "decompose-request.example.json",
        ))
        .expect("decompose request fixture json");
        let request = serde_json::json!({
            "requestId": "loom-tea-decompose-1",
            "caller": "tea",
            "capability": "tea.ticket.decompose.v1",
            "input": input
        });

        let invoke = http_json_post(address.port(), "/v1/invoke", &request.to_string());

        assert_eq!(invoke["requestId"], "loom-tea-decompose-1");
        assert_eq!(invoke["status"], "succeeded");
        assert_eq!(
            invoke["output"]["run"]["capability"],
            "tea.ticket.decompose.v1"
        );
        assert_eq!(
            invoke["output"]["proposal"]["analysis"]["intent"],
            "engineering_work_order"
        );
        assert_eq!(
            invoke["output"]["proposal"]["analysis"]["recommended_workflow"],
            "loom.tea_ticket_decompose.v1"
        );
        assert!(
            invoke["output"]["proposal"]["plan"]["steps"]
                .as_array()
                .expect("plan steps")
                .len()
                >= 3
        );
        assert_eq!(invoke["output"]["proposal"]["requires_human_review"], true);
        assert!(invoke["output"]["summary"]
            .as_str()
            .expect("summary")
            .contains("Release smoke matrix includes Tea"));

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_accepts_root_contract_local_capability_invoke_fixture() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
        let request = shared_local_capability_example("loom-invoke-request.json");

        let response = http_request(address.port(), "POST", "/v1/invoke", Some(&request));

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "unexpected response: {response}"
        );
        let invoke = response_json_body(&response);
        assert_eq!(invoke["requestId"], "loom-request-1");
        assert_eq!(invoke["status"], "succeeded");
        assert_eq!(invoke["output"]["run"]["capability"], "brain.plan");
        assert_eq!(invoke["output"]["run"]["input"]["goal"], "release smoke");
        assert_eq!(
            invoke["output"]["run"]["input"]["constraints"][0],
            "Hook Talk Loom"
        );
        assert!(invoke["output"]["summary"]
            .as_str()
            .expect("summary")
            .contains("release smoke"));
    }

    #[test]
    fn daemon_returns_structured_failure_for_unknown_capability() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let response = http_request(
            address.port(),
            "POST",
            "/v1/invoke",
            Some(
                r#"{"requestId":"loom-request-unknown","caller":"hook","capability":"unknown.tool","input":{}}"#,
            ),
        );

        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
        let body = response_json_body(&response);
        assert_eq!(body["requestId"], "loom-request-unknown");
        assert_eq!(body["status"], "failed");
        assert_eq!(body["error"]["code"], "unknown_capability");
        assert_eq!(body["error"]["capability"], "unknown.tool");
        assert!(body["error"]["message"]
            .as_str()
            .expect("message")
            .contains("unknown.tool"));

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_returns_structured_errors_for_invalid_invoke_goal_and_missing_route() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let invalid_json = http_request(address.port(), "POST", "/v1/invoke", Some("{"));
        assert!(invalid_json.starts_with("HTTP/1.1 400 Bad Request"));
        let invalid_json_body = response_json_body(&invalid_json);
        assert_eq!(invalid_json_body["status"], "failed");
        assert_eq!(invalid_json_body["error"]["code"], "invalid_request");

        let missing_goal = http_request(
            address.port(),
            "POST",
            "/v1/invoke",
            Some(
                r#"{"requestId":"loom-request-missing-goal","caller":"hook","capability":"brain.plan","input":{}}"#,
            ),
        );
        assert!(missing_goal.starts_with("HTTP/1.1 400 Bad Request"));
        let missing_goal_body = response_json_body(&missing_goal);
        assert_eq!(missing_goal_body["requestId"], "loom-request-missing-goal");
        assert_eq!(missing_goal_body["status"], "failed");
        assert_eq!(missing_goal_body["error"]["code"], "invalid_input");
        assert!(missing_goal_body["error"]["message"]
            .as_str()
            .expect("message")
            .contains("goal"));

        let not_found = http_request(address.port(), "GET", "/v1/does-not-exist", None);
        assert!(not_found.starts_with("HTTP/1.1 404 Not Found"));
        let not_found_body = response_json_body(&not_found);
        assert_eq!(not_found_body["error"]["code"], "not_found");

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_validates_stop_and_retry_path_run_ids() {
        let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let invoke = http_json_post(
            address.port(),
            "/v1/invoke",
            r#"{"requestId":"loom-request-run-action","caller":"hook","capability":"brain.plan","input":{"goal":"validate run id"}}"#,
        );
        let run_id = invoke["output"]["runId"].as_str().expect("run id");
        let run = invoke["output"]["run"].clone();
        let mismatched_body = format!(r#"{{"run":{run}}}"#);

        let stop_response = http_request(
            address.port(),
            "POST",
            "/v1/runs/not-the-run/stop",
            Some(&mismatched_body),
        );
        assert!(stop_response.starts_with("HTTP/1.1 400 Bad Request"));
        let stop_error = response_json_body(&stop_response);
        assert_eq!(stop_error["error"]["code"], "run_id_mismatch");
        assert_eq!(stop_error["error"]["path_run_id"], "not-the-run");
        assert_eq!(stop_error["error"]["body_run_id"], run_id);

        let retry_response = http_request(
            address.port(),
            "POST",
            "/v1/runs/not-the-run/retry",
            Some(&mismatched_body),
        );
        assert!(retry_response.starts_with("HTTP/1.1 400 Bad Request"));
        let retry_error = response_json_body(&retry_response);
        assert_eq!(retry_error["error"]["code"], "run_id_mismatch");
        assert_eq!(retry_error["error"]["path_run_id"], "not-the-run");
        assert_eq!(retry_error["error"]["body_run_id"], run_id);

        let unknown_run_body = r#"{"run":{"id":"missing-run","status":"succeeded"}}"#;
        let missing_run_response = http_request(
            address.port(),
            "POST",
            "/v1/runs/missing-run/stop",
            Some(unknown_run_body),
        );
        assert!(missing_run_response.starts_with("HTTP/1.1 404 Not Found"));
        let missing_run_error = response_json_body(&missing_run_response);
        assert_eq!(missing_run_error["error"]["code"], "run_not_found");

        let forged = serde_json::json!({
            "run": {
                "id": run_id,
                "status": "succeeded",
                "input": { "goal": "forged" },
                "output": { "summary": "forged" }
            }
        });
        let stopped = http_json_post(
            address.port(),
            &format!("/v1/runs/{run_id}/stop"),
            &forged.to_string(),
        );
        assert_eq!(stopped["status"], "stopped");
        assert_eq!(stopped["input"]["goal"], "validate run id");
        assert_ne!(stopped["output"]["summary"], "forged");

        let retry_body = format!(r#"{{"run":{stopped}}}"#);
        let retrying = http_json_post(
            address.port(),
            &format!("/v1/runs/{run_id}/retry"),
            &retry_body,
        );
        assert_eq!(retrying["status"], "retrying");

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_config_supports_non_loopback_bind_host_for_containers() {
        let config = DaemonConfig::bind_host("0.0.0.0", 8765);

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8765);
    }

    #[test]
    fn daemon_rejects_discovery_manifest_for_non_loopback_bind_host() {
        let temp_dir = unique_temp_dir("non-loopback-manifest");
        let manifest_dir = temp_dir.join("capabilities");

        let bind_error = match LoomDaemon::bind(
            DaemonConfig::bind_host("0.0.0.0", 0)
                .with_bearer_token("local-token")
                .with_manifest_dir(&manifest_dir),
        ) {
            Ok(_) => panic!("non-loopback manifest bind should fail"),
            Err(error) => error,
        };

        assert!(bind_error
            .to_string()
            .contains("loom discovery manifest requires a loopback bind host"));
        assert!(
            !manifest_dir.join("loom.json").exists(),
            "non-loopback manifest bind should not write loom.json"
        );
    }

    #[test]
    fn daemon_requires_bearer_token_for_non_loopback_mutating_routes() {
        let bind_error = LoomDaemon::bind(DaemonConfig::bind_host("0.0.0.0", 0))
            .err()
            .expect("non-loopback daemon binds require an auth token");
        assert!(bind_error.to_string().contains("auth token"));

        let daemon = LoomDaemon::bind(
            DaemonConfig::bind_host("0.0.0.0", 0).with_bearer_token("local-token"),
        )
        .expect("bind daemon with token");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let unauthorized = http_request(
            address.port(),
            "POST",
            "/v1/invoke",
            Some(
                r#"{"requestId":"loom-request-auth","caller":"hook","capability":"brain.plan","input":{"goal":"token protected"}}"#,
            ),
        );
        assert!(unauthorized.starts_with("HTTP/1.1 401 Unauthorized"));
        let unauthorized_body = response_json_body(&unauthorized);
        assert_eq!(unauthorized_body["error"]["code"], "unauthorized");

        let authorized = http_request_with_bearer(
            address.port(),
            "POST",
            "/v1/invoke",
            Some(
                r#"{"requestId":"loom-request-auth","caller":"hook","capability":"brain.plan","input":{"goal":"token protected"}}"#,
            ),
            "local-token",
        );
        assert!(authorized.starts_with("HTTP/1.1 200 OK"));
        let authorized_body = response_json_body(&authorized);
        assert_eq!(authorized_body["requestId"], "loom-request-auth");
        assert_eq!(authorized_body["status"], "succeeded");
        let run_id = authorized_body["output"]["runId"].as_str().expect("run id");

        let unauthorized_status = http_get(address.port(), "/status");
        assert!(
            unauthorized_status.starts_with("HTTP/1.1 401 Unauthorized"),
            "unauthorized_status={unauthorized_status}"
        );
        let unauthorized_capabilities = http_get(address.port(), "/v1/capabilities");
        assert!(
            unauthorized_capabilities.starts_with("HTTP/1.1 401 Unauthorized"),
            "unauthorized_capabilities={unauthorized_capabilities}"
        );
        let unauthorized_run = http_get(address.port(), &format!("/v1/runs/{run_id}"));
        assert!(
            unauthorized_run.starts_with("HTTP/1.1 401 Unauthorized"),
            "unauthorized_run={unauthorized_run}"
        );
        let unauthorized_events = http_get(address.port(), &format!("/v1/runs/{run_id}/events"));
        assert!(
            unauthorized_events.starts_with("HTTP/1.1 401 Unauthorized"),
            "unauthorized_events={unauthorized_events}"
        );

        let public_health = http_get(address.port(), "/health");
        assert!(
            public_health.starts_with("HTTP/1.1 200 OK"),
            "public_health={public_health}"
        );
        let authorized_events = http_request_with_bearer(
            address.port(),
            "GET",
            &format!("/v1/runs/{run_id}/events"),
            None,
            "local-token",
        );
        assert!(
            authorized_events.starts_with("HTTP/1.1 200 OK"),
            "authorized_events={authorized_events}"
        );

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    #[test]
    fn daemon_rejects_oversized_declared_request_body() {
        let daemon = LoomDaemon::bind(
            DaemonConfig::bind_host("0.0.0.0", 0).with_bearer_token("local-token"),
        )
        .expect("bind daemon with token");
        let address = daemon.local_addr().expect("local address");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

        let response = http_request_with_declared_content_length(
            address.port(),
            "POST",
            "/v1/invoke",
            2 * 1024 * 1024,
            Some("local-token"),
        );
        assert!(
            response.starts_with("HTTP/1.1 413 Payload Too Large"),
            "response={response}"
        );

        shutdown_tx.send(()).expect("shutdown");
        server.join().expect("server thread");
    }

    fn http_get(port: u16, path: &str) -> String {
        http_request(port, "GET", path, None)
    }

    fn http_post(port: u16, path: &str, body: &str) -> String {
        let response = http_request(port, "POST", path, Some(body));
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "unexpected response: {response}"
        );
        response
            .split_once("\r\n\r\n")
            .expect("response body")
            .1
            .to_string()
    }

    fn http_json_get(port: u16, path: &str) -> serde_json::Value {
        let response = http_get(port, path);
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "unexpected response: {response}"
        );
        response_json_body(&response)
    }

    fn http_json_post(port: u16, path: &str, body: &str) -> serde_json::Value {
        let response = http_request(port, "POST", path, Some(body));
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "unexpected response: {response}"
        );
        response_json_body(&response)
    }

    fn http_json_put(port: u16, path: &str, body: &str) -> serde_json::Value {
        let response = http_request(port, "PUT", path, Some(body));
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "unexpected response: {response}"
        );
        response_json_body(&response)
    }

    fn shared_local_capability_example(name: &str) -> String {
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join("local-capability")
                .join(name),
        )
        .expect("read standalone local capability fixture")
    }

    fn shared_tea_brain_provider_example(name: &str) -> String {
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join("tea-brain-provider")
                .join(name),
        )
        .expect("read standalone Tea BrainProvider fixture")
    }

    fn http_request(port: u16, method: &str, path: &str, body: Option<&str>) -> String {
        http_request_with_extra_headers(port, method, path, body, "")
    }

    fn http_request_with_bearer(
        port: u16,
        method: &str,
        path: &str,
        body: Option<&str>,
        token: &str,
    ) -> String {
        http_request_with_extra_headers(
            port,
            method,
            path,
            body,
            &format!("Authorization: Bearer {token}\r\n"),
        )
    }

    fn http_request_with_extra_headers(
        port: u16,
        method: &str,
        path: &str,
        body: Option<&str>,
        extra_headers: &str,
    ) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect daemon");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set timeout");
        if let Some(body) = body {
            write!(
                stream,
                "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write request");
        } else {
            write!(
                stream,
                "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{extra_headers}Connection: close\r\n\r\n"
            )
            .expect("write request");
        }

        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        response
    }

    fn http_request_with_declared_content_length(
        port: u16,
        method: &str,
        path: &str,
        content_length: usize,
        token: Option<&str>,
    ) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect daemon");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        let authorization = token
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {content_length}\r\n{authorization}Connection: close\r\n\r\n",
        )
        .expect("write request");
        stream.shutdown(Shutdown::Write).expect("shutdown write");

        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        response
    }

    fn response_json_body(response: &str) -> serde_json::Value {
        let body = response.split_once("\r\n\r\n").expect("response body").1;
        serde_json::from_str(body).expect("json body")
    }

    fn restore_env(name: &str, value: Option<String>) {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }

    fn current_test_binary_mcp_fixture_config() -> serde_json::Value {
        current_test_binary_mcp_fixture_config_with_env(&[])
    }

    fn current_test_binary_mcp_fixture_config_with_env(
        extra_env: &[(&str, String)],
    ) -> serde_json::Value {
        let exe = std::env::current_exe().expect("current test executable");
        let mut env = serde_json::Map::new();
        env.insert(
            "LOOM_DAEMON_MCP_FIXTURE_SERVER".to_owned(),
            Value::String("1".to_owned()),
        );
        for (key, value) in extra_env {
            env.insert((*key).to_owned(), Value::String(value.clone()));
        }
        serde_json::json!({
            "id": "fixture",
            "name": "Fixture MCP",
            "command": exe.display().to_string(),
            "args": [
                "tests::daemon_mcp_fixture_server",
                "--exact",
                "--nocapture"
            ],
            "env": env,
            "enabled": true
        })
    }

    fn run_mcp_fixture_server() {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let fixture_image_url = std::env::var("LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL").ok();
        let fixture_image_url_alt = std::env::var("LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL_ALT").ok();

        for line in stdin.lock().lines() {
            let line = line.expect("fixture stdin line");
            let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let method = request["method"].as_str().unwrap_or_default();
            match method {
                "initialize" => write_fixture_response(
                    &mut stdout,
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"].clone(),
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": { "tools": {} },
                            "serverInfo": {
                                "name": "daemon-fixture",
                                "version": "0.1.0"
                            }
                        }
                    }),
                ),
                "notifications/initialized" => {}
                "tools/list" => write_fixture_response(
                    &mut stdout,
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"].clone(),
                        "result": {
                            "tools": [
                                {
                                    "name": "echo",
                                    "description": "Echo arguments",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "text": { "type": "string" }
                                        }
                                    }
                                },
                                {
                                    "name": "brave_image_search",
                                    "description": "Return structured image-search results",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "query": { "type": "string" },
                                            "count": { "type": "integer" },
                                            "search_lang": {
                                                "type": "string",
                                                "enum": ["zh-hans", "en"]
                                            },
                                            "spellcheck": { "type": "boolean" }
                                        },
                                        "required": ["query"]
                                    }
                                },
                                {
                                    "name": "brave_image_search_realshape",
                                    "description": "Return structured image-search results with Brave-like string-only search_lang schema",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "query": { "type": "string" },
                                            "count": { "type": "integer" },
                                            "search_lang": { "type": "string" },
                                            "spellcheck": { "type": "boolean" }
                                        },
                                        "required": ["query"]
                                    }
                                }
                            ]
                        }
                    }),
                ),
                "tools/call" => {
                    let tool_name = request["params"]["name"].as_str().unwrap_or_default();
                    match tool_name {
                        "echo" => {
                            let text = request["params"]["arguments"]["text"]
                                .as_str()
                                .unwrap_or_default();
                            write_fixture_response(
                                &mut stdout,
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": request["id"].clone(),
                                    "result": {
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": text
                                            }
                                        ]
                                    }
                                }),
                            );
                        }
                        "brave_image_search" | "brave_image_search_realshape" => {
                            let arguments = &request["params"]["arguments"];
                            if arguments.get("count").is_some()
                                && !arguments["count"].is_i64()
                                && !arguments["count"].is_u64()
                            {
                                write_fixture_response(
                                    &mut stdout,
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": request["id"].clone(),
                                        "error": {
                                            "code": -32602,
                                            "message": "count must be an integer"
                                        }
                                    }),
                                );
                                continue;
                            }
                            if arguments.get("spellcheck").is_some()
                                && !arguments["spellcheck"].is_boolean()
                            {
                                write_fixture_response(
                                    &mut stdout,
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": request["id"].clone(),
                                        "error": {
                                            "code": -32602,
                                            "message": "spellcheck must be a boolean"
                                        }
                                    }),
                                );
                                continue;
                            }
                            if let Some(search_lang) = arguments
                                .get("search_lang")
                                .and_then(serde_json::Value::as_str)
                            {
                                if !matches!(search_lang, "zh-hans" | "en") {
                                    write_fixture_response(
                                        &mut stdout,
                                        serde_json::json!({
                                            "jsonrpc": "2.0",
                                            "id": request["id"].clone(),
                                            "error": {
                                                "code": -32602,
                                                "message": "search_lang must be one of [\"zh-hans\", \"en\"]"
                                            }
                                        }),
                                    );
                                    continue;
                                }
                            } else if arguments.get("search_lang").is_some() {
                                write_fixture_response(
                                    &mut stdout,
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": request["id"].clone(),
                                        "error": {
                                            "code": -32602,
                                            "message": "search_lang must be a string"
                                        }
                                    }),
                                );
                                continue;
                            }
                            let query = request["params"]["arguments"]["query"]
                                .as_str()
                                .unwrap_or_default();
                            if query.contains("offensive fixture") {
                                write_fixture_response(
                                    &mut stdout,
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": request["id"].clone(),
                                        "result": {
                                            "content": [
                                                {
                                                    "type": "text",
                                                    "text": "{\"type\":\"object\",\"items\":[],\"count\":0,\"might_be_offensive\":true}"
                                                }
                                            ],
                                            "structuredContent": {
                                                "type": "object",
                                                "items": [],
                                                "count": 0,
                                                "might_be_offensive": true
                                            }
                                        }
                                    }),
                                );
                                continue;
                            }
                            let image_url = fixture_image_url.clone().unwrap_or_else(|| {
                                "https://example.invalid/fixture.png".to_owned()
                            });
                            let alternate_image_url = fixture_image_url_alt
                                .clone()
                                .unwrap_or_else(|| image_url.clone());
                            write_fixture_response(
                                &mut stdout,
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": request["id"].clone(),
                                    "result": {
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": format!("fixture brave_image_search results for {query}")
                                            }
                                        ],
                                        "structuredContent": {
                                            "type": "object",
                                            "items": [
                                                {
                                                    "title": "Fixture image",
                                                    "url": "https://example.invalid/page",
                                                    "properties": {
                                                        "url": image_url,
                                                        "width": 1,
                                                        "height": 1
                                                    }
                                                },
                                                {
                                                    "title": "Fixture image alternate",
                                                    "url": "https://example.invalid/page-2",
                                                    "properties": {
                                                        "url": alternate_image_url,
                                                        "width": 1,
                                                        "height": 1
                                                    }
                                                }
                                            ]
                                        }
                                    }
                                }),
                            );
                        }
                        _ => write_fixture_response(
                            &mut stdout,
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": request["id"].clone(),
                                "error": {
                                    "code": -32601,
                                    "message": format!("unknown tool {tool_name}")
                                }
                            }),
                        ),
                    }
                }
                _ => write_fixture_response(
                    &mut stdout,
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"].clone(),
                        "error": {
                            "code": -32601,
                            "message": format!("unknown method {method}")
                        }
                    }),
                ),
            }
        }
    }

    fn write_fixture_response(stdout: &mut impl Write, response: serde_json::Value) {
        writeln!(
            stdout,
            "\n{}",
            serde_json::to_string(&response).expect("serialize fixture response")
        )
        .expect("write fixture response");
        stdout.flush().expect("flush fixture response");
    }
}
