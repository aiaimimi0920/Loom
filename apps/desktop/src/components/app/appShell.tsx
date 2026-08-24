// Shared shell models, safe helpers, and compact visual primitives.
import { artPublisherIconSource, type ArtPublisherIdentity, officialFrameworkDisplayName } from "../../services/artHubUi";
import { keepNewestHookCanvasSnapshot } from "../../services/hookCanvas";
import { DEFAULT_LOOM_DAEMON_URL, LoomPythonPortDefinition, LoomSnapshot } from "../../services/loomApi";
import { type PythonArtPort } from "../../services/pythonArtSource";
import { useEffect, useState } from "react";

export type SectionId =
  | "mcp"
  | "registry"
  | "hook-bridge"
  | "devices"
  | "settings";

export interface RuntimeConfig {
  loomDaemonUrl: string;
  settingsUrl: string;
  hookBridgeUrl?: string;
}

export type ShellIconKind =
  | "sidebar"
  | "back"
  | "mcp"
  | "registry"
  | "hook-bridge"
  | "device"
  | "settings"
  | "refresh"
  | "minimize"
  | "maximize"
  | "close";

export interface NavigationItem {
  id: SectionId;
  label: string;
  eyebrow: string;
  icon: ShellIconKind;
}

export const navigationItems: NavigationItem[] = [
  { id: "mcp", label: "MCP", eyebrow: "服务工具", icon: "mcp" },
  { id: "registry", label: "Art", eyebrow: "", icon: "registry" },
  { id: "hook-bridge", label: "Hook 同步", eyebrow: "", icon: "hook-bridge" },
  { id: "devices", label: "设备管理", eyebrow: "客户端连接", icon: "device" },
  { id: "settings", label: "设置", eyebrow: "配置中心", icon: "settings" },
];

export const primaryNavigationItems = navigationItems.filter(
  (item) => item.id !== "settings" && item.id !== "devices",
);
export const utilityNavigationItems = navigationItems.filter(
  (item) => item.id === "settings",
);

export const fallbackSnapshot: LoomSnapshot = {
  baseUrl: DEFAULT_LOOM_DAEMON_URL,
  connectionState: "offline",
  checkedAt: new Date(0).toISOString(),
  health: null,
  status: null,
  capabilities: [],
  mcpServers: [],
  tools: [],
  pythonArts: [],
  workflows: [],
  hookBridge: null,
  settings: {
    root: `${DEFAULT_LOOM_DAEMON_URL}/settings`,
    tea: `${DEFAULT_LOOM_DAEMON_URL}/settings/tea`,
    hook: `${DEFAULT_LOOM_DAEMON_URL}/settings/hook`,
    talk: `${DEFAULT_LOOM_DAEMON_URL}/settings/talk`,
  },
  error: "尚未加载状态快照",
};

export const DEFAULT_HOOK_BRIDGE_URL = "ws://127.0.0.1:19820";

// Poll the Hook canvas while online so desktop edits (node moves, new captures,
// image changes) sync automatically without a manual refresh. The daemon
// computes a cheap content revision and keepNewestHookCanvasSnapshot dedupes by
// it, so a poll that finds no change does not re-render the canvas.
export const HOOK_CANVAS_POLL_INTERVAL_MS = 1500;

export const firstWords = (value: string | undefined, fallback: string) => {
  if (!value) return fallback;
  return value.length > 96 ? `${value.slice(0, 96)}...` : value;
};

export const defaultCurlCommand = `curl -X POST http://127.0.0.1:8765/v1/tools/fixture-cloud/execute -H "Content-Type: application/json" -d '{"prompt":"hello loom","strength":0.75}'`;

export const defaultResponseSample = `{
  "image_url": "https://example.local/result.png",
  "seed": 12345
}`;

export type StudioMessageKind = "info" | "error";

export interface StudioMessage {
  kind: StudioMessageKind;
  text: string;
}

export type ArtWizardMode = "cloud_api" | "mcp" | "process" | "workflow";

export interface ArtWizardModeDescriptor {
  id: ArtWizardMode;
  title: string;
  subtitle: string;
}

export const artWizardModes: ArtWizardModeDescriptor[] = [
  {
    id: "cloud_api",
    title: "云端",
    subtitle: "把 REST/云接口封装成 Art。",
  },
  {
    id: "mcp",
    title: "MCP",
    subtitle: "绑定已配置的 MCP 工具。",
  },
  {
    id: "process",
    title: "脚本",
    subtitle: "把命令、脚本或 Python 入口封装成 Art。",
  },
  {
    id: "workflow",
    title: "流程",
    subtitle: "把已保存工作流变成可复用 Art 节点。",
  },
];

export const artModeById = (mode: ArtWizardMode, modes = artWizardModes) =>
  modes.find((item) => item.id === mode) ?? modes[0] ?? artWizardModes[0];

export const parseListText = (value: string) =>
  value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

