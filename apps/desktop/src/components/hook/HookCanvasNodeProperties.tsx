// Read-only details for the currently selected Hook canvas node.

import type { HookCanvasNode } from "../../services/hookCanvas.ts";
import { NODE_KIND_LABELS } from "./hookCanvasThumbnailModel.ts";

export function HookCanvasNodeProperties({ node }: { node: HookCanvasNode }) {
  return (
    <div className="hook-canvas-node-props" data-testid="hook-canvas-node-props" aria-live="polite">
      <p className="hook-canvas-node-props__title">节点属性</p>
      <dl className="hook-canvas-node-props__grid">
        <div className="hook-canvas-node-props__row">
          <dt>类型</dt>
          <dd>{NODE_KIND_LABELS[node.kind] ?? node.kind}</dd>
        </div>
        {node.artId ? (
          <div className="hook-canvas-node-props__row">
            <dt>能力</dt>
            <dd>{node.artId}</dd>
          </div>
        ) : null}
        <div className="hook-canvas-node-props__row">
          <dt>尺寸</dt>
          <dd>{Math.round(node.width)} × {Math.round(node.height)}</dd>
        </div>
        <div className="hook-canvas-node-props__row hook-canvas-node-props__row--wide">
          <dt>节点 ID</dt>
          <dd className="hook-canvas-node-props__mono">{node.id}</dd>
        </div>
      </dl>
    </div>
  );
}
