import { useMemo } from "react";

import {
  edgeEndpoints,
  fitHookCanvas,
  type HookCanvasSnapshot,
} from "../../services/hookCanvas.ts";
import { HookCanvasNode } from "./HookCanvasNode.tsx";

const VIEWPORT_WIDTH = 1200;
const VIEWPORT_HEIGHT = 700;

interface HookCanvasViewProps {
  snapshot: HookCanvasSnapshot;
  baseUrl: string;
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
  onRunWorkflow?: () => void;
}

export function HookCanvasView({
  snapshot,
  baseUrl,
  selectedNodeId,
  onSelectNode,
  onRunWorkflow,
}: HookCanvasViewProps) {
  const layout = useMemo(
    () => fitHookCanvas(snapshot, {
      width: VIEWPORT_WIDTH,
      height: VIEWPORT_HEIGHT,
      padding: 42,
      minimumNodeSize: 28,
    }),
    [snapshot],
  );
  const selectedNode = snapshot.nodes.find((node) => node.id === selectedNodeId) ?? null;

  return (
    <section className="hook-canvas-workspace" data-testid="hook-canvas-view">
      <div className="hook-canvas-workspace__head">
        <div>
          <p className="card-kicker">Hook 实时工作流</p>
          <h3>可视化节点画布</h3>
        </div>
        <span className="hook-canvas-status">{snapshot.nodes.length} 个节点 · {snapshot.edges.length} 条连线</span>
      </div>
      <div className="hook-canvas-workspace__body">
        <div
          className="hook-canvas-surface hook-canvas-surface--large"
          style={{ aspectRatio: `${VIEWPORT_WIDTH} / ${VIEWPORT_HEIGHT}` }}
        >
          <div className="hook-canvas-grid" aria-hidden="true" />
          <svg
            className="hook-canvas-edges"
            viewBox={`0 0 ${VIEWPORT_WIDTH} ${VIEWPORT_HEIGHT}`}
            role="presentation"
          >
            {snapshot.edges.map((edge) => {
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
            })}
          </svg>
          <div className="hook-canvas-node-layer">
            {layout.nodes.map((node) => (
              <HookCanvasNode
                key={node.id}
                node={node}
                baseUrl={baseUrl}
                viewportWidth={VIEWPORT_WIDTH}
                viewportHeight={VIEWPORT_HEIGHT}
                selected={node.id === selectedNodeId}
                interactive
                onSelect={onSelectNode}
              />
            ))}
          </div>
        </div>
        <aside className="hook-canvas-inspector">
          <div>
            <p className="card-kicker">节点检查器</p>
            <h4>{selectedNode?.label ?? "选择一个节点"}</h4>
          </div>
          {selectedNode ? (
            <dl>
              <div><dt>类型</dt><dd>{selectedNode.kind}</dd></div>
              <div><dt>状态</dt><dd>{selectedNode.status}</dd></div>
              <div><dt>位置</dt><dd>{Math.round(selectedNode.x)}, {Math.round(selectedNode.y)}</dd></div>
              <div><dt>尺寸</dt><dd>{Math.round(selectedNode.width)} × {Math.round(selectedNode.height)}</dd></div>
              <div><dt>预览</dt><dd>{selectedNode.previewAvailable ? "可用" : "不可用"}</dd></div>
            </dl>
          ) : (
            <p>点击画布中的节点查看状态和几何信息。</p>
          )}
          {onRunWorkflow ? (
            <button className="signal-button" type="button" onClick={onRunWorkflow} disabled={!selectedNode}>
              执行工作流
            </button>
          ) : null}
        </aside>
      </div>
    </section>
  );
}
