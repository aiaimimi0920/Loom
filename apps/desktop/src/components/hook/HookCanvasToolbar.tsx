// Workflow selection, persistence actions, and viewport controls for the canvas.

import {
  scaleToSliderValue,
  sliderValueToScale,
  type CanvasWorkflowSummary,
} from "../../services/hookCanvas.ts";
import { LIVE_WORKFLOW_ID, type StatusMessage } from "./hookCanvasThumbnailModel.ts";
import { MAX_SCALE, MIN_SCALE } from "./useHookCanvasViewport.ts";

interface HookCanvasToolbarProps {
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

export function HookCanvasToolbar({
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
}: HookCanvasToolbarProps) {
  return (
    <div className="hook-canvas-toolbar">
      <label className="hook-canvas-workflow-select">
        <span className="hook-canvas-workflow-select__label">工作流</span>
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
        <div className="hook-canvas-workflow-actions">
          <button className="ghost-button" type="button" onClick={onRename} disabled={workflowBusy}>
            重命名
          </button>
          <button
            className="ghost-button hook-canvas-workflow-delete"
            type="button"
            onClick={onDelete}
            disabled={workflowBusy}
          >
            删除
          </button>
        </div>
      ) : null}
      {hasNodes ? (
        <div className="hook-canvas-toolbar__controls">
          {isLive ? (
            <button
              className="signal-button hook-canvas-save-workflow"
              type="button"
              onClick={onSave}
              disabled={saving || !canSave}
            >
              {saving ? "保存中" : "保存为工作流"}
            </button>
          ) : null}
          <label className="hook-canvas-zoom">
            <span className="hook-canvas-zoom__label">缩放</span>
            <input
              className="hook-canvas-zoom__slider"
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
            <span className="hook-canvas-zoom__value">{Math.round(scale * 100)}%</span>
          </label>
          <button
            className="ghost-button hook-canvas-minimap-toggle"
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
