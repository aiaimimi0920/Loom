// Pure view-model helpers shared by the Hook canvas orchestrator and panels.

import type {
  ExposableParam,
  HookCanvasNode,
  HookCanvasSnapshot,
} from "../../services/hookCanvas.ts";
import type { LoomToolDefinition } from "../../services/loomApi.ts";
import {
  mapParamExecutionType,
  mapParamUiType,
  normalizeToolParams,
  toolDefinitionsByIdentity,
  type ToolParamDefinition,
} from "../../services/workflowStudio.ts";

export const LIVE_WORKFLOW_ID = "__live__";

export const NODE_KIND_LABELS: Record<string, string> = {
  screenshot: "截图节点",
  art: "Art 节点",
  unknown: "未知节点",
};

export interface StatusMessage {
  ok: boolean;
  text: string;
}

export interface ParamRow {
  key: string;
  target: string;
  label: string;
  currentValue: string;
  param: ToolParamDefinition;
}

export interface NodeParamGroup {
  nodeId: string;
  workflowNodeId: string;
  label: string;
  rows: ParamRow[];
}

export interface WorkflowArtCreationRequest {
  workflowId: string;
  workflowName: string;
  tool: LoomToolDefinition;
}

function paramValueFromNode(
  node: HookCanvasNode,
  target: string,
  param: ToolParamDefinition,
): string {
  const live = node.params?.[target];
  const value = live !== undefined && live !== null ? live : param.default;
  if (value === undefined || value === null) return "";
  return typeof value === "string" ? value : String(value);
}

export function buildNodeParamGroups(
  snapshot: HookCanvasSnapshot,
  tools: LoomToolDefinition[],
): NodeParamGroup[] {
  const toolMap = toolDefinitionsByIdentity(tools);
  const groups: NodeParamGroup[] = [];
  for (const node of snapshot.nodes) {
    if (!node.artId) continue;
    const tool = toolMap.get(node.artId);
    if (!tool) continue;
    const workflowNodeId = node.workflowNodeId || node.id;
    const rows: ParamRow[] = [];
    for (const param of normalizeToolParams(tool)) {
      const target = param.id || param.name;
      if (!target || param.disabled) continue;
      rows.push({
        key: `${workflowNodeId}::${target}`,
        target,
        label: param.label || param.name || target,
        currentValue: paramValueFromNode(node, target, param),
        param,
      });
    }
    if (rows.length) {
      groups.push({ nodeId: node.id, workflowNodeId, label: node.label || workflowNodeId, rows });
    }
  }
  return groups;
}

export function buildExposableParams(groups: NodeParamGroup[]): ExposableParam[] {
  return groups.flatMap((group) =>
    group.rows.map((row) => {
      const uiType = mapParamUiType(row.param);
      return {
        workflowNodeId: group.workflowNodeId,
        target: row.target,
        label: `${group.label} / ${row.label}`,
        uiType,
        executionType: mapParamExecutionType(row.param, uiType),
        widget: row.param.widget,
        dataType: row.param.dataType || row.param.data_type,
        defaultValue: row.param.default,
        min: row.param.min,
        max: row.param.max,
        step: row.param.step,
        options: row.param.options,
        multiline: row.param.multiline,
        group: row.param.group,
        required: row.param.required,
        secret: row.param.secret,
      };
    }),
  );
}
