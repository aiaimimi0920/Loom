import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  buildSubWorkflowYaml,
  buildHookWorkflowInstantiationGraph,
  buildWorkflowArtBundle,
  connectedNodeIds,
  edgeEndpoints,
  edgeWorldEndpoints,
  fitViewport,
  inferCanvasWorkflowInterface,
  isEdgeHighlighted,
  listCanvasWorkflows,
  readCanvasWorkflowSnapshot,
  scaleToSliderValue,
  sliderValueToScale,
  viewportLayout,
  type CanvasWorkflowSummary,
  type ExposableParam,
  type HookCanvasNode as HookCanvasNodeData,
  type HookCanvasSnapshot,
  type HookCanvasViewport,
} from "../../services/hookCanvas.ts";
import {
  deleteCanvasWorkflow,
  instantiateArtLoomWorkflow,
  renameCanvasWorkflow,
  saveHookCanvasWorkflow,
  saveWorkflowBundle,
  type LoomToolDefinition,
} from "../../services/loomApi.ts";
import {
  mapParamExecutionType,
  mapParamUiType,
  normalizeToolParams,
  toolDefinitionsByIdentity,
  type ToolParamDefinition,
} from "../../services/workflowStudio.ts";
import { HookCanvasNode } from "./HookCanvasNode.tsx";

// The dropdown's fixed default value: the whole live desktop-sync session.
const LIVE_WORKFLOW_ID = "__live__";

// One exposable parameter of an Art node, resolved from the node's tool schema.
interface ParamRow {
  key: string; // `${workflowNodeId}::${target}`
  target: string;
  label: string;
  currentValue: string;
  param: ToolParamDefinition;
}
interface NodeParamGroup {
  nodeId: string;
  workflowNodeId: string;
  label: string;
  rows: ParamRow[];
}

// Resolve a parameter's display value: the node's live Hook value if present,
// otherwise the tool schema default.
function paramValueFromNode(
  node: HookCanvasNodeData,
  target: string,
  param: ToolParamDefinition,
): string {
  const live = node.params?.[target];
  const value = live !== undefined && live !== null ? live : param.default;
  if (value === undefined || value === null) return "";
  return typeof value === "string" ? value : String(value);
}

const NODE_KIND_LABELS: Record<string, string> = {
  screenshot: "截图节点",
  art: "Art 节点",
  unknown: "未知节点",
};

const SURFACE_WIDTH = 1000;
const SURFACE_HEIGHT = 620;
const MIN_SCALE = 0.05;
const MAX_SCALE = 6;
const ZOOM_STEP = 1.12;
const MINIMAP_WIDTH = 200;
const MINIMAP_HEIGHT = 130;
const MINIMAP_PADDING = 8;
// The minimap maps a world window larger than the node bounds, so the current
// viewport rectangle sits inside it with margin at fit-all and visibly shrinks
// when zooming in / grows when zooming out (instead of always filling the map).
const MINIMAP_WORLD_EXPANSION = 1.8;
// Pointer movement (in surface px) beyond which a press counts as a pan/drag
// rather than a click, so node selection is suppressed after a drag.
const DRAG_THRESHOLD = 3;

interface HookCanvasThumbnailProps {
  snapshot: HookCanvasSnapshot | null;
  baseUrl: string;
  error: string | null;
  tools?: LoomToolDefinition[];
  onCreateWorkflowArt?: (request: WorkflowArtCreationRequest) => void;
}

export interface WorkflowArtCreationRequest {
  workflowId: string;
  workflowName: string;
  tool: LoomToolDefinition;
}

