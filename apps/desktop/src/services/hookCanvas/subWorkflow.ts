// Serializes a selected connected component as a standalone workflow YAML document.

import type { HookCanvasNode, HookCanvasSnapshot } from "./types.ts";
import {
  requireWorkflowReferenceToken,
  workflowEdgeBindings,
  workflowNodeUses,
  workflowOutputReference,
} from "./workflowBindings.ts";

function workflowNodeId(node: HookCanvasNode): string {
  const base = (node.artId || node.id || "node").toString();
  const safe = base.replace(/[^A-Za-z0-9_-]/g, "-").replace(/-+/g, "-").replace(/^-|-$/g, "");
  return safe || "node";
}

function yamlSingleQuoted(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

function yamlMappingKey(value: string): string {
  return /^[A-Za-z0-9_-]+$/.test(value) ? value : yamlSingleQuoted(value);
}

export function buildSubWorkflowYaml(
  snapshot: HookCanvasSnapshot,
  nodeIds: Set<string>,
  workflowName: string,
): string {
  const members = snapshot.nodes.filter((node) => nodeIds.has(node.id));
  const safeName = workflowName.trim() || "hook-pipeline";
  const lines: string[] = [`name: ${yamlSingleQuoted(safeName)}`, "nodes:"];
  if (!members.length) {
    lines.push("  []");
    return `${lines.join("\n")}\n`;
  }

  const canUseDaemonMetadata = members.every(
    (node) => typeof node.workflowNodeId === "string"
      && node.workflowNodeId.length > 0
      && Array.isArray(node.upstreamWorkflowNodeIds),
  );

  const idMap = new Map<string, string>();
  if (canUseDaemonMetadata) {
    const usedIds = new Set<string>();
    for (const node of members) {
      const workflowNodeId = requireWorkflowReferenceToken(
        node.workflowNodeId as string,
        "workflow node id",
      );
      if (usedIds.has(workflowNodeId)) {
        throw new Error(`Duplicate workflow node id: ${workflowNodeId}`);
      }
      usedIds.add(workflowNodeId);
      idMap.set(node.id, workflowNodeId);
    }
  } else {
    const usedIds = new Set<string>();
    for (const node of members) {
      let candidate = workflowNodeId(node);
      let suffix = 2;
      while (usedIds.has(candidate)) {
        candidate = `${workflowNodeId(node)}-${suffix}`;
        suffix += 1;
      }
      usedIds.add(candidate);
      idMap.set(node.id, candidate);
    }
  }
  const selectedWorkflowIds = new Set(idMap.values());
  const edgeBindingsByTarget = workflowEdgeBindings(snapshot, idMap, selectedWorkflowIds);

  for (const node of members) {
    const wid = idMap.get(node.id) as string;
    lines.push(`  - id: ${wid}`);
    lines.push(`    uses: ${yamlSingleQuoted(workflowNodeUses(node))}`);
    const edgeBindings = edgeBindingsByTarget.get(wid) ?? [];
    const needs = [...new Set([
      ...(canUseDaemonMetadata ? node.upstreamWorkflowNodeIds ?? [] : [])
        .filter((upstreamId) => selectedWorkflowIds.has(upstreamId)),
      ...edgeBindings.map((binding) => binding.sourceNodeId),
    ])];
    if (needs.length) lines.push(`    needs: [${needs.join(", ")}]`);
    if (edgeBindings.length) {
      lines.push("    with:");
      for (const binding of edgeBindings) {
        lines.push(
          `      ${yamlMappingKey(binding.targetPortId)}: ${yamlSingleQuoted(workflowOutputReference(binding))}`,
        );
      }
    }
  }
  return `${lines.join("\n")}\n`;
}
