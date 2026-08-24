// Projects canvas edges into workflow dependencies and parameter bindings.

import type { HookCanvasNode, HookCanvasSnapshot } from "./types.ts";

export interface WorkflowEdgeBinding {
  sourceNodeId: string;
  sourcePortId: string;
  targetPortId: string;
}

const WORKFLOW_REFERENCE_TOKEN = /^[A-Za-z0-9_-]+$/;

export function requireWorkflowReferenceToken(value: string, label: string): string {
  if (!WORKFLOW_REFERENCE_TOKEN.test(value)) {
    throw new Error(`Invalid ${label}: expected an ASCII letter, digit, underscore, or hyphen token.`);
  }
  return value;
}

export function workflowNodeUses(node: HookCanvasNode): string {
  return node.artId || (node.kind === "screenshot" ? "sticker" : node.kind);
}

export function workflowOutputReference(binding: WorkflowEdgeBinding): string {
  const sourceNodeId = requireWorkflowReferenceToken(binding.sourceNodeId, "workflow node id");
  const sourcePortId = requireWorkflowReferenceToken(binding.sourcePortId, "source port id");
  return "${{ nodes."
    + sourceNodeId
    + ".outputs."
    + sourcePortId
    + " }}";
}

export function workflowEdgeBindings(
  snapshot: HookCanvasSnapshot,
  rawToWorkflowId: Map<string, string>,
  memberIds: Set<string>,
): Map<string, WorkflowEdgeBinding[]> {
  const bindingsByTarget = new Map<string, WorkflowEdgeBinding[]>();
  for (const edge of snapshot.edges) {
    const sourceNodeId = rawToWorkflowId.get(edge.sourceNodeId);
    const targetNodeId = rawToWorkflowId.get(edge.targetNodeId);
    if (
      !sourceNodeId
      || !targetNodeId
      || !memberIds.has(sourceNodeId)
      || !memberIds.has(targetNodeId)
    ) {
      continue;
    }

    const binding: WorkflowEdgeBinding = {
      sourceNodeId: requireWorkflowReferenceToken(sourceNodeId, "workflow node id"),
      sourcePortId: requireWorkflowReferenceToken(
        edge.sourcePortId?.trim() || "output_image",
        "source port id",
      ),
      targetPortId: requireWorkflowReferenceToken(
        edge.targetPortId?.trim() || "image",
        "target port id",
      ),
    };
    const bindings = bindingsByTarget.get(targetNodeId) ?? [];
    const existing = bindings.find((candidate) => candidate.targetPortId === binding.targetPortId);
    if (existing) {
      if (
        existing.sourceNodeId !== binding.sourceNodeId
        || existing.sourcePortId !== binding.sourcePortId
      ) {
        throw new Error(
          `Workflow node ${targetNodeId} has multiple incoming edges for port ${binding.targetPortId}.`,
        );
      }
      continue;
    }
    bindings.push(binding);
    bindingsByTarget.set(targetNodeId, bindings);
  }
  return bindingsByTarget;
}
