import {
  MAX_WORKFLOW_EDGES,
  MAX_WORKFLOW_NODES,
  MAX_WORKFLOW_NODE_FIELDS,
  MAX_WORKFLOW_PARAMETERS,
} from "./shared.ts";
import type { WorkflowGraphLite, WorkflowStudioNode } from "./types.ts";

const hasOwn = (value: object, key: string) => Object.prototype.hasOwnProperty.call(value, key);

export const boundedWithEntries = (value: Record<string, string>): Array<[string, string]> => {
  const entries: Array<[string, string]> = [];
  let fields = 0;
  for (const key in value) {
    if (!hasOwn(value, key)) continue;
    fields += 1;
    if (fields > MAX_WORKFLOW_NODE_FIELDS) throw new RangeError("Workflow node has too many parameters.");
    if (key.trim()) entries.push([key, value[key]]);
  }
  return entries;
};

export const assertWorkflowNodeLimits = (node: Pick<WorkflowStudioNode, "needs" | "with">): void => {
  if (node.needs.length > MAX_WORKFLOW_NODE_FIELDS) throw new RangeError("Workflow node has too many dependencies.");
  let fields = 0;
  for (const key in node.with) {
    if (!hasOwn(node.with, key)) continue;
    fields += 1;
    if (fields > MAX_WORKFLOW_NODE_FIELDS) throw new RangeError("Workflow node has too many parameters.");
  }
};

export const assertWorkflowGraphLimits = (graph: WorkflowGraphLite): void => {
  if (graph.nodes.length > MAX_WORKFLOW_NODES) throw new RangeError("Workflow has too many nodes.");
  let edges = 0;
  let parameters = 0;
  for (const node of graph.nodes) {
    assertWorkflowNodeLimits(node);
    edges += node.needs.length;
    if (edges > MAX_WORKFLOW_EDGES) throw new RangeError("Workflow has too many dependency edges.");
    for (const key in node.with) {
      if (hasOwn(node.with, key)) parameters += 1;
    }
    if (parameters > MAX_WORKFLOW_PARAMETERS) throw new RangeError("Workflow has too many parameters.");
  }
};
