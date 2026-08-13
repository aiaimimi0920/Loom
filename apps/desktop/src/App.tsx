import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, Dispatch, KeyboardEvent, ReactNode, SetStateAction } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import desktopPackage from "../package.json";
import {
  LoomAppPaths,
  LoomArtStoreSettings,
  LoomCacheSettings,
  LoomSettings,
  HookCacheSettings,
  LoomMcpSettings,
  LoomProxySettings,
  LoomShortcutConfig,
  DEFAULT_LOOM_DAEMON_URL,
  DEFAULT_LOOM_SETTINGS,
  LoomHookBridgeStatus,
  LoomMcpServer,
  LoomPythonPortDefinition,
  LoomSnapshot,
  type LoomArtRuntimeManifest,
  LoomToolDefinition,
  LoomToolExecution,
  LoomWorkflowMetadata,
  bootstrapPackagedArts,
  autoUpdateArts,
  addManagedDevice,
  approveManagedDevice,
  checkPythonArtJsonNearby,
  createAuthoredArtPackage,
  deleteToolDefinition,
  getLoomAppPaths,
  getLoomSettings,
  getLoomShortcuts,
  inferPythonArtPorts,
  readLoomSnapshot,
  retainAvailableSnapshotData,
  readPythonArtJson,
  readPythonArtSource,
  saveLoomSettings,
  saveToolDefinition,
  startHookBridge,
  startLoomDaemon,
  listFrameworks,
  listManagedDevices,
  installFramework,
  uninstallFramework,
  upgradeFrameworkPackage,
  listPluginTrust,
  setPluginTrustPolicy,
  trustPluginUser,
  untrustPluginUser,
  listPluginCredentials,
  savePluginCredential,
  deletePluginCredential,
  revealPluginCredential,
  getPublisherIdentity,
  rotatePublisherIdentity,
  removeManagedDevice,
  revealPublisherPrivateKey,
  fetchArtStoreCatalog,
  getArtManagement,
  getWorkflowBundle,
  installArtFromStore,
  publishArt,
  saveArtManagementSettings,
  updateArtToVersion,
  updateManagedDevice,
  uninstallArtPackage,
  type LoomFramework,
  type LoomManagedDevice,
  type LoomDeviceKind,
  type LoomFrameworkAuthoringField,
  type LoomCredentialSummary,
  type LoomCredentialDetails,
  type LoomCredentialValueType,
  type LoomPluginTrustPolicy,
  type LoomPluginTrustStore,
  type LoomPublisherIdentityState,
  type LoomArtManagement,
  type LoomArtManagementParameter,
  type LoomArtManagementSettingsInput,
  type ArtStoreEntry,
  testMcpConnection,
  waitForLoomOnline,
} from "./services/loomApi";
import {
  buildAuthoredArtPackage,
  defaultAuthoringValues,
} from "./services/artAuthoring";
import {
  inferPortsFromPythonCode,
  mapArtJsonPorts,
  type PythonArtPort,
} from "./services/pythonArtSource";
import { startHookBridgeWorkflowSync } from "./services/hookBridgeWorkflowSync";
import { createLatestRequestGate, createSingleFlightGate } from "./services/latestRequest";
import {
  artDisplayIdentity,
  artFrameworkReference,
  artPackageIdentity,
  artPublisherIconSource,
  artWorkspaceItems,
  filterArtStoreEntries,
  filterToolsByFrameworks,
  frameworkFilterLabel,
  frameworkIdentity,
  isLocallyAuthoredTool,
  nextArtWorkspaceIndex,
  officialFrameworkDisplayName,
  type ArtPublisherIdentity,
  type ArtWorkspaceId,
} from "./services/artHubUi";
import {
  getHookCanvasRefreshTrigger,
  keepNewestHookCanvasSnapshot,
  readHookCanvasSnapshot,
  type HookCanvasSnapshot,
} from "./services/hookCanvas";
import {
  HookCanvasThumbnail,
  type WorkflowArtCreationRequest,
} from "./components/hook/HookCanvasThumbnail";
import { McpHub } from "./components/mcp/McpHub";
import {
  autoTemplateResponse,
  collectWorkflowParamBindingCandidates,
  collectWorkflowPreviewNodeOptions,
  inferWorkflowArtInterface,
  parseRawCommand,
  parseCurlCommand,
  parseWorkflowYamlLite,
  portsFromMcpToolSchema,
} from "./services/workflowStudio";
import { applyLoomGeneralSettings } from "./services/loomGeneralSettings";
import type {
  CurlImportResult,
  ParsedPort,
  WorkflowBindingKind,
  WorkflowExecutionBindings,
  WorkflowGraphLite,
  WorkflowInputBinding,
  WorkflowOutputBinding,
  WorkflowParamBindingCandidate,
  WorkflowPreviewNodeOption,
} from "./services/workflowStudio";

type SectionId =
  | "mcp"
  | "registry"
  | "hook-bridge"
  | "devices"
  | "settings";

interface RuntimeConfig {
  loomDaemonUrl: string;
  settingsUrl: string;
  hookBridgeUrl?: string;
}

type ShellIconKind =
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

interface NavigationItem {
  id: SectionId;
  label: string;
  eyebrow: string;
  icon: ShellIconKind;
}

const navigationItems: NavigationItem[] = [
  { id: "mcp", label: "MCP", eyebrow: "服务工具", icon: "mcp" },
  { id: "registry", label: "Art", eyebrow: "", icon: "registry" },
  { id: "hook-bridge", label: "Hook 同步", eyebrow: "", icon: "hook-bridge" },
  { id: "devices", label: "设备管理", eyebrow: "客户端连接", icon: "device" },
  { id: "settings", label: "设置", eyebrow: "配置中心", icon: "settings" },
];

const primaryNavigationItems = navigationItems.filter(
  (item) => item.id !== "settings" && item.id !== "devices",
);
const utilityNavigationItems = navigationItems.filter(
  (item) => item.id === "settings",
);

