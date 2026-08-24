// Saved-workflow interface summary and actions for desktop/Art instantiation.

import type { CanvasWorkflowInterface } from "../../services/hookCanvas.ts";
import { HookCanvasParamExposure } from "./HookCanvasParamExposure.tsx";
import type { NodeParamGroup, StatusMessage } from "./hookCanvasThumbnailModel.ts";

interface HookCanvasWorkflowInterfaceProps {
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

export function HookCanvasWorkflowInterface({
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
}: HookCanvasWorkflowInterfaceProps) {
  const totalInputCount = workflowInterface.inputs.length + exposedParamRows.length;
  return (
    <div
      className="hook-canvas-workflow-io"
      data-testid="hook-canvas-workflow-io"
      aria-busy={workflowBusy}
    >
      <div className="hook-canvas-workflow-io__head">
        <p className="hook-canvas-workflow-io__title">工作流接口</p>
        <div className="hook-canvas-workflow-io__action">
          <button className="ghost-button" type="button" onClick={onAddToDesktop} disabled={workflowBusy}>
            {workflowBusyAction === "desktop" ? "添加中" : "添加到桌面"}
          </button>
          <button className="signal-button" type="button" onClick={onAddAsArt} disabled={workflowBusy}>
            {workflowBusyAction === "art" ? "处理中" : "添加为 Art"}
          </button>
          {message ? (
            <span
              className={message.ok ? "success-text" : "error-text"}
              role={message.ok ? "status" : "alert"}
            >
              {message.text}
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
                <li key={`${port.nodeId}::${port.portId}`} className="hook-canvas-workflow-io__port">
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
                <li key={`${port.nodeId}::${port.portId}`} className="hook-canvas-workflow-io__port">
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
      <HookCanvasParamExposure
        groups={paramGroups}
        exposed={exposedParams}
        values={paramValues}
        onExposureChange={onExposureChange}
        onValueChange={onValueChange}
      />
    </div>
  );
}
