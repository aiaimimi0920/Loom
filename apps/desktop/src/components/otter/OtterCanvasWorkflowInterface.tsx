// Saved-workflow interface summary and actions for desktop/Art instantiation.

import { Badge, Button, Notice, Panel } from "@neuro/ui";
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
  const totalInputCount = workflowInterface.inputs.length + exposedParamRows.length;
  return (
    <Panel className="otter-canvas-workflow-io" data-testid="otter-canvas-workflow-io" aria-busy={workflowBusy}>
      <div className="otter-canvas-workflow-io__head">
        <p className="otter-canvas-workflow-io__title">工作流接口</p>
        <div className="otter-canvas-workflow-io__action">
          <Button onClick={onAddToDesktop} disabled={workflowBusy} loading={workflowBusyAction === "desktop"}>
            添加到桌面
          </Button>
          <Button variant="primary" onClick={onAddAsArt} disabled={workflowBusy} loading={workflowBusyAction === "art"}>
            添加为 Art
          </Button>
        </div>
      </div>
      {message ? <Notice tone={message.ok ? "success" : "danger"}>{message.text}</Notice> : null}
      <div className="otter-canvas-workflow-io__groups">
        <div className="otter-canvas-workflow-io__group">
          <p className="otter-canvas-workflow-io__label">输入属性{totalInputCount ? `（${totalInputCount}）` : ""}</p>
          {totalInputCount ? (
            <ul className="otter-canvas-workflow-io__list" tabIndex={0} aria-label="工作流输入属性">
              {workflowInterface.inputs.map((port) => (
                <li key={`${port.nodeId}::${port.portId}`} className="otter-canvas-workflow-io__port">
                  <span className="otter-canvas-workflow-io__port-name" title={port.label}>
                    {port.label}
                  </span>
                  <Badge>输入图像</Badge>
                </li>
              ))}
              {exposedParamRows.map((row) => (
                <li key={row.key} className="otter-canvas-workflow-io__port">
                  <span className="otter-canvas-workflow-io__port-name" title={row.label}>
                    {row.label}
                  </span>
                  <Badge>参数</Badge>
                </li>
              ))}
            </ul>
          ) : (
            <p className="otter-canvas-workflow-io__empty">无</p>
          )}
        </div>
        <div className="otter-canvas-workflow-io__group">
          <p className="otter-canvas-workflow-io__label">输出属性</p>
          {workflowInterface.outputs.length ? (
            <ul className="otter-canvas-workflow-io__list" tabIndex={0} aria-label="工作流输出属性">
              {workflowInterface.outputs.map((port) => (
                <li key={`${port.nodeId}::${port.portId}`} className="otter-canvas-workflow-io__port">
                  <span className="otter-canvas-workflow-io__port-name" title={port.label}>
                    {port.label}
                  </span>
                  <Badge>输出图像</Badge>
                </li>
              ))}
            </ul>
          ) : (
            <p className="otter-canvas-workflow-io__empty">无</p>
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
