import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, KeyboardEvent, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArtLoomAppPaths,
  ArtLoomCompatArt,
  ArtLoomCompatSettings,
  ArtLoomShortcutConfig,
  DEFAULT_LOOM_DAEMON_URL,
  DEFAULT_ARTLOOM_COMPAT_SETTINGS,
  LoomCapability,
  LoomHookBridgeStatus,
  LoomMcpServer,
  LoomModuleStatus,
  LoomPythonArt,
  LoomPythonPortDefinition,
  LoomSnapshot,
  LoomToolDefinition,
  LoomToolExecution,
  LoomWorkflowMetadata,
  buildMcpPackageInstallPlan,
  checkPythonArtJsonNearby,
  checkMcpPackageInstalled,
  createAuthoredArtPackage,
  deleteMcpServer,
  deleteToolDefinition,
  deleteWorkflowBundle,
  disableArtLoomCompatArt,
  enableArtLoomCompatArt,
  artLoomExecuteArtNodeErrorMessage,
  executeArtLoomArtNode,
  fetchMcpRegistry,
  getArtLoomCompatArt,
  getArtLoomCompatAppPaths,
  getArtLoomCompatSettings,
  getArtLoomCompatShortcuts,
  getPythonEngineStatus,
  getWorkflowBundle,
  inferPythonArtPorts,
  listArtLoomCompatArts,
  prefetchPythonArtShader,
  readLoomSnapshot,
  readPythonArtJson,
  readPythonArtSource,
  saveArtLoomCompatSettings,
  saveMcpServer,
  saveToolDefinition,
  saveWorkflowBundle,
  setArtLoomCompatAutostart,
  setArtLoomCompatMinimizeToTray,
  startHookBridge,
  startLoomDaemon,
  listFrameworks,
  installFramework,
  uninstallFramework,
  upgradeFrameworkPackage,
  listPluginTrust,
  trustPluginPublisher,
  revokePluginPublisher,
  listPluginCredentials,
  savePluginCredential,
  deletePluginCredential,
  fetchArtStoreCatalog,
  installArtFromStore,
  type LoomFramework,
  type LoomFrameworkAuthoringField,
  type LoomPublisherTrustRecord,
  type LoomCredentialSummary,
  type ArtStoreEntry,
  syncArtLoomCompatArts,
  testMcpConnection,
  updateArtLoomCompatArtDefaults,
  updateArtLoomWorkflowNode,
  updateArtLoomCompatShortcut,
  waitForLoomOnline,
} from "./services/loomApi";
import {
  buildAuthoredArtPackage,
  defaultAuthoringValues,
} from "./services/artAuthoring";
import {
  MCP_MARKET_CATEGORIES,
  MCP_MARKET_SERVERS,
  buildMarketplaceServerConfig,
  getMarketplaceHealth,
  mapRegistryResponseToMarketplace,
  mergeRegistryAndCuratedMarketplace,
  mcpMarketCategoryLabel,
  type McpMarketCategory,
  type McpMarketServer,
  type McpMarketplaceTestSnapshot,
} from "./services/mcpMarketplace";
import {
  inferPortsFromPythonCode,
  mapArtJsonPorts,
  type PythonArtPort,
} from "./services/pythonArtSource";
import { startHookBridgeWorkflowSync } from "./services/hookBridgeWorkflowSync";
import { createLatestRequestGate } from "./services/latestRequest";
import {
  artWorkspaceItems,
  filterToolsByFrameworks,
  frameworkFilterLabel,
  frameworkIdentity,
  nextArtWorkspaceIndex,
  type ArtWorkspaceId,
} from "./services/artHubUi";
import {
  IMAGE_SEARCH_ART_ID,
  IMAGE_SEARCH_SERVER_ID,
  buildImageSearchArtDefinition,
  buildImageSearchExecutionRequest,
  buildImageSearchServerConfig,
  canExecuteHookCanvasNodeManually,
} from "./services/mcpImageSearch";
import {
  getHookCanvasRefreshTrigger,
  keepNewestHookCanvasSnapshot,
  readHookCanvasSnapshot,
  type HookCanvasSnapshot,
} from "./services/hookCanvas";
import { HookCanvasThumbnail } from "./components/hook/HookCanvasThumbnail";
import { HookCanvasView } from "./components/hook/HookCanvasView";
import {
  addWorkflowGraphNode,
  autoTemplateResponse,
  deleteWorkflowGraphNode,
  inferWorkflowArtInterface,
  parseRawCommand,
  parseCurlCommand,
  parseWorkflowYamlLite,
  portsFromMcpToolSchema,
  serializeWorkflowGraphLite,
  updateWorkflowGraphNode,
} from "./services/workflowStudio";
import type {
  CurlImportResult,
  ParsedPort,
  WorkflowInterfaceInference,
  WorkflowStudioNode,
} from "./services/workflowStudio";

type SectionId =
  | "overview"
  | "mcp"
  | "registry"
  | "hook-bridge"
  | "workflows"
  | "agents"
  | "runs"
  | "settings"
  | "about";

interface RuntimeConfig {
  loomDaemonUrl: string;
  settingsUrl: string;
  hookBridgeUrl?: string;
}

interface WorkflowOpenRequest {
  workflowId: string;
  selectedNodeId?: string;
}

interface NavigationItem {
  id: SectionId;
  label: string;
  eyebrow: string;
}

const navigationItems: NavigationItem[] = [
  { id: "overview", label: "总览", eyebrow: "本地工作台" },
  { id: "mcp", label: "MCP", eyebrow: "服务工具" },
  { id: "registry", label: "Art", eyebrow: "" },
  { id: "hook-bridge", label: "Hook 同步", eyebrow: "" },
  { id: "workflows", label: "工作流工作台", eyebrow: "节点编排" },
  { id: "agents", label: "智能体", eyebrow: "本地大脑" },
  { id: "runs", label: "运行记录", eyebrow: "证据" },
  { id: "settings", label: "设置", eyebrow: "配置中心" },
  { id: "about", label: "关于", eyebrow: "Loom" },
];

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

const formatTime = (value: string) => {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "未检查";
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
};

const firstWords = (value: string | undefined, fallback: string) => {
  if (!value) return fallback;
  return value.length > 96 ? `${value.slice(0, 96)}...` : value;
};

const defaultWorkflowYaml = `name: Loom 示例流程
description: Loom 桌面端示例工作流
nodes:
  - id: prompt
    uses: fixture-script-art
    with:
      text: hello loom
`;

const defaultCurlCommand = `curl -X POST http://127.0.0.1:8765/v1/tools/fixture-cloud/execute -H "Content-Type: application/json" -d '{"prompt":"hello loom","strength":0.75}'`;

const defaultResponseSample = `{
  "image_url": "https://example.local/result.png",
  "seed": 12345
}`;

const HOOK_LIVE_WORKFLOW_ID = "hook-live";

const isHookLiveWorkflow = (workflow: Pick<LoomWorkflowMetadata, "id">) =>
  workflow.id === HOOK_LIVE_WORKFLOW_ID || workflow.id === "arthook-live";

const workflowDisplayName = (workflow: LoomWorkflowMetadata) => {
  if (isHookLiveWorkflow(workflow)) {
    return workflow.name && workflow.name !== workflow.id ? workflow.name : "Hook 实时工作流";
  }
  return workflow.name || workflow.id;
};

type StudioMessageKind = "info" | "error";

interface StudioMessage {
  kind: StudioMessageKind;
  text: string;
}

interface ResponseTemplatePreview {
  templatedJson: string;
  ports: ParsedPort[];
}

interface GraphNodeDraft {
  id: string;
  uses: string;
  needsText: string;
  withText: string;
}

type ArtWizardMode = string;

interface ArtWizardModeDescriptor {
  id: ArtWizardMode;
  title: string;
  subtitle: string;
  executionLabel: string;
}

const artWizardModes: ArtWizardModeDescriptor[] = [
  {
    id: "cli_wrapper",
    title: "CLI 包装 Art",
    subtitle: "把本地命令封装成 Art。",
    executionLabel: "cli_wrapper",
  },
  {
    id: "cloud_api",
    title: "云 API Art",
    subtitle: "把 REST/云接口封装成 Art。",
    executionLabel: "cloud_api",
  },
  {
    id: "script",
    title: "脚本 / Python Art",
    subtitle: "注册本地 Python 脚本。",
    executionLabel: "script",
  },
  {
    id: "mcp",
    title: "MCP 关联 Art",
    subtitle: "绑定已配置的 MCP 工具。",
    executionLabel: "mcp",
  },
  {
    id: "python_art",
    title: "已安装 Python Art",
    subtitle: "导入本地服务发现的 Python Art。",
    executionLabel: "python_art",
  },
  {
    id: "workflow",
    title: "工作流 Art",
    subtitle: "把已保存工作流变成可复用 Art 节点。",
    executionLabel: "workflow",
  },
  {
    id: "native_image",
    title: "原生图像 Art",
    subtitle: "注册图像路径/base64/buffer 工具。",
    executionLabel: "image_path / image_base64 / image_buffer",
  },
];

const artModeById = (mode: ArtWizardMode, modes = artWizardModes) =>
  modes.find((item) => item.id === mode) ?? modes[0] ?? artWizardModes[0];

const workflowToolId = (workflowId: string) => {
  const normalized = workflowId.trim().replace(/[^a-zA-Z0-9_-]/g, "-").replace(/^-+|-+$/g, "");
  return `${normalized || "workflow"}-tool`;
};

const createNodeDraft = (node: WorkflowStudioNode | null): GraphNodeDraft => ({
  id: node?.id ?? "",
  uses: node?.uses ?? "",
  needsText: node?.needs.join(", ") ?? "",
  withText: node ? Object.entries(node.with).map(([key, value]) => `${key}: ${value}`).join("\n") : "",
});

const parseNeedsText = (value: string) =>
  value
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);

const parseWithText = (value: string) =>
  value.split(/\r?\n/).reduce<Record<string, string>>((fields, line) => {
    const trimmed = line.trim();
    if (!trimmed) return fields;
    const separatorIndex = trimmed.includes(":") ? trimmed.indexOf(":") : trimmed.indexOf("=");
    if (separatorIndex <= 0) return fields;
    const key = trimmed.slice(0, separatorIndex).trim();
    const fieldValue = trimmed.slice(separatorIndex + 1).trim();
    if (key) fields[key] = fieldValue;
    return fields;
  }, {});

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

const openHookLiveWorkflow = (
  setWorkflowOpenRequest: Dispatch<SetStateAction<WorkflowOpenRequest | null>>,
  setActiveSection: Dispatch<SetStateAction<SectionId>>,
  selectedNodeId?: string,
) => {
  setWorkflowOpenRequest(null);
  window.setTimeout(() => {
    setWorkflowOpenRequest({ workflowId: HOOK_LIVE_WORKFLOW_ID, selectedNodeId });
    setActiveSection("workflows");
  }, 0);
};

function StatusPill({ snapshot }: { snapshot: LoomSnapshot }) {
  const online = snapshot.connectionState === "online";
  return (
    <span className={online ? "status-pill status-pill--online" : "status-pill status-pill--offline"}>
      <span className="status-dot" />
      {online ? "本地服务在线" : "本地服务离线"}
    </span>
  );
}

function ModuleCard({ module }: { module: LoomModuleStatus }) {
  return (
    <article className="glass-card module-card">
      <div>
        <p className="card-kicker">模块</p>
        <h3>{module.name}</h3>
      </div>
      <span className="mini-chip">{module.status}</span>
      {module.detail ? <p>{module.detail}</p> : null}
    </article>
  );
}

function CapabilityCard({ capability }: { capability: LoomCapability }) {
  return (
    <article className="glass-card capability-card">
      <p className="card-kicker">能力</p>
      <h3>{capability.id}</h3>
      <p>{firstWords(capability.description, "本地能力")}</p>
      <span className="mini-chip">{capability.mode || "run"}</span>
    </article>
  );
}

function OverviewPanel({
  snapshot,
  refresh,
  startLocalService,
  localServiceBusy,
  localServiceMessage,
}: {
  snapshot: LoomSnapshot;
  refresh: () => void;
  startLocalService: () => void;
  localServiceBusy: boolean;
  localServiceMessage: StudioMessage | null;
}) {
  const modules = snapshot.status?.modules ?? [];
  return (
    <section className="content-grid">
      <div className="hero-panel">
        <div>
          <p className="section-kicker">本地工作台</p>
          <h1>Loom 桌面端</h1>
          <div className="hero-actions">
            <button className="signal-button" type="button" onClick={startLocalService} disabled={localServiceBusy}>
              {localServiceBusy ? "启动中" : "启动 Loom 本地服务"}
            </button>
            <button className="signal-button" type="button" onClick={refresh}>
              刷新本地服务
            </button>
            <button className="ghost-button" type="button" onClick={() => openExternal(snapshot.settings.root)}>
              打开设置
            </button>
          </div>
        </div>
        <div className="hero-status-card">
          <StatusPill snapshot={snapshot} />
          <strong>{snapshot.baseUrl}</strong>
          <span>检查时间：{formatTime(snapshot.checkedAt)}</span>
          {snapshot.error ? <p className="error-text">{snapshot.error}</p> : <p>健康状态：{snapshot.health?.status ?? "ok"}</p>}
          {localServiceMessage ? (
            <p className={localServiceMessage.kind === "error" ? "error-text" : "success-text"}>
              {localServiceMessage.text}
            </p>
          ) : null}
        </div>
      </div>

      <div className="stat-row">
        <div className="stat-card">
          <span>模块</span>
          <strong>{modules.length}</strong>
        </div>
        <div className="stat-card">
          <span>能力</span>
          <strong>{snapshot.capabilities.length}</strong>
        </div>
        <div className="stat-card">
          <span>运行时</span>
          <strong>{snapshot.status?.status ?? "offline"}</strong>
        </div>
      </div>

      <div className="card-grid">
        {modules.length ? modules.map((module) => <ModuleCard key={module.name} module={module} />) : (
          <article className="glass-card empty-card">
            <h3>暂无模块状态</h3>
            <p>启动 Loom 本地服务后会显示。</p>
          </article>
        )}
      </div>
    </section>
  );
}