export function HookCanvasThumbnail({
  snapshot,
  baseUrl,
  error,
  tools = [],
  onCreateWorkflowArt,
}: HookCanvasThumbnailProps) {
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [viewport, setViewport] = useState<HookCanvasViewport | null>(null);
  const [isPanning, setIsPanning] = useState(false);
  const [showMinimap, setShowMinimap] = useState(true);
  // Workflow selector: `__live__` is the live desktop-sync session; other values
  // are saved (frozen) canvas workflows loaded on demand.
  const [workflows, setWorkflows] = useState<CanvasWorkflowSummary[]>([]);
  const [selectedWorkflow, setSelectedWorkflow] = useState<string>(LIVE_WORKFLOW_ID);
  const [savedSnapshot, setSavedSnapshot] = useState<HookCanvasSnapshot | null>(null);
  // Rename dialog: holds the draft name while the二级 dialog is open, or null.
  const [renameDraft, setRenameDraft] = useState<string | null>(null);
  const [workflowBusy, setWorkflowBusy] = useState(false);
  const [workflowBusyAction, setWorkflowBusyAction] = useState<"desktop" | "art" | null>(null);
  const [artMessage, setArtMessage] = useState<{ ok: boolean; text: string } | null>(null);
  // Parameter exposure: which "<workflowNodeId>::<target>" keys are exposed as
  // workflow inputs, and the current value of every parameter (exposed → its
  // default/runtime value; not exposed → the constant baked into the YAML `with`).
  const [exposedParams, setExposedParams] = useState<Set<string>>(new Set());
  const [paramValues, setParamValues] = useState<Record<string, string>>({});

  const emptySnapshot = useMemo<HookCanvasSnapshot>(() => ({
    available: false,
    revision: "empty",
    updatedAt: null,
    workflowId: null,
    bounds: { x: 0, y: 0, width: 0, height: 0 },
    nodes: [],
    edges: [],
    warnings: [],
  }), []);
  const isLive = selectedWorkflow === LIVE_WORKFLOW_ID;
  // The rendered snapshot is the live session for `__live__`, else the frozen one.
  const sourceSnapshot = isLive ? snapshot : savedSnapshot;
  const activeSnapshot = sourceSnapshot ?? emptySnapshot;
  const hasNodes = Boolean(sourceSnapshot?.nodes.length);

  const refreshWorkflowList = useCallback(() => {
    void listCanvasWorkflows(baseUrl)
      .then(setWorkflows)
      .catch(() => setWorkflows([]));
  }, [baseUrl]);

  useEffect(() => {
    refreshWorkflowList();
  }, [refreshWorkflowList]);

  // Load the frozen snapshot when a saved workflow is selected; clear when live.
  useEffect(() => {
    if (isLive) {
      setSavedSnapshot(null);
      return;
    }
    let cancelled = false;
    void readCanvasWorkflowSnapshot(baseUrl, selectedWorkflow)
      .then((snap) => {
        if (!cancelled) setSavedSnapshot(snap);
      })
      .catch(() => {
        if (!cancelled) setSavedSnapshot(null);
      });
    return () => {
      cancelled = true;
    };
  }, [baseUrl, isLive, selectedWorkflow]);

  // Reset to fit-all whenever the rendered content changes — either the live
  // revision updates or the user switches workflow — then let the user pan/zoom
  // freely from there.
  const revision = sourceSnapshot?.revision ?? "empty";
  useEffect(() => {
    setViewport(fitViewport(activeSnapshot, SURFACE_WIDTH, SURFACE_HEIGHT));
    setSelectedNodeId(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision, selectedWorkflow]);

  const effectiveViewport = viewport ?? fitViewport(activeSnapshot, SURFACE_WIDTH, SURFACE_HEIGHT);
  const layout = useMemo(
    () => viewportLayout(activeSnapshot, effectiveViewport),
    [activeSnapshot, effectiveViewport],
  );
  // In desktop-sync (live) mode, selecting a node highlights its whole connected
  // component — that pipeline is what "保存为工作流" freezes. Inside an existing
  // saved workflow, selection is single-node only (just inspect one node).
  const highlighted = useMemo(
    () =>
      isLive
        ? connectedNodeIds(activeSnapshot, selectedNodeId)
        : new Set<string>(selectedNodeId ? [selectedNodeId] : []),
    [activeSnapshot, selectedNodeId, isLive],
  );
  // The currently selected node, for the属性 panel below the canvas.
  const selectedNode = useMemo(
    () =>
      selectedNodeId
        ? activeSnapshot.nodes.find((node) => node.id === selectedNodeId) ?? null
        : null,
    [activeSnapshot, selectedNodeId],
  );
  // Pipeline-level input/output ports of a saved workflow (topological).
  const workflowInterface = useMemo(
    () => inferCanvasWorkflowInterface(activeSnapshot),
    [activeSnapshot],
  );
  // Per-node exposable parameters, derived from each Art node's registered tool
  // schema (artId → tool). Pure screenshot nodes have no params and are skipped.
  const toolMap = useMemo(
    () => toolDefinitionsByIdentity(tools),
    [tools],
  );
  const paramGroups = useMemo<NodeParamGroup[]>(() => {
    const groups: NodeParamGroup[] = [];
    for (const node of activeSnapshot.nodes) {
      if (!node.artId) continue;
      const tool = toolMap.get(node.artId);
      if (!tool) continue;
      const workflowNodeId = node.workflowNodeId || node.id;
      const rows: ParamRow[] = [];
      for (const param of normalizeToolParams(tool)) {
        const target = param.id || param.name;
        if (!target || param.disabled) continue;
        const key = `${workflowNodeId}::${target}`;
        rows.push({
          key,
          target,
          label: param.label || param.name || target,
          currentValue: paramValueFromNode(node, target, param),
          param,
        });
      }
      if (rows.length) {
        groups.push({ nodeId: node.id, workflowNodeId, label: node.label || workflowNodeId, rows });
      }
    }
    return groups;
  }, [activeSnapshot, toolMap]);

  // Seed paramValues from current values whenever the workflow's param set
  // changes, so unedited fields still carry their Hook value into the constant.
  useEffect(() => {
    setParamValues((previous) => {
      const next: Record<string, string> = {};
      for (const group of paramGroups) {
        for (const row of group.rows) {
          next[row.key] = previous[row.key] ?? row.currentValue;
        }
      }
      return next;
    });
    setExposedParams((previous) => {
      const validKeys = new Set(paramGroups.flatMap((g) => g.rows.map((r) => r.key)));
      const filtered = new Set<string>();
      for (const key of previous) if (validKeys.has(key)) filtered.add(key);
      return filtered;
    });
  }, [paramGroups]);

  // The exposed params, flattened for the interface summary — these plus the
  // topological image inputs make up the wrapped node's full input surface.
  const exposedParamRows = useMemo(
    () =>
      paramGroups.flatMap((group) =>
        group.rows
          .filter((row) => exposedParams.has(row.key))
          .map((row) => ({ key: row.key, label: `${group.label} / ${row.label}` })),
      ),
    [paramGroups, exposedParams],
  );
  const totalInputCount = workflowInterface.inputs.length + exposedParamRows.length;

  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const minimapRef = useRef<HTMLDivElement | null>(null);
  // Latest viewport, kept in a ref so global pointer handlers read fresh values
  // without re-subscribing on every viewport change.
  const viewportRef = useRef(effectiveViewport);
  viewportRef.current = effectiveViewport;

  // A surface drag (pan). `suppressClick` tells the node click handler to ignore
  // the click that fires right after a real drag.
  const surfaceDrag = useRef<
    | {
        startClientX: number;
        startClientY: number;
        origin: HookCanvasViewport;
        rectWidth: number;
        rectHeight: number;
      }
    | null
  >(null);
  const suppressClick = useRef(false);

  // A minimap drag: repositions the main viewport as the pointer moves.
  const minimapDrag = useRef(false);

  const handleSelect = (nodeId: string) => {
    if (suppressClick.current) {
      suppressClick.current = false;
      return;
    }
    setSaveMessage(null);
    setSelectedNodeId((current) => (current === nodeId ? null : nodeId));
  };

  const handleWheel = (event: React.WheelEvent<HTMLDivElement>) => {
    if (!hasNodes) return;
    event.preventDefault();
    const surface = surfaceRef.current;
    if (!surface) return;
    const rect = surface.getBoundingClientRect();
    const pointerX = ((event.clientX - rect.left) / rect.width) * SURFACE_WIDTH;
    const pointerY = ((event.clientY - rect.top) / rect.height) * SURFACE_HEIGHT;
    setViewport((current) => {
      const base = current ?? viewportRef.current;
      const factor = event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
      const nextScale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, base.scale * factor));
      if (nextScale === base.scale) return base;
      const worldX = base.offsetX + pointerX / base.scale;
      const worldY = base.offsetY + pointerY / base.scale;
      return {
        scale: nextScale,
        offsetX: worldX - pointerX / nextScale,
        offsetY: worldY - pointerY / nextScale,
      };
    });
  };

  // Start a pan on any press on the surface (including on top of nodes — when
  // zoomed in, nodes can cover the whole surface, so requiring empty space would
  // make panning impossible). A click only selects a node if the pointer barely
  // moved (see suppressClick).
  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!hasNodes) return;
    const surface = surfaceRef.current;
    if (!surface) return;
    const rect = surface.getBoundingClientRect();
    surfaceDrag.current = {
      startClientX: event.clientX,
      startClientY: event.clientY,
      origin: viewportRef.current,
      rectWidth: rect.width,
      rectHeight: rect.height,
    };
    suppressClick.current = false;
  };

  // World point under a minimap pointer event.
  const minimapWorldPoint = (clientX: number, clientY: number) => {
    const el = minimapRef.current;
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    const mapX = ((clientX - rect.left) / rect.width) * MINIMAP_WIDTH;
    const mapY = ((clientY - rect.top) / rect.height) * MINIMAP_HEIGHT;
    return {
      worldX: minimap.worldX + (mapX - minimap.originX) / minimap.scale,
      worldY: minimap.worldY + (mapY - minimap.originY) / minimap.scale,
    };
  };

  const centerViewportOn = (worldX: number, worldY: number) => {
    setViewport((current) => {
      const base = current ?? viewportRef.current;
      return {
        scale: base.scale,
        offsetX: worldX - SURFACE_WIDTH / 2 / base.scale,
        offsetY: worldY - SURFACE_HEIGHT / 2 / base.scale,
      };
    });
  };

  const handleMinimapPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!hasNodes) return;
    event.stopPropagation();
    minimapDrag.current = true;
    setIsPanning(true);
    const point = minimapWorldPoint(event.clientX, event.clientY);
    if (point) centerViewportOn(point.worldX, point.worldY);
  };

  // Global pointer handlers cover both surface pans and minimap drags. Using
  // window listeners (not pointer capture) keeps the move stream alive across the
  // frequent re-renders that happen while panning.
  useEffect(() => {
    const handleMove = (event: PointerEvent) => {
      if (minimapDrag.current) {
        const point = minimapWorldPoint(event.clientX, event.clientY);
        if (point) centerViewportOn(point.worldX, point.worldY);
        return;
      }
      const drag = surfaceDrag.current;
      if (!drag) return;
      const dxPx = ((event.clientX - drag.startClientX) / drag.rectWidth) * SURFACE_WIDTH;
      const dyPx = ((event.clientY - drag.startClientY) / drag.rectHeight) * SURFACE_HEIGHT;
      if (Math.abs(event.clientX - drag.startClientX) > DRAG_THRESHOLD
        || Math.abs(event.clientY - drag.startClientY) > DRAG_THRESHOLD) {
        suppressClick.current = true;
        setIsPanning(true);
      }
      setViewport({
        scale: drag.origin.scale,
        offsetX: drag.origin.offsetX - dxPx / drag.origin.scale,
        offsetY: drag.origin.offsetY - dyPx / drag.origin.scale,
      });
    };
    const handleUp = () => {
      surfaceDrag.current = null;
      minimapDrag.current = false;
      setIsPanning(false);
    };
    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", handleUp);
    window.addEventListener("pointercancel", handleUp);
    return () => {
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", handleUp);
      window.removeEventListener("pointercancel", handleUp);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasNodes]);

  // Zoom to an explicit scale anchored at the surface center (slider).
  const zoomToScale = (nextScaleRaw: number) => {
    const centerX = SURFACE_WIDTH / 2;
    const centerY = SURFACE_HEIGHT / 2;
    setViewport((current) => {
      const base = current ?? viewportRef.current;
      const nextScale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, nextScaleRaw));
      if (nextScale === base.scale) return base;
      const worldX = base.offsetX + centerX / base.scale;
      const worldY = base.offsetY + centerY / base.scale;
      return {
        scale: nextScale,
        offsetX: worldX - centerX / nextScale,
        offsetY: worldY - centerY / nextScale,
      };
    });
  };

  // Minimap projection: map a fixed world window (node bounds expanded around
  // their center) into the minimap box.
  const minimap = useMemo(() => {
    const nodeBounds = activeSnapshot.bounds;
    const nodeWidth = nodeBounds.width > 0 ? nodeBounds.width : 1;
    const nodeHeight = nodeBounds.height > 0 ? nodeBounds.height : 1;
    const worldWidth = nodeWidth * MINIMAP_WORLD_EXPANSION;
    const worldHeight = nodeHeight * MINIMAP_WORLD_EXPANSION;
    const worldX = nodeBounds.x + nodeWidth / 2 - worldWidth / 2;
    const worldY = nodeBounds.y + nodeHeight / 2 - worldHeight / 2;
    const usableW = MINIMAP_WIDTH - MINIMAP_PADDING * 2;
    const usableH = MINIMAP_HEIGHT - MINIMAP_PADDING * 2;
    const scale = Math.min(usableW / worldWidth, usableH / worldHeight);
    const contentW = worldWidth * scale;
    const contentH = worldHeight * scale;
    const originX = MINIMAP_PADDING + (usableW - contentW) / 2;
    const originY = MINIMAP_PADDING + (usableH - contentH) / 2;
    const toMap = (wx: number, wy: number) => ({
      x: originX + (wx - worldX) * scale,
      y: originY + (wy - worldY) * scale,
    });
    const viewW = SURFACE_WIDTH / effectiveViewport.scale;
    const viewH = SURFACE_HEIGHT / effectiveViewport.scale;
    const viewTopLeft = toMap(effectiveViewport.offsetX, effectiveViewport.offsetY);
    return {
      scale,
      originX,
      originY,
      worldX,
      worldY,
      toMap,
      viewRect: {
        x: viewTopLeft.x,
        y: viewTopLeft.y,
        w: viewW * scale,
        h: viewH * scale,
      },
    };
  }, [activeSnapshot.bounds, effectiveViewport]);

  const saveAsWorkflow = async () => {
    if (!snapshot || !selectedNodeId || !highlighted.size) return;
    const workflowId = `hook-pipeline-${selectedNodeId.slice(0, 8)}`;
    setSaving(true);
    setSaveMessage(null);
    try {
      try {
        await saveHookCanvasWorkflow(baseUrl, {
          workflowId,
          selectedNodeId,
          workflowName: workflowId,
        });
      } catch (saveError) {
        const message = saveError instanceof Error ? saveError.message : String(saveError);
        if (!message.includes("/v1/hook-bridge/canvas/workflows/") || !message.includes("HTTP 404")) {
          throw saveError;
        }
        const yaml = buildSubWorkflowYaml(snapshot, highlighted, workflowId);
        await saveWorkflowBundle(baseUrl, { id: workflowId }, yaml);
      }
      setSaveMessage({ ok: true, text: `已保存工作流 ${workflowId}（${highlighted.size} 个节点）。` });
      // Refresh the selector list and switch to the freshly saved workflow so the
      // user immediately sees its frozen preview.
      refreshWorkflowList();
      setSelectedNodeId(null);
      setSelectedWorkflow(workflowId);
    } catch (saveError) {
      setSaveMessage({
        ok: false,
        text: saveError instanceof Error ? saveError.message : "保存工作流失败。",
      });
    } finally {
      setSaving(false);
    }
  };

  const currentWorkflowName =
    workflows.find((workflow) => workflow.id === selectedWorkflow)?.name ?? selectedWorkflow;

  const deleteSelectedWorkflow = async () => {
    if (isLive) return;
    const target = selectedWorkflow;
    setWorkflowBusy(true);
    try {
      await deleteCanvasWorkflow(baseUrl, target);
      setSelectedWorkflow(LIVE_WORKFLOW_ID);
      setSavedSnapshot(null);
      setArtMessage(null);
      refreshWorkflowList();
    } catch {
      // Leave the selection as-is on failure.
    } finally {
      setWorkflowBusy(false);
    }
  };

  // The rename dialog is open when renameDraft is a string (null = closed).
  const openRenameDialog = () => {
    if (isLive) return;
    setRenameDraft(currentWorkflowName);
  };

  const submitRename = async () => {
    const name = (renameDraft ?? "").trim();
    if (isLive || !name) {
      setRenameDraft(null);
      return;
    }
    const target = selectedWorkflow;
    setWorkflowBusy(true);
    try {
      await renameCanvasWorkflow(baseUrl, target, name);
      refreshWorkflowList();
    } catch {
      // Ignore; the list refresh keeps the last known name.
    } finally {
      setWorkflowBusy(false);
      setRenameDraft(null);
    }
  };

  const addWorkflowToDesktop = async () => {
    if (isLive || !hasNodes) return;
    setWorkflowBusy(true);
    setWorkflowBusyAction("desktop");
    setArtMessage(null);
    try {
      const graph = buildHookWorkflowInstantiationGraph(activeSnapshot, baseUrl);
      await instantiateArtLoomWorkflow(baseUrl, {
        ...graph,
        mode: "reference",
        workflowId: selectedWorkflow,
      });
      setArtMessage({ ok: true, text: `已添加到桌面：${currentWorkflowName}` });
    } catch (error) {
      setArtMessage({
        ok: false,
        text: error instanceof Error ? error.message : "添加到桌面失败。",
      });
    } finally {
      setWorkflowBusy(false);
      setWorkflowBusyAction(null);
    }
  };

  // Prepare the saved workflow for the standard Art creation dialog. Constants
  // are persisted first; the dialog receives the generated ports, parameters,
  // bindings and identity as editable defaults instead of registering directly.
  const addWorkflowAsArt = async () => {
    if (isLive) return;
    if (!onCreateWorkflowArt) {
      setArtMessage({ ok: false, text: "Art 创建界面暂不可用。" });
      return;
    }
    if (!workflowInterface.inputs.length) {
      setArtMessage({ ok: false, text: "该工作流没有可用的输入图像，无法封装为 Art。" });
      return;
    }
    const params: ExposableParam[] = paramGroups.flatMap((group) =>
      group.rows.map((row) => ({
        workflowNodeId: group.workflowNodeId,
        target: row.target,
        label: `${group.label} / ${row.label}`,
        uiType: mapParamUiType(row.param),
        executionType: mapParamExecutionType(row.param, mapParamUiType(row.param)),
        widget: row.param.widget,
        dataType: row.param.dataType || row.param.data_type,
        defaultValue: row.param.default,
        min: row.param.min,
        max: row.param.max,
        step: row.param.step,
        options: row.param.options,
        multiline: row.param.multiline,
        group: row.param.group,
        required: row.param.required,
        secret: row.param.secret,
      })),
    );
    const { yaml, tool } = buildWorkflowArtBundle({
      snapshot: activeSnapshot,
      workflowId: selectedWorkflow,
      workflowName: currentWorkflowName,
      params,
      exposed: exposedParams,
      values: paramValues,
    });
    setWorkflowBusy(true);
    setWorkflowBusyAction("art");
    setArtMessage(null);
    try {
      // Persist baked constants before opening the creator. The final Art is
      // created only after the user reviews and submits the prefilled form.
      await saveWorkflowBundle(baseUrl, { id: selectedWorkflow }, yaml);
      onCreateWorkflowArt({
        workflowId: selectedWorkflow,
        workflowName: currentWorkflowName,
        tool,
      });
    } catch (error) {
      setArtMessage({
        ok: false,
        text: error instanceof Error ? error.message : "添加为 Art 失败。",
      });
    } finally {
      setWorkflowBusy(false);
      setWorkflowBusyAction(null);
    }
  };

  return (
    <section
      className="hook-canvas-thumbnail"
      data-testid="hook-canvas-thumbnail"
      data-revision={snapshot?.revision ?? "empty"}
    >
      <div className="hook-canvas-toolbar">
        <label className="hook-canvas-workflow-select">
          <span className="hook-canvas-workflow-select__label">工作流</span>
          <select
            value={selectedWorkflow}
            onChange={(event) => {
              setSelectedNodeId(null);
              setSaveMessage(null);
              setArtMessage(null);
              setSelectedWorkflow(event.target.value);
            }}
          >
            <option value={LIVE_WORKFLOW_ID}>桌面同步</option>
            {workflows.map((workflow) => (
              <option key={workflow.id} value={workflow.id}>
                {workflow.name}（{workflow.nodeCount} 节点）
              </option>
            ))}
          </select>
        </label>
        {!isLive ? (
          <div className="hook-canvas-workflow-actions">
            <button
              className="ghost-button"
              type="button"
              onClick={openRenameDialog}
              disabled={workflowBusy}
            >
              重命名
            </button>
            <button
              className="ghost-button hook-canvas-workflow-delete"
              type="button"
              onClick={deleteSelectedWorkflow}
              disabled={workflowBusy}
            >
              删除
            </button>
          </div>
        ) : null}
        {hasNodes ? (
          <div className="hook-canvas-toolbar__controls">
            {isLive ? (
              <button
                className="signal-button hook-canvas-save-workflow"
                type="button"
                onClick={saveAsWorkflow}
                disabled={saving || !selectedNodeId || !highlighted.size}
              >
                {saving ? "保存中" : "保存为工作流"}
              </button>
            ) : null}
            <label className="hook-canvas-zoom">
              <span className="hook-canvas-zoom__label">缩放</span>
              <input
                className="hook-canvas-zoom__slider"
                type="range"
                min={0}
                max={1000}
                step={1}
                value={scaleToSliderValue(effectiveViewport.scale, MIN_SCALE, MAX_SCALE)}
                onChange={(event) => zoomToScale(sliderValueToScale(Number(event.target.value), MIN_SCALE, MAX_SCALE))}
                aria-label="画布缩放"
              />
              <span className="hook-canvas-zoom__value">{Math.round(effectiveViewport.scale * 100)}%</span>
            </label>
            <button
              className="ghost-button hook-canvas-minimap-toggle"
              type="button"
              onClick={() => setShowMinimap((value) => !value)}
              aria-pressed={showMinimap}
            >
              {showMinimap ? "隐藏缩略图" : "显示缩略图"}
            </button>
            {isLive && saveMessage ? (
              <span className={saveMessage.ok ? "success-text" : "error-text"}>{saveMessage.text}</span>
            ) : null}
          </div>
        ) : null}
      </div>
      <div
        ref={surfaceRef}
        className={`hook-canvas-surface${hasNodes ? " hook-canvas-surface--interactive" : ""}${isPanning ? " hook-canvas-surface--panning" : ""}`}
        style={{ aspectRatio: `${SURFACE_WIDTH} / ${SURFACE_HEIGHT}` }}
        onWheel={handleWheel}
        onPointerDown={handlePointerDown}
      >
        <div className="hook-canvas-grid" aria-hidden="true" />
        <svg
          className="hook-canvas-edges"
          viewBox={`0 0 ${SURFACE_WIDTH} ${SURFACE_HEIGHT}`}
          role="presentation"
        >
          <defs>
            <marker
              id="hook-canvas-arrow"
              markerWidth="10"
              markerHeight="7"
              refX="9"
              refY="3.5"
              orient="auto"
            >
              <polygon points="0 0, 10 3.5, 0 7" className="hook-canvas-arrow" />
            </marker>
            <marker
              id="hook-canvas-arrow-active"
              markerWidth="10"
              markerHeight="7"
              refX="9"
              refY="3.5"
              orient="auto"
            >
              <polygon points="0 0, 10 3.5, 0 7" className="hook-canvas-arrow--active" />
            </marker>
          </defs>
          {layout.nodes.length ? activeSnapshot.edges.map((edge) => {
            const endpoints = edgeEndpoints(layout, edge);
            if (!endpoints) return null;
            const active = selectedNodeId !== null && isEdgeHighlighted(edge, highlighted);
            // Cubic bezier with horizontal control handles, matching Hook's link
            // curve, and an arrowhead at the target end.
            const dx = Math.max(30, Math.abs(endpoints.target.x - endpoints.source.x) / 2);
            const d = `M ${endpoints.source.x} ${endpoints.source.y} `
              + `C ${endpoints.source.x + dx} ${endpoints.source.y}, `
              + `${endpoints.target.x - dx} ${endpoints.target.y}, `
              + `${endpoints.target.x} ${endpoints.target.y}`;
            return (
              <path
                key={edge.id}
                className={active ? "hook-canvas-edge--active" : undefined}
                d={d}
                fill="none"
                markerEnd={active ? "url(#hook-canvas-arrow-active)" : "url(#hook-canvas-arrow)"}
              />
            );
          }) : null}
        </svg>
        <div className="hook-canvas-node-layer">
          {layout.nodes.map((node) => (
            <HookCanvasNode
              key={node.id}
              node={node}
              baseUrl={baseUrl}
              viewportWidth={SURFACE_WIDTH}
              viewportHeight={SURFACE_HEIGHT}
              selected={selectedNodeId !== null && highlighted.has(node.id)}
              interactive={hasNodes}
              onSelect={handleSelect}
            />
          ))}
        </div>
        {hasNodes && showMinimap ? (
          <div
            ref={minimapRef}
            className="hook-canvas-minimap"
            style={{ width: `${MINIMAP_WIDTH}px`, height: `${MINIMAP_HEIGHT}px` }}
            onPointerDown={handleMinimapPointerDown}
            role="presentation"
          >
            <svg viewBox={`0 0 ${MINIMAP_WIDTH} ${MINIMAP_HEIGHT}`} width="100%" height="100%">
              {activeSnapshot.edges.map((edge) => {
                const endpoints = edgeWorldEndpoints(activeSnapshot, edge);
                if (!endpoints) return null;
                const a = minimap.toMap(endpoints.source.x, endpoints.source.y);
                const b = minimap.toMap(endpoints.target.x, endpoints.target.y);
                return (
                  <line
                    key={edge.id}
                    className="hook-canvas-minimap__edge"
                    x1={a.x}
                    y1={a.y}
                    x2={b.x}
                    y2={b.y}
                    markerEnd="url(#hook-canvas-minimap-arrow)"
                  />
                );
              })}
              <defs>
                <marker
                  id="hook-canvas-minimap-arrow"
                  markerWidth="6"
                  markerHeight="5"
                  refX="5"
                  refY="2.5"
                  orient="auto"
                >
                  <polygon points="0 0, 6 2.5, 0 5" className="hook-canvas-minimap__arrow" />
                </marker>
              </defs>
              {activeSnapshot.nodes.map((node) => {
                const p = minimap.toMap(node.x, node.y);
                const w = Math.max(2, node.width * minimap.scale);
                const h = Math.max(2, node.height * minimap.scale);
                return (
                  <rect
                    key={node.id}
                    className="hook-canvas-minimap__node"
                    x={p.x}
                    y={p.y}
                    width={w}
                    height={h}
                  />
                );
              })}
              <rect
                className="hook-canvas-minimap__view"
                x={minimap.viewRect.x}
                y={minimap.viewRect.y}
                width={minimap.viewRect.w}
                height={minimap.viewRect.h}
              />
            </svg>
          </div>
        ) : null}
      </div>
      {error && !snapshot ? <p className="error-text">{error}</p> : null}
      {!isLive && hasNodes ? (
        <div className="hook-canvas-workflow-io" data-testid="hook-canvas-workflow-io">
          <div className="hook-canvas-workflow-io__head">
            <p className="hook-canvas-workflow-io__title">工作流接口</p>
            <div className="hook-canvas-workflow-io__action">
              <button
                className="ghost-button"
                type="button"
                onClick={() => void addWorkflowToDesktop()}
                disabled={workflowBusy}
              >
                {workflowBusyAction === "desktop" ? "添加中" : "添加到桌面"}
              </button>
              <button
                className="signal-button"
                type="button"
                onClick={() => void addWorkflowAsArt()}
                disabled={workflowBusy}
              >
                {workflowBusyAction === "art" ? "处理中" : "添加为 Art"}
              </button>
              {artMessage ? (
                <span className={artMessage.ok ? "success-text" : "error-text"}>
                  {artMessage.text}
                </span>
              ) : null}
            </div>
          </div>
          <div className="hook-canvas-workflow-io__groups">
            <div className="hook-canvas-workflow-io__group">
              <p className="hook-canvas-workflow-io__label">
                输入属性{totalInputCount ? `（${totalInputCount}）` : ""}
              </p>
              {totalInputCount ? (
                <ul className="hook-canvas-workflow-io__list">
                  {workflowInterface.inputs.map((port) => (
                    <li key={port.nodeId} className="hook-canvas-workflow-io__port">
                      <span className="hook-canvas-workflow-io__port-name">{port.label}</span>
                      <span className="hook-canvas-workflow-io__port-type">输入图像</span>
                    </li>
                  ))}
                  {exposedParamRows.map((row) => (
                    <li key={row.key} className="hook-canvas-workflow-io__port">
                      <span className="hook-canvas-workflow-io__port-name">{row.label}</span>
                      <span className="hook-canvas-workflow-io__port-type">参数</span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="hook-canvas-workflow-io__empty">无</p>
              )}
            </div>
            <div className="hook-canvas-workflow-io__group">
              <p className="hook-canvas-workflow-io__label">输出属性</p>
              {workflowInterface.outputs.length ? (
                <ul className="hook-canvas-workflow-io__list">
                  {workflowInterface.outputs.map((port) => (
                    <li key={port.nodeId} className="hook-canvas-workflow-io__port">
                      <span className="hook-canvas-workflow-io__port-name">{port.label}</span>
                      <span className="hook-canvas-workflow-io__port-type">输出图像</span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="hook-canvas-workflow-io__empty">无</p>
              )}
            </div>
          </div>
          {paramGroups.length ? (
            <div className="hook-canvas-param-expose">
              <p className="hook-canvas-param-expose__hint">
                勾选要外露的参数，未勾选的将以当前值固定。外露的参数会成为封装节点的输入。
              </p>
              {paramGroups.map((group) => (
                <div key={group.workflowNodeId} className="hook-canvas-param-expose__group">
                  <p className="hook-canvas-param-expose__node">{group.label}</p>
                  {group.rows.map((row) => {
                    const exposed = exposedParams.has(row.key);
                    const uiType = mapParamUiType(row.param);
                    const value = paramValues[row.key] ?? "";
                    const setValue = (next: string) =>
                      setParamValues((previous) => ({ ...previous, [row.key]: next }));
                    return (
                      <div key={row.key} className="hook-canvas-param-expose__row">
                        <label className="hook-canvas-param-expose__toggle">
                          <input
                            type="checkbox"
                            checked={exposed}
                            onChange={(event) => {
                              setExposedParams((previous) => {
                                const next = new Set(previous);
                                if (event.target.checked) next.add(row.key);
                                else next.delete(row.key);
                                return next;
                              });
                            }}
                          />
                          <span>{row.label}</span>
                        </label>
                        {uiType === "boolean" ? (
                          <select
                            className="hook-canvas-param-expose__value"
                            value={value || "false"}
                            disabled={exposed}
                            onChange={(event) => setValue(event.target.value)}
                          >
                            <option value="true">true</option>
                            <option value="false">false</option>
                          </select>
                        ) : uiType === "int" || uiType === "float" ? (
                          <input
                            className="hook-canvas-param-expose__value"
                            type="number"
                            value={value}
                            disabled={exposed}
                            placeholder={exposed ? "运行时输入" : "常量值"}
                            min={row.param.min}
                            max={row.param.max}
                            step={row.param.step ?? (uiType === "int" ? 1 : undefined)}
                            onChange={(event) => setValue(event.target.value)}
                          />
                        ) : (
                          <input
                            className="hook-canvas-param-expose__value"
                            value={value}
                            disabled={exposed}
                            placeholder={exposed ? "运行时输入" : "常量值"}
                            onChange={(event) => setValue(event.target.value)}
                          />
                        )}
                      </div>
                    );
                  })}
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
      {selectedNode ? (
        <div className="hook-canvas-node-props" data-testid="hook-canvas-node-props">
          <p className="hook-canvas-node-props__title">节点属性</p>
          <dl className="hook-canvas-node-props__grid">
            <div className="hook-canvas-node-props__row">
              <dt>类型</dt>
              <dd>{NODE_KIND_LABELS[selectedNode.kind] ?? selectedNode.kind}</dd>
            </div>
            {selectedNode.artId ? (
              <div className="hook-canvas-node-props__row">
                <dt>能力</dt>
                <dd>{selectedNode.artId}</dd>
              </div>
            ) : null}
            <div className="hook-canvas-node-props__row">
              <dt>尺寸</dt>
              <dd>
                {Math.round(selectedNode.width)} × {Math.round(selectedNode.height)}
              </dd>
            </div>
            <div className="hook-canvas-node-props__row hook-canvas-node-props__row--wide">
              <dt>节点 ID</dt>
              <dd className="hook-canvas-node-props__mono">{selectedNode.id}</dd>
            </div>
          </dl>
        </div>
      ) : null}
      {renameDraft !== null ? (
        <div
          className="hook-canvas-rename-backdrop"
          role="presentation"
          onClick={() => setRenameDraft(null)}
        >
          <div
            className="hook-canvas-rename-dialog"
            role="dialog"
            aria-label="重命名工作流"
            onClick={(event) => event.stopPropagation()}
          >
            <p className="hook-canvas-rename-dialog__title">重命名工作流</p>
            <input
              className="hook-canvas-rename-dialog__input"
              value={renameDraft}
              autoFocus
              onChange={(event) => setRenameDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void submitRename();
                if (event.key === "Escape") setRenameDraft(null);
              }}
            />
            <div className="hook-canvas-rename-dialog__actions">
              <button
                className="ghost-button"
                type="button"
                onClick={() => setRenameDraft(null)}
                disabled={workflowBusy}
              >
                取消
              </button>
              <button
                className="signal-button"
                type="button"
                onClick={() => void submitRename()}
                disabled={workflowBusy || !renameDraft.trim()}
              >
                {workflowBusy ? "保存中" : "确定"}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
