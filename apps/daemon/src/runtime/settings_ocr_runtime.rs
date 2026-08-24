// Runtime logging, settings defaults, shortcut models, and OCR provider ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
enum RuntimeLogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}

static RUNTIME_LOG_LEVEL: AtomicU8 = AtomicU8::new(RuntimeLogLevel::Info as u8);

fn parse_runtime_log_level(value: &str) -> RuntimeLogLevel {
    match value {
        "error" => RuntimeLogLevel::Error,
        "warn" => RuntimeLogLevel::Warn,
        "debug" => RuntimeLogLevel::Debug,
        _ => RuntimeLogLevel::Info,
    }
}

fn configure_runtime_log_level(value: &str) {
    RUNTIME_LOG_LEVEL.store(parse_runtime_log_level(value) as u8, Ordering::Relaxed);
}

fn runtime_log_enabled(level: RuntimeLogLevel) -> bool {
    level as u8 <= RUNTIME_LOG_LEVEL.load(Ordering::Relaxed)
}

fn runtime_log(level: RuntimeLogLevel, message: &str) {
    if !runtime_log_enabled(level) {
        return;
    }
    let label = match level {
        RuntimeLogLevel::Error => "ERROR",
        RuntimeLogLevel::Warn => "WARN",
        RuntimeLogLevel::Info => "INFO",
        RuntimeLogLevel::Debug => "DEBUG",
    };
    eprintln!("[{label}] {message}");
    let log_dir = std::env::var_os("LOOM_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_control_plane_root().join("logs"));
    if fs::create_dir_all(&log_dir).is_ok() {
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("loom-daemon.log"))
        {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or_default();
            let _ = writeln!(file, "{timestamp} [{label}] {message}");
        }
    }
}

pub fn runtime_log_info(message: impl AsRef<str>) {
    runtime_log(RuntimeLogLevel::Info, message.as_ref());
}

fn runtime_log_error(message: impl AsRef<str>) {
    runtime_log(RuntimeLogLevel::Error, message.as_ref());
}

fn runtime_log_warn(message: impl AsRef<str>) {
    runtime_log(RuntimeLogLevel::Warn, message.as_ref());
}

fn runtime_log_debug(message: impl AsRef<str>) {
    runtime_log(RuntimeLogLevel::Debug, message.as_ref());
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ProxySettings {
    mode: String,
    protocol: String,
    address: String,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            mode: "system".to_owned(),
            protocol: "http".to_owned(),
            address: String::new(),
        }
    }
}

