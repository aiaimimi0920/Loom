// Owns Hook canvas viewport projection and pointer/keyboard interaction lifetimes.

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
  type WheelEvent as ReactWheelEvent,
} from "react";

import {
  fitViewport,
  viewportLayout,
  type HookCanvasLayout,
  type HookCanvasSnapshot,
  type HookCanvasViewport,
} from "../../services/hookCanvas.ts";

export const SURFACE_WIDTH = 1000;
export const SURFACE_HEIGHT = 620;
export const MIN_SCALE = 0.05;
export const MAX_SCALE = 6;
export const MINIMAP_WIDTH = 200;
export const MINIMAP_HEIGHT = 130;

const ZOOM_STEP = 1.12;
const MINIMAP_PADDING = 8;
const MINIMAP_WORLD_EXPANSION = 1.8;
const DRAG_THRESHOLD = 3;
const KEYBOARD_PAN_PIXELS = 48;

export interface MinimapProjection {
  scale: number;
  originX: number;
  originY: number;
  worldX: number;
  worldY: number;
  toMap: (worldX: number, worldY: number) => { x: number; y: number };
  viewRect: { x: number; y: number; w: number; h: number };
}

interface SurfaceDrag {
  pointerId: number;
  startClientX: number;
  startClientY: number;
  origin: HookCanvasViewport;
  rectWidth: number;
  rectHeight: number;
}

export function createMinimapProjection(
  snapshot: HookCanvasSnapshot,
  viewport: HookCanvasViewport,
): MinimapProjection {
  const bounds = snapshot.bounds;
  const nodeWidth = bounds.width > 0 ? bounds.width : 1;
  const nodeHeight = bounds.height > 0 ? bounds.height : 1;
  const worldWidth = nodeWidth * MINIMAP_WORLD_EXPANSION;
  const worldHeight = nodeHeight * MINIMAP_WORLD_EXPANSION;
  const worldX = bounds.x + nodeWidth / 2 - worldWidth / 2;
  const worldY = bounds.y + nodeHeight / 2 - worldHeight / 2;
  const usableWidth = MINIMAP_WIDTH - MINIMAP_PADDING * 2;
  const usableHeight = MINIMAP_HEIGHT - MINIMAP_PADDING * 2;
  const scale = Math.min(usableWidth / worldWidth, usableHeight / worldHeight);
  const originX = MINIMAP_PADDING + (usableWidth - worldWidth * scale) / 2;
  const originY = MINIMAP_PADDING + (usableHeight - worldHeight * scale) / 2;
  const toMap = (targetX: number, targetY: number) => ({
    x: originX + (targetX - worldX) * scale,
    y: originY + (targetY - worldY) * scale,
  });
  const viewTopLeft = toMap(viewport.offsetX, viewport.offsetY);
  return {
    scale,
    originX,
    originY,
    worldX,
    worldY,
    toMap,
    viewRect: {
      x: viewTopLeft.x,
      y: viewTopLeft.y,
      w: (SURFACE_WIDTH / viewport.scale) * scale,
      h: (SURFACE_HEIGHT / viewport.scale) * scale,
    },
  };
}

export function worldPointFromMinimap(
  projection: MinimapProjection,
  rect: Pick<DOMRect, "left" | "top" | "width" | "height">,
  clientX: number,
  clientY: number,
): { worldX: number; worldY: number } | null {
  if (!(rect.width > 0) || !(rect.height > 0)) return null;
  const mapX = ((clientX - rect.left) / rect.width) * MINIMAP_WIDTH;
  const mapY = ((clientY - rect.top) / rect.height) * MINIMAP_HEIGHT;
  return {
    worldX: projection.worldX + (mapX - projection.originX) / projection.scale,
    worldY: projection.worldY + (mapY - projection.originY) / projection.scale,
  };
}

export function viewportFromSurfaceDrag(
  drag: Omit<SurfaceDrag, "pointerId">,
  clientX: number,
  clientY: number,
): HookCanvasViewport | null {
  if (!(drag.rectWidth > 0) || !(drag.rectHeight > 0)) return null;
  const clientDeltaX = clientX - drag.startClientX;
  const clientDeltaY = clientY - drag.startClientY;
  if (Math.abs(clientDeltaX) <= DRAG_THRESHOLD && Math.abs(clientDeltaY) <= DRAG_THRESHOLD) {
    return null;
  }
  const surfaceDeltaX = (clientDeltaX / drag.rectWidth) * SURFACE_WIDTH;
  const surfaceDeltaY = (clientDeltaY / drag.rectHeight) * SURFACE_HEIGHT;
  return {
    scale: drag.origin.scale,
    offsetX: drag.origin.offsetX - surfaceDeltaX / drag.origin.scale,
    offsetY: drag.origin.offsetY - surfaceDeltaY / drag.origin.scale,
  };
}

