// Workflow selection, persistence actions, and viewport controls for the canvas.

import {
  scaleToSliderValue,
  sliderValueToScale,
  type CanvasWorkflowSummary,
} from "../../services/otterCanvas.ts";
import { LIVE_WORKFLOW_ID, type StatusMessage } from "./otterCanvasThumbnailModel.ts";
import { MAX_SCALE, MIN_SCALE } from "./useOtterCanvasViewport.ts";

interface OtterCanvasToolbarProps {
  workflows: CanvasWorkflowSummary[];
  selectedWorkflow: string;
  isLive: boolean;
  hasNodes: boolean;
  workflowBusy: boolean;
  saving: boolean;
  canSave: boolean;
  scale: number;
  showMinimap: boolean;
  saveMessage: StatusMessage | null;
  onWorkflowChange: (workflowId: string) => void;
  onRename: () => void;
  onDelete: () => void;
  onSave: () => void;
  onScaleChange: (scale: number) => void;
  onToggleMinimap: () => void;
}

export function OtterCanvasToolbar({
  workflows,
  selectedWorkflow,
  isLive,
  hasNodes,
  workflowBusy,
  saving,
  canSave,
  scale,
  showMinimap,
  saveMessage,
  onWorkflowChange,
  onRename,
  onDelete,
  onSave,
  onScaleChange,
  onToggleMinimap,
}: OtterCanvasToolbarProps) {
  return (
    <div className="otter-canvas-toolbar">
      <label className="otter-canvas-workflow-select">
        <span className="otter-canvas-workflow-select__label">工作流</span>
        <select value={selectedWorkflow} onChange={(event) => onWorkflowChange(event.target.value)}>
          <option value={LIVE_WORKFLOW_ID}>桌面同步</option>
          {workflows.map((workflow) => (
            <option key={workflow.id} value={workflow.id}>
              {workflow.name}（{workflow.nodeCount} 节点）
            </option>
          ))}
        </select>
      </label>
      {!isLive ? (
        <div className="otter-canvas-workflow-actions">
          <button className="ghost-button" type="button" onClick={onRename} disabled={workflowBusy}>
            重命名
          </button>
          <button
            className="ghost-button otter-canvas-workflow-delete"
            type="button"
            onClick={onDelete}
            disabled={workflowBusy}
          >
            删除
          </button>
        </div>
      ) : null}
      {hasNodes ? (
        <div className="otter-canvas-toolbar__controls">
          {isLive ? (
            <button
              className="signal-button otter-canvas-save-workflow"
              type="button"
              onClick={onSave}
              disabled={saving || !canSave}
            >
              {saving ? "保存中" : "保存为工作流"}
            </button>
          ) : null}
          <label className="otter-canvas-zoom">
            <span className="otter-canvas-zoom__label">缩放</span>
            <input
              className="otter-canvas-zoom__slider"
              type="range"
              min={0}
              max={1000}
              step={1}
              value={scaleToSliderValue(scale, MIN_SCALE, MAX_SCALE)}
              onChange={(event) => onScaleChange(
                sliderValueToScale(Number(event.target.value), MIN_SCALE, MAX_SCALE),
              )}
              aria-label="画布缩放"
              aria-valuetext={`${Math.round(scale * 100)}%`}
            />
            <span className="otter-canvas-zoom__value">{Math.round(scale * 100)}%</span>
          </label>
          <button
            className="ghost-button otter-canvas-minimap-toggle"
            type="button"
            onClick={onToggleMinimap}
            aria-pressed={showMinimap}
          >
            {showMinimap ? "隐藏缩略图" : "显示缩略图"}
          </button>
          {isLive && saveMessage ? (
            <span
              className={saveMessage.ok ? "success-text" : "error-text"}
              role={saveMessage.ok ? "status" : "alert"}
            >
              {saveMessage.text}
            </span>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