export const parseEnvText = (value: string) =>
  value.split(/\r?\n/).reduce<Record<string, string>>((env, line) => {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) return env;
    const separatorIndex = trimmed.indexOf("=");
    if (separatorIndex <= 0) return env;
    const key = trimmed.slice(0, separatorIndex).trim();
    const envValue = trimmed.slice(separatorIndex + 1).trim();
    if (key) env[key] = envValue;
    return env;
  }, {});

export const basenameWithoutExtension = (path: string) => {
  const fileName = path.trim().split(/[\\/]/).filter(Boolean).pop() || "python-source";
  return fileName.replace(/\.[^.]+$/, "") || "python-source";
};

export const normalizeToolId = (value: string) =>
  value.trim().replace(/[^a-zA-Z0-9_-]/g, "-").replace(/^-+|-+$/g, "") || "python-source-tool";

export const pythonProcessAdapterSource = `import importlib.util
import json
import pathlib
import sys

request = json.loads(sys.stdin.buffer.read().decode("utf-8-sig"))
plugin_root = pathlib.Path(__file__).resolve().parent / "plugin"
source_path = plugin_root / "main.py"
if not source_path.is_file():
    source_path = pathlib.Path(__file__).resolve().parent / "source.py"
spec = importlib.util.spec_from_file_location("loom_process_art", source_path)
if spec is None or spec.loader is None:
    raise RuntimeError(f"cannot load Python Art source: {source_path}")
module = importlib.util.module_from_spec(spec)
sys.path.insert(0, str(source_path.parent))
spec.loader.exec_module(module)
entry = next((getattr(module, name) for name in ("main", "entry_point", "run") if hasattr(module, name)), None)
if entry is None:
    raise RuntimeError("Python Art must define main(args), entry_point(args), or run(args)")
arguments = {}
arguments.update(request.get("inputs") or {})
arguments.update(request.get("params") or {})
arguments["context"] = request.get("context") or {}
result = entry(arguments)
if isinstance(result, dict) and isinstance(result.get("status"), str):
    response = result
elif isinstance(result, dict):
    response = {"status": "success", "output": result}
else:
    response = {"status": "success", "output": {"content": [{"type": "text", "text": str(result)}]}}
print(json.dumps(response, ensure_ascii=False, separators=(",", ":")))
`;

export const normalizePythonPort = (port: LoomPythonPortDefinition): PythonArtPort => ({
  name: port.name,
  label: port.label || port.name,
  type: port.type === "image" || port.type === "file" || port.type === "int" || port.type === "float" ||
    port.type === "boolean"
    ? port.type
    : "string",
  execution_type: port.execution_type || port.executionType || "string",
  executionType: port.executionType || port.execution_type || "string",
  default: port.default,
});

export function LoomMark() {
  return (
    <svg className="loom-mark" viewBox="0 0 1024 1024" aria-hidden="true">
      <path d="M196 330h632c-38 96-130 148-270 158v108h106l96 194H264l96-194h106V488H318L196 330Z" fill="var(--loom-brand-primary)" />
      <path d="m690 206 30 66 72 8-54 48 16 70-64-36-64 36 16-70-54-48 72-8 30-66Z" fill="var(--loom-brand-secondary)" />
    </svg>
  );
}

