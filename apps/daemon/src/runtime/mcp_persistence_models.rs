// MCP server persistence plus daemon request and settings data models.
fn mcp_server_store_path(control_plane_root: &Path) -> PathBuf {
    control_plane_root.join("mcp").join("servers.json")
}

fn mcp_registry_cache_path(control_plane_root: &Path) -> PathBuf {
    control_plane_root.join("mcp").join("registry-cache.json")
}

fn load_persisted_mcp_servers(control_plane_root: &Path) -> HashMap<String, McpServerConfig> {
    let path = mcp_server_store_path(control_plane_root);
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        // An absent store is the normal first-run state.
        Err(error) if error.kind() == ErrorKind::NotFound => return HashMap::new(),
        Err(error) => {
            eprintln!(
                "[WARN] loom could not read the MCP server store `{}`: {error}",
                path.display()
            );
            return HashMap::new();
        }
    };

    match serde_json::from_str::<Vec<McpServerConfig>>(&content) {
        Ok(servers) => servers
            .into_iter()
            .map(|server| (server.id.clone(), server))
            .collect(),
        // The store is authoritative, so an unparsable file must not be silently overwritten by
        // the next snapshot: move it aside first, which keeps the configured servers recoverable.
        // Unlike the device registry this is re-addable configuration and carries no revocation
        // state, so it degrades to an empty list rather than refusing to start.
        Err(error) => {
            quarantine_unreadable_file(&path, &format!("unparsable MCP server store: {error}"));
            HashMap::new()
        }
    }
}

fn persist_mcp_servers_snapshot(
    path: &Path,
    servers: &HashMap<String, McpServerConfig>,
) -> Result<()> {
    let mut ordered = servers.values().cloned().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.id.cmp(&right.id));
    write_json_atomically(path, &ordered)
        .with_context(|| format!("write MCP server store {}", path.display()))
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpRegistryCache {
    schema_version: u32,
    #[serde(default)]
    entries: BTreeMap<String, McpRegistryCacheEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpRegistryCacheEntry {
    fetched_at_ms: u64,
    response: Value,
}

fn load_mcp_registry_cache(path: &Path) -> McpRegistryCache {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<McpRegistryCache>(&bytes).ok())
        .filter(|cache| cache.schema_version == MCP_REGISTRY_CACHE_SCHEMA_VERSION)
        .unwrap_or_else(|| McpRegistryCache {
            schema_version: MCP_REGISTRY_CACHE_SCHEMA_VERSION,
            entries: BTreeMap::new(),
        })
}

fn persist_mcp_registry_cache(path: &Path, cache: &McpRegistryCache) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("MCP registry cache path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create MCP registry cache dir {}", parent.display()))?;
    let mut bytes = serde_json::to_vec_pretty(cache)?;
    bytes.push(b'\n');
    let (temporary, mut file) = create_sensitive_temporary(path).with_context(|| {
        format!(
            "create MCP registry cache temporary in {}",
            parent.display()
        )
    })?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)
            .with_context(|| format!("write MCP registry cache {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("flush MCP registry cache {}", temporary.display()))?;
        drop(file);
        replace_sensitive_file(&temporary, path)
            .with_context(|| format!("replace MCP registry cache {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn annotate_mcp_registry_response(
    mut response: Value,
    source: &str,
    stale: bool,
    fetched_at_ms: u64,
) -> Value {
    if let Some(object) = response.as_object_mut() {
        object.insert(
            "loomRegistry".to_owned(),
            json!({
                "provider": "official",
                "source": source,
                "stale": stale,
                "fetchedAtMs": fetched_at_ms,
            }),
        );
    }
    response
}

fn cache_mcp_registry_response(path: &Path, key: &str, response: &Value, fetched_at_ms: u64) {
    let mut cache = load_mcp_registry_cache(path);
    cache.entries.insert(
        key.to_owned(),
        McpRegistryCacheEntry {
            fetched_at_ms,
            response: response.clone(),
        },
    );
    if cache.entries.len() > MCP_REGISTRY_CACHE_MAX_ENTRIES {
        let mut oldest = cache
            .entries
            .iter()
            .map(|(key, entry)| (entry.fetched_at_ms, key.clone()))
            .collect::<Vec<_>>();
        oldest.sort();
        let remove_count = cache
            .entries
            .len()
            .saturating_sub(MCP_REGISTRY_CACHE_MAX_ENTRIES);
        for (_, key) in oldest.into_iter().take(remove_count) {
            cache.entries.remove(&key);
        }
    }
    if let Err(error) = persist_mcp_registry_cache(path, &cache) {
        runtime_log_error(format!("persist MCP Registry cache failed: {error:#}"));
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct StatusResponse {
    status: &'static str,
    pid: u32,
    #[serde(rename = "executablePath")]
    executable_path: String,
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
    #[serde(default)]
    zip_base64: String,
}

#[derive(Debug, Deserialize)]
struct PythonSourceReadRequest {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PythonArtJsonReadRequest {
    #[serde(default)]
    art_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PythonNearbyArtJsonRequest {
    #[serde(default)]
    python_path: String,
}

#[derive(Debug, Deserialize)]
struct PythonInferPortsRequest {
    #[serde(default)]
    code: String,
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize)]
struct StartHookBridgeRequest {
    port: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpPackageCheckRequest {
    #[serde(default)]
    module_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpPackageInstallPlanRequest {
    #[serde(default)]
    package_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpServerPackageInstallRequest {
    #[serde(default)]
    zip_base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpServerCredentialUpdateRequest {
    #[serde(default)]
    values: BTreeMap<String, String>,
    #[serde(default)]
    clear: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtUninstallRequest {
    #[serde(default)]
    remove_unused_mcp_servers: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpToolCallRequest {
    #[serde(default)]
    transport: McpTransport,
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_args: Value,
}

#[derive(Debug, Deserialize)]
struct ToggleRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookWorkflowInstantiateHttpRequest {
    #[serde(default)]
    nodes: Vec<Value>,
    #[serde(default)]
    edges: Vec<Value>,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    workflow_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookWorkflowNodeUpdateHttpRequest {
    #[serde(default)]
    workflow_id: String,
    #[serde(default)]
    node_id: String,
    #[serde(default)]
    param: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
type SharedLoomSettingsStore = Arc<Mutex<LoomSettingsStore>>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct LoomShortcutConfig {
    id: String,
    label: String,
    keys: String,
    enabled: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct LoomGeneralSettings {
    theme: String,
    language: String,
    auto_start: bool,
    minimize_to_tray: bool,
    enable_tray_icon: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct HookGeneralSettings {
    theme: String,
    language: String,
    close_to_tray: bool,
}

impl Default for HookGeneralSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_owned(),
            language: "zh-Hans".to_owned(),
            close_to_tray: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct LoomSystemPreferences {
    auto_check_updates: bool,
    enable_run_log: bool,
    #[serde(default = "default_log_level")]
    loom_log_level: String,
    #[serde(default = "default_log_level")]
    hook_log_level: String,
    run_as_admin: bool,
    record_screenshot_history: bool,
    history_retention: String,
}

fn default_log_level() -> String {
    "info".to_owned()
}