interface UseHookCanvasViewportOptions {
  snapshot: HookCanvasSnapshot;
  hasNodes: boolean;
  revision: string;
  selectedWorkflow: string;
  onNodeSelect: (nodeId: string) => void;
}

export interface HookCanvasViewportController {
  viewport: HookCanvasViewport;
  layout: HookCanvasLayout;
  minimap: MinimapProjection;
  showMinimap: boolean;
  isPanning: boolean;
  surfaceRef: RefObject<HTMLDivElement | null>;
  minimapRef: RefObject<HTMLDivElement | null>;
  handleNodeSelect: (nodeId: string) => void;
  handleWheel: (event: ReactWheelEvent<HTMLDivElement>) => void;
  handlePointerDown: (event: ReactPointerEvent<HTMLDivElement>) => void;
  handleMinimapPointerDown: (event: ReactPointerEvent<HTMLDivElement>) => void;
  handleSurfaceKeyDown: (event: ReactKeyboardEvent<HTMLDivElement>) => void;
  zoomToScale: (scale: number) => void;
  toggleMinimap: () => void;
}

export function useHookCanvasViewport({
  snapshot,
  hasNodes,
  revision,
  selectedWorkflow,
  onNodeSelect,
}: UseHookCanvasViewportOptions): HookCanvasViewportController {
  const [viewport, setViewport] = useState<HookCanvasViewport | null>(null);
  const [isPanning, setIsPanning] = useState(false);
  const [showMinimap, setShowMinimap] = useState(true);
  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const minimapRef = useRef<HTMLDivElement | null>(null);
  const surfaceDragRef = useRef<SurfaceDrag | null>(null);
  const minimapPointerIdRef = useRef<number | null>(null);
  const suppressClickRef = useRef(false);

  useEffect(() => {
    setViewport(fitViewport(snapshot, SURFACE_WIDTH, SURFACE_HEIGHT));
    // Snapshot identity intentionally follows the daemon revision contract.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision, selectedWorkflow]);

  const effectiveViewport = viewport ?? fitViewport(snapshot, SURFACE_WIDTH, SURFACE_HEIGHT);
  const viewportRef = useRef(effectiveViewport);
  viewportRef.current = effectiveViewport;
  const layout = useMemo(
    () => viewportLayout(snapshot, effectiveViewport),
    [snapshot, effectiveViewport],
  );
  const minimap = useMemo(
    () => createMinimapProjection(snapshot, effectiveViewport),
    [snapshot.bounds, effectiveViewport],
  );
  const minimapProjectionRef = useRef(minimap);
  minimapProjectionRef.current = minimap;

  const centerViewportOn = useCallback((worldX: number, worldY: number) => {
    setViewport((current) => {
      const base = current ?? viewportRef.current;
      return {
        scale: base.scale,
        offsetX: worldX - SURFACE_WIDTH / 2 / base.scale,
        offsetY: worldY - SURFACE_HEIGHT / 2 / base.scale,
      };
    });
  }, []);

  useEffect(() => {
    if (!hasNodes) return;
    const handleMove = (event: globalThis.PointerEvent) => {
      if (minimapPointerIdRef.current === event.pointerId) {
        const element = minimapRef.current;
        if (!element) return;
        const point = worldPointFromMinimap(
          minimapProjectionRef.current,
          element.getBoundingClientRect(),
          event.clientX,
          event.clientY,
        );
        if (point) centerViewportOn(point.worldX, point.worldY);
        return;
      }
      const drag = surfaceDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      const nextViewport = viewportFromSurfaceDrag(drag, event.clientX, event.clientY);
      if (!nextViewport) return;
      suppressClickRef.current = true;
      setIsPanning(true);
      setViewport(nextViewport);
    };
    const handleUp = (event: globalThis.PointerEvent) => {
      if (surfaceDragRef.current?.pointerId === event.pointerId) surfaceDragRef.current = null;
      if (minimapPointerIdRef.current === event.pointerId) minimapPointerIdRef.current = null;
      if (!surfaceDragRef.current && minimapPointerIdRef.current === null) setIsPanning(false);
    };
    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", handleUp);
    window.addEventListener("pointercancel", handleUp);
    return () => {
      surfaceDragRef.current = null;
      minimapPointerIdRef.current = null;
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", handleUp);
      window.removeEventListener("pointercancel", handleUp);
    };
  }, [centerViewportOn, hasNodes]);

  const zoomToScale = useCallback((nextScaleRaw: number) => {
    const centerX = SURFACE_WIDTH / 2;
    const centerY = SURFACE_HEIGHT / 2;
    setViewport((current) => {
      const base = current ?? viewportRef.current;
      const nextScale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, nextScaleRaw));
      if (nextScale === base.scale) return base;
      const worldX = base.offsetX + centerX / base.scale;
      const worldY = base.offsetY + centerY / base.scale;
      return {
        scale: nextScale,
        offsetX: worldX - centerX / nextScale,
        offsetY: worldY - centerY / nextScale,
      };
    });
  }, []);

  const handleWheel = useCallback((event: ReactWheelEvent<HTMLDivElement>) => {
    if (!hasNodes) return;
    event.preventDefault();
    const surface = surfaceRef.current;
    if (!surface) return;
    const rect = surface.getBoundingClientRect();
    const pointerX = ((event.clientX - rect.left) / rect.width) * SURFACE_WIDTH;
    const pointerY = ((event.clientY - rect.top) / rect.height) * SURFACE_HEIGHT;
    setViewport((current) => {
      const base = current ?? viewportRef.current;
      const factor = event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
      const nextScale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, base.scale * factor));
      if (nextScale === base.scale) return base;
      const worldX = base.offsetX + pointerX / base.scale;
      const worldY = base.offsetY + pointerY / base.scale;
      return {
        scale: nextScale,
        offsetX: worldX - pointerX / nextScale,
        offsetY: worldY - pointerY / nextScale,
      };
    });
  }, [hasNodes]);

  const handlePointerDown = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (!hasNodes || !event.isPrimary || event.button !== 0) return;
    const surface = surfaceRef.current;
    if (!surface) return;
    const rect = surface.getBoundingClientRect();
    if (!(rect.width > 0) || !(rect.height > 0)) return;
    surfaceDragRef.current = {
      pointerId: event.pointerId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      origin: viewportRef.current,
      rectWidth: rect.width,
      rectHeight: rect.height,
    };
    suppressClickRef.current = false;
  }, [hasNodes]);

  const handleMinimapPointerDown = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (!hasNodes || !event.isPrimary || event.button !== 0) return;
    event.stopPropagation();
    const element = minimapRef.current;
    if (!element) return;
    minimapPointerIdRef.current = event.pointerId;
    setIsPanning(true);
    const point = worldPointFromMinimap(
      minimapProjectionRef.current,
      element.getBoundingClientRect(),
      event.clientX,
      event.clientY,
    );
    if (point) centerViewportOn(point.worldX, point.worldY);
  }, [centerViewportOn, hasNodes]);

  const handleSurfaceKeyDown = useCallback((event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (!hasNodes || event.target !== event.currentTarget) return;
    const base = viewportRef.current;
    const worldStep = KEYBOARD_PAN_PIXELS / base.scale;
    let next: HookCanvasViewport | null = null;
    if (event.key === "ArrowLeft") next = { ...base, offsetX: base.offsetX - worldStep };
    else if (event.key === "ArrowRight") next = { ...base, offsetX: base.offsetX + worldStep };
    else if (event.key === "ArrowUp") next = { ...base, offsetY: base.offsetY - worldStep };
    else if (event.key === "ArrowDown") next = { ...base, offsetY: base.offsetY + worldStep };
    else if (event.key === "+" || event.key === "=") zoomToScale(base.scale * ZOOM_STEP);
    else if (event.key === "-") zoomToScale(base.scale / ZOOM_STEP);
    else if (event.key === "Home") next = fitViewport(snapshot, SURFACE_WIDTH, SURFACE_HEIGHT);
    else return;
    event.preventDefault();
    if (next) setViewport(next);
  }, [hasNodes, snapshot, zoomToScale]);

  const handleNodeSelect = useCallback((nodeId: string) => {
    if (suppressClickRef.current) {
      suppressClickRef.current = false;
      return;
    }
    onNodeSelect(nodeId);
  }, [onNodeSelect]);

  return {
    viewport: effectiveViewport,
    layout,
    minimap,
    showMinimap,
    isPanning,
    surfaceRef,
    minimapRef,
    handleNodeSelect,
    handleWheel,
    handlePointerDown,
    handleMinimapPointerDown,
    handleSurfaceKeyDown,
    zoomToScale,
    toggleMinimap: () => setShowMinimap((current) => !current),
  };
}
