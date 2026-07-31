import { getLoomDaemonJson, type ConnectionState, type LoomToolDefinition } from "./loomApi.ts";
import { serializeWorkflowGraphLite, type WorkflowStudioNode } from "./workflowStudio.ts";

export type HookCanvasNodeKind = "screenshot" | "art" | "unknown";
export type HookCanvasNodeStatus = "ready" | "processing" | "error" | "unknown";

export interface HookCanvasBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HookCanvasCrop {
  // Ratios relative to the node box (window), so the frontend can reproduce
  // Hook's minified crop with no dependency on rendered pixel size or zoom:
  // the image is rendered at `imageWidthRatio × 100%` of the box and panned by
  // `-offsetXRatio × 100%`. See the daemon's HookCanvasCrop for the derivation.
  imageWidthRatio: number;
  imageHeightRatio: number;
  offsetXRatio: number;
  offsetYRatio: number;
}

export interface HookCanvasNode {
  id: string;
  // Connected-component identity in Hook world coordinates. Equal componentIds
  // mean the nodes belong to the same pipeline regardless of current viewport.
  componentId?: string | null;
  // Stable YAML-safe workflow node id emitted by the daemon.
  workflowNodeId?: string | null;
  // Direct upstream workflow node ids emitted by the daemon, already expressed
  // in workflowNodeId space and independent of viewport state.
  upstreamWorkflowNodeIds?: string[] | null;
  kind: HookCanvasNodeKind;
  label: string;
  artId: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  previewAvailable: boolean;
  previewUrl: string | null;
  status: HookCanvasNodeStatus;
  errorMessage?: string | null;
  minified: boolean;
  crop: HookCanvasCrop | null;
  opacity: number;
  // Raw node params (unit.params) passed through by the daemon, so the UI can
  // show an Art node's current parameter values when exposing them as inputs.
  params?: Record<string, unknown> | null;
  resultCandidates?: HookCanvasResultCandidate[] | null;
  selectedResultIndex?: number | null;
}

export interface HookCanvasResultCandidate {
  index: number;
  title?: string | null;
  imageUrl: string;
  thumbnailUrl?: string | null;
  sourcePageUrl?: string | null;
  width?: number | null;
  height?: number | null;
}

export interface HookCanvasEdge {
  id: string;
  sourceNodeId: string;
  sourcePortId: string | null;
  // World-space output anchor already computed by the daemon; the desktop only
  // applies viewport/minimap projection to it.
  sourcePoint?: HookCanvasPoint | null;
  targetNodeId: string;
  targetPortId: string | null;
  // World-space input anchor already computed by the daemon; the desktop only
  // applies viewport/minimap projection to it.
  targetPoint?: HookCanvasPoint | null;
}

export interface HookCanvasSnapshot {
  available: boolean;
  revision: string;
  updatedAt: string | null;
  workflowId: string | null;
  bounds: HookCanvasBounds;
  nodes: HookCanvasNode[];
  edges: HookCanvasEdge[];
  warnings: string[];
}

export interface HookCanvasLayoutOptions {
  width: number;
  height: number;
  padding: number;
  minimumNodeSize: number;
}

