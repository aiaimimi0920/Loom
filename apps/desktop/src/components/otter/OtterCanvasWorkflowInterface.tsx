// Saved-workflow interface summary and actions for desktop/Art instantiation.

import { Badge, Button, Notice, Panel } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import type { CanvasWorkflowInterface } from "../../services/otterCanvas.ts";
import { OtterCanvasParamExposure } from "./OtterCanvasParamExposure.tsx";
import type { NodeParamGroup, StatusMessage } from "./otterCanvasThumbnailModel.ts";

interface OtterCanvasWorkflowInterfaceProps {
  workflowInterface: CanvasWorkflowInterface;
  exposedParamRows: Array<{ key: string; label: string }>;
  paramGroups: NodeParamGroup[];
  exposedParams: Set<string>;
  paramValues: Record<string, string>;
  workflowBusy: boolean;
  workflowBusyAction: "desktop" | "art" | null;
  message: StatusMessage | null;
  onAddToDesktop: () => void;
  onAddAsArt: () => void;
  onExposureChange: (key: string, exposed: boolean) => void;
  onValueChange: (key: string, value: string) => void;
}

export function OtterCanvasWorkflowInterface({
  workflowInterface,
  exposedParamRows,
  paramGroups,
  exposedParams,
  paramValues,
  workflowBusy,
  workflowBusyAction,
  message,
  onAddToDesktop,
  onAddAsArt,
  onExposureChange,
  onValueChange,
}: OtterCanvasWorkflowInterfaceProps) {
  const { t, tf } = useUiText();
  const totalInputCount = workflowInterface.inputs.length + exposedParamRows.length;
  return (
    <Panel className="otter-canvas-workflow-io" data-testid="otter-canvas-workflow-io" aria-busy={workflowBusy}>
      <div className="otter-canvas-workflow-io__head">
        <p className="otter-canvas-workflow-io__title">{t("otter.interface.title")}</p>
        <div className="otter-canvas-workflow-io__action">
          <Button onClick={onAddToDesktop} disabled={workflowBusy} loading={workflowBusyAction === "desktop"}>
            {t("otter.interface.addToDesktop")}
          </Button>
          <Button variant="primary" onClick={onAddAsArt} disabled={workflowBusy} loading={workflowBusyAction === "art"}>
            {t("otter.interface.addAsArt")}
          </Button>
        </div>
      </div>
      {message ? <Notice tone={message.ok ? "success" : "danger"}>{message.text}</Notice> : null}
      <div className="otter-canvas-workflow-io__groups">
        <div className="otter-canvas-workflow-io__group">
          <p className="otter-canvas-workflow-io__label">
            {totalInputCount ? tf("otter.interface.inputsCount", { count: totalInputCount }) : t("otter.interface.inputs")}
          </p>
          {totalInputCount ? (
            <ul className="otter-canvas-workflow-io__list" tabIndex={0} aria-label={t("otter.interface.inputsAria")}>
              {workflowInterface.inputs.map((port) => (
                <li key={`${port.nodeId}::${port.portId}`} className="otter-canvas-workflow-io__port">
                  <span className="otter-canvas-workflow-io__port-name" title={port.label}>
                    {port.label}
                  </span>
                  <Badge>{t("otter.port.inputImage")}</Badge>
                </li>
              ))}
              {exposedParamRows.map((row) => (
                <li key={row.key} className="otter-canvas-workflow-io__port">
                  <span className="otter-canvas-workflow-io__port-name" title={row.label}>
                    {row.label}
                  </span>
                  <Badge>{t("otter.port.parameter")}</Badge>
                </li>
              ))}
            </ul>
          ) : (
            <p className="otter-canvas-workflow-io__empty">{t("common.none")}</p>
          )}
        </div>
        <div className="otter-canvas-workflow-io__group">
          <p className="otter-canvas-workflow-io__label">{t("otter.interface.outputs")}</p>
          {workflowInterface.outputs.length ? (
            <ul className="otter-canvas-workflow-io__list" tabIndex={0} aria-label={t("otter.interface.outputsAria")}>
              {workflowInterface.outputs.map((port) => (
                <li key={`${port.nodeId}::${port.portId}`} className="otter-canvas-workflow-io__port">
                  <span className="otter-canvas-workflow-io__port-name" title={port.label}>
                    {port.label}
                  </span>
                  <Badge>{t("otter.port.outputImage")}</Badge>
                </li>
              ))}
            </ul>
          ) : (
            <p className="otter-canvas-workflow-io__empty">{t("common.none")}</p>
          )}
        </div>
      </div>
      <OtterCanvasParamExposure
        groups={paramGroups}
        exposed={exposedParams}
        values={paramValues}
        onExposureChange={onExposureChange}
        onValueChange={onValueChange}
      />
    </Panel>
  );
}
