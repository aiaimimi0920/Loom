import { useMemo } from "react";

import {
  edgeEndpoints,
  fitHookCanvas,
  type HookCanvasSnapshot,
} from "../../services/hookCanvas.ts";
import { HookCanvasNode } from "./HookCanvasNode.tsx";

const VIEWPORT_WIDTH = 1000;
const VIEWPORT_HEIGHT = 620;

interface HookCanvasThumbnailProps {
  snapshot: HookCanvasSnapshot | null;
  baseUrl: string;
  loading: boolean;
  error: string | null;
  hookConnected: boolean;
  onRefresh: () => void;
  onOpen: (nodeId?: string) => void;
}

function connectionLabel(
  snapshot: HookCanvasSnapshot | null,
  loading: boolean,
  error: string | null,
  hookConnected: boolean,
): string {
  if (loading) return "同步中";
  if (error && snapshot) return "离线快照";
  if (hookConnected) return "实时连接";
  if (snapshot?.available) return "离线快照";
  return "等待 Hook";
}

export function HookCanvasThumbnail({
  snapshot,
  baseUrl,
  loading,
  error,
  hookConnected,
  onRefresh,
  onOpen,
}: HookCanvasThumbnailProps) {
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
  const layout = useMemo(
    () => fitHookCanvas(snapshot ?? emptySnapshot, {
      width: VIEWPORT_WIDTH,
      height: VIEWPORT_HEIGHT,
      padding: 32,
      minimumNodeSize: 24,
    }),
    [emptySnapshot, snapshot],
  );
  const status = connectionLabel(snapshot, loading, error, hookConnected);
  const hasNodes = Boolean(snapshot?.nodes.length);

  return (
    <section
      className="hook-canvas-thumbnail"
      data-testid="hook-canvas-thumbnail"
      data-revision={snapshot?.revision ?? "empty"}
    >
      <div className="hook-canvas-thumbnail__head">
        <div>
          <p className="card-kicker">实时画布</p>
          <h3>Hook 节点排列</h3>
        </div>
        <span className="hook-canvas-status">{status}</span>
      </div>
      <div
        className="hook-canvas-surface"
        style={{ aspectRatio: `${VIEWPORT_WIDTH} / ${VIEWPORT_HEIGHT}` }}
      >
        <div className="hook-canvas-grid" aria-hidden="true" />
        <svg
          className="hook-canvas-edges"
          viewBox={`0 0 ${VIEWPORT_WIDTH} ${VIEWPORT_HEIGHT}`}
          role="presentation"
        >
          {layout.nodes.length ? (snapshot?.edges ?? []).map((edge) => {
            const endpoints = edgeEndpoints(layout, edge);
            if (!endpoints) return null;
            return (
              <line
                key={edge.id}
                x1={endpoints.source.x}
                y1={endpoints.source.y}
                x2={endpoints.target.x}
                y2={endpoints.target.y}
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
              viewportWidth={VIEWPORT_WIDTH}
              viewportHeight={VIEWPORT_HEIGHT}
              selected={false}
              interactive={hasNodes}
              onSelect={onOpen}
            />
          ))}
        </div>
        {!hasNodes ? (
          <div className="hook-canvas-empty" role="status">
            <strong>{snapshot?.available ? "Hook 画布为空" : "等待 Hook 画布"}</strong>
            <span>连接 Hook 后，这里会显示真实节点、预览和连线。</span>
          </div>
        ) : null}
      </div>
      <div className="hook-canvas-metrics" aria-label="Hook 画布状态">
        <span><strong>{snapshot?.nodes.length ?? 0}</strong> 节点</span>
        <span><strong>{snapshot?.edges.length ?? 0}</strong> 连线</span>
        <span><strong>{snapshot?.revision ? snapshot.revision.slice(0, 8) : "--"}</strong> 版本</span>
      </div>
      {error && !snapshot ? <p className="error-text">{error}</p> : null}
      {snapshot?.warnings.length ? (
        <p className="hook-canvas-warning">部分节点预览不可用，但画布布局仍已保留。</p>
      ) : null}
      <div className="hook-canvas-thumbnail__actions">
        <button className="ghost-button" type="button" onClick={onRefresh} disabled={loading}>
          {loading ? "刷新中" : "刷新画布"}
        </button>
        <button className="signal-button" type="button" onClick={() => onOpen()} disabled={!hasNodes}>
          打开可视化工作流
        </button>
      </div>
    </section>
  );
}
