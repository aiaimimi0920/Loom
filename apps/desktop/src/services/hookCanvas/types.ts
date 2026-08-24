// Shared Hook canvas transport, workflow, presentation, and layout contracts.

import type { ConnectionState, LoomToolDefinition } from "../loomApi.ts";

export type HookCanvasNodeKind = "screenshot" | "art" | "unknown";
export type HookCanvasNodeStatus = "ready" | "processing" | "error" | "unknown";

export interface HookCanvasBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HookCanvasCrop {
  // Ratios relative to the node box let the frontend reproduce Hook's crop at any zoom.
  imageWidthRatio: number;
  imageHeightRatio: number;
  offsetXRatio: number;
  offsetYRatio: number;
}

export interface HookCanvasNode {
  id: string;
  // Connected-component identity in Hook world coordinates.
  componentId?: string | null;
  // Stable YAML-safe workflow metadata emitted by the daemon.
  workflowNodeId?: string | null;
  upstreamWorkflowNodeIds?: string[] | null;
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
  errorMessage?: string | null;
  minified: boolean;
  crop: HookCanvasCrop | null;
  opacity: number;
  params?: Record<string, unknown> | null;
  resultCandidates?: HookCanvasResultCandidate[] | null;
  selectedResultIndex?: number | null;
}

export interface HookCanvasResultCandidate {
  index: number;
  title?: string | null;
  imageUrl: string;
  thumbnailUrl?: string | null;
  sourcePageUrl?: string | null;
  width?: number | null;
  height?: number | null;
}

export interface HookCanvasEdge {
  id: string;
  sourceNodeId: string;
  sourcePortId: string | null;
  sourcePoint?: HookCanvasPoint | null;
  targetNodeId: string;
  targetPortId: string | null;
  targetPoint?: HookCanvasPoint | null;
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
  worldOriginX: number;
  worldOriginY: number;
  screenOriginX: number;
  screenOriginY: number;
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

export interface HookCanvasNodePreviewRuntimeState {
  hasResolvedPreview: boolean;
  previewFailed: boolean;
}

export interface HookCanvasNodePresentation {
  showPreviewImage: boolean;
  placeholderText: string | null;
  detailText: string | null;
  placeholderTone: "neutral" | "error";
}

export interface HookCanvasRefreshTriggerInput {
  connectionState: ConnectionState;
  baseUrl: string;
  refreshVersion: number;
}

export interface CanvasWorkflowSummary {
  id: string;
  name: string;
  nodeCount: number;
  savedAt: number;
}

export interface HookWorkflowInstantiationGraph {
  nodes: Array<Record<string, unknown>>;
  edges: Array<Record<string, unknown>>;
}

export interface CanvasWorkflowPort {
  nodeId: string;
  portId: string;
  label: string;
  semanticTarget?: string;
}

export interface CanvasWorkflowInterface {
  inputs: CanvasWorkflowPort[];
  outputs: CanvasWorkflowPort[];
}

// One Art-node parameter that can be exposed as a workflow input.
export interface ExposableParam {
  workflowNodeId: string;
  target: string;
  label: string;
  uiType: string;
  executionType: string;
  widget?: string;
  dataType?: string;
  defaultValue?: unknown;
  min?: number;
  max?: number;
  step?: number;
  options?: unknown[];
  multiline?: boolean;
  group?: string;
  required?: boolean;
  secret?: boolean;
}

export interface WorkflowArtBundle {
  yaml: string;
  tool: LoomToolDefinition;
}

export interface HookCanvasViewport {
  scale: number;
  offsetX: number;
  offsetY: number;
}
