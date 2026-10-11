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
  type OtterCanvasLayout,
  type OtterCanvasSnapshot,
} from "../../services/otterCanvas.ts";
import { OtterCanvasNode } from "./OtterCanvasNode.tsx";
import {
  MINIMAP_HEIGHT,
  MINIMAP_WIDTH,
  SURFACE_HEIGHT,
  SURFACE_WIDTH,
  type MinimapProjection,
} from "./useOtterCanvasViewport.ts";

interface OtterCanvasSurfaceProps {
  snapshot: OtterCanvasSnapshot;
  layout: OtterCanvasLayout;
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

export function OtterCanvasSurface({
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
}: OtterCanvasSurfaceProps) {
  return (
    <div
      ref={surfaceRef}
      className={`otter-canvas-surface${hasNodes ? " otter-canvas-surface--interactive" : ""}${isPanning ? " otter-canvas-surface--panning" : ""}`}
      style={{ aspectRatio: `${SURFACE_WIDTH} / ${SURFACE_HEIGHT}` }}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      role="region"
      aria-label="Otter 工作流画布"
      aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown Home + -"
      tabIndex={hasNodes ? 0 : undefined}
    >
      <div className="otter-canvas-grid" aria-hidden="true" />
      <svg
        className="otter-canvas-edges"
        viewBox={`0 0 ${SURFACE_WIDTH} ${SURFACE_HEIGHT}`}
        role="presentation"
      >
        <defs>
          <marker id="otter-canvas-arrow" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
            <polygon points="0 0, 10 3.5, 0 7" className="otter-canvas-arrow" />
          </marker>
          <marker
            id="otter-canvas-arrow-active"
            markerWidth="10"
            markerHeight="7"
            refX="9"
            refY="3.5"
            orient="auto"
          >
            <polygon points="0 0, 10 3.5, 0 7" className="otter-canvas-arrow--active" />
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
              className={active ? "otter-canvas-edge--active" : undefined}
              d={d}
              fill="none"
              markerEnd={active ? "url(#otter-canvas-arrow-active)" : "url(#otter-canvas-arrow)"}
            />
          );
        }) : null}
      </svg>
      <div className="otter-canvas-node-layer">
        {layout.nodes.map((node) => (
          <OtterCanvasNode
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
          className="otter-canvas-minimap"
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
                  className="otter-canvas-minimap__edge"
                  x1={source.x}
                  y1={source.y}
                  x2={target.x}
                  y2={target.y}
                  markerEnd="url(#otter-canvas-minimap-arrow)"
                />
              );
            })}
            <defs>
              <marker id="otter-canvas-minimap-arrow" markerWidth="6" markerHeight="5" refX="5" refY="2.5" orient="auto">
                <polygon points="0 0, 6 2.5, 0 5" className="otter-canvas-minimap__arrow" />
              </marker>
            </defs>
            {snapshot.nodes.map((node) => {
              const point = minimap.toMap(node.x, node.y);
              return (
                <rect
                  key={node.id}
                  className="otter-canvas-minimap__node"
                  x={point.x}
                  y={point.y}
                  width={Math.max(2, node.width * minimap.scale)}
                  height={Math.max(2, node.height * minimap.scale)}
                />
              );
            })}
            <rect
              className="otter-canvas-minimap__view"
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
