// Orchestrates live/frozen Hook canvas state while focused modules own rendering.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  buildHookWorkflowInstantiationGraph,
  buildSubWorkflowYaml,
  buildWorkflowArtBundle,
  connectedNodeIds,
  inferCanvasWorkflowInterface,
  listCanvasWorkflows,
  readCanvasWorkflowSnapshot,
  type CanvasWorkflowSummary,
  type HookCanvasSnapshot,
} from "../../services/hookCanvas.ts";
import {
  deleteCanvasWorkflow,
  instantiateHookWorkflow,
  renameCanvasWorkflow,
  saveHookCanvasWorkflow,
  saveWorkflowBundle,
  type LoomToolDefinition,
} from "../../services/loomApi.ts";
import { HookCanvasNodeProperties } from "./HookCanvasNodeProperties.tsx";
import { HookCanvasRenameDialog } from "./HookCanvasRenameDialog.tsx";
import { HookCanvasSurface } from "./HookCanvasSurface.tsx";
import { HookCanvasToolbar } from "./HookCanvasToolbar.tsx";
import { HookCanvasWorkflowInterface } from "./HookCanvasWorkflowInterface.tsx";
import {
  buildExposableParams,
  buildNodeParamGroups,
  LIVE_WORKFLOW_ID,
  type StatusMessage,
  type WorkflowArtCreationRequest,
} from "./hookCanvasThumbnailModel.ts";
import { useHookCanvasViewport } from "./useHookCanvasViewport.ts";

export type { WorkflowArtCreationRequest } from "./hookCanvasThumbnailModel.ts";

interface HookCanvasThumbnailProps {
  snapshot: HookCanvasSnapshot | null;
  baseUrl: string;
  error: string | null;
  tools?: LoomToolDefinition[];
  onCreateWorkflowArt?: (request: WorkflowArtCreationRequest) => void;
}

const EMPTY_SNAPSHOT: HookCanvasSnapshot = {
  available: false,
  revision: "empty",
  updatedAt: null,
  workflowId: null,
  bounds: { x: 0, y: 0, width: 0, height: 0 },
  nodes: [],
  edges: [],
  warnings: [],
};