const fallbackSnapshot: LoomSnapshot = {
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

const DEFAULT_HOOK_BRIDGE_URL = "ws://127.0.0.1:19820";

// Poll the Hook canvas while online so desktop edits (node moves, new captures,
// image changes) sync automatically without a manual refresh. The daemon
// computes a cheap content revision and keepNewestHookCanvasSnapshot dedupes by
// it, so a poll that finds no change does not re-render the canvas.
const HOOK_CANVAS_POLL_INTERVAL_MS = 1500;

const firstWords = (value: string | undefined, fallback: string) => {
  if (!value) return fallback;
  return value.length > 96 ? `${value.slice(0, 96)}...` : value;
};

const defaultCurlCommand = `curl -X POST http://127.0.0.1:8765/v1/tools/fixture-cloud/execute -H "Content-Type: application/json" -d '{"prompt":"hello loom","strength":0.75}'`;

const defaultResponseSample = `{
  "image_url": "https://example.local/result.png",
  "seed": 12345
}`;

type StudioMessageKind = "info" | "error";

interface StudioMessage {
  kind: StudioMessageKind;
  text: string;
}

type ArtWizardMode = "cloud_api" | "mcp" | "process" | "workflow";

interface ArtWizardModeDescriptor {
  id: ArtWizardMode;
  title: string;
  subtitle: string;
}

const artWizardModes: ArtWizardModeDescriptor[] = [
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

const artModeById = (mode: ArtWizardMode, modes = artWizardModes) =>
  modes.find((item) => item.id === mode) ?? modes[0] ?? artWizardModes[0];

const parseListText = (value: string) =>
  value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

const parseEnvText = (value: string) =>
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

const basenameWithoutExtension = (path: string) => {
  const fileName = path.trim().split(/[\\/]/).filter(Boolean).pop() || "python-source";
  return fileName.replace(/\.[^.]+$/, "") || "python-source";
};

const normalizeToolId = (value: string) =>
  value.trim().replace(/[^a-zA-Z0-9_-]/g, "-").replace(/^-+|-+$/g, "") || "python-source-tool";

const pythonProcessAdapterSource = `import importlib.util
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

const normalizePythonPort = (port: LoomPythonPortDefinition): PythonArtPort => ({
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

const openExternal = (url: string) => {
  window.open(url, "_blank", "noopener,noreferrer");
};

function LoomMark() {
  return (
    <svg className="loom-mark" viewBox="0 0 1024 1024" aria-hidden="true">
      <path d="M196 330h632c-38 96-130 148-270 158v108h106l96 194H264l96-194h106V488H318L196 330Z" fill="var(--loom-brand-primary)" />
      <path d="m690 206 30 66 72 8-54 48 16 70-64-36-64 36 16-70-54-48 72-8 30-66Z" fill="var(--loom-brand-secondary)" />
    </svg>
  );
}

function ShellIcon({ kind }: { kind: ShellIconKind }) {
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

function EnabledChip({ enabled }: { enabled?: boolean }) {
  return <span className="mini-chip">{enabled === false ? "已禁用" : "已启用"}</span>;
}

type ArtIconKind =
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

function ArtIcon({ kind }: { kind: ArtIconKind }) {
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

function ArtPublisherIcon({ publisher }: { publisher: ArtPublisherIdentity }) {
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

function artFrameworkIconKind(reference: string | null): ArtIconKind {
  switch (reference) {
    case "cloud_api": return "cloud";
    case "process": return "terminal";
    case "mcp": return "plug";
    case "workflow": return "workflow";
    default: return "package";
  }
}

function artFrameworkIconLabel(reference: string | null): string {
  return officialFrameworkDisplayName(reference) ?? "Art";
}

interface ArtSecretDraft {
  useGlobal: boolean;
  credential: string;
  storedCredential: string;
  value: string;
}

function credentialMatchesArtParameter(
  parameter: LoomArtManagementParameter,
  credential: LoomCredentialSummary,
): boolean {
  const valueType = credential.valueType ?? "string";
  if (parameter.secret) return valueType === "string";
  switch (parameter.parameterType) {
    case "number":
      return valueType === "number" || valueType === "integer";
    case "integer":
      return valueType === "integer";
    case "boolean":
      return valueType === "boolean";
    case "json":
      return valueType === "json";
    default:
      return valueType === "string";
  }
}

function ArtEditDialog({
  tool,
  management,
  loading,
  busyAction,
  error,
  onClose,
  onSave,
  onUpdate,
}: {
  tool: LoomToolDefinition | null;
  management: LoomArtManagement | null;
  loading: boolean;
  busyAction: "save" | "update" | null;
  error: string | null;
  onClose: () => void;
  onSave: (input: LoomArtManagementSettingsInput) => Promise<void>;
  onUpdate: (version: string, input: LoomArtManagementSettingsInput) => Promise<void>;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const nameInputRef = useRef<HTMLInputElement | null>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [autoUpdate, setAutoUpdate] = useState(true);
  const [targetVersion, setTargetVersion] = useState("");
  const [parameterValues, setParameterValues] = useState<Record<string, unknown>>({});
  const [valueBindings, setValueBindings] = useState<Record<string, string>>({});
  const [useGlobalValues, setUseGlobalValues] = useState<Record<string, boolean>>({});
  const [secretDrafts, setSecretDrafts] = useState<Record<string, ArtSecretDraft>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const busy = busyAction !== null;

  useEffect(() => {
    if (!management) return;
    const identity = tool
      ? artDisplayIdentity(
          tool,
          management.artId,
          typeof document === "undefined"
            ? "zh-CN"
            : document.documentElement.lang || window.navigator.language,
        )
      : null;
    setName(management.canEditIdentity
      ? management.name || tool?.name || ""
      : identity?.localizedName || management.name || tool?.name || "");
    setDescription(management.canEditIdentity
      ? management.description || tool?.description || ""
      : identity?.localizedDescription || management.description || tool?.description || "");
    setAutoUpdate(management.autoUpdate !== false);
    setTargetVersion(management.currentVersion);
    setParameterValues(Object.fromEntries(
      management.parameters
        .filter((parameter) => !parameter.secret)
        .map((parameter) => {
          const hasSavedValue = Object.prototype.hasOwnProperty.call(management.defaults, parameter.id);
          const value = hasSavedValue
            ? management.defaults[parameter.id]
            : parameter.required
              ? parameter.default
              : "";
          return [parameter.id, parameter.parameterType === "json" && value !== undefined && value !== ""
            ? JSON.stringify(value, null, 2)
            : value ?? (parameter.required && parameter.parameterType === "boolean" ? false : "")];
        }),
    ));
    setValueBindings({ ...management.valueBindings });
    setUseGlobalValues(Object.fromEntries(
      management.parameters
        .filter((parameter) => !parameter.secret)
        .map((parameter) => [parameter.id, Boolean(management.valueBindings[parameter.id])]),
    ));
    const credentialsByName = new Map(
      management.availableCredentials.map((credential) => [credential.name, credential]),
    );
    setSecretDrafts(Object.fromEntries(
      management.parameters
        .filter((parameter) => parameter.secret)
        .map((parameter) => {
          const binding = management.credentialBindings[parameter.id] || "";
          const summary = credentialsByName.get(binding);
          const storedCredential = summary?.scope.artId === management.artId
            || binding.startsWith("loom-art-secret-")
            ? binding
            : "";
          return [parameter.id, {
            useGlobal: Boolean(binding && !storedCredential),
            credential: binding && !storedCredential ? binding : "",
            storedCredential,
            value: "",
          } satisfies ArtSecretDraft];
        }),
    ));
    setFormError(null);
  }, [management, tool]);

  useEffect(() => {
    if (!tool) return;
    if (management && !loading) nameInputRef.current?.focus();
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busy) onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )];
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [busy, loading, management, onClose, tool]);

  if (!tool) return null;

  const displayIdentity = artDisplayIdentity(
    tool,
    management?.artId,
    typeof document === "undefined"
      ? "zh-CN"
      : document.documentElement.lang || window.navigator.language,
  );
  const requiredParameters = management?.parameters.filter((parameter) => parameter.required && !parameter.secret) ?? [];
  const optionalParameters = management?.parameters.filter((parameter) => !parameter.required && !parameter.secret) ?? [];
  const secretParameters = management?.parameters.filter((parameter) => parameter.secret) ?? [];
  const hasSplitWorkspace = requiredParameters.length > 0 && secretParameters.length > 0;
  const globalCredentials = management?.availableCredentials.filter((credential) => (
    !credential.scope.frameworkId && !credential.scope.artId
  )) ?? [];
  const visibleError = formError || error;

  const valuePresent = (value: unknown) => (
    value !== undefined && value !== null && (typeof value !== "string" || value.trim() !== "")
  );

  const buildSettingsInput = (): LoomArtManagementSettingsInput => {
    if (!management) throw new Error("Art 管理信息尚未加载。");
    const defaults: Record<string, unknown> = {};
    const nextValueBindings: Record<string, string> = {};
    for (const parameter of management.parameters.filter((candidate) => !candidate.secret)) {
      if (useGlobalValues[parameter.id]) {
        const binding = valueBindings[parameter.id];
        if (!binding) throw new Error(`必须为 ${parameter.label} 选择全局值。`);
        const credential = globalCredentials.find((candidate) => candidate.name === binding);
        if (!credential || !credentialMatchesArtParameter(parameter, credential)) {
          throw new Error(`${parameter.label} 的全局值不存在或类型不匹配。`);
        }
        nextValueBindings[parameter.id] = binding;
        continue;
      }
      const draft = parameterValues[parameter.id];
      if (parameter.required && !valuePresent(draft)) {
        throw new Error(`必须填写 ${parameter.label}。`);
      }
      if (!valuePresent(draft)) continue;
      if (parameter.parameterType === "json" && typeof draft === "string") {
        try {
          defaults[parameter.id] = JSON.parse(draft);
        } catch {
          throw new Error(`${parameter.label} 不是有效的 JSON。`);
        }
      } else {
        defaults[parameter.id] = draft;
      }
    }
    const bindings: Record<string, string> = {};
    const secretValues: Record<string, string> = {};
    for (const parameter of management.parameters.filter((candidate) => candidate.secret)) {
      const draft = secretDrafts[parameter.id] ?? {
        useGlobal: false,
        credential: "",
        storedCredential: "",
        value: "",
      };
      if (draft.useGlobal) {
        if (!draft.credential) throw new Error(`必须为 ${parameter.label} 选择全局机密。`);
        const credential = globalCredentials.find((candidate) => candidate.name === draft.credential);
        if (!credential || !credentialMatchesArtParameter(parameter, credential)) {
          throw new Error(`${parameter.label} 的全局机密不存在或不是文本类型。`);
        }
        bindings[parameter.id] = draft.credential;
        continue;
      }
      if (draft.value.trim()) {
        secretValues[parameter.id] = draft.value;
        if (draft.storedCredential) bindings[parameter.id] = draft.storedCredential;
        continue;
      }
      if (draft.storedCredential) {
        bindings[parameter.id] = draft.storedCredential;
        continue;
      }
      if (parameter.required) throw new Error(`必须填写 ${parameter.label}。`);
    }
    return {
      ...(management.canEditIdentity ? { name: name.trim(), description } : {}),
      autoUpdate,
      defaults,
      valueBindings: nextValueBindings,
      credentialBindings: bindings,
      ...(Object.keys(secretValues).length ? { secretValues } : {}),
    };
  };

  const submit = async (action: "save" | "update") => {
    setFormError(null);
    try {
      const input = buildSettingsInput();
      if (action === "update") {
        await onUpdate(targetVersion, input);
      } else {
        await onSave(input);
      }
    } catch (submitError) {
      setFormError(submitError instanceof Error ? submitError.message : "无法保存 Art 设置。");
    }
  };

  const renderParameter = (parameter: LoomArtManagementParameter) => {
    const value = parameterValues[parameter.id];
    const setValue = (next: unknown) => setParameterValues((current) => ({ ...current, [parameter.id]: next }));
    const options = Array.isArray(parameter.options) ? parameter.options : [];
    const compatibleCredentials = globalCredentials.filter((credential) => (
      credentialMatchesArtParameter(parameter, credential)
    ));
    const usingGlobal = Boolean(useGlobalValues[parameter.id]);
    const selectedBinding = valueBindings[parameter.id] || "";
    const selectedBindingAvailable = compatibleCredentials.some((credential) => (
      credential.name === selectedBinding
    ));
    const renderLiteralEditor = () => {
      if (parameter.parameterType === "boolean") {
        if (!parameter.required) {
          return (
            <select aria-label={parameter.label} className="studio-input" value={value === "" ? "" : Boolean(value) ? "true" : "false"} onChange={(event) => setValue(event.target.value === "" ? "" : event.target.value === "true")} disabled={busy}>
              <option value="">使用默认值</option>
              <option value="true">启用</option>
              <option value="false">禁用</option>
            </select>
          );
        }
        return (
          <label className="art-edit-dialog__boolean">
            <input aria-label={parameter.label} type="checkbox" checked={Boolean(value)} onChange={(event) => setValue(event.target.checked)} disabled={busy} />
            <span>{Boolean(value) ? "启用" : "禁用"}</span>
          </label>
        );
      }
      if (parameter.parameterType === "enum" || options.length) {
        return (
          <select aria-label={parameter.label} className="studio-input" value={String(value ?? "")} onChange={(event) => setValue(event.target.value)} disabled={busy} required={parameter.required}>
            {!parameter.required ? <option value="">使用默认值</option> : null}
            {options.map((option, index) => {
              const optionValue = typeof option === "object" && option !== null && "value" in option
                ? String((option as { value: unknown }).value)
                : String(option);
              const optionLabel = typeof option === "object" && option !== null && "label" in option
                ? String((option as { label: unknown }).label)
                : optionValue;
              return <option key={`${parameter.id}-${index}-${optionValue}`} value={optionValue}>{optionLabel}</option>;
            })}
          </select>
        );
      }
      if (parameter.parameterType === "json") {
        return <textarea aria-label={parameter.label} className="studio-input art-edit-dialog__json" value={String(value ?? "")} onChange={(event) => setValue(event.target.value)} disabled={busy} required={parameter.required} />;
      }
      const numericInput = parameter.parameterType === "number" || parameter.parameterType === "integer";
      return (
        <input
          aria-label={parameter.label}
          className={`studio-input art-edit-dialog__value-input${numericInput ? " art-edit-dialog__value-input--number" : ""}`}
          type={numericInput ? "number" : "text"}
          inputMode={numericInput ? parameter.parameterType === "integer" ? "numeric" : "decimal" : undefined}
          value={typeof value === "string" || typeof value === "number" ? value : ""}
          min={typeof parameter.minimum === "number" ? parameter.minimum : undefined}
          max={typeof parameter.maximum === "number" ? parameter.maximum : undefined}
          step={typeof parameter.step === "number" ? parameter.step : parameter.parameterType === "integer" ? 1 : undefined}
          placeholder={!parameter.required && parameter.default !== undefined ? `默认：${String(parameter.default)}` : undefined}
          onChange={(event) => setValue(
            numericInput
              ? (event.target.value === "" ? "" : Number(event.target.value))
              : event.target.value,
          )}
          disabled={busy}
          required={parameter.required}
        />
      );
    };
    return (
      <div className="art-edit-dialog__parameter" key={parameter.id}>
        <div className="art-edit-dialog__parameter-head">
          <span>{parameter.label}{parameter.required ? " *" : ""}</span>
          <label className="art-edit-dialog__binding-toggle">
            <input
              type="checkbox"
              checked={usingGlobal}
              onChange={(event) => {
                const checked = event.target.checked;
                setUseGlobalValues((current) => ({ ...current, [parameter.id]: checked }));
                if (checked && !selectedBindingAvailable) {
                  setValueBindings((current) => ({
                    ...current,
                    [parameter.id]: compatibleCredentials[0]?.name ?? "",
                  }));
                }
              }}
              disabled={busy}
              aria-label={`${parameter.label} 引用全局值`}
            />
            <span>引用</span>
          </label>
        </div>
        {usingGlobal ? (
          <select
            className="studio-input"
            aria-label={`${parameter.label} 使用的全局值`}
            value={selectedBindingAvailable ? selectedBinding : ""}
            onChange={(event) => setValueBindings((current) => ({
              ...current,
              [parameter.id]: event.target.value,
            }))}
            disabled={busy}
          >
            <option value="">{compatibleCredentials.length ? "选择全局值" : "暂无匹配的全局值"}</option>
            {compatibleCredentials.map((credential) => (
              <option key={credential.name} value={credential.name}>{credential.name}</option>
            ))}
          </select>
        ) : renderLiteralEditor()}
      </div>
    );
  };

  const renderSecretParameter = (parameter: LoomArtManagementParameter) => {
    const draft = secretDrafts[parameter.id] ?? {
      useGlobal: false,
      credential: "",
      storedCredential: "",
      value: "",
    };
    const compatibleCredentials = globalCredentials.filter((credential) => (
      credentialMatchesArtParameter(parameter, credential)
    ));
    const globalCredentialExists = compatibleCredentials.some((credential) => (
      credential.name === draft.credential
    ));
    const setDraft = (next: Partial<ArtSecretDraft>) => setSecretDrafts((current) => ({
      ...current,
      [parameter.id]: { ...(current[parameter.id] ?? draft), ...next },
    }));
    return (
      <div className="art-edit-dialog__secret" key={parameter.id}>
        <div className="art-edit-dialog__secret-label">
          <span>{parameter.label}{parameter.required ? " *" : ""}</span>
          <label className="art-edit-dialog__binding-toggle">
            <input
              type="checkbox"
              checked={draft.useGlobal}
              onChange={(event) => setDraft({
                useGlobal: event.target.checked,
                credential: event.target.checked
                  ? globalCredentialExists ? draft.credential : compatibleCredentials[0]?.name ?? ""
                  : draft.credential,
              })}
              disabled={busy}
              aria-label={`${parameter.label} 引用全局机密`}
            />
            <span>引用</span>
          </label>
        </div>
        <div className="art-edit-dialog__secret-controls">
          {draft.useGlobal ? (
            <select
              className="studio-input"
              aria-label={`${parameter.label} 使用的全局机密`}
              value={globalCredentialExists ? draft.credential : ""}
              onChange={(event) => setDraft({ credential: event.target.value })}
              disabled={busy}
            >
              <option value="">{compatibleCredentials.length ? "选择全局机密" : "暂无匹配的全局机密"}</option>
              {compatibleCredentials.map((credential) => (
                <option key={credential.name} value={credential.name}>{credential.name}</option>
              ))}
            </select>
          ) : (
            <input
              className="studio-input"
              type="password"
              autoComplete="new-password"
              aria-label={`${parameter.label} 的默认机密值`}
              placeholder={draft.storedCredential ? "已保存；输入新值可替换" : "输入机密值"}
              value={draft.value}
              onChange={(event) => setDraft({ value: event.target.value })}
              disabled={busy}
            />
          )}
        </div>
        {!draft.useGlobal && draft.storedCredential && !draft.value ? (
          <span className="art-edit-dialog__secret-status">已安全保存到当前 Art</span>
        ) : null}
      </div>
    );
  };

  return (
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (!busy && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="framework-dialog art-edit-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="art-edit-dialog-title"
      >
        <header className="framework-dialog__header">
          <h2 id="art-edit-dialog-title">编辑</h2>
          <button
            className="art-card-action"
            type="button"
            aria-label="关闭编辑"
            title="关闭"
            onClick={onClose}
            disabled={busy}
          >
            <ArtIcon kind="close" />
          </button>
        </header>
        {visibleError ? <p className="error-text" role="alert">{visibleError}</p> : null}
        {loading || !management ? <div className="art-edit-dialog__loading">加载中</div> : null}
        <form
          className="art-edit-dialog__form"
          onSubmit={(event) => {
            event.preventDefault();
            void submit("save");
          }}
        >
          {management ? (
            <div className="art-edit-dialog__scroll">
              <div className="art-edit-dialog__overview">
                <section className="art-edit-dialog__section art-edit-dialog__identity" aria-label="Art 信息">
                  <div className="art-edit-dialog__identity-meta">
                    <span
                      className="art-edit-dialog__publisher"
                      title={displayIdentity.publisher.id}
                    >
                      <ArtPublisherIcon publisher={displayIdentity.publisher} />
                      <strong>{displayIdentity.publisher.name}</strong>
                    </span>
                    <strong
                      className="art-edit-dialog__english-name"
                      title={displayIdentity.englishName}
                    >
                      {displayIdentity.englishName}
                    </strong>
                    {displayIdentity.globalId ? (
                      <code
                        className="art-edit-dialog__id"
                        aria-label={`Art 编号 ${displayIdentity.globalId}`}
                        title={displayIdentity.globalId}
                      >
                        {displayIdentity.globalId}
                      </code>
                    ) : null}
                  </div>
                  <input
                    className="studio-input art-edit-dialog__name"
                    ref={nameInputRef}
                    aria-label="名称"
                    placeholder="Art 名称"
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    disabled={busy || !management.canEditIdentity}
                    required
                  />
                  <textarea
                    className="studio-input art-edit-dialog__description"
                    aria-label="描述"
                    placeholder="描述"
                    value={description}
                    onChange={(event) => setDescription(event.target.value)}
                    disabled={busy || !management.canEditIdentity}
                  />
                </section>

                <section className="art-edit-dialog__section art-edit-dialog__version" aria-label="版本">
                  <div className="art-edit-dialog__version-primary">
                    <strong
                      className="art-edit-dialog__current-version"
                      aria-label={`当前版本 ${management.currentVersion}`}
                    >
                      {management.currentVersion}
                    </strong>
                    <label className="art-edit-dialog__toggle">
                      <input
                        type="checkbox"
                        aria-label="自动更新"
                        checked={autoUpdate}
                        onChange={(event) => setAutoUpdate(event.target.checked)}
                        disabled={busy}
                      />
                      <span>自动更新</span>
                    </label>
                  </div>
                  <div className="art-edit-dialog__version-action">
                    <div className="art-edit-dialog__version-select">
                      {management.updateAvailable ? (
                        <span className="art-edit-dialog__version-new" aria-label="有新版本">new</span>
                      ) : null}
                      <select
                        className="studio-input"
                        aria-label="目标版本"
                        value={targetVersion}
                        onChange={(event) => setTargetVersion(event.target.value)}
                        disabled={busy || autoUpdate}
                      >
                        {management.availableVersions.map((version) => <option key={version} value={version}>{version}</option>)}
                      </select>
                    </div>
                    <button
                      className="ghost-button art-edit-dialog__version-update"
                      type="button"
                      disabled={busy || autoUpdate || !targetVersion || targetVersion === management.currentVersion}
                      onClick={() => void submit("update")}
                    >
                      {busyAction === "update" ? "更新中" : "更新"}
                    </button>
                  </div>
                </section>
              </div>

              <div className={`art-edit-dialog__workspace${hasSplitWorkspace ? "" : " art-edit-dialog__workspace--single"}`}>
                {requiredParameters.length ? (
                  <section className="art-edit-dialog__section art-edit-dialog__parameters" aria-label="参数">
                    <div className="art-edit-dialog__fields">{requiredParameters.map(renderParameter)}</div>
                  </section>
                ) : null}

                {secretParameters.length ? (
                  <section className="art-edit-dialog__section art-edit-dialog__secrets" aria-label="机密参数">
                    <div className="art-edit-dialog__secret-list">
                      {secretParameters.map(renderSecretParameter)}
                    </div>
                  </section>
                ) : null}

                {optionalParameters.length ? (
                  <details className="art-edit-dialog__section art-edit-dialog__optional">
                    <summary aria-label="可选参数">更多</summary>
                    <div className="art-edit-dialog__fields art-edit-dialog__fields--optional">
                      {optionalParameters.map(renderParameter)}
                    </div>
                  </details>
                ) : null}
              </div>
            </div>
          ) : null}
          <div className="art-edit-dialog__actions">
            <button className="ghost-button" type="button" onClick={onClose} disabled={busy}>取消</button>
            <button className="signal-button" type="submit" disabled={loading || !management || busy || (management.canEditIdentity && !name.trim())}>
              {busyAction === "save" ? "保存中" : "保存"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

interface ArtWizardSubmitDraft {
  mode: ArtWizardMode;
  frameworkValues: Record<string, unknown>;
  repositoryName: string;
  name: string;
  description: string;
  command: string;
  argsText: string;
  endpoint: string;
  method: string;
  contentType: string;
  headersText: string;
  bodyText: string;
  mcpServerId: string;
  mcpToolName: string;
  workflowId: string;
  workflowPreviewOutput?: WorkflowOutputBinding;
  workflowPreviewRequiredNodes: string[];
  scriptEntryKind: "python" | "command";
  scriptSourcePath: string;
  scriptSourceCode: string;
  scriptSourceDirectory: string;
  inputPorts: ArtWizardPortDraft[];
  paramPorts: ArtWizardPortDraft[];
  outputPorts: ArtWizardPortDraft[];
  templateTool?: LoomToolDefinition;
}

type ArtPortCaptureMode = "explicit_path" | "fixed_filename" | "derived_template" | "stdout";

interface ArtWizardPortDraft {
  id: string;
  name: string;
  label: string;
  type: string;
  executionType: string;
  widget: string;
  dataType: string;
  defaultValue: string;
  min?: number;
  max?: number;
  step?: number;
  options?: unknown[];
  multiline: boolean;
  group: string;
  required: boolean;
  secret: boolean;
  disabled: boolean;
  jsonPath: string;
  captureMode: ArtPortCaptureMode;
  filename: string;
  originalValue: string;
  bindingNodeId: string;
  bindingTarget: string;
  bindingKind: WorkflowBindingKind | "";
}

interface ArtCreationRequest {
  requestId: string;
  mode: ArtWizardMode;
  repositoryName: string;
  name: string;
  description: string;
  workflowId: string;
  templateTool?: LoomToolDefinition;
}

const outputCaptureModes: ArtPortCaptureMode[] = [
  "explicit_path",
  "fixed_filename",
  "derived_template",
  "stdout",
];

const defaultExecutionTypeForPort = (type: string, direction: "input" | "output") => {
  if (type === "image") return direction === "input" ? "image_path" : "image_buffer";
  if (type === "file") return "image_path";
  if (type === "boolean") return "bool";
  if (type === "int" || type === "float") return "number";
  return "string";
};

const createPortDraft = (
  direction: "input" | "output",
  overrides: Partial<ArtWizardPortDraft> = {},
): ArtWizardPortDraft => {
  const type = overrides.type || (direction === "input" ? "image" : "image");
  return {
    id: overrides.id || "",
    name: overrides.name || (direction === "input" ? "input" : "result"),
    label: overrides.label || (direction === "input" ? "输入" : "结果"),
    type,
    executionType: overrides.executionType || defaultExecutionTypeForPort(type, direction),
    widget: overrides.widget || "",
    dataType: overrides.dataType || "",
    defaultValue: overrides.defaultValue || "",
    min: overrides.min,
    max: overrides.max,
    step: overrides.step,
    options: overrides.options,
    multiline: overrides.multiline ?? false,
    group: overrides.group || "",
    required: overrides.required ?? false,
    secret: overrides.secret ?? false,
    disabled: overrides.disabled ?? false,
    jsonPath: overrides.jsonPath || "",
    captureMode: overrides.captureMode || "explicit_path",
    filename: overrides.filename || "",
    originalValue: overrides.originalValue || "",
    bindingNodeId: overrides.bindingNodeId || "",
    bindingTarget: overrides.bindingTarget || "",
    bindingKind: overrides.bindingKind || "",
  };
};

const portDraftFromParsedPort = (port: ParsedPort, direction: "input" | "output") =>
  createPortDraft(direction, {
    name: port.name,
    label: port.label || port.name,
    type: port.type,
    executionType: port.executionType || defaultExecutionTypeForPort(port.type, direction),
    defaultValue: port.default || "",
    jsonPath: port.jsonPath || "",
    originalValue: port.originalValue || "",
  });

const recordValue = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;

const stringValue = (value: unknown) => typeof value === "string" ? value : "";

const numberValue = (value: unknown) => typeof value === "number" && Number.isFinite(value) ? value : undefined;

const portDraftFromToolDefinition = (
  value: unknown,
  direction: "input" | "output",
): ArtWizardPortDraft | null => {
  const port = recordValue(value);
  if (!port) return null;
  const name = stringValue(port.name) || stringValue(port.id);
  if (!name) return null;
  const defaultValue = port.default === undefined || port.default === null
    ? ""
    : typeof port.default === "string"
      ? port.default
      : String(port.default);
  return createPortDraft(direction, {
    id: stringValue(port.id),
    name,
    label: stringValue(port.label) || name,
    type: stringValue(port.type) || "string",
    executionType: stringValue(port.executionType)
      || stringValue(port.execution_type)
      || defaultExecutionTypeForPort(stringValue(port.type), direction),
    widget: stringValue(port.widget),
    dataType: stringValue(port.data_type) || stringValue(port.dataType),
    defaultValue,
    min: numberValue(port.min) ?? numberValue(port.minimum),
    max: numberValue(port.max) ?? numberValue(port.maximum),
    step: numberValue(port.step),
    options: Array.isArray(port.options) ? port.options : undefined,
    multiline: port.multiline === true,
    group: stringValue(port.group),
    required: port.required === true,
    secret: port.secret === true,
    disabled: port.disabled === true,
    jsonPath: stringValue(port.jsonPath),
    captureMode: outputCaptureModes.includes(port.captureMode as ArtPortCaptureMode)
      ? port.captureMode as ArtPortCaptureMode
      : "explicit_path",
    filename: stringValue(port.filename),
    originalValue: stringValue(port.originalValue),
  });
};

const toolPortDrafts = (
  values: unknown[] | undefined,
  direction: "input" | "output",
) => (values ?? [])
  .map((value) => portDraftFromToolDefinition(value, direction))
  .filter((port): port is ArtWizardPortDraft => port !== null);

const workflowInputBindingKinds = new Set(["input_image", "input_value", "param"]);

const normalizeWorkflowBindings = (value: unknown): WorkflowExecutionBindings => {
  const bindings = recordValue(value);
  const inputs: WorkflowInputBinding[] = Array.isArray(bindings?.inputs)
    ? bindings.inputs.flatMap((entry) => {
        const input = recordValue(entry);
        const workflowParam = stringValue(input?.workflowParam);
        const nodeId = stringValue(input?.nodeId);
        const target = stringValue(input?.target);
        const kind = stringValue(input?.kind);
        if (!workflowParam || !nodeId || !target || !workflowInputBindingKinds.has(kind)) return [];
        return [{
          workflowParam,
          nodeId,
          target,
          kind: kind as WorkflowInputBinding["kind"],
        }];
      })
    : [];
  const rawPrimaryOutput = recordValue(bindings?.primaryOutput);
  const primaryNodeId = stringValue(rawPrimaryOutput?.nodeId);
  const primaryTarget = stringValue(rawPrimaryOutput?.output);
  const rawPreviewOutput = recordValue(bindings?.previewOutput);
  const previewNodeId = stringValue(rawPreviewOutput?.nodeId);
  const previewTarget = stringValue(rawPreviewOutput?.output);
  const previewRequiredNodes = Array.isArray(bindings?.previewRequiredNodes)
    ? [...new Set(bindings.previewRequiredNodes.map(stringValue).filter(Boolean))]
    : [];
  return {
    inputs,
    ...(primaryNodeId && primaryTarget
      ? {
          primaryOutput: {
            nodeId: primaryNodeId,
            output: primaryTarget,
            kind: "node_result" as const,
          },
        }
      : {}),
    ...(previewNodeId && previewTarget
      ? {
          previewOutput: {
            nodeId: previewNodeId,
            output: previewTarget,
            kind: "node_result" as const,
          },
        }
      : {}),
    ...(previewRequiredNodes.length ? { previewRequiredNodes } : {}),
  };
};

const workflowBindingsFromTool = (tool: LoomToolDefinition | undefined) =>
  normalizeWorkflowBindings(recordValue(tool?.execution)?.workflowBindings);

const applyWorkflowInputBindingsToDrafts = (
  ports: ArtWizardPortDraft[],
  tool: LoomToolDefinition | undefined,
  kinds: ReadonlySet<string>,
  paramPorts = false,
) => {
  const bindings = workflowBindingsFromTool(tool).inputs;
  return ports.map((port) => {
    const workflowParam = paramPorts
      ? port.id.trim() || port.name.trim()
      : port.name.trim() || port.id.trim();
    const binding = bindings.find((candidate) => (
      candidate.workflowParam === workflowParam && kinds.has(candidate.kind)
    ));
    return binding
      ? {
          ...port,
          bindingNodeId: binding.nodeId,
          bindingTarget: binding.target,
          bindingKind: binding.kind,
        }
      : port;
  });
};

const applyWorkflowOutputBindingToDrafts = (
  ports: ArtWizardPortDraft[],
  tool: LoomToolDefinition | undefined,
) => {
  const primaryOutput = workflowBindingsFromTool(tool).primaryOutput;
  if (!primaryOutput || !ports.length) return ports;
  return ports.map((port, index) => index === 0
    ? {
        ...port,
        bindingNodeId: primaryOutput.nodeId,
        bindingTarget: primaryOutput.output,
        bindingKind: "node_result" as const,
      }
    : port);
};

const defaultWizardPorts = (mode: ArtWizardMode) => {
  switch (mode) {
    case "process":
      return {
        inputs: [createPortDraft("input", { name: "input", label: "输入", type: "file", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "file", executionType: "image_path" })],
      };
    case "cloud_api":
      return {
        inputs: [createPortDraft("input", { name: "image", label: "图像", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "image", executionType: "image_path" })],
      };
    case "mcp":
      return {
        inputs: [createPortDraft("input", { name: "arguments", label: "参数", type: "string", executionType: "string" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "string", executionType: "string" })],
      };
    case "workflow":
      return {
        inputs: [createPortDraft("input", { name: "input", label: "工作流输入", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "工作流结果", type: "image", executionType: "image_path" })],
      };
    default:
      return {
        inputs: [createPortDraft("input")],
        outputs: [createPortDraft("output")],
      };
  }
};

const toolPortFromDraft = (port: ArtWizardPortDraft, direction: "input" | "output") => {
  const next: Record<string, unknown> = {
    name: port.name.trim() || (direction === "input" ? "input" : "result"),
    label: port.label.trim() || port.name.trim() || (direction === "input" ? "输入" : "结果"),
    type: port.type.trim() || "string",
    executionType: port.executionType.trim() || defaultExecutionTypeForPort(port.type, direction),
  };
  if (port.id.trim()) next.id = port.id.trim();
  if (port.widget.trim()) next.widget = port.widget.trim();
  if (port.dataType.trim()) next.data_type = port.dataType.trim();
  if (direction === "input") {
    if (port.defaultValue.trim()) next.default = port.defaultValue;
    if (typeof port.min === "number") next.min = port.min;
    if (typeof port.max === "number") next.max = port.max;
    if (typeof port.step === "number") next.step = port.step;
    if (port.options?.length) next.options = port.options;
    if (port.multiline) next.multiline = true;
    if (port.group.trim()) next.group = port.group.trim();
    if (port.required) next.required = true;
    if (port.secret) next.secret = true;
    if (port.disabled) next.disabled = true;
  } else {
    next.captureMode = port.captureMode;
    if (port.jsonPath.trim()) next.jsonPath = port.jsonPath.trim();
    if (port.filename.trim()) next.filename = port.filename.trim();
    if (port.originalValue.trim()) next.originalValue = port.originalValue.trim();
  }
  return next;
};

const defaultWidgetForParam = (type: string) => {
  if (type === "image") return "image_link";
  if (type === "int" || type === "float" || type === "number") return "number";
  if (type === "boolean") return "checkbox";
  return "text";
};

const toolParamFromDraft = (port: ArtWizardPortDraft) => ({
  ...toolPortFromDraft(port, "input"),
  id: port.id.trim() || port.name.trim(),
  widget: port.widget.trim() || defaultWidgetForParam(port.type),
});

const workflowBindingsFromDraft = (
  draft: ArtWizardSubmitDraft,
): WorkflowExecutionBindings | undefined => {
  if (draft.mode !== "workflow") return undefined;
  const existing = workflowBindingsFromTool(draft.templateTool);
  const managedWorkflowParams = new Set<string>();
  const additions: WorkflowInputBinding[] = [];

  for (const port of draft.inputPorts) {
    const workflowParam = port.name.trim() || port.id.trim();
    if (!workflowParam) continue;
    managedWorkflowParams.add(workflowParam);
    if (
      port.bindingNodeId
      && port.bindingTarget
      && (port.bindingKind === "input_image" || port.bindingKind === "input_value")
    ) {
      additions.push({
        workflowParam,
        nodeId: port.bindingNodeId,
        target: port.bindingTarget,
        kind: port.bindingKind,
      });
    }
  }
  for (const port of draft.paramPorts) {
    const workflowParam = port.id.trim() || port.name.trim();
    if (!workflowParam) continue;
    managedWorkflowParams.add(workflowParam);
    if (port.bindingNodeId && port.bindingTarget && port.bindingKind === "param") {
      additions.push({
        workflowParam,
        nodeId: port.bindingNodeId,
        target: port.bindingTarget,
        kind: "param",
      });
    }
  }

  const retained = existing.inputs.filter((binding) => (
    !managedWorkflowParams.has(binding.workflowParam)
    && !additions.some((addition) => (
      addition.nodeId === binding.nodeId
      && addition.target === binding.target
      && addition.kind === binding.kind
    ))
  ));
  const inputs = [...retained, ...additions];
  const outputPort = draft.outputPorts.find((port) => (
    port.bindingKind === "node_result" && port.bindingNodeId && port.bindingTarget
  ));
  const primaryOutput = outputPort
    ? {
        nodeId: outputPort.bindingNodeId,
        output: outputPort.bindingTarget,
        kind: "node_result" as const,
      }
    : existing.primaryOutput;
  const previewOutput = draft.workflowPreviewOutput ?? existing.previewOutput;
  const previewRequiredNodes = [...new Set([
    ...draft.workflowPreviewRequiredNodes,
    ...(previewOutput ? [previewOutput.nodeId] : []),
  ])];

  if (!inputs.length && !primaryOutput && !previewOutput && !previewRequiredNodes.length) return undefined;
  return {
    inputs,
    ...(primaryOutput ? { primaryOutput } : {}),
    ...(previewOutput ? { previewOutput } : {}),
    ...(previewRequiredNodes.length ? { previewRequiredNodes } : {}),
  };
};

function FrameworkAuthoringFieldInput({
  field,
  value,
  credentialNames,
  onChange,
}: {
  field: LoomFrameworkAuthoringField;
  value: unknown;
  credentialNames: string[];
  onChange: (value: unknown) => void;
}) {
  if (field.secret || field.type === "secret") {
    return (
      <label className="field-label">
        {field.label}{field.required ? " *" : ""}
        <select className="studio-input" value={String(value ?? "")} onChange={(event) => onChange(event.target.value)} required={field.required}>
          <option value="">选择全局机密</option>
          {credentialNames.map((credential) => <option key={credential} value={credential}>{credential}</option>)}
        </select>
      </label>
    );
  }
  if (field.type === "boolean") {
    return (
      <label className="checkbox-line">
        <input type="checkbox" checked={Boolean(value)} onChange={(event) => onChange(event.target.checked)} />
        <span>{field.label}{field.required ? " *" : ""}</span>
      </label>
    );
  }
  if (field.type === "enum") {
    return (
      <label className="field-label">
        {field.label}{field.required ? " *" : ""}
        <select className="studio-input" value={String(value ?? "")} onChange={(event) => onChange(event.target.value)}>
          <option value="">请选择</option>
          {(field.options ?? []).map((option) => (
            <option key={`${field.id}-${String(option.value)}`} value={String(option.value)}>{option.label}</option>
          ))}
        </select>
      </label>
    );
  }
  if (field.type === "json") {
    return (
      <label className="field-label">
        {field.label}{field.required ? " *" : ""}
        <textarea
          className="studio-textarea studio-textarea--compact"
          value={typeof value === "string" ? value : JSON.stringify(value ?? {}, null, 2)}
          placeholder={field.placeholder || "{}"}
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
    );
  }
  return (
    <label className="field-label">
      {field.label}{field.required ? " *" : ""}
      <input
        className="studio-input"
        type={field.type === "number" ? "number" : "text"}
        value={typeof value === "number" || typeof value === "string" ? value : ""}
        min={field.minimum ?? undefined}
        max={field.maximum ?? undefined}
        step={field.step ?? undefined}
        placeholder={field.placeholder ?? undefined}
        onChange={(event) => onChange(field.type === "number" ? Number(event.target.value) : event.target.value)}
      />
    </label>
  );
}

function ArtPortEditor({
  inputPorts,
  paramPorts,
  outputPorts,
  workflowMode,
  paramBindingCandidates,
  paramBindingsLoading,
  setInputPorts,
  setParamPorts,
  setOutputPorts,
}: {
  inputPorts: ArtWizardPortDraft[];
  paramPorts: ArtWizardPortDraft[];
  outputPorts: ArtWizardPortDraft[];
  workflowMode: boolean;
  paramBindingCandidates: WorkflowParamBindingCandidate[];
  paramBindingsLoading: boolean;
  setInputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setParamPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setOutputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
}) {
  const updateInputPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setInputPorts((ports) => ports.map((port, portIndex) => portIndex === index ? { ...port, ...patch } : port));
  };
  const updateOutputPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setOutputPorts((ports) => ports.map((port, portIndex) => portIndex === index ? { ...port, ...patch } : port));
  };
  const updateParamPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setParamPorts((ports) => ports.map((port, portIndex) => portIndex === index ? { ...port, ...patch } : port));
  };
  const bindingCandidatesByNode = useMemo(() => {
    const groups = new Map<string, { label: string; candidates: WorkflowParamBindingCandidate[] }>();
    for (const candidate of paramBindingCandidates) {
      const group = groups.get(candidate.nodeId) ?? {
        label: `${candidate.nodeLabel} · ${candidate.nodeId}`,
        candidates: [],
      };
      group.candidates.push(candidate);
      groups.set(candidate.nodeId, group);
    }
    return [...groups.entries()];
  }, [paramBindingCandidates]);
  const selectedBindingKey = (port: ArtWizardPortDraft) => (
    paramBindingCandidates.find((candidate) => (
      candidate.nodeId === port.bindingNodeId && candidate.target === port.bindingTarget
    ))?.key ?? ""
  );
  const updateParamBinding = (index: number, key: string) => {
    const port = paramPorts[index];
    if (!port) return;
    const candidate = paramBindingCandidates.find((item) => item.key === key);
    if (!candidate) {
      updateParamPort(index, { bindingNodeId: "", bindingTarget: "", bindingKind: "" });
      return;
    }
    const defaultName = !port.name.trim() || port.name.trim() === "param";
    const defaultLabel = !port.label.trim() || port.label.trim() === "参数";
    updateParamPort(index, {
      bindingNodeId: candidate.nodeId,
      bindingTarget: candidate.target,
      bindingKind: "param",
      type: candidate.type,
      executionType: candidate.executionType,
      widget: candidate.widget || defaultWidgetForParam(candidate.type),
      dataType: candidate.dataType || "",
      min: candidate.min,
      max: candidate.max,
      step: candidate.step,
      options: candidate.options,
      multiline: candidate.multiline ?? false,
      group: candidate.group || "",
      required: candidate.required ?? false,
      secret: candidate.secret ?? false,
      ...(defaultName ? { name: candidate.target, id: candidate.target } : {}),
      ...(defaultLabel ? { label: candidate.paramLabel } : {}),
      ...(!port.defaultValue.trim() ? { defaultValue: candidate.defaultValue } : {}),
    });
  };
  const portTypeOptions = (
    <>
      <option value="image">图像 image</option>
      <option value="file">文件 file</option>
      <option value="string">文本 string</option>
      <option value="int">整数 int</option>
      <option value="float">小数 float</option>
      <option value="boolean">布尔 boolean</option>
    </>
  );

  return (
    <div className="advanced-port-editor">
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>输入</h4>
          <button className="ghost-button" type="button" onClick={() => setInputPorts((ports) => [...ports, createPortDraft("input")])}>
            添加
          </button>
        </div>
        {inputPorts.map((port, index) => (
          <div className="port-editor-row" key={`input-${index}`}>
            <input className="studio-input" value={port.name} onChange={(event) => updateInputPort(index, { name: event.target.value })} placeholder="name" />
            <input className="studio-input" value={port.label} onChange={(event) => updateInputPort(index, { label: event.target.value })} placeholder="label" />
            <select
              className="studio-input"
              value={port.type}
              onChange={(event) => {
                const nextType = event.target.value;
                updateInputPort(index, { type: nextType, executionType: defaultExecutionTypeForPort(nextType, "input") });
              }}
            >
              {portTypeOptions}
            </select>
            <input className="studio-input" value={port.executionType} onChange={(event) => updateInputPort(index, { executionType: event.target.value })} placeholder="execution type" />
            <input className="studio-input" value={port.defaultValue} onChange={(event) => updateInputPort(index, { defaultValue: event.target.value })} placeholder="default" />
            <label className="checkbox-line checkbox-line--compact">
              <input type="checkbox" checked={port.disabled} onChange={(event) => updateInputPort(index, { disabled: event.target.checked })} />
              <span>禁用</span>
            </label>
            <button className="ghost-button" type="button" onClick={() => setInputPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}>
              删除
            </button>
          </div>
        ))}
      </div>
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>参数</h4>
          <button className="ghost-button" type="button" onClick={() => setParamPorts((ports) => [...ports, createPortDraft("input", { name: "param", label: "参数", type: "string" })])}>
            添加
          </button>
        </div>
        {paramPorts.map((port, index) => (
          <div className={workflowMode ? "port-editor-row port-editor-row--param-workflow" : "port-editor-row"} key={`param-${index}`}>
            <input className="studio-input" value={port.name} onChange={(event) => updateParamPort(index, { name: event.target.value })} placeholder="参数 ID" />
            <input className="studio-input" value={port.label} onChange={(event) => updateParamPort(index, { label: event.target.value })} placeholder="显示名称" />
            <select
              className="studio-input"
              value={port.type}
              onChange={(event) => {
                const nextType = event.target.value;
                updateParamPort(index, {
                  type: nextType,
                  executionType: defaultExecutionTypeForPort(nextType, "input"),
                  widget: defaultWidgetForParam(nextType),
                });
              }}
            >
              {portTypeOptions}
            </select>
            {!workflowMode ? (
              <input className="studio-input" value={port.executionType} onChange={(event) => updateParamPort(index, { executionType: event.target.value })} placeholder="execution type" />
            ) : null}
            <input className="studio-input" value={port.defaultValue} onChange={(event) => updateParamPort(index, { defaultValue: event.target.value })} placeholder="默认值" />
            {workflowMode ? (
              <select
                className="studio-input port-binding-select"
                value={selectedBindingKey(port)}
                onChange={(event) => updateParamBinding(index, event.target.value)}
                disabled={paramBindingsLoading || !paramBindingCandidates.length}
                aria-label={`绑定 ${port.label || port.name || `参数 ${index + 1}`}`}
                title="绑定到流程节点参数"
              >
                <option value="">{paramBindingsLoading ? "读取流程参数..." : "绑定到节点参数"}</option>
                {bindingCandidatesByNode.map(([nodeId, group]) => (
                  <optgroup key={nodeId} label={group.label}>
                    {group.candidates.map((candidate) => (
                      <option key={candidate.key} value={candidate.key}>
                        {candidate.paramLabel} · {candidate.target}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </select>
            ) : null}
            <button className="ghost-button" type="button" onClick={() => setParamPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}>
              删除
            </button>
          </div>
        ))}
      </div>
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>输出</h4>
          <button className="ghost-button" type="button" onClick={() => setOutputPorts((ports) => [...ports, createPortDraft("output")])}>
            添加
          </button>
        </div>
        {outputPorts.map((port, index) => (
          <div className="port-editor-row port-editor-row--output" key={`output-${index}`}>
            <input className="studio-input" value={port.name} onChange={(event) => updateOutputPort(index, { name: event.target.value })} placeholder="name" />
            <input className="studio-input" value={port.label} onChange={(event) => updateOutputPort(index, { label: event.target.value })} placeholder="label" />
            <select
              className="studio-input"
              value={port.type}
              onChange={(event) => {
                const nextType = event.target.value;
                updateOutputPort(index, { type: nextType, executionType: defaultExecutionTypeForPort(nextType, "output") });
              }}
            >
              {portTypeOptions}
            </select>
            <input className="studio-input" value={port.executionType} onChange={(event) => updateOutputPort(index, { executionType: event.target.value })} placeholder="execution type" />
            <select className="studio-input" value={port.captureMode} onChange={(event) => updateOutputPort(index, { captureMode: event.target.value as ArtPortCaptureMode })} aria-label="捕获模式">
              {outputCaptureModes.map((captureMode) => <option key={captureMode} value={captureMode}>{captureMode}</option>)}
            </select>
            <input className="studio-input" value={port.jsonPath} onChange={(event) => updateOutputPort(index, { jsonPath: event.target.value })} placeholder="JSONPath" />
            <input className="studio-input" value={port.filename} onChange={(event) => updateOutputPort(index, { filename: event.target.value })} placeholder="filename/template" />
            <button className="ghost-button" type="button" onClick={() => setOutputPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}>
              删除
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

function AddArtWizard({
  baseUrl,
  frameworks,
  mcpServers,
  workflows,
  tools,
  initialRequest,
  busy,
  onCreate,
}: {
  baseUrl: string;
  frameworks: LoomFramework[];
  mcpServers: LoomMcpServer[];
  workflows: LoomWorkflowMetadata[];
  tools: LoomToolDefinition[];
  initialRequest: ArtCreationRequest | null;
  busy: boolean;
  onCreate: (draft: ArtWizardSubmitDraft) => Promise<boolean>;
}) {
  const initialMode = initialRequest?.mode ?? "cloud_api";
  const initialTemplate = initialRequest?.templateTool;
  const initialWorkflowBindings = workflowBindingsFromTool(initialTemplate);
  const [mode, setMode] = useState<ArtWizardMode>(initialMode);
  const [frameworkValues, setFrameworkValues] = useState<Record<string, unknown>>(
    initialRequest?.workflowId ? { workflowId: initialRequest.workflowId } : {},
  );
  const [repositoryName, setRepositoryName] = useState(initialRequest?.repositoryName ?? "");
  const [name, setName] = useState(initialRequest?.name ?? "");
  const [description, setDescription] = useState(initialRequest?.description ?? "");
  const [command, setCommand] = useState("python.exe");
  const [argsText, setArgsText] = useState("");
  const [endpoint, setEndpoint] = useState("https://api.example.com/v1/process");
  const [method, setMethod] = useState("POST");
  const [contentType, setContentType] = useState("application/json");
  const [headersText, setHeadersText] = useState('{"Content-Type":"application/json"}');
  const [bodyText, setBodyText] = useState('{"image":"{{inputs.image.path}}"}');
  const [mcpServerId, setMcpServerId] = useState("");
  const [mcpToolName, setMcpToolName] = useState("");
  const [mcpArgumentsText, setMcpArgumentsText] = useState("{}");
  const [workflowId, setWorkflowId] = useState(initialRequest?.workflowId ?? "");
  const [workflowGraph, setWorkflowGraph] = useState<WorkflowGraphLite | null>(null);
  const [workflowPreviewOutput, setWorkflowPreviewOutput] = useState<WorkflowOutputBinding | undefined>(
    initialMode === "workflow" ? initialWorkflowBindings.previewOutput : undefined,
  );
  const [workflowPreviewRequiredNodes, setWorkflowPreviewRequiredNodes] = useState<string[]>(
    initialMode === "workflow" ? initialWorkflowBindings.previewRequiredNodes ?? [] : [],
  );
  const [rawCommandText, setRawCommandText] = useState("ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}");
  const [cloudCurlText, setCloudCurlText] = useState(defaultCurlCommand);
  const [cloudResponseText, setCloudResponseText] = useState(defaultResponseSample);
  const [scriptEntryKind, setScriptEntryKind] = useState<"python" | "command">("python");
  const [scriptSourcePath, setScriptSourcePath] = useState("");
  const [scriptSourceCode, setScriptSourceCode] = useState("");
  const [scriptArtJsonPath, setScriptArtJsonPath] = useState("");
  const [scriptSourceDirectory, setScriptSourceDirectory] = useState("");
  const [sourceBusyAction, setSourceBusyAction] = useState<"read" | "art-json" | "infer" | null>(null);
  const [inputPorts, setInputPorts] = useState<ArtWizardPortDraft[]>(() => {
    const ports = toolPortDrafts(initialTemplate?.inputs, "input");
    return applyWorkflowInputBindingsToDrafts(
      ports.length ? ports : defaultWizardPorts(initialMode).inputs,
      initialTemplate,
      new Set(["input_image", "input_value"]),
    );
  });
  const [paramPorts, setParamPorts] = useState<ArtWizardPortDraft[]>(() => (
    applyWorkflowInputBindingsToDrafts(
      toolPortDrafts(initialTemplate?.params, "input"),
      initialTemplate,
      new Set(["param"]),
      true,
    )
  ));
  const [outputPorts, setOutputPorts] = useState<ArtWizardPortDraft[]>(() => {
    const ports = toolPortDrafts(initialTemplate?.outputs, "output");
    return applyWorkflowOutputBindingToDrafts(
      ports.length ? ports : defaultWizardPorts(initialMode).outputs,
      initialTemplate,
    );
  });
  const [wizardMessage, setWizardMessage] = useState<StudioMessage | null>(null);
  const [mcpTools, setMcpTools] = useState<unknown[]>([]);
  const [selectedMcpSchemaToolName, setSelectedMcpSchemaToolName] = useState("");
  const [mcpDiscoveryBusy, setMcpDiscoveryBusy] = useState(false);
  const [credentialNames, setCredentialNames] = useState<string[]>([]);
  const [workflowParamCandidates, setWorkflowParamCandidates] = useState<WorkflowParamBindingCandidate[]>([]);
  const [workflowInterfaceLoading, setWorkflowInterfaceLoading] = useState(false);
  const [workflowInterfaceError, setWorkflowInterfaceError] = useState<string | null>(null);
  const workflowInterfaceAppliedRef = useRef("");
  const workflowPreviewNodeOptions = useMemo(
    () => workflowGraph ? collectWorkflowPreviewNodeOptions(workflowGraph, tools) : [],
    [tools, workflowGraph],
  );
  const selectedMode = artModeById(mode);
  const selectedFramework = useMemo(() => (
    frameworks.find((framework) => frameworkIdentity(framework) === `neuro.official/${mode}`)
    ?? frameworks.find((framework) => frameworkIdentity(framework) === mode)
  ), [frameworks, mode]);
  const selectedAuthoringSchema = selectedFramework?.authoringSchema ?? null;
  const selectedFrameworkReady = Boolean(
    selectedFramework?.installed
    && selectedFramework.enabled
    && selectedFramework.ready
    && selectedAuthoringSchema,
  );
  const handledFieldIds = useMemo(() => new Set(
    mode === "cloud_api" ? ["endpoint", "method", "headers", "body"]
      : mode === "mcp" ? ["serverId", "toolName", "arguments"]
        : mode === "workflow" ? ["workflowId"]
          : ["runtimeCommand", "runtimeArgs"],
  ), [mode]);
  const additionalAuthoringFields = (selectedAuthoringSchema?.fields ?? [])
    .filter((field) => !handledFieldIds.has(field.id));

  useEffect(() => {
    if (!mcpServerId && mcpServers[0]) setMcpServerId(mcpServers[0].id);
  }, [mcpServerId, mcpServers]);

  useEffect(() => {
    if (!workflowId && workflows[0]) setWorkflowId(workflows[0].id);
  }, [workflowId, workflows]);

  useEffect(() => {
    let active = true;
    void listPluginCredentials(baseUrl)
      .then((credentials) => {
        if (!active) return;
        setCredentialNames(credentials
          .filter((credential) => !credential.scope.frameworkId && !credential.scope.artId)
          .map((credential) => credential.name));
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [baseUrl]);

  useEffect(() => {
    const defaults = defaultWizardPorts(mode);
    const schema = selectedFramework?.authoringSchema;
    const template = initialRequest?.mode === mode ? initialRequest.templateTool : undefined;
    const templateInputs = toolPortDrafts(template?.inputs, "input");
    const templateParams = toolPortDrafts(template?.params, "input");
    const templateOutputs = toolPortDrafts(template?.outputs, "output");
    setInputPorts(applyWorkflowInputBindingsToDrafts(
      templateInputs.length ? templateInputs : schema?.inputs?.map((port) => createPortDraft("input", {
        name: port.name,
        label: port.label,
        type: port.type,
        executionType: port.executionType,
      })) ?? defaults.inputs,
      template,
      new Set(["input_image", "input_value"]),
    ));
    setParamPorts(applyWorkflowInputBindingsToDrafts(
      templateParams,
      template,
      new Set(["param"]),
      true,
    ));
    setOutputPorts(applyWorkflowOutputBindingToDrafts(
      templateOutputs.length ? templateOutputs : schema?.outputs?.map((port) => createPortDraft("output", {
        name: port.name,
        label: port.label,
        type: port.type,
        executionType: port.executionType,
      })) ?? defaults.outputs,
      template,
    ));
    setFrameworkValues({
      ...defaultAuthoringValues(schema?.fields),
      ...(initialRequest?.mode === mode && initialRequest.workflowId
        ? { workflowId: initialRequest.workflowId }
        : {}),
    });
    setWizardMessage(null);
    const templateBindings = workflowBindingsFromTool(template);
    setWorkflowGraph(null);
    setWorkflowPreviewOutput(mode === "workflow" ? templateBindings.previewOutput : undefined);
    setWorkflowPreviewRequiredNodes(
      mode === "workflow" ? templateBindings.previewRequiredNodes ?? [] : [],
    );
    workflowInterfaceAppliedRef.current = "";
  }, [initialRequest, mode, selectedFramework]);

  useEffect(() => {
    if (mode !== "workflow" || !workflowId.trim()) {
      setWorkflowParamCandidates([]);
      setWorkflowGraph(null);
      setWorkflowInterfaceLoading(false);
      setWorkflowInterfaceError(null);
      return;
    }

    let active = true;
    setWorkflowInterfaceLoading(true);
    setWorkflowInterfaceError(null);
    void getWorkflowBundle(baseUrl, workflowId.trim())
      .then((bundle) => {
        if (!active) return;
        const workflow = parseWorkflowYamlLite(bundle.data);
        const candidates = collectWorkflowParamBindingCandidates(workflow, tools);
        const inferred = inferWorkflowArtInterface(workflow, tools);
        setWorkflowParamCandidates(candidates);
        setWorkflowGraph(workflow);

        if (workflowInterfaceAppliedRef.current !== workflowId) {
          const templateMatches = initialRequest?.mode === "workflow"
            && initialRequest.workflowId === workflowId
            && Boolean(initialRequest.templateTool);
          if (!templateMatches) {
            const inferredInputs = inferred.inputs
              .filter((port) => port.bindingKind !== "param")
              .map((port) => createPortDraft("input", {
                name: port.name,
                label: port.label,
                type: port.type,
                executionType: port.executionType,
                defaultValue: port.default || "",
                bindingNodeId: port.bindingNodeId || "",
                bindingTarget: port.bindingTarget || "",
                bindingKind: port.bindingKind || "",
              }));
            const inferredOutputs = inferred.outputs.map((port) => createPortDraft("output", {
              name: port.name,
              label: port.label,
              type: port.type,
              executionType: port.executionType,
              bindingNodeId: port.bindingNodeId || "",
              bindingTarget: port.bindingTarget || "",
              bindingKind: port.bindingKind || "",
            }));
            if (inferredInputs.length) setInputPorts(inferredInputs);
            if (inferredOutputs.length) setOutputPorts(inferredOutputs);
            setParamPorts([]);
            setWorkflowPreviewOutput(inferred.bindings.primaryOutput);
            setWorkflowPreviewRequiredNodes(
              inferred.bindings.primaryOutput ? [inferred.bindings.primaryOutput.nodeId] : [],
            );
          }
          workflowInterfaceAppliedRef.current = workflowId;
        }
      })
      .catch((error) => {
        if (!active) return;
        setWorkflowParamCandidates([]);
        setWorkflowGraph(null);
        setWorkflowInterfaceError(error instanceof Error ? error.message : "无法读取流程结构。");
      })
      .finally(() => {
        if (active) setWorkflowInterfaceLoading(false);
      });
    return () => {
      active = false;
    };
  }, [baseUrl, initialRequest, mode, tools, workflowId]);

  const mcpToolLabel = (tool: unknown) => {
    if (tool && typeof tool === "object" && !Array.isArray(tool)) {
      const record = tool as Record<string, unknown>;
      if (typeof record.name === "string" && record.name.trim()) return record.name.trim();
    }
    return "mcp_tool";
  };

  const applyPythonPorts = (ports: { inputs: PythonArtPort[]; outputs: PythonArtPort[] }) => {
    setInputPorts(ports.inputs.map((port) => createPortDraft("input", {
      name: port.name,
      label: port.label,
      type: port.type,
      executionType: port.executionType || port.execution_type,
      defaultValue: typeof port.default === "string" ? port.default : JSON.stringify(port.default ?? ""),
    })));
    setOutputPorts(ports.outputs.map((port) => createPortDraft("output", {
      name: port.name,
      label: port.label,
      type: port.type,
      executionType: port.executionType || port.execution_type,
    })));
  };

  const applyArtJson = (artJson: unknown, sourcePath = scriptSourcePath) => {
    applyPythonPorts(mapArtJsonPorts(artJson));
    if (!artJson || typeof artJson !== "object" || Array.isArray(artJson)) return;
    const record = artJson as Record<string, unknown>;
    const artId = typeof record.art_id === "string" ? record.art_id : basenameWithoutExtension(sourcePath);
    const label = typeof record.label === "string" ? record.label : artId;
    const nextDescription = typeof record.description === "string" ? record.description : "";
    if (artId) setRepositoryName(normalizeToolId(artId));
    if (label) setName(label);
    if (nextDescription) setDescription(nextDescription);
  };

  const readScriptSource = async () => {
    if (!scriptSourcePath.trim()) {
      setWizardMessage({ kind: "error", text: "请输入 Python 源码路径。" });
      return;
    }
    setSourceBusyAction("read");
    try {
      const response = await readPythonArtSource(baseUrl, scriptSourcePath.trim());
      setScriptSourcePath(response.path);
      setScriptSourceCode(response.content);
      const baseName = basenameWithoutExtension(response.path);
      if (!repositoryName.trim()) setRepositoryName(normalizeToolId(baseName));
      if (!name.trim()) setName(baseName);
      let configured = false;
      try {
        const nearby = await checkPythonArtJsonNearby(baseUrl, response.path);
        if (nearby.found && nearby.artJson) {
          setScriptArtJsonPath(nearby.artJsonPath || "");
          applyArtJson(nearby.artJson, response.path);
          configured = true;
        }
      } catch {
        configured = false;
      }
      if (!configured) {
        const inferred = await inferPythonArtPorts(baseUrl, { path: response.path });
        applyPythonPorts({
          inputs: (inferred.inputs || []).map(normalizePythonPort),
          outputs: (inferred.outputs || []).map(normalizePythonPort),
        });
      }
      setWizardMessage({ kind: "info", text: configured ? "已读取源码和 art.json。" : "已读取源码并推断端口。" });
    } catch (error) {
      setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法读取 Python 源码。" });
    } finally {
      setSourceBusyAction(null);
    }
  };

  const readScriptArtJson = async () => {
    if (!scriptArtJsonPath.trim()) {
      setWizardMessage({ kind: "error", text: "请输入 art.json 路径或 Art 目录。" });
      return;
    }
    setSourceBusyAction("art-json");
    try {
      const response = await readPythonArtJson(baseUrl, scriptArtJsonPath.trim());
      setScriptArtJsonPath(response.artJsonPath || scriptArtJsonPath);
      applyArtJson(response.artJson, scriptSourcePath);
      setWizardMessage({ kind: "info", text: "已读取 art.json。" });
    } catch (error) {
      setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法读取 art.json。" });
    } finally {
      setSourceBusyAction(null);
    }
  };

  const inferScriptPorts = async () => {
    if (!scriptSourcePath.trim() && !scriptSourceCode.trim()) {
      setWizardMessage({ kind: "error", text: "请输入源码路径或源码内容。" });
      return;
    }
    setSourceBusyAction("infer");
    try {
      const response = await inferPythonArtPorts(baseUrl, {
        path: scriptSourcePath.trim() || undefined,
        code: scriptSourcePath.trim() ? undefined : scriptSourceCode,
      });
      applyPythonPorts({
        inputs: (response.inputs || []).map(normalizePythonPort),
        outputs: (response.outputs || []).map(normalizePythonPort),
      });
      setWizardMessage({ kind: "info", text: "已更新端口。" });
    } catch (error) {
      const fallback = inferPortsFromPythonCode(scriptSourceCode);
      if (fallback.inputs.length || fallback.outputs.length) {
        applyPythonPorts(fallback);
        setWizardMessage({ kind: "info", text: "已从源码更新端口。" });
      } else {
        setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法推断端口。" });
      }
    } finally {
      setSourceBusyAction(null);
    }
  };

  const importRawCommand = () => {
    const parsed = parseRawCommand(rawCommandText);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "请输入命令。" });
      return;
    }
    setCommand(parsed.command);
    setArgsText(parsed.argsText);
    const parsedInputs = parsed.ports.filter((port) => port.isInput).map((port) => portDraftFromParsedPort(port, "input"));
    const parsedOutputs = parsed.ports.filter((port) => !port.isInput).map((port) => portDraftFromParsedPort(port, "output"));
    if (parsedInputs.length) setInputPorts(parsedInputs);
    if (parsedOutputs.length) setOutputPorts(parsedOutputs);
    setWizardMessage({ kind: "info", text: "已解析命令。" });
  };

  const importCloudSmartTemplate = () => {
    const parsed = parseCurlCommand(cloudCurlText);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "请输入有效的 cURL 命令。" });
      return;
    }
    setEndpoint(parsed.url || endpoint);
    setMethod(parsed.method || "POST");
    if (Object.keys(parsed.headers).length) setHeadersText(JSON.stringify(parsed.headers, null, 2));
    const nextContentType = parsed.headers["Content-Type"] || parsed.headers["content-type"];
    if (nextContentType) setContentType(nextContentType);
    if (parsed.body) setBodyText(parsed.body);
    if (parsed.suggestedInputs.length) setInputPorts(parsed.suggestedInputs.map((port) => portDraftFromParsedPort(port, "input")));
    const responsePreview = autoTemplateResponse(cloudResponseText);
    if (responsePreview.ports.length) setOutputPorts(responsePreview.ports.map((port) => portDraftFromParsedPort(port, "output")));
    setWizardMessage({ kind: "info", text: "已导入请求和响应结构。" });
  };

  const discoverMcpTools = async () => {
    const server = mcpServers.find((item) => item.id === mcpServerId);
    if (!server) {
      setWizardMessage({ kind: "error", text: "请选择 MCP 服务。" });
      return;
    }
    setMcpDiscoveryBusy(true);
    try {
      const result = await testMcpConnection(baseUrl, server);
      const tools = Array.isArray(result.tools) ? result.tools : [];
      setMcpTools(tools);
      const firstToolName = tools[0] ? mcpToolLabel(tools[0]) : "";
      setSelectedMcpSchemaToolName(firstToolName);
      if (firstToolName && !mcpToolName.trim()) setMcpToolName(firstToolName);
      setWizardMessage({
        kind: result.success === false ? "error" : "info",
        text: result.success === false ? result.error || "MCP 连接失败。" : `发现 ${tools.length} 个工具。`,
      });
    } catch (error) {
      setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法发现 MCP 工具。" });
    } finally {
      setMcpDiscoveryBusy(false);
    }
  };

  const useSelectedMcpToolSchema = () => {
    const tool = mcpTools.find((item) => mcpToolLabel(item) === selectedMcpSchemaToolName);
    const parsed = portsFromMcpToolSchema(tool);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "该工具没有可用的 input_schema。" });
      return;
    }
    setMcpToolName(parsed.toolName);
    if (parsed.suggestedInputs.length) setInputPorts(parsed.suggestedInputs.map((port) => portDraftFromParsedPort(port, "input")));
    if (parsed.suggestedOutputs.length) setOutputPorts(parsed.suggestedOutputs.map((port) => portDraftFromParsedPort(port, "output")));
    setWizardMessage({ kind: "info", text: "已导入 MCP 工具结构。" });
  };

  const setWorkflowNodeRequired = (nodeId: string, required: boolean) => {
    setWorkflowPreviewRequiredNodes((current) => {
      const next = new Set(current);
      if (required || workflowPreviewOutput?.nodeId === nodeId) next.add(nodeId);
      else next.delete(nodeId);
      return [...next];
    });
  };

  const selectWorkflowPreviewNode = (option: WorkflowPreviewNodeOption) => {
    const output = option.outputs.find((candidate) => (
      workflowPreviewOutput?.nodeId === option.nodeId
      && workflowPreviewOutput.output === candidate.name
    )) ?? option.outputs[0];
    if (!output) return;
    setWorkflowPreviewOutput({ nodeId: option.nodeId, output: output.name, kind: "node_result" });
    setWorkflowNodeRequired(option.nodeId, true);
  };

  const submit = async () => {
    const values = { ...frameworkValues };
    if (mode === "cloud_api") Object.assign(values, { endpoint, method, headers: headersText, body: bodyText });
    if (mode === "mcp") Object.assign(values, { serverId: mcpServerId, toolName: mcpToolName, arguments: mcpArgumentsText });
    if (mode === "workflow") Object.assign(values, { workflowId });
    if (mode === "process") Object.assign(values, {
      runtimeCommand: scriptEntryKind === "python" ? "python.exe" : command,
      runtimeArgs: scriptEntryKind === "python" ? "runtime/adapter.py" : argsText,
    });
    await onCreate({
      mode,
      frameworkValues: values,
      repositoryName,
      name,
      description,
      command,
      argsText,
      endpoint,
      method,
      contentType,
      headersText,
      bodyText,
      mcpServerId,
      mcpToolName,
      workflowId,
      workflowPreviewOutput,
      workflowPreviewRequiredNodes,
      scriptEntryKind,
      scriptSourcePath,
      scriptSourceCode,
      scriptSourceDirectory,
      inputPorts,
      paramPorts,
      outputPorts,
      templateTool: initialRequest?.mode === mode ? initialRequest.templateTool : undefined,
    });
  };

  return (
    <section className="add-art-wizard" aria-label="创建 Art">
      <div className="art-mode-grid" role="tablist" aria-label="Art 类型">
        {artWizardModes.map((item) => (
          <button
            className={mode === item.id ? "art-mode-card art-mode-card--active" : "art-mode-card"}
            type="button"
            role="tab"
            aria-selected={mode === item.id}
            key={item.id}
            onClick={() => setMode(item.id)}
          >
            {item.title}
          </button>
        ))}
      </div>

      <form className="art-creator-panel" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
        <div className="art-creator-identity">
          <label className="field-label">
            仓库名称
            <input className="studio-input" value={repositoryName} onChange={(event) => setRepositoryName(event.target.value)} placeholder={`${mode}-my-art`} />
          </label>
          <label className="field-label">
            Art 名称
            <input className="studio-input" value={name} onChange={(event) => setName(event.target.value)} placeholder={selectedMode.title} />
          </label>
          <label className="field-label art-creator-identity__description">
            描述
            <input className="studio-input" value={description} onChange={(event) => setDescription(event.target.value)} placeholder={selectedMode.subtitle} />
          </label>
        </div>

        <div className="art-creator-config" role="tabpanel">
          {mode === "cloud_api" ? (
            <div className="art-creator-fields">
              <label className="field-label art-creator-field--wide">
                Endpoint
                <input className="studio-input" value={endpoint} onChange={(event) => setEndpoint(event.target.value)} placeholder="https://api.example.com/v1/process" />
              </label>
              <label className="field-label">
                方法
                <select className="studio-input" value={method} onChange={(event) => setMethod(event.target.value)}>
                  <option value="POST">POST</option>
                  <option value="PUT">PUT</option>
                  <option value="PATCH">PATCH</option>
                  <option value="GET">GET</option>
                  <option value="DELETE">DELETE</option>
                </select>
              </label>
              <label className="field-label">
                Content-Type
                <input className="studio-input" value={contentType} onChange={(event) => setContentType(event.target.value)} />
              </label>
              <label className="field-label">
                Headers
                <textarea className="studio-textarea studio-textarea--compact" value={headersText} onChange={(event) => setHeadersText(event.target.value)} spellCheck={false} />
              </label>
              <label className="field-label">
                Body
                <textarea className="studio-textarea studio-textarea--compact" value={bodyText} onChange={(event) => setBodyText(event.target.value)} spellCheck={false} />
              </label>
              <label className="field-label">
                cURL
                <textarea className="studio-textarea studio-textarea--compact" value={cloudCurlText} onChange={(event) => setCloudCurlText(event.target.value)} spellCheck={false} />
              </label>
              <label className="field-label">
                响应示例
                <textarea className="studio-textarea studio-textarea--compact" value={cloudResponseText} onChange={(event) => setCloudResponseText(event.target.value)} spellCheck={false} />
              </label>
              <div className="art-creator-inline-action">
                <button className="ghost-button" type="button" onClick={importCloudSmartTemplate}>导入 cURL</button>
              </div>
            </div>
          ) : null}

          {mode === "mcp" ? (
            <div className="art-creator-fields">
              <label className="field-label">
                MCP 服务
                <select className="studio-input" value={mcpServerId} onChange={(event) => setMcpServerId(event.target.value)}>
                  <option value="">选择服务</option>
                  {mcpServers.map((server) => <option key={server.id} value={server.id}>{server.name || server.id}</option>)}
                </select>
              </label>
              <label className="field-label">
                MCP 工具
                <input className="studio-input" value={mcpToolName} onChange={(event) => setMcpToolName(event.target.value)} placeholder="search" />
              </label>
              <label className="field-label art-creator-field--wide">
                默认参数
                <textarea className="studio-textarea studio-textarea--compact" value={mcpArgumentsText} onChange={(event) => setMcpArgumentsText(event.target.value)} spellCheck={false} />
              </label>
              <div className="art-creator-inline-action">
                <button className="ghost-button" type="button" onClick={discoverMcpTools} disabled={mcpDiscoveryBusy}>
                  {mcpDiscoveryBusy ? "发现中" : "发现工具"}
                </button>
                <select className="studio-input" value={selectedMcpSchemaToolName} onChange={(event) => setSelectedMcpSchemaToolName(event.target.value)} disabled={!mcpTools.length} aria-label="已发现的 MCP 工具">
                  <option value="">选择已发现工具</option>
                  {mcpTools.map((tool, index) => {
                    const label = mcpToolLabel(tool);
                    return <option key={`${label}-${index}`} value={label}>{label}</option>;
                  })}
                </select>
                <button className="ghost-button" type="button" onClick={useSelectedMcpToolSchema} disabled={!selectedMcpSchemaToolName}>使用结构</button>
              </div>
            </div>
          ) : null}

          {mode === "process" ? (
            <div className="art-script-creator">
              <div className="art-script-kind" role="group" aria-label="脚本入口">
                <button className={scriptEntryKind === "python" ? "art-script-kind__button art-script-kind__button--active" : "art-script-kind__button"} type="button" onClick={() => setScriptEntryKind("python")}>Python</button>
                <button className={scriptEntryKind === "command" ? "art-script-kind__button art-script-kind__button--active" : "art-script-kind__button"} type="button" onClick={() => setScriptEntryKind("command")}>命令</button>
              </div>
              {scriptEntryKind === "python" ? (
                <div className="art-creator-fields">
                  <label className="field-label art-creator-field--wide">
                    源码文件
                    <div className="art-creator-path-row">
                      <input className="studio-input" value={scriptSourcePath} onChange={(event) => setScriptSourcePath(event.target.value)} placeholder="C:\\path\\to\\main.py" />
                      <button className="ghost-button" type="button" onClick={readScriptSource} disabled={Boolean(sourceBusyAction)}>{sourceBusyAction === "read" ? "读取中" : "读取"}</button>
                    </div>
                  </label>
                  <label className="field-label art-creator-field--wide">
                    art.json（可选）
                    <div className="art-creator-path-row">
                      <input className="studio-input" value={scriptArtJsonPath} onChange={(event) => setScriptArtJsonPath(event.target.value)} placeholder="C:\\path\\to\\art.json" />
                      <button className="ghost-button" type="button" onClick={readScriptArtJson} disabled={Boolean(sourceBusyAction)}>{sourceBusyAction === "art-json" ? "读取中" : "读取"}</button>
                    </div>
                  </label>
                  <label className="field-label art-creator-field--full">
                    源码
                    <textarea className="studio-textarea art-script-source" value={scriptSourceCode} onChange={(event) => setScriptSourceCode(event.target.value)} placeholder="def run(args): ..." spellCheck={false} />
                  </label>
                  <div className="art-creator-inline-action">
                    <button className="ghost-button" type="button" onClick={inferScriptPorts} disabled={Boolean(sourceBusyAction)}>{sourceBusyAction === "infer" ? "推断中" : "推断端口"}</button>
                  </div>
                </div>
              ) : (
                <div className="art-creator-fields">
                  <label className="field-label art-creator-field--wide">
                    资源目录（可选）
                    <input className="studio-input" value={scriptSourceDirectory} onChange={(event) => setScriptSourceDirectory(event.target.value)} placeholder="C:\\path\\to\\package" />
                  </label>
                  <label className="field-label">
                    运行命令
                    <input className="studio-input" value={command} onChange={(event) => setCommand(event.target.value)} placeholder="runtime/source/tool.exe" />
                  </label>
                  <label className="field-label">
                    参数（每行一个）
                    <textarea className="studio-textarea studio-textarea--compact" value={argsText} onChange={(event) => setArgsText(event.target.value)} />
                  </label>
                  <label className="field-label art-creator-field--wide">
                    命令行
                    <div className="art-creator-path-row">
                      <input className="studio-input" value={rawCommandText} onChange={(event) => setRawCommandText(event.target.value)} placeholder="ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}" />
                      <button className="ghost-button" type="button" onClick={importRawCommand}>解析</button>
                    </div>
                  </label>
                </div>
              )}
            </div>
          ) : null}

          {mode === "workflow" ? (
            <div className="art-creator-fields">
              <label className="field-label art-creator-field--wide">
                工作流
                <select
                  className="studio-input"
                  value={workflowId}
                  onChange={(event) => {
                    workflowInterfaceAppliedRef.current = "";
                    setWorkflowGraph(null);
                    setWorkflowPreviewOutput(undefined);
                    setWorkflowPreviewRequiredNodes([]);
                    setWorkflowId(event.target.value);
                  }}
                >
                  <option value="">选择已保存工作流</option>
                  {workflows.map((workflow) => <option key={workflow.id} value={workflow.id}>{workflow.name || workflow.id}</option>)}
                </select>
              </label>
              {workflowInterfaceLoading ? <small className="art-workflow-interface-state">读取流程结构...</small> : null}
              {workflowInterfaceError ? <small className="art-workflow-interface-state art-workflow-interface-state--error">{workflowInterfaceError}</small> : null}
              {workflowPreviewNodeOptions.length ? (
                <div className="art-workflow-preview-policy" aria-label="流程预览策略">
                  <div className="art-workflow-preview-policy__head" aria-hidden="true">
                    <span>节点</span>
                    <span>必要</span>
                    <span>预览输出</span>
                  </div>
                  <div className="art-workflow-preview-policy__list">
                    {workflowPreviewNodeOptions.map((option) => {
                      const selected = workflowPreviewOutput?.nodeId === option.nodeId;
                      const required = selected || workflowPreviewRequiredNodes.includes(option.nodeId);
                      return (
                        <div className="art-workflow-preview-policy__row" key={option.nodeId}>
                          <span className="art-workflow-preview-policy__name" title={`${option.label} · ${option.nodeId}`}>
                            {option.label}
                          </span>
                          <label className="art-workflow-preview-policy__toggle" title="预览发布前必须完成">
                            <input
                              type="checkbox"
                              aria-label={`${option.label} 必要`}
                              checked={required}
                              disabled={selected}
                              onChange={(event) => setWorkflowNodeRequired(option.nodeId, event.target.checked)}
                            />
                          </label>
                          <div className="art-workflow-preview-policy__output">
                            <label className="art-workflow-preview-policy__toggle" title="作为节点贴图预览">
                              <input
                                type="radio"
                                aria-label={`${option.label} 预览输出`}
                                name="workflow-preview-output"
                                checked={selected}
                                disabled={!option.outputs.length}
                                onChange={() => selectWorkflowPreviewNode(option)}
                              />
                            </label>
                            {selected && option.outputs.length > 1 ? (
                              <select
                                className="studio-input art-workflow-preview-policy__select"
                                aria-label={`${option.label} 预览端口`}
                                value={workflowPreviewOutput.output}
                                onChange={(event) => setWorkflowPreviewOutput({
                                  nodeId: option.nodeId,
                                  output: event.target.value,
                                  kind: "node_result",
                                })}
                              >
                                {option.outputs.map((output) => (
                                  <option key={output.name} value={output.name}>{output.label}</option>
                                ))}
                              </select>
                            ) : null}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}

          {additionalAuthoringFields.length ? (
            <div className="art-creator-fields art-creator-fields--additional">
              {additionalAuthoringFields.map((field) => (
                <FrameworkAuthoringFieldInput
                  key={field.id}
                  field={field}
                  value={frameworkValues[field.id]}
                  credentialNames={credentialNames}
                  onChange={(value) => setFrameworkValues((current) => ({ ...current, [field.id]: value }))}
                />
              ))}
            </div>
          ) : null}
        </div>

        <details className="art-creator-ports">
          <summary>
            <span>端口</span>
            <small>{inputPorts.length} / {paramPorts.length} / {outputPorts.length}</small>
          </summary>
          <ArtPortEditor
            inputPorts={inputPorts}
            paramPorts={paramPorts}
            outputPorts={outputPorts}
            workflowMode={mode === "workflow"}
            paramBindingCandidates={workflowParamCandidates}
            paramBindingsLoading={workflowInterfaceLoading}
            setInputPorts={setInputPorts}
            setParamPorts={setParamPorts}
            setOutputPorts={setOutputPorts}
          />
        </details>

        {wizardMessage ? <p className={wizardMessage.kind === "error" ? "error-text" : "success-text"}>{wizardMessage.text}</p> : null}
        {!selectedFrameworkReady ? <p className="error-text">{selectedMode.title}未安装或未就绪。</p> : null}

        <div className="art-creator-panel__footer">
          <button className="signal-button" type="submit" disabled={busy || Boolean(sourceBusyAction) || !selectedFrameworkReady}>
            {busy ? "创建中" : "创建 Art"}
          </button>
        </div>
      </form>
    </section>
  );
}

function ArtCreationDialog({
  open,
  busy,
  message,
  onClose,
  children,
}: {
  open: boolean;
  busy: boolean;
  message: StudioMessage | null;
  onClose: () => void;
  children: ReactNode;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) return;
    closeButtonRef.current?.focus();
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busy) onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )];
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [busy, onClose, open]);

  if (!open) return null;

  return (
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (!busy && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="framework-dialog art-create-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="art-create-dialog-title"
      >
        <header className="framework-dialog__header">
          <h2 id="art-create-dialog-title">创建 Art</h2>
          <button
            className="ghost-button"
            type="button"
            ref={closeButtonRef}
            onClick={onClose}
            disabled={busy}
          >
            关闭
          </button>
        </header>
        <div className="art-create-dialog__scroll">
          {message ? (
            <p className={message.kind === "error" ? "error-text" : "success-text"}>{message.text}</p>
          ) : null}
          {children}
        </div>
      </div>
    </div>
  );
}

function McpPanel({
  servers,
  baseUrl,
  refresh,
}: {
  servers: LoomMcpServer[];
  baseUrl: string;
  refresh: () => Promise<void>;
}) {
  const notify = useCallback((level: "info" | "warning" | "error", text: string) => {
    pushAppToast({ level, text });
  }, []);
  return (
    <McpHub
      servers={servers}
      baseUrl={baseUrl}
      refresh={refresh}
      notify={notify}
      confirmRemove={(server) => requestAppConfirmation({
        title: "删除 MCP",
        message: `删除 ${server.name || server.id} 后，使用该服务的 Art 将无法运行。`,
        confirmLabel: "删除",
        tone: "danger",
      })}
    />
  );
}

function RegistryPanel({
  tools,
  mcpServers,
  workflows,
  frameworks,
  selectedFrameworkIds,
  createDialogOpen,
  createRequest,
  onCloseCreateDialog,
  reloadFrameworks,
  baseUrl,
  refresh,
}: {
  tools: LoomToolDefinition[];
  mcpServers: LoomMcpServer[];
  workflows: LoomWorkflowMetadata[];
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  createDialogOpen: boolean;
  createRequest: ArtCreationRequest | null;
  onCloseCreateDialog: () => void;
  reloadFrameworks: () => Promise<void>;
  baseUrl: string;
  refresh: () => Promise<void>;
}) {
  const [busyToolId, setBusyToolId] = useState<string | null>(null);
  const [busyToolAction, setBusyToolAction] = useState<"delete" | "toggle" | "edit" | null>(null);
  const [busyWizard, setBusyWizard] = useState(false);
  const [registryMessage, setRegistryMessage] = useState<StudioMessage | null>(null);
  const [editingTool, setEditingTool] = useState<LoomToolDefinition | null>(null);
  const [artManagement, setArtManagement] = useState<LoomArtManagement | null>(null);
  const [artManagementLoading, setArtManagementLoading] = useState(false);
  const [editBusyAction, setEditBusyAction] = useState<"save" | "update" | null>(null);
  const [editError, setEditError] = useState<string | null>(null);
  const editButtonRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const autoUpdateAttemptedUrl = useRef<string | null>(null);
  const visibleTools = useMemo(
    () => filterToolsByFrameworks(tools, frameworks, selectedFrameworkIds),
    [frameworks, selectedFrameworkIds, tools],
  );

  useEffect(() => {
    if (createDialogOpen) setRegistryMessage(null);
  }, [createDialogOpen]);

  useEffect(() => {
    if (autoUpdateAttemptedUrl.current === baseUrl) return;
    autoUpdateAttemptedUrl.current = baseUrl;
    let active = true;
    void autoUpdateArts(baseUrl)
      .then((result) => {
        if (active && Array.isArray(result.updated) && result.updated.length) void refresh();
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [baseUrl, refresh]);

  const removeTool = async (tool: LoomToolDefinition) => {
    setRegistryMessage(null);
    setBusyToolId(tool.id);
    setBusyToolAction("delete");
    try {
      const packageIdentity = artPackageIdentity(tool);
      if (packageIdentity) {
        await uninstallArtPackage(baseUrl, packageIdentity);
      } else {
        await deleteToolDefinition(baseUrl, tool.id);
      }
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法删除工具。",
      });
    } finally {
      setBusyToolId(null);
      setBusyToolAction(null);
    }
  };

  const toggleTool = async (tool: LoomToolDefinition) => {
    const nextEnabled = tool.enabled === false;
    setBusyToolId(tool.id);
    setBusyToolAction("toggle");
    try {
      await saveToolDefinition(baseUrl, { ...tool, enabled: nextEnabled });
      setRegistryMessage({
        kind: "info",
        text: `已${nextEnabled ? "启用" : "禁用"} ${tool.name || tool.id}。`,
      });
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法切换 Art 状态。",
      });
    } finally {
      setBusyToolId(null);
      setBusyToolAction(null);
    }
  };

  const closeToolEditor = useCallback(() => {
    const toolId = editingTool?.id;
    setEditingTool(null);
    setArtManagement(null);
    setArtManagementLoading(false);
    setEditBusyAction(null);
    setEditError(null);
    if (toolId) {
      window.setTimeout(() => editButtonRefs.current[toolId]?.focus(), 0);
    }
  }, [editingTool]);

  const openToolEditor = async (tool: LoomToolDefinition) => {
    setEditingTool(tool);
    setArtManagement(null);
    setArtManagementLoading(true);
    setEditError(null);
    try {
      await autoUpdateArts(baseUrl);
      setArtManagement(await getArtManagement(baseUrl, tool.id));
    } catch (error) {
      setEditError(error instanceof Error ? error.message : "无法读取 Art 设置。");
    } finally {
      setArtManagementLoading(false);
    }
  };

  const saveToolEdits = async (input: LoomArtManagementSettingsInput) => {
    if (!artManagement) return;
    setEditBusyAction("save");
    setEditError(null);
    try {
      await saveArtManagementSettings(baseUrl, artManagement.artId, input);
      await refresh();
      closeToolEditor();
    } catch (error) {
      const detail = error instanceof Error ? error.message : "无法更新 Art。";
      setEditError(detail);
      setRegistryMessage({
        kind: "error",
        text: detail,
      });
    } finally {
      setEditBusyAction(null);
    }
  };

  const updateToolVersion = async (version: string, input: LoomArtManagementSettingsInput) => {
    if (!artManagement) return;
    setEditBusyAction("update");
    setEditError(null);
    try {
      await saveArtManagementSettings(baseUrl, artManagement.artId, input);
      const updated = await updateArtToVersion(baseUrl, artManagement.artId, version);
      setArtManagement(updated);
      await refresh();
    } catch (error) {
      const detail = error instanceof Error ? error.message : "无法更新 Art 版本。";
      setEditError(detail);
      setRegistryMessage({ kind: "error", text: detail });
    } finally {
      setEditBusyAction(null);
    }
  };

  const createArtToolFromWizard = async (draft: ArtWizardSubmitDraft): Promise<boolean> => {
    const selectedFramework = frameworks.find(
      (framework) => frameworkIdentity(framework) === `neuro.official/${draft.mode}`,
    ) ?? frameworks.find((framework) => frameworkIdentity(framework) === draft.mode);
    const modeInfo = artModeById(draft.mode);
    const selectedWorkflow = workflows.find((workflow) => workflow.id === draft.workflowId);
    const derivedName = draft.name.trim() || selectedWorkflow?.name || modeInfo.title;
    const repositoryName = normalizeToolId(draft.repositoryName || `${draft.mode}-${derivedName}`);
    const description = draft.description.trim() || modeInfo.subtitle;
    const workflowBindings = workflowBindingsFromDraft(draft);
    if (selectedFramework?.authoringSchema) {
      setBusyWizard(true);
      try {
        const authored = buildAuthoredArtPackage(selectedFramework, {
          id: repositoryName,
          name: derivedName,
          description,
          values: draft.frameworkValues,
          inputs: draft.inputPorts.map((port) => ({
            name: port.name,
            label: port.label,
            type: port.type,
            executionType: port.executionType,
          })),
          outputs: draft.outputPorts.map((port) => ({
            name: port.name,
            label: port.label,
            type: port.type,
            executionType: port.executionType,
          })),
        });
        authored.tool.inputs = draft.inputPorts.map((port) => toolPortFromDraft(port, "input"));
        authored.tool.params = draft.paramPorts.map(toolParamFromDraft);
        authored.tool.outputs = draft.outputPorts.map((port) => toolPortFromDraft(port, "output"));
        if (draft.mode === "workflow" && workflowBindings) {
          authored.tool.execution = {
            ...authored.tool.execution,
            workflowBindings,
          };
        }
        const templateMetadata = recordValue(draft.templateTool?.metadata);
        if (templateMetadata) {
          authored.tool.metadata = {
            ...templateMetadata,
            ...(recordValue(authored.tool.metadata) ?? {}),
          };
        }
        if (draft.mode === "cloud_api") {
          authored.tool.execution = {
            ...authored.tool.execution,
            contentType: draft.contentType.trim() || "application/json",
          };
        }
        if (draft.mode === "process" && draft.scriptEntryKind === "python") {
          if (!draft.scriptSourceCode.trim() && !draft.scriptSourcePath.trim()) {
            throw new Error("请输入 Python 源码路径或源码内容。");
          }
          const source = draft.scriptSourceCode.trim()
            ? draft.scriptSourceCode
            : (await readPythonArtSource(baseUrl, draft.scriptSourcePath.trim())).content;
          const metadata = authored.tool.metadata && typeof authored.tool.metadata === "object" && !Array.isArray(authored.tool.metadata)
            ? authored.tool.metadata as Record<string, unknown>
            : {};
          const existingAuthoring = metadata.authoring && typeof metadata.authoring === "object" && !Array.isArray(metadata.authoring)
            ? metadata.authoring as Record<string, unknown>
            : {};
          authored.tool.metadata = {
            ...metadata,
            authoring: {
              ...existingAuthoring,
              profile: "python_source",
              sourceName: basenameWithoutExtension(draft.scriptSourcePath || `${repositoryName}.py`),
            },
          };
          await createAuthoredArtPackage(
            baseUrl,
            authored.tool,
            {
              protocolVersion: "loom.art.runtime.v1",
              entry: { command: "python.exe", args: ["runtime/adapter.py"] },
            },
            {
              files: [
                { path: "runtime/adapter.py", content: pythonProcessAdapterSource },
                { path: "runtime/source.py", content: source },
              ],
            },
          );
        } else {
          await createAuthoredArtPackage(
            baseUrl,
            authored.tool,
            authored.runtime,
            draft.mode === "process" && draft.scriptSourceDirectory.trim()
              ? {
                  sourceDirectory: draft.scriptSourceDirectory.trim(),
                  sourceDirectoryTarget: "runtime/source",
                }
              : undefined,
          );
        }
        setRegistryMessage({
          kind: "info",
          text: `已创建并安装 Art ${derivedName}。`,
        });
        await refresh();
        await reloadFrameworks();
        return true;
      } catch (error) {
        setRegistryMessage({
          kind: "error",
          text: error instanceof Error ? error.message : "无法创建框架 Art 包。",
        });
        return false;
      } finally {
        setBusyWizard(false);
      }
    }
    let execution: LoomToolExecution;
    let runtime: LoomArtRuntimeManifest | undefined;
    const fallbackPorts = defaultWizardPorts(draft.mode);
    const inputs = (draft.inputPorts.length ? draft.inputPorts : fallbackPorts.inputs)
      .map((port) => toolPortFromDraft(port, "input"));
    const outputs = (draft.outputPorts.length ? draft.outputPorts : fallbackPorts.outputs)
      .map((port) => toolPortFromDraft(port, "output"));
    const params: LoomToolDefinition["params"] = draft.paramPorts.map(toolParamFromDraft);

    switch (draft.mode) {
      case "process": {
        execution = {
          type: "framework_art",
          framework: "process",
        };
        runtime = {
          protocolVersion: "loom.art.runtime.v1",
          entry: {
            command: draft.command.trim() || "echo",
            args: parseListText(draft.argsText),
          },
        };
        break;
      }
      case "cloud_api": {
        execution = {
          type: "cloud_api",
          endpoint: draft.endpoint.trim() || "http://127.0.0.1:8765/v1/shared-images/convert",
          method: draft.method.trim().toUpperCase() || "POST",
          contentType: draft.contentType.trim() || "application/json",
          headers: draft.headersText,
          body: draft.bodyText,
        };
        break;
      }
      case "mcp": {
        if (!draft.mcpServerId.trim() || !draft.mcpToolName.trim()) {
          setRegistryMessage({ kind: "error", text: "MCP 关联 Art 需要 MCP 服务和工具名。" });
          return false;
        }
        execution = {
          type: "mcp",
          serverId: draft.mcpServerId.trim(),
          toolName: draft.mcpToolName.trim(),
        };
        break;
      }
      case "workflow": {
        if (!draft.workflowId.trim()) {
          setRegistryMessage({ kind: "error", text: "工作流 Art 需要已保存工作流。" });
          return false;
        }
        execution = {
          type: "workflow",
          workflowId: draft.workflowId.trim(),
          ...(workflowBindings ? { workflowBindings } : {}),
        };
        break;
      }
      default: {
        setRegistryMessage({ kind: "error", text: `框架 ${draft.mode} 没有可用的 authoring schema。` });
        return false;
      }
    }

    const tool: LoomToolDefinition = {
      id: repositoryName,
      name: derivedName,
      description,
      enabled: true,
      execution,
      inputs,
      outputs,
      params,
      metadata: {
        ...(recordValue(draft.templateTool?.metadata) ?? {}),
        packageSecurity: { version: "0.1.0" },
        dependencies: {
          framework: selectedFramework?.qualifiedId || selectedFramework?.id || draft.mode,
        },
        authoring: { origin: "local", owner: "local-user" },
      },
    };

    setBusyWizard(true);
    try {
      await createAuthoredArtPackage(baseUrl, tool, runtime);
      setRegistryMessage({ kind: "info", text: `已创建并安装 Art ${derivedName}。` });
      await refresh();
      return true;
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法通过添加 Art 向导创建 Art。",
      });
      return false;
    } finally {
      setBusyWizard(false);
    }
  };

  return (
    <section className="content-grid">
      {registryMessage ? (
        <p className={registryMessage.kind === "error" ? "error-text" : "success-text"}>{registryMessage.text}</p>
      ) : null}
      <div className="card-grid art-registry-grid">
        {visibleTools.map((tool) => {
          const enabled = tool.enabled !== false;
          const frameworkReference = artFrameworkReference(tool) || tool.execution?.type || null;
          const framework = frameworks.find((candidate) => (
            frameworkIdentity(candidate) === frameworkReference || candidate.id === frameworkReference
          ));
          const frameworkLabel = framework
            ? frameworkFilterLabel(framework)
            : artFrameworkIconLabel(frameworkReference);
          const toolBusy = busyToolId === tool.id;
          return (
            <article
              className={`glass-card art-registry-card ${enabled ? "art-registry-card--enabled" : "art-registry-card--disabled"}`}
              key={tool.id}
            >
              <div className="art-registry-card__head">
                <h3 title={tool.name || tool.id}>{tool.name || tool.id}</h3>
                <span
                  className="art-registry-card__framework-icon"
                  role="img"
                  aria-label={`${frameworkLabel} Art`}
                  title={frameworkLabel}
                >
                  <ArtIcon kind={artFrameworkIconKind(frameworkReference)} />
                </span>
              </div>
              {tool.description ? (
                <p className="art-registry-card__description" title={tool.description}>{tool.description}</p>
              ) : null}
              <div className="art-registry-card__actions">
                <button
                  className="art-card-action"
                  type="button"
                  ref={(element) => { editButtonRefs.current[tool.id] = element; }}
                  aria-label={`编辑 ${tool.name || tool.id}`}
                  title="编辑"
                  onClick={() => {
                    void openToolEditor(tool);
                  }}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="edit" />
                </button>
                <button
                  className={`art-card-action art-card-action--toggle ${enabled ? "art-card-action--active" : ""}`}
                  type="button"
                  aria-label={`${enabled ? "禁用" : "启用"} ${tool.name || tool.id}`}
                  aria-pressed={enabled}
                  aria-busy={toolBusy && busyToolAction === "toggle"}
                  title={enabled ? "禁用" : "启用"}
                  onClick={() => void toggleTool(tool)}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="power" />
                </button>
                <button
                  className="art-card-action art-card-action--danger"
                  type="button"
                  aria-label={`删除 ${tool.name || tool.id}`}
                  aria-busy={toolBusy && busyToolAction === "delete"}
                  title="删除"
                  onClick={() => void removeTool(tool)}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="trash" />
                </button>
              </div>
            </article>
          );
        })}
      </div>
      <ArtEditDialog
        tool={editingTool}
        management={artManagement}
        loading={artManagementLoading}
        busyAction={editBusyAction}
        error={editError}
        onClose={closeToolEditor}
        onSave={saveToolEdits}
        onUpdate={updateToolVersion}
      />
      <ArtCreationDialog
        open={createDialogOpen}
        busy={busyWizard}
        message={registryMessage}
        onClose={onCloseCreateDialog}
      >
        <AddArtWizard
          key={createRequest?.requestId ?? "manual"}
          baseUrl={baseUrl}
          frameworks={frameworks}
          mcpServers={mcpServers}
          workflows={workflows}
          tools={tools}
          initialRequest={createRequest}
          busy={busyWizard}
          onCreate={async (draft) => {
            const created = await createArtToolFromWizard(draft);
            if (created) onCloseCreateDialog();
            return created;
          }}
        />
      </ArtCreationDialog>
    </section>
  );
}

const credentialValueTypeLabels: Record<LoomCredentialValueType, string> = {
  string: "文本",
  number: "数字",
  integer: "整数",
  boolean: "布尔",
  json: "JSON",
};

const defaultPluginTrustStore: LoomPluginTrustStore = {
  publishers: [],
  policy: "allow_unsigned",
  trustedPublishers: [],
};

interface CredentialFieldDraft {
  name: string;
  value: string;
  valueType: LoomCredentialValueType;
  original?: LoomCredentialDetails;
}

function credentialFieldId(credential: Pick<LoomCredentialSummary, "name" | "scope">): string {
  return `${credential.name}:${credential.scope.frameworkId || "*"}:${credential.scope.artId || "*"}`;
}

type AppToastLevel = "error" | "warning" | "info";

interface AppToastEntry {
  id: number;
  level: AppToastLevel;
  text: string;
  leaving?: boolean;
}

let nextAppToastId = 1;
const appToastSubscribers = new Set<(entry: AppToastEntry) => void>();

function pushAppToast(message: { level: AppToastLevel; text: string }) {
  const entry = { ...message, id: nextAppToastId++ };
  appToastSubscribers.forEach((subscriber) => subscriber(entry));
}

function AppToastViewport() {
  const [entries, setEntries] = useState<AppToastEntry[]>([]);
  const timers = useRef(new Map<number, number>());

  const dismiss = useCallback((id: number) => {
    const existingTimer = timers.current.get(id);
    if (existingTimer) window.clearTimeout(existingTimer);
    setEntries((current) => current.map((entry) => entry.id === id ? { ...entry, leaving: true } : entry));
    const removalTimer = window.setTimeout(() => {
      setEntries((current) => current.filter((entry) => entry.id !== id));
      timers.current.delete(id);
    }, 220);
    timers.current.set(id, removalTimer);
  }, []);

  useEffect(() => {
    const receive = (entry: AppToastEntry) => {
      setEntries((current) => [...current, entry]);
      const lifetime = entry.level === "error" ? 5200 : entry.level === "warning" ? 4200 : 3200;
      timers.current.set(entry.id, window.setTimeout(() => dismiss(entry.id), lifetime));
    };
    appToastSubscribers.add(receive);
    return () => {
      appToastSubscribers.delete(receive);
      timers.current.forEach((timer) => window.clearTimeout(timer));
      timers.current.clear();
    };
  }, [dismiss]);

  if (entries.length === 0) return null;
  return createPortal(
    <div className="app-toast-stack" aria-label="通知">
      {entries.map((entry, index) => (
        <button
          className={`app-toast app-toast--${entry.level}${entry.leaving ? " app-toast--leaving" : ""}`}
          type="button"
          key={entry.id}
          role={entry.level === "error" ? "alert" : "status"}
          aria-live={entry.level === "error" ? "assertive" : "polite"}
          aria-label={`${entry.text}，点击关闭`}
          style={{ "--toast-offset": `${index * 72}px` } as CSSProperties}
          onClick={() => dismiss(entry.id)}
        >
          {entry.text}
        </button>
      ))}
    </div>,
    document.body,
  );
}

type AppConfirmTone = "danger" | "warning";

interface AppConfirmRequest {
  id: number;
  title: string;
  message: string;
  confirmLabel: string;
  tone: AppConfirmTone;
  resolve: (accepted: boolean) => void;
}

let nextAppConfirmId = 1;
let appConfirmSubscriber: ((request: AppConfirmRequest) => void) | null = null;

function requestAppConfirmation(options: {
  title: string;
  message: string;
  confirmLabel?: string;
  tone?: AppConfirmTone;
}): Promise<boolean> {
  return new Promise((resolve) => {
    if (!appConfirmSubscriber) {
      resolve(false);
      return;
    }
    appConfirmSubscriber({
      id: nextAppConfirmId++,
      title: options.title,
      message: options.message,
      confirmLabel: options.confirmLabel ?? "确认",
      tone: options.tone ?? "danger",
      resolve,
    });
  });
}

function AppConfirmViewport() {
  const [queue, setQueue] = useState<AppConfirmRequest[]>([]);
  const queueRef = useRef<AppConfirmRequest[]>([]);
  const dialogRef = useRef<HTMLElement | null>(null);
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  const bodyOverflowRef = useRef<string | null>(null);
  const active = queue[0] ?? null;

  const settle = useCallback((accepted: boolean) => {
    const [current, ...remaining] = queueRef.current;
    current?.resolve(accepted);
    queueRef.current = remaining;
    setQueue(remaining);
  }, []);

  useEffect(() => {
    appConfirmSubscriber = (request) => {
      setQueue((current) => {
        const next = [...current, request];
        queueRef.current = next;
        return next;
      });
    };
    return () => {
      appConfirmSubscriber = null;
      queueRef.current.forEach((request) => request.resolve(false));
      queueRef.current = [];
    };
  }, []);

  useEffect(() => {
    if (active) {
      if (bodyOverflowRef.current === null) {
        bodyOverflowRef.current = document.body.style.overflow;
        restoreFocusRef.current = document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
        document.body.style.overflow = "hidden";
      }
      return;
    }
    if (bodyOverflowRef.current !== null) {
      document.body.style.overflow = bodyOverflowRef.current;
      bodyOverflowRef.current = null;
      restoreFocusRef.current?.focus();
      restoreFocusRef.current = null;
    }
  }, [active]);

  useEffect(() => () => {
    if (bodyOverflowRef.current !== null) {
      document.body.style.overflow = bodyOverflowRef.current;
    }
    restoreFocusRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!active) return;
    const focusTimer = window.setTimeout(() => cancelRef.current?.focus(), 0);
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        settle(false);
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? []);
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.clearTimeout(focusTimer);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [active, settle]);

  if (!active) return null;
  return createPortal(
    <div className="app-confirm-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) settle(false);
    }}>
      <section ref={dialogRef} className={`app-confirm-dialog app-confirm-dialog--${active.tone}`} role="alertdialog" aria-modal="true" aria-labelledby="app-confirm-title" aria-describedby="app-confirm-message">
        <header><h2 id="app-confirm-title">{active.title}</h2></header>
        <p id="app-confirm-message">{active.message}</p>
        <footer>
          <button ref={cancelRef} className="ghost-button" type="button" onClick={() => settle(false)}>取消</button>
          <button className={active.tone === "danger" ? "danger-button" : "signal-button"} type="button" onClick={() => settle(true)}>{active.confirmLabel}</button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}

const deviceKindLabels: Record<LoomDeviceKind, string> = {
  computer: "电脑",
  tablet: "平板",
  phone: "手机",
  other: "其他",
};

function DeviceManagementPanel({
  baseUrl,
  online,
}: {
  baseUrl: string;
  online: boolean;
}) {
  const [devices, setDevices] = useState<LoomManagedDevice[]>([]);
  const [pending, setPending] = useState<LoomManagedDevice[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [approvalDialogOpen, setApprovalDialogOpen] = useState(false);
  const [editingDevice, setEditingDevice] = useState<LoomManagedDevice | null>(null);
  const [selectedPendingIds, setSelectedPendingIds] = useState<string[]>([]);
  const [draftName, setDraftName] = useState("");
  const [draftAddress, setDraftAddress] = useState("");
  const [draftKind, setDraftKind] = useState<LoomDeviceKind>("computer");

  const applyResponse = useCallback((response: Awaited<ReturnType<typeof listManagedDevices>>) => {
    setDevices([...(response.devices ?? [])].sort((left, right) => Number(right.isLocal) - Number(left.isLocal)));
    setPending(response.pending ?? []);
  }, []);

  const refreshDevices = useCallback(async (quiet = false) => {
    if (!online) {
      setDevices([]);
      setPending([]);
      return;
    }
    if (!quiet) setLoading(true);
    try {
      applyResponse(await listManagedDevices(baseUrl));
    } catch (error) {
      if (!quiet) pushAppToast({ level: "error", text: error instanceof Error ? error.message : "设备列表加载失败" });
    } finally {
      if (!quiet) setLoading(false);
    }
  }, [applyResponse, baseUrl, online]);

  useEffect(() => {
    void refreshDevices();
    if (!online) return;
    const timer = window.setInterval(() => void refreshDevices(true), 4000);
    return () => window.clearInterval(timer);
  }, [online, refreshDevices]);

  useEffect(() => {
    if (!dialogOpen && !approvalDialogOpen && !editingDevice) return;
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape" && !busyId) {
        setDialogOpen(false);
        setApprovalDialogOpen(false);
        setEditingDevice(null);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [approvalDialogOpen, busyId, dialogOpen, editingDevice]);

  const submitDevice = async () => {
    const name = draftName.trim();
    const address = draftAddress.trim();
    if (!name || !address) {
      pushAppToast({ level: "warning", text: "请填写设备名称和连接地址" });
      return;
    }
    setBusyId("create");
    try {
      applyResponse(await addManagedDevice(baseUrl, { name, address, kind: draftKind }));
      setDialogOpen(false);
      setDraftName("");
      setDraftAddress("");
      setDraftKind("computer");
      pushAppToast({ level: "info", text: "设备已添加" });
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "设备添加失败" });
    } finally {
      setBusyId(null);
    }
  };

  const approveSelectedDevices = async () => {
    if (selectedPendingIds.length === 0) return;
    setBusyId("approve-selected");
    try {
      let response: Awaited<ReturnType<typeof approveManagedDevice>> | null = null;
      for (const deviceId of selectedPendingIds) {
        response = await approveManagedDevice(baseUrl, deviceId);
      }
      if (response) applyResponse(response);
      setApprovalDialogOpen(false);
      setSelectedPendingIds([]);
      pushAppToast({ level: "info", text: `已批准 ${selectedPendingIds.length} 台设备` });
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "批准失败" });
    } finally {
      setBusyId(null);
    }
  };

  const removeDevice = async (device: LoomManagedDevice) => {
    setBusyId(device.id);
    try {
      applyResponse(await removeManagedDevice(baseUrl, device.id));
      setEditingDevice(null);
      pushAppToast({ level: "info", text: `已移除 ${device.name}` });
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "移除失败" });
    } finally {
      setBusyId(null);
    }
  };

  const saveEditedDevice = async () => {
    if (!editingDevice || editingDevice.isLocal) return;
    const name = editingDevice.name.trim();
    const address = editingDevice.address.trim();
    if (!name || !address) {
      pushAppToast({ level: "warning", text: "请填写设备名称和连接地址" });
      return;
    }
    setBusyId(editingDevice.id);
    try {
      applyResponse(await updateManagedDevice(baseUrl, editingDevice.id, {
        name,
        address,
        kind: editingDevice.kind,
        enabled: editingDevice.enabled ?? true,
      }));
      setEditingDevice(null);
      pushAppToast({ level: "info", text: "设备设置已保存" });
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "设备保存失败" });
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="device-management">
      <div className="device-management__toolbar">
        <div>
          <div className="device-management__title-row">
            <h2>所有设备</h2>
            <span>{devices.length} 个</span>
          </div>
        </div>
        <div className="device-management__actions">
          <button
            className="ghost-button device-approval-button"
            type="button"
            onClick={() => {
              setSelectedPendingIds([]);
              setApprovalDialogOpen(true);
            }}
          >
            批准设备
            {pending.length > 0 ? <span>{pending.length}</span> : null}
          </button>
          <button className="signal-button" type="button" onClick={() => setDialogOpen(true)} disabled={!online}>
            <span aria-hidden="true">＋</span> 添加设备
          </button>
        </div>
      </div>

      {loading ? <div className="device-empty">正在读取设备...</div> : devices.length === 0 ? (
        <div className="device-empty">
          <ShellIcon kind="device" />
          <strong>还没有已添加的设备</strong>
          <span>添加电脑、平板或其他客户端后会显示在这里。</span>
        </div>
      ) : (
        <div className="device-card-grid">
          {devices.map((device) => {
            const selected = editingDevice?.id === device.id;
            const enabled = device.enabled ?? true;
            return <button
              className={`device-card${device.isLocal ? " device-card--local" : ""}${selected ? " device-card--selected" : ""}${enabled ? "" : " device-card--disabled"}`}
              type="button"
              key={device.id}
              aria-pressed={selected}
              onClick={() => setEditingDevice({ ...device, enabled })}
            >
              <div className="device-card__icon"><ShellIcon kind="device" /></div>
              <div className="device-card__body">
                <div className="device-card__heading">
                  <strong>{device.name}</strong>
                  <span className="device-status device-status--approved">{device.isLocal ? "本机" : enabled ? "启用" : "禁用"}</span>
                </div>
                {!device.isLocal ? <span>{deviceKindLabels[device.kind]}</span> : null}
                <code>{device.address}</code>
              </div>
              <span className={enabled ? "device-card__online" : "device-card__offline"} title={enabled ? "设备已启用" : "设备已禁用"} />
            </button>;
          })}
        </div>
      )}

      {dialogOpen ? createPortal(
        <div
          className="framework-dialog-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && !busyId) setDialogOpen(false);
          }}
        >
          <section className="framework-dialog device-add-dialog" role="dialog" aria-modal="true" aria-labelledby="device-add-title">
            <header className="framework-dialog__header">
              <div>
                <h2 id="device-add-title">添加设备</h2>
                <p>保存客户端的连接地址。</p>
              </div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setDialogOpen(false)} disabled={Boolean(busyId)}>
                <ShellIcon kind="close" />
              </button>
            </header>
            <div className="device-add-dialog__body">
              <label>
                <span>设备名称</span>
                <input className="studio-input" autoFocus value={draftName} onChange={(event) => setDraftName(event.target.value)} placeholder="例如：工作室电脑" />
              </label>
              <label>
                <span>设备类型</span>
                <select className="studio-input" value={draftKind} onChange={(event) => setDraftKind(event.target.value as LoomDeviceKind)}>
                  {Object.entries(deviceKindLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                </select>
              </label>
              <label className="device-add-dialog__wide">
                <span>连接地址</span>
                <input className="studio-input" value={draftAddress} onChange={(event) => setDraftAddress(event.target.value)} placeholder="192.168.1.28:19820" />
              </label>
            </div>
            <footer className="device-add-dialog__footer">
              <button className="ghost-button" type="button" onClick={() => setDialogOpen(false)} disabled={Boolean(busyId)}>取消</button>
              <button className="signal-button" type="button" onClick={() => void submitDevice()} disabled={Boolean(busyId)}>{busyId ? "添加中..." : "添加"}</button>
            </footer>
          </section>
        </div>,
        document.body,
      ) : null}
      {approvalDialogOpen ? createPortal(
        <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget && !busyId) setApprovalDialogOpen(false);
        }}>
          <section className="framework-dialog device-approval-dialog" role="dialog" aria-modal="true" aria-labelledby="device-approval-title">
            <header className="framework-dialog__header">
              <div><h2 id="device-approval-title">批准设备</h2><p>{pending.length} 台设备等待批准</p></div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setApprovalDialogOpen(false)} disabled={Boolean(busyId)}><ShellIcon kind="close" /></button>
            </header>
            <div className="device-approval-dialog__list">
              {pending.length === 0 ? <div className="device-approval__empty">暂无待批准设备</div> : pending.map((device) => {
                const checked = selectedPendingIds.includes(device.id);
                return <label className={checked ? "device-request-row device-request-row--selected" : "device-request-row"} key={device.id}>
                  <input type="checkbox" checked={checked} onChange={() => setSelectedPendingIds((ids) => checked ? ids.filter((id) => id !== device.id) : [...ids, device.id])} />
                  <div className="device-card__icon"><ShellIcon kind="device" /></div>
                  <div className="device-request-row__identity"><strong>{device.name}</strong><span>{deviceKindLabels[device.kind]} · {device.address}</span></div>
                </label>;
              })}
            </div>
            <footer className="device-add-dialog__footer">
              <button className="ghost-button" type="button" onClick={() => setApprovalDialogOpen(false)} disabled={Boolean(busyId)}>取消</button>
              <button className="signal-button" type="button" onClick={() => void approveSelectedDevices()} disabled={Boolean(busyId) || selectedPendingIds.length === 0}>{busyId ? "批准中..." : `批准 ${selectedPendingIds.length || ""}`}</button>
            </footer>
          </section>
        </div>, document.body,
      ) : null}
      {editingDevice ? createPortal(
        <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget && !busyId) setEditingDevice(null);
        }}>
          <section className="framework-dialog device-edit-dialog" role="dialog" aria-modal="true" aria-labelledby="device-edit-title">
            <header className="framework-dialog__header">
              <div><h2 id="device-edit-title">编辑设备</h2><p>{editingDevice.id}</p></div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setEditingDevice(null)} disabled={Boolean(busyId)}><ShellIcon kind="close" /></button>
            </header>
            <div className="device-edit-dialog__body">
              <label><span>设备名称</span><input className="studio-input" autoFocus={!editingDevice.isLocal} disabled={editingDevice.isLocal} value={editingDevice.name} onChange={(event) => setEditingDevice({ ...editingDevice, name: event.target.value })} /></label>
              <label><span>设备类型</span><select className="studio-input" disabled={editingDevice.isLocal} value={editingDevice.kind} onChange={(event) => setEditingDevice({ ...editingDevice, kind: event.target.value as LoomDeviceKind })}>{Object.entries(deviceKindLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
              <label className="device-edit-dialog__wide"><span>连接地址</span><input className="studio-input" disabled={editingDevice.isLocal} value={editingDevice.address} onChange={(event) => setEditingDevice({ ...editingDevice, address: event.target.value })} /></label>
              {editingDevice.isLocal ? <p className="device-edit-dialog__protected">Loom 启动主机始终启用，不能删除。</p> : <label className="device-edit-toggle"><input type="checkbox" checked={editingDevice.enabled ?? true} onChange={(event) => setEditingDevice({ ...editingDevice, enabled: event.target.checked })} /><span>启用设备</span></label>}
            </div>
            <footer className="device-edit-dialog__footer">
              {!editingDevice.isLocal ? <button className="danger-button" type="button" disabled={Boolean(busyId)} onClick={() => void (async () => {
                const accepted = await requestAppConfirmation({
                  title: "删除设备",
                  message: `删除 ${editingDevice.name} 后，需要重新添加或批准才能再次使用。`,
                  confirmLabel: "删除",
                });
                if (accepted) await removeDevice(editingDevice);
              })()}>删除</button> : <span />}
              <div>
                <button className="ghost-button" type="button" disabled={Boolean(busyId)} onClick={() => setEditingDevice(null)}>取消</button>
                {!editingDevice.isLocal ? <button className="signal-button" type="button" disabled={Boolean(busyId)} onClick={() => void saveEditedDevice()}>{busyId ? "保存中..." : "保存"}</button> : null}
              </div>
            </footer>
          </section>
        </div>, document.body,
      ) : null}
    </div>
  );
}

function CredentialFieldDialog({
  draft,
  busy,
  onChange,
  onSave,
  onDelete,
  onClose,
}: {
  draft: CredentialFieldDraft | null;
  busy: boolean;
  onChange: (draft: CredentialFieldDraft) => void;
  onSave: () => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const dialogOpen = draft !== null;

  useEffect(() => {
    if (!dialogOpen) return;
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busy) onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [busy, dialogOpen, onClose]);

  if (!draft) return null;
  return createPortal(
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (!busy && event.target === event.currentTarget) onClose();
      }}
    >
      <section className="framework-dialog credential-field-dialog" role="dialog" aria-modal="true" aria-labelledby="credential-field-dialog-title">
        <header className="framework-dialog__header">
          <h2 id="credential-field-dialog-title">{draft.original ? "编辑字段" : "添加字段"}</h2>
          <button className="icon-button" type="button" aria-label="关闭" disabled={busy} onClick={onClose}>
            <ArtIcon kind="close" />
          </button>
        </header>
        <div className="credential-field-dialog__form">
          <input autoFocus className="studio-input" aria-label="字段名字" placeholder="名字" value={draft.name} onChange={(event) => onChange({ ...draft, name: event.target.value })} />
          <select className="studio-input" aria-label="字段格式" value={draft.valueType} onChange={(event) => onChange({ ...draft, valueType: event.target.value as LoomCredentialValueType, value: "" })}>
            {(Object.entries(credentialValueTypeLabels) as Array<[LoomCredentialValueType, string]>).map(([valueType, label]) => (
              <option key={valueType} value={valueType}>{label}</option>
            ))}
          </select>
          {draft.valueType === "boolean" ? (
            <select className="studio-input" aria-label="字段值" value={draft.value} onChange={(event) => onChange({ ...draft, value: event.target.value })}>
              <option value="">选择</option>
              <option value="true">true</option>
              <option value="false">false</option>
            </select>
          ) : draft.valueType === "json" ? (
            <textarea className="studio-input credential-field-dialog__value" aria-label="字段值" placeholder="{}" value={draft.value} onChange={(event) => onChange({ ...draft, value: event.target.value })} />
          ) : (
            <input
              className="studio-input"
              aria-label="字段值"
              type={draft.valueType === "string" ? "text" : "number"}
              step={draft.valueType === "integer" ? 1 : "any"}
              placeholder="值"
              value={draft.value}
              onChange={(event) => onChange({ ...draft, value: event.target.value })}
            />
          )}
        </div>
        <footer className="credential-field-dialog__actions">
          {draft.original ? (
            <button className="danger-button" type="button" disabled={busy} onClick={onDelete}>删除</button>
          ) : <span />}
          <button className="signal-button" type="button" disabled={busy || !draft.name.trim() || !draft.value} onClick={onSave}>
            {busy ? "保存中" : "保存"}
          </button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}

function PluginSecurityPanel({ baseUrl }: { baseUrl: string }) {
  const [trust, setTrust] = useState<LoomPluginTrustStore>(defaultPluginTrustStore);
  const [credentials, setCredentials] = useState<LoomCredentialDetails[]>([]);
  const [identityState, setIdentityState] = useState<LoomPublisherIdentityState>({ identity: null, hasPrivateKey: false });
  const [privateKey, setPrivateKey] = useState("");
  const [showPrivateKey, setShowPrivateKey] = useState(false);
  const [trustedUserId, setTrustedUserId] = useState("");
  const [credentialDraft, setCredentialDraft] = useState<CredentialFieldDraft | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const loadVersion = useRef(0);

  const load = useCallback(async () => {
    const version = ++loadVersion.current;
    try {
      const [nextTrust, summaries, nextIdentity] = await Promise.all([
        listPluginTrust(baseUrl),
        listPluginCredentials(baseUrl),
        getPublisherIdentity(baseUrl),
      ]);
      const visibleSummaries = summaries.filter((credential) => (
        !credential.name.startsWith("loom-")
        && !credential.scope.frameworkId
        && !credential.scope.artId
      ));
      const details = await Promise.all(visibleSummaries.map(async (credential) => (
        await revealPluginCredential(baseUrl, credential.name, credential.scope)
        ?? { ...credential, value: "" }
      )));
      const revealedPrivateKey = nextIdentity.identity && nextIdentity.hasPrivateKey
        ? (await revealPublisherPrivateKey(baseUrl)).privateKey
        : "";
      if (version !== loadVersion.current) return;
      setTrust(nextTrust);
      setCredentials(details);
      setIdentityState(nextIdentity);
      setPrivateKey(revealedPrivateKey);
    } catch (err) {
      if (version === loadVersion.current) {
        pushAppToast({ level: "error", text: err instanceof Error ? err.message : "无法读取密钥与安全设置。" });
      }
    }
  }, [baseUrl]);

  useEffect(() => {
    void load();
    return () => {
      loadVersion.current += 1;
    };
  }, [load]);

  const openNewCredential = () => {
    setCredentialDraft({
      name: "",
      value: "",
      valueType: "string",
    });
  };

  const openCredential = (credential: LoomCredentialDetails) => {
    setCredentialDraft({
      name: credential.name,
      value: credential.value,
      valueType: credential.valueType ?? "string",
      original: credential,
    });
  };

  const saveCredential = async () => {
    if (!credentialDraft) return;
    setBusy("credential");
    try {
      await savePluginCredential(baseUrl, {
        name: credentialDraft.name.trim(),
        value: credentialDraft.value,
        valueType: credentialDraft.valueType,
        scope: {},
      });
      const original = credentialDraft.original;
      if (original && original.name !== credentialDraft.name.trim()) {
        await deletePluginCredential(baseUrl, original.name, original.scope);
      }
      setCredentialDraft(null);
      await load();
      pushAppToast({ level: "info", text: "已保存" });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "保存失败。" });
    } finally {
      setBusy(null);
    }
  };

  const removeCredential = async () => {
    const credential = credentialDraft?.original;
    if (!credential) return;
    if (!await requestAppConfirmation({
      title: "删除凭据",
      message: `删除 ${credential.name} 后，引用该凭据的 Art 可能无法运行。`,
      confirmLabel: "删除",
    })) return;
    setBusy("credential");
    try {
      await deletePluginCredential(baseUrl, credential.name, credential.scope);
      setCredentialDraft(null);
      await load();
      pushAppToast({ level: "info", text: "已删除" });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "删除失败。" });
    } finally {
      setBusy(null);
    }
  };

  const updatePolicy = async (policy: LoomPluginTrustPolicy) => {
    setBusy("policy");
    try {
      setTrust(await setPluginTrustPolicy(baseUrl, policy));
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "策略保存失败。" });
    } finally {
      setBusy(null);
    }
  };

  const addTrustedUser = async () => {
    const userId = trustedUserId.trim();
    setBusy("trusted-user");
    try {
      setTrust(await trustPluginUser(baseUrl, userId));
      setTrustedUserId("");
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "添加可信用户失败。" });
    } finally {
      setBusy(null);
    }
  };

  const removeTrustedUser = async (userId: string) => {
    if (!await requestAppConfirmation({
      title: "移除可信用户",
      message: `从信任库移除 ${userId} 后，该用户后续发布的 Art 将不再被自动信任。`,
      confirmLabel: "移除",
    })) return;
    setBusy(userId);
    try {
      setTrust(await untrustPluginUser(baseUrl, userId));
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "移除可信用户失败。" });
    } finally {
      setBusy(null);
    }
  };

  const resetIdentity = async () => {
    if (!await requestAppConfirmation({
      title: "重置密钥",
      message: "重置会生成新的私钥和公钥。平台仍需保留旧公钥，才能验证旧版本 Art。",
      confirmLabel: "重置",
    })) return;
    setBusy("identity");
    try {
      setIdentityState(await rotatePublisherIdentity(baseUrl));
      await load();
      pushAppToast({ level: "info", text: "密钥已重置" });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "密钥重置失败。" });
    } finally {
      setBusy(null);
    }
  };

  const identity = identityState.identity;
  return (
    <div className="plugin-security">
      <section className="security-section security-section--credentials">
        <div className="credential-field-list" role="list">
          <div className="credential-field-row credential-field-row--head">
            <span>名字</span><span>格式</span><span>值</span>
            <button className="icon-button" type="button" aria-label="添加字段" title="添加字段" disabled={busy !== null} onClick={openNewCredential}>
              <ArtIcon kind="plus" />
            </button>
          </div>
          {credentials.map((credential) => (
            <div className="credential-field-row" role="listitem" key={credentialFieldId(credential)}>
              <strong title={credential.name}>{credential.name}</strong>
              <span>{credentialValueTypeLabels[credential.valueType ?? "string"]}</span>
              <code title={credential.value}>{credential.value}</code>
              <button className="icon-button" type="button" aria-label={`编辑 ${credential.name}`} title="编辑" disabled={busy !== null} onClick={() => openCredential(credential)}>
                <ArtIcon kind="edit" />
              </button>
            </div>
          ))}
          {credentials.length === 0 ? <p className="muted-line">暂无字段</p> : null}
        </div>
      </section>

      <section className="security-section security-section--policy">
        <div className="security-policy-row">
          <strong>安装策略</strong>
          <select className="studio-input security-policy-select" aria-label="Art 安装信任原则" value={trust.policy} disabled={busy !== null} onChange={(event) => void updatePolicy(event.target.value as LoomPluginTrustPolicy)}>
            <option value="require_signed">安装签名认证成功的</option>
            <option value="require_trusted">安装签名认证成功且在信任库中用户发布的 Art</option>
            <option value="allow_unsigned">可安装无签名 Art</option>
          </select>
        </div>

        {trust.policy === "require_trusted" ? (
          <div className="trusted-user-library">
            <div className="trusted-user-library__add">
              <input className="studio-input" aria-label="可信用户 ID" placeholder="L0000000000" value={trustedUserId} onChange={(event) => setTrustedUserId(event.target.value)} />
              <button className="icon-button" type="button" aria-label="添加可信用户" title="添加" disabled={busy !== null || !/^(?:NU\d{11}|L\d{10})$/.test(trustedUserId.trim())} onClick={() => void addTrustedUser()}>
                <ArtIcon kind="plus" />
              </button>
            </div>
            <div className="trusted-user-library__list">
              {trust.trustedPublishers.map((userId) => (
                <div className="trusted-user-library__row" key={userId}>
                  <code>{userId}</code>
                  <span>{trust.publishers.filter((key) => key.publisherId === userId && !key.revoked).length} keys</span>
                  <button className="icon-button" type="button" aria-label={`移除 ${userId}`} title="移除" disabled={busy !== null} onClick={() => void removeTrustedUser(userId)}>
                    <ArtIcon kind="trash" />
                  </button>
                </div>
              ))}
              {trust.trustedPublishers.length === 0 ? <p className="muted-line">暂无可信用户</p> : null}
            </div>
          </div>
        ) : null}

        <div className="publisher-identity">
          <div className="publisher-identity__id-row">
            <div className="publisher-identity__id">
              <strong>用户 ID：</strong>
              <code>{identity?.userId ?? "L0000000000"}</code>
            </div>
            <button className="ghost-button publisher-identity__action" type="button" disabled={busy !== null} onClick={() => void resetIdentity()}>
              {busy === "identity" ? "重置中" : "重置密钥"}
            </button>
          </div>
          <div className="publisher-identity__keys">
            <label>
              <span>私钥</span>
              <div className="publisher-identity__secret">
                <input className="studio-input mono-line" readOnly type={showPrivateKey ? "text" : "password"} value={privateKey} />
                <button className="icon-button" type="button" aria-label={showPrivateKey ? "隐藏私钥" : "显示私钥"} title={showPrivateKey ? "隐藏" : "显示"} onClick={() => setShowPrivateKey((shown) => !shown)}>
                  <ArtIcon kind="eye" />
                </button>
              </div>
            </label>
            <label>
              <span>公钥</span>
              <input className="studio-input mono-line" readOnly value={identity?.publicKey ?? ""} />
            </label>
          </div>
        </div>
      </section>

      <CredentialFieldDialog
        draft={credentialDraft}
        busy={busy === "credential"}
        onChange={setCredentialDraft}
        onSave={() => void saveCredential()}
        onDelete={() => void removeCredential()}
        onClose={() => setCredentialDraft(null)}
      />
    </div>
  );
}

function ArtStoreCard({
  baseUrl,
  active,
  frameworks,
  selectedFrameworkIds,
  searchText,
  officialOnly,
  refreshToken,
  onInstalled,
}: {
  baseUrl: string;
  active: boolean;
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  searchText: string;
  officialOnly: boolean;
  refreshToken: number;
  onInstalled: () => void | Promise<void>;
}) {
  const [catalog, setCatalog] = useState<ArtStoreEntry[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const catalogLoadVersion = useRef(0);
  const visibleCatalog = useMemo(() => filterArtStoreEntries(
    catalog,
    frameworks,
    selectedFrameworkIds,
    searchText,
    officialOnly,
  ), [catalog, frameworks, officialOnly, searchText, selectedFrameworkIds]);

  const loadCatalog = useCallback(async () => {
    const version = ++catalogLoadVersion.current;
    setCatalogLoading(true);
    setMessage(null);
    try {
      const arts = await fetchArtStoreCatalog(baseUrl);
      if (version !== catalogLoadVersion.current) return;
      setCatalog(arts);
    } catch (err) {
      if (version === catalogLoadVersion.current) {
        setMessage({ ok: false, text: err instanceof Error ? err.message : "无法访问商店。" });
      }
    } finally {
      if (version === catalogLoadVersion.current) setCatalogLoading(false);
    }
  }, [baseUrl]);

  useEffect(() => {
    if (!active) return;
    void loadCatalog();
    return () => {
      catalogLoadVersion.current += 1;
    };
  }, [active, loadCatalog, refreshToken]);

  const install = async (art: ArtStoreEntry) => {
    if (art.official !== true && !await requestAppConfirmation({
      title: "安装未认证 Art",
      message: `“${art.name ?? art.id}”未经官方认证，可能包含恶意代码。仅在信任发布者和包来源时继续。`,
      confirmLabel: "继续安装",
      tone: "warning",
    })) return;
    setInstallingId(art.id);
    setMessage(null);
    try {
      await installArtFromStore(baseUrl, art.id);
      setMessage({ ok: true, text: `已安装 ${art.name ?? art.id}。` });
      await onInstalled();
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : "安装失败。" });
    } finally {
      setInstallingId(null);
    }
  };

  return (
    <section className="content-grid art-store">
      {message ? (
        <div className={message.ok ? "art-store__message success-text" : "art-store__message error-text"}>
          <span>{message.text}</span>
          {!message.ok ? (
            <button className="ghost-button" type="button" onClick={() => void loadCatalog()} disabled={catalogLoading}>
              重试
            </button>
          ) : null}
        </div>
      ) : null}
      {catalogLoading && catalog.length === 0 ? <p className="muted-line">加载中…</p> : null}
      {!catalogLoading && !message && visibleCatalog.length === 0 ? <p className="muted-line">没有匹配的 Art</p> : null}
      <div className="card-grid art-store-grid">
        {visibleCatalog.map((art) => {
          const frameworkReference = art.framework?.startsWith("neuro.official/")
            ? art.framework.slice("neuro.official/".length)
            : art.framework ?? null;
          const frameworkLabel = officialFrameworkDisplayName(art.framework) ?? art.framework ?? "Art";
          const installing = installingId === art.id;
          return (
            <article
              className={`glass-card art-store-card ${art.official === true ? "art-store-card--official" : "art-store-card--unverified"}`}
              key={art.qualifiedId || art.id}
            >
              <div className="art-store-card__head">
                <h3 title={art.name ?? art.id}>{art.name ?? art.id}</h3>
                <div className="art-store-card__badges">
                  <span className={art.official === true ? "art-store-card__trust art-store-card__trust--official" : "art-store-card__trust art-store-card__trust--unverified"}>
                    {art.official === true ? "官方" : "未认证"}
                  </span>
                  <span
                    className="art-registry-card__framework-icon"
                    role="img"
                    aria-label={`${frameworkLabel} Art`}
                    title={frameworkLabel}
                  >
                    <ArtIcon kind={artFrameworkIconKind(frameworkReference)} />
                  </span>
                </div>
              </div>
              <div className="art-store-card__body">
                {art.description ? (
                  <p className="art-store-card__description" title={art.description}>{art.description}</p>
                ) : null}
                <p className="art-store-card__identity mono-line" title={art.globalId || art.qualifiedId || art.id}>
                  {art.globalId || art.qualifiedId || art.id}
                  {art.latestVersion ? ` · ${art.latestVersion}` : ""}
                </p>
              </div>
              <div className="art-store-card__actions">
                <button
                  className="signal-button"
                  type="button"
                  onClick={() => void install(art)}
                  disabled={installingId !== null}
                >
                  {installing ? "安装中" : "安装"}
                </button>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}

function FrameworkFilter({
  frameworks,
  selectedFrameworkIds,
  onToggle,
  actions,
}: {
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  onToggle: (frameworkId: string) => void;
  actions: ReactNode;
}) {
  return (
    <div className="framework-filter" role="group" aria-label="按框架筛选 Art">
      <div className="framework-filter__options">
        {frameworks.map((framework) => {
          const identity = frameworkIdentity(framework);
          const checked = selectedFrameworkIds === null || selectedFrameworkIds.has(identity);
          return (
            <label
              className={checked ? "framework-filter__option framework-filter__option--checked" : "framework-filter__option"}
              key={identity}
            >
              <input
                type="checkbox"
                checked={checked}
                onChange={() => onToggle(identity)}
              />
              <span>{frameworkFilterLabel(framework)}</span>
            </label>
          );
        })}
        {frameworks.length === 0 ? <span className="muted-line">暂无框架</span> : null}
      </div>
      <div className="framework-filter__actions">
        {actions}
      </div>
    </div>
  );
}

function authoredArtVersion(tool: LoomToolDefinition): string {
  const metadata = typeof tool.metadata === "object" && tool.metadata !== null && !Array.isArray(tool.metadata)
    ? tool.metadata as Record<string, unknown>
    : null;
  const packageSecurity = typeof metadata?.packageSecurity === "object"
    && metadata.packageSecurity !== null
    && !Array.isArray(metadata.packageSecurity)
    ? metadata.packageSecurity as Record<string, unknown>
    : null;
  return typeof packageSecurity?.version === "string" && packageSecurity.version.trim()
    ? packageSecurity.version.trim()
    : "0.1.0";
}

function ArtPublishDialog({
  open,
  tools,
  baseUrl,
  onClose,
  onPublished,
}: {
  open: boolean;
  tools: LoomToolDefinition[];
  baseUrl: string;
  onClose: () => void;
  onPublished: () => Promise<void>;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const [busyArtId, setBusyArtId] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const authoredTools = useMemo(() => tools.filter(isLocallyAuthoredTool), [tools]);

  useEffect(() => {
    if (!open) {
      setMessage(null);
      return;
    }
    closeButtonRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busyArtId) onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )];
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [busyArtId, onClose, open]);

  if (!open) return null;

  const publish = async (tool: LoomToolDefinition) => {
    setBusyArtId(tool.id);
    setMessage(null);
    try {
      const result = await publishArt(baseUrl, tool.id);
      await onPublished();
      setMessage({ ok: true, text: `已发布 ${tool.name || tool.id} · ${result.globalId}` });
    } catch (error) {
      setMessage({ ok: false, text: error instanceof Error ? error.message : "发布失败。" });
    } finally {
      setBusyArtId(null);
    }
  };

  return (
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (!busyArtId && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="framework-dialog art-publish-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="art-publish-dialog-title"
      >
        <header className="framework-dialog__header">
          <h2 id="art-publish-dialog-title">发布 Art</h2>
          <button className="ghost-button" type="button" ref={closeButtonRef} onClick={onClose} disabled={busyArtId !== null}>
            关闭
          </button>
        </header>
        {message ? <p className={message.ok ? "success-text" : "error-text"}>{message.text}</p> : null}
        <div className="art-publish-dialog__list">
          {authoredTools.map((tool) => {
            const busy = busyArtId === tool.id;
            return (
              <article className="art-publish-dialog__row" key={tool.id}>
                <div className="art-publish-dialog__identity">
                  <h3 title={tool.name || tool.id}>{tool.name || tool.id}</h3>
                  <p className="mono-line" title={tool.id}>{tool.id}</p>
                </div>
                <span className="art-publish-dialog__version">{authoredArtVersion(tool)}</span>
                <button className="signal-button" type="button" onClick={() => void publish(tool)} disabled={busyArtId !== null}>
                  {busy ? "发布中" : "发布"}
                </button>
              </article>
            );
          })}
          {authoredTools.length === 0 ? <p className="muted-line">暂无本地创建的 Art</p> : null}
        </div>
      </div>
    </div>
  );
}

type FrameworkBusyAction = "toggle" | "upgrade" | null;

function readFrameworkPackageBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("无法读取框架更新包。"));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      const separator = result.indexOf(",");
      if (separator < 0 || !result.slice(separator + 1)) {
        reject(new Error("框架更新包内容为空。"));
        return;
      }
      resolve(result.slice(separator + 1));
    };
    reader.readAsDataURL(file);
  });
}

function FrameworkManagementDialog({
  open,
  frameworks,
  busyId,
  busyAction,
  error,
  message,
  onClose,
  onToggle,
  onUpgrade,
}: {
  open: boolean;
  frameworks: LoomFramework[];
  busyId: string | null;
  busyAction: FrameworkBusyAction;
  error: string | null;
  message: StudioMessage | null;
  onClose: () => void;
  onToggle: (framework: LoomFramework) => Promise<void>;
  onUpgrade: (framework: LoomFramework, file: File) => Promise<void>;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const fileInputRefs = useRef<Record<string, HTMLInputElement | null>>({});
  const [pendingUninstallId, setPendingUninstallId] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    closeButtonRef.current?.focus();
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]):not([hidden]), [tabindex]:not([tabindex="-1"])',
      )];
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  useEffect(() => {
    if (!open) setPendingUninstallId(null);
  }, [open]);

  if (!open) return null;

  return (
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="framework-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="framework-dialog-title"
      >
        <header className="framework-dialog__header">
          <h2 id="framework-dialog-title">管理框架</h2>
          <button className="ghost-button" type="button" ref={closeButtonRef} onClick={onClose}>
            关闭
          </button>
        </header>
        {error ? <p className="error-text">{error}</p> : null}
        {message ? (
          <p className={message.kind === "error" ? "error-text" : "success-text"}>{message.text}</p>
        ) : null}
        <div className="framework-dialog__table-wrap">
          <table className="framework-dialog__table">
            <thead>
              <tr>
                <th scope="col">框架</th>
                <th scope="col">版本</th>
                <th scope="col">安装</th>
                <th scope="col">更新</th>
              </tr>
            </thead>
            <tbody>
              {frameworks.map((framework) => {
                const identity = frameworkIdentity(framework);
                const rowBusy = busyId === identity;
                const toggleBusy = rowBusy && busyAction === "toggle";
                const upgradeBusy = rowBusy && busyAction === "upgrade";
                const confirmingUninstall = framework.installed && pendingUninstallId === identity;
                return (
                  <tr key={identity}>
                    <th scope="row">{frameworkFilterLabel(framework)}</th>
                    <td>{framework.version || "—"}</td>
                    <td>
                      <button
                        className={framework.installed ? "ghost-button" : "signal-button"}
                        type="button"
                        disabled={busyId !== null}
                        onClick={() => {
                          if (!framework.installed) {
                            void onToggle(framework);
                            return;
                          }
                          if (!confirmingUninstall) {
                            setPendingUninstallId(identity);
                            return;
                          }
                          setPendingUninstallId(null);
                          void onToggle(framework);
                        }}
                      >
                        {toggleBusy
                          ? "处理中"
                          : confirmingUninstall
                            ? "确认卸载"
                            : framework.installed
                              ? "卸载"
                              : "安装"}
                      </button>
                    </td>
                    <td>
                      <input
                        hidden
                        ref={(element) => {
                          fileInputRefs.current[identity] = element;
                        }}
                        type="file"
                        accept=".zip,application/zip"
                        onChange={(event) => {
                          const file = event.currentTarget.files?.[0];
                          event.currentTarget.value = "";
                          if (file) void onUpgrade(framework, file);
                        }}
                      />
                      <button
                        className="ghost-button"
                        type="button"
                        disabled={!framework.installed || busyId !== null}
                        onClick={() => fileInputRefs.current[identity]?.click()}
                      >
                        {upgradeBusy ? "更新中" : "更新"}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {frameworks.length === 0 && !error ? <p className="muted-line">加载中…</p> : null}
        </div>
      </div>
    </div>
  );
}

function ArtPanel({
  tools,
  mcpServers,
  workflows,
  baseUrl,
  refresh,
  pendingCreationRequest,
  onCreationRequestHandled,
}: {
  tools: LoomToolDefinition[];
  mcpServers: LoomMcpServer[];
  workflows: LoomWorkflowMetadata[];
  baseUrl: string;
  refresh: () => Promise<void>;
  pendingCreationRequest: ArtCreationRequest | null;
  onCreationRequestHandled: () => void;
}) {
  const [activeWorkspace, setActiveWorkspace] = useState<ArtWorkspaceId>("registry");
  const [frameworks, setFrameworks] = useState<LoomFramework[]>([]);
  const [frameworkBusyId, setFrameworkBusyId] = useState<string | null>(null);
  const [frameworkBusyAction, setFrameworkBusyAction] = useState<FrameworkBusyAction>(null);
  const [frameworkError, setFrameworkError] = useState<string | null>(null);
  const [frameworkManagementMessage, setFrameworkManagementMessage] = useState<StudioMessage | null>(null);
  const [frameworkDialogOpen, setFrameworkDialogOpen] = useState(false);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createRequest, setCreateRequest] = useState<ArtCreationRequest | null>(null);
  const [publishDialogOpen, setPublishDialogOpen] = useState(false);
  const [storeSearchText, setStoreSearchText] = useState("");
  const [storeOfficialOnly, setStoreOfficialOnly] = useState(false);
  const [storeCatalogRefreshToken, setStoreCatalogRefreshToken] = useState(0);
  const [snapshotRefreshError, setSnapshotRefreshError] = useState<string | null>(null);
  const [selectedFrameworkIds, setSelectedFrameworkIds] = useState<Set<string> | null>(null);
  const frameworkLoadVersion = useRef(0);
  const createArtButtonRef = useRef<HTMLButtonElement | null>(null);
  const publishArtButtonRef = useRef<HTMLButtonElement | null>(null);
  const frameworkManageButtonRef = useRef<HTMLButtonElement | null>(null);
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const frameworkIds = useMemo(
    () => [...new Set(frameworks.map((framework) => frameworkIdentity(framework)))],
    [frameworks],
  );

  const toggleFrameworkFilter = (frameworkId: string) => {
    setSelectedFrameworkIds((current) => {
      const next = current === null ? new Set(frameworkIds) : new Set(current);
      if (next.has(frameworkId)) {
        next.delete(frameworkId);
      } else {
        next.add(frameworkId);
      }
      return next.size === frameworkIds.length ? null : next;
    });
  };

  const loadFrameworks = useCallback(async () => {
    const version = ++frameworkLoadVersion.current;
    try {
      const list = await listFrameworks(baseUrl);
      if (version !== frameworkLoadVersion.current) return;
      setFrameworks(list);
      setFrameworkError(null);
    } catch (error) {
      if (version === frameworkLoadVersion.current) {
        setFrameworkError(error instanceof Error ? error.message : "无法读取框架列表。");
      }
    }
  }, [baseUrl]);

  useEffect(() => {
    setSnapshotRefreshError(null);
    void loadFrameworks();
    return () => {
      frameworkLoadVersion.current += 1;
    };
  }, [loadFrameworks]);

  useEffect(() => {
    let cancelled = false;
    void getLoomSettings(baseUrl)
      .then((settings) => {
        if (!cancelled) setStoreOfficialOnly(settings.art_store?.official_only === true);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [baseUrl]);

  useEffect(() => {
    if (!pendingCreationRequest) return;
    setActiveWorkspace("registry");
    setFrameworkDialogOpen(false);
    setPublishDialogOpen(false);
    setCreateRequest(pendingCreationRequest);
    setCreateDialogOpen(true);
    onCreationRequestHandled();
  }, [onCreationRequestHandled, pendingCreationRequest]);

  const synchronizeArtState = useCallback(async () => {
    const [, snapshotResult] = await Promise.allSettled([loadFrameworks(), refresh()]);
    if (snapshotResult.status === "rejected") {
      const detail = snapshotResult.reason instanceof Error
        ? snapshotResult.reason.message
        : "无法刷新 Loom 主快照。";
      setSnapshotRefreshError(`Art 操作已完成，但主快照刷新失败：${detail}`);
      return;
    }
    setSnapshotRefreshError(null);
  }, [loadFrameworks, refresh]);

  const toggleFramework = async (framework: LoomFramework) => {
    const identity = frameworkIdentity(framework);
    const action = framework.installed ? "卸载" : "安装";
    setFrameworkBusyId(identity);
    setFrameworkBusyAction("toggle");
    setFrameworkError(null);
    setFrameworkManagementMessage(null);
    try {
      if (framework.installed) {
        await uninstallFramework(baseUrl, identity);
      } else {
        await installFramework(baseUrl, identity);
      }
      await synchronizeArtState();
      setFrameworkManagementMessage({ kind: "info", text: `已${action} ${frameworkFilterLabel(framework)}。` });
    } catch (error) {
      const detail = error instanceof Error ? error.message : "框架操作失败。";
      setFrameworkError(detail);
    } finally {
      setFrameworkBusyId(null);
      setFrameworkBusyAction(null);
    }
  };

  const upgradeFramework = async (framework: LoomFramework, file: File) => {
    if (!framework.installed) return;
    const identity = frameworkIdentity(framework);
    setFrameworkBusyId(identity);
    setFrameworkBusyAction("upgrade");
    setFrameworkError(null);
    setFrameworkManagementMessage(null);
    try {
      const zipBase64 = await readFrameworkPackageBase64(file);
      await upgradeFrameworkPackage(baseUrl, identity, zipBase64);
      await synchronizeArtState();
      setFrameworkManagementMessage({
        kind: "info",
        text: `已更新 ${frameworkFilterLabel(framework)}。`,
      });
    } catch (error) {
      const detail = error instanceof Error ? error.message : "框架更新失败。";
      setFrameworkError(detail);
    } finally {
      setFrameworkBusyId(null);
      setFrameworkBusyAction(null);
    }
  };

  const closeFrameworkDialog = useCallback(() => {
    setFrameworkDialogOpen(false);
    window.setTimeout(() => frameworkManageButtonRef.current?.focus(), 0);
  }, []);

  const closeCreateDialog = useCallback(() => {
    setCreateDialogOpen(false);
    setCreateRequest(null);
    window.setTimeout(() => createArtButtonRef.current?.focus(), 0);
  }, []);

  const closePublishDialog = useCallback(() => {
    setPublishDialogOpen(false);
    window.setTimeout(() => publishArtButtonRef.current?.focus(), 0);
  }, []);

  const selectAdjacentWorkspace = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const nextIndex = nextArtWorkspaceIndex(event.key, index, artWorkspaceItems.length);
    if (nextIndex === null) return;
    event.preventDefault();
    const next = artWorkspaceItems[nextIndex];
    setActiveWorkspace(next.id);
    tabRefs.current[nextIndex]?.focus();
  };

  return (
    <section className="art-hub" aria-label="Art">
      <div className="art-hub__navigation">
        <div
          className={activeWorkspace === "registry" || activeWorkspace === "store" ? "art-hub__tabs art-hub__tabs--with-filter" : "art-hub__tabs"}
          role="tablist"
          aria-label="Art 工作区"
        >
          {artWorkspaceItems.map((item, index) => {
            const active = activeWorkspace === item.id;
            return (
              <button
                key={item.id}
                ref={(element) => {
                  tabRefs.current[index] = element;
                }}
                id={`art-tab-${item.id}`}
                className={active ? "art-hub__tab art-hub__tab--active" : "art-hub__tab"}
                type="button"
                role="tab"
                aria-selected={active}
                aria-controls={`art-panel-${item.id}`}
                tabIndex={active ? 0 : -1}
                onClick={() => setActiveWorkspace(item.id)}
                onKeyDown={(event) => selectAdjacentWorkspace(event, index)}
              >
                <span>{item.label}</span>
              </button>
            );
          })}
        </div>
        {activeWorkspace === "registry" ? (
          <FrameworkFilter
            frameworks={frameworks}
            selectedFrameworkIds={selectedFrameworkIds}
            onToggle={toggleFrameworkFilter}
            actions={(
              <>
                <button
                  className="ghost-button framework-filter__create"
                  type="button"
                  ref={createArtButtonRef}
                  onClick={() => {
                    setFrameworkDialogOpen(false);
                    setPublishDialogOpen(false);
                    setCreateRequest(null);
                    setCreateDialogOpen(true);
                  }}
                >
                  创建 Art
                </button>
                <button
                  className="ghost-button framework-filter__manage"
                  type="button"
                  ref={frameworkManageButtonRef}
                  onClick={() => {
                    setCreateDialogOpen(false);
                    setPublishDialogOpen(false);
                    setFrameworkManagementMessage(null);
                    setFrameworkDialogOpen(true);
                  }}
                >
                  管理框架
                </button>
              </>
            )}
          />
        ) : activeWorkspace === "store" ? (
          <FrameworkFilter
            frameworks={frameworks}
            selectedFrameworkIds={selectedFrameworkIds}
            onToggle={toggleFrameworkFilter}
            actions={(
              <>
                <button
                  className="ghost-button framework-filter__publish"
                  type="button"
                  ref={publishArtButtonRef}
                  onClick={() => {
                    setCreateDialogOpen(false);
                    setFrameworkDialogOpen(false);
                    setPublishDialogOpen(true);
                  }}
                >
                  发布 Art
                </button>
                <input
                  className="framework-filter__search"
                  type="search"
                  aria-label="搜索 Art"
                  placeholder="搜索 Art"
                  value={storeSearchText}
                  onChange={(event) => setStoreSearchText(event.target.value)}
                />
                <label className={storeOfficialOnly ? "framework-filter__official framework-filter__official--checked" : "framework-filter__official"}>
                  <input
                    type="checkbox"
                    aria-label="只显示官方"
                    checked={storeOfficialOnly}
                    onChange={(event) => setStoreOfficialOnly(event.target.checked)}
                  />
                  <span title="只显示官方">官</span>
                </label>
              </>
            )}
          />
        ) : null}
      </div>

      {frameworkError ? (
        <div className="art-hub__notice" role="alert">
          <span>{frameworkError}</span>
          <button className="ghost-button" type="button" onClick={() => void loadFrameworks()}>
            重试框架状态
          </button>
        </div>
      ) : null}

      {snapshotRefreshError ? (
        <div className="art-hub__notice art-hub__notice--warning" role="alert">
          <span>{snapshotRefreshError}</span>
          <button className="ghost-button" type="button" onClick={() => void synchronizeArtState()}>
            重新同步 Art 状态
          </button>
        </div>
      ) : null}

      <div
        className="art-hub__surface"
        id="art-panel-registry"
        role="tabpanel"
        aria-labelledby="art-tab-registry"
        hidden={activeWorkspace !== "registry"}
      >
        <RegistryPanel
          tools={tools}
          mcpServers={mcpServers}
          workflows={workflows}
          frameworks={frameworks}
          selectedFrameworkIds={selectedFrameworkIds}
          createDialogOpen={createDialogOpen}
          createRequest={createRequest}
          onCloseCreateDialog={closeCreateDialog}
          reloadFrameworks={loadFrameworks}
          baseUrl={baseUrl}
          refresh={refresh}
        />
      </div>
      <div
        className="art-hub__surface"
        id="art-panel-store"
        role="tabpanel"
        aria-labelledby="art-tab-store"
        hidden={activeWorkspace !== "store"}
      >
        <ArtStoreCard
          baseUrl={baseUrl}
          active={activeWorkspace === "store"}
          frameworks={frameworks}
          selectedFrameworkIds={selectedFrameworkIds}
          searchText={storeSearchText}
          officialOnly={storeOfficialOnly}
          refreshToken={storeCatalogRefreshToken}
          onInstalled={synchronizeArtState}
        />
      </div>
      <div
        className="art-hub__surface"
        id="art-panel-security"
        role="tabpanel"
        aria-labelledby="art-tab-security"
        hidden={activeWorkspace !== "security"}
      >
        <PluginSecurityPanel baseUrl={baseUrl} />
      </div>
      <FrameworkManagementDialog
        open={frameworkDialogOpen}
        frameworks={frameworks}
        busyId={frameworkBusyId}
        busyAction={frameworkBusyAction}
        error={frameworkError}
        message={frameworkManagementMessage}
        onClose={closeFrameworkDialog}
        onToggle={toggleFramework}
        onUpgrade={upgradeFramework}
      />
      <ArtPublishDialog
        open={publishDialogOpen}
        tools={tools}
        baseUrl={baseUrl}
        onClose={closePublishDialog}
        onPublished={async () => {
          await synchronizeArtState();
          setStoreCatalogRefreshToken((current) => current + 1);
        }}
      />
    </section>
  );
}

function HookBridgePanel({
  baseUrl,
  hookCanvas,
  hookCanvasError,
  tools,
  onCreateWorkflowArt,
}: {
  baseUrl: string;
  hookCanvas: HookCanvasSnapshot | null;
  hookCanvasError: string | null;
  tools: LoomToolDefinition[];
  onCreateWorkflowArt: (request: WorkflowArtCreationRequest) => void;
}) {
  return (
    <HookCanvasThumbnail
      snapshot={hookCanvas}
      baseUrl={baseUrl}
      error={hookCanvasError}
      tools={tools}
      onCreateWorkflowArt={onCreateWorkflowArt}
    />
  );
}

type SettingsAppId = "loom" | "hook";
type SettingsSectionId = "general" | "shortcuts" | "mcp" | "art-store" | "network" | "cache" | "about";
type SettingsSectionIconKind = SettingsSectionId | "system";

interface ApplicationDiagnosticsInfo {
  app: SettingsAppId;
  appName: string;
  version: string;
  repositoryUrl: string | null;
  commitShort: string | null;
  logDir: string;
  logFile: string | null;
  logFileExists: boolean;
}

interface HookCacheEntryInfo {
  key: string;
  label: string;
  path: string;
  bytes: number;
  fileCount: number;
}

interface HookCacheSnapshotInfo {
  temporary: HookCacheEntryInfo;
  recycleBinEntries: number;
  referenceEntries: number;
}

interface HookCacheClearResult {
  kind: string;
  freedBytes: number;
  snapshot: HookCacheSnapshotInfo;
}

interface LoomCacheSnapshotInfo {
  artRuntime: HookCacheEntryInfo;
  frameworkTemporary: HookCacheEntryInfo;
}

interface LoomCacheClearResult {
  kind: string;
  freedBytes: number;
  snapshot: LoomCacheSnapshotInfo;
}

type HookShortcutGroupIconKind = "capture" | "tools" | "sticker" | "transform";

type HookShortcutContext = "capture-selecting" | "sticker-editing" | "unit-selected" | "canvas";
type HookShortcutGestureAction = "点击" | "拖动" | "滚轮";
type ShortcutSlot = 0 | 1 | 2;
type ShortcutSlots = [string, string, string];

interface HookShortcutDisplayItem {
  id: string;
  label: string;
  description: string;
  keys: string[];
  sourceId?: string;
  contexts: HookShortcutContext[];
  gestureAction?: HookShortcutGestureAction;
  conflictFamily?: string;
}

interface HookShortcutDisplayGroup {
  id: string;
  label: string;
  icon: HookShortcutGroupIconKind;
  items: HookShortcutDisplayItem[];
}

interface ShortcutEditorState {
  item: HookShortcutDisplayItem;
  keys: ShortcutSlots;
  activeSlot: ShortcutSlot;
  slotCount: 1 | 2 | 3;
}

interface QuickBindingEditorState {
  id: string;
  art: string;
  keys: ShortcutSlots;
  activeSlot: ShortcutSlot;
  slotCount: 1 | 2 | 3;
}

const ALL_HOOK_SHORTCUT_CONTEXTS: HookShortcutContext[] = [
  "capture-selecting",
  "sticker-editing",
  "unit-selected",
  "canvas",
];

const HOOK_SHORTCUT_GROUPS: HookShortcutDisplayGroup[] = [
  {
    id: "capture-file",
    label: "捕获与操作",
    icon: "capture",
    items: [
      { id: "capture", sourceId: "capture", label: "截图", description: "截取屏幕区域", keys: ["Ctrl+1"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "long-capture", sourceId: "long_capture", label: "长截图", description: "开始或结束长截图", keys: ["Ctrl+3"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "open-image", sourceId: "open_image", label: "打开图片", description: "导入图片并创建贴图", keys: ["Ctrl+O"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "save", sourceId: "save_image", label: "保存图片", description: "保存当前贴图的正式输出", keys: ["Ctrl+S"], contexts: ["unit-selected"] },
      { id: "toggle-clean-view", sourceId: "toggle_clean_view", label: "清爽视图", description: "显示或隐藏界面辅助控件", keys: ["Ctrl+4"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "cancel", sourceId: "cancel", label: "取消 / 退出", description: "根据当前状态取消、删除或退出", keys: ["Escape", "Delete", "Backspace"], contexts: ["capture-selecting", "sticker-editing", "unit-selected"], conflictFamily: "contextual-cancel-delete" },
      { id: "force-close", label: "强行关闭", description: "连续按下 3 次 Esc 强行退出 Hook", keys: ["Esc × 3"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
    ],
  },
  {
    id: "panels-tools",
    label: "高级工具",
    icon: "tools",
    items: [
      { id: "toggle-actions", sourceId: "toggle_actions", label: "Art 菜单", description: "显示或隐藏添加 Art 面板", keys: ["Shift+1"], contexts: ["unit-selected"] },
      { id: "toggle-params", sourceId: "toggle_params", label: "参数面板", description: "显示或隐藏当前节点参数", keys: ["Tab"], contexts: ["unit-selected"] },
    ],
  },
  {
    id: "sticker-operation",
    label: "贴图操作",
    icon: "sticker",
    items: [
      { id: "copy", sourceId: "copy_unit", label: "复制贴图", description: "复制当前选中的完整贴图", keys: ["Ctrl+C"], contexts: ["unit-selected"] },
      { id: "paste", sourceId: "paste_unit", label: "粘贴贴图", description: "在鼠标位置粘贴完整贴图", keys: ["Ctrl+V"], contexts: ["unit-selected", "canvas"] },
      { id: "delete", sourceId: "delete_unit", label: "删除贴图", description: "删除当前选中的完整贴图", keys: ["Escape", "Delete", "Backspace"], contexts: ["unit-selected"], conflictFamily: "contextual-cancel-delete" },
      { id: "sticker-resize", sourceId: "sticker_resize", label: "调整尺寸", description: "缩放当前贴图的整体尺寸", keys: ["Ctrl+滚轮"], contexts: ["unit-selected"], gestureAction: "滚轮" },
      { id: "sticker-opacity", sourceId: "sticker_opacity", label: "调整透明度", description: "调整当前贴图的整体透明度", keys: ["Alt+滚轮"], contexts: ["unit-selected"], gestureAction: "滚轮" },
      { id: "drag-align", sourceId: "drag_alignment", label: "吸附对齐", description: "拖动贴图时启用吸附与对齐", keys: ["Alt+拖动"], contexts: ["unit-selected"], gestureAction: "拖动" },
      { id: "drag-out", sourceId: "drag_out", label: "拖出文件", description: "将贴图拖出为本地文件", keys: ["Shift+拖动"], contexts: ["unit-selected"], gestureAction: "拖动" },
      { id: "drag-cascade", sourceId: "drag_cascade", label: "层叠放置", description: "拖动贴图时采用层叠放置", keys: ["Ctrl+拖动"], contexts: ["unit-selected"], gestureAction: "拖动" },
    ],
  },
  {
    id: "sticker-edit",
    label: "贴图编辑",
    icon: "transform",
    items: [
      { id: "toggle-sticker-toolbar", sourceId: "toggle_sticker_toolbar", label: "贴图工具栏", description: "显示或隐藏贴图编辑工具栏", keys: ["Ctrl+E"], contexts: ALL_HOOK_SHORTCUT_CONTEXTS },
      { id: "control-copy", sourceId: "copy_unit", label: "复制控件", description: "复制贴图内当前选中的控件", keys: ["Ctrl+C"], contexts: ["sticker-editing"] },
      { id: "control-paste", sourceId: "paste_unit", label: "粘贴控件", description: "粘贴已复制的贴图控件", keys: ["Ctrl+V"], contexts: ["sticker-editing"] },
      { id: "control-delete", sourceId: "delete_unit", label: "删除控件", description: "删除贴图内当前选中的控件", keys: ["Escape", "Delete", "Backspace"], contexts: ["sticker-editing"], conflictFamily: "contextual-cancel-delete" },
      { id: "undo-edit", sourceId: "undo_edit", label: "撤销编辑", description: "撤销上一次控件编辑", keys: ["Ctrl+Z"], contexts: ["sticker-editing"] },
      { id: "redo-edit", sourceId: "redo_edit", label: "重做编辑", description: "恢复上一次撤销的控件编辑", keys: ["Ctrl+Y"], contexts: ["sticker-editing"] },
      { id: "transform-select", sourceId: "transform_select", label: "选择模式", description: "切换到控件选择模式", keys: ["Q"], contexts: ["unit-selected", "sticker-editing"] },
      { id: "transform-move", sourceId: "transform_move", label: "移动模式", description: "切换到控件移动模式", keys: ["W"], contexts: ["unit-selected", "sticker-editing"] },
      { id: "transform-rotate", sourceId: "transform_rotate", label: "旋转模式", description: "切换到控件旋转模式", keys: ["E"], contexts: ["unit-selected", "sticker-editing"] },
      { id: "transform-scale", sourceId: "transform_scale", label: "缩放模式", description: "切换到控件缩放模式", keys: ["R"], contexts: ["unit-selected", "sticker-editing"] },
      { id: "control-multi-select", sourceId: "control_multi_select", label: "多选控件", description: "添加或移除当前控件选择", keys: ["Shift+点击"], contexts: ["sticker-editing"], gestureAction: "点击" },
      { id: "control-quick-move", sourceId: "control_quick_move", label: "快速移动控件", description: "在选择模式下直接移动控件", keys: ["Alt+拖动"], contexts: ["sticker-editing"], gestureAction: "拖动" },
      { id: "control-quick-rotate", sourceId: "control_quick_rotate", label: "快速旋转控件", description: "在选择模式下直接旋转控件", keys: ["Ctrl+拖动"], contexts: ["sticker-editing"], gestureAction: "拖动" },
      { id: "control-scale", sourceId: "control_scale", label: "缩放选中控件", description: "以选中控件组的中心进行缩放", keys: ["Ctrl+Alt+滚轮"], contexts: ["sticker-editing"], gestureAction: "滚轮" },
      { id: "control-scale-own-center", sourceId: "control_scale_own_center", label: "独立中心缩放", description: "分别以每个控件自身中心缩放", keys: ["Ctrl+Alt+Shift+滚轮"], contexts: ["sticker-editing"], gestureAction: "滚轮" },
    ],
  },
];

function HookShortcutGroupIcon({ kind }: { kind: HookShortcutGroupIconKind }) {
  const props = {
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  switch (kind) {
    case "capture":
      return <svg {...props}><path d="M8 4H5a1 1 0 0 0-1 1v3M16 4h3a1 1 0 0 1 1 1v3M8 20H5a1 1 0 0 1-1-1v-3M16 20h3a1 1 0 0 0 1-1v-3" /><rect x="8" y="8" width="8" height="8" rx="1" /></svg>;
    case "tools":
      return <svg {...props}><path d="M4 7h10M18 7h2M4 17h2M10 17h10" /><circle cx="16" cy="7" r="2" /><circle cx="8" cy="17" r="2" /></svg>;
    case "sticker":
      return <svg {...props}><rect x="3" y="4" width="18" height="16" rx="2" /><path d="m6 16 4-4 3 3 2-2 3 3M8 8h.01" /></svg>;
    case "transform":
      return <svg {...props}><path d="M8 3H3v5M16 3h5v5M8 21H3v-5M16 21h5v-5M3 8l6-5M21 8l-6-5M3 16l6 5M21 16l-6 5" /></svg>;
    default:
      return null;
  }
}

const splitShortcutAlternatives = (value: string) => value
  .split(/\s*\/\s*/)
  .map((shortcut) => shortcut.trim())
  .filter(Boolean);

const shortcutKeyParts = (shortcut: string) => shortcut
  .split("+")
  .map((key) => key.trim())
  .filter(Boolean);

const shortcutKeyDisplayLabel = (key: string) => key === "Escape" ? "Esc" : key;

const SHORTCUT_GESTURE_MODIFIERS = ["Ctrl", "Alt", "Shift", "Meta"] as const;
type ShortcutGestureModifier = typeof SHORTCUT_GESTURE_MODIFIERS[number];

const gestureShortcutModifiers = (shortcut: string, action: HookShortcutGestureAction) => new Set(
  shortcutKeyParts(shortcut).filter((part): part is ShortcutGestureModifier => (
    part !== action && SHORTCUT_GESTURE_MODIFIERS.includes(part as ShortcutGestureModifier)
  )),
);

const toggleGestureShortcutModifier = (
  shortcut: string,
  action: HookShortcutGestureAction,
  modifier: ShortcutGestureModifier,
) => {
  const selected = gestureShortcutModifiers(shortcut, action);
  if (selected.has(modifier)) selected.delete(modifier);
  else selected.add(modifier);
  return [
    ...SHORTCUT_GESTURE_MODIFIERS.filter((candidate) => selected.has(candidate)),
    action,
  ].join("+");
};

const shortcutContextsOverlap = (
  left: readonly HookShortcutContext[],
  right: readonly HookShortcutContext[],
) => left.some((context) => right.includes(context));

const shortcutSlotCount = (keys: readonly string[]): 1 | 2 | 3 => (
  keys[2] ? 3 : keys[1] ? 2 : 1
);

const removeShortcutSlot = (keys: ShortcutSlots, slot: ShortcutSlot): ShortcutSlots => {
  if (slot === 0) return ["", keys[1], keys[2]];
  if (slot === 1) return [keys[0], keys[2], ""];
  return [keys[0], keys[1], ""];
};

function ShortcutKeySequence({ shortcuts }: { shortcuts: string[] }) {
  return (
    <span className="hook-shortcut-key-sequences">
      {shortcuts.map((shortcut, shortcutIndex) => (
        <span className="hook-shortcut-key-sequence" key={`${shortcut}-${shortcutIndex}`}>
          {shortcutKeyParts(shortcut).map((key, keyIndex) => (
            <span className="hook-shortcut-key-part" key={`${key}-${keyIndex}`}>
              {keyIndex > 0 ? <span className="hook-shortcut-key-plus" aria-hidden="true">+</span> : null}
              <kbd>{shortcutKeyDisplayLabel(key)}</kbd>
            </span>
          ))}
          {shortcutIndex < shortcuts.length - 1 ? <span className="hook-shortcut-key-or">或</span> : null}
        </span>
      ))}
    </span>
  );
}

const shortcutKeyFromKeyboardEvent = (event: globalThis.KeyboardEvent) => {
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3);
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (/^Numpad[0-9]$/.test(event.code)) return `Num${event.code.slice(6)}`;
  const aliases: Record<string, string> = {
    " ": "Space",
    Control: "Ctrl",
    Meta: "Meta",
  };
  return aliases[event.key] ?? event.key;
};

const shortcutFromKeyboardEvent = (event: globalThis.KeyboardEvent) => {
  const key = shortcutKeyFromKeyboardEvent(event);
  if (["Ctrl", "Alt", "Shift", "Meta"].includes(key)) return null;
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Meta");
  parts.push(key);
  return parts.join("+");
};

const FALLBACK_APPLICATION_DIAGNOSTICS: Record<SettingsAppId, ApplicationDiagnosticsInfo> = {
  loom: {
    app: "loom",
    appName: "Loom",
    version: desktopPackage.version,
    repositoryUrl: "https://github.com/aiaimimi0920/Loom",
    commitShort: null,
    logDir: "",
    logFile: null,
    logFileExists: false,
  },
  hook: {
    app: "hook",
    appName: "Hook",
    version: "0.1.7",
    repositoryUrl: "https://github.com/aiaimimi0920/Hook",
    commitShort: null,
    logDir: "",
    logFile: null,
    logFileExists: false,
  },
};

function SettingsSectionIcon({ kind }: { kind: SettingsSectionIconKind }) {
  const iconProps = {
    className: "settings-section__icon",
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  switch (kind) {
    case "general":
      return <svg {...iconProps}><path d="M4 7h10M18 7h2M4 17h2M10 17h10" /><circle cx="16" cy="7" r="2" /><circle cx="8" cy="17" r="2" /></svg>;
    case "shortcuts":
      return <svg {...iconProps}><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M7 9h.01M11 9h.01M15 9h.01M7 13h.01M11 13h6M7 16h10" /></svg>;
    case "mcp":
      return <svg {...iconProps}><rect x="3" y="5" width="7" height="5" rx="1" /><rect x="14" y="14" width="7" height="5" rx="1" /><path d="M10 7.5h4a3 3 0 0 1 3 3V14M14 16.5h-4a3 3 0 0 1-3-3V10" /></svg>;
    case "art-store":
      return <svg {...iconProps}><path d="M4 9h16l-1-4H5L4 9Z" /><path d="M5 9v10h14V9M9 19v-6h6v6" /><path d="M4 9c0 2 3 2 4 0 1 2 3 2 4 0 1 2 3 2 4 0 1 2 4 2 4 0" /></svg>;
    case "system":
      return <svg {...iconProps}><path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.6 5.6l2.8 2.8M15.6 15.6l2.8 2.8M18.4 5.6l-2.8 2.8M8.4 15.6l-2.8 2.8" /><circle cx="12" cy="12" r="3" /></svg>;
    case "network":
      return <svg {...iconProps}><circle cx="12" cy="12" r="9" /><path d="M3.5 9h17M3.5 15h17M12 3c2.2 2.4 3.3 5.4 3.3 9S14.2 18.6 12 21M12 3C9.8 5.4 8.7 8.4 8.7 12s1.1 6.6 3.3 9" /></svg>;
    case "cache":
      return <svg {...iconProps}><path d="M4 7h16v10H4zM7 10h10M7 14h6" /></svg>;
    case "about":
      return <svg {...iconProps}><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" /></svg>;
    default:
      return null;
  }
}

function SettingsAccordionSection({
  id,
  label,
  open,
  onToggle,
  children,
}: {
  id: SettingsSectionId;
  label: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  const contentId = `settings-section-${id}`;
  return (
    <section className={open ? "settings-section settings-section--open" : "settings-section"}>
      <h2 className="settings-section__heading">
        <button
          className="settings-section__trigger"
          type="button"
          aria-expanded={open}
          aria-controls={contentId}
          onClick={onToggle}
        >
          <SettingsSectionIcon kind={id} />
          <span>{label}</span>
          <svg className="settings-section__chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
            <path d="m7 9 5 5 5-5" />
          </svg>
        </button>
      </h2>
      {open ? <div className="settings-section__body" id={contentId}>{children}</div> : null}
    </section>
  );
}

interface GeneralSettingsValue {
  language: string;
  theme: string;
  closeToTray: boolean;
}

function GeneralSettingsPanel({
  appName,
  value,
  onChange,
}: {
  appName: SettingsAppId;
  value: GeneralSettingsValue;
  onChange: (patch: Partial<GeneralSettingsValue>) => void;
}) {
  const applicationName = appName === "loom" ? "Loom" : "Hook";
  return (
    <section className="settings-general-panel" aria-label={`${applicationName} 常规设置`}>
      <header className="settings-general-panel__header">
        <SettingsSectionIcon kind="general" />
        <strong>应用设置</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>语言</strong>
          <span>界面显示语言</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label={`${applicationName} 语言`}
          value={value.language}
          onChange={(event) => onChange({ language: event.target.value })}
        >
          <option value="zh-Hans">简体中文</option>
          <option value="en">English</option>
        </select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>主题</strong>
          <span>界面明暗模式</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label={`${applicationName} 主题`}
          value={value.theme}
          onChange={(event) => onChange({ theme: event.target.value })}
        >
          <option value="system">跟随系统</option>
          <option value="dark">深色</option>
          <option value="light">浅色</option>
        </select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>关闭到系统托盘</strong>
          <span>关闭窗口后继续在后台运行</span>
        </div>
        <label className="settings-general-toggle">
          <input
            type="checkbox"
            aria-label={`${applicationName} 关闭到系统托盘`}
            checked={value.closeToTray}
            onChange={(event) => onChange({ closeToTray: event.target.checked })}
          />
          <span>{value.closeToTray ? "已开启" : "已关闭"}</span>
        </label>
      </div>
    </section>
  );
}

function McpSettingsPanel({
  value,
  onChange,
}: {
  value: LoomMcpSettings;
  onChange: (patch: Partial<LoomMcpSettings>) => void;
}) {
  return (
    <section className="settings-mcp-panel" aria-label="MCP 设置">
      <header className="settings-mcp-panel__header">
        <SettingsSectionIcon kind="mcp" />
        <strong>运行限制</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>请求超时</strong>
          <span>单次 MCP 初始化、工具列表或调用的最长等待时间</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label="MCP 请求超时"
          value={value.request_timeout_seconds}
          onChange={(event) => onChange({ request_timeout_seconds: Number(event.target.value) })}
        >
          {[15, 30, 60, 120, 300].map((seconds) => (
            <option key={seconds} value={seconds}>{seconds < 60 ? `${seconds} 秒` : `${seconds / 60} 分钟`}</option>
          ))}
        </select>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>子进程内存上限</strong>
          <span>限制每个由 Loom 启动的 MCP 服务进程</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label="MCP 子进程内存上限"
          value={value.memory_limit_bytes}
          onChange={(event) => onChange({ memory_limit_bytes: Number(event.target.value) })}
        >
          {[
            { value: 256 * 1024 * 1024, label: "256 MB" },
            { value: 512 * 1024 * 1024, label: "512 MB" },
            { value: 1024 * 1024 * 1024, label: "1 GB" },
            { value: 2 * 1024 * 1024 * 1024, label: "2 GB" },
          ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
        </select>
      </div>
    </section>
  );
}

function ArtStoreSettingsPanel({
  value,
  trustPolicy,
  trustPolicyBusy,
  onChange,
  onTrustPolicyChange,
}: {
  value: LoomArtStoreSettings;
  trustPolicy: LoomPluginTrustPolicy;
  trustPolicyBusy: boolean;
  onChange: (patch: Partial<LoomArtStoreSettings>) => void;
  onTrustPolicyChange: (policy: LoomPluginTrustPolicy) => void;
}) {
  return (
    <section className="settings-art-store-panel" aria-label="Art 设置">
      <header className="settings-art-store-panel__header">
        <SettingsSectionIcon kind="art-store" />
        <strong>更新与安装</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>自动更新</strong>
          <span>启动 Art 页面时更新已启用自动更新的 Art</span>
        </div>
        <label className="settings-general-toggle">
          <input type="checkbox" aria-label="Art 自动更新" checked={value.auto_update} onChange={(event) => onChange({ auto_update: event.target.checked })} />
          <span>{value.auto_update ? "已开启" : "已关闭"}</span>
        </label>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>默认只显示官方</strong>
          <span>进入商店时优先隐藏未经官方认证的 Art</span>
        </div>
        <label className="settings-general-toggle">
          <input type="checkbox" aria-label="Art 默认只显示官方" checked={value.official_only} onChange={(event) => onChange({ official_only: event.target.checked })} />
          <span>{value.official_only ? "已开启" : "已关闭"}</span>
        </label>
      </div>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>安装策略</strong>
          <span>决定 Loom 可以安装何种签名状态的 Art</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label="Art 安装策略"
          value={trustPolicy}
          disabled={trustPolicyBusy}
          onChange={(event) => onTrustPolicyChange(event.target.value as LoomPluginTrustPolicy)}
        >
          <option value="require_trusted">仅可信发布者</option>
          <option value="require_signed">需要有效签名</option>
          <option value="allow_unsigned">允许未签名</option>
        </select>
      </div>
    </section>
  );
}

function NetworkSettingsPanel({
  appName,
  value,
  onChange,
}: {
  appName: string;
  value: LoomProxySettings;
  onChange: (patch: Partial<LoomProxySettings>) => void;
}) {
  return (
    <section className="settings-network-panel" aria-label={`${appName} 网络设置`}>
      <header className="settings-network-panel__header">
        <SettingsSectionIcon kind="network" />
        <strong>代理设置</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>代理模式</strong>
          <span>选择网络请求使用的代理方式</span>
        </div>
        <select
          className="studio-input settings-network-row__control"
          aria-label={`${appName} 代理模式`}
          value={value.mode}
          onChange={(event) => onChange({ mode: event.target.value as LoomProxySettings["mode"] })}
        >
          <option value="system">跟随系统</option>
          <option value="custom">自定义</option>
          <option value="disabled">不使用代理</option>
        </select>
      </div>
      {value.mode === "custom" ? (
        <div className="settings-network-row">
          <div className="settings-network-row__label">
            <strong>代理地址</strong>
            <span>填写 host:port，例如 127.0.0.1:7890</span>
          </div>
          <div className="settings-network-address">
            <select
              className="studio-input"
              aria-label={`${appName} 代理协议`}
              value={value.protocol}
              onChange={(event) => onChange({ protocol: event.target.value as LoomProxySettings["protocol"] })}
            >
              <option value="http">http://</option>
              <option value="https">https://</option>
              <option value="socks5">socks5://</option>
            </select>
            <input
              className="studio-input"
              aria-label={`${appName} 代理地址`}
              value={value.address}
              placeholder="127.0.0.1:7890"
              spellCheck={false}
              onChange={(event) => onChange({ address: event.target.value })}
            />
          </div>
        </div>
      ) : null}
    </section>
  );
}

function formatCacheBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / (1024 ** unitIndex);
  return `${value >= 10 || unitIndex === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}

function loomCacheSettingsForUi(value?: Partial<LoomCacheSettings>): LoomCacheSettings {
  const merged = { ...DEFAULT_LOOM_SETTINGS.loom_cache, ...value };
  return {
    art_cache_max_bytes: [0, 256 * 1024 * 1024, 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024]
      .includes(merged.art_cache_max_bytes)
      ? merged.art_cache_max_bytes
      : 1024 * 1024 * 1024,
    art_cache_retention_days: [0, 3, 7, 30].includes(merged.art_cache_retention_days)
      ? merged.art_cache_retention_days
      : 30,
    framework_temp_retention_days: [0, 1, 3, 7, 30]
      .includes(merged.framework_temp_retention_days)
      ? merged.framework_temp_retention_days
      : 3,
  };
}

function loomCachePreferencesForRuntime(settings: LoomCacheSettings) {
  return {
    artCacheMaxBytes: settings.art_cache_max_bytes,
    artCacheRetentionDays: settings.art_cache_retention_days,
    frameworkTempRetentionDays: settings.framework_temp_retention_days,
  };
}

function LoomCacheSettingsPanel({
  settings,
  snapshot,
  loading,
  busyKind,
  onSettingsChange,
  onClear,
}: {
  settings: LoomCacheSettings;
  snapshot: LoomCacheSnapshotInfo | null;
  loading: boolean;
  busyKind: string | null;
  onSettingsChange: (patch: Partial<LoomCacheSettings>) => void;
  onClear: (kind: "artRuntime" | "frameworkTemporary") => void;
}) {
  const retentionLabel = (days: number) => days === 0 ? "无限" : `${days} 天`;
  return (
    <div className="hook-cache-settings loom-cache-settings" aria-busy={loading || Boolean(busyKind)}>
      <section className="hook-cache-group" aria-labelledby="loom-cache-policy-title">
        <header className="hook-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="13" r="7" /><path d="M12 9v4l2.5 1.5M9 3h6M12 3v3" /></svg>
          <strong id="loom-cache-policy-title">缓存规则</strong>
        </header>
        <div className="hook-cache-row">
          <span><strong>Art 运行缓存上限</strong><small>限制已安装 Art 在运行时生成的可重建缓存</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="Art 运行缓存上限" value={settings.art_cache_max_bytes} onChange={(event) => onSettingsChange({ art_cache_max_bytes: Number(event.target.value) })}>
            {[
              { value: 256 * 1024 * 1024, label: "256 MB" },
              { value: 1024 * 1024 * 1024, label: "1 GB" },
              { value: 4 * 1024 * 1024 * 1024, label: "4 GB" },
              { value: 0, label: "无限制" },
            ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>Art 运行缓存自动清理周期</strong><small>按最后修改时间移除可重新生成的缓存文件</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="Art 运行缓存自动清理周期" value={settings.art_cache_retention_days} onChange={(event) => onSettingsChange({ art_cache_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>框架临时文件自动清理周期</strong><small>清理已经结束的框架执行残留，不影响已安装框架</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="框架临时文件自动清理周期" value={settings.framework_temp_retention_days} onChange={(event) => onSettingsChange({ framework_temp_retention_days: Number(event.target.value) })}>
            {[1, 3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </select>
        </div>
      </section>

      <section className="hook-cache-group" aria-labelledby="loom-cache-clean-title">
        <header className="hook-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 18 8-8 4 4-8 8H4v-4ZM13.5 8.5l2-2 2 2-2 2M15.5 6.5l2-2 2 2-2 2" /></svg>
          <strong id="loom-cache-clean-title">手动清理</strong>
        </header>
        <div className="hook-cache-row hook-cache-row--action" title={snapshot?.artRuntime.path}>
          <span><strong>Art 运行缓存</strong><small>{snapshot ? `${snapshot.artRuntime.fileCount} 个文件` : "正在统计缓存"}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.artRuntime.bytes) : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("artRuntime")}>{busyKind === "artRuntime" ? "清理中..." : "清空"}</button>
        </div>
        <div className="hook-cache-row hook-cache-row--action" title={snapshot?.frameworkTemporary.path}>
          <span><strong>框架临时文件</strong><small>{snapshot ? `${snapshot.frameworkTemporary.fileCount} 个文件` : "正在统计缓存"}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.frameworkTemporary.bytes) : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("frameworkTemporary")}>{busyKind === "frameworkTemporary" ? "清理中..." : "清空"}</button>
        </div>
      </section>
    </div>
  );
}

function hookCacheSettingsForUi(value?: Partial<HookCacheSettings>): HookCacheSettings {
  const merged = { ...DEFAULT_LOOM_SETTINGS.hook_cache, ...value };
  return {
    recycle_bin_max_entries: [0, 15, 50].includes(merged.recycle_bin_max_entries)
      ? merged.recycle_bin_max_entries
      : 15,
    recycle_bin_retention_days: [0, 3, 7, 30].includes(merged.recycle_bin_retention_days)
      ? merged.recycle_bin_retention_days
      : 7,
    temp_cache_max_bytes: [0, 128 * 1024 * 1024, 256 * 1024 * 1024, 1024 * 1024 * 1024]
      .includes(merged.temp_cache_max_bytes)
      ? merged.temp_cache_max_bytes
      : 256 * 1024 * 1024,
    temp_cache_retention_days: [0, 3, 7, 30].includes(merged.temp_cache_retention_days)
      ? merged.temp_cache_retention_days
      : 7,
  };
}

function hookCachePreferencesForRuntime(settings: HookCacheSettings) {
  return {
    recycleBinMaxEntries: settings.recycle_bin_max_entries,
    recycleBinRetentionDays: settings.recycle_bin_retention_days,
    tempCacheMaxBytes: settings.temp_cache_max_bytes,
    tempCacheRetentionDays: settings.temp_cache_retention_days,
  };
}

function HookCacheSettingsPanel({
  settings,
  snapshot,
  loading,
  busyKind,
  onSettingsChange,
  onClear,
}: {
  settings: HookCacheSettings;
  snapshot: HookCacheSnapshotInfo | null;
  loading: boolean;
  busyKind: string | null;
  onSettingsChange: (patch: Partial<HookCacheSettings>) => void;
  onClear: (kind: "recycleBin" | "temporary" | "referenceLibrary") => void;
}) {
  const retentionLabel = (days: number) => days === 0 ? "无限" : `${days} 天`;
  return (
    <div className="hook-cache-settings" aria-busy={loading || Boolean(busyKind)}>
      <section className="hook-cache-group" aria-labelledby="hook-cache-policy-title">
        <header className="hook-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="13" r="7" /><path d="M12 9v4l2.5 1.5M9 3h6M12 3v3" /></svg>
          <strong id="hook-cache-policy-title">缓存规则</strong>
        </header>
        <div className="hook-cache-row">
          <span><strong>回收站上限</strong><small>超过上限时优先移除最早删除的贴图</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="回收站上限" value={settings.recycle_bin_max_entries} onChange={(event) => onSettingsChange({ recycle_bin_max_entries: Number(event.target.value) })}>
            {[15, 50, 0].map((value) => <option key={value} value={value}>{value === 0 ? "无限" : `${value} 项`}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>回收站自动清理周期</strong><small>按删除时间清理过期贴图</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="回收站自动清理周期" value={settings.recycle_bin_retention_days} onChange={(event) => onSettingsChange({ recycle_bin_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>临时缓存上限</strong><small>用于截图、拖放和图像处理中转文件</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="临时缓存上限" value={settings.temp_cache_max_bytes} onChange={(event) => onSettingsChange({ temp_cache_max_bytes: Number(event.target.value) })}>
            {[
              { value: 128 * 1024 * 1024, label: "128 MB" },
              { value: 256 * 1024 * 1024, label: "256 MB" },
              { value: 1024 * 1024 * 1024, label: "1 GB" },
              { value: 0, label: "无限制" },
            ].map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </div>
        <div className="hook-cache-row">
          <span><strong>临时缓存自动清理周期</strong><small>移除超过保留时间的临时文件</small></span>
          <select className="studio-input hook-cache-row__control" aria-label="临时缓存自动清理周期" value={settings.temp_cache_retention_days} onChange={(event) => onSettingsChange({ temp_cache_retention_days: Number(event.target.value) })}>
            {[3, 7, 30, 0].map((value) => <option key={value} value={value}>{retentionLabel(value)}</option>)}
          </select>
        </div>
      </section>

      <section className="hook-cache-group" aria-labelledby="hook-cache-clean-title">
        <header className="hook-cache-group__header">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 18 8-8 4 4-8 8H4v-4ZM13.5 8.5l2-2 2 2-2 2M15.5 6.5l2-2 2 2-2 2" /></svg>
          <strong id="hook-cache-clean-title">手动清理</strong>
        </header>
        <div className="hook-cache-row hook-cache-row--action">
          <span><strong>清空回收站</strong><small>永久移除回收站中的贴图记录</small></span>
          <b>{snapshot ? `${snapshot.recycleBinEntries} 项` : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("recycleBin")}>{busyKind === "recycleBin" ? "清理中..." : "清空"}</button>
        </div>
        <div className="hook-cache-row hook-cache-row--action" title={snapshot?.temporary.path}>
          <span><strong>清空临时缓存</strong><small>{snapshot ? `${snapshot.temporary.fileCount} 个文件` : "正在统计缓存"}</small></span>
          <b>{snapshot ? formatCacheBytes(snapshot.temporary.bytes) : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("temporary")}>{busyKind === "temporary" ? "清理中..." : "清空"}</button>
        </div>
        <div className="hook-cache-row hook-cache-row--action">
          <span><strong>清空参考图</strong><small>移除参考列表，不删除桌面上的贴图</small></span>
          <b>{snapshot ? `${snapshot.referenceEntries} 项` : "统计中..."}</b>
          <button className="ghost-button" type="button" disabled={Boolean(busyKind)} onClick={() => onClear("referenceLibrary")}>{busyKind === "referenceLibrary" ? "清理中..." : "清空"}</button>
        </div>
      </section>
    </div>
  );
}

function SettingsPanel({ snapshot }: { snapshot: LoomSnapshot }) {
  const [draft, setDraft] = useState<LoomSettings>(DEFAULT_LOOM_SETTINGS);
  const [shortcuts, setShortcuts] = useState<LoomShortcutConfig[]>(
    Object.values(DEFAULT_LOOM_SETTINGS.shortcuts),
  );
  const [appPaths, setAppPaths] = useState<LoomAppPaths | null>(null);
  const [appDiagnostics, setAppDiagnostics] = useState<Record<SettingsAppId, ApplicationDiagnosticsInfo>>(
    FALLBACK_APPLICATION_DIAGNOSTICS,
  );
  const [loomCacheSnapshot, setLoomCacheSnapshot] = useState<LoomCacheSnapshotInfo | null>(null);
  const [loomCacheLoading, setLoomCacheLoading] = useState(false);
  const [loomCacheBusyKind, setLoomCacheBusyKind] = useState<string | null>(null);
  const [hookCacheSnapshot, setHookCacheSnapshot] = useState<HookCacheSnapshotInfo | null>(null);
  const [hookCacheLoading, setHookCacheLoading] = useState(false);
  const [hookCacheBusyKind, setHookCacheBusyKind] = useState<string | null>(null);
  const [artStoreTrustPolicy, setArtStoreTrustPolicy] = useState<LoomPluginTrustPolicy>("allow_unsigned");
  const [artStoreTrustPolicyBusy, setArtStoreTrustPolicyBusy] = useState(false);
  const [activeSettingsApp, setActiveSettingsApp] = useState<SettingsAppId>("loom");
  const [openSettingsSection, setOpenSettingsSection] = useState<SettingsSectionId | null>(null);
  const [openShortcutGroups, setOpenShortcutGroups] = useState<Set<string>>(() => new Set());
  const [shortcutEditor, setShortcutEditor] = useState<ShortcutEditorState | null>(null);
  const [quickBindingEditor, setQuickBindingEditor] = useState<QuickBindingEditorState | null>(null);
  const settingsHydratedRef = useRef(false);
  const suppressNextSettingsSaveRef = useRef(false);
  const settingsSaveTimerRef = useRef<number | null>(null);
  const settingsSaveActiveRef = useRef(false);
  const settingsMountedRef = useRef(true);
  const pendingSettingsRef = useRef<LoomSettings | null>(null);
  const lastSavedSettingsRef = useRef<LoomSettings>(DEFAULT_LOOM_SETTINGS);
  const settingsBaseUrlRef = useRef(snapshot.baseUrl);
  const shortcutsRef = useRef(shortcuts);
  const availableArtTools = useMemo(
    () => snapshot.tools
      .filter((tool) => tool.enabled !== false)
      .sort((left, right) => (left.name || left.id).localeCompare(right.name || right.id, "zh-CN")),
    [snapshot.tools],
  );
  const availableArtToolById = useMemo(
    () => new Map(availableArtTools.map((tool) => [tool.id, tool])),
    [availableArtTools],
  );

  const flushSettingsQueue = useCallback(async () => {
    if (settingsSaveActiveRef.current) return;
    settingsSaveActiveRef.current = true;
    while (pendingSettingsRef.current) {
      const nextSettings = pendingSettingsRef.current;
      const baseUrl = settingsBaseUrlRef.current;
      pendingSettingsRef.current = null;
      try {
        const loomGeneralChanged = JSON.stringify(nextSettings.general)
          !== JSON.stringify(lastSavedSettingsRef.current.general);
        const loomCacheChanged = JSON.stringify(nextSettings.loom_cache)
          !== JSON.stringify(lastSavedSettingsRef.current.loom_cache);
        const hookCacheChanged = JSON.stringify(nextSettings.hook_cache)
          !== JSON.stringify(lastSavedSettingsRef.current.hook_cache);
        const saved = await saveLoomSettings(baseUrl, nextSettings);
        if (settingsBaseUrlRef.current === baseUrl) {
          lastSavedSettingsRef.current = saved;
        }
        if (loomGeneralChanged) {
          try {
            await invoke("apply_loom_general_settings", {
              settings: { minimizeToTray: saved.general.minimize_to_tray },
            });
          } catch (error) {
            pushAppToast({
              level: "warning",
              text: error instanceof Error ? error.message : String(error),
            });
          }
        }
        if (loomCacheChanged) {
          try {
            setLoomCacheSnapshot(await invoke<LoomCacheSnapshotInfo>("apply_loom_cache_settings", {
              settings: loomCachePreferencesForRuntime(saved.loom_cache),
            }));
          } catch (error) {
            pushAppToast({
              level: "warning",
              text: error instanceof Error ? error.message : String(error),
            });
          }
        }
        if (hookCacheChanged) {
          try {
            await invoke("wait_for_hook_cache_settings", {
              settings: hookCachePreferencesForRuntime(saved.hook_cache),
            });
          } catch (error) {
            pushAppToast({
              level: "warning",
              text: error instanceof Error ? error.message : String(error),
            });
          }
        }
      } catch (error) {
        if (settingsBaseUrlRef.current === baseUrl) {
          pendingSettingsRef.current = null;
          if (settingsMountedRef.current) {
            suppressNextSettingsSaveRef.current = true;
            const rollback = lastSavedSettingsRef.current;
            const rollbackShortcuts = Object.values(rollback.shortcuts);
            shortcutsRef.current = rollbackShortcuts;
            setDraft(rollback);
            setShortcuts(rollbackShortcuts);
          }
          pushAppToast({
            level: "error",
            text: error instanceof Error ? error.message : "设置自动保存失败",
          });
        }
        break;
      }
    }
    settingsSaveActiveRef.current = false;
  }, []);

  const refreshLoomCache = useCallback(async () => {
    setLoomCacheLoading(true);
    try {
      setLoomCacheSnapshot(await invoke<LoomCacheSnapshotInfo>("get_loom_cache_snapshot"));
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setLoomCacheLoading(false);
    }
  }, []);

  const refreshHookCache = useCallback(async () => {
    setHookCacheLoading(true);
    try {
      setHookCacheSnapshot(await invoke<HookCacheSnapshotInfo>("get_hook_cache_snapshot"));
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setHookCacheLoading(false);
    }
  }, []);

  useEffect(() => {
    applyLoomGeneralSettings(draft.general);
  }, [draft.general.language, draft.general.minimize_to_tray, draft.general.theme]);

  useEffect(() => {
    let cancelled = false;
    settingsBaseUrlRef.current = snapshot.baseUrl;
    settingsHydratedRef.current = false;
    pendingSettingsRef.current = null;
    if (settingsSaveTimerRef.current !== null) {
      window.clearTimeout(settingsSaveTimerRef.current);
      settingsSaveTimerRef.current = null;
    }
    const loadSettings = async () => {
      try {
        const [loadedSettings, loadedShortcuts, loadedPaths] = await Promise.all([
          getLoomSettings(snapshot.baseUrl),
          getLoomShortcuts(snapshot.baseUrl),
          getLoomAppPaths(snapshot.baseUrl),
        ]);
        if (cancelled) return;
        const nextShortcuts = loadedShortcuts.length
          ? loadedShortcuts
          : Object.values(DEFAULT_LOOM_SETTINGS.shortcuts);
        const hydratedSettings = {
          ...loadedSettings,
          general: {
            ...DEFAULT_LOOM_SETTINGS.general,
            ...loadedSettings.general,
          },
          hook_general: {
            ...DEFAULT_LOOM_SETTINGS.hook_general,
            ...loadedSettings.hook_general,
          },
          system: {
            ...DEFAULT_LOOM_SETTINGS.system,
            ...loadedSettings.system,
          },
          network: {
            loom: {
              ...DEFAULT_LOOM_SETTINGS.network.loom,
              ...loadedSettings.network?.loom,
            },
            hook: {
              ...DEFAULT_LOOM_SETTINGS.network.hook,
              ...loadedSettings.network?.hook,
            },
          },
          mcp: {
            ...DEFAULT_LOOM_SETTINGS.mcp,
            ...loadedSettings.mcp,
          },
          art_store: {
            ...DEFAULT_LOOM_SETTINGS.art_store,
            ...loadedSettings.art_store,
          },
          loom_cache: loomCacheSettingsForUi(loadedSettings.loom_cache),
          hook_cache: hookCacheSettingsForUi(loadedSettings.hook_cache),
          shortcuts: Object.fromEntries(nextShortcuts.map((shortcut) => [shortcut.id, shortcut])),
        };
        lastSavedSettingsRef.current = hydratedSettings;
        shortcutsRef.current = nextShortcuts;
        suppressNextSettingsSaveRef.current = true;
        setDraft(hydratedSettings);
        setShortcuts(nextShortcuts);
        setAppPaths(loadedPaths);
        settingsHydratedRef.current = true;
      } catch (error) {
        if (cancelled) return;
        const fallbackShortcuts = Object.values(DEFAULT_LOOM_SETTINGS.shortcuts);
        lastSavedSettingsRef.current = DEFAULT_LOOM_SETTINGS;
        shortcutsRef.current = fallbackShortcuts;
        suppressNextSettingsSaveRef.current = true;
        setDraft(DEFAULT_LOOM_SETTINGS);
        setShortcuts(fallbackShortcuts);
        settingsHydratedRef.current = true;
        pushAppToast({
          level: "error",
          text: error instanceof Error
            ? `使用 Loom 默认设置：${error.message}`
            : "使用 Loom 默认设置。",
        });
      }
    };
    void loadSettings();
    return () => {
      cancelled = true;
    };
  }, [snapshot.baseUrl]);

  useEffect(() => {
    let cancelled = false;
    const loadDiagnostics = async () => {
      const results = await Promise.allSettled(
        (["loom", "hook"] as const).map((app) => invoke<ApplicationDiagnosticsInfo>(
          "resolve_application_diagnostics",
          { app },
        )),
      );
      if (cancelled) return;
      setAppDiagnostics((current) => {
        const next = { ...current };
        results.forEach((result) => {
          if (result.status === "fulfilled") next[result.value.app] = result.value;
        });
        return next;
      });
    };
    void loadDiagnostics();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (openSettingsSection !== "cache") return;
    if (activeSettingsApp === "loom") {
      void refreshLoomCache();
    } else {
      void refreshHookCache();
    }
  }, [activeSettingsApp, openSettingsSection, refreshHookCache, refreshLoomCache]);

  useEffect(() => {
    if (activeSettingsApp !== "loom" || openSettingsSection !== "art-store") return;
    let cancelled = false;
    void listPluginTrust(snapshot.baseUrl)
      .then((trustStore) => {
        if (!cancelled) setArtStoreTrustPolicy(trustStore.policy);
      })
      .catch((error) => {
        if (!cancelled) {
          pushAppToast({
            level: "error",
            text: error instanceof Error ? error.message : String(error),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [activeSettingsApp, openSettingsSection, snapshot.baseUrl]);

  useEffect(() => {
    if (!settingsHydratedRef.current) return;
    if (suppressNextSettingsSaveRef.current) {
      suppressNextSettingsSaveRef.current = false;
      return;
    }
    pendingSettingsRef.current = draft;
    if (settingsSaveTimerRef.current !== null) {
      window.clearTimeout(settingsSaveTimerRef.current);
    }
    settingsSaveTimerRef.current = window.setTimeout(() => {
      settingsSaveTimerRef.current = null;
      void flushSettingsQueue();
    }, 360);
    return () => {
      if (settingsSaveTimerRef.current !== null) {
        window.clearTimeout(settingsSaveTimerRef.current);
        settingsSaveTimerRef.current = null;
      }
    };
  }, [draft, flushSettingsQueue]);

  useEffect(() => {
    settingsMountedRef.current = true;
    return () => {
      settingsMountedRef.current = false;
      if (settingsSaveTimerRef.current !== null) {
        window.clearTimeout(settingsSaveTimerRef.current);
      }
      void flushSettingsQueue();
    };
  }, [flushSettingsQueue]);

  const updateShortcutDraft = (id: string, label: string, keys: string) => {
    const existing = shortcutsRef.current.find((shortcut) => shortcut.id === id);
    const updated: LoomShortcutConfig = {
      id,
      label: existing?.label || label,
      keys,
      enabled: existing?.enabled ?? true,
    };
    const nextShortcuts = existing
      ? shortcutsRef.current.map((shortcut) => shortcut.id === id ? updated : shortcut)
      : [...shortcutsRef.current, updated];
    shortcutsRef.current = nextShortcuts;
    setShortcuts(nextShortcuts);
    setDraft((current) => ({
      ...current,
      shortcuts: Object.fromEntries(nextShortcuts.map((shortcut) => [shortcut.id, shortcut])),
    }));
  };

  const updateNetworkDraft = (app: SettingsAppId, patch: Partial<LoomProxySettings>) => {
    setDraft((current) => ({
      ...current,
      network: {
        ...current.network,
        [app]: { ...current.network[app], ...patch },
      },
    }));
  };

  const updateHookGeneralDraft = (patch: Partial<GeneralSettingsValue>) => {
    setDraft((current) => ({
      ...current,
      hook_general: {
        ...current.hook_general,
        ...(patch.language === undefined ? {} : { language: patch.language }),
        ...(patch.theme === undefined ? {} : { theme: patch.theme }),
        ...(patch.closeToTray === undefined ? {} : { close_to_tray: patch.closeToTray }),
      },
    }));
  };

  const updateLoomCacheDraft = (patch: Partial<LoomCacheSettings>) => {
    setDraft((current) => ({
      ...current,
      loom_cache: { ...current.loom_cache, ...patch },
    }));
  };

  const updateMcpDraft = (patch: Partial<LoomMcpSettings>) => {
    setDraft((current) => ({
      ...current,
      mcp: { ...current.mcp, ...patch },
    }));
  };

  const updateArtStoreDraft = (patch: Partial<LoomArtStoreSettings>) => {
    setDraft((current) => ({
      ...current,
      art_store: { ...current.art_store, ...patch },
    }));
  };

  const updateArtStoreTrustPolicy = async (policy: LoomPluginTrustPolicy) => {
    const previous = artStoreTrustPolicy;
    setArtStoreTrustPolicy(policy);
    setArtStoreTrustPolicyBusy(true);
    try {
      const trustStore = await setPluginTrustPolicy(snapshot.baseUrl, policy);
      setArtStoreTrustPolicy(trustStore.policy);
      pushAppToast({ level: "info", text: "Art 安装策略已更新" });
    } catch (error) {
      setArtStoreTrustPolicy(previous);
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setArtStoreTrustPolicyBusy(false);
    }
  };

  const updateHookCacheDraft = (patch: Partial<HookCacheSettings>) => {
    setDraft((current) => ({
      ...current,
      hook_cache: { ...current.hook_cache, ...patch },
    }));
  };

  const clearLoomCache = async (kind: "artRuntime" | "frameworkTemporary") => {
    const label = kind === "artRuntime" ? "Art 运行缓存" : "框架临时文件";
    const accepted = await requestAppConfirmation({
      title: `清空${label}`,
      message: kind === "artRuntime"
        ? "将删除 Art 生成的可重建运行缓存，不会卸载 Art 或删除工作流。"
        : "将删除框架执行产生的临时文件。请先等待正在运行的 Art 完成。",
      confirmLabel: "清空",
      tone: "warning",
    });
    if (!accepted) return;
    setLoomCacheBusyKind(kind);
    try {
      const result = await invoke<LoomCacheClearResult>("clear_loom_cache", { kind });
      setLoomCacheSnapshot(result.snapshot);
      pushAppToast({
        level: "info",
        text: `已清空${label}，释放 ${formatCacheBytes(result.freedBytes)}`,
      });
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setLoomCacheBusyKind(null);
    }
  };

  const clearHookCache = async (kind: "recycleBin" | "temporary" | "referenceLibrary") => {
    const labels = {
      recycleBin: "回收站",
      temporary: "临时缓存",
      referenceLibrary: "参考图",
    } as const;
    const accepted = await requestAppConfirmation({
      title: `清空${labels[kind]}`,
      message: kind === "referenceLibrary"
        ? "将移除 Hook 参考列表中的全部记录，桌面贴图不会被删除。"
        : kind === "recycleBin"
          ? "回收站中的贴图记录将被永久移除。"
          : "将移除 Hook 的临时中转文件，后续需要时会自动重新生成。",
      confirmLabel: "清空",
      tone: kind === "recycleBin" ? "danger" : "warning",
    });
    if (!accepted) return;
    setHookCacheBusyKind(kind);
    try {
      const result = await invoke<HookCacheClearResult>("clear_hook_cache", { kind });
      setHookCacheSnapshot(result.snapshot);
      pushAppToast({
        level: "info",
        text: kind === "temporary"
          ? `已清空临时缓存，释放 ${formatCacheBytes(result.freedBytes)}`
          : `已清空${labels[kind]}`,
      });
      if (kind !== "temporary") {
        window.setTimeout(() => void refreshHookCache(), 500);
      }
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setHookCacheBusyKind(null);
    }
  };

  const toggleMinimizeToTray = (enabled = !draft.general.minimize_to_tray) => {
    setDraft((current) => ({ ...current, general: { ...current.general, minimize_to_tray: enabled } }));
  };

  const toggleSettingsSection = (section: SettingsSectionId) => {
    setOpenSettingsSection((current) => current === section ? null : section);
  };

  const selectSettingsApp = (app: SettingsAppId) => {
    setActiveSettingsApp(app);
    setOpenSettingsSection(null);
  };

  const openApplicationLog = async (app: SettingsAppId, target: "directory" | "file") => {
    try {
      await invoke("open_application_log_location", { app, target });
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const openRepository = async (url: string) => {
    try {
      await invoke("open_external_url", { url });
    } catch (error) {
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const checkApplicationUpdate = (app: SettingsAppId) => {
    const repositoryUrl = appDiagnostics[app].repositoryUrl?.replace(/\/$/, "");
    if (!repositoryUrl) {
      pushAppToast({ level: "warning", text: `${appDiagnostics[app].appName} 暂无更新地址` });
      return;
    }
    void openRepository(`${repositoryUrl}/releases/latest`);
  };

  const resolveShortcutKeys = (item: HookShortcutDisplayItem) => {
    if (!item.sourceId) return item.keys;
    const configured = shortcuts.find((shortcut) => shortcut.id === item.sourceId)?.keys.trim();
    const normalizedConfigured = configured?.replace(/\s+/g, "").toLocaleLowerCase();
    const contextualDefaultOverride = (
      (item.sourceId === "cancel" && normalizedConfigured === "escape")
      || (item.sourceId === "delete_unit" && normalizedConfigured === "delete/backspace")
    );
    if (contextualDefaultOverride) return item.keys;
    return configured ? splitShortcutAlternatives(configured) : item.keys;
  };

  const toggleShortcutGroup = (groupId: string) => {
    setOpenShortcutGroups((current) => {
      const next = new Set(current);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  };

  const openShortcutEditor = (item: HookShortcutDisplayItem) => {
    if (!item.sourceId) return;
    const keys = resolveShortcutKeys(item);
    setShortcutEditor({
      item,
      keys: [keys[0] || "", keys[1] || "", keys[2] || ""],
      activeSlot: 0,
      slotCount: shortcutSlotCount(keys),
    });
  };

  const handleShortcutCapture = (event: KeyboardEvent<HTMLButtonElement>, slot: ShortcutSlot) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
      setShortcutEditor(null);
      return;
    }
    const keys = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!keys) return;
    setShortcutEditor((current) => current ? {
      ...current,
      activeSlot: slot,
      keys: current.keys.map((value, index) => index === slot ? keys : value) as ShortcutSlots,
    } : current);
  };

  const shortcutConflictMessage = (
    candidateKeys: ShortcutSlots,
    candidateContexts: readonly HookShortcutContext[],
    excludeSourceId?: string,
    excludeQuickBindingId?: string,
    candidateConflictFamily?: string,
  ) => {
    const normalized = candidateKeys.map((keys) => keys.trim().toLocaleLowerCase()).filter(Boolean);
    if (new Set(normalized).size !== normalized.length) return "同一事件的快捷键不能重复";
    const shortcutConflict = HOOK_SHORTCUT_GROUPS
      .flatMap((group) => group.items)
      .find((item) => (
        item.sourceId
        && item.sourceId !== excludeSourceId
        && (!candidateConflictFamily || item.conflictFamily !== candidateConflictFamily)
        && shortcutContextsOverlap(candidateContexts, item.contexts)
        && resolveShortcutKeys(item).some((keys) => normalized.includes(keys.toLocaleLowerCase()))
      ));
    if (shortcutConflict) return `与“${shortcutConflict.label}”冲突`;
    const quickBindingConflict = draft.quick_bindings.find((binding) => (
      binding.id !== excludeQuickBindingId
      && availableArtToolById.has(binding.art)
      && candidateContexts.includes("unit-selected")
      && splitShortcutAlternatives(binding.key).some((keys) => normalized.includes(keys.toLocaleLowerCase()))
    ));
    if (quickBindingConflict) {
      const art = availableArtToolById.get(quickBindingConflict.art);
      return `与“${art?.name || quickBindingConflict.art}”冲突`;
    }
    return null;
  };

  const shortcutEditorConflict = shortcutEditor
    ? shortcutConflictMessage(
      shortcutEditor.keys,
      HOOK_SHORTCUT_GROUPS
        .flatMap((group) => group.items)
        .filter((item) => item.sourceId === shortcutEditor.item.sourceId)
        .flatMap((item) => item.contexts)
        .filter((context, index, contexts) => contexts.indexOf(context) === index),
      shortcutEditor.item.sourceId,
      undefined,
      shortcutEditor.item.conflictFamily,
    )
    : null;

  const applyShortcutEditor = () => {
    if (!shortcutEditor?.item.sourceId || !shortcutEditor.keys.some((keys) => keys.trim()) || shortcutEditorConflict) return;
    const keys = shortcutEditor.keys.map((value) => value.trim()).filter(Boolean).join(" / ");
    updateShortcutDraft(
      shortcutEditor.item.sourceId,
      shortcutEditor.item.label,
      keys,
    );
    setShortcutEditor(null);
    pushAppToast({ level: "info", text: `${shortcutEditor.item.label}快捷键已更新` });
  };

  const openQuickBindingEditor = (binding?: LoomSettings["quick_bindings"][number]) => {
    const keys = binding ? splitShortcutAlternatives(binding.key) : [];
    setQuickBindingEditor({
      id: binding?.id || `${Date.now()}`,
      art: binding?.art || availableArtTools[0]?.id || "",
      keys: [keys[0] || "", keys[1] || "", keys[2] || ""],
      activeSlot: 0,
      slotCount: shortcutSlotCount(keys),
    });
  };

  const handleQuickBindingCapture = (event: KeyboardEvent<HTMLButtonElement>, slot: ShortcutSlot) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
      setQuickBindingEditor(null);
      return;
    }
    const keys = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!keys) return;
    setQuickBindingEditor((current) => current ? {
      ...current,
      activeSlot: slot,
      keys: current.keys.map((value, index) => index === slot ? keys : value) as ShortcutSlots,
    } : current);
  };

  const quickBindingConflict = quickBindingEditor
    ? shortcutConflictMessage(quickBindingEditor.keys, ["unit-selected"], undefined, quickBindingEditor.id)
    : null;

  const applyQuickBindingEditor = () => {
    if (!quickBindingEditor?.art || !quickBindingEditor.keys.some((keys) => keys.trim()) || quickBindingConflict) return;
    const nextBinding = {
      id: quickBindingEditor.id,
      art: quickBindingEditor.art,
      key: quickBindingEditor.keys.map((value) => value.trim()).filter(Boolean).join(" / "),
    };
    setDraft((current) => ({
      ...current,
      quick_bindings: current.quick_bindings.some((binding) => binding.id === nextBinding.id)
        ? current.quick_bindings.map((binding) => binding.id === nextBinding.id ? nextBinding : binding)
        : [...current.quick_bindings, nextBinding],
    }));
    setQuickBindingEditor(null);
    pushAppToast({ level: "info", text: "Art 快捷键已更新" });
  };

  return (
    <section className="settings-page" aria-labelledby="settings-page-title">
      <header className="settings-page__heading">
        <h1 id="settings-page-title">设置</h1>
        <nav className="settings-app-tabs" aria-label="应用设置" role="tablist">
          <button
            className={activeSettingsApp === "loom" ? "settings-app-tab settings-app-tab--active" : "settings-app-tab"}
            type="button"
            role="tab"
            aria-selected={activeSettingsApp === "loom"}
            aria-controls="settings-app-panel-loom"
            onClick={() => selectSettingsApp("loom")}
          >
            Loom
          </button>
          <button
            className={activeSettingsApp === "hook" ? "settings-app-tab settings-app-tab--active" : "settings-app-tab"}
            type="button"
            role="tab"
            aria-selected={activeSettingsApp === "hook"}
            aria-controls="settings-app-panel-hook"
            onClick={() => selectSettingsApp("hook")}
          >
            Hook
          </button>
        </nav>
      </header>

      <div
        className="settings-app-panel"
        id={`settings-app-panel-${activeSettingsApp}`}
        role="tabpanel"
        aria-label={`${activeSettingsApp === "loom" ? "Loom" : "Hook"} 设置`}
      >
      {activeSettingsApp === "loom" ? (
        <div className="settings-accordion">
        <SettingsAccordionSection id="general" label="常规" open={openSettingsSection === "general"} onToggle={() => toggleSettingsSection("general")}>
          <GeneralSettingsPanel
            appName="loom"
            value={{
              language: draft.general.language,
              theme: draft.general.theme,
              closeToTray: draft.general.minimize_to_tray,
            }}
            onChange={(patch) => {
              if (patch.closeToTray !== undefined) {
                toggleMinimizeToTray(patch.closeToTray);
                return;
              }
              setDraft((current) => ({
                ...current,
                general: {
                  ...current.general,
                  ...(patch.language === undefined ? {} : { language: patch.language }),
                  ...(patch.theme === undefined ? {} : { theme: patch.theme }),
                },
              }));
            }}
          />
        </SettingsAccordionSection>

        <SettingsAccordionSection id="mcp" label="MCP" open={openSettingsSection === "mcp"} onToggle={() => toggleSettingsSection("mcp")}>
          <McpSettingsPanel value={draft.mcp} onChange={updateMcpDraft} />
        </SettingsAccordionSection>

        <SettingsAccordionSection id="art-store" label="Art" open={openSettingsSection === "art-store"} onToggle={() => toggleSettingsSection("art-store")}>
          <ArtStoreSettingsPanel
            value={draft.art_store}
            trustPolicy={artStoreTrustPolicy}
            trustPolicyBusy={artStoreTrustPolicyBusy}
            onChange={updateArtStoreDraft}
            onTrustPolicyChange={(policy) => void updateArtStoreTrustPolicy(policy)}
          />
        </SettingsAccordionSection>

        <SettingsAccordionSection id="cache" label="缓存" open={openSettingsSection === "cache"} onToggle={() => toggleSettingsSection("cache")}>
          <LoomCacheSettingsPanel
            settings={draft.loom_cache}
            snapshot={loomCacheSnapshot}
            loading={loomCacheLoading}
            busyKind={loomCacheBusyKind}
            onSettingsChange={updateLoomCacheDraft}
            onClear={(kind) => void clearLoomCache(kind)}
          />
        </SettingsAccordionSection>

        <SettingsAccordionSection id="network" label="网络" open={openSettingsSection === "network"} onToggle={() => toggleSettingsSection("network")}>
          <NetworkSettingsPanel
            appName="Loom"
            value={draft.network.loom}
            onChange={(patch) => updateNetworkDraft("loom", patch)}
          />
        </SettingsAccordionSection>

        <SettingsAccordionSection id="about" label="关于" open={openSettingsSection === "about"} onToggle={() => toggleSettingsSection("about")}>
          <AboutPanel
            app="loom"
            diagnostics={{
              ...appDiagnostics.loom,
              logDir: appDiagnostics.loom.logDir || appPaths?.logDir || "",
            }}
            logLevel={draft.system.loom_log_level}
            onLogLevelChange={(logLevel) => setDraft((current) => ({
              ...current,
              system: { ...current.system, loom_log_level: logLevel },
            }))}
            onCheckUpdate={() => checkApplicationUpdate("loom")}
            onOpenLog={(target) => void openApplicationLog("loom", target)}
            onOpenRepository={(url) => void openRepository(url)}
          />
        </SettingsAccordionSection>
        </div>
      ) : (
        <div className="settings-accordion">
          <SettingsAccordionSection id="general" label="常规" open={openSettingsSection === "general"} onToggle={() => toggleSettingsSection("general")}>
            <GeneralSettingsPanel
              appName="hook"
              value={{
                language: draft.hook_general.language,
                theme: draft.hook_general.theme,
                closeToTray: draft.hook_general.close_to_tray,
              }}
              onChange={updateHookGeneralDraft}
            />
          </SettingsAccordionSection>

          <SettingsAccordionSection id="shortcuts" label="快捷键" open={openSettingsSection === "shortcuts"} onToggle={() => toggleSettingsSection("shortcuts")}>
            <div className="hook-shortcut-groups">
              {HOOK_SHORTCUT_GROUPS.map((group) => {
                const groupOpen = openShortcutGroups.has(group.id);
                const contentId = `hook-shortcut-group-content-${group.id}`;
                return (
                <section className={groupOpen ? "hook-shortcut-group hook-shortcut-group--open" : "hook-shortcut-group"} key={group.id} aria-labelledby={`hook-shortcut-group-${group.id}`}>
                  <header className="hook-shortcut-group__header">
                    <h3 id={`hook-shortcut-group-${group.id}`}>
                      <button
                        className="hook-shortcut-group__trigger"
                        type="button"
                        aria-expanded={groupOpen}
                        aria-controls={contentId}
                        onClick={() => toggleShortcutGroup(group.id)}
                      >
                        <HookShortcutGroupIcon kind={group.icon} />
                        <span>{group.label}</span>
                        <svg className="hook-shortcut-group__chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                          <path d="m7 9 5 5 5-5" />
                        </svg>
                      </button>
                    </h3>
                  </header>
                  {groupOpen ? <div className="hook-shortcut-list" id={contentId}>
                    {group.items.map((item) => (
                      <div className="hook-shortcut-row" key={item.id}>
                        <span className="hook-shortcut-row__text">
                          <strong>{item.label}</strong>
                          <small>{item.description}</small>
                        </span>
                        {item.sourceId ? (
                          <button
                            className="hook-shortcut-row__keys hook-shortcut-row__keys--editable"
                            type="button"
                            aria-label={`修改${item.label}快捷键`}
                            title="修改快捷键"
                            onClick={() => openShortcutEditor(item)}
                          >
                            <ShortcutKeySequence shortcuts={resolveShortcutKeys(item)} />
                            <svg className="hook-shortcut-row__edit-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                              <path d="M4 20h4l11-11-4-4L4 16v4Z" /><path d="m13.5 6.5 4 4" />
                            </svg>
                          </button>
                        ) : (
                          <span className="hook-shortcut-row__keys" aria-label={`${item.label}操作手势`}>
                            <ShortcutKeySequence shortcuts={item.keys} />
                          </span>
                        )}
                      </div>
                    ))}
                    {group.id === "panels-tools" ? (
                      <>
                        {draft.quick_bindings
                          .filter((binding) => availableArtToolById.has(binding.art))
                          .map((binding) => {
                            const tool = availableArtToolById.get(binding.art)!;
                            return (
                              <div className="hook-shortcut-row hook-shortcut-row--art-binding" key={binding.id}>
                                <span className="hook-shortcut-row__text">
                                  <strong>{tool.name || tool.id}</strong>
                                  <small>快速添加 Art 节点</small>
                                </span>
                                <span className="hook-quick-binding-actions">
                                  <button
                                    className="hook-shortcut-row__keys hook-shortcut-row__keys--editable"
                                    type="button"
                                    aria-label={`修改${tool.name || tool.id}快捷键`}
                                    onClick={() => openQuickBindingEditor(binding)}
                                  >
                                    <ShortcutKeySequence shortcuts={splitShortcutAlternatives(binding.key)} />
                                    <svg className="hook-shortcut-row__edit-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                                      <path d="M4 20h4l11-11-4-4L4 16v4Z" /><path d="m13.5 6.5 4 4" />
                                    </svg>
                                  </button>
                                  <button
                                    className="hook-quick-binding-remove"
                                    type="button"
                                    aria-label={`删除${tool.name || tool.id}快捷键`}
                                    title="删除"
                                    onClick={() => setDraft((current) => ({
                                      ...current,
                                      quick_bindings: current.quick_bindings.filter((item) => item.id !== binding.id),
                                    }))}
                                  >
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                                      <path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" />
                                    </svg>
                                  </button>
                                </span>
                              </div>
                            );
                          })}
                        <button
                          className="hook-shortcut-add-art"
                          type="button"
                          disabled={!availableArtTools.length}
                          onClick={() => openQuickBindingEditor()}
                        >
                          <span aria-hidden="true">＋</span>
                          <strong>添加 Art 快捷键</strong>
                        </button>
                      </>
                    ) : null}
                  </div> : null}
                </section>
              )})}
            </div>
          </SettingsAccordionSection>

          <SettingsAccordionSection id="cache" label="缓存" open={openSettingsSection === "cache"} onToggle={() => toggleSettingsSection("cache")}>
            <HookCacheSettingsPanel
              settings={draft.hook_cache}
              snapshot={hookCacheSnapshot}
              loading={hookCacheLoading}
              busyKind={hookCacheBusyKind}
              onSettingsChange={updateHookCacheDraft}
              onClear={(kind) => void clearHookCache(kind)}
            />
          </SettingsAccordionSection>

          <SettingsAccordionSection id="network" label="网络" open={openSettingsSection === "network"} onToggle={() => toggleSettingsSection("network")}>
            <NetworkSettingsPanel
              appName="Hook"
              value={draft.network.hook}
              onChange={(patch) => updateNetworkDraft("hook", patch)}
            />
          </SettingsAccordionSection>

          <SettingsAccordionSection id="about" label="关于" open={openSettingsSection === "about"} onToggle={() => toggleSettingsSection("about")}>
            <AboutPanel
              app="hook"
              diagnostics={appDiagnostics.hook}
              logLevel={draft.system.hook_log_level}
              onLogLevelChange={(logLevel) => setDraft((current) => ({
                ...current,
                system: { ...current.system, hook_log_level: logLevel },
              }))}
              onCheckUpdate={() => checkApplicationUpdate("hook")}
              onOpenLog={(target) => void openApplicationLog("hook", target)}
              onOpenRepository={(url) => void openRepository(url)}
            />
          </SettingsAccordionSection>
        </div>
      )}
      </div>
      {shortcutEditor ? createPortal(
        <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) setShortcutEditor(null);
        }}>
          <section
            className="framework-dialog shortcut-edit-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="shortcut-edit-dialog-title"
            onKeyDown={(event) => {
              if (event.key === "Escape") setShortcutEditor(null);
            }}
          >
            <header className="framework-dialog__header">
              <div>
                <h2 id="shortcut-edit-dialog-title">修改快捷键</h2>
                <p>{shortcutEditor.item.label}</p>
              </div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setShortcutEditor(null)}>×</button>
            </header>
            <div className="shortcut-edit-dialog__body">
              {Array.from({ length: shortcutEditor.slotCount }, (_, index) => index as ShortcutSlot).map((slot) => {
                const keys = shortcutEditor.keys[slot];
                return (
                  <div className="shortcut-capture-slot" key={slot}>
                    <span>快捷键 {slot + 1}</span>
                    <div className="shortcut-capture-slot__controls">
                      {shortcutEditor.item.gestureAction ? (
                        <div className="shortcut-gesture-picker" role="group" aria-label={`${shortcutEditor.item.label}快捷键 ${slot + 1}`}>
                          <span className="shortcut-gesture-picker__action">{shortcutEditor.item.gestureAction}</span>
                          <span className="shortcut-gesture-picker__modifiers">
                            {SHORTCUT_GESTURE_MODIFIERS.map((modifier) => {
                              const selected = gestureShortcutModifiers(keys, shortcutEditor.item.gestureAction!).has(modifier);
                              return (
                                <button
                                  className={selected ? "shortcut-gesture-modifier shortcut-gesture-modifier--active" : "shortcut-gesture-modifier"}
                                  type="button"
                                  aria-pressed={selected}
                                  key={modifier}
                                  onClick={() => setShortcutEditor((current) => current?.item.gestureAction ? {
                                    ...current,
                                    activeSlot: slot,
                                    keys: current.keys.map((value, keyIndex) => keyIndex === slot
                                      ? toggleGestureShortcutModifier(value, current.item.gestureAction!, modifier)
                                      : value) as ShortcutSlots,
                                  } : current)}
                                >{modifier}</button>
                              );
                            })}
                          </span>
                        </div>
                      ) : (
                        <button
                          className={shortcutEditor.activeSlot === slot ? "shortcut-capture-field shortcut-capture-field--active" : "shortcut-capture-field"}
                          type="button"
                          autoFocus={slot === shortcutEditor.activeSlot}
                          onFocus={() => setShortcutEditor((current) => current ? { ...current, activeSlot: slot } : current)}
                          onKeyDown={(event) => handleShortcutCapture(event, slot)}
                        >
                          {keys ? <ShortcutKeySequence shortcuts={[keys]} /> : <strong>未设置</strong>}
                          <small>按下新的组合键</small>
                        </button>
                      )}
                      <button
                        className="shortcut-capture-clear"
                        type="button"
                        disabled={slot === 0 && !keys}
                        onClick={() => setShortcutEditor((current) => current ? {
                          ...current,
                          activeSlot: slot > 0 ? 0 : current.activeSlot,
                          slotCount: slot > 0 ? Math.max(1, current.slotCount - 1) as 1 | 2 | 3 : current.slotCount,
                          keys: removeShortcutSlot(current.keys, slot),
                        } : current)}
                      >{slot > 0 ? "删除" : "清除"}</button>
                    </div>
                  </div>
                );
              })}
              {shortcutEditor.slotCount < 3 ? (
                <button
                  className="shortcut-add-secondary"
                  type="button"
                  onClick={() => setShortcutEditor((current) => current ? {
                    ...current,
                    activeSlot: current.slotCount as ShortcutSlot,
                    slotCount: (current.slotCount + 1) as 2 | 3,
                  } : current)}
                >＋ 添加额外快捷键</button>
              ) : null}
              {shortcutEditorConflict ? (
                <p className="shortcut-edit-dialog__conflict">{shortcutEditorConflict}</p>
              ) : null}
            </div>
            <footer className="shortcut-edit-dialog__footer">
              <button className="ghost-button" type="button" onClick={() => setShortcutEditor((current) => current ? {
                ...current,
                keys: [current.item.keys[0] || "", current.item.keys[1] || "", current.item.keys[2] || ""],
                activeSlot: 0,
                slotCount: shortcutSlotCount(current.item.keys),
              } : current)}>恢复默认</button>
              <span />
              <button className="ghost-button" type="button" onClick={() => setShortcutEditor(null)}>取消</button>
              <button className="signal-button" type="button" disabled={!shortcutEditor.keys.some((keys) => keys.trim()) || Boolean(shortcutEditorConflict)} onClick={applyShortcutEditor}>应用</button>
            </footer>
          </section>
        </div>,
        document.body,
      ) : null}
      {quickBindingEditor ? createPortal(
        <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) setQuickBindingEditor(null);
        }}>
          <section
            className="framework-dialog shortcut-edit-dialog quick-binding-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="quick-binding-dialog-title"
            onKeyDown={(event) => {
              if (event.key === "Escape") setQuickBindingEditor(null);
            }}
          >
            <header className="framework-dialog__header">
              <div>
                <h2 id="quick-binding-dialog-title">Art 快捷键</h2>
                <p>选择要快速添加的 Art</p>
              </div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setQuickBindingEditor(null)}>×</button>
            </header>
            <div className="shortcut-edit-dialog__body">
              <label className="quick-binding-dialog__art">
                <span>Art</span>
                <select
                  className="studio-input"
                  value={quickBindingEditor.art}
                  onChange={(event) => setQuickBindingEditor((current) => current ? { ...current, art: event.target.value } : current)}
                >
                  {availableArtTools.map((tool) => <option value={tool.id} key={tool.id}>{tool.name || tool.id}</option>)}
                </select>
              </label>
              {Array.from({ length: quickBindingEditor.slotCount }, (_, index) => index as ShortcutSlot).map((slot) => {
                const keys = quickBindingEditor.keys[slot];
                return (
                  <div className="shortcut-capture-slot" key={slot}>
                    <span>快捷键 {slot + 1}</span>
                    <div className="shortcut-capture-slot__controls">
                      <button
                        className={quickBindingEditor.activeSlot === slot ? "shortcut-capture-field shortcut-capture-field--active" : "shortcut-capture-field"}
                        type="button"
                        autoFocus={slot === quickBindingEditor.activeSlot}
                        onFocus={() => setQuickBindingEditor((current) => current ? { ...current, activeSlot: slot } : current)}
                        onKeyDown={(event) => handleQuickBindingCapture(event, slot)}
                      >
                        {keys ? <ShortcutKeySequence shortcuts={[keys]} /> : <strong>未设置</strong>}
                        <small>按下新的组合键</small>
                      </button>
                      <button
                        className="shortcut-capture-clear"
                        type="button"
                        disabled={slot === 0 && !keys}
                        onClick={() => setQuickBindingEditor((current) => current ? {
                          ...current,
                          activeSlot: slot > 0 ? 0 : current.activeSlot,
                          slotCount: slot > 0 ? Math.max(1, current.slotCount - 1) as 1 | 2 | 3 : current.slotCount,
                          keys: removeShortcutSlot(current.keys, slot),
                        } : current)}
                      >{slot > 0 ? "删除" : "清除"}</button>
                    </div>
                  </div>
                );
              })}
              {quickBindingEditor.slotCount < 3 ? (
                <button
                  className="shortcut-add-secondary"
                  type="button"
                  onClick={() => setQuickBindingEditor((current) => current ? {
                    ...current,
                    activeSlot: current.slotCount as ShortcutSlot,
                    slotCount: (current.slotCount + 1) as 2 | 3,
                  } : current)}
                >＋ 添加额外快捷键</button>
              ) : null}
              {quickBindingConflict ? <p className="shortcut-edit-dialog__conflict">{quickBindingConflict}</p> : null}
            </div>
            <footer className="shortcut-edit-dialog__footer shortcut-edit-dialog__footer--simple">
              <span />
              <span />
              <button className="ghost-button" type="button" onClick={() => setQuickBindingEditor(null)}>取消</button>
              <button
                className="signal-button"
                type="button"
                disabled={!quickBindingEditor.art || !quickBindingEditor.keys.some((keys) => keys.trim()) || Boolean(quickBindingConflict)}
                onClick={applyQuickBindingEditor}
              >应用</button>
            </footer>
          </section>
        </div>,
        document.body,
      ) : null}
    </section>
  );
}

function ApplicationAboutMark({ app }: { app: SettingsAppId }) {
  if (app === "loom") {
    return <LoomMark />;
  }
  return (
    <svg className="about-panel__hook-mark" viewBox="0 0 1024 1024" aria-hidden="true">
      <defs>
        <filter id="about-hook-green-glow" x="-90%" y="-90%" width="280%" height="280%">
          <feGaussianBlur stdDeviation="18" result="blur" />
          <feColorMatrix in="blur" type="matrix" values="0 0 0 0 0.13  0 0 0 0 0.77  0 0 0 0 0.37  0 0 0 0.78 0" />
          <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
        <filter id="about-hook-yellow-glow" x="-90%" y="-90%" width="280%" height="280%">
          <feGaussianBlur stdDeviation="18" result="blur" />
          <feColorMatrix in="blur" type="matrix" values="0 0 0 0 1  0 0 0 0 0.9  0 0 0 0 0  0 0 0 0.82 0" />
          <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
      </defs>
      <g fill="none" strokeLinecap="round" strokeLinejoin="round">
        <path d="M250 394V250h144" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-hook-green-glow)" />
        <path d="M774 394V250H630" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-hook-green-glow)" />
        <path d="M250 630v144h144" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-hook-green-glow)" />
        <path d="M774 630v144H630" stroke="var(--loom-brand-secondary)" strokeWidth="72" filter="url(#about-hook-yellow-glow)" />
      </g>
    </svg>
  );
}

function AboutPanel({
  app,
  diagnostics,
  logLevel,
  onLogLevelChange,
  onCheckUpdate,
  onOpenLog,
  onOpenRepository,
}: {
  app: SettingsAppId;
  diagnostics: ApplicationDiagnosticsInfo;
  logLevel: string;
  onLogLevelChange: (value: string) => void;
  onCheckUpdate: () => void;
  onOpenLog: (target: "directory" | "file") => void;
  onOpenRepository: (url: string) => void;
}) {
  const titleId = `about-product-name-${app}`;
  return (
    <section className="about-panel" aria-labelledby={titleId}>
      <div className="about-panel__group">
        <div className="about-panel__identity">
          <span className={`about-panel__mark about-panel__mark--${app}`} aria-hidden="true">
            <ApplicationAboutMark app={app} />
          </span>
          <h2 id={titleId}>{diagnostics.appName}</h2>
        </div>
        <dl className="about-panel__rows">
          <div>
            <dt>应用名称</dt>
            <dd>{diagnostics.appName}</dd>
          </div>
          <div>
            <dt>版本号</dt>
            <dd>{diagnostics.version}</dd>
          </div>
          <div>
            <dt>检查更新</dt>
            <dd><button className="ghost-button" type="button" onClick={onCheckUpdate}>立即检查</button></dd>
          </div>
          {diagnostics.repositoryUrl ? (
            <div className="about-panel__repository-row">
              <dt>仓库</dt>
              <dd>
                <button
                  className="about-panel__repository-link"
                  type="button"
                  title={`打开 ${diagnostics.repositoryUrl}`}
                  onClick={() => onOpenRepository(diagnostics.repositoryUrl!)}
                >
                  <span>{diagnostics.repositoryUrl}</span>
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M14 5h5v5M12 12l7-7M19 13v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5" />
                  </svg>
                </button>
                <span className="about-panel__commit" title="构建对应的 Git 提交">
                  {diagnostics.commitShort?.slice(0, 6) || "------"}
                </span>
              </dd>
            </div>
          ) : null}
        </dl>
      </div>

      <div className="about-panel__group about-panel__group--diagnostics">
        <div className="about-panel__group-title">
          <SettingsSectionIcon kind="system" />
          <h3>诊断日志</h3>
        </div>
        <dl className="about-panel__rows">
          <div>
            <dt>日志级别</dt>
            <dd>
              <select
                className="studio-input about-panel__log-level"
                aria-label={`${diagnostics.appName} 日志级别`}
                value={logLevel}
                onChange={(event) => onLogLevelChange(event.target.value)}
              >
                <option value="error">Error</option>
                <option value="warn">Warn</option>
                <option value="info">Info（默认）</option>
                <option value="debug">Debug</option>
              </select>
            </dd>
          </div>
          <div className="about-panel__path-row">
            <dt>日志位置</dt>
            <dd>
              <code title={diagnostics.logDir || undefined}>{diagnostics.logDir || "未加载"}</code>
              <button className="ghost-button" type="button" onClick={() => onOpenLog("directory")}>打开文件夹</button>
            </dd>
          </div>
          <div>
            <dt>查看日志</dt>
            <dd>
              <span className="about-panel__log-file">
                {diagnostics.logFile ? diagnostics.logFile.split(/[\\/]/).pop() : "暂无日志"}
              </span>
              <button
                className="ghost-button"
                type="button"
                disabled={!diagnostics.logFileExists}
                onClick={() => onOpenLog("file")}
              >
                查看
              </button>
            </dd>
          </div>
        </dl>
      </div>
    </section>
  );
}

export default function App() {
  const snapshotRequestGate = useRef(createLatestRequestGate());
  const snapshotSingleFlight = useRef(createSingleFlightGate());
  const hookCanvasRequestGate = useRef(createLatestRequestGate());
  const hookCanvasSingleFlight = useRef(createSingleFlightGate());
  const hookCanvasFlightBaseUrl = useRef<string | null>(null);
  const packagedArtsBootstrapBaseUrl = useRef<string | null>(null);
  const [activeSection, setActiveSection] = useState<SectionId>("mcp");
  const [railCollapsed, setRailCollapsed] = useState(false);
  const [snapshot, setSnapshot] = useState<LoomSnapshot>(fallbackSnapshot);
  const [loading, setLoading] = useState(false);
  const [autoStartAttempted, setAutoStartAttempted] = useState(false);
  const [pendingArtCreationRequest, setPendingArtCreationRequest] = useState<ArtCreationRequest | null>(null);
  const [hookCanvas, setHookCanvas] = useState<HookCanvasSnapshot | null>(null);
  const [hookCanvasLoading, setHookCanvasLoading] = useState(false);
  const [hookCanvasError, setHookCanvasError] = useState<string | null>(null);
  const [hookCanvasRefreshVersion, setHookCanvasRefreshVersion] = useState(0);
  const [hookBridgeUrl, setHookBridgeUrl] = useState(DEFAULT_HOOK_BRIDGE_URL);
  const hookCanvasRefreshTrigger = getHookCanvasRefreshTrigger({
    connectionState: snapshot.connectionState,
    baseUrl: snapshot.baseUrl,
    refreshVersion: hookCanvasRefreshVersion,
  });

  const refreshSnapshot = useCallback(async (abortSignal?: AbortSignal): Promise<LoomSnapshot> => {
    return await snapshotSingleFlight.current.run(async () => {
      const requestToken = snapshotRequestGate.current.begin();
      const abortRequest = () => {
        if (snapshotRequestGate.current.isCurrent(requestToken)) {
          snapshotRequestGate.current.invalidate();
          setLoading(false);
        }
      };
      abortSignal?.addEventListener("abort", abortRequest, { once: true });
      setLoading(true);
      try {
        let baseUrl = DEFAULT_LOOM_DAEMON_URL;
        let nextHookBridgeUrl = DEFAULT_HOOK_BRIDGE_URL;
        try {
          const runtimeConfig = await invoke<RuntimeConfig>("resolve_loom_daemon_url");
          baseUrl = runtimeConfig.loomDaemonUrl || DEFAULT_LOOM_DAEMON_URL;
          nextHookBridgeUrl = runtimeConfig.hookBridgeUrl || DEFAULT_HOOK_BRIDGE_URL;
        } catch {
          baseUrl = DEFAULT_LOOM_DAEMON_URL;
        }
        const next = await readLoomSnapshot(baseUrl);
        if (!abortSignal?.aborted && snapshotRequestGate.current.isCurrent(requestToken)) {
          setHookBridgeUrl(nextHookBridgeUrl);
          setSnapshot((previous) => retainAvailableSnapshotData(previous, next));
        }
        return next;
      } finally {
        abortSignal?.removeEventListener("abort", abortRequest);
        if (snapshotRequestGate.current.isCurrent(requestToken)) {
          setLoading(false);
        }
      }
    });
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    await refreshSnapshot();
  }, [refreshSnapshot]);

  const refreshHookCanvas = useCallback(async (baseUrl: string) => {
    if (hookCanvasFlightBaseUrl.current !== baseUrl) {
      hookCanvasFlightBaseUrl.current = baseUrl;
      hookCanvasRequestGate.current.invalidate();
      hookCanvasSingleFlight.current.invalidate();
    }
    await hookCanvasSingleFlight.current.run(async () => {
      const requestToken = hookCanvasRequestGate.current.begin();
      setHookCanvasLoading(true);
      try {
        const next = await readHookCanvasSnapshot(baseUrl);
        if (hookCanvasRequestGate.current.isCurrent(requestToken)) {
          setHookCanvas((previous) => keepNewestHookCanvasSnapshot(previous, next));
          setHookCanvasError(null);
        }
      } catch (error) {
        if (hookCanvasRequestGate.current.isCurrent(requestToken)) {
          setHookCanvasError(error instanceof Error ? error.message : "无法读取 Hook 画布。");
        }
      } finally {
        if (hookCanvasRequestGate.current.isCurrent(requestToken)) {
          setHookCanvasLoading(false);
        }
      }
    });
  }, []);

  const startLocalService = async () => {
    try {
      await startLoomDaemon();
      await waitForLoomOnline(refreshSnapshot);
    } catch {
      // The regular snapshot state reports startup failures.
    }
  };

  useEffect(() => {
    setAutoStartAttempted(true);
    void refresh();
    void startLocalService();
  }, []);

  useEffect(() => {
    if (snapshot.connectionState !== "online") return;
    let cancelled = false;
    void getLoomSettings(snapshot.baseUrl)
      .then(async (settings) => {
        if (cancelled) return;
        applyLoomGeneralSettings(settings.general);
        await invoke("apply_loom_general_settings", {
          settings: { minimizeToTray: settings.general.minimize_to_tray },
        });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [snapshot.baseUrl, snapshot.connectionState]);

  useEffect(() => {
    if (hookCanvasRefreshTrigger === null) {
      hookCanvasFlightBaseUrl.current = null;
      hookCanvasRequestGate.current.invalidate();
      hookCanvasSingleFlight.current.invalidate();
      setHookCanvasLoading(false);
      return;
    }
    void refreshHookCanvas(snapshot.baseUrl);
  }, [hookCanvasRefreshTrigger, refreshHookCanvas, snapshot.baseUrl]);

  // Auto-sync the Hook canvas while online. Hook persists position and image
  // edits to session.json without always emitting a bridge broadcast, so poll
  // the snapshot on an interval. The daemon returns a cheap content revision and
  // keepNewestHookCanvasSnapshot dedupes by revision, so an unchanged canvas
  // does not cause a re-render or reload previews.
  useEffect(() => {
    if (hookCanvasRefreshTrigger === null) {
      return;
    }
    const interval = window.setInterval(() => {
      void refreshHookCanvas(snapshot.baseUrl);
    }, HOOK_CANVAS_POLL_INTERVAL_MS);
    return () => {
      window.clearInterval(interval);
    };
  }, [hookCanvasRefreshTrigger, refreshHookCanvas, snapshot.baseUrl]);

  useEffect(() => {
    if (
      autoStartAttempted ||
      loading ||
      snapshot.connectionState !== "offline" ||
      snapshot.checkedAt === fallbackSnapshot.checkedAt
    ) {
      return;
    }
    setAutoStartAttempted(true);
    void startLocalService();
  }, [autoStartAttempted, loading, snapshot.connectionState, snapshot.checkedAt]);

  useEffect(() => {
    if (
      snapshot.connectionState !== "online"
      || packagedArtsBootstrapBaseUrl.current === snapshot.baseUrl
    ) {
      return;
    }
    packagedArtsBootstrapBaseUrl.current = snapshot.baseUrl;
    let cancelled = false;
    void (async () => {
      let lastError: unknown = null;
      for (let attempt = 0; attempt < 3; attempt += 1) {
        if (cancelled) return;
        try {
          const result = await bootstrapPackagedArts(snapshot.baseUrl);
          if (!cancelled && result.applied) {
            await refresh();
          }
          return;
        } catch (error) {
          lastError = error;
          if (attempt < 2) {
            await new Promise((resolve) => window.setTimeout(resolve, 1000 * (attempt + 1)));
          }
        }
      }
      if (!cancelled) {
        pushAppToast({
          level: "error",
          text: lastError instanceof Error ? lastError.message : "无法加载打包 Art。",
        });
      }
    })();
    return () => {
      cancelled = true;
      if (packagedArtsBootstrapBaseUrl.current === snapshot.baseUrl) {
        packagedArtsBootstrapBaseUrl.current = null;
      }
    };
  }, [refresh, snapshot.baseUrl, snapshot.connectionState]);

  // Ensure the configured Hook bridge is running once the daemon is online.
  // Idempotent: the daemon returns 409 if already running, which we ignore.
  useEffect(() => {
    if (snapshot.connectionState !== "online") {
      return;
    }
    let bridgePort: number | undefined;
    try {
      const parsedPort = Number(new URL(hookBridgeUrl).port);
      bridgePort = Number.isInteger(parsedPort) && parsedPort > 0 ? parsedPort : undefined;
    } catch {
      bridgePort = undefined;
    }
    void startHookBridge(snapshot.baseUrl, bridgePort).catch(() => {
      // Already running or transient failure — the workflow-sync client below
      // will retry connecting regardless.
    });
  }, [hookBridgeUrl, snapshot.connectionState, snapshot.baseUrl]);

  useEffect(() => {
    if (
      snapshot.connectionState !== "online"
      || typeof window === "undefined"
      || typeof WebSocket === "undefined"
    ) {
      return;
    }
    const sync = startHookBridgeWorkflowSync({
      refresh,
      websocketUrl: hookBridgeUrl,
      invalidateHookCanvas: () => {
        setHookCanvasRefreshVersion((version) => version + 1);
      },
    });

    return () => {
      sync.dispose();
    };
  }, [hookBridgeUrl, refresh, snapshot.connectionState]);

  const openWorkflowArtCreator = useCallback((request: WorkflowArtCreationRequest) => {
    setPendingArtCreationRequest({
      requestId: `${request.workflowId}-${Date.now()}`,
      mode: "workflow",
      repositoryName: request.tool.id,
      name: request.tool.name || request.workflowName,
      description: request.tool.description || "由 Hook 工作流创建的 Art。",
      workflowId: request.workflowId,
      templateTool: request.tool,
    });
    setActiveSection("registry");
  }, []);

  const handleArtCreationRequestHandled = useCallback(() => {
    setPendingArtCreationRequest(null);
  }, []);

  const activeNavigation = useMemo(
    () => navigationItems.find((item) => item.id === activeSection) ?? navigationItems[0],
    [activeSection],
  );

  const runWindowCommand = useCallback(async (
    command: "minimize" | "toggle-maximize" | "close",
  ): Promise<void> => {
    try {
      const currentWindow = getCurrentWindow();
      if (command === "minimize") await currentWindow.minimize();
      if (command === "toggle-maximize") await currentWindow.toggleMaximize();
      if (command === "close") await currentWindow.close();
    } catch {
      // Browser previews do not expose a native Tauri window.
    }
  }, []);

  const renderNavigationItem = (item: NavigationItem) => (
    <button
      className={activeSection === item.id ? "rail-item rail-item--active" : "rail-item"}
      type="button"
      key={item.id}
      title={item.label}
      aria-label={item.label}
      aria-current={activeSection === item.id ? "page" : undefined}
      data-testid={item.id === "hook-bridge" ? "nav-hook-bridge" : undefined}
      onClick={() => setActiveSection(item.id)}
    >
      <span className="rail-item__icon"><ShellIcon kind={item.icon} /></span>
      <span className="rail-item__label">{item.label}</span>
    </button>
  );

  return (
    <main className={railCollapsed ? "desktop-shell desktop-shell--rail-collapsed" : "desktop-shell"}>
      <AppToastViewport />
      <AppConfirmViewport />
      <header className="app-titlebar">
        <div
          className="app-titlebar__drag-region"
          data-tauri-drag-region
          onDoubleClick={() => void runWindowCommand("toggle-maximize")}
        >
          {activeSection === "settings" ? (
            <button
              className="app-titlebar__back"
              type="button"
              aria-label="返回 MCP"
              title="返回 MCP"
              onDoubleClick={(event) => event.stopPropagation()}
              onClick={() => setActiveSection("mcp")}
            >
              <ShellIcon kind="back" />
            </button>
          ) : null}
        </div>
        <div className="app-titlebar__controls">
          <button
            className={loading ? "window-control window-control--refresh window-control--loading" : "window-control window-control--refresh"}
            type="button"
            aria-label={loading ? "正在刷新" : "刷新"}
            title={loading ? "正在刷新" : "刷新"}
            onClick={() => void refresh()}
            disabled={loading}
          >
            <ShellIcon kind="refresh" />
          </button>
          <button
            className="window-control"
            type="button"
            aria-label="最小化"
            title="最小化"
            onClick={() => void runWindowCommand("minimize")}
          >
            <ShellIcon kind="minimize" />
          </button>
          <button
            className="window-control"
            type="button"
            aria-label="最大化或还原"
            title="最大化或还原"
            onClick={() => void runWindowCommand("toggle-maximize")}
          >
            <ShellIcon kind="maximize" />
          </button>
          <button
            className="window-control window-control--close"
            type="button"
            aria-label="关闭"
            title="关闭"
            onClick={() => void runWindowCommand("close")}
          >
            <ShellIcon kind="close" />
          </button>
        </div>
      </header>

      <aside className="left-rail">
        <div className="app-titlebar__brand">
          <button
            className="shell-icon-button shell-rail-toggle"
            type="button"
            aria-label={railCollapsed ? "展开侧栏" : "收起侧栏"}
            title={railCollapsed ? "展开侧栏" : "收起侧栏"}
            aria-expanded={!railCollapsed}
            onClick={() => setRailCollapsed((collapsed) => !collapsed)}
          >
            <span className="shell-rail-toggle__icon"><ShellIcon kind="sidebar" /></span>
            <span className="shell-rail-toggle__mark"><LoomMark /></span>
          </button>
          <span className="app-titlebar__product-mark"><LoomMark /></span>
          <strong className="app-titlebar__product-name">Loom</strong>
        </div>

        <nav className="rail-nav" aria-label="Loom sections">
          {primaryNavigationItems.map(renderNavigationItem)}
        </nav>

        <div className="rail-footer">
          <button
            className={activeSection === "devices" ? "rail-item rail-item--active rail-device-button" : "rail-item rail-device-button"}
            type="button"
            title="设备管理"
            aria-label="设备管理"
            aria-current={activeSection === "devices" ? "page" : undefined}
            onClick={() => setActiveSection("devices")}
          >
            <span className="rail-item__icon"><ShellIcon kind="device" /></span>
            <span className="rail-item__label">设备管理</span>
          </button>
          <nav className="rail-utility-nav" aria-label="Loom utilities">
            {utilityNavigationItems.map(renderNavigationItem)}
          </nav>
        </div>
      </aside>

      <section className={activeSection === "settings"
        ? "workspace-panel workspace-panel--settings"
        : activeSection === "registry" || activeSection === "hook-bridge"
          ? "workspace-panel workspace-panel--tooling"
          : "workspace-panel"}>
        {activeSection !== "devices" && activeSection !== "settings" ? <header className={activeSection === "registry" || activeSection === "hook-bridge"
          ? "workspace-header workspace-header--tooling"
          : "workspace-header"}>
          <div>
            {activeNavigation.eyebrow ? (
              <p className="section-kicker">{activeNavigation.eyebrow}</p>
            ) : null}
            <h1>{activeNavigation.label}</h1>
          </div>
        </header> : null}

        <div className={activeSection === "devices"
          ? "workspace-scroll workspace-scroll--devices"
          : activeSection === "settings"
            ? "workspace-scroll workspace-scroll--settings"
            : activeSection === "registry" || activeSection === "hook-bridge"
              ? "workspace-scroll workspace-scroll--tooling"
              : "workspace-scroll"}>
          {activeSection === "mcp" && (
            <McpPanel
              servers={snapshot.mcpServers}
              baseUrl={snapshot.baseUrl}
              refresh={refresh}
            />
          )}
          {activeSection === "registry" && (
            <ArtPanel
              tools={snapshot.tools}
              mcpServers={snapshot.mcpServers}
              workflows={snapshot.workflows}
              baseUrl={snapshot.baseUrl}
              refresh={refresh}
              pendingCreationRequest={pendingArtCreationRequest}
              onCreationRequestHandled={handleArtCreationRequestHandled}
            />
          )}
          {activeSection === "hook-bridge" && (
            <HookBridgePanel
              baseUrl={snapshot.baseUrl}
              hookCanvas={hookCanvas}
              hookCanvasError={hookCanvasError}
              tools={snapshot.tools}
              onCreateWorkflowArt={openWorkflowArtCreator}
            />
          )}
          {activeSection === "devices" && (
            <DeviceManagementPanel
              baseUrl={snapshot.baseUrl}
              online={snapshot.connectionState === "online"}
            />
          )}
          {activeSection === "settings" && <SettingsPanel snapshot={snapshot} />}
        </div>
      </section>
    </main>
  );
}
