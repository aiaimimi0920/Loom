// Workflow selection, persistence actions, and viewport controls for the canvas.

import { Button, Field, Notice, Range, Select } from "@neuro/ui";
import { useLayoutEffect, useRef } from "react";
import { scaleToSliderValue, sliderValueToScale, type CanvasWorkflowSummary } from "../../services/otterCanvas.ts";
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
  const workflowSelect = useRef<HTMLSelectElement>(null);
  const focusAfterDelete = useRef(false);
  useLayoutEffect(() => {
    if (workflowBusy || !focusAfterDelete.current) return;
    focusAfterDelete.current = false;
    if (isLive) workflowSelect.current?.focus();
  }, [isLive, workflowBusy]);
  return (
    <div className="otter-canvas-toolbar">
      <div className="otter-canvas-workflow-select">
        <Field label="工作流">
          <Select
            ref={workflowSelect}
            value={selectedWorkflow}
            onChange={(event) => {
              focusAfterDelete.current = false;
              onWorkflowChange(event.target.value);
            }}
          >
            <option value={LIVE_WORKFLOW_ID}>桌面同步</option>
            {workflows.map((workflow) => (
              <option key={workflow.id} value={workflow.id}>
                {workflow.name}（{workflow.nodeCount} 节点）
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {!isLive ? (
        <div className="otter-canvas-workflow-actions">
          <Button variant="quiet" onClick={onRename} disabled={workflowBusy}>
            重命名
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              focusAfterDelete.current = true;
              onDelete();
            }}
            disabled={workflowBusy}
          >
            删除
          </Button>
        </div>
      ) : null}
      {hasNodes ? (
        <div className="otter-canvas-toolbar__controls">
          {isLive ? (
            <Button
              variant="primary"
              className="otter-canvas-save-workflow"
              loading={saving}
              onClick={onSave}
              disabled={saving || !canSave}
            >
              保存为工作流
            </Button>
          ) : null}
          <div className="otter-canvas-zoom">
            <Field label="缩放">
              <Range
                className="otter-canvas-zoom__slider"
                min={0}
                max={1000}
                step={1}
                value={scaleToSliderValue(scale, MIN_SCALE, MAX_SCALE)}
                onChange={(event) =>
                  onScaleChange(sliderValueToScale(Number(event.target.value), MIN_SCALE, MAX_SCALE))
                }
                aria-label="画布缩放"
                aria-valuetext={`${Math.round(scale * 100)}%`}
              />
            </Field>
            <span className="otter-canvas-zoom__value">{Math.round(scale * 100)}%</span>
          </div>
          <Button onClick={onToggleMinimap} aria-pressed={showMinimap}>
            缩略图
          </Button>
        </div>
      ) : null}
      {isLive && hasNodes && saveMessage ? (
        <Notice tone={saveMessage.ok ? "success" : "danger"}>{saveMessage.text}</Notice>
      ) : null}
    </div>
  );
}