function WorkflowStudioPanel({
  snapshot,
  refresh,
  hookCanvas,
  refreshHookCanvas,
  workflowOpenRequest,
  onWorkflowOpenRequestHandled,
}: {
  snapshot: LoomSnapshot;
  refresh: () => Promise<void>;
  hookCanvas: HookCanvasSnapshot | null;
  refreshHookCanvas: (baseUrl?: string) => Promise<void>;
  workflowOpenRequest: WorkflowOpenRequest | null;
  onWorkflowOpenRequestHandled: () => void;
}) {
  const [workflowId, setWorkflowId] = useState("studio-sample-flow");
  const [workflowName, setWorkflowName] = useState("Loom 示例流程");
  const [workflowYaml, setWorkflowYaml] = useState(defaultWorkflowYaml);
  const [curlCommand, setCurlCommand] = useState(defaultCurlCommand);
  const [responseSample, setResponseSample] = useState(defaultResponseSample);
  const [curlPreview, setCurlPreview] = useState<CurlImportResult | null>(null);
  const [responsePreview, setResponsePreview] = useState<ResponseTemplatePreview | null>(null);
  const [inference, setInference] = useState<WorkflowInterfaceInference | null>(null);
  const [message, setMessage] = useState<StudioMessage | null>(null);
  const [busy, setBusy] = useState(false);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedCanvasNodeId, setSelectedCanvasNodeId] = useState<string | null>(null);
  const [nodeDraft, setNodeDraft] = useState<GraphNodeDraft>(() => createNodeDraft(null));
  const [canvasExecutionBusy, setCanvasExecutionBusy] = useState(false);
  const [canvasExecutionMessage, setCanvasExecutionMessage] = useState<StudioMessage | null>(null);

  const workflowGraph = useMemo(() => parseWorkflowYamlLite(workflowYaml), [workflowYaml]);
  const toolById = useMemo(
    () => new Map(snapshot.tools.map((tool) => [tool.id, tool])),
    [snapshot.tools],
  );
  const selectedGraphNode = useMemo(
    () => workflowGraph.nodes.find((node) => node.id === selectedNodeId) ?? workflowGraph.nodes[0] ?? null,
    [workflowGraph.nodes, selectedNodeId],
  );
  const selectedCanvasNode = useMemo(
    () => hookCanvas?.nodes.find((node) => node.id === selectedCanvasNodeId) ?? null,
    [hookCanvas, selectedCanvasNodeId],
  );
  const selectedCanvasTool = useMemo(
    () => (selectedCanvasNode?.artId ? toolById.get(selectedCanvasNode.artId) : undefined),
    [selectedCanvasNode, toolById],
  );
  const generatedToolId = workflowToolId(workflowId);

  useEffect(() => {
    if (!workflowGraph.nodes.length) {
      if (selectedNodeId !== null) setSelectedNodeId(null);
      return;
    }
    if (!selectedNodeId || !workflowGraph.nodes.some((node) => node.id === selectedNodeId)) {
      setSelectedNodeId(workflowGraph.nodes[0].id);
    }
  }, [selectedNodeId, workflowGraph.nodes]);

  useEffect(() => {
    setNodeDraft(createNodeDraft(selectedGraphNode));
  }, [selectedGraphNode]);

  useEffect(() => {
    if (!hookCanvas) {
      if (selectedCanvasNodeId !== null) setSelectedCanvasNodeId(null);
      return;
    }
    const retained = hookCanvas.nodes.some((node) => node.id === selectedCanvasNodeId)
      ? selectedCanvasNodeId
      : null;
    if (retained !== selectedCanvasNodeId) setSelectedCanvasNodeId(retained);
  }, [hookCanvas, selectedCanvasNodeId]);

  useEffect(() => {
    setCanvasExecutionMessage(null);
  }, [selectedCanvasNodeId]);

  const runSmartImport = () => {
    const parsedCurl = parseCurlCommand(curlCommand);
    if (!parsedCurl) {
      setCurlPreview(null);
      setResponsePreview(null);
      setMessage({ kind: "error", text: "智能导入需要以 curl 开头的命令。" });
      return;
    }

    setCurlPreview(parsedCurl);
    setResponsePreview(autoTemplateResponse(responseSample));
    setMessage({ kind: "info", text: "智能导入已解析请求和输出端口。" });
  };

  const runInterfaceInference = () => {
    const nextInference = inferWorkflowArtInterface(workflowGraph, snapshot.tools);
    setInference(nextInference);
    setMessage({
      kind: "info",
      text: `已推断接口：输入 ${nextInference.inputs.length}，输出 ${nextInference.outputs.length}。`,
    });
  };

  const saveWorkflow = async () => {
    if (!workflowId.trim()) {
      setMessage({ kind: "error", text: "保存前需要工作流 ID。" });
      return;
    }

    setBusy(true);
    try {
      await saveWorkflowBundle(snapshot.baseUrl, { id: workflowId.trim() }, workflowYaml);
      setMessage({ kind: "info", text: `已保存工作流 ${workflowId.trim()}。` });
      await refresh();
    } catch (error) {
      setMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法保存工作流。",
      });
    } finally {
      setBusy(false);
    }
  };

  const wrapWorkflowAsTool = async () => {
    if (!workflowId.trim()) {
      setMessage({ kind: "error", text: "封装成工具前需要工作流 ID。" });
      return;
    }

    const nextInference = inferWorkflowArtInterface(workflowGraph, snapshot.tools);
    setInference(nextInference);
    const toolName = workflowName.trim() || workflowGraph.name || workflowId.trim();
    const tool: LoomToolDefinition = {
      id: generatedToolId,
      name: `${toolName} 工具`,
      description: "由工作流工作台生成的 Loom 工具。",
      enabled: true,
      execution: {
        type: "workflow",
        workflowId: workflowId.trim(),
        workflowBindings: nextInference.bindings,
      },
      inputs: nextInference.inputs.map((input) => ({
        name: input.name,
        label: input.label,
        type: input.type,
        executionType: input.executionType,
        default: input.default,
      })),
      outputs: nextInference.outputs.map((output) => ({
        name: output.name,
        label: output.label,
        type: output.type,
        executionType: output.executionType,
      })),
    };

    setBusy(true);
    try {
      await saveWorkflowBundle(snapshot.baseUrl, { id: workflowId.trim() }, workflowYaml);
      await saveToolDefinition(snapshot.baseUrl, tool);
      setMessage({ kind: "info", text: `已把 ${workflowId.trim()} 封装为工具 ${tool.id}。` });
      await refresh();
    } catch (error) {
      setMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法封装工作流工具。",
      });
    } finally {
      setBusy(false);
    }
  };

  const loadWorkflowById = async (targetWorkflowId: string, requestedNodeId?: string) => {
    setBusy(true);
    setWorkflowId(targetWorkflowId);
    if (isHookLiveWorkflow({ id: targetWorkflowId })) {
      setWorkflowName("Hook 实时工作流");
      setSelectedCanvasNodeId(requestedNodeId ?? null);
    }
    try {
      const bundle = await getWorkflowBundle(snapshot.baseUrl, targetWorkflowId);
      setWorkflowId(bundle.id);
      setWorkflowName(bundle.name || bundle.id);
      setWorkflowYaml(bundle.data);
      setSelectedNodeId(null);
      setMessage({
        kind: "info",
        text: isHookLiveWorkflow(bundle)
          ? "已加载 Hook 实时工作流。"
          : `已加载工作流 ${bundle.id}。`,
      });
    } catch (error) {
      setMessage(isHookLiveWorkflow({ id: targetWorkflowId })
        ? { kind: "info", text: "Hook 画布已打开；工作流定义尚未持久化。" }
        : {
            kind: "error",
            text: error instanceof Error ? error.message : "无法加载工作流。",
          });
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!workflowOpenRequest) return;
    void loadWorkflowById(
      workflowOpenRequest.workflowId,
      workflowOpenRequest.selectedNodeId,
    ).finally(onWorkflowOpenRequestHandled);
  }, [workflowOpenRequest, onWorkflowOpenRequestHandled]);

  const loadSavedWorkflow = async (workflow: LoomWorkflowMetadata) => {
    await loadWorkflowById(workflow.id);
  };

  const executeSelectedCanvasNode = async (selectedIndex?: number) => {
    if (!selectedCanvasNode || !selectedCanvasTool) {
      setCanvasExecutionMessage({ kind: "error", text: "请先选择一个可执行的 Hook Art 节点。" });
      return;
    }
    if (!canExecuteHookCanvasNodeManually(selectedCanvasNode, selectedCanvasTool)) {
      setCanvasExecutionMessage({
        kind: "error",
        text: "当前节点依赖上游图像输入，暂不支持在 Loom 桌面中直接手工执行。",
      });
      return;
    }

    setCanvasExecutionBusy(true);
    try {
      const request = buildImageSearchExecutionRequest(selectedCanvasNode, selectedIndex);
      if (typeof selectedIndex === "number") {
        await updateArtLoomWorkflowNode(snapshot.baseUrl, {
          workflowId: hookCanvas?.workflowId ?? workflowId,
          nodeId: selectedCanvasNode.id,
          param: "result_index",
          value: Math.floor(selectedIndex),
        });
      }
      const response = await executeArtLoomArtNode(snapshot.baseUrl, request);
      if (response.type !== "success") {
        throw new Error(artLoomExecuteArtNodeErrorMessage(response) || "节点执行失败。");
      }
      await refreshHookCanvas(snapshot.baseUrl);
      setCanvasExecutionMessage({
        kind: "info",
        text: selectedIndex === undefined
          ? `已执行节点 ${selectedCanvasNode.label || selectedCanvasNode.id}。`
          : `已切换到搜索结果 ${selectedIndex + 1} 并重新执行节点。`,
      });
    } catch (error) {
      setCanvasExecutionMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法执行选中的 Hook Art 节点。",
      });
    } finally {
      setCanvasExecutionBusy(false);
    }
  };

  const removeSavedWorkflow = async (workflow: LoomWorkflowMetadata) => {
    setBusy(true);
    try {
      await deleteWorkflowBundle(snapshot.baseUrl, workflow.id);
      setMessage({ kind: "info", text: `已删除工作流 ${workflow.name || workflow.id}。` });
      await refresh();
    } catch (error) {
      setMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法删除工作流。",
      });
    } finally {
      setBusy(false);
    }
  };

  const applyNodeChanges = () => {
    if (!selectedGraphNode) {
      setMessage({ kind: "error", text: "请先选择工作流节点。" });
      return;
    }
    const nextGraph = updateWorkflowGraphNode(workflowGraph, selectedGraphNode.id, {
      id: nodeDraft.id,
      uses: nodeDraft.uses,
      needs: parseNeedsText(nodeDraft.needsText),
      with: parseWithText(nodeDraft.withText),
    });
    setWorkflowYaml(serializeWorkflowGraphLite(nextGraph));
    setSelectedNodeId(nodeDraft.id.trim() || selectedGraphNode.id);
    setMessage({ kind: "info", text: `已更新节点 ${selectedGraphNode.id}。` });
  };

  const addGraphNode = () => {
    const defaultToolId = snapshot.tools[0]?.id ?? "";
    const nextGraph = addWorkflowGraphNode(workflowGraph, {
      uses: defaultToolId,
      needs: selectedGraphNode ? [selectedGraphNode.id] : [],
      with: {},
    });
    const addedNode = nextGraph.nodes[nextGraph.nodes.length - 1] ?? null;
    setWorkflowYaml(serializeWorkflowGraphLite(nextGraph));
    setSelectedNodeId(addedNode?.id ?? null);
    setMessage({ kind: "info", text: `已创建节点 ${addedNode?.id ?? "new"}。` });
  };

  const deleteSelectedGraphNode = () => {
    if (!selectedGraphNode) {
      setMessage({ kind: "error", text: "请先选择要删除的节点。" });
      return;
    }
    const nextGraph = deleteWorkflowGraphNode(workflowGraph, selectedGraphNode.id);
    setWorkflowYaml(serializeWorkflowGraphLite(nextGraph));
    setSelectedNodeId(nextGraph.nodes[0]?.id ?? null);
    setMessage({ kind: "info", text: `已删除节点 ${selectedGraphNode.id}。` });
  };

  const graphEdgeCount = workflowGraph.nodes.reduce((count, node) => count + node.needs.length, 0);

  const addArtNodeFromTool = (tool: LoomToolDefinition) => {
    const nextGraph = addWorkflowGraphNode(workflowGraph, {
      uses: tool.id,
      needs: selectedGraphNode ? [selectedGraphNode.id] : [],
      with: {},
    });
    const addedNode = nextGraph.nodes[nextGraph.nodes.length - 1] ?? null;
    setWorkflowYaml(serializeWorkflowGraphLite(nextGraph));
    setSelectedNodeId(addedNode?.id ?? null);
    setMessage({ kind: "info", text: `已添加 Art 节点 ${addedNode?.id ?? "new"}，工具：${tool.name || tool.id}。` });
  };

  const selectCanvasNode = (nodeId: string) => {
    setSelectedCanvasNodeId(nodeId);
    if (workflowGraph.nodes.some((node) => node.id === nodeId)) {
      setSelectedNodeId(nodeId);
    }
  };

  return (
    <section className="content-grid workflow-studio">
      <div className="main-board studio-hero">
        <p className="section-kicker">工作流</p>
        <h2>工作流工作台</h2>
        <div className="studio-actions">
          <button className="signal-button" type="button" onClick={saveWorkflow} disabled={busy}>
            {busy ? "保存中" : "保存工作流"}
          </button>
          <button className="ghost-button" type="button" onClick={runInterfaceInference}>
            推断工作流接口
          </button>
          <button className="ghost-button" type="button" onClick={wrapWorkflowAsTool} disabled={busy}>
            封装为 Loom 工具
          </button>
        </div>
        {message ? <p className={message.kind === "error" ? "error-text" : "success-text"}>{message.text}</p> : null}
      </div>

      {isHookLiveWorkflow({ id: workflowId }) && hookCanvas ? (
        <HookCanvasView
          snapshot={hookCanvas}
          baseUrl={snapshot.baseUrl}
          selectedNodeId={selectedCanvasNodeId}
          onSelectNode={selectCanvasNode}
          selectedNodeCanExecute={Boolean(
            selectedCanvasNode && selectedCanvasTool &&
              canExecuteHookCanvasNodeManually(selectedCanvasNode, selectedCanvasTool),
          )}
          executionBusy={canvasExecutionBusy}
          executionMessage={canvasExecutionMessage}
          onExecuteSelectedNode={() => void executeSelectedCanvasNode()}
          onSelectResultCandidate={(index) => void executeSelectedCanvasNode(index)}
        />
      ) : null}

      <div className="studio-grid">
        <details
          className="advanced-technical-information advanced-technical-information--studio"
          data-testid="advanced-technical-information"
        >
          <summary>高级技术信息 · YAML 源定义</summary>
          <article className="glass-card studio-card studio-card--wide">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">YAML 编辑</p>
              <h3>工作流定义</h3>
            </div>
            <span className="mini-chip">{workflowGraph.nodes.length} 个节点</span>
          </div>
          <label className="field-label">
            工作流 ID
            <input
              className="studio-input"
              value={workflowId}
              onChange={(event) => setWorkflowId(event.target.value)}
              placeholder="my-loom-flow"
            />
          </label>
          <label className="field-label">
            生成工具名称
            <input
              className="studio-input"
              value={workflowName}
              onChange={(event) => setWorkflowName(event.target.value)}
              placeholder={workflowGraph.name}
            />
          </label>
          <label className="field-label">
            工作流 YAML
            <textarea
              className="studio-textarea studio-textarea--yaml"
              value={workflowYaml}
              onChange={(event) => setWorkflowYaml(event.target.value)}
              spellCheck={false}
            />
          </label>
          <div className="terminal-list">
            <span>PUT /v1/workflows/{workflowId.trim() || "workflow-id"}</span>
            <span>生成工具 ID: {generatedToolId}</span>
          </div>
          </article>
        </details>

        <article className="glass-card studio-card workflow-graph-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">可视化编辑</p>
              <h3>图形视图 / Art 节点画布</h3>
            </div>
            <span className="mini-chip">{graphEdgeCount} 条边</span>
          </div>
          <div className="art-node-palette">
            <div className="control-card__head">
              <div>
                <p className="card-kicker">Art 节点面板</p>
                <h3>注册表工具</h3>
              </div>
              <span className="mini-chip">{snapshot.tools.length}</span>
            </div>
            <div className="palette-list">
              {snapshot.tools.length ? snapshot.tools.slice(0, 8).map((tool) => (
                <button
                  className="palette-art"
                  type="button"
                  key={tool.id}
                  onClick={() => addArtNodeFromTool(tool)}
                >
                  <span>
                    <strong>{tool.name || tool.id}</strong>
                    <small>{tool.execution?.type || "工具"} · {tool.id}</small>
                  </span>
                  <em>添加 Art 节点</em>
                </button>
              )) : (
                <div className="empty-card">
                  <p>暂无注册表工具，请先添加 Art。</p>
                  <button className="ghost-button" type="button" disabled>
                    添加 Art 节点
                  </button>
                </div>
              )}
            </div>
          </div>
          <div className="workflow-graph">
            {workflowGraph.nodes.length ? workflowGraph.nodes.map((node, index) => {
              const nodeTool = toolById.get(node.uses);
              return (
              <button
                className={
                  selectedGraphNode?.id === node.id
                    ? "workflow-node art-node-card workflow-node--selected"
                    : "workflow-node art-node-card"
                }
                key={node.id}
                type="button"
                onClick={() => setSelectedNodeId(node.id)}
              >
                <span className="workflow-node__index">{index + 1}</span>
                <span className="workflow-node__body">
                  <strong>Art 节点 · {node.id}</strong>
                  <small>{nodeTool?.name || node.uses || "未选择工具"}</small>
                  {node.needs.length ? <em>依赖 {node.needs.join(", ")}</em> : <em>入口节点</em>}
                  <span className="art-node-preview">预览</span>
                  <span className="port-summary">
                    <b>输入 {(nodeTool?.inputs || []).length || "自动"}</b>
                    <b>输出 {(nodeTool?.outputs || []).length || "自动"}</b>
                    <b>参数 {Object.keys(node.with).length}</b>
                    <b>结果 {nodeTool?.execution?.type || "空闲"}</b>
                  </span>
                </span>
              </button>
              );
            }) : (
              <div className="empty-card">暂无工作流节点。</div>
            )}
          </div>
          <div className="workflow-edges">
            {workflowGraph.nodes.flatMap((node) => (
              node.needs.map((neededId) => (
                <span className="method-chip marketplace-tag--neutral" key={`${neededId}->${node.id}`}>
                  {neededId} -&gt; {node.id}
                </span>
              ))
            ))}
            {!graphEdgeCount ? <span className="method-chip marketplace-tag--neutral">单阶段流程</span> : null}
          </div>
          <div className="node-properties-panel">
            <div className="control-card__head">
              <div>
                <p className="card-kicker">属性</p>
                <h3>节点属性</h3>
              </div>
              <span className="mini-chip">{selectedGraphNode?.id ?? "未选中"}</span>
            </div>
            <label className="field-label">
              节点 ID
              <input
                className="studio-input"
                value={nodeDraft.id}
                onChange={(event) => setNodeDraft((draft) => ({ ...draft, id: event.target.value }))}
                placeholder="step-id"
              />
            </label>
            <label className="field-label">
              使用工具
              <input
                className="studio-input"
                list="workflow-tool-options"
                value={nodeDraft.uses}
                onChange={(event) => setNodeDraft((draft) => ({ ...draft, uses: event.target.value }))}
                placeholder="tool-id"
              />
              <datalist id="workflow-tool-options">
                {snapshot.tools.map((tool) => <option key={tool.id} value={tool.id}>{tool.name}</option>)}
              </datalist>
            </label>
            <label className="field-label">
              依赖节点
              <textarea
                className="studio-textarea studio-textarea--compact"
                value={nodeDraft.needsText}
                onChange={(event) => setNodeDraft((draft) => ({ ...draft, needsText: event.target.value }))}
                placeholder="previous-node, another-node"
                spellCheck={false}
              />
            </label>
            <label className="field-label">
              参数字段
              <textarea
                className="studio-textarea studio-textarea--compact"
                value={nodeDraft.withText}
                onChange={(event) => setNodeDraft((draft) => ({ ...draft, withText: event.target.value }))}
                placeholder="prompt: hello loom"
                spellCheck={false}
              />
            </label>
            <div className="studio-actions">
              <button className="signal-button" type="button" onClick={applyNodeChanges} disabled={!selectedGraphNode}>
                应用节点修改
              </button>
              <button className="ghost-button" type="button" onClick={addGraphNode}>
                添加节点
              </button>
              <button className="ghost-button" type="button" onClick={deleteSelectedGraphNode} disabled={!selectedGraphNode}>
                删除节点
              </button>
            </div>
          </div>
        </article>

        <details
          className="advanced-technical-information advanced-technical-information--studio"
          data-testid="advanced-technical-information"
        >
          <summary>高级技术信息 · 导入与绑定</summary>
          <div className="advanced-technical-information__body">
        <article className="glass-card studio-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">智能导入</p>
              <h3>cURL 与响应模板</h3>
            </div>
          </div>
          <label className="field-label">
            cURL 请求
            <textarea
              className="studio-textarea"
              value={curlCommand}
              onChange={(event) => setCurlCommand(event.target.value)}
              spellCheck={false}
            />
          </label>
          <label className="field-label">
            响应示例
            <textarea
              className="studio-textarea"
              value={responseSample}
              onChange={(event) => setResponseSample(event.target.value)}
              spellCheck={false}
            />
          </label>
          <button className="signal-button" type="button" onClick={runSmartImport}>
            智能导入
          </button>
          {curlPreview ? (
            <pre className="studio-json">
              {JSON.stringify(
                {
                  method: curlPreview.method,
                  url: curlPreview.url,
                  headers: curlPreview.headers,
                  body: curlPreview.body,
                  suggestedInputs: curlPreview.suggestedInputs,
                  templatedResponse: responsePreview?.templatedJson ?? "",
                  responseOutputs: responsePreview?.ports ?? [],
                },
                null,
                2,
              )}
            </pre>
          ) : null}
        </article>

        <article className="glass-card studio-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">接口</p>
              <h3>工作流绑定</h3>
            </div>
            <span className="mini-chip">{snapshot.tools.length} 个工具</span>
          </div>
          <div className="studio-actions">
            <button className="ghost-button" type="button" onClick={runInterfaceInference}>
              推断工作流接口
            </button>
            <button className="ghost-button" type="button" onClick={() => openExternal(`${snapshot.baseUrl}/v1/tools`)}>
              查看注册表
            </button>
          </div>
          {inference ? (
            <pre className="studio-json">
              {JSON.stringify(
                {
                  inputs: inference.inputs,
                  outputs: inference.outputs,
                  bindings: inference.bindings,
                  warnings: inference.warnings,
                },
                null,
                2,
              )}
            </pre>
          ) : (
            <div className="empty-card">
              运行接口推断后预览输入、输出和 workflowBindings。
            </div>
          )}
        </article>
          </div>
        </details>

        <article className="glass-card studio-card studio-card--wide">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">工作流管理</p>
              <h3>已保存工作流</h3>
            </div>
            <span className="mini-chip">{snapshot.workflows.length} 个</span>
          </div>
          <div className="studio-list">
            {snapshot.workflows.length ? snapshot.workflows.map((workflow) => (
              <div
                className={isHookLiveWorkflow(workflow) ? "studio-list-item studio-list-item--live" : "studio-list-item"}
                key={workflow.id}
              >
                <span>{workflowDisplayName(workflow)}</span>
                <small>{workflow.id} / {workflow.nodeCount ?? 0} 个节点</small>
                <div className="studio-actions">
                  <button
                    className="ghost-button"
                    type="button"
                    onClick={() => loadSavedWorkflow(workflow)}
                    disabled={busy}
                  >
                    {isHookLiveWorkflow(workflow) ? "打开 Hook 工作流" : "打开工作流"}
                  </button>
                  <button
                    className="ghost-button"
                    type="button"
                    onClick={() => removeSavedWorkflow(workflow)}
                    disabled={busy}
                  >
                    删除工作流
                  </button>
                </div>
              </div>
            )) : (
              <div className="empty-card">
                本地服务暂无已保存工作流。
              </div>
            )}
          </div>
        </article>
      </div>
    </section>
  );
}