export interface HookCanvasLayoutNode extends HookCanvasNode {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HookCanvasLayout {
  width: number;
  height: number;
  scale: number;
  worldOriginX: number;
  worldOriginY: number;
  screenOriginX: number;
  screenOriginY: number;
  nodes: HookCanvasLayoutNode[];
}

export interface HookCanvasPoint {
  x: number;
  y: number;
}

export interface HookCanvasEdgeEndpoints {
  source: HookCanvasPoint;
  target: HookCanvasPoint;
}

export interface HookCanvasNodePreviewRuntimeState {
  hasResolvedPreview: boolean;
  previewFailed: boolean;
}

export interface HookCanvasNodePresentation {
  showPreviewImage: boolean;
  placeholderText: string | null;
  detailText: string | null;
  placeholderTone: "neutral" | "error";
}

export interface HookCanvasRefreshTriggerInput {
  connectionState: ConnectionState;
  baseUrl: string;
  refreshVersion: number;
}

export function getHookCanvasRefreshTrigger(
  input: HookCanvasRefreshTriggerInput,
): string | null {
  if (input.connectionState !== "online") {
    return null;
  }
  return JSON.stringify([input.baseUrl, input.refreshVersion]);
}

export async function readHookCanvasSnapshot(baseUrl: string): Promise<HookCanvasSnapshot> {
  return await getLoomDaemonJson<HookCanvasSnapshot>(baseUrl, "/v1/hook-bridge/canvas");
}

// A saved (frozen) canvas workflow: a full HookCanvasSnapshot with image copies,
// listed alongside the live "桌面同步" session in the canvas workflow selector.
export interface CanvasWorkflowSummary {
  id: string;
  name: string;
  nodeCount: number;
  savedAt: number;
}

interface CanvasWorkflowListResponse {
  workflows?: CanvasWorkflowSummary[];
}

export async function listCanvasWorkflows(baseUrl: string): Promise<CanvasWorkflowSummary[]> {
  const response = await getLoomDaemonJson<CanvasWorkflowListResponse>(
    baseUrl,
    "/v1/hook-bridge/canvas/workflows",
  );
  return Array.isArray(response.workflows) ? response.workflows : [];
}

export async function readCanvasWorkflowSnapshot(
  baseUrl: string,
  workflowId: string,
): Promise<HookCanvasSnapshot> {
  return await getLoomDaemonJson<HookCanvasSnapshot>(
    baseUrl,
    `/v1/hook-bridge/canvas/workflows/${encodeURIComponent(workflowId)}`,
  );
}

// A pipeline-level port of a canvas workflow: an image that must be supplied to
// the workflow (input) or that the workflow produces (output). Derived purely
// from the snapshot topology — source nodes (no upstream edge) are inputs, sink
// nodes (no downstream edge) are outputs. Sticker port handles are fixed by
// convention: input "image", output "output_image".
export interface CanvasWorkflowPort {
  nodeId: string;
  portId: string;
  label: string;
}

export interface CanvasWorkflowInterface {
  inputs: CanvasWorkflowPort[];
  outputs: CanvasWorkflowPort[];
}

export function inferCanvasWorkflowInterface(
  snapshot: HookCanvasSnapshot,
): CanvasWorkflowInterface {
  const incoming = new Set(snapshot.edges.map((edge) => edge.targetNodeId));
  const outgoing = new Set(snapshot.edges.map((edge) => edge.sourceNodeId));
  const inputs: CanvasWorkflowPort[] = [];
  const outputs: CanvasWorkflowPort[] = [];
  for (const node of snapshot.nodes) {
    if (!incoming.has(node.id)) {
      inputs.push({ nodeId: node.id, portId: "image", label: node.label || "输入图像" });
    }
    if (!outgoing.has(node.id)) {
      outputs.push({ nodeId: node.id, portId: "output_image", label: node.label || "输出图像" });
    }
  }
  return { inputs, outputs };
}

// One Art-node parameter that can be exposed as a workflow input. `key` is
// `${workflowNodeId}::${target}`; `uiType`/`executionType` describe the tool port.
export interface ExposableParam {
  workflowNodeId: string;
  target: string;
  label: string;
  uiType: string;
  executionType: string;
}

export interface WorkflowArtBundle {
  yaml: string;
  tool: LoomToolDefinition;
}

// Sanitize a workflow-input parameter name to a YAML/identifier-safe slug.
function safeParamName(raw: string): string {
  const cleaned = raw.replace(/[^a-zA-Z0-9_]+/g, "_").replace(/^_+|_+$/g, "");
  return cleaned || "input";
}

// Map a Loom param ui-type to Hook's ArtParameter `widget` value so the wrapped
// tool's params deserialize into Hook's Art node list.
function widgetForUiType(uiType: string): string {
  if (uiType === "image") return "image_link";
  if (uiType === "int" || uiType === "float" || uiType === "number") return "number";
  if (uiType === "boolean") return "checkbox";
  return "text";
}

function nodeWorkflowId(snapshot: HookCanvasSnapshot, rawNodeId: string): string {
  const node = snapshot.nodes.find((n) => n.id === rawNodeId);
  return node?.workflowNodeId || rawNodeId;
}

// Build the YAML + tool definition for wrapping a saved canvas workflow into a
// reusable "type: workflow" tool. Exposed params become tool input ports (kind
// "param"); unexposed params are baked into each node's YAML `with` as constants.
// Image inputs come from the topology. All binding/YAML node ids use the daemon's
// workflowNodeId so the runtime's `apply_input_bindings` matches by node id.
export function buildWorkflowArtBundle(options: {
  snapshot: HookCanvasSnapshot;
  workflowId: string;
  workflowName: string;
  params: ExposableParam[];
  exposed: Set<string>;
  values: Record<string, string>;
}): WorkflowArtBundle {
  const { snapshot, workflowId, workflowName, params, exposed, values } = options;

  // Group params by workflow node id for `with` construction.
  const paramsByNode = new Map<string, ExposableParam[]>();
  for (const param of params) {
    const list = paramsByNode.get(param.workflowNodeId) ?? [];
    list.push(param);
    paramsByNode.set(param.workflowNodeId, list);
  }

  // Member node ids (workflow-node-id space) so `needs` never references a node
  // absent from this bundle, mirroring buildSubWorkflowYaml's filtering.
  const memberIds = new Set(snapshot.nodes.map((node) => node.workflowNodeId || node.id));

  const nodes: WorkflowStudioNode[] = snapshot.nodes.map((node) => {
    const wid = node.workflowNodeId || node.id;
    const withMap: Record<string, string> = {};
    for (const param of paramsByNode.get(wid) ?? []) {
      const key = `${wid}::${param.target}`;
      // Bake only unexposed params that have a real value; an empty value is
      // left out so the tool's own default applies instead of forcing "".
      if (!exposed.has(key)) {
        const value = values[key] ?? "";
        if (value !== "") {
          withMap[param.target] = value;
        }
      }
    }
    return {
      id: wid,
      uses: node.artId || node.kind,
      needs: (node.upstreamWorkflowNodeIds ?? []).filter((id) => memberIds.has(id)),
      with: withMap,
    };
  });
  const yaml = serializeWorkflowGraphLite({ name: workflowName, description: "", nodes });

  const iface = inferCanvasWorkflowInterface(snapshot);
  const usedNames = new Set<string>();
  const reserve = (preferred: string) => {
    let candidate = safeParamName(preferred);
    let suffix = 2;
    while (usedNames.has(candidate)) {
      candidate = `${safeParamName(preferred)}_${suffix}`;
      suffix += 1;
    }
    usedNames.add(candidate);
    return candidate;
  };

  const bindingInputs: Array<{
    workflowParam: string;
    nodeId: string;
    target: string;
    kind: "input_image" | "param";
  }> = [];
  // Image inputs go into the tool's `inputs`; exposed params go into `params`.
  // Loom's artloom_compat_art_json maps tool.params → art `params` (Hook's
  // ArtParameter list) and tool.inputs → art `inputs`, so keeping them separate
  // is what makes the exposed params show up as Hook node parameters.
  const toolInputs: Array<{
    id: string;
    name: string;
    label: string;
    widget: string;
    type: string;
    executionType: string;
    default: string;
  }> = [];
  const toolParams: Array<{
    id: string;
    name: string;
    label: string;
    widget: string;
    type: string;
    executionType: string;
    default: string;
  }> = [];

  iface.inputs.forEach((port, index) => {
    const wid = nodeWorkflowId(snapshot, port.nodeId);
    const workflowParam = reserve(index === 0 ? "input" : `input_${index + 1}`);
    bindingInputs.push({ workflowParam, nodeId: wid, target: port.portId, kind: "input_image" });
    toolInputs.push({
      id: workflowParam,
      name: workflowParam,
      label: "输入图像",
      widget: "image_link",
      type: "image",
      executionType: "image_buffer",
      default: "",
    });
  });

  for (const param of params) {
    const key = `${param.workflowNodeId}::${param.target}`;
    if (!exposed.has(key)) continue;
    const workflowParam = reserve(`${param.workflowNodeId}_${param.target}`);
    bindingInputs.push({
      workflowParam,
      nodeId: param.workflowNodeId,
      target: param.target,
      kind: "param",
    });
    toolParams.push({
      id: workflowParam,
      name: workflowParam,
      label: param.label,
      widget: widgetForUiType(param.uiType),
      type: param.uiType,
      executionType: param.executionType,
      default: values[key] ?? "",
    });
  }

  const outPort = iface.outputs[0];
  const primaryOutput = outPort
    ? {
        nodeId: nodeWorkflowId(snapshot, outPort.nodeId),
        output: outPort.portId,
        kind: "node_result" as const,
      }
    : undefined;

  const tool: LoomToolDefinition = {
    id: `hook-wf-${workflowId}`,
    name: `${workflowName} 工具`,
    description: "由 Hook 截图工作流封装的 Loom 工具。",
    enabled: true,
    execution: {
      type: "workflow",
      workflowId,
      workflowBindings: { inputs: bindingInputs, primaryOutput },
    },
    inputs: toolInputs,
    params: toolParams,
    outputs: [
      { name: "result", label: "输出图像", type: "image", executionType: "image_buffer" },
    ],
    // Mark as ArtLoom-compat managed so it surfaces in Hook's Art node list
    // (GET /v1/artloom-compat/arts filters on this metadata).
    metadata: {
      artloomCompat: { source: "artloom-compat", managedBy: "hook-workflow" },
    },
  };
  return { yaml, tool };
}

export function keepNewestHookCanvasSnapshot(
  previous: HookCanvasSnapshot | null,
  next: HookCanvasSnapshot,
): HookCanvasSnapshot {
  if (previous?.available && !next.available) {
    return previous;
  }
  return previous?.revision === next.revision ? previous : next;
}

export function resolveHookCanvasPreviewUrl(
  baseUrl: string,
  node: HookCanvasNode,
): string | null {
  if (!node.previewAvailable || !node.previewUrl) {
    return null;
  }
  const normalizedBaseUrl = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  return new URL(node.previewUrl, normalizedBaseUrl).toString();
}

// The daemon-relative preview path (including its `?v=` cache token) for a node,
// or null when the node has no available preview. The desktop loads this through
// the native Tauri command so the WebView does not fetch the daemon directly.
export function hookCanvasPreviewPath(node: HookCanvasNode): string | null {
  if (!node.previewAvailable || !node.previewUrl) {
    return null;
  }
  return node.previewUrl;
}

export function getHookCanvasNodePresentation(
  node: HookCanvasNode,
  runtime: HookCanvasNodePreviewRuntimeState,
): HookCanvasNodePresentation {
  if (node.kind === "art" && node.status === "error") {
    return {
      showPreviewImage: false,
      placeholderText: "执行失败",
      detailText: node.errorMessage?.trim() || null,
      placeholderTone: "error",
    };
  }
  if (!runtime.hasResolvedPreview || runtime.previewFailed) {
    return {
      showPreviewImage: false,
      placeholderText: "预览不可用",
      detailText: null,
      placeholderTone: "neutral",
    };
  }
  return {
    showPreviewImage: true,
    placeholderText: null,
    detailText: null,
    placeholderTone: "neutral",
  };
}

// All nodes reachable from `nodeId` following edges in either direction (the
// connected component). Used to edge-highlight a whole pipeline when a node is
// clicked and to scope "save as workflow" to that pipeline.
export function connectedNodeIds(
  snapshot: HookCanvasSnapshot,
  nodeId: string | null,
): Set<string> {
  const result = new Set<string>();
  if (!nodeId) {
    return result;
  }
  const selectedNode = snapshot.nodes.find((node) => node.id === nodeId);
  if (!selectedNode) {
    return result;
  }
  if (selectedNode.componentId) {
    return new Set(
      snapshot.nodes
        .filter((node) => node.componentId === selectedNode.componentId)
        .map((node) => node.id),
    );
  }
  const adjacency = new Map<string, string[]>();
  const link = (from: string, to: string) => {
    const list = adjacency.get(from) ?? [];
    list.push(to);
    adjacency.set(from, list);
  };
  for (const edge of snapshot.edges) {
    link(edge.sourceNodeId, edge.targetNodeId);
    link(edge.targetNodeId, edge.sourceNodeId);
  }
  const queue = [nodeId];
  result.add(nodeId);
  while (queue.length) {
    const current = queue.shift() as string;
    for (const neighbor of adjacency.get(current) ?? []) {
      if (!result.has(neighbor)) {
        result.add(neighbor);
        queue.push(neighbor);
      }
    }
  }
  return result;
}

export function edgeWorldEndpoints(
  snapshot: HookCanvasSnapshot,
  edge: HookCanvasEdge,
): HookCanvasEdgeEndpoints | null {
  if (edge.sourcePoint && edge.targetPoint) {
    return {
      source: edge.sourcePoint,
      target: edge.targetPoint,
    };
  }

  const source = snapshot.nodes.find((node) => node.id === edge.sourceNodeId);
  const target = snapshot.nodes.find((node) => node.id === edge.targetNodeId);
  if (!source || !target) {
    return null;
  }
  return {
    source: {
      x: source.x + source.width + (source.minified ? 4 : 6),
      y: source.y + source.height / 2,
    },
    target: {
      x: target.x - (target.minified ? 4 : 6),
      y: target.y + target.height / 2,
    },
  };
}

// An edge is highlighted when both of its endpoints are in the connected set.
export function isEdgeHighlighted(
  edge: HookCanvasEdge,
  highlighted: Set<string>,
): boolean {
  return highlighted.has(edge.sourceNodeId) && highlighted.has(edge.targetNodeId);
}

// A stable, YAML-safe identifier for a workflow node derived from a canvas node.
function workflowNodeId(node: HookCanvasNode): string {
  const base = (node.artId || node.id || "node").toString();
  const safe = base.replace(/[^A-Za-z0-9_-]/g, "-").replace(/-+/g, "-").replace(/^-|-$/g, "");
  return safe || "node";
}

// Serialize a connected component (a whole pipeline) into a standalone workflow
// YAML the user can save and reuse. `needs` edges are kept only between nodes
// that are part of the selected component, so a multi-input/multi-output branch
// saves as a self-contained graph.
export function buildSubWorkflowYaml(
  snapshot: HookCanvasSnapshot,
  nodeIds: Set<string>,
  workflowName: string,
): string {
  const members = snapshot.nodes.filter((node) => nodeIds.has(node.id));
  const safeName = workflowName.trim() || "hook-pipeline";
  const lines: string[] = [`name: ${yamlSingleQuoted(safeName)}`, "nodes:"];
  if (!members.length) {
    lines.push("  []");
    return `${lines.join("\n")}\n`;
  }

  const canUseDaemonMetadata = members.every(
    (node) => typeof node.workflowNodeId === "string"
      && node.workflowNodeId.length > 0
      && Array.isArray(node.upstreamWorkflowNodeIds),
  );

  if (canUseDaemonMetadata) {
    const selectedWorkflowIds = new Set(
      members
        .map((node) => node.workflowNodeId as string),
    );
    for (const node of members) {
      const wid = node.workflowNodeId as string;
      lines.push(`  - id: ${wid}`);
      lines.push(`    uses: ${yamlSingleQuoted(node.artId || node.kind)}`);
      const needs = (node.upstreamWorkflowNodeIds ?? []).filter((upstreamId) =>
        selectedWorkflowIds.has(upstreamId),
      );
      if (needs.length) {
        lines.push(`    needs: [${needs.join(", ")}]`);
      }
    }
    return `${lines.join("\n")}\n`;
  }

  const idMap = new Map<string, string>();
  const usedIds = new Set<string>();
  for (const node of members) {
    let candidate = workflowNodeId(node);
    let suffix = 2;
    while (usedIds.has(candidate)) {
      candidate = `${workflowNodeId(node)}-${suffix}`;
      suffix += 1;
    }
    usedIds.add(candidate);
    idMap.set(node.id, candidate);
  }

  // Upstream dependencies: for each edge inside the component, the target needs
  // the source.
  const needsByNode = new Map<string, string[]>();
  for (const edge of snapshot.edges) {
    if (!nodeIds.has(edge.sourceNodeId) || !nodeIds.has(edge.targetNodeId)) continue;
    const target = idMap.get(edge.targetNodeId);
    const source = idMap.get(edge.sourceNodeId);
    if (!target || !source || target === source) continue;
    const list = needsByNode.get(target) ?? [];
    if (!list.includes(source)) list.push(source);
    needsByNode.set(target, list);
  }

  for (const node of members) {
    const wid = idMap.get(node.id) as string;
    lines.push(`  - id: ${wid}`);
    lines.push(`    uses: ${yamlSingleQuoted(node.artId || node.kind)}`);
    const needs = needsByNode.get(wid);
    if (needs && needs.length) {
      lines.push(`    needs: [${needs.join(", ")}]`);
    }
  }
  return `${lines.join("\n")}\n`;
}

export function retainHookCanvasSelection(
  selectedNodeId: string | null,
  snapshot: HookCanvasSnapshot,
): string | null {
  return selectedNodeId && snapshot.nodes.some((node) => node.id === selectedNodeId)
    ? selectedNodeId
    : null;
}

export function fitHookCanvas(
  snapshot: HookCanvasSnapshot,
  options: HookCanvasLayoutOptions,
): HookCanvasLayout {
  const width = positiveFinite(options.width, 1);
  const height = positiveFinite(options.height, 1);
  const padding = nonNegativeFinite(options.padding, 0);
  const minimumNodeSize = positiveFinite(options.minimumNodeSize, 1);
  const usableWidth = Math.max(1, width - padding * 2);
  const usableHeight = Math.max(1, height - padding * 2);
  const sourceWidth = positiveFinite(snapshot.bounds.width, 1);
  const sourceHeight = positiveFinite(snapshot.bounds.height, 1);
  const scale = Math.min(usableWidth / sourceWidth, usableHeight / sourceHeight);
  const contentWidth = sourceWidth * scale;
  const contentHeight = sourceHeight * scale;
  const offsetX = (width - contentWidth) / 2;
  const offsetY = (height - contentHeight) / 2;

  return {
    width,
    height,
    scale,
    worldOriginX: finite(snapshot.bounds.x, 0),
    worldOriginY: finite(snapshot.bounds.y, 0),
    screenOriginX: offsetX,
    screenOriginY: offsetY,
    nodes: snapshot.nodes.map((node) => ({
      ...node,
      x: offsetX + (finite(node.x, 0) - finite(snapshot.bounds.x, 0)) * scale,
      y: offsetY + (finite(node.y, 0) - finite(snapshot.bounds.y, 0)) * scale,
      width: Math.max(minimumNodeSize, positiveFinite(node.width, minimumNodeSize) * scale),
      height: Math.max(minimumNodeSize, positiveFinite(node.height, minimumNodeSize) * scale),
    })),
  };
}

export function edgeEndpoints(
  layout: HookCanvasLayout,
  edge: HookCanvasEdge,
): HookCanvasEdgeEndpoints | null {
  if (edge.sourcePoint && edge.targetPoint) {
    return {
      source: projectPointToLayout(layout, edge.sourcePoint),
      target: projectPointToLayout(layout, edge.targetPoint),
    };
  }

  const source = layout.nodes.find((node) => node.id === edge.sourceNodeId);
  const target = layout.nodes.find((node) => node.id === edge.targetNodeId);
  if (!source || !target) {
    return null;
  }
  return {
    source: {
      x: source.x + source.width + (source.minified ? 4 : 6),
      y: source.y + source.height / 2,
    },
    target: {
      x: target.x - (target.minified ? 4 : 6),
      y: target.y + target.height / 2,
    },
  };
}

function finite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}