impl ProxySettings {
    fn validate(&self) -> std::result::Result<(), String> {
        if !matches!(self.mode.as_str(), "system" | "custom" | "disabled") {
            return Err("代理模式必须是跟随系统、自定义或不使用代理".to_owned());
        }
        if !matches!(self.protocol.as_str(), "http" | "https" | "socks5") {
            return Err("代理协议必须是 http、https 或 socks5".to_owned());
        }
        if self.mode == "custom" {
            let address = self.address.trim();
            if address.is_empty() {
                return Err("自定义代理必须填写地址".to_owned());
            }
            let url = format!("{}://{address}", self.protocol);
            let parsed = reqwest::Url::parse(&url)
                .map_err(|error| format!("自定义代理地址无效：{error}"))?;
            if parsed.host_str().is_none() || parsed.port_or_known_default().is_none() {
                return Err("自定义代理地址必须包含主机和端口".to_owned());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct NetworkSettings {
    #[serde(default)]
    loom: ProxySettings,
    #[serde(default)]
    hook: ProxySettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct HookCacheSettings {
    recycle_bin_max_entries: u32,
    recycle_bin_retention_days: u32,
    temp_cache_max_bytes: u64,
    temp_cache_retention_days: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HookCacheSettingsWire {
    recycle_bin_max_entries: u32,
    recycle_bin_retention_days: u32,
    temp_cache_max_bytes: u64,
    temp_cache_retention_days: u32,
}

impl From<&HookCacheSettings> for HookCacheSettingsWire {
    fn from(settings: &HookCacheSettings) -> Self {
        Self {
            recycle_bin_max_entries: settings.recycle_bin_max_entries,
            recycle_bin_retention_days: settings.recycle_bin_retention_days,
            temp_cache_max_bytes: settings.temp_cache_max_bytes,
            temp_cache_retention_days: settings.temp_cache_retention_days,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct McpSettings {
    request_timeout_seconds: u64,
    memory_limit_bytes: u64,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            request_timeout_seconds: 60,
            memory_limit_bytes: 512 * 1024 * 1024,
        }
    }
}

impl McpSettings {
    fn validate(&self) -> std::result::Result<(), String> {
        if !(5..=600).contains(&self.request_timeout_seconds) {
            return Err("MCP 请求超时必须介于 5 秒到 10 分钟之间".to_owned());
        }
        if !(64 * 1024 * 1024..=4 * 1024 * 1024 * 1024).contains(&self.memory_limit_bytes) {
            return Err("MCP 子进程内存上限必须介于 64 MB 到 4 GB 之间".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ArtStoreSettings {
    auto_update: bool,
    official_only: bool,
}

impl Default for ArtStoreSettings {
    fn default() -> Self {
        Self {
            auto_update: true,
            official_only: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct LoomCacheSettings {
    art_cache_max_bytes: u64,
    art_cache_retention_days: u32,
    framework_temp_retention_days: u32,
}

impl Default for LoomCacheSettings {
    fn default() -> Self {
        Self {
            art_cache_max_bytes: 1024 * 1024 * 1024,
            art_cache_retention_days: 30,
            framework_temp_retention_days: 3,
        }
    }
}

impl LoomCacheSettings {
    fn validate(&self) -> std::result::Result<(), String> {
        if self.art_cache_max_bytes != 0
            && !(64 * 1024 * 1024..=64 * 1024 * 1024 * 1024).contains(&self.art_cache_max_bytes)
        {
            return Err("Art 运行缓存上限必须为无限制或介于 64 MB 到 64 GB 之间".to_owned());
        }
        if self.art_cache_retention_days > 3650 {
            return Err("Art 运行缓存自动清理周期不能超过 3650 天".to_owned());
        }
        if self.framework_temp_retention_days > 3650 {
            return Err("框架临时文件自动清理周期不能超过 3650 天".to_owned());
        }
        Ok(())
    }
}

impl Default for HookCacheSettings {
    fn default() -> Self {
        Self {
            recycle_bin_max_entries: 15,
            // The existing Hook recycle bin never expired by age. Keep that
            // behavior until the user explicitly chooses a retention period.
            recycle_bin_retention_days: 0,
            temp_cache_max_bytes: 256 * 1024 * 1024,
            temp_cache_retention_days: 7,
        }
    }
}

impl HookCacheSettings {
    fn validate(&self) -> std::result::Result<(), String> {
        if self.recycle_bin_max_entries > 500 {
            return Err("回收站上限必须为无限或不超过 500 项".to_owned());
        }
        if self.recycle_bin_retention_days > 3650 {
            return Err("回收站自动清理周期不能超过 3650 天".to_owned());
        }
        if self.temp_cache_max_bytes != 0
            && !(32 * 1024 * 1024..=16 * 1024 * 1024 * 1024).contains(&self.temp_cache_max_bytes)
        {
            return Err("临时缓存上限必须为无限制或介于 32 MB 到 16 GB 之间".to_owned());
        }
        if self.temp_cache_retention_days > 3650 {
            return Err("临时缓存自动清理周期不能超过 3650 天".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct EngineSettings {
    comfyui_url: String,
    python_interpreter: String,
    virtual_env_path: String,
    compute_device: String,
    vram_reservation_gb: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct QuickBinding {
    id: String,
    art: String,
    key: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct LoomSettings {
    appearance_version: u32,
    general: LoomGeneralSettings,
    hook_general: HookGeneralSettings,
    system: LoomSystemPreferences,
    network: NetworkSettings,
    mcp: McpSettings,
    art_store: ArtStoreSettings,
    loom_cache: LoomCacheSettings,
    hook_cache: HookCacheSettings,
    engine: EngineSettings,
    quick_bindings: Vec<QuickBinding>,
    shortcuts: HashMap<String, LoomShortcutConfig>,
}

impl Default for LoomSettings {
    fn default() -> Self {
        let mut shortcuts = HashMap::new();
        for shortcut in default_shortcuts() {
            shortcuts.insert(shortcut.id.clone(), shortcut);
        }
        Self {
            appearance_version: CURRENT_APPEARANCE_VERSION,
            general: LoomGeneralSettings {
                theme: "dark".to_owned(),
                language: "zh-Hans".to_owned(),
                auto_start: false,
                minimize_to_tray: true,
                enable_tray_icon: true,
            },
            hook_general: HookGeneralSettings::default(),
            system: LoomSystemPreferences {
                auto_check_updates: true,
                enable_run_log: true,
                loom_log_level: default_log_level(),
                hook_log_level: default_log_level(),
                run_as_admin: false,
                record_screenshot_history: true,
                history_retention: "7d".to_owned(),
            },
            network: NetworkSettings::default(),
            mcp: McpSettings::default(),
            art_store: ArtStoreSettings::default(),
            loom_cache: LoomCacheSettings::default(),
            hook_cache: HookCacheSettings::default(),
            engine: EngineSettings {
                comfyui_url: "http://127.0.0.1:8188".to_owned(),
                python_interpreter: "python.exe".to_owned(),
                virtual_env_path: "./venv".to_owned(),
                compute_device: "0".to_owned(),
                vram_reservation_gb: 12,
            },
            quick_bindings: Vec::new(),
            shortcuts,
        }
    }
}

fn hook_settings_protocol_value(settings: &LoomSettings) -> serde_json::Value {
    let mut value = serde_json::to_value(settings).unwrap_or_default();
    if let Some(object) = value.as_object_mut() {
        object.remove("hook_cache");
        object.insert(
            "hookCache".to_owned(),
            serde_json::to_value(HookCacheSettingsWire::from(&settings.hook_cache))
                .unwrap_or_default(),
        );
    }
    value
}

const CURRENT_APPEARANCE_VERSION: u32 = 1;

fn apply_runtime_settings(settings: &LoomSettings) {
    loom_mcp::configure_runtime_limits(
        settings.mcp.request_timeout_seconds,
        settings.mcp.memory_limit_bytes,
    );
    if let Err(error) = loom_tool_registry::network_policy::configure_runtime_proxy(
        &settings.network.loom.mode,
        &settings.network.loom.protocol,
        &settings.network.loom.address,
    ) {
        runtime_log_error(format!("apply Loom proxy settings failed: {error}"));
    }
    if let Err(error) = loom_gateway::configure_runtime_proxy(
        &settings.network.loom.mode,
        &settings.network.loom.protocol,
        &settings.network.loom.address,
    ) {
        runtime_log_error(format!("apply Gateway proxy settings failed: {error}"));
    }
    configure_runtime_log_level(&settings.system.loom_log_level);
}

struct LoomSettingsStore {
    path: PathBuf,
    settings: LoomSettings,
}

impl LoomSettingsStore {
    fn new(path: PathBuf) -> Self {
        let settings = match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<LoomSettings>(&content) {
                Ok(settings) => settings,
                // Defaulting silently here used to be invisible, and the next `save` would then
                // overwrite the user's real settings with those defaults. Settings are recoverable
                // configuration rather than authorization state, so this degrades instead of
                // refusing to start — but the previous bytes are moved aside first.
                Err(error) => {
                    quarantine_unreadable_file(
                        &path,
                        &format!("unparsable Loom settings: {error}"),
                    );
                    LoomSettings::default()
                }
            },
            // An absent settings file is the normal first-run state.
            Err(error) if error.kind() == ErrorKind::NotFound => LoomSettings::default(),
            Err(error) => {
                eprintln!(
                    "[WARN] loom could not read settings `{}`, continuing with defaults: {error}",
                    path.display()
                );
                LoomSettings::default()
            }
        };
        Self { path, settings }
    }

    fn save(&self) -> Result<()> {
        write_json_atomically(&self.path, &self.settings)
            .with_context(|| format!("write Loom settings {}", self.path.display()))
    }
}

fn default_shortcuts() -> Vec<LoomShortcutConfig> {
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
    .map(|(id, label, keys)| LoomShortcutConfig {
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