function EnabledChip({ enabled }: { enabled?: boolean }) {
  return <span className="mini-chip">{enabled === false ? "已禁用" : "已启用"}</span>;
}

interface ArtWizardSubmitDraft {
  mode: ArtWizardMode;
  frameworkValues: Record<string, unknown>;
  toolId: string;
  name: string;
  description: string;
  command: string;
  argsText: string;
  endpoint: string;
  method: string;
  contentType: string;
  headersText: string;
  bodyText: string;
  scriptPath: string;
  mcpServerId: string;
  mcpToolName: string;
  pythonArtId: string;
  workflowId: string;
  nativeFilter: string;
  inputPorts: ArtWizardPortDraft[];
  outputPorts: ArtWizardPortDraft[];
  shaderMode: boolean;
}

type ArtPortCaptureMode = "explicit_path" | "fixed_filename" | "derived_template" | "stdout";

interface ArtWizardPortDraft {
  name: string;
  label: string;
  type: string;
  executionType: string;
  defaultValue: string;
  disabled: boolean;
  jsonPath: string;
  captureMode: ArtPortCaptureMode;
  filename: string;
  originalValue: string;
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
    name: overrides.name || (direction === "input" ? "input" : "result"),
    label: overrides.label || (direction === "input" ? "输入" : "结果"),
    type,
    executionType: overrides.executionType || defaultExecutionTypeForPort(type, direction),
    defaultValue: overrides.defaultValue || "",
    disabled: overrides.disabled ?? false,
    jsonPath: overrides.jsonPath || "",
    captureMode: overrides.captureMode || "explicit_path",
    filename: overrides.filename || "",
    originalValue: overrides.originalValue || "",
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

const defaultWizardPorts = (mode: ArtWizardMode) => {
  switch (mode) {
    case "cli_wrapper":
      return {
        inputs: [createPortDraft("input", { name: "input", label: "输入", type: "file", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "file", executionType: "image_path" })],
      };
    case "cloud_api":
      return {
        inputs: [createPortDraft("input", { name: "image", label: "图像", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "image", executionType: "image_path" })],
      };
    case "script":
      return {
        inputs: [createPortDraft("input", { name: "input_path", label: "输入路径", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "output_path", label: "输出路径", type: "image", executionType: "image_path" })],
      };
    case "mcp":
      return {
        inputs: [createPortDraft("input", { name: "arguments", label: "参数", type: "string", executionType: "string" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "string", executionType: "string" })],
      };
    case "python_art":
      return {
        inputs: [createPortDraft("input", { name: "input", label: "输入", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "image", executionType: "image_path" })],
      };
    case "workflow":
      return {
        inputs: [createPortDraft("input", { name: "input", label: "工作流输入", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "工作流结果", type: "image", executionType: "image_path" })],
      };
    case "native_image":
      return {
        inputs: [createPortDraft("input", { name: "image", label: "图像", type: "image", executionType: "image_path" })],
        outputs: [createPortDraft("output", { name: "result", label: "结果", type: "image", executionType: "image_path" })],
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
  if (direction === "input") {
    if (port.defaultValue.trim()) next.default = port.defaultValue;
    if (port.disabled) next.disabled = true;
  } else {
    next.captureMode = port.captureMode;
    if (port.jsonPath.trim()) next.jsonPath = port.jsonPath.trim();
    if (port.filename.trim()) next.filename = port.filename.trim();
    if (port.originalValue.trim()) next.originalValue = port.originalValue.trim();
  }
  return next;
};

function FrameworkAuthoringFieldInput({
  field,
  value,
  onChange,
}: {
  field: LoomFrameworkAuthoringField;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
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
        type={field.secret || field.type === "secret" ? "password" : field.type === "number" ? "number" : "text"}
        value={typeof value === "number" || typeof value === "string" ? value : ""}
        min={field.minimum ?? undefined}
        max={field.maximum ?? undefined}
        step={field.step ?? undefined}
        placeholder={field.placeholder ?? undefined}
        onChange={(event) => onChange(field.type === "number" ? Number(event.target.value) : event.target.value)}
      />
      {field.secret || field.type === "secret" ? (
        <small>填写凭据名称；实际密钥由 Loom 凭据代理注入，不写入 Art 包。</small>
      ) : null}
    </label>
  );
}

function AddArtWizard({
  baseUrl,
  frameworks,
  mcpServers,
  pythonArts,
  workflows,
  busy,
  onCreate,
}: {
  baseUrl: string;
  frameworks: LoomFramework[];
  mcpServers: LoomMcpServer[];
  pythonArts: LoomPythonArt[];
  workflows: LoomWorkflowMetadata[];
  busy: boolean;
  onCreate: (draft: ArtWizardSubmitDraft) => Promise<void>;
}) {
  const [mode, setMode] = useState<ArtWizardMode>("mcp");
  const [frameworkValues, setFrameworkValues] = useState<Record<string, unknown>>({});
  const [toolId, setToolId] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [command, setCommand] = useState("npx");
  const [argsText, setArgsText] = useState("");
  const [endpoint, setEndpoint] = useState("http://127.0.0.1:8765/v1/shared-images/convert");
  const [method, setMethod] = useState("POST");
  const [contentType, setContentType] = useState("application/json");
  const [headersText, setHeadersText] = useState("Content-Type: application/json");
  const [bodyText, setBodyText] = useState('{"image":"{{inputs.image.path}}"}');
  const [scriptPath, setScriptPath] = useState("");
  const [mcpServerId, setMcpServerId] = useState("");
  const [mcpToolName, setMcpToolName] = useState("");
  const [pythonArtId, setPythonArtId] = useState("");
  const [workflowId, setWorkflowId] = useState("");
  const [nativeFilter, setNativeFilter] = useState("identity");
  const [rawCommandText, setRawCommandText] = useState("ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}");
  const [cloudCurlText, setCloudCurlText] = useState(defaultCurlCommand);
  const [cloudResponseText, setCloudResponseText] = useState(defaultResponseSample);
  const [inputPorts, setInputPorts] = useState<ArtWizardPortDraft[]>(() => defaultWizardPorts("mcp").inputs);
  const [outputPorts, setOutputPorts] = useState<ArtWizardPortDraft[]>(() => defaultWizardPorts("mcp").outputs);
  const [shaderMode, setShaderMode] = useState(false);
  const [wizardMessage, setWizardMessage] = useState<StudioMessage | null>(null);
  const [mcpTools, setMcpTools] = useState<unknown[]>([]);
  const [selectedMcpSchemaToolName, setSelectedMcpSchemaToolName] = useState("");
  const [mcpDiscoveryBusy, setMcpDiscoveryBusy] = useState(false);
  const availableModes = useMemo<ArtWizardModeDescriptor[]>(() => {
    const dynamic = frameworks
      .filter((framework) => framework.installed && framework.enabled && framework.authoringSchema)
      .map((framework) => ({
        id: framework.qualifiedId || framework.id,
        title: framework.authoringSchema?.title || framework.name,
        subtitle: framework.authoringSchema?.description || framework.description,
        executionLabel: framework.qualifiedId || framework.id,
      }));
    if (!dynamic.length) return artWizardModes;
    const native = artWizardModes.find((item) => item.id === "native_image");
    return native ? [...dynamic, native] : dynamic;
  }, [frameworks]);
  const selectedMode = artModeById(mode, availableModes);
  const selectedFramework = frameworks.find(
    (framework) => (framework.qualifiedId || framework.id) === mode,
  );
  const selectedAuthoringSchema = selectedFramework?.authoringSchema ?? null;

  useEffect(() => {
    if (!mcpServerId && mcpServers[0]) setMcpServerId(mcpServers[0].id);
  }, [mcpServerId, mcpServers]);

  useEffect(() => {
    if (!pythonArtId && pythonArts[0]) setPythonArtId(pythonArts[0].art_id);
  }, [pythonArtId, pythonArts]);

  useEffect(() => {
    if (!workflowId && workflows[0]) setWorkflowId(workflows[0].id);
  }, [workflowId, workflows]);

  useEffect(() => {
    if (!availableModes.some((item) => item.id === mode) && availableModes[0]) {
      setMode(availableModes[0].id);
    }
  }, [availableModes, mode]);

  useEffect(() => {
    const defaults = defaultWizardPorts(mode);
    const schema = selectedFramework?.authoringSchema;
    setInputPorts(schema?.inputs?.map((port) => createPortDraft("input", {
      name: port.name,
      label: port.label,
      type: port.type,
      executionType: port.executionType,
    })) ?? defaults.inputs);
    setOutputPorts(schema?.outputs?.map((port) => createPortDraft("output", {
      name: port.name,
      label: port.label,
      type: port.type,
      executionType: port.executionType,
    })) ?? defaults.outputs);
    setFrameworkValues(defaultAuthoringValues(schema?.fields));
    setWizardMessage(null);
    setShaderMode(false);
  }, [mode, selectedFramework]);

  const mcpToolLabel = (tool: unknown) => {
    if (tool && typeof tool === "object" && !Array.isArray(tool)) {
      const record = tool as Record<string, unknown>;
      if (typeof record.name === "string" && record.name.trim()) return record.name.trim();
    }
    return "mcp_tool";
  };

  const updateInputPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setInputPorts((ports) => ports.map((port, portIndex) => portIndex === index ? { ...port, ...patch } : port));
  };

  const updateOutputPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setOutputPorts((ports) => ports.map((port, portIndex) => portIndex === index ? { ...port, ...patch } : port));
  };

  const importRawCommand = () => {
    const parsed = parseRawCommand(rawCommandText);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "请输入 CLI 命令。" });
      return;
    }
    setCommand(parsed.command);
    setArgsText(parsed.argsText);
    const parsedInputs = parsed.ports.filter((port) => port.isInput).map((port) => portDraftFromParsedPort(port, "input"));
    const parsedOutputs = parsed.ports.filter((port) => !port.isInput).map((port) => portDraftFromParsedPort(port, "output"));
    if (parsedInputs.length) setInputPorts(parsedInputs);
    if (parsedOutputs.length) setOutputPorts(parsedOutputs);
    setWizardMessage({ kind: "info", text: `已解析 ${parsed.args.length} 个参数。` });
  };

  const importCloudSmartTemplate = () => {
    const parsed = parseCurlCommand(cloudCurlText);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "云 API 智能导入需要 cURL 命令。" });
      return;
    }
    setEndpoint(parsed.url || endpoint);
    setMethod(parsed.method || "POST");
    const nextHeaders = Object.entries(parsed.headers).map(([key, value]) => `${key}: ${value}`).join("\n");
    if (nextHeaders) setHeadersText(nextHeaders);
    const nextContentType = parsed.headers["Content-Type"] || parsed.headers["content-type"];
    if (nextContentType) setContentType(nextContentType);
    if (parsed.body) setBodyText(parsed.body);
    if (parsed.suggestedInputs.length) {
      setInputPorts(parsed.suggestedInputs.map((port) => portDraftFromParsedPort(port, "input")));
    }
    const responsePreview = autoTemplateResponse(cloudResponseText);
    if (responsePreview.ports.length) {
      setOutputPorts(responsePreview.ports.map((port) => portDraftFromParsedPort(port, "output")));
    }
    setWizardMessage({ kind: "info", text: "已填充请求字段和输出端口。" });
  };

  const discoverMcpTools = async () => {
    const server = mcpServers.find((item) => item.id === mcpServerId);
    if (!server) {
      setWizardMessage({ kind: "error", text: "请先选择 MCP 服务。" });
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
        text: result.success === false
          ? result.error || "MCP 连接测试失败。"
          : `发现 ${tools.length} 个 MCP 工具。`,
      });
    } catch (error) {
      setWizardMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法发现 MCP 工具。",
      });
    } finally {
      setMcpDiscoveryBusy(false);
    }
  };

  const useSelectedMcpToolSchema = () => {
    const tool = mcpTools.find((item) => mcpToolLabel(item) === selectedMcpSchemaToolName);
    const parsed = portsFromMcpToolSchema(tool);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "选中的 MCP 工具没有 input_schema / inputSchema。" });
      return;
    }
    setMcpToolName(parsed.toolName);
    if (parsed.suggestedInputs.length) {
      setInputPorts(parsed.suggestedInputs.map((port) => portDraftFromParsedPort(port, "input")));
    }
    if (parsed.suggestedOutputs.length) {
      setOutputPorts(parsed.suggestedOutputs.map((port) => portDraftFromParsedPort(port, "output")));
    }
    setWizardMessage({
      kind: "info",
      text: `已导入 ${parsed.suggestedInputs.length} 个 input_schema 端口。`,
    });
  };

  const submit = async () => {
    await onCreate({
      mode,
      frameworkValues,
      toolId,
      name,
      description,
      command,
      argsText,
      endpoint,
      method,
      contentType,
      headersText,
      bodyText,
      scriptPath,
      mcpServerId,
      mcpToolName,
      pythonArtId,
      workflowId,
      nativeFilter,
      inputPorts,
      outputPorts,
      shaderMode,
    });
  };

  return (
    <section className="main-board add-art-wizard" aria-label="AddArtWizard">
      <div className="wizard-head">
        <div>
          <p className="section-kicker">添加 Art 向导</p>
          <h2>添加 Art</h2>
        </div>
        <span className="mini-chip">{selectedMode.executionLabel}</span>
      </div>

      <div className="art-mode-grid">
        {availableModes.map((item) => (
          <button
            className={mode === item.id ? "art-mode-card art-mode-card--active" : "art-mode-card"}
            type="button"
            key={item.id}
            onClick={() => setMode(item.id)}
          >
            <strong>{item.title}</strong>
            <span>{item.subtitle}</span>
            <small>{item.executionLabel}</small>
          </button>
        ))}
      </div>

      <div className="studio-grid add-art-grid">
        <article className="glass-card studio-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">基础信息</p>
              <h3>{selectedMode.title}</h3>
            </div>
            <span className="mini-chip">Art 节点</span>
          </div>
          <label className="field-label">
            工具 ID
            <input
              className="studio-input"
              value={toolId}
              onChange={(event) => setToolId(event.target.value)}
              placeholder={`${selectedFramework?.id || mode}-my-art`}
            />
          </label>
          <label className="field-label">
            Art 名称
            <input
              className="studio-input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={selectedMode.title}
            />
          </label>
          <label className="field-label">
            描述
            <textarea
              className="studio-textarea studio-textarea--compact"
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              placeholder={selectedMode.subtitle}
            />
          </label>
        </article>

        <article className="glass-card studio-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">执行配置</p>
              <h3>{selectedMode.executionLabel}</h3>
            </div>
          </div>

          {selectedAuthoringSchema ? (
            <>
              {(selectedAuthoringSchema.fields ?? []).map((field) => (
                <FrameworkAuthoringFieldInput
                  key={field.id}
                  field={field}
                  value={frameworkValues[field.id]}
                  onChange={(value) => setFrameworkValues((current) => ({ ...current, [field.id]: value }))}
                />
              ))}
              <p className="tiny-text">
                表单由已安装框架 {selectedFramework?.qualifiedId || selectedFramework?.id} 的 authoring schema 提供。
              </p>
            </>
          ) : null}

          {!selectedAuthoringSchema && (mode === "cli_wrapper" || mode === "native_image") ? (
            <>
              <label className="field-label">
                {mode === "native_image" ? "图像工具命令" : "命令"}
                <input
                  className="studio-input"
                  value={command}
                  onChange={(event) => setCommand(event.target.value)}
                  placeholder={mode === "native_image" ? "loom-image-filter" : "ffmpeg"}
                />
              </label>
              <label className="field-label">
                参数，每行一个
                <textarea
                  className="studio-textarea studio-textarea--compact"
                  value={argsText}
                  onChange={(event) => setArgsText(event.target.value)}
                  placeholder="--input {{inputs.image.path}}&#10;--output {{outputs.result.path}}"
                />
              </label>
              {mode === "cli_wrapper" ? (
                <>
                  <label className="field-label">
                    原始 CLI 命令导入
                    <textarea
                      className="studio-textarea studio-textarea--compact"
                      value={rawCommandText}
                      onChange={(event) => setRawCommandText(event.target.value)}
                      placeholder="ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}"
                    />
                  </label>
                  <button className="ghost-button" type="button" onClick={importRawCommand}>
                    解析 CLI 命令
                  </button>
                </>
              ) : null}
              {mode === "native_image" ? (
                <label className="field-label">
                  原生图像滤镜
                  <input
                    className="studio-input"
                    value={nativeFilter}
                    onChange={(event) => setNativeFilter(event.target.value)}
                    placeholder="identity, grayscale, resize"
                  />
                </label>
              ) : null}
            </>
          ) : null}

          {!selectedAuthoringSchema && mode === "cloud_api" ? (
            <>
              <label className="field-label">
                Endpoint URL
                <input
                  className="studio-input"
                  value={endpoint}
                  onChange={(event) => setEndpoint(event.target.value)}
                  placeholder="https://api.example.com/v1/images"
                />
              </label>
              <label className="field-label">
                方法
                <input
                  className="studio-input"
                  value={method}
                  onChange={(event) => setMethod(event.target.value)}
                  placeholder="POST"
                />
              </label>
              <label className="field-label">
                内容类型
                <input
                  className="studio-input"
                  value={contentType}
                  onChange={(event) => setContentType(event.target.value)}
                  placeholder="application/json or multipart/form-data"
                />
              </label>
              <label className="field-label">
                Headers 模板
                <textarea
                  className="studio-textarea studio-textarea--compact"
                  value={headersText}
                  onChange={(event) => setHeadersText(event.target.value)}
                />
              </label>
              <label className="field-label">
                Body 模板
                <textarea
                  className="studio-textarea studio-textarea--compact"
                  value={bodyText}
                  onChange={(event) => setBodyText(event.target.value)}
                />
              </label>
              <label className="field-label">
                云 API 智能导入 cURL
                <textarea
                  className="studio-textarea studio-textarea--compact"
                  value={cloudCurlText}
                  onChange={(event) => setCloudCurlText(event.target.value)}
                  placeholder="curl -X POST https://api.example.com ..."
                />
              </label>
              <label className="field-label">
                输出端口响应示例
                <textarea
                  className="studio-textarea studio-textarea--compact"
                  value={cloudResponseText}
                  onChange={(event) => setCloudResponseText(event.target.value)}
                />
              </label>
              <button className="ghost-button" type="button" onClick={importCloudSmartTemplate}>
                智能导入请求和响应端口
              </button>
            </>
          ) : null}

          {!selectedAuthoringSchema && mode === "script" ? (
            <>
              <label className="field-label">
                Python 脚本路径
                <input
                  className="studio-input"
                  value={scriptPath}
                  onChange={(event) => setScriptPath(event.target.value)}
                  placeholder="C:\\path\\to\\main.py"
                />
              </label>
              <label className="checkbox-line">
                <input
                  type="checkbox"
                  checked={shaderMode}
                  onChange={(event) => setShaderMode(event.target.checked)}
                />
                <span>Shader 模式</span>
              </label>
            </>
          ) : null}

          {!selectedAuthoringSchema && mode === "mcp" ? (
            <>
              <label className="field-label">
                MCP 服务
                <select
                  className="studio-input"
                  value={mcpServerId}
                  onChange={(event) => setMcpServerId(event.target.value)}
                >
                  <option value="">选择已配置 MCP 服务</option>
                  {mcpServers.map((server) => (
                    <option key={server.id} value={server.id}>{server.name || server.id}</option>
                  ))}
                </select>
              </label>
              <label className="field-label">
                MCP 工具名
                <input
                  className="studio-input"
                  value={mcpToolName}
                  onChange={(event) => setMcpToolName(event.target.value)}
                  placeholder="screenshot, search, fetch..."
                />
              </label>
              <div className="studio-actions">
                <button
                  className="ghost-button"
                  type="button"
                  onClick={discoverMcpTools}
                  disabled={mcpDiscoveryBusy}
                >
                  {mcpDiscoveryBusy ? "发现中" : "发现 MCP 工具"}
                </button>
                <button
                  className="ghost-button"
                  type="button"
                  onClick={useSelectedMcpToolSchema}
                  disabled={!mcpTools.length}
                >
                  使用 MCP 工具结构
                </button>
              </div>
              <label className="field-label">
                MCP input_schema 工具
                <select
                  className="studio-input"
                  value={selectedMcpSchemaToolName}
                  onChange={(event) => setSelectedMcpSchemaToolName(event.target.value)}
                >
                  <option value="">选择已发现 MCP 工具</option>
                  {mcpTools.map((tool, index) => {
                    const label = mcpToolLabel(tool);
                    return <option key={`${label}-${index}`} value={label}>{label}</option>;
                  })}
                </select>
              </label>
            </>
          ) : null}

          {!selectedAuthoringSchema && mode === "python_art" ? (
            <label className="field-label">
              已安装 Python Art
              <select
                className="studio-input"
                value={pythonArtId}
                onChange={(event) => setPythonArtId(event.target.value)}
              >
                <option value="">选择已安装 Python Art</option>
                {pythonArts.map((art) => (
                  <option key={art.art_id} value={art.art_id}>{art.label || art.art_id}</option>
                ))}
              </select>
            </label>
          ) : null}

          {!selectedAuthoringSchema && mode === "workflow" ? (
            <label className="field-label">
              已保存工作流
              <select
                className="studio-input"
                value={workflowId}
                onChange={(event) => setWorkflowId(event.target.value)}
              >
                <option value="">选择工作流</option>
                {workflows.map((workflow) => (
                  <option key={workflow.id} value={workflow.id}>{workflow.name || workflow.id}</option>
                ))}
              </select>
            </label>
          ) : null}

          <div className="advanced-port-editor">
            <div className="control-card__head">
              <div>
                <p className="card-kicker">AddArtModal 兼容</p>
                <h3>高级端口编辑</h3>
              </div>
              <span className="mini-chip">{inputPorts.length}/{outputPorts.length}</span>
            </div>
            <div className="port-editor-section">
              <div className="section-heading-row section-heading-row--compact">
                <h4>输入端口</h4>
                <button
                  className="ghost-button"
                  type="button"
                  onClick={() => setInputPorts((ports) => [...ports, createPortDraft("input")])}
                >
                  添加输入端口
                </button>
              </div>
              {inputPorts.map((port, index) => (
                <div className="port-editor-row" key={`input-${index}`}>
                  <input
                    className="studio-input"
                    value={port.name}
                    onChange={(event) => updateInputPort(index, { name: event.target.value })}
                    placeholder="name"
                  />
                  <input
                    className="studio-input"
                    value={port.label}
                    onChange={(event) => updateInputPort(index, { label: event.target.value })}
                    placeholder="label"
                  />
                  <select
                    className="studio-input"
                    value={port.type}
                    onChange={(event) => {
                      const nextType = event.target.value;
                      updateInputPort(index, {
                        type: nextType,
                        executionType: defaultExecutionTypeForPort(nextType, "input"),
                      });
                    }}
                  >
                    <option value="image">图像 image</option>
                    <option value="file">文件 file</option>
                    <option value="string">文本 string</option>
                    <option value="int">整数 int</option>
                    <option value="float">小数 float</option>
                    <option value="boolean">布尔 boolean</option>
                  </select>
                  <input
                    className="studio-input"
                    value={port.executionType}
                    onChange={(event) => updateInputPort(index, { executionType: event.target.value })}
                    placeholder="execution type"
                  />
                  <input
                    className="studio-input"
                    value={port.defaultValue}
                    onChange={(event) => updateInputPort(index, { defaultValue: event.target.value })}
                    placeholder="default"
                  />
                  <label className="checkbox-line checkbox-line--compact">
                    <input
                      type="checkbox"
                      checked={port.disabled}
                      onChange={(event) => updateInputPort(index, { disabled: event.target.checked })}
                    />
                    <span>禁用</span>
                  </label>
                  <button
                    className="ghost-button"
                    type="button"
                    onClick={() => setInputPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}
                    disabled={inputPorts.length <= 1}
                  >
                    删除
                  </button>
                </div>
              ))}
            </div>

            <div className="port-editor-section">
              <div className="section-heading-row section-heading-row--compact">
                <h4>输出端口</h4>
                <button
                  className="ghost-button"
                  type="button"
                  onClick={() => setOutputPorts((ports) => [...ports, createPortDraft("output")])}
                >
                  添加输出端口
                </button>
              </div>
              {outputPorts.map((port, index) => (
                <div className="port-editor-row port-editor-row--output" key={`output-${index}`}>
                  <input
                    className="studio-input"
                    value={port.name}
                    onChange={(event) => updateOutputPort(index, { name: event.target.value })}
                    placeholder="name"
                  />
                  <input
                    className="studio-input"
                    value={port.label}
                    onChange={(event) => updateOutputPort(index, { label: event.target.value })}
                    placeholder="label"
                  />
                  <select
                    className="studio-input"
                    value={port.type}
                    onChange={(event) => {
                      const nextType = event.target.value;
                      updateOutputPort(index, {
                        type: nextType,
                        executionType: defaultExecutionTypeForPort(nextType, "output"),
                      });
                    }}
                  >
                    <option value="image">图像 image</option>
                    <option value="file">文件 file</option>
                    <option value="string">文本 string</option>
                    <option value="int">整数 int</option>
                    <option value="float">小数 float</option>
                    <option value="boolean">布尔 boolean</option>
                  </select>
                  <input
                    className="studio-input"
                    value={port.executionType}
                    onChange={(event) => updateOutputPort(index, { executionType: event.target.value })}
                    placeholder="execution type"
                  />
                  <label className="field-label field-label--inline">
                    捕获模式
                    <select
                      className="studio-input"
                      value={port.captureMode}
                      onChange={(event) => updateOutputPort(index, { captureMode: event.target.value as ArtPortCaptureMode })}
                    >
                      {outputCaptureModes.map((captureMode) => (
                        <option key={captureMode} value={captureMode}>{captureMode}</option>
                      ))}
                    </select>
                  </label>
                  <input
                    className="studio-input"
                    value={port.jsonPath}
                    onChange={(event) => updateOutputPort(index, { jsonPath: event.target.value })}
                    placeholder="JSONPath"
                  />
                  <input
                    className="studio-input"
                    value={port.filename}
                    onChange={(event) => updateOutputPort(index, { filename: event.target.value })}
                    placeholder="filename/template"
                  />
                  <button
                    className="ghost-button"
                    type="button"
                    onClick={() => setOutputPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}
                    disabled={outputPorts.length <= 1}
                  >
                    删除
                  </button>
                </div>
              ))}
            </div>
          </div>

          {wizardMessage ? (
            <p className={wizardMessage.kind === "error" ? "error-text" : "success-text"}>{wizardMessage.text}</p>
          ) : null}

          <div className="art-wizard-preview">
            <strong>端口</strong>
            <span>输入: {inputPorts.map((port) => `${port.name}:${port.executionType}`).join(", ")}</span>
            <span>输出: {outputPorts.map((port) => `${port.name}:${port.executionType}`).join(", ")}</span>
            <span>参数: 模板会写入执行配置</span>
          </div>
          <button className="signal-button" type="button" onClick={submit} disabled={busy}>
            {busy ? "创建中" : "创建 Art"}
          </button>
        </article>
      </div>
    </section>
  );
}

