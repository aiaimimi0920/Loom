//! Shared constants, process state, and public diagnostics/runtime models.

use super::*;

pub(super) const DEFAULT_LOOM_DAEMON_URL: &str = "http://127.0.0.1:8765";
pub(super) const DEFAULT_HOOK_BRIDGE_URL: &str = "ws://127.0.0.1:19820";
pub(super) const HOOK_COMPANION_VERSION: &str = "0.1.7";
pub(super) const LOOM_DAEMON_EXECUTABLE_ENV: &str = "LOOM_DAEMON_EXECUTABLE";
pub(super) const FRAMEWORK_PACKAGE_CATALOG_ENV: &str = "LOOM_FRAMEWORK_PACKAGE_CATALOG_DIR";
pub(super) const FRAMEWORK_PACKAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const ART_PACKAGE_CATALOG_ENV: &str = "LOOM_ART_PACKAGE_CATALOG_DIR";
pub(super) const MCP_SERVER_PACKAGE_CATALOG_ENV: &str = "LOOM_MCP_SERVER_PACKAGE_CATALOG_DIR";
pub(super) const BUNDLED_ART_SHA256_ALLOWLIST_ENV: &str = "LOOM_BUNDLED_ART_SHA256_ALLOWLIST";
pub(super) const DAEMON_AUTH_TOKEN_FILE: &str = "daemon-token";
pub(super) const ART_PACKAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const MCP_SERVER_PACKAGE_MAX_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const LOOM_DAEMON_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub(super) const LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
pub(super) const LOOM_MCP_REGISTRY_REQUEST_TIMEOUT: Duration = Duration::from_secs(50);
pub(super) const MAX_DAEMON_JSON_REQUEST_BYTES: usize = 96 * 1024 * 1024;
pub(super) const MAX_DAEMON_JSON_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_DAEMON_BINARY_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_DAEMON_RESPONSE_HEADER_BYTES: usize = 64 * 1024;
pub(super) const OFFICIAL_FRAMEWORK_IDS: [&str; 4] = ["process", "cloud_api", "mcp", "workflow"];
pub(super) static PACKAGED_ART_BOOTSTRAP_LOCK: Mutex<()> = Mutex::new(());
pub(super) static DAEMON_START_LOCK: Mutex<()> = Mutex::new(());
pub(super) static ACTIVE_DAEMON_URL: OnceLock<Mutex<Option<String>>> = OnceLock::new();
pub(super) static ACTIVE_HOOK_BRIDGE_URL: OnceLock<Mutex<Option<String>>> = OnceLock::new();
pub(super) static OWNED_DAEMON_PROCESS: OnceLock<Mutex<Option<std::process::Child>>> =
    OnceLock::new();
pub(super) static LOOM_CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(true);
pub(super) static LOOM_EXITING: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
pub(super) const LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV: &str =
    "LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT";
#[cfg(target_os = "windows")]
pub(super) const DEFAULT_WEBVIEW2_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRuntimeConfig {
    pub loom_daemon_url: String,
    pub settings_url: String,
    pub hook_bridge_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsLinks {
    pub root: String,
    pub tea: String,
    pub hook: String,
    pub talk: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationDiagnostics {
    pub app: String,
    pub app_name: String,
    pub version: String,
    pub repository_url: Option<String>,
    pub commit_short: Option<String>,
    pub log_dir: String,
    pub log_file: Option<String>,
    pub log_file_exists: bool,
}
