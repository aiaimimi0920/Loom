// Desktop settings, cache, proxy, shortcut, and application-path contracts.

export interface LoomShortcutConfig {
  id: string;
  label: string;
  keys: string;
  enabled: boolean;
}

export interface LoomSettings {
  appearance_version: number;
  general: {
    theme: string;
    language: string;
    auto_start: boolean;
    minimize_to_tray: boolean;
    enable_tray_icon: boolean;
  };
  hook_general: {
    theme: string;
    language: string;
    close_to_tray: boolean;
  };
  system: {
    auto_check_updates: boolean;
    enable_run_log: boolean;
    loom_log_level: string;
    hook_log_level: string;
    run_as_admin: boolean;
    record_screenshot_history: boolean;
    history_retention: string;
  };
  network: {
    loom: LoomProxySettings;
    hook: LoomProxySettings;
  };
  mcp: LoomMcpSettings;
  art_store: LoomArtStoreSettings;
  loom_cache: LoomCacheSettings;
  hook_cache: HookCacheSettings;
  engine: {
    comfyui_url: string;
    python_interpreter: string;
    virtual_env_path: string;
    compute_device: string;
    vram_reservation_gb: number;
  };
  quick_bindings: Array<{
    id: string;
    art: string;
    key: string;
  }>;
  shortcuts: Record<string, LoomShortcutConfig>;
}

export interface LoomMcpSettings {
  request_timeout_seconds: number;
  memory_limit_bytes: number;
}

export interface LoomArtStoreSettings {
  auto_update: boolean;
  official_only: boolean;
}

export interface LoomCacheSettings {
  art_cache_max_bytes: number;
  art_cache_retention_days: number;
  framework_temp_retention_days: number;
}

export interface HookCacheSettings {
  recycle_bin_max_entries: number;
  recycle_bin_retention_days: number;
  temp_cache_max_bytes: number;
  temp_cache_retention_days: number;
}

export interface LoomProxySettings {
  mode: "system" | "custom" | "disabled";
  protocol: "http" | "https" | "socks5";
  address: string;
}

export interface LoomAppPaths {
  dataDir: string;
  configDir: string;
  logDir: string;
}
