// Projects Hook world geometry into fitted and explicitly controlled desktop viewports.

import type {
  HookCanvasEdge,
  HookCanvasEdgeEndpoints,
  HookCanvasLayout,
  HookCanvasLayoutOptions,
  HookCanvasPoint,
  HookCanvasSnapshot,
  HookCanvasViewport,
} from "./types.ts";

function finite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}

function positiveFinite(value: number, fallback: number): number {
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function nonNegativeFinite(value: number, fallback: number): number {
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

function projectPointToLayout(layout: HookCanvasLayout, point: HookCanvasPoint): HookCanvasPoint {
  return {
    x: layout.screenOriginX + (finite(point.x, 0) - layout.worldOriginX) * layout.scale,
    y: layout.screenOriginY + (finite(point.y, 0) - layout.worldOriginY) * layout.scale,
  };
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
    worldOriginX: finite(snapshot.bounds.x, 0),
    worldOriginY: finite(snapshot.bounds.y, 0),
    screenOriginX: offsetX,
    screenOriginY: offsetY,
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
  if (edge.sourcePoint && edge.targetPoint) {
    return {
      source: projectPointToLayout(layout, edge.sourcePoint),
      target: projectPointToLayout(layout, edge.targetPoint),
    };
  }
  const source = layout.nodes.find((node) => node.id === edge.sourceNodeId);
  const target = layout.nodes.find((node) => node.id === edge.targetNodeId);
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

export function viewportLayout(
  snapshot: HookCanvasSnapshot,
  viewport: HookCanvasViewport,
): HookCanvasLayout {
  const scale = positiveFinite(viewport.scale, 1);
  return {
    width: 0,
    height: 0,
    scale,
    worldOriginX: finite(viewport.offsetX, 0),
    worldOriginY: finite(viewport.offsetY, 0),
    screenOriginX: 0,
    screenOriginY: 0,
    nodes: snapshot.nodes.map((node) => ({
      ...node,
      x: (finite(node.x, 0) - finite(viewport.offsetX, 0)) * scale,
      y: (finite(node.y, 0) - finite(viewport.offsetY, 0)) * scale,
      width: positiveFinite(node.width, 1) * scale,
      height: positiveFinite(node.height, 1) * scale,
    })),
  };
}

export function fitViewport(
  snapshot: HookCanvasSnapshot,
  surfaceWidth: number,
  surfaceHeight: number,
  padding = 40,
): HookCanvasViewport {
  const width = positiveFinite(surfaceWidth, 1);
  const height = positiveFinite(surfaceHeight, 1);
  const safePadding = nonNegativeFinite(padding, 0);
  const sourceWidth = positiveFinite(snapshot.bounds.width, 1);
  const sourceHeight = positiveFinite(snapshot.bounds.height, 1);
  const usableWidth = Math.max(1, width - safePadding * 2);
  const usableHeight = Math.max(1, height - safePadding * 2);
  const scale = Math.min(usableWidth / sourceWidth, usableHeight / sourceHeight);
  const worldCenterX = finite(snapshot.bounds.x, 0) + sourceWidth / 2;
  const worldCenterY = finite(snapshot.bounds.y, 0) + sourceHeight / 2;
  return {
    scale,
    offsetX: worldCenterX - width / 2 / scale,
    offsetY: worldCenterY - height / 2 / scale,
  };
}

export function scaleToSliderValue(
  scale: number,
  minScale: number,
  maxScale: number,
  steps = 1000,
): number {
  const minimum = positiveFinite(minScale, 1);
  const maximum = Number.isFinite(maxScale) && maxScale > minimum ? maxScale : minimum;
  if (maximum === minimum) return 0;
  const safeSteps = positiveFinite(steps, 1000);
  const clamped = Math.min(maximum, Math.max(minimum, positiveFinite(scale, minimum)));
  const ratio = Math.log(clamped / minimum) / Math.log(maximum / minimum);
  return Math.round(ratio * safeSteps);
}

export function sliderValueToScale(
  value: number,
  minScale: number,
  maxScale: number,
  steps = 1000,
): number {
  const minimum = positiveFinite(minScale, 1);
  const maximum = Number.isFinite(maxScale) && maxScale > minimum ? maxScale : minimum;
  if (maximum === minimum) return minimum;
  const safeSteps = positiveFinite(steps, 1000);
  const ratio = Math.min(1, Math.max(0, finite(value, 0) / safeSteps));
  return minimum * Math.pow(maximum / minimum, ratio);
}