export function ShellIcon({ kind }: { kind: ShellIconKind }) {
  const iconProps = {
    className: "shell-icon",
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  switch (kind) {
    case "sidebar":
      return <svg {...iconProps}><rect x="3" y="4" width="18" height="16" rx="1.5" /><path d="M8 4v16" /></svg>;
    case "back":
      return <svg {...iconProps}><path d="m15 18-6-6 6-6" /><path d="M9 12h11" /></svg>;
    case "mcp":
      return <svg {...iconProps}><rect x="4" y="3" width="16" height="6" rx="2" /><rect x="4" y="15" width="16" height="6" rx="2" /><path d="M8 6h.01M8 18h.01M12 9v6" /></svg>;
    case "registry":
      return <svg {...iconProps}><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z" /><path d="m4 7.5 8 4.5 8-4.5M12 12v9" /></svg>;
    case "hook-bridge":
      return <svg {...iconProps}><path d="M7 7h10a4 4 0 0 1 0 8h-1" /><path d="m9 4-3 3 3 3M15 20l3-3-3-3" /><path d="M17 17H7a4 4 0 0 1 0-8h1" /></svg>;
    case "device":
      return <svg {...iconProps}><rect x="3" y="4" width="18" height="13" rx="2" /><path d="M8 21h8M12 17v4M7 8h4" /></svg>;
    case "settings":
      return <svg {...iconProps}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1A1.7 1.7 0 0 0 9 4.6 1.7 1.7 0 0 0 10 3v-.2h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z" /></svg>;
    case "refresh":
      return <svg {...iconProps}><path d="M20 11a8 8 0 1 0-2.3 5.7" /><path d="M20 5v6h-6" /></svg>;
    case "minimize":
      return <svg {...iconProps}><path d="M6 17h12" /></svg>;
    case "maximize":
      return <svg {...iconProps}><rect x="5" y="5" width="14" height="14" rx="1" /></svg>;
    case "close":
      return <svg {...iconProps}><path d="m6 6 12 12M18 6 6 18" /></svg>;
    default:
      return null;
  }
}

export function EnabledChip({ enabled }: { enabled?: boolean }) {
  return <span className="mini-chip">{enabled === false ? "已禁用" : "已启用"}</span>;
}

export type ArtIconKind =
  | "cloud"
  | "terminal"
  | "code"
  | "python"
  | "plug"
  | "workflow"
  | "image"
  | "package"
  | "edit"
  | "power"
  | "trash"
  | "plus"
  | "eye"
  | "close";

export function ArtIcon({ kind }: { kind: ArtIconKind }) {
  const iconProps = {
    className: "art-icon",
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  switch (kind) {
    case "cloud":
      return <svg {...iconProps}><path d="M6.5 18a4.5 4.5 0 0 1-.4-8.98A6 6 0 0 1 17.7 10.5 3.75 3.75 0 0 1 17 18H6.5Z" /></svg>;
    case "terminal":
      return <svg {...iconProps}><rect x="3" y="5" width="18" height="14" rx="2" /><path d="m7 9 3 3-3 3M13 15h4" /></svg>;
    case "code":
      return <svg {...iconProps}><path d="m8 8-4 4 4 4M16 8l4 4-4 4M14 5l-4 14" /></svg>;
    case "python":
      return <svg {...iconProps}><path d="M8 4h5a3 3 0 0 1 3 3v3H9a3 3 0 0 0-3 3v1" /><path d="M16 20h-5a3 3 0 0 1-3-3v-3h7a3 3 0 0 0 3-3v-1" /><path d="M10 7h.01M14 17h.01" /></svg>;
    case "plug":
      return <svg {...iconProps}><path d="M8 3v5M16 3v5M6 8h12v2a6 6 0 0 1-6 6v5M9 21h6" /></svg>;
    case "workflow":
      return <svg {...iconProps}><circle cx="6" cy="6" r="2" /><circle cx="18" cy="6" r="2" /><circle cx="12" cy="18" r="2" /><path d="M8 6h8M7 8l4 8M17 8l-4 8" /></svg>;
    case "image":
      return <svg {...iconProps}><rect x="3" y="4" width="18" height="16" rx="2" /><circle cx="8.5" cy="9" r="1.5" /><path d="m5 17 4-4 3 3 2-2 5 5" /></svg>;
    case "edit":
      return <svg {...iconProps}><path d="M4 20h4l11-11a2.8 2.8 0 0 0-4-4L4 16v4Z" /><path d="m13.5 6.5 4 4" /></svg>;
    case "power":
      return <svg {...iconProps}><path d="M12 3v9" /><path d="M7.1 5.8a8 8 0 1 0 9.8 0" /></svg>;
    case "trash":
      return <svg {...iconProps}><path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" /></svg>;
    case "plus":
      return <svg {...iconProps}><path d="M12 5v14M5 12h14" /></svg>;
    case "eye":
      return <svg {...iconProps}><path d="M2.5 12s3.5-6 9.5-6 9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6Z" /><circle cx="12" cy="12" r="2.5" /></svg>;
    case "close":
      return <svg {...iconProps}><path d="m6 6 12 12M18 6 6 18" /></svg>;
    default:
      return <svg {...iconProps}><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z" /><path d="m4 7.5 8 4.5 8-4.5M12 12v9" /></svg>;
  }
}

export function ArtPublisherIcon({ publisher }: { publisher: ArtPublisherIdentity }) {
  const [imageFailed, setImageFailed] = useState(false);
  const imageSource = artPublisherIconSource(publisher.icon);
  const glyph = publisher.icon && !imageSource
    ? Array.from(publisher.icon)[0] ?? publisher.initials
    : publisher.initials;

  useEffect(() => {
    setImageFailed(false);
  }, [imageSource]);

  return (
    <span className="art-edit-dialog__publisher-icon" aria-hidden="true">
      {imageSource && !imageFailed ? (
        <img
          src={imageSource}
          alt=""
          referrerPolicy="no-referrer"
          onError={() => setImageFailed(true)}
        />
      ) : (
        <span>{glyph}</span>
      )}
    </span>
  );
}

export function artFrameworkIconKind(reference: string | null): ArtIconKind {
  switch (reference) {
    case "cloud_api": return "cloud";
    case "process": return "terminal";
    case "mcp": return "plug";
    case "workflow": return "workflow";
    default: return "package";
  }
}

export function artFrameworkIconLabel(reference: string | null): string {
  return officialFrameworkDisplayName(reference) ?? "Art";
}
