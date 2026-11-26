// Workflow selection, persistence actions, and viewport controls for the canvas.

import { Button, Field, Notice, Range, Select } from "@neuro/ui";
import { useLayoutEffect, useRef } from "react";
import { useUiText } from "../../i18n/index.ts";
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
  saveLabel?: string;
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
  saveLabel,
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
  const { t, tf } = useUiText();
  const resolvedSaveLabel = saveLabel ?? t("otter.canvas.saveWorkflow");
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
        <Field label={t("otter.toolbar.workflowLabel")}>
          <Select
            ref={workflowSelect}
            value={selectedWorkflow}
            onChange={(event) => {
              focusAfterDelete.current = false;
              onWorkflowChange(event.target.value);
            }}
          >
            <option value={LIVE_WORKFLOW_ID}>{t("otter.toolbar.liveSync")}</option>
            {workflows.map((workflow) => (
              <option key={workflow.id} value={workflow.id}>
                {tf("otter.toolbar.workflowOption", { name: workflow.name, count: workflow.nodeCount })}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {!isLive ? (
        <div className="otter-canvas-workflow-actions">
          <Button variant="quiet" onClick={onRename} disabled={workflowBusy}>
            {t("common.rename")}
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              focusAfterDelete.current = true;
              onDelete();
            }}
            disabled={workflowBusy}
          >
            {t("common.delete")}
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
              {resolvedSaveLabel}
            </Button>
          ) : null}
          <div className="otter-canvas-zoom">
            <Field label={t("otter.toolbar.zoomLabel")}>
              <Range
                className="otter-canvas-zoom__slider"
                min={0}
                max={1000}
                step={1}
                value={scaleToSliderValue(scale, MIN_SCALE, MAX_SCALE)}
                onChange={(event) =>
                  onScaleChange(sliderValueToScale(Number(event.target.value), MIN_SCALE, MAX_SCALE))
                }
                aria-label={t("otter.toolbar.zoomAria")}
                aria-valuetext={`${Math.round(scale * 100)}%`}
              />
            </Field>
            <span className="otter-canvas-zoom__value">{Math.round(scale * 100)}%</span>
          </div>
          <Button onClick={onToggleMinimap} aria-pressed={showMinimap}>
            {t("otter.toolbar.minimap")}
          </Button>
        </div>
      ) : null}
      {isLive && hasNodes && saveMessage ? (
        <Notice tone={saveMessage.ok ? "success" : "danger"}>{saveMessage.text}</Notice>
      ) : null}
    </div>
  );
}
