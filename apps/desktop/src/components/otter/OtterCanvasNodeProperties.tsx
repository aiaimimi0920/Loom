// Read-only details for the currently selected Otter canvas node.

import { Panel } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import type { OtterCanvasNode } from "../../services/otterCanvas.ts";
import { NODE_KIND_LABELS } from "./otterCanvasThumbnailModel.ts";

export function OtterCanvasNodeProperties({ node }: { node: OtterCanvasNode }) {
  const { t } = useUiText();
  return (
    <Panel className="otter-canvas-node-props" data-testid="otter-canvas-node-props" aria-live="polite">
      <p className="otter-canvas-node-props__title">{t("otter.nodeProps.title")}</p>
      <dl className="otter-canvas-node-props__grid">
        <div className="otter-canvas-node-props__row">
          <dt>{t("otter.nodeProps.kind")}</dt>
          <dd title={NODE_KIND_LABELS[node.kind] ?? node.kind}>{NODE_KIND_LABELS[node.kind] ?? node.kind}</dd>
        </div>
        {node.artId ? (
          <div className="otter-canvas-node-props__row">
            <dt>{t("otter.nodeProps.capability")}</dt>
            <dd title={node.artId}>{node.artId}</dd>
          </div>
        ) : null}
        <div className="otter-canvas-node-props__row">
          <dt>{t("otter.nodeProps.dimensions")}</dt>
          <dd>
            {Math.round(node.width)} × {Math.round(node.height)}
          </dd>
        </div>
        <div className="otter-canvas-node-props__row otter-canvas-node-props__row--wide">
          <dt>{t("otter.nodeProps.nodeId")}</dt>
          <dd className="otter-canvas-node-props__mono" title={node.id}>
            {node.id}
          </dd>
        </div>
      </dl>
    </Panel>
  );
}