export function HookCanvasThumbnail({
  snapshot,
  baseUrl,
  error,
  tools = [],
  onCreateWorkflowArt,
}: HookCanvasThumbnailProps) {
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<StatusMessage | null>(null);
  const [workflows, setWorkflows] = useState<CanvasWorkflowSummary[]>([]);
  const [selectedWorkflow, setSelectedWorkflow] = useState(LIVE_WORKFLOW_ID);
  const [savedSnapshot, setSavedSnapshot] = useState<HookCanvasSnapshot | null>(null);
  const [renameDraft, setRenameDraft] = useState<string | null>(null);
  const [workflowBusy, setWorkflowBusy] = useState(false);
  const [workflowBusyAction, setWorkflowBusyAction] = useState<"desktop" | "art" | null>(null);
  const [artMessage, setArtMessage] = useState<StatusMessage | null>(null);
  const [exposedParams, setExposedParams] = useState<Set<string>>(new Set());
  const [paramValues, setParamValues] = useState<Record<string, string>>({});
  const workflowListGeneration = useRef(0);

  const isLive = selectedWorkflow === LIVE_WORKFLOW_ID;
  const sourceSnapshot = isLive ? snapshot : savedSnapshot;
  const activeSnapshot = sourceSnapshot ?? EMPTY_SNAPSHOT;
  const hasNodes = Boolean(sourceSnapshot?.nodes.length);
  const revision = sourceSnapshot?.revision ?? "empty";

  const refreshWorkflowList = useCallback(() => {
    const generation = ++workflowListGeneration.current;
    void listCanvasWorkflows(baseUrl)
      .then((nextWorkflows) => {
        if (workflowListGeneration.current === generation) setWorkflows(nextWorkflows);
      })
      .catch(() => {
        if (workflowListGeneration.current === generation) setWorkflows([]);
      });
  }, [baseUrl]);

  useEffect(() => {
    refreshWorkflowList();
    return () => {
      workflowListGeneration.current += 1;
    };
  }, [refreshWorkflowList]);

  useEffect(() => {
    if (isLive) {
      setSavedSnapshot(null);
      return;
    }
    let cancelled = false;
    void readCanvasWorkflowSnapshot(baseUrl, selectedWorkflow)
      .then((nextSnapshot) => {
        if (!cancelled) setSavedSnapshot(nextSnapshot);
      })
      .catch(() => {
        if (!cancelled) setSavedSnapshot(null);
      });
    return () => {
      cancelled = true;
    };
  }, [baseUrl, isLive, selectedWorkflow]);

  useEffect(() => {
    setSelectedNodeId(null);
  }, [revision, selectedWorkflow]);

  const highlighted = useMemo(
    () => isLive
      ? connectedNodeIds(activeSnapshot, selectedNodeId)
      : new Set<string>(selectedNodeId ? [selectedNodeId] : []),
    [activeSnapshot, isLive, selectedNodeId],
  );
  const selectedNode = useMemo(
    () => selectedNodeId
      ? activeSnapshot.nodes.find((node) => node.id === selectedNodeId) ?? null
      : null,
    [activeSnapshot, selectedNodeId],
  );
  const workflowInterface = useMemo(
    () => inferCanvasWorkflowInterface(activeSnapshot),
    [activeSnapshot],
  );
  const paramGroups = useMemo(
    () => buildNodeParamGroups(activeSnapshot, tools),
    [activeSnapshot, tools],
  );

  useEffect(() => {
    setParamValues((previous) => {
      const next: Record<string, string> = {};
      for (const group of paramGroups) {
        for (const row of group.rows) next[row.key] = previous[row.key] ?? row.currentValue;
      }
      return next;
    });
    setExposedParams((previous) => {
      const validKeys = new Set(paramGroups.flatMap((group) => group.rows.map((row) => row.key)));
      return new Set([...previous].filter((key) => validKeys.has(key)));
    });
  }, [paramGroups]);

  const exposedParamRows = useMemo(
    () => paramGroups.flatMap((group) =>
      group.rows
        .filter((row) => exposedParams.has(row.key))
        .map((row) => ({ key: row.key, label: `${group.label} / ${row.label}` })),
    ),
    [exposedParams, paramGroups],
  );

  const onNodeSelect = useCallback((nodeId: string) => {
    setSaveMessage(null);
    setSelectedNodeId((current) => current === nodeId ? null : nodeId);
  }, []);
  const canvasViewport = useHookCanvasViewport({
    snapshot: activeSnapshot,
    hasNodes,
    revision,
    selectedWorkflow,
    onNodeSelect,
  });

  const currentWorkflowName = workflows.find(
    (workflow) => workflow.id === selectedWorkflow,
  )?.name ?? selectedWorkflow;

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
      // Preserve the selected workflow when the daemon rejects deletion.
    } finally {
      setWorkflowBusy(false);
    }
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
      // A later refresh keeps the last confirmed daemon name.
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
      await instantiateHookWorkflow(baseUrl, {
        ...graph,
        mode: "reference",
        workflowId: selectedWorkflow,
      });
      setArtMessage({ ok: true, text: `已添加到桌面：${currentWorkflowName}` });
    } catch (actionError) {
      setArtMessage({
        ok: false,
        text: actionError instanceof Error ? actionError.message : "添加到桌面失败。",
      });
    } finally {
      setWorkflowBusy(false);
      setWorkflowBusyAction(null);
    }
  };

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
    const { yaml, tool } = buildWorkflowArtBundle({
      snapshot: activeSnapshot,
      workflowId: selectedWorkflow,
      workflowName: currentWorkflowName,
      params: buildExposableParams(paramGroups),
      exposed: exposedParams,
      values: paramValues,
    });
    setWorkflowBusy(true);
    setWorkflowBusyAction("art");
    setArtMessage(null);
    try {
      await saveWorkflowBundle(baseUrl, { id: selectedWorkflow }, yaml);
      onCreateWorkflowArt({ workflowId: selectedWorkflow, workflowName: currentWorkflowName, tool });
    } catch (actionError) {
      setArtMessage({
        ok: false,
        text: actionError instanceof Error ? actionError.message : "添加为 Art 失败。",
      });
    } finally {
      setWorkflowBusy(false);
      setWorkflowBusyAction(null);
    }
  };

  const handleWorkflowChange = (workflowId: string) => {
    setSelectedNodeId(null);
    setSaveMessage(null);
    setArtMessage(null);
    setSelectedWorkflow(workflowId);
  };
  const handleExposureChange = (key: string, exposed: boolean) => {
    setExposedParams((previous) => {
      const next = new Set(previous);
      if (exposed) next.add(key);
      else next.delete(key);
      return next;
    });
  };
  const handleValueChange = (key: string, value: string) => {
    setParamValues((previous) => ({ ...previous, [key]: value }));
  };

  return (
    <section
      className="hook-canvas-thumbnail"
      data-testid="hook-canvas-thumbnail"
      data-revision={snapshot?.revision ?? "empty"}
    >
      <HookCanvasToolbar
        workflows={workflows}
        selectedWorkflow={selectedWorkflow}
        isLive={isLive}
        hasNodes={hasNodes}
        workflowBusy={workflowBusy}
        saving={saving}
        canSave={Boolean(selectedNodeId && highlighted.size)}
        scale={canvasViewport.viewport.scale}
        showMinimap={canvasViewport.showMinimap}
        saveMessage={saveMessage}
        onWorkflowChange={handleWorkflowChange}
        onRename={() => setRenameDraft(currentWorkflowName)}
        onDelete={() => void deleteSelectedWorkflow()}
        onSave={() => void saveAsWorkflow()}
        onScaleChange={canvasViewport.zoomToScale}
        onToggleMinimap={canvasViewport.toggleMinimap}
      />
      <HookCanvasSurface
        snapshot={activeSnapshot}
        layout={canvasViewport.layout}
        baseUrl={baseUrl}
        hasNodes={hasNodes}
        isPanning={canvasViewport.isPanning}
        selectedNodeId={selectedNodeId}
        highlighted={highlighted}
        showMinimap={canvasViewport.showMinimap}
        minimap={canvasViewport.minimap}
        surfaceRef={canvasViewport.surfaceRef}
        minimapRef={canvasViewport.minimapRef}
        onWheel={canvasViewport.handleWheel}
        onPointerDown={canvasViewport.handlePointerDown}
        onMinimapPointerDown={canvasViewport.handleMinimapPointerDown}
        onKeyDown={canvasViewport.handleSurfaceKeyDown}
        onSelect={canvasViewport.handleNodeSelect}
      />
      {error && !snapshot ? <p className="error-text" role="alert">{error}</p> : null}
      {!isLive && hasNodes ? (
        <HookCanvasWorkflowInterface
          workflowInterface={workflowInterface}
          exposedParamRows={exposedParamRows}
          paramGroups={paramGroups}
          exposedParams={exposedParams}
          paramValues={paramValues}
          workflowBusy={workflowBusy}
          workflowBusyAction={workflowBusyAction}
          message={artMessage}
          onAddToDesktop={() => void addWorkflowToDesktop()}
          onAddAsArt={() => void addWorkflowAsArt()}
          onExposureChange={handleExposureChange}
          onValueChange={handleValueChange}
        />
      ) : null}
      {selectedNode ? <HookCanvasNodeProperties node={selectedNode} /> : null}
      {renameDraft !== null ? (
        <HookCanvasRenameDialog
          value={renameDraft}
          busy={workflowBusy}
          onChange={setRenameDraft}
          onClose={() => setRenameDraft(null)}
          onSubmit={() => void submitRename()}
        />
      ) : null}
    </section>
  );
}
