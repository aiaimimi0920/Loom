import { getLoomDaemonJson } from "./loomApi.ts";

export type HookCanvasNodeKind = "screenshot" | "art" | "unknown";
export type HookCanvasNodeStatus = "ready" | "processing" | "error" | "unknown";

export interface HookCanvasBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HookCanvasNode {
  id: string;
  kind: HookCanvasNodeKind;
  label: string;
  artId: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  previewAvailable: boolean;
  previewUrl: string | null;
  status: HookCanvasNodeStatus;
}

export interface HookCanvasEdge {
  id: string;
  sourceNodeId: string;
  sourcePortId: string | null;
  targetNodeId: string;
  targetPortId: string | null;
}

export interface HookCanvasSnapshot {
  available: boolean;
  revision: string;
  updatedAt: string | null;
  workflowId: string | null;
  bounds: HookCanvasBounds;
  nodes: HookCanvasNode[];
  edges: HookCanvasEdge[];
  warnings: string[];
}

export interface HookCanvasLayoutOptions {
  width: number;
  height: number;
  padding: number;
  minimumNodeSize: number;
}

export interface HookCanvasLayoutNode extends HookCanvasNode {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HookCanvasLayout {
  width: number;
  height: number;
  scale: number;
  nodes: HookCanvasLayoutNode[];
}

export interface HookCanvasPoint {
  x: number;
  y: number;
}

export interface HookCanvasEdgeEndpoints {
  source: HookCanvasPoint;
  target: HookCanvasPoint;
}

export async function readHookCanvasSnapshot(baseUrl: string): Promise<HookCanvasSnapshot> {
  return await getLoomDaemonJson<HookCanvasSnapshot>(baseUrl, "/v1/hook-bridge/canvas");
}

export function keepNewestHookCanvasSnapshot(
  previous: HookCanvasSnapshot | null,
  next: HookCanvasSnapshot,
): HookCanvasSnapshot {
  if (previous?.available && !next.available) {
    return previous;
  }
  return previous?.revision === next.revision ? previous : next;
}

export function resolveHookCanvasPreviewUrl(
  baseUrl: string,
  node: HookCanvasNode,
): string | null {
  if (!node.previewAvailable || !node.previewUrl) {
    return null;
  }
  const normalizedBaseUrl = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  return new URL(node.previewUrl, normalizedBaseUrl).toString();
}

export function retainHookCanvasSelection(
  selectedNodeId: string | null,
  snapshot: HookCanvasSnapshot,
): string | null {
  return selectedNodeId && snapshot.nodes.some((node) => node.id === selectedNodeId)
    ? selectedNodeId
    : null;
}

export function fitHookCanvas(
  snapshot: HookCanvasSnapshot,
  options: HookCanvasLayoutOptions,
): HookCanvasLayout {
  const width = positiveFinite(options.width, 1);
  const height = positiveFinite(options.height, 1);
  const padding = nonNegativeFinite(options.padding, 0);
  const minimumNodeSize = positiveFinite(options.minimumNodeSize, 1);
  const usableWidth = Math.max(1, width - padding * 2);
  const usableHeight = Math.max(1, height - padding * 2);
  const sourceWidth = positiveFinite(snapshot.bounds.width, 1);
  const sourceHeight = positiveFinite(snapshot.bounds.height, 1);
  const scale = Math.min(usableWidth / sourceWidth, usableHeight / sourceHeight);
  const contentWidth = sourceWidth * scale;
  const contentHeight = sourceHeight * scale;
  const offsetX = (width - contentWidth) / 2;
  const offsetY = (height - contentHeight) / 2;

  return {
    width,
    height,
    scale,
    nodes: snapshot.nodes.map((node) => ({
      ...node,
      x: offsetX + (finite(node.x, 0) - finite(snapshot.bounds.x, 0)) * scale,
      y: offsetY + (finite(node.y, 0) - finite(snapshot.bounds.y, 0)) * scale,
      width: Math.max(minimumNodeSize, positiveFinite(node.width, minimumNodeSize) * scale),
      height: Math.max(minimumNodeSize, positiveFinite(node.height, minimumNodeSize) * scale),
    })),
  };
}

export function edgeEndpoints(
  layout: HookCanvasLayout,
  edge: HookCanvasEdge,
): HookCanvasEdgeEndpoints | null {
  const source = layout.nodes.find((node) => node.id === edge.sourceNodeId);
  const target = layout.nodes.find((node) => node.id === edge.targetNodeId);
  if (!source || !target) {
    return null;
  }
  return {
    source: { x: source.x + source.width / 2, y: source.y + source.height / 2 },
    target: { x: target.x + target.width / 2, y: target.y + target.height / 2 },
  };
}

function finite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}

function positiveFinite(value: number, fallback: number): number {
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function nonNegativeFinite(value: number, fallback: number): number {
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}