function positiveFinite(value: number, fallback: number): number {
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function nonNegativeFinite(value: number, fallback: number): number {
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

function projectPointToLayout(
  layout: HookCanvasLayout,
  point: HookCanvasPoint,
): HookCanvasPoint {
  return {
    x: layout.screenOriginX + (finite(point.x, 0) - layout.worldOriginX) * layout.scale,
    y: layout.screenOriginY + (finite(point.y, 0) - layout.worldOriginY) * layout.scale,
  };
}

export interface HookCanvasViewport {
  scale: number;
  offsetX: number;
  offsetY: number;
}

// Layout using an explicit viewport (pan offset + zoom) instead of auto-fit.
// World coordinates are the snapshot's own node coordinates; the surface shows
// a fixed window into that world the user can pan (drag) and zoom (wheel).
export function viewportLayout(
  snapshot: HookCanvasSnapshot,
  viewport: HookCanvasViewport,
): HookCanvasLayout {
  const scale = positiveFinite(viewport.scale, 1);
  return {
    width: 0,
    height: 0,
    scale,
    worldOriginX: finite(viewport.offsetX, 0),
    worldOriginY: finite(viewport.offsetY, 0),
    screenOriginX: 0,
    screenOriginY: 0,
    nodes: snapshot.nodes.map((node) => ({
      ...node,
      x: (finite(node.x, 0) - finite(viewport.offsetX, 0)) * scale,
      y: (finite(node.y, 0) - finite(viewport.offsetY, 0)) * scale,
      width: positiveFinite(node.width, 1) * scale,
      height: positiveFinite(node.height, 1) * scale,
    })),
  };
}

// Compute a viewport that fits the whole snapshot into a surface of the given
// pixel size, used as the initial view before the user pans or zooms.
export function fitViewport(
  snapshot: HookCanvasSnapshot,
  surfaceWidth: number,
  surfaceHeight: number,
  padding = 40,
): HookCanvasViewport {
  const sourceWidth = positiveFinite(snapshot.bounds.width, 1);
  const sourceHeight = positiveFinite(snapshot.bounds.height, 1);
  const usableWidth = Math.max(1, surfaceWidth - padding * 2);
  const usableHeight = Math.max(1, surfaceHeight - padding * 2);
  const scale = Math.min(usableWidth / sourceWidth, usableHeight / sourceHeight);
  const worldCenterX = finite(snapshot.bounds.x, 0) + sourceWidth / 2;
  const worldCenterY = finite(snapshot.bounds.y, 0) + sourceHeight / 2;
  // Center the content: offset so the world center maps to the surface center.
  return {
    scale,
    offsetX: worldCenterX - surfaceWidth / 2 / scale,
    offsetY: worldCenterY - surfaceHeight / 2 / scale,
  };
}

// Map a scale to a 0..1000 slider position and back using a log scale so the
// slider feels uniform across the wide zoom range. The slider and the wheel
// share the same underlying zoom model; only the trigger differs.
export function scaleToSliderValue(
  scale: number,
  minScale: number,
  maxScale: number,
  steps = 1000,
): number {
  const clamped = Math.min(maxScale, Math.max(minScale, scale));
  const ratio = Math.log(clamped / minScale) / Math.log(maxScale / minScale);
  return Math.round(ratio * steps);
}

export function sliderValueToScale(
  value: number,
  minScale: number,
  maxScale: number,
  steps = 1000,
): number {
  const ratio = Math.min(1, Math.max(0, value / steps));
  return minScale * Math.pow(maxScale / minScale, ratio);
}

function yamlSingleQuoted(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}
