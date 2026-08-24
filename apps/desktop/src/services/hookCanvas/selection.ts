// Owns pipeline selection, highlighting, and world-space fallback edge geometry.

import type {
  HookCanvasEdge,
  HookCanvasEdgeEndpoints,
  HookCanvasSnapshot,
} from "./types.ts";

export function connectedNodeIds(
  snapshot: HookCanvasSnapshot,
  nodeId: string | null,
): Set<string> {
  const result = new Set<string>();
  if (!nodeId) return result;
  const selectedNode = snapshot.nodes.find((node) => node.id === nodeId);
  if (!selectedNode) return result;
  if (selectedNode.componentId) {
    return new Set(
      snapshot.nodes
        .filter((node) => node.componentId === selectedNode.componentId)
        .map((node) => node.id),
    );
  }
  const adjacency = new Map<string, string[]>();
  const link = (from: string, to: string) => {
    const list = adjacency.get(from) ?? [];
    list.push(to);
    adjacency.set(from, list);
  };
  for (const edge of snapshot.edges) {
    link(edge.sourceNodeId, edge.targetNodeId);
    link(edge.targetNodeId, edge.sourceNodeId);
  }
  const queue = [nodeId];
  let queueIndex = 0;
  result.add(nodeId);
  while (queueIndex < queue.length) {
    const current = queue[queueIndex];
    queueIndex += 1;
    for (const neighbor of adjacency.get(current) ?? []) {
      if (!result.has(neighbor)) {
        result.add(neighbor);
        queue.push(neighbor);
      }
    }
  }
  return result;
}

export function edgeWorldEndpoints(
  snapshot: HookCanvasSnapshot,
  edge: HookCanvasEdge,
): HookCanvasEdgeEndpoints | null {
  if (edge.sourcePoint && edge.targetPoint) {
    return { source: edge.sourcePoint, target: edge.targetPoint };
  }
  const source = snapshot.nodes.find((node) => node.id === edge.sourceNodeId);
  const target = snapshot.nodes.find((node) => node.id === edge.targetNodeId);
  if (!source || !target) return null;
  return {
    source: {
      x: source.x + source.width + (source.minified ? 4 : 6),
      y: source.y + source.height / 2,
    },
    target: {
      x: target.x - (target.minified ? 4 : 6),
      y: target.y + target.height / 2,
    },
  };
}

export function isEdgeHighlighted(edge: HookCanvasEdge, highlighted: Set<string>): boolean {
  return highlighted.has(edge.sourceNodeId) && highlighted.has(edge.targetNodeId);
}

export function retainHookCanvasSelection(
  selectedNodeId: string | null,
  snapshot: HookCanvasSnapshot,
): string | null {
  return selectedNodeId && snapshot.nodes.some((node) => node.id === selectedNodeId)
    ? selectedNodeId
    : null;
}
