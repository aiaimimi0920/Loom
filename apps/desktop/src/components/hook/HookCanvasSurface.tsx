// Renders the projected node graph and its decorative minimap.

import type {
  KeyboardEvent,
  PointerEvent,
  RefObject,
  WheelEvent,
} from "react";

import {
  edgeEndpoints,
  edgeWorldEndpoints,
  isEdgeHighlighted,
  type HookCanvasLayout,
  type HookCanvasSnapshot,
} from "../../services/hookCanvas.ts";
import { HookCanvasNode } from "./HookCanvasNode.tsx";
import {
  MINIMAP_HEIGHT,
  MINIMAP_WIDTH,
  SURFACE_HEIGHT,
  SURFACE_WIDTH,
  type MinimapProjection,
} from "./useHookCanvasViewport.ts";

interface HookCanvasSurfaceProps {
  snapshot: HookCanvasSnapshot;
  layout: HookCanvasLayout;
  baseUrl: string;
  hasNodes: boolean;
  isPanning: boolean;
  selectedNodeId: string | null;
  highlighted: Set<string>;
  showMinimap: boolean;
  minimap: MinimapProjection;
  surfaceRef: RefObject<HTMLDivElement | null>;
  minimapRef: RefObject<HTMLDivElement | null>;
  onWheel: (event: WheelEvent<HTMLDivElement>) => void;
  onPointerDown: (event: PointerEvent<HTMLDivElement>) => void;
  onMinimapPointerDown: (event: PointerEvent<HTMLDivElement>) => void;
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  onSelect: (nodeId: string) => void;
}

export function HookCanvasSurface({
  snapshot,
  layout,
  baseUrl,
  hasNodes,
  isPanning,
  selectedNodeId,
  highlighted,
  showMinimap,
  minimap,
  surfaceRef,
  minimapRef,
  onWheel,
  onPointerDown,
  onMinimapPointerDown,
  onKeyDown,
  onSelect,
}: HookCanvasSurfaceProps) {
  return (
    <div
      ref={surfaceRef}
      className={`hook-canvas-surface${hasNodes ? " hook-canvas-surface--interactive" : ""}${isPanning ? " hook-canvas-surface--panning" : ""}`}
      style={{ aspectRatio: `${SURFACE_WIDTH} / ${SURFACE_HEIGHT}` }}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      role="region"
      aria-label="Hook 工作流画布"
      aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown Home + -"
      tabIndex={hasNodes ? 0 : undefined}
    >
      <div className="hook-canvas-grid" aria-hidden="true" />
      <svg
        className="hook-canvas-edges"
        viewBox={`0 0 ${SURFACE_WIDTH} ${SURFACE_HEIGHT}`}
        role="presentation"
      >
        <defs>
          <marker id="hook-canvas-arrow" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
            <polygon points="0 0, 10 3.5, 0 7" className="hook-canvas-arrow" />
          </marker>
          <marker
            id="hook-canvas-arrow-active"
            markerWidth="10"
            markerHeight="7"
            refX="9"
            refY="3.5"
            orient="auto"
          >
            <polygon points="0 0, 10 3.5, 0 7" className="hook-canvas-arrow--active" />
          </marker>
        </defs>
        {layout.nodes.length ? snapshot.edges.map((edge) => {
          const endpoints = edgeEndpoints(layout, edge);
          if (!endpoints) return null;
          const active = selectedNodeId !== null && isEdgeHighlighted(edge, highlighted);
          const dx = Math.max(30, Math.abs(endpoints.target.x - endpoints.source.x) / 2);
          const d = `M ${endpoints.source.x} ${endpoints.source.y} `
            + `C ${endpoints.source.x + dx} ${endpoints.source.y}, `
            + `${endpoints.target.x - dx} ${endpoints.target.y}, `
            + `${endpoints.target.x} ${endpoints.target.y}`;
          return (
            <path
              key={edge.id}
              className={active ? "hook-canvas-edge--active" : undefined}
              d={d}
              fill="none"
              markerEnd={active ? "url(#hook-canvas-arrow-active)" : "url(#hook-canvas-arrow)"}
            />
          );
        }) : null}
      </svg>
      <div className="hook-canvas-node-layer">
        {layout.nodes.map((node) => (
          <HookCanvasNode
            key={node.id}
            node={node}
            baseUrl={baseUrl}
            viewportWidth={SURFACE_WIDTH}
            viewportHeight={SURFACE_HEIGHT}
            selected={selectedNodeId !== null && highlighted.has(node.id)}
            interactive={hasNodes}
            onSelect={onSelect}
          />
        ))}
      </div>
      {hasNodes && showMinimap ? (
        <div
          ref={minimapRef}
          className="hook-canvas-minimap"
          style={{ width: `${MINIMAP_WIDTH}px`, height: `${MINIMAP_HEIGHT}px` }}
          onPointerDown={onMinimapPointerDown}
          role="presentation"
        >
          <svg viewBox={`0 0 ${MINIMAP_WIDTH} ${MINIMAP_HEIGHT}`} width="100%" height="100%">
            {snapshot.edges.map((edge) => {
              const endpoints = edgeWorldEndpoints(snapshot, edge);
              if (!endpoints) return null;
              const source = minimap.toMap(endpoints.source.x, endpoints.source.y);
              const target = minimap.toMap(endpoints.target.x, endpoints.target.y);
              return (
                <line
                  key={edge.id}
                  className="hook-canvas-minimap__edge"
                  x1={source.x}
                  y1={source.y}
                  x2={target.x}
                  y2={target.y}
                  markerEnd="url(#hook-canvas-minimap-arrow)"
                />
              );
            })}
            <defs>
              <marker id="hook-canvas-minimap-arrow" markerWidth="6" markerHeight="5" refX="5" refY="2.5" orient="auto">
                <polygon points="0 0, 6 2.5, 0 5" className="hook-canvas-minimap__arrow" />
              </marker>
            </defs>
            {snapshot.nodes.map((node) => {
              const point = minimap.toMap(node.x, node.y);
              return (
                <rect
                  key={node.id}
                  className="hook-canvas-minimap__node"
                  x={point.x}
                  y={point.y}
                  width={Math.max(2, node.width * minimap.scale)}
                  height={Math.max(2, node.height * minimap.scale)}
                />
              );
            })}
            <rect
              className="hook-canvas-minimap__view"
              x={minimap.viewRect.x}
              y={minimap.viewRect.y}
              width={minimap.viewRect.w}
              height={minimap.viewRect.h}
            />
          </svg>
        </div>
      ) : null}
    </div>
  );
}