function McpPanel({
  servers,
  baseUrl,
  refresh,
  openWorkflowStudio,
  openHookWorkflow,
}: {
  servers: LoomMcpServer[];
  baseUrl: string;
  refresh: () => Promise<void>;
  openWorkflowStudio: () => void;
  openHookWorkflow: () => void;
}) {
  const [busyServerId, setBusyServerId] = useState<string | null>(null);
  const [busyMarketplaceId, setBusyMarketplaceId] = useState<string | null>(null);
  const [mcpMessage, setMcpMessage] = useState<StudioMessage | null>(null);
  const [searchText, setSearchText] = useState("");
  const [marketCategory, setMarketCategory] = useState<McpMarketCategory | "All">("All");
  const [marketServers, setMarketServers] = useState<McpMarketServer[]>([...MCP_MARKET_SERVERS]);
  const [marketSource, setMarketSource] = useState<"registry" | "fallback">("fallback");
  const [registryCount, setRegistryCount] = useState(0);
  const [registryCursor, setRegistryCursor] = useState<string | null>(null);
  const [testSnapshots, setTestSnapshots] = useState<Record<string, McpMarketplaceTestSnapshot>>({});
  const [manualServerId, setManualServerId] = useState("manual-mcp-server");
  const [manualServerName, setManualServerName] = useState("手动 MCP 服务");
  const [manualDescription, setManualDescription] = useState("供 Loom Art 节点使用的手动 MCP 服务。");
  const [manualCommand, setManualCommand] = useState("npx");
  const [manualArgsText, setManualArgsText] = useState("-y\n@modelcontextprotocol/server-memory");
  const [manualEnvText, setManualEnvText] = useState("");
  const [packageModuleName, setPackageModuleName] = useState("json");
  const [packageName, setPackageName] = useState("mcp-server-demo");
  const [packageBusy, setPackageBusy] = useState<"check" | "plan" | null>(null);
  const [packageResult, setPackageResult] = useState<string | null>(null);
  const [imageSearchApiKey, setImageSearchApiKey] = useState("");
  const [imageSearchBusy, setImageSearchBusy] = useState(false);

  useEffect(() => {
    const configured = servers.find((server) => server.id === IMAGE_SEARCH_SERVER_ID);
    const nextKey = configured?.env?.BRAVE_API_KEY ?? "";
    setImageSearchApiKey((previous) => (previous || nextKey ? previous || nextKey : ""));
  }, [servers]);

  const removeServer = async (server: LoomMcpServer) => {
    setBusyServerId(server.id);
    try {
      await deleteMcpServer(baseUrl, server.id);
      setMcpMessage({ kind: "info", text: `已删除 MCP 服务 ${server.name || server.id}。` });
      await refresh();
    } catch (error) {
      setMcpMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法删除 MCP 服务。",
      });
    } finally {
      setBusyServerId(null);
    }
  };

  const refreshMarketplace = async (append = false) => {
    setBusyMarketplaceId("registry-refresh");
    try {
      const response = await fetchMcpRegistry(baseUrl, {
        search: searchText,
        limit: 80,
        cursor: append ? registryCursor : null,
      });
      const registryServers = mapRegistryResponseToMarketplace(response);
      const existingRegistryServers = append
        ? marketServers.filter((server) => server.sourceKind === "registry")
        : [];
      const combinedRegistryServers = mergeRegistryAndCuratedMarketplace(
        [...existingRegistryServers, ...registryServers],
        [],
      );
      setMarketServers(mergeRegistryAndCuratedMarketplace(combinedRegistryServers, MCP_MARKET_SERVERS));
      setMarketSource("registry");
      setRegistryCount(combinedRegistryServers.length);
      setRegistryCursor(response.metadata?.nextCursor || null);
      setMcpMessage({
        kind: "info",
        text: `已加载 ${registryServers.length} 个 MCP Registry stdio 模板。`,
      });
    } catch (error) {
      setMarketServers([...MCP_MARKET_SERVERS]);
      setMarketSource("fallback");
      setRegistryCount(0);
      setRegistryCursor(null);
      setMcpMessage({
        kind: "error",
        text: error instanceof Error
          ? `MCP Registry 不可用，使用内置列表。${error.message}`
          : "MCP Registry 不可用，使用内置列表。",
      });
    } finally {
      setBusyMarketplaceId(null);
    }
  };

  const installMarketplaceServer = async (marketItem: McpMarketServer, testAfterInstall: boolean) => {
    const existing = servers.find((server) => server.id === marketItem.id || server.name === marketItem.name);
    const serverConfig = buildMarketplaceServerConfig(marketItem, existing);
    const health = getMarketplaceHealth(marketItem, serverConfig, existing ? testSnapshots[existing.id] : undefined);
    const actionKey = `${marketItem.id}:${testAfterInstall ? "test" : "install"}`;
    setBusyMarketplaceId(actionKey);
    try {
      const savedServer = await saveMcpServer(baseUrl, serverConfig);
      if (testAfterInstall) {
        if (marketItem.requiresManualConfiguration) {
          setMcpMessage({
            kind: "error",
            text: `${marketItem.name} 已保存，但测试前仍需补充参数。`,
          });
        } else if (!health.requiredEnvPresent) {
          setMcpMessage({
            kind: "error",
            text: `${marketItem.name} 已保存，但缺少环境变量。`,
          });
        } else {
          const result = await testMcpConnection(baseUrl, savedServer);
          const tools = Array.isArray(result.tools) ? result.tools : [];
          setTestSnapshots((previous) => ({
            ...previous,
            [savedServer.id]: {
              status: result.success === false ? "error" : "success",
              toolCount: tools.length,
              testedAt: new Date().toISOString(),
              error: result.success === false ? result.error || "MCP 测试失败" : undefined,
            },
          }));
          setMcpMessage({
            kind: result.success === false ? "error" : "info",
            text: result.success === false
              ? `${marketItem.name} 已保存，但连接测试失败：${result.error || "未知错误"}`
              : `${marketItem.name} 已保存，发现 ${tools.length} 个工具。`,
          });
        }
      } else {
        setMcpMessage({ kind: "info", text: `已安装服务 ${marketItem.name}。` });
      }
      await refresh();
    } catch (error) {
      setMcpMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法安装 MCP 市场服务。",
      });
    } finally {
      setBusyMarketplaceId(null);
    }
  };

  const testConfiguredServer = async (server: LoomMcpServer) => {
    setBusyServerId(server.id);
    try {
      const result = await testMcpConnection(baseUrl, server);
      const tools = Array.isArray(result.tools) ? result.tools : [];
      setTestSnapshots((previous) => ({
        ...previous,
        [server.id]: {
          status: result.success === false ? "error" : "success",
          toolCount: tools.length,
          testedAt: new Date().toISOString(),
          error: result.success === false ? result.error || "MCP 测试失败" : undefined,
        },
      }));
      setMcpMessage({
        kind: result.success === false ? "error" : "info",
        text: result.success === false
          ? `${server.name || server.id} 测试失败：${result.error || "未知错误"}`
          : `${server.name || server.id} 暴露 ${tools.length} 个工具。`,
      });
    } catch (error) {
      setMcpMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法测试 MCP 服务。",
      });
    } finally {
      setBusyServerId(null);
    }
  };

  const connectManualMcpServer = async (testAfterSave: boolean) => {
    const id = normalizeToolId(manualServerId || manualServerName);
    const server: LoomMcpServer = {
      id,
      name: manualServerName.trim() || "手动 MCP 服务",
      description: manualDescription.trim(),
      command: manualCommand.trim() || "npx",
      args: parseListText(manualArgsText),
      env: parseEnvText(manualEnvText),
      enabled: true,
    };

    setBusyServerId(id);
    try {
      const savedServer = await saveMcpServer(baseUrl, server);
      if (testAfterSave) {
        const result = await testMcpConnection(baseUrl, savedServer);
        const tools = Array.isArray(result.tools) ? result.tools : [];
        setTestSnapshots((previous) => ({
          ...previous,
          [savedServer.id]: {
            status: result.success === false ? "error" : "success",
            toolCount: tools.length,
            testedAt: new Date().toISOString(),
            error: result.success === false ? result.error || "MCP 测试失败" : undefined,
          },
        }));
        setMcpMessage({
          kind: result.success === false ? "error" : "info",
          text: result.success === false
            ? `已保存 MCP 服务，但测试失败：${result.error || "未知错误"}`
            : `已连接 ${savedServer.name}，发现 ${tools.length} 个工具。`,
        });
      } else {
        setMcpMessage({ kind: "info", text: `已保存 MCP 服务 ${savedServer.name}。` });
      }
      setManualServerId(savedServer.id);
      await refresh();
    } catch (error) {
      setMcpMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法保存手动 MCP 服务。",
      });
    } finally {
      setBusyServerId(null);
    }
  };

  const runMcpPackageCheck = async () => {
    setPackageBusy("check");
    try {
      const result = await checkMcpPackageInstalled(baseUrl, packageModuleName.trim() || "json");
      setPackageResult(
        `check_mcp_package_installed: module=${result.module || packageModuleName}, installed=${result.installed === true}`,
      );
    } catch (error) {
      setPackageResult(error instanceof Error ? error.message : "无法检查 MCP 包。");
    } finally {
      setPackageBusy(null);
    }
  };

  const prepareMcpPackageInstallPlan = async () => {
    setPackageBusy("plan");
    try {
      const result = await buildMcpPackageInstallPlan(baseUrl, packageName.trim() || "mcp-server-demo");
      setPackageResult(
        `install_mcp_package safe plan: ${(result.command || []).join(" ")}; sideEffect=${result.sideEffect === true}`,
      );
    } catch (error) {
      setPackageResult(error instanceof Error ? error.message : "无法生成 MCP 包安装计划。");
    } finally {
      setPackageBusy(null);
    }
  };

  const normalizedSearch = searchText.trim().toLowerCase();
  const installImageSearchManualFlow = async () => {
    const braveApiKey = imageSearchApiKey.trim();
    if (!braveApiKey) {
      setMcpMessage({ kind: "error", text: "图片搜索手工测试流需要先填写 BRAVE_API_KEY。" });
      return;
    }

    const existingServer = servers.find((server) => server.id === IMAGE_SEARCH_SERVER_ID);
    setImageSearchBusy(true);
    try {
      const savedServer = await saveMcpServer(
        baseUrl,
        buildImageSearchServerConfig(braveApiKey, existingServer),
      );
      const framework = await installFramework(baseUrl, "mcp");
      await saveToolDefinition(baseUrl, buildImageSearchArtDefinition(savedServer.id));
      const connection = await testMcpConnection(baseUrl, savedServer);
      const tools = Array.isArray(connection.tools) ? connection.tools : [];
      setTestSnapshots((previous) => ({
        ...previous,
        [savedServer.id]: {
          status: connection.success === false ? "error" : "success",
          toolCount: tools.length,
          testedAt: new Date().toISOString(),
          error: connection.success === false ? connection.error || "MCP 测试失败" : undefined,
        },
      }));
      setMcpMessage({
        kind: connection.success === false ? "error" : "info",
        text: connection.success === false
          ? `图片搜索 Art 已保存，但 Brave Search 测试失败：${connection.error || "未知错误"}`
          : `图片搜索手工测试流已就绪：MCP 框架 ${framework?.ready ? "已就绪" : "已安装"}，服务 ${savedServer.name} 暴露 ${tools.length} 个工具，Art 已保存为 ${IMAGE_SEARCH_ART_ID}。`,
      });
      await refresh();
    } catch (error) {
      setMcpMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法安装图片搜索手工测试流。",
      });
    } finally {
      setImageSearchBusy(false);
    }
  };

  const filteredServers = servers.filter((server) => {
    const matchesSearch =
      !normalizedSearch ||
      server.name.toLowerCase().includes(normalizedSearch) ||
      server.id.toLowerCase().includes(normalizedSearch) ||
      (server.description || "").toLowerCase().includes(normalizedSearch);
    return matchesSearch;
  });
  const filteredMarketServers = marketServers.filter((server) => {
    const matchesSearch =
      !normalizedSearch ||
      server.name.toLowerCase().includes(normalizedSearch) ||
      server.description.toLowerCase().includes(normalizedSearch) ||
      server.id.toLowerCase().includes(normalizedSearch);
    const matchesCategory = marketCategory === "All" || server.category === marketCategory;
    return matchesSearch && matchesCategory;
  });

  return (
    <section className="content-grid">
      <div className="main-board">
        <p className="section-kicker">MCP</p>
        <h2>MCP 服务</h2>
        <button className="ghost-button" type="button" onClick={() => openExternal(`${baseUrl}/v1/mcp/servers`)}>
          查看 MCP 服务 JSON
        </button>
        <div className="marketplace-toolbar">
          <label className="field-label">
            搜索
            <input
              className="studio-input"
              value={searchText}
              onChange={(event) => setSearchText(event.target.value)}
              placeholder="搜索 MCP 服务"
            />
          </label>
          <label className="field-label">
            分类
            <select
              className="studio-input"
              value={marketCategory}
              onChange={(event) => setMarketCategory(event.target.value as McpMarketCategory | "All")}
            >
              <option value="All">全部分类</option>
              {MCP_MARKET_CATEGORIES.map((category) => (
                <option key={category} value={category}>{mcpMarketCategoryLabel(category)}</option>
              ))}
            </select>
          </label>
          <button
            className="signal-button"
            type="button"
            onClick={() => refreshMarketplace(false)}
            disabled={busyMarketplaceId === "registry-refresh"}
          >
            刷新注册表
          </button>
          {registryCursor ? (
            <button
              className="ghost-button"
              type="button"
              onClick={() => refreshMarketplace(true)}
              disabled={busyMarketplaceId === "registry-refresh"}
            >
              加载更多
            </button>
          ) : null}
        </div>
        <p className="mono-line">
          市场来源：{marketSource === "registry" ? `MCP Registry（${registryCount} 个模板）` : "内置列表"}
        </p>
        {mcpMessage ? (
          <p className={mcpMessage.kind === "error" ? "error-text" : "success-text"}>{mcpMessage.text}</p>
        ) : null}
      </div>
      <article className="glass-card studio-card manual-mcp-card">
        <div className="control-card__head">
          <div>
            <p className="card-kicker">图片搜索</p>
            <h3>手工测试流</h3>
          </div>
          <span className="mini-chip">Brave Search</span>
        </div>
        <p>
          一键保存 Brave Search MCP 服务、安装 MCP 框架，并注册
          <code>{IMAGE_SEARCH_ART_ID}</code>
          ，方便直接在工作流工作台或 Hook 实时工作流里手工执行“图片搜索”节点。
        </p>
        <label className="field-label">
          BRAVE_API_KEY
          <input
            className="studio-input"
            value={imageSearchApiKey}
            onChange={(event) => setImageSearchApiKey(event.target.value)}
            placeholder="输入 Brave Search API Key"
          />
        </label>
        <div className="studio-actions">
          <button
            className="signal-button"
            type="button"
            onClick={installImageSearchManualFlow}
            disabled={imageSearchBusy}
          >
            {imageSearchBusy ? "安装中" : "安装图片搜索测试流"}
          </button>
          <button className="ghost-button" type="button" onClick={openWorkflowStudio}>
            打开工作流工作台
          </button>
          <button className="ghost-button" type="button" onClick={openHookWorkflow}>
            打开 Hook 实时工作流
          </button>
        </div>
      </article>
      <article className="glass-card studio-card manual-mcp-card">
        <div className="control-card__head">
          <div>
            <p className="card-kicker">手动 MCP 服务</p>
            <h3>连接 MCP 服务</h3>
          </div>
          <span className="mini-chip">stdio</span>
        </div>
        <div className="manual-mcp-grid">
          <label className="field-label">
            服务 ID
            <input
              className="studio-input"
              value={manualServerId}
              onChange={(event) => setManualServerId(event.target.value)}
            />
          </label>
          <label className="field-label">
            名称
            <input
              className="studio-input"
              value={manualServerName}
              onChange={(event) => setManualServerName(event.target.value)}
            />
          </label>
          <label className="field-label">
            命令
            <input
              className="studio-input"
              value={manualCommand}
              onChange={(event) => setManualCommand(event.target.value)}
              placeholder="npx, uvx, python, docker"
            />
          </label>
          <label className="field-label">
            描述
            <input
              className="studio-input"
              value={manualDescription}
              onChange={(event) => setManualDescription(event.target.value)}
            />
          </label>
          <label className="field-label">
            参数，每行一个
            <textarea
              className="studio-textarea studio-textarea--compact"
              value={manualArgsText}
              onChange={(event) => setManualArgsText(event.target.value)}
            />
          </label>
          <label className="field-label">
            环境变量，KEY=value 每行一个
            <textarea
              className="studio-textarea studio-textarea--compact"
              value={manualEnvText}
              onChange={(event) => setManualEnvText(event.target.value)}
              placeholder="BRAVE_API_KEY=..."
            />
          </label>
        </div>
        <div className="studio-actions">
          <button
            className="ghost-button"
            type="button"
            onClick={() => connectManualMcpServer(false)}
            disabled={busyServerId !== null}
          >
            保存 MCP 服务
          </button>
          <button
            className="signal-button"
            type="button"
            onClick={() => connectManualMcpServer(true)}
            disabled={busyServerId !== null}
          >
            连接 MCP 服务
          </button>
        </div>
      </article>
      <article className="glass-card studio-card mcp-package-card">
        <div className="control-card__head">
          <div>
            <p className="card-kicker">MCP 包兼容</p>
            <h3>check_mcp_package_installed / install_mcp_package</h3>
          </div>
          <span className="mini-chip">安全预览</span>
        </div>
        <div className="manual-mcp-grid">
          <label className="field-label">
            要检查的模块
            <input
              className="studio-input"
              value={packageModuleName}
              onChange={(event) => setPackageModuleName(event.target.value)}
              placeholder="mcp_server_brave_search"
            />
          </label>
          <label className="field-label">
            安装计划包名
            <input
              className="studio-input"
              value={packageName}
              onChange={(event) => setPackageName(event.target.value)}
              placeholder="mcp-server-brave-search"
            />
          </label>
        </div>
        <div className="studio-actions">
          <button
            className="ghost-button"
            type="button"
            onClick={runMcpPackageCheck}
            disabled={packageBusy !== null}
          >
            {packageBusy === "check" ? "检查中" : "检查包"}
          </button>
          <button
            className="signal-button"
            type="button"
            onClick={prepareMcpPackageInstallPlan}
            disabled={packageBusy !== null}
          >
            {packageBusy === "plan" ? "生成中" : "安装命令预览"}
          </button>
        </div>
        {packageResult ? <p className="mono-line">{packageResult}</p> : null}
      </article>
      <div className="section-heading-row">
        <div>
          <p className="section-kicker">已配置服务</p>
          <h3>已配置服务</h3>
        </div>
        <span className="mini-chip">{filteredServers.length}</span>
      </div>
      <div className="card-grid">
        {filteredServers.length ? filteredServers.map((server) => {
          const testSnapshot = testSnapshots[server.id];
          return (
          <article className="glass-card control-card" key={server.id}>
            <div className="control-card__head">
              <div>
                <p className="card-kicker">MCP 服务</p>
                <h3>{server.name || server.id}</h3>
              </div>
              <EnabledChip enabled={server.enabled} />
            </div>
            <p className="mono-line">{server.command}</p>
            <p>{server.args?.length ? server.args.join(" ") : "未配置启动参数。"}</p>
            {server.description ? <p>{firstWords(server.description, server.description)}</p> : null}
            {testSnapshot ? (
              <p className={testSnapshot.status === "error" ? "error-text" : "success-text"}>
                {testSnapshot.status === "error"
                  ? testSnapshot.error || "测试失败"
                  : `上次测试发现 ${testSnapshot.toolCount} 个工具。`}
              </p>
            ) : null}
            <div className="studio-actions">
              <button
                className="ghost-button"
                type="button"
                onClick={() => testConfiguredServer(server)}
                disabled={busyServerId === server.id}
              >
                测试连接
              </button>
              <button
                className="ghost-button"
                type="button"
                onClick={() => removeServer(server)}
                disabled={busyServerId === server.id}
              >
                {busyServerId === server.id ? "删除中" : "删除服务"}
              </button>
            </div>
          </article>
          );
        }) : (
          <article className="glass-card empty-card">
            <h3>暂无 MCP 服务</h3>
          </article>
        )}
      </div>
      <div className="section-heading-row">
        <div>
          <p className="section-kicker">市场</p>
          <h3>MCP 市场</h3>
        </div>
        <span className="mini-chip">{filteredMarketServers.length}</span>
      </div>
      <div className="card-grid">
        {filteredMarketServers.map((marketItem) => {
          const configured = servers.find((server) => server.id === marketItem.id || server.name === marketItem.name);
          const health = getMarketplaceHealth(marketItem, configured, configured ? testSnapshots[configured.id] : undefined);
          const installKey = `${marketItem.id}:install`;
          const testKey = `${marketItem.id}:test`;
          return (
            <article className="glass-card control-card" key={marketItem.id}>
              <div className="control-card__head">
                <div>
                  <p className="card-kicker">{marketItem.sourceKind === "registry" ? "注册表服务" : "内置服务"}</p>
                  <h3>{marketItem.name}</h3>
                </div>
                <EnabledChip enabled={marketItem.defaultEnabled !== false} />
              </div>
              <p>{firstWords(marketItem.description, "无描述。")}</p>
              <p className="mono-line">{marketItem.command} {marketItem.args.join(" ")}</p>
              <div className="marketplace-tags">
                <span className="method-chip">{mcpMarketCategoryLabel(marketItem.category)}</span>
                <span className="method-chip">{marketItem.sourceKind === "registry" ? "注册表" : "内置"}</span>
                {health.tags.map((tag) => (
                  <span className={`method-chip marketplace-tag--${tag.tone}`} key={tag.label}>{tag.label}</span>
                ))}
              </div>
              {marketItem.notes ? <p className="mono-line">{marketItem.notes}</p> : null}
              <p className="mono-line">
                {marketItem.installSource.registry}:{marketItem.installSource.packageName}
              </p>
              <div className="studio-actions">
                <button
                  className="signal-button"
                  type="button"
                  onClick={() => installMarketplaceServer(marketItem, false)}
                  disabled={Boolean(busyMarketplaceId)}
                >
                  {busyMarketplaceId === installKey ? "安装中" : "安装服务"}
                </button>
                <button
                  className="ghost-button"
                  type="button"
                  onClick={() => installMarketplaceServer(marketItem, true)}
                  disabled={Boolean(busyMarketplaceId)}
                >
                  {busyMarketplaceId === testKey ? "测试中" : "安装并测试"}
                </button>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}

function RegistryPanel({
  tools,
  pythonArts,
  mcpServers,
  workflows,
  frameworks,
  selectedFrameworkIds,
  reloadFrameworks,
  baseUrl,
  refresh,
}: {
  tools: LoomToolDefinition[];
  pythonArts: LoomPythonArt[];
  mcpServers: LoomMcpServer[];
  workflows: LoomWorkflowMetadata[];
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  reloadFrameworks: () => Promise<void>;
  baseUrl: string;
  refresh: () => Promise<void>;
}) {
  const [busyArtId, setBusyArtId] = useState<string | null>(null);
  const [busyToolId, setBusyToolId] = useState<string | null>(null);
  const [busyWizard, setBusyWizard] = useState(false);
  const [registryMessage, setRegistryMessage] = useState<StudioMessage | null>(null);
  const [sourcePath, setSourcePath] = useState("");
  const [sourceToolId, setSourceToolId] = useState("");
  const [sourceToolName, setSourceToolName] = useState("");
  const [sourceDescription, setSourceDescription] = useState("");
  const [sourceCode, setSourceCode] = useState("");
  const [sourcePorts, setSourcePorts] = useState<{ inputs: PythonArtPort[]; outputs: PythonArtPort[] } | null>(null);
  const [sourceArtJsonPath, setSourceArtJsonPath] = useState("");
  const [sourceBusyAction, setSourceBusyAction] = useState<string | null>(null);
  const [compatBusyAction, setCompatBusyAction] = useState<string | null>(null);
  const [compatArtCount, setCompatArtCount] = useState<number | null>(null);
  const [compatArts, setCompatArts] = useState<ArtLoomCompatArt[]>([]);
  const [pythonEngineSummary, setPythonEngineSummary] = useState<string>("Not probed");
  const [shaderArtId, setShaderArtId] = useState("");
  const visibleTools = useMemo(
    () => filterToolsByFrameworks(tools, frameworks, selectedFrameworkIds),
    [frameworks, selectedFrameworkIds, tools],
  );

  const importPythonArt = async (art: LoomPythonArt) => {
    const toolId = `python-art-${art.art_id.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
    const tool: LoomToolDefinition = {
      id: toolId,
      name: art.label || art.art_id,
      description: art.description || "从 Loom 包目录导入的 Python Art。",
      enabled: true,
      execution: {
        type: "python_art",
        artId: art.art_id,
        artPath: art.path,
      },
    };

    setBusyArtId(art.art_id);
    try {
      await saveToolDefinition(baseUrl, tool);
      setRegistryMessage({ kind: "info", text: `已导入 Python Art ${art.label || art.art_id} 为 ${toolId}。` });
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法导入 Python Art。",
      });
    } finally {
      setBusyArtId(null);
    }
  };

  const loadRegistryCompatibility = async () => {
    setCompatBusyAction("list-arts");
    try {
      const response = await listArtLoomCompatArts(baseUrl);
      const arts = response.arts || [];
      setCompatArts(arts);
      setCompatArtCount(typeof response.count === "number" ? response.count : arts.length);
      if (arts[0]?.id || arts[0]?.art_id) {
        await getArtLoomCompatArt(baseUrl, String(arts[0].id || arts[0].art_id));
      }
      setRegistryMessage({
        kind: "info",
        text: `list_arts 返回 ${arts.length} 个 Art。`,
      });
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法读取 ArtLoom 注册表兼容接口。",
      });
    } finally {
      setCompatBusyAction(null);
    }
  };

  const syncRegistryCompatibility = async () => {
    setCompatBusyAction("sync-user-arts");
    try {
      const response = await syncArtLoomCompatArts(baseUrl);
      const arts = response.arts || [];
      setCompatArts(arts);
      setCompatArtCount(typeof response.count === "number" ? response.count : arts.length);
      setRegistryMessage({
        kind: "info",
        text:
          response.message ||
          `sync_user_arts 已镜像当前 Loom Arts；sideEffect=${response.sideEffect === true}。`,
      });
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法同步 ArtLoom 用户 Art。",
      });
    } finally {
      setCompatBusyAction(null);
    }
  };

  const toggleFirstArtCompatibility = async (enabled: boolean) => {
    const firstArt = compatArts[0];
    const firstArtId = String(firstArt?.id || firstArt?.art_id || "");
    if (!firstArtId) {
      setRegistryMessage({ kind: "error", text: "没有可用于 enable_art / disable_art 的 Art。" });
      return;
    }
    setCompatBusyAction(enabled ? "enable-art" : "disable-art");
    try {
      const response = enabled
        ? await enableArtLoomCompatArt(baseUrl, firstArtId)
        : await disableArtLoomCompatArt(baseUrl, firstArtId);
      setRegistryMessage({
        kind: "info",
        text: `${response.compatCommand || (enabled ? "enable_art" : "disable_art")} 已应用到 ${firstArtId}。`,
      });
      await loadRegistryCompatibility();
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法通过兼容接口切换 Art。",
      });
    } finally {
      setCompatBusyAction(null);
    }
  };

  const updateFirstArtDefaultsCompatibility = async () => {
    const firstArt = compatArts[0];
    const firstArtId = String(firstArt?.id || firstArt?.art_id || "");
    if (!firstArtId) {
      setRegistryMessage({ kind: "error", text: "没有可用于 update_art_defaults 的 Art。" });
      return;
    }
    setCompatBusyAction("update-defaults");
    try {
      const response = await updateArtLoomCompatArtDefaults(baseUrl, firstArtId, { compatPreview: true });
      setRegistryMessage({
        kind: "info",
        text: `${response.compatCommand || "update_art_defaults"} 已保存到 ${firstArtId}。`,
      });
      await loadRegistryCompatibility();
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法更新 Art 默认值。",
      });
    } finally {
      setCompatBusyAction(null);
    }
  };

  const probePythonEngineCompatibility = async () => {
    setCompatBusyAction("python-engine-status");
    try {
      const response = await getPythonEngineStatus(baseUrl);
      setPythonEngineSummary(
        `${response.compatCommand || "python_engine_status"}: available=${response.available === true}, arts=${response.installedArtCount ?? 0}`,
      );
      setRegistryMessage({ kind: "info", text: "python_engine_status 检查完成。" });
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法读取 Python 引擎状态。",
      });
    } finally {
      setCompatBusyAction(null);
    }
  };

  const prefetchShaderCompatibility = async () => {
    const targetArtId = (shaderArtId.trim() || pythonArts[0]?.art_id || "").trim();
    if (!targetArtId) {
      setRegistryMessage({ kind: "error", text: "没有可用于 prefetch_shader 的 Python Art。" });
      return;
    }
    setCompatBusyAction("prefetch-shader");
    try {
      const selectedArt = pythonArts.find((art) => art.art_id === targetArtId);
      const response = await prefetchPythonArtShader(baseUrl, targetArtId, selectedArt?.path);
      setRegistryMessage({
        kind: "info",
        text: `${response.compatCommand || "prefetch_shader"} 已完成：${targetArtId}。`,
      });
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法执行 prefetch_shader。",
      });
    } finally {
      setCompatBusyAction(null);
    }
  };

  const removeTool = async (tool: LoomToolDefinition) => {
    setBusyToolId(tool.id);
    try {
      await deleteToolDefinition(baseUrl, tool.id);
      setRegistryMessage({ kind: "info", text: `已删除工具 ${tool.name || tool.id}。` });
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法删除工具。",
      });
    } finally {
      setBusyToolId(null);
    }
  };

  const readSourceFile = async () => {
    if (!sourcePath.trim()) {
      setRegistryMessage({ kind: "error", text: "需要 Python 源码路径。" });
      return;
    }
    setSourceBusyAction("read-source");
    try {
      const response = await readPythonArtSource(baseUrl, sourcePath.trim());
      setSourcePath(response.path);
      setSourceCode(response.content);
      const baseName = basenameWithoutExtension(response.path);
      if (!sourceToolId.trim()) setSourceToolId(normalizeToolId(`python-source-${baseName}`));
      if (!sourceToolName.trim()) setSourceToolName(baseName);
      setRegistryMessage({ kind: "info", text: `已读取 Python 源码 ${response.path}。` });
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法读取 Python 源码。",
      });
    } finally {
      setSourceBusyAction(null);
    }
  };

  const checkNearbyArtJson = async () => {
    if (!sourcePath.trim()) {
      setRegistryMessage({ kind: "error", text: "检查 art.json 前需要 Python 源码路径。" });
      return;
    }
    setSourceBusyAction("check-art-json");
    try {
      const response = await checkPythonArtJsonNearby(baseUrl, sourcePath.trim());
      if (!response.found || !response.artJson) {
        setSourceArtJsonPath("");
        setRegistryMessage({ kind: "info", text: "源码附近没有 art.json。" });
        return;
      }
      setSourceArtJsonPath(response.artJsonPath || "");
      const artJson = response.artJson;
      if (typeof artJson === "object" && artJson !== null) {
        const record = artJson as Record<string, unknown>;
        const artId = typeof record.art_id === "string" ? record.art_id : basenameWithoutExtension(sourcePath);
        const label = typeof record.label === "string" ? record.label : artId;
        const description = typeof record.description === "string" ? record.description : "";
        setSourceToolId(normalizeToolId(`python-source-${artId}`));
        setSourceToolName(label);
        setSourceDescription(description);
      }
      setSourcePorts(mapArtJsonPorts(artJson));
      setRegistryMessage({ kind: "info", text: `已加载 art.json：${response.artJsonPath}。` });
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法检查附近 art.json。",
      });
    } finally {
      setSourceBusyAction(null);
    }
  };

  const readArtJsonByPath = async () => {
    if (!sourceArtJsonPath.trim()) {
      setRegistryMessage({ kind: "error", text: "需要 art.json 路径或 Art 目录。" });
      return;
    }
    setSourceBusyAction("read-art-json");
    try {
      const response = await readPythonArtJson(baseUrl, sourceArtJsonPath.trim());
      if (response.artJson) {
        setSourceArtJsonPath(response.artJsonPath || sourceArtJsonPath);
        setSourcePorts(mapArtJsonPorts(response.artJson));
      }
      setRegistryMessage({ kind: "info", text: `已读取 art.json：${response.artJsonPath || sourceArtJsonPath}。` });
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法读取 art.json。",
      });
    } finally {
      setSourceBusyAction(null);
    }
  };

  const inferSourcePorts = async () => {
    setSourceBusyAction("infer-ports");
    try {
      const response = await inferPythonArtPorts(baseUrl, {
        path: sourcePath.trim() || undefined,
        code: sourcePath.trim() ? undefined : sourceCode,
      });
      const nextPorts = {
        inputs: (response.inputs || []).map(normalizePythonPort),
        outputs: (response.outputs || []).map(normalizePythonPort),
      };
      setSourcePorts(nextPorts);
      setRegistryMessage({
        kind: "info",
        text: `从 Python 源码推断出 ${nextPorts.inputs.length} 个输入、${nextPorts.outputs.length} 个输出。`,
      });
    } catch (error) {
      const fallback = inferPortsFromPythonCode(sourceCode);
      if (sourceCode.trim() && (fallback.inputs.length || fallback.outputs.length)) {
        setSourcePorts(fallback);
        setRegistryMessage({
          kind: "info",
          text: `桌面端回退推断出 ${fallback.inputs.length} 个输入、${fallback.outputs.length} 个输出。`,
        });
      } else {
        setRegistryMessage({
          kind: "error",
          text: error instanceof Error ? error.message : "无法推断 Python 端口。",
        });
      }
    } finally {
      setSourceBusyAction(null);
    }
  };

  const importSourceAsScriptTool = async () => {
    if (!sourcePath.trim()) {
      setRegistryMessage({ kind: "error", text: "导入脚本工具前需要 Python 源码路径。" });
      return;
    }
    const toolId = normalizeToolId(sourceToolId || `python-source-${basenameWithoutExtension(sourcePath)}`);
    const fallbackPorts = sourcePorts || inferPortsFromPythonCode(sourceCode);
    const tool: LoomToolDefinition = {
      id: toolId,
      name: sourceToolName.trim() || basenameWithoutExtension(sourcePath),
      description: sourceDescription.trim() || "通过 Loom 桌面导入的 Python 源码。",
      enabled: true,
      execution: {
        type: "script",
        path: sourcePath.trim(),
      },
      inputs: fallbackPorts.inputs.map((input) => ({
        name: input.name,
        label: input.label,
        type: input.type,
        executionType: input.executionType,
        default: input.default,
      })),
      outputs: fallbackPorts.outputs.map((output) => ({
        name: output.name,
        label: output.label,
        type: output.type,
        executionType: output.executionType,
      })),
    };

    setSourceBusyAction("import-source");
    try {
      await saveToolDefinition(baseUrl, tool);
      setSourceToolId(toolId);
      setRegistryMessage({ kind: "info", text: `已导入脚本工具 ${toolId}。` });
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法导入脚本工具。",
      });
    } finally {
      setSourceBusyAction(null);
    }
  };

  const createArtToolFromWizard = async (draft: ArtWizardSubmitDraft) => {
    const selectedFramework = frameworks.find(
      (framework) => (framework.qualifiedId || framework.id) === draft.mode,
    );
    const modeInfo = artModeById(draft.mode, selectedFramework ? [{
      id: draft.mode,
      title: selectedFramework.authoringSchema?.title || selectedFramework.name,
      subtitle: selectedFramework.authoringSchema?.description || selectedFramework.description,
      executionLabel: selectedFramework.qualifiedId || selectedFramework.id,
    }] : artWizardModes);
    const selectedPythonArt = pythonArts.find((art) => art.art_id === draft.pythonArtId);
    const selectedWorkflow = workflows.find((workflow) => workflow.id === draft.workflowId);
    const derivedName = draft.name.trim() || selectedPythonArt?.label || selectedWorkflow?.name || modeInfo.title;
    const toolId = normalizeToolId(draft.toolId || `${draft.mode}-${derivedName}`);
    const description = draft.description.trim() || modeInfo.subtitle;
    if (selectedFramework?.authoringSchema) {
      setBusyWizard(true);
      try {
        const authored = buildAuthoredArtPackage(selectedFramework, {
          id: toolId,
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
        await createAuthoredArtPackage(baseUrl, authored.tool, authored.runtime);
        setRegistryMessage({
          kind: "info",
          text: `已通过 ${selectedFramework.name} 的 authoring schema 创建并安装 Art ${derivedName}。`,
        });
        await refresh();
        await reloadFrameworks();
      } catch (error) {
        setRegistryMessage({
          kind: "error",
          text: error instanceof Error ? error.message : "无法创建框架 Art 包。",
        });
      } finally {
        setBusyWizard(false);
      }
      return;
    }
    let execution: LoomToolExecution;
    const fallbackPorts = defaultWizardPorts(draft.mode);
    const inputs = (draft.inputPorts.length ? draft.inputPorts : fallbackPorts.inputs)
      .map((port) => toolPortFromDraft(port, "input"));
    const outputs = (draft.outputPorts.length ? draft.outputPorts : fallbackPorts.outputs)
      .map((port) => toolPortFromDraft(port, "output"));
    const params = draft.shaderMode && draft.mode === "script"
      ? [{
        id: "shaderMode",
        label: "Shader 模式",
        widget: "checkbox",
        dataType: "bool",
        default: true,
      }]
      : [];

    switch (draft.mode) {
      case "cli_wrapper": {
        execution = {
          type: "cli_wrapper",
          command: draft.command.trim() || "echo",
          args: parseListText(draft.argsText),
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
      case "script": {
        if (!draft.scriptPath.trim()) {
          setRegistryMessage({ kind: "error", text: "脚本 / Python Art 需要 Python 脚本路径。" });
          return;
        }
        execution = {
          type: "script",
          path: draft.scriptPath.trim(),
        };
        break;
      }
      case "mcp": {
        if (!draft.mcpServerId.trim() || !draft.mcpToolName.trim()) {
          setRegistryMessage({ kind: "error", text: "MCP 关联 Art 需要 MCP 服务和工具名。" });
          return;
        }
        execution = {
          type: "mcp",
          serverId: draft.mcpServerId.trim(),
          toolName: draft.mcpToolName.trim(),
        };
        break;
      }
      case "python_art": {
        if (!selectedPythonArt) {
          setRegistryMessage({ kind: "error", text: "已安装 Python Art 需要选择目录项。" });
          return;
        }
        execution = {
          type: "python_art",
          artId: selectedPythonArt.art_id,
          artPath: selectedPythonArt.path,
        };
        break;
      }
      case "workflow": {
        if (!draft.workflowId.trim()) {
          setRegistryMessage({ kind: "error", text: "工作流 Art 需要已保存工作流。" });
          return;
        }
        execution = {
          type: "workflow",
          workflowId: draft.workflowId.trim(),
        };
        break;
      }
      case "native_image": {
        const nativeArgs = parseListText(draft.argsText);
        execution = {
          type: "cli_wrapper",
          command: draft.command.trim() || "loom-image-filter",
          args: nativeArgs.length ? nativeArgs : [
            "--filter",
            draft.nativeFilter.trim() || "identity",
            "--input",
            "{{inputs.image.path}}",
            "--output",
            "{{outputs.result.path}}",
          ],
        };
        break;
      }
      default: {
        setRegistryMessage({ kind: "error", text: `框架 ${draft.mode} 没有可用的 authoring schema。` });
        return;
      }
    }

    const tool: LoomToolDefinition = {
      id: toolId,
      name: derivedName,
      description,
      enabled: true,
      execution,
      inputs,
      outputs,
      params,
    };

    setBusyWizard(true);
    try {
      await saveToolDefinition(baseUrl, tool);
      setRegistryMessage({ kind: "info", text: `已创建 Art ${derivedName}，ID：${toolId}。` });
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法通过添加 Art 向导创建 Art。",
      });
    } finally {
      setBusyWizard(false);
    }
  };

  return (
    <section className="content-grid">
      {registryMessage ? (
        <p className={registryMessage.kind === "error" ? "error-text" : "success-text"}>{registryMessage.text}</p>
      ) : null}
      <div className="section-heading-row">
        <div>
          <p className="section-kicker">Art 注册表卡片</p>
          <h3>已保存 Art / 工具定义</h3>
        </div>
        <span className="mini-chip">{visibleTools.length}</span>
      </div>
      <div className="card-grid">
        {visibleTools.length ? visibleTools.map((tool) => (
          <article className="glass-card control-card art-registry-card" key={tool.id}>
            <div className="control-card__head">
              <div>
                <p className="card-kicker">{tool.execution?.type ?? "tool"}</p>
                <h3>{tool.name || tool.id}</h3>
              </div>
              <EnabledChip enabled={tool.enabled} />
            </div>
            <p>{firstWords(tool.description, "无描述。")}</p>
            <div className="port-summary">
              <span>输入 {(tool.inputs || []).length || "auto"}</span>
              <span>输出 {(tool.outputs || []).length || "auto"}</span>
              <span>参数 {Object.keys(tool.execution || {}).length}</span>
            </div>
            <button
              className="ghost-button"
              type="button"
              onClick={() => removeTool(tool)}
              disabled={busyToolId === tool.id}
            >
              {busyToolId === tool.id ? "删除中" : "删除工具"}
            </button>
          </article>
        )) : (
          <article className="glass-card empty-card">
            <h3>{tools.length ? "所选框架下暂无 Art" : "暂无工具"}</h3>
          </article>
        )}
      </div>
      <div className="card-grid">
        <article className="glass-card control-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">ArtLoom 注册表兼容</p>
              <h3>旧 Art 注册表别名</h3>
            </div>
            <span className="mini-chip">{compatArtCount ?? compatArts.length} 个 Art</span>
          </div>
          <div className="studio-actions">
            <button
              className="ghost-button"
              type="button"
              onClick={loadRegistryCompatibility}
              disabled={Boolean(compatBusyAction)}
            >
              {compatBusyAction === "list-arts" ? "读取中" : "list_arts"}
            </button>
            <button
              className="ghost-button"
              type="button"
              onClick={syncRegistryCompatibility}
              disabled={Boolean(compatBusyAction)}
            >
              {compatBusyAction === "sync-user-arts" ? "同步中" : "sync_user_arts"}
            </button>
            <button
              className="ghost-button"
              type="button"
              onClick={() => toggleFirstArtCompatibility(true)}
              disabled={Boolean(compatBusyAction) || !compatArts.length}
            >
              enable_art
            </button>
            <button
              className="ghost-button"
              type="button"
              onClick={() => toggleFirstArtCompatibility(false)}
              disabled={Boolean(compatBusyAction) || !compatArts.length}
            >
              disable_art
            </button>
            <button
              className="ghost-button"
              type="button"
              onClick={updateFirstArtDefaultsCompatibility}
              disabled={Boolean(compatBusyAction) || !compatArts.length}
            >
              update_art_defaults
            </button>
          </div>
        </article>
        <article className="glass-card control-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">Python 引擎兼容</p>
              <h3>python_engine_status / prefetch_shader</h3>
            </div>
            <span className="mini-chip">{pythonArts.length} 个 Python Art</span>
          </div>
          <p className="tiny-text">{pythonEngineSummary}</p>
          <label className="field-stack">
            <span>Shader Art ID</span>
            <input
              value={shaderArtId}
              onChange={(event) => setShaderArtId(event.target.value)}
              placeholder={pythonArts[0]?.art_id || "art_shader"}
            />
          </label>
          <div className="studio-actions">
            <button
              className="ghost-button"
              type="button"
              onClick={probePythonEngineCompatibility}
              disabled={Boolean(compatBusyAction)}
            >
              {compatBusyAction === "python-engine-status" ? "检查中" : "python_engine_status"}
            </button>
            <button
              className="ghost-button"
              type="button"
              onClick={prefetchShaderCompatibility}
              disabled={Boolean(compatBusyAction) || (!shaderArtId.trim() && !pythonArts.length)}
            >
              {compatBusyAction === "prefetch-shader" ? "预热中" : "prefetch_shader"}
            </button>
          </div>
        </article>
      </div>
      <AddArtWizard
        baseUrl={baseUrl}
        frameworks={frameworks}
        mcpServers={mcpServers}
        pythonArts={pythonArts}
        workflows={workflows}
        busy={busyWizard}
        onCreate={createArtToolFromWizard}
      />
      <div className="main-board python-art-board">
        <p className="section-kicker">Python Art 目录</p>
        <h2>已安装 Python Art</h2>
        <div className="studio-actions">
          <button className="ghost-button" type="button" onClick={refresh}>
            刷新 Python Art
          </button>
          <button className="ghost-button" type="button" onClick={() => openExternal(`${baseUrl}/v1/python-arts`)}>
            查看 Python Art JSON
          </button>
        </div>
      </div>
      <div className="studio-grid">
        <article className="glass-card studio-card studio-card--wide">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">源码辅助</p>
              <h3>Python 源码导入</h3>
            </div>
            <span className="mini-chip">{sourcePorts ? `${sourcePorts.inputs.length}/${sourcePorts.outputs.length}` : "ports"}</span>
          </div>
          <label className="field-label">
            Python 源码路径
            <input
              className="studio-input"
              value={sourcePath}
              onChange={(event) => setSourcePath(event.target.value)}
              placeholder="C:\\path\\to\\main.py"
            />
          </label>
          <label className="field-label">
            art.json 路径或 Art 目录
            <input
              className="studio-input"
              value={sourceArtJsonPath}
              onChange={(event) => setSourceArtJsonPath(event.target.value)}
              placeholder="C:\\path\\to\\art.json"
            />
          </label>
          <div className="studio-actions">
            <button
              className="ghost-button"
              type="button"
              onClick={readSourceFile}
              disabled={Boolean(sourceBusyAction)}
            >
              {sourceBusyAction === "read-source" ? "读取中" : "读取 Python 源码"}
            </button>
            <button
              className="ghost-button"
              type="button"
              onClick={checkNearbyArtJson}
              disabled={Boolean(sourceBusyAction)}
            >
              {sourceBusyAction === "check-art-json" ? "检查中" : "检查附近 art.json"}
            </button>
            <button
              className="ghost-button"
              type="button"
              onClick={readArtJsonByPath}
              disabled={Boolean(sourceBusyAction)}
            >
              {sourceBusyAction === "read-art-json" ? "读取中" : "读取 art.json"}
            </button>
            <button
              className="signal-button"
              type="button"
              onClick={inferSourcePorts}
              disabled={Boolean(sourceBusyAction)}
            >
              {sourceBusyAction === "infer-ports" ? "推断中" : "从 Python 源码推断端口"}
            </button>
          </div>
          <label className="field-label">
            源码预览
            <textarea
              className="studio-textarea"
              value={sourceCode}
              onChange={(event) => setSourceCode(event.target.value)}
              placeholder="def run(args): ..."
              spellCheck={false}
            />
          </label>
        </article>
        <article className="glass-card studio-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">脚本工具</p>
              <h3>导入为脚本工具</h3>
            </div>
            <span className="mini-chip">{sourceToolId || "新建"}</span>
          </div>
          <label className="field-label">
            工具 ID
            <input
              className="studio-input"
              value={sourceToolId}
              onChange={(event) => setSourceToolId(event.target.value)}
              placeholder="python-source-my-art"
            />
          </label>
          <label className="field-label">
            工具名称
            <input
              className="studio-input"
              value={sourceToolName}
              onChange={(event) => setSourceToolName(event.target.value)}
              placeholder="My Python Source"
            />
          </label>
          <label className="field-label">
            描述
            <textarea
              className="studio-textarea studio-textarea--compact"
              value={sourceDescription}
              onChange={(event) => setSourceDescription(event.target.value)}
              placeholder="描述导入的源码工具"
            />
          </label>
          <button
            className="signal-button"
            type="button"
            onClick={importSourceAsScriptTool}
            disabled={Boolean(sourceBusyAction)}
          >
            {sourceBusyAction === "import-source" ? "导入中" : "导入为脚本工具"}
          </button>
          {sourcePorts ? (
            <pre className="studio-json">
              {JSON.stringify({ inputs: sourcePorts.inputs, outputs: sourcePorts.outputs }, null, 2)}
            </pre>
          ) : (
            <div className="empty-card">
              读取源码、检查 art.json 或推断端口后会显示预览。
            </div>
          )}
        </article>
      </div>
      <div className="card-grid">
        {pythonArts.length ? pythonArts.map((art) => (
          <article className="glass-card control-card" key={art.art_id}>
            <div className="control-card__head">
              <div>
                <p className="card-kicker">python_art</p>
                <h3>{art.label || art.art_id}</h3>
              </div>
              <span className="mini-chip">{art.version || "1.0.0"}</span>
            </div>
            <p>{firstWords(art.description, "已安装 Python Art。")}</p>
            <p className="mono-line">{art.path}</p>
            <button
              className="ghost-button"
              type="button"
              onClick={() => importPythonArt(art)}
              disabled={busyArtId === art.art_id}
            >
              {busyArtId === art.art_id ? "导入中" : "导入为 Loom 工具"}
            </button>
          </article>
        )) : (
          <article className="glass-card empty-card">
            <h3>未找到 Python Art</h3>
          </article>
        )}
      </div>
    </section>
  );
}

function PluginSecurityPanel({ baseUrl }: { baseUrl: string }) {
  const [publishers, setPublishers] = useState<LoomPublisherTrustRecord[]>([]);
  const [credentials, setCredentials] = useState<LoomCredentialSummary[]>([]);
  const [publisherId, setPublisherId] = useState("");
  const [keyId, setKeyId] = useState("");
  const [publicKey, setPublicKey] = useState("");
  const [credentialName, setCredentialName] = useState("");
  const [credentialValue, setCredentialValue] = useState("");
  const [credentialFramework, setCredentialFramework] = useState("");
  const [credentialArt, setCredentialArt] = useState("");
  const [credentialExpiresAt, setCredentialExpiresAt] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const loadVersion = useRef(0);

  const load = useCallback(async () => {
    const version = ++loadVersion.current;
    try {
      const [trust, summaries] = await Promise.all([
        listPluginTrust(baseUrl),
        listPluginCredentials(baseUrl),
      ]);
      if (version !== loadVersion.current) return;
      setPublishers(trust);
      setCredentials(summaries);
      setMessage(null);
    } catch (err) {
      if (version === loadVersion.current) {
        setMessage({ ok: false, text: err instanceof Error ? err.message : "无法读取插件安全状态。" });
      }
    }
  }, [baseUrl]);

  useEffect(() => {
    void load();
    return () => {
      loadVersion.current += 1;
    };
  }, [load]);

  const addPublisher = async () => {
    const normalizedPublisherId = publisherId.trim();
    const normalizedKeyId = keyId.trim();
    const normalizedPublicKey = publicKey.trim();
    if (!window.confirm(`信任发布者密钥 ${normalizedPublisherId}/${normalizedKeyId}？`)) return;
    setBusy("publisher");
    setMessage(null);
    try {
      const trust = await trustPluginPublisher(baseUrl, {
        publisherId: normalizedPublisherId,
        keyId: normalizedKeyId,
        publicKey: normalizedPublicKey,
      });
      setPublishers(trust);
      setPublisherId("");
      setKeyId("");
      setPublicKey("");
      setMessage({ ok: true, text: `已信任 ${normalizedPublisherId}/${normalizedKeyId}。` });
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : "添加信任失败。" });
    } finally {
      setBusy(null);
    }
  };

  const revokePublisher = async (publisher: LoomPublisherTrustRecord) => {
    const id = `${publisher.publisherId}/${publisher.keyId}`;
    if (!window.confirm(`吊销发布者密钥 ${id}？已安装包会在下次完整性检查时被拒绝。`)) return;
    setBusy(id);
    setMessage(null);
    try {
      setPublishers(await revokePluginPublisher(baseUrl, publisher.publisherId, publisher.keyId));
      setMessage({ ok: true, text: `已吊销 ${id}。` });
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : "吊销失败。" });
    } finally {
      setBusy(null);
    }
  };

  const saveCredential = async () => {
    const normalizedName = credentialName.trim();
    setBusy("credential");
    setMessage(null);
    try {
      await savePluginCredential(baseUrl, {
        name: normalizedName,
        value: credentialValue,
        scope: {
          ...(credentialFramework.trim() ? { frameworkId: credentialFramework.trim() } : {}),
          ...(credentialArt.trim() ? { artId: credentialArt.trim() } : {}),
        },
        ...(credentialExpiresAt.trim() ? { expiresAt: credentialExpiresAt.trim() } : {}),
      });
      setCredentialValue("");
      setCredentials(await listPluginCredentials(baseUrl));
      setCredentialName(normalizedName);
      setMessage({ ok: true, text: `已保存凭据 ${normalizedName}；值已从界面清除。` });
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : "保存凭据失败。" });
    } finally {
      setBusy(null);
    }
  };

  const removeCredential = async (credential: LoomCredentialSummary) => {
    const id = `${credential.name}:${credential.scope.frameworkId || "*"}:${credential.scope.artId || "*"}`;
    if (!window.confirm(`删除凭据 ${credential.name}（framework=${credential.scope.frameworkId || "*"}, art=${credential.scope.artId || "*"}）？`)) return;
    setBusy(id);
    setMessage(null);
    try {
      await deletePluginCredential(baseUrl, credential.name, credential.scope);
      setCredentials(await listPluginCredentials(baseUrl));
      setMessage({ ok: true, text: `已删除凭据 ${credential.name}。` });
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : "删除凭据失败。" });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="main-board">
      <p className="section-kicker">插件安全</p>
      <h3>发布者信任与作用域凭据</h3>
      <p className="muted-line">
        Verified 只表示签名有效，Trusted 才表示密钥已被本机信任。凭据值只写入、不回读，按 framework/art 作用域授予。
      </p>
      {message ? <p className={message.ok ? "success-text" : "error-text"}>{message.text}</p> : null}
      <div className="card-grid">
        <article className="glass-card control-card">
          <div className="control-card__head">
            <div><p className="card-kicker">信任库</p><h3>添加发布者密钥</h3></div>
            <span className="mini-chip">{publishers.length} keys</span>
          </div>
          <input aria-label="发布者 ID" className="hook-canvas-param-expose__value" placeholder="publisherId" value={publisherId} onChange={(event) => setPublisherId(event.target.value)} />
          <input aria-label="发布者密钥 ID" className="hook-canvas-param-expose__value" placeholder="keyId" value={keyId} onChange={(event) => setKeyId(event.target.value)} />
          <textarea aria-label="Ed25519 发布者公钥" className="hook-canvas-param-expose__value" placeholder="Ed25519 publicKey (base64)" value={publicKey} onChange={(event) => setPublicKey(event.target.value)} />
          <button className="signal-button" type="button" disabled={busy !== null || !publisherId.trim() || !keyId.trim() || !publicKey.trim()} onClick={() => void addPublisher()}>
            {busy === "publisher" ? "保存中" : "信任发布者"}
          </button>
          {publishers.map((publisher) => {
            const id = `${publisher.publisherId}/${publisher.keyId}`;
            return (
              <div key={id} style={{ marginTop: 12 }}>
                <p className="mono-line">{id}</p>
                <span className={publisher.revoked ? "mini-chip" : "mini-chip mini-chip--ok"}>{publisher.revoked ? "revoked" : "trusted"}</span>
                {!publisher.revoked ? (
                  <button className="ghost-button" type="button" disabled={busy !== null} onClick={() => void revokePublisher(publisher)} style={{ marginLeft: 8 }}>
                    {busy === id ? "吊销中" : "吊销"}
                  </button>
                ) : null}
              </div>
            );
          })}
        </article>
        <article className="glass-card control-card">
          <div className="control-card__head">
            <div><p className="card-kicker">凭据 Broker</p><h3>保存作用域凭据</h3></div>
            <span className="mini-chip">{credentials.length} credentials</span>
          </div>
          <input aria-label="凭据名称" className="hook-canvas-param-expose__value" placeholder="name" value={credentialName} onChange={(event) => setCredentialName(event.target.value)} />
          <input aria-label="凭据值" className="hook-canvas-param-expose__value" type="password" autoComplete="new-password" placeholder="write-only value" value={credentialValue} onChange={(event) => setCredentialValue(event.target.value)} />
          <input aria-label="凭据框架作用域" className="hook-canvas-param-expose__value" placeholder="frameworkId（可选，支持 publisher/id）" value={credentialFramework} onChange={(event) => setCredentialFramework(event.target.value)} />
          <input aria-label="凭据 Art 作用域" className="hook-canvas-param-expose__value" placeholder="artId（可选，支持 publisher/id）" value={credentialArt} onChange={(event) => setCredentialArt(event.target.value)} />
          <input aria-label="凭据过期时间" className="hook-canvas-param-expose__value" placeholder="expiresAt RFC3339（可选）" value={credentialExpiresAt} onChange={(event) => setCredentialExpiresAt(event.target.value)} />
          <button className="signal-button" type="button" disabled={busy !== null || !credentialName.trim() || !credentialValue} onClick={() => void saveCredential()}>
            {busy === "credential" ? "保存中" : "保存凭据"}
          </button>
          {credentials.map((credential) => {
            const id = `${credential.name}:${credential.scope.frameworkId || "*"}:${credential.scope.artId || "*"}`;
            return (
              <div key={id} style={{ marginTop: 12 }}>
                <p className="mono-line">{credential.name} · framework={credential.scope.frameworkId || "*"} · art={credential.scope.artId || "*"}</p>
                <p className="muted-line">{credential.protection}{credential.expiresAt ? ` · expires ${credential.expiresAt}` : ""}</p>
                <button className="ghost-button" type="button" disabled={busy !== null} onClick={() => void removeCredential(credential)}>
                  {busy === id ? "删除中" : "删除"}
                </button>
              </div>
            );
          })}
        </article>
      </div>
    </div>
  );
}

function ArtStoreCard({
  baseUrl,
  onInstalled,
}: {
  baseUrl: string;
  onInstalled: () => void | Promise<void>;
}) {
  const [store, setStore] = useState("");
  const [catalog, setCatalog] = useState<ArtStoreEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const browse = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const arts = await fetchArtStoreCatalog(baseUrl, store || undefined);
      setCatalog(arts);
      if (arts.length === 0) setMessage({ ok: true, text: "商店目录为空。" });
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : "无法访问商店。" });
    } finally {
      setBusy(false);
    }
  };

  const install = async (artId: string) => {
    setBusy(true);
    setMessage(null);
    try {
      await installArtFromStore(baseUrl, artId, store || undefined);
      setMessage({ ok: true, text: `已安装 ${artId}（含依赖）。` });
      await onInstalled();
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : "安装失败。" });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="main-board">
      <p className="section-kicker">art 商店</p>
      <h3>从商店安装 art</h3>
      <p className="muted-line">
        安装 art 会自动装齐其依赖（第三方 exe、附属 art）。缺框架会提示先装框架。商店地址留空则用
        LOOM_ART_STORE_URL。
      </p>
      <div className="hook-canvas-toolbar__controls" style={{ marginBottom: 12 }}>
        <input
          className="hook-canvas-param-expose__value"
          style={{ flex: "1 1 auto" }}
          placeholder="商店地址（可留空）"
          value={store}
          onChange={(event) => setStore(event.target.value)}
        />
        <button className="signal-button" type="button" onClick={browse} disabled={busy}>
          {busy ? "处理中" : "浏览商店"}
        </button>
      </div>
      {message ? (
        <p className={message.ok ? "success-text" : "error-text"}>{message.text}</p>
      ) : null}
      <div className="card-grid">
        {catalog.map((art) => (
          <article className="glass-card control-card" key={art.id}>
            <div className="control-card__head">
              <div>
                <p className="card-kicker">{art.framework ?? "art"}</p>
                <h3>{art.name ?? art.id}</h3>
              </div>
            </div>
            <p>{art.description ?? ""}</p>
            <p className="mono-line">{art.id}</p>
            <button
              className="signal-button"
              type="button"
              onClick={() => install(art.id)}
              disabled={busy}
            >
              安装
            </button>
          </article>
        ))}
      </div>
    </div>
  );
}

function FrameworkFilter({
  frameworks,
  selectedFrameworkIds,
  onToggle,
  onManage,
  manageButtonRef,
}: {
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  onToggle: (frameworkId: string) => void;
  onManage: () => void;
  manageButtonRef: (element: HTMLButtonElement | null) => void;
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
      <button
        className="ghost-button framework-filter__manage"
        type="button"
        ref={manageButtonRef}
        onClick={onManage}
      >
        管理框架
      </button>
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
  pythonArts,
  mcpServers,
  workflows,
  baseUrl,
  refresh,
}: {
  tools: LoomToolDefinition[];
  pythonArts: LoomPythonArt[];
  mcpServers: LoomMcpServer[];
  workflows: LoomWorkflowMetadata[];
  baseUrl: string;
  refresh: () => Promise<void>;
}) {
  const [activeWorkspace, setActiveWorkspace] = useState<ArtWorkspaceId>("registry");
  const [frameworks, setFrameworks] = useState<LoomFramework[]>([]);
  const [frameworkBusyId, setFrameworkBusyId] = useState<string | null>(null);
  const [frameworkBusyAction, setFrameworkBusyAction] = useState<FrameworkBusyAction>(null);
  const [frameworkError, setFrameworkError] = useState<string | null>(null);
  const [frameworkManagementMessage, setFrameworkManagementMessage] = useState<StudioMessage | null>(null);
  const [frameworkDialogOpen, setFrameworkDialogOpen] = useState(false);
  const [snapshotRefreshError, setSnapshotRefreshError] = useState<string | null>(null);
  const [selectedFrameworkIds, setSelectedFrameworkIds] = useState<Set<string> | null>(null);
  const frameworkLoadVersion = useRef(0);
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
          className={activeWorkspace === "registry" ? "art-hub__tabs art-hub__tabs--with-filter" : "art-hub__tabs"}
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
            onManage={() => {
              setFrameworkManagementMessage(null);
              setFrameworkDialogOpen(true);
            }}
            manageButtonRef={(element) => {
              frameworkManageButtonRef.current = element;
            }}
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
          pythonArts={pythonArts}
          mcpServers={mcpServers}
          workflows={workflows}
          frameworks={frameworks}
          selectedFrameworkIds={selectedFrameworkIds}
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
        <ArtStoreCard baseUrl={baseUrl} onInstalled={synchronizeArtState} />
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
    </section>
  );
}

function HookBridgePanel({
  baseUrl,
  hookCanvas,
  hookCanvasError,
  tools,
  onOpenWorkflow,
}: {
  baseUrl: string;
  hookCanvas: HookCanvasSnapshot | null;
  hookCanvasError: string | null;
  tools: LoomToolDefinition[];
  onOpenWorkflow: (selectedNodeId?: string) => void;
}) {
  return (
    <HookCanvasThumbnail
      snapshot={hookCanvas}
      baseUrl={baseUrl}
      error={hookCanvasError}
      tools={tools}
      onOpenWorkflow={onOpenWorkflow}
    />
  );
}

function AgentsPanel({ capabilities }: { capabilities: LoomCapability[] }) {
  return (
    <section className="content-grid">
      <div className="main-board">
        <p className="section-kicker">智能体</p>
        <h2>本地能力</h2>
      </div>
      <div className="card-grid">
        {capabilities.length ? capabilities.map((capability) => (
          <CapabilityCard key={capability.id} capability={capability} />
        )) : (
          <article className="glass-card empty-card">
            <h3>暂无能力</h3>
          </article>
        )}
      </div>
    </section>
  );
}

function RunsPanel() {
  return (
    <section className="main-board">
      <p className="section-kicker">运行记录</p>
      <h2>运行证据时间线</h2>
      <div className="terminal-list">
        <span>GET /v1/runs/&lt;run_id&gt;</span>
        <span>GET /v1/runs/&lt;run_id&gt;/events</span>
        <span>POST /v1/invoke</span>
      </div>
    </section>
  );
}

function SettingsPanel({ snapshot }: { snapshot: LoomSnapshot }) {
  const links = [
    ["Loom 设置", snapshot.settings.root],
    ["Tea 设置", snapshot.settings.tea],
    ["Hook 设置", snapshot.settings.hook],
    ["Talk 设置", snapshot.settings.talk],
  ] as const;
  const [draft, setDraft] = useState<ArtLoomCompatSettings>(DEFAULT_ARTLOOM_COMPAT_SETTINGS);
  const [shortcuts, setShortcuts] = useState<ArtLoomShortcutConfig[]>(
    Object.values(DEFAULT_ARTLOOM_COMPAT_SETTINGS.shortcuts),
  );
  const [appPaths, setAppPaths] = useState<ArtLoomAppPaths | null>(null);
  const [settingsMessage, setSettingsMessage] = useState<StudioMessage | null>(null);
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [shortcutBusyId, setShortcutBusyId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const loadSettings = async () => {
      try {
        const [loadedSettings, loadedShortcuts, loadedPaths] = await Promise.all([
          getArtLoomCompatSettings(snapshot.baseUrl),
          getArtLoomCompatShortcuts(snapshot.baseUrl),
          getArtLoomCompatAppPaths(snapshot.baseUrl),
        ]);
        if (cancelled) return;
        setDraft(loadedSettings);
        setShortcuts(loadedShortcuts.length ? loadedShortcuts : Object.values(DEFAULT_ARTLOOM_COMPAT_SETTINGS.shortcuts));
        setAppPaths(loadedPaths);
      } catch (error) {
        if (cancelled) return;
        setDraft(DEFAULT_ARTLOOM_COMPAT_SETTINGS);
        setShortcuts(Object.values(DEFAULT_ARTLOOM_COMPAT_SETTINGS.shortcuts));
        setSettingsMessage({
          kind: "error",
          text: error instanceof Error
            ? `使用 ArtLoom 兼容默认设置：${error.message}`
            : "使用 ArtLoom 兼容默认设置。",
        });
      }
    };
    void loadSettings();
    return () => {
      cancelled = true;
    };
  }, [snapshot.baseUrl]);

  const saveSettingsDraft = async () => {
    setSettingsBusy(true);
    try {
      const saved = await saveArtLoomCompatSettings(snapshot.baseUrl, draft);
      setDraft(saved);
      setSettingsMessage({ kind: "info", text: "已保存 ArtLoom 兼容设置。" });
    } catch (error) {
      setSettingsMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法保存 ArtLoom 兼容设置。",
      });
    } finally {
      setSettingsBusy(false);
    }
  };

  const updateShortcutDraft = (id: string, patch: Partial<ArtLoomShortcutConfig>) => {
    setShortcuts((previous) => previous.map((shortcut) => (
      shortcut.id === id ? { ...shortcut, ...patch } : shortcut
    )));
  };

  const saveShortcutDraft = async (shortcut: ArtLoomShortcutConfig) => {
    setShortcutBusyId(shortcut.id);
    try {
      const saved = await updateArtLoomCompatShortcut(snapshot.baseUrl, shortcut);
      updateShortcutDraft(saved.id, saved);
      setSettingsMessage({ kind: "info", text: `已保存快捷键 ${saved.label}。` });
    } catch (error) {
      setSettingsMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法保存快捷键。",
      });
    } finally {
      setShortcutBusyId(null);
    }
  };

  const toggleAutostart = async () => {
    const enabled = !draft.general.auto_start;
    setDraft((current) => ({ ...current, general: { ...current.general, auto_start: enabled } }));
    try {
      const result = await setArtLoomCompatAutostart(snapshot.baseUrl, enabled);
      setSettingsMessage({
        kind: "info",
        text: `开机自启已保存（${result.mode || "compat-preview"}，sideEffect=${result.sideEffect === true}）。`,
      });
    } catch (error) {
      setSettingsMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法更新开机自启。",
      });
    }
  };

  const toggleMinimizeToTray = async () => {
    const enabled = !draft.general.minimize_to_tray;
    setDraft((current) => ({ ...current, general: { ...current.general, minimize_to_tray: enabled } }));
    try {
      const result = await setArtLoomCompatMinimizeToTray(snapshot.baseUrl, enabled);
      setSettingsMessage({
        kind: "info",
        text: `最小化到托盘已保存（${result.mode || "compat-preview"}，sideEffect=${result.sideEffect === true}）。`,
      });
    } catch (error) {
      setSettingsMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法更新托盘设置。",
      });
    }
  };

  return (
    <section className="content-grid legacy-settings">
      <div className="main-board">
        <p className="section-kicker">设置</p>
        <h2>本地配置</h2>
        <div className="settings-grid">
          {links.map(([label, url]) => (
            <button className="settings-link" type="button" key={label} onClick={() => openExternal(url)}>
              <span>{label}</span>
              <small>{url}</small>
            </button>
          ))}
        </div>
        <div className="studio-actions">
          <button className="signal-button" type="button" onClick={saveSettingsDraft} disabled={settingsBusy}>
            {settingsBusy ? "保存中" : "保存兼容设置"}
          </button>
        </div>
        {settingsMessage ? (
          <p className={settingsMessage.kind === "error" ? "error-text" : "success-text"}>{settingsMessage.text}</p>
        ) : null}
      </div>

      <div className="legacy-settings-grid">
        <article className="glass-card settings-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">通用设置</p>
              <h3>通用 / 托盘</h3>
            </div>
            <span className="mini-chip">get_settings</span>
          </div>
          <div className="settings-field-grid">
            <label className="field-label">
              语言
              <select
                className="studio-input"
                value={draft.general.language}
                onChange={(event) => setDraft((current) => ({
                  ...current,
                  general: { ...current.general, language: event.target.value },
                }))}
              >
                <option value="en">英文</option>
                <option value="zh-Hans">简体中文</option>
              </select>
            </label>
            <label className="field-label">
              主题
              <select
                className="studio-input"
                value={draft.general.theme}
                onChange={(event) => setDraft((current) => ({
                  ...current,
                  general: { ...current.general, theme: event.target.value },
                }))}
              >
                <option value="dark">深色</option>
                <option value="light">浅色</option>
                <option value="system">跟随系统</option>
              </select>
            </label>
          </div>
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={draft.general.enable_tray_icon}
              onChange={(event) => setDraft((current) => ({
                ...current,
                general: { ...current.general, enable_tray_icon: event.target.checked },
              }))}
            />
            <span>启用托盘图标</span>
          </label>
          <label className="toggle-row">
            <input type="checkbox" checked={draft.general.minimize_to_tray} onChange={toggleMinimizeToTray} />
            <span>最小化到托盘</span>
          </label>
        </article>

        <article className="glass-card settings-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">引擎设置</p>
              <h3>引擎配置</h3>
            </div>
            <span className="mini-chip">update_settings</span>
          </div>
          <label className="field-label">
            ComfyUI API 地址
            <input
              className="studio-input"
              value={draft.engine.comfyui_url}
              onChange={(event) => setDraft((current) => ({
                ...current,
                engine: { ...current.engine, comfyui_url: event.target.value },
              }))}
            />
          </label>
          <label className="field-label">
            Python 解释器
            <input
              className="studio-input"
              value={draft.engine.python_interpreter}
              onChange={(event) => setDraft((current) => ({
                ...current,
                engine: { ...current.engine, python_interpreter: event.target.value },
              }))}
            />
          </label>
          <label className="field-label">
            虚拟环境路径
            <input
              className="studio-input"
              value={draft.engine.virtual_env_path}
              onChange={(event) => setDraft((current) => ({
                ...current,
                engine: { ...current.engine, virtual_env_path: event.target.value },
              }))}
            />
          </label>
          <div className="settings-field-grid">
            <label className="field-label">
              计算设备
              <select
                className="studio-input"
                value={draft.engine.compute_device}
                onChange={(event) => setDraft((current) => ({
                  ...current,
                  engine: { ...current.engine, compute_device: event.target.value },
                }))}
              >
                <option value="0">CUDA 0</option>
                <option value="cpu">CPU</option>
              </select>
            </label>
            <label className="field-label">
              显存预留（GB）
              <input
                className="studio-input"
                type="number"
                min="1"
                value={draft.engine.vram_reservation_gb}
                onChange={(event) => setDraft((current) => ({
                  ...current,
                  engine: { ...current.engine, vram_reservation_gb: Number(event.target.value) || 1 },
                }))}
              />
            </label>
          </div>
        </article>

        <article className="glass-card settings-card settings-card--wide">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">快捷键设置</p>
              <h3>ArtHook 快捷键与快速绑定</h3>
            </div>
            <span className="mini-chip">get_shortcuts</span>
          </div>
          <p className="mono-line">快捷键录入</p>
          <div className="shortcut-grid">
            {shortcuts.map((shortcut) => (
              <div className="shortcut-row" key={shortcut.id}>
                <label className="toggle-row">
                  <input
                    type="checkbox"
                    checked={shortcut.enabled}
                    onChange={(event) => updateShortcutDraft(shortcut.id, { enabled: event.target.checked })}
                  />
                  <span>{shortcut.label}</span>
                </label>
                <input
                  className="studio-input"
                  value={shortcut.keys}
                  onChange={(event) => updateShortcutDraft(shortcut.id, { keys: event.target.value })}
                />
                <button
                  className="ghost-button"
                  type="button"
                  onClick={() => saveShortcutDraft(shortcut)}
                  disabled={shortcutBusyId === shortcut.id}
                >
                  {shortcutBusyId === shortcut.id ? "保存中" : "保存快捷键"}
                </button>
              </div>
            ))}
          </div>
          <div className="section-heading-row section-heading-row--compact">
            <h4>快速绑定</h4>
            <button
              className="ghost-button"
              type="button"
              onClick={() => setDraft((current) => ({
                ...current,
                quick_bindings: [
                  ...current.quick_bindings,
                  { id: `${Date.now()}`, art: "", key: "" },
                ],
              }))}
            >
              添加绑定
            </button>
          </div>
          <div className="quick-binding-grid">
            {draft.quick_bindings.map((binding) => (
              <div className="shortcut-row" key={binding.id}>
                <input
                  className="studio-input"
                  value={binding.art}
                  onChange={(event) => setDraft((current) => ({
                    ...current,
                    quick_bindings: current.quick_bindings.map((item) => (
                      item.id === binding.id ? { ...item, art: event.target.value } : item
                    )),
                  }))}
                  placeholder="选择 Art..."
                />
                <input
                  className="studio-input"
                  value={binding.key}
                  onChange={(event) => setDraft((current) => ({
                    ...current,
                    quick_bindings: current.quick_bindings.map((item) => (
                      item.id === binding.id ? { ...item, key: event.target.value } : item
                    )),
                  }))}
                  placeholder="Ctrl+Shift+1"
                />
                <button
                  className="ghost-button"
                  type="button"
                  onClick={() => setDraft((current) => ({
                    ...current,
                    quick_bindings: current.quick_bindings.filter((item) => item.id !== binding.id),
                  }))}
                >
                  删除
                </button>
              </div>
            ))}
          </div>
        </article>

        <article className="glass-card settings-card">
          <div className="control-card__head">
            <div>
              <p className="card-kicker">系统设置</p>
              <h3>系统与数据</h3>
            </div>
            <span className="mini-chip">安全预览</span>
          </div>
          <label className="toggle-row">
            <input type="checkbox" checked={draft.general.auto_start} onChange={toggleAutostart} />
            <span>开机自启</span>
          </label>
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={draft.system.auto_check_updates}
              onChange={(event) => setDraft((current) => ({
                ...current,
                system: { ...current.system, auto_check_updates: event.target.checked },
              }))}
            />
            <span>自动检查更新</span>
          </label>
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={draft.system.enable_run_log}
              onChange={(event) => setDraft((current) => ({
                ...current,
                system: { ...current.system, enable_run_log: event.target.checked },
              }))}
            />
            <span>启用运行日志</span>
          </label>
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={draft.system.record_screenshot_history}
              onChange={(event) => setDraft((current) => ({
                ...current,
                system: { ...current.system, record_screenshot_history: event.target.checked },
              }))}
            />
            <span>记录截图历史</span>
          </label>
          <label className="field-label">
            历史保留
            <select
              className="studio-input"
              value={draft.system.history_retention}
              onChange={(event) => setDraft((current) => ({
                ...current,
                system: { ...current.system, history_retention: event.target.value },
              }))}
            >
              <option value="1d">1 天</option>
              <option value="3d">3 天</option>
              <option value="7d">1 周</option>
              <option value="30d">1 个月</option>
              <option value="forever">永久</option>
            </select>
          </label>
          <div className="terminal-list">
            <span>数据目录：{appPaths?.dataDir || "未加载"}</span>
            <span>配置目录：{appPaths?.configDir || "未加载"}</span>
            <span>日志目录：{appPaths?.logDir || "未加载"}</span>
          </div>
        </article>
      </div>
    </section>
  );
}

function AboutPanel() {
  return (
    <section className="main-board">
      <p className="section-kicker">关于</p>
      <h2>Loom 桌面外壳</h2>
      <div className="terminal-list">
        <span>外壳：Tauri + React + Rsbuild</span>
        <span>运行时：Loom 本地服务 HTTP APIs</span>
        <span>界面：Loom modern-gradient terminal workbench</span>
      </div>
    </section>
  );
}

export default function App() {
  const snapshotRequestGate = useRef(createLatestRequestGate());
  const hookCanvasRequestGate = useRef(createLatestRequestGate());
  const [activeSection, setActiveSection] = useState<SectionId>("overview");
  const [snapshot, setSnapshot] = useState<LoomSnapshot>(fallbackSnapshot);
  const [loading, setLoading] = useState(false);
  const [localServiceBusy, setLocalServiceBusy] = useState(false);
  const [localServiceMessage, setLocalServiceMessage] = useState<StudioMessage | null>(null);
  const [autoStartAttempted, setAutoStartAttempted] = useState(false);
  const [workflowOpenRequest, setWorkflowOpenRequest] = useState<WorkflowOpenRequest | null>(null);
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
        setSnapshot(next);
      }
      return next;
    } finally {
      abortSignal?.removeEventListener("abort", abortRequest);
      if (snapshotRequestGate.current.isCurrent(requestToken)) {
        setLoading(false);
      }
    }
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    await refreshSnapshot();
  }, [refreshSnapshot]);

  const refreshHookCanvas = useCallback(async (baseUrl = snapshot.baseUrl) => {
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
  }, [snapshot.baseUrl]);

  const startLocalService = async (silent = false) => {
    setLocalServiceBusy(true);
    if (!silent) setLocalServiceMessage(null);
    try {
      const result = await startLoomDaemon();
      if (!silent || result.started) {
        setLocalServiceMessage({ kind: "info", text: result.message || "已启动 Loom 本地服务。" });
      }
      const nextSnapshot = await waitForLoomOnline(refreshSnapshot);
      if (!silent && nextSnapshot?.connectionState !== "online") {
        setLocalServiceMessage({
          kind: "error",
          text: nextSnapshot?.error || "Loom 本地服务启动后仍未就绪，请稍后重试。",
        });
      }
    } catch (error) {
      if (!silent) {
        setLocalServiceMessage({
          kind: "error",
          text: error instanceof Error ? error.message : "无法启动 Loom 本地服务。",
        });
      }
    } finally {
      setLocalServiceBusy(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    if (hookCanvasRefreshTrigger === null) {
      hookCanvasRequestGate.current.invalidate();
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
    void startLocalService(true);
  }, [autoStartAttempted, loading, snapshot.connectionState, snapshot.checkedAt]);

  // Ensure the Hook bridge WS server (port 19820) is running once the daemon is
  // online, so peer apps (Hook) can execute art/workflow nodes through it.
  // Idempotent: the daemon returns 409 if already running, which we ignore.
  useEffect(() => {
    if (snapshot.connectionState !== "online") {
      return;
    }
    void startHookBridge(snapshot.baseUrl).catch(() => {
      // Already running or transient failure — the workflow-sync client below
      // will retry connecting regardless.
    });
  }, [snapshot.connectionState, snapshot.baseUrl]);

  useEffect(() => {
    if (typeof window === "undefined" || typeof WebSocket === "undefined") {
      return;
    }
    const sync = startHookBridgeWorkflowSync({
      refresh,
      websocketUrl: hookBridgeUrl,
      invalidateHookCanvas: () => {
        setHookCanvasRefreshVersion((version) => version + 1);
      },
      openHookWorkflow: () => {
        openHookLiveWorkflow(setWorkflowOpenRequest, setActiveSection);
      },
    });

    return () => {
      sync.dispose();
    };
  }, [hookBridgeUrl, refresh]);

  const handleWorkflowOpenRequestHandled = useCallback(() => {
    setWorkflowOpenRequest(null);
  }, []);

  const activeNavigation = useMemo(
    () => navigationItems.find((item) => item.id === activeSection) ?? navigationItems[0],
    [activeSection],
  );

  return (
    <main className="desktop-shell">
      <aside className="left-rail">
        <div className="brand-block">
          <span className="brand-orb">LO</span>
          <div>
            <strong>Loom</strong>
            <small>本地优先工作台</small>
          </div>
        </div>

        <nav className="rail-nav" aria-label="Loom sections">
          {navigationItems.map((item) => (
            <button
              className={activeSection === item.id ? "rail-item rail-item--active" : "rail-item"}
              type="button"
              key={item.id}
              data-testid={item.id === "hook-bridge" ? "nav-hook-bridge" : undefined}
              onClick={() => setActiveSection(item.id)}
            >
              <span>{item.label}</span>
              {item.eyebrow ? <small>{item.eyebrow}</small> : null}
            </button>
          ))}
        </nav>

        <div className="rail-footer">
          <StatusPill snapshot={snapshot} />
          <button className="rail-refresh" type="button" onClick={refresh} disabled={loading}>
            {loading ? "刷新中" : "刷新"}
          </button>
        </div>
      </aside>

      <section className="workspace-panel">
        <header className="workspace-header">
          <div>
            {activeNavigation.eyebrow ? (
              <p className="section-kicker">{activeNavigation.eyebrow}</p>
            ) : null}
            <h1>{activeNavigation.label}</h1>
          </div>
        </header>

        <div className="workspace-scroll">
          {activeSection === "overview" && (
            <OverviewPanel
              snapshot={snapshot}
              refresh={refresh}
              startLocalService={() => void startLocalService(false)}
              localServiceBusy={localServiceBusy}
              localServiceMessage={localServiceMessage}
            />
          )}
          {activeSection === "mcp" && (
            <McpPanel
              servers={snapshot.mcpServers}
              baseUrl={snapshot.baseUrl}
              refresh={refresh}
              openWorkflowStudio={() => setActiveSection("workflows")}
              openHookWorkflow={() =>
                openHookLiveWorkflow(setWorkflowOpenRequest, setActiveSection)
              }
            />
          )}
          {activeSection === "registry" && (
            <ArtPanel
              tools={snapshot.tools}
              pythonArts={snapshot.pythonArts}
              mcpServers={snapshot.mcpServers}
              workflows={snapshot.workflows}
              baseUrl={snapshot.baseUrl}
              refresh={refresh}
            />
          )}
          {activeSection === "hook-bridge" && (
            <HookBridgePanel
              baseUrl={snapshot.baseUrl}
              hookCanvas={hookCanvas}
              hookCanvasError={hookCanvasError}
              tools={snapshot.tools}
              onOpenWorkflow={(selectedNodeId) =>
                openHookLiveWorkflow(setWorkflowOpenRequest, setActiveSection, selectedNodeId)}
            />
          )}
          {activeSection === "workflows" && (
            <WorkflowStudioPanel
              snapshot={snapshot}
              refresh={refresh}
              hookCanvas={hookCanvas}
              refreshHookCanvas={refreshHookCanvas}
              workflowOpenRequest={workflowOpenRequest}
              onWorkflowOpenRequestHandled={handleWorkflowOpenRequestHandled}
            />
          )}
          {activeSection === "agents" && <AgentsPanel capabilities={snapshot.capabilities} />}
          {activeSection === "runs" && <RunsPanel />}
          {activeSection === "settings" && <SettingsPanel snapshot={snapshot} />}
          {activeSection === "about" && <AboutPanel />}
        </div>
      </section>
    </main>
  );
}
