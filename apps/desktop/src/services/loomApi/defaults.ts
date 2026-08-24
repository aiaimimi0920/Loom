// Default local daemon endpoint and desktop settings used when the service omits values.
import type { LoomSettings } from "./settingsTypes.ts";

export const DEFAULT_LOOM_DAEMON_URL = "http://127.0.0.1:8765";

export const DEFAULT_LOOM_SETTINGS: LoomSettings = {
  appearance_version: 1,
  general: {
    theme: "dark",
    language: "zh-Hans",
    auto_start: false,
    minimize_to_tray: true,
    enable_tray_icon: true,
  },
  hook_general: {
    theme: "dark",
    language: "zh-Hans",
    close_to_tray: true,
  },
  system: {
    auto_check_updates: true,
    enable_run_log: true,
    loom_log_level: "info",
    hook_log_level: "info",
    run_as_admin: false,
    record_screenshot_history: true,
    history_retention: "7d",
  },
  network: {
    loom: { mode: "system", protocol: "http", address: "" },
    hook: { mode: "system", protocol: "http", address: "" },
  },
  mcp: {
    request_timeout_seconds: 60,
    memory_limit_bytes: 512 * 1024 * 1024,
  },
  art_store: {
    auto_update: true,
    official_only: false,
  },
  loom_cache: {
    art_cache_max_bytes: 1024 * 1024 * 1024,
    art_cache_retention_days: 30,
    framework_temp_retention_days: 3,
  },
  hook_cache: {
    recycle_bin_max_entries: 15,
    recycle_bin_retention_days: 0,
    temp_cache_max_bytes: 256 * 1024 * 1024,
    temp_cache_retention_days: 7,
  },
  engine: {
    comfyui_url: "http://127.0.0.1:8188",
    python_interpreter: "python.exe",
    virtual_env_path: "./venv",
    compute_device: "0",
    vram_reservation_gb: 12,
  },
  quick_bindings: [],
  shortcuts: {
    cancel: { id: "cancel", label: "Cancel / Deselect", keys: "Escape", enabled: true },
    capture: { id: "capture", label: "Screenshot", keys: "Ctrl+1", enabled: true },
    copy_unit: { id: "copy_unit", label: "Copy Unit", keys: "Ctrl+C", enabled: true },
    paste_unit: { id: "paste_unit", label: "Paste Unit", keys: "Ctrl+V", enabled: true },
    save_image: { id: "save_image", label: "Save Image", keys: "Ctrl+S", enabled: true },
    toggle_ocr: { id: "toggle_ocr", label: "Toggle OCR", keys: "Alt+2", enabled: true },
    toggle_translation: { id: "toggle_translation", label: "Toggle Translation", keys: "Alt+3", enabled: true },
  },
};
