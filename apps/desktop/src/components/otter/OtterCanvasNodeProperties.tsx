// Read-only details for the currently selected Otter canvas node.

import type { OtterCanvasNode } from "../../services/otterCanvas.ts";
import { NODE_KIND_LABELS } from "./otterCanvasThumbnailModel.ts";

export function OtterCanvasNodeProperties({ node }: { node: OtterCanvasNode }) {
  return (
    <div className="otter-canvas-node-props" data-testid="otter-canvas-node-props" aria-live="polite">
      <p className="otter-canvas-node-props__title">节点属性</p>
      <dl className="otter-canvas-node-props__grid">
        <div className="otter-canvas-node-props__row">
          <dt>类型</dt>
          <dd>{NODE_KIND_LABELS[node.kind] ?? node.kind}</dd>
        </div>
        {node.artId ? (
          <div className="otter-canvas-node-props__row">
            <dt>能力</dt>
            <dd>{node.artId}</dd>
          </div>
        ) : null}
        <div className="otter-canvas-node-props__row">
          <dt>尺寸</dt>
          <dd>{Math.round(node.width)} × {Math.round(node.height)}</dd>
        </div>
        <div className="otter-canvas-node-props__row otter-canvas-node-props__row--wide">
          <dt>节点 ID</dt>
          <dd className="otter-canvas-node-props__mono">{node.id}</dd>
        </div>
      </dl>
    </div>
  );
}
