// Converts frozen snapshots to desktop graphs and infers their external image interface.

import { resolveHookCanvasPreviewUrl } from "./presentation.ts";
import type {
  CanvasWorkflowInterface,
  CanvasWorkflowPort,
  HookCanvasSnapshot,
  HookWorkflowInstantiationGraph,
} from "./types.ts";

export function buildHookWorkflowInstantiationGraph(
  snapshot: HookCanvasSnapshot,
  baseUrl: string,
): HookWorkflowInstantiationGraph {
  const nodes = snapshot.nodes.map((node) => {
    const previewUrl = resolveHookCanvasPreviewUrl(baseUrl, node);
    return {
      id: node.id,
      type: node.kind === "art" ? "artNode" : "sticker",
      position: { x: node.x, y: node.y },
      measured: { width: node.width, height: node.height },
      data: {
        ...(node.artId ? { artId: node.artId } : {}),
        label: node.label,
        w: node.width,
        h: node.height,
        params: node.params ?? {},
        ...(previewUrl ? { src: previewUrl, previewSrc: previewUrl } : {}),
        minified: node.minified,
        opacityNormal: node.opacity,
        opacityMini: node.opacity,
      },
    };
  });
  const edges = snapshot.edges.map((edge) => ({
    id: edge.id,
    source: edge.sourceNodeId,
    target: edge.targetNodeId,
    sourceHandle: edge.sourcePortId ?? "output_image",
    targetHandle: edge.targetPortId ?? "image",
  }));
  return { nodes, edges };
}

export function inferCanvasWorkflowInterface(snapshot: HookCanvasSnapshot): CanvasWorkflowInterface {
  const incoming = new Set<string>();
  const outgoing = new Set<string>();
  const firstTargetPortBySource = new Map<string, string | undefined>();
  for (const edge of snapshot.edges) {
    incoming.add(edge.targetNodeId);
    outgoing.add(edge.sourceNodeId);
    if (!firstTargetPortBySource.has(edge.sourceNodeId)) {
      firstTargetPortBySource.set(edge.sourceNodeId, edge.targetPortId?.trim() || undefined);
    }
  }
  const inputs: Array<CanvasWorkflowPort & { sourceOrder: number }> = [];
  const outputs: CanvasWorkflowPort[] = [];
  for (const [sourceOrder, node] of snapshot.nodes.entries()) {
    if (!incoming.has(node.id)) {
      const semanticTarget = firstTargetPortBySource.get(node.id);
      inputs.push({
        nodeId: node.id,
        portId: "image",
        label: node.label || "输入图像",
        semanticTarget,
        sourceOrder,
      });
    }
    if (!outgoing.has(node.id)) {
      outputs.push({ nodeId: node.id, portId: "output_image", label: node.label || "输出图像" });
    }
  }
  inputs.sort((left, right) => {
    const semanticRank = (target?: string) => {
      const normalized = target?.trim().toLowerCase() || "";
      if (["input", "image", "input_image", "source", "source_image"].includes(normalized)) return 0;
      if (["reference", "reference_image", "ref", "style", "style_image"].includes(normalized)) return 2;
      return 1;
    };
    return semanticRank(left.semanticTarget) - semanticRank(right.semanticTarget)
      || left.sourceOrder - right.sourceOrder;
  });
  return {
    inputs: inputs.map((port) => ({
      nodeId: port.nodeId,
      portId: port.portId,
      label: port.label,
      semanticTarget: port.semanticTarget,
    })),
    outputs,
  };
}
