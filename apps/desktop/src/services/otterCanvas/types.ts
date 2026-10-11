// Shared Otter canvas transport, workflow, presentation, and layout contracts.

import type { ConnectionState, LoomToolDefinition } from "../loomApi.ts";

export type OtterCanvasNodeKind = "screenshot" | "art" | "unknown";
export type OtterCanvasNodeStatus = "ready" | "processing" | "error" | "unknown";

export interface OtterCanvasBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface OtterCanvasCrop {
  // Ratios relative to the node box let the frontend reproduce Otter's crop at any zoom.
  imageWidthRatio: number;
  imageHeightRatio: number;
  offsetXRatio: number;
  offsetYRatio: number;
}

export interface OtterCanvasNode {
  id: string;
  // Connected-component identity in Otter world coordinates.
  componentId?: string | null;
  // Stable YAML-safe workflow metadata emitted by the daemon.
  workflowNodeId?: string | null;
  upstreamWorkflowNodeIds?: string[] | null;
  kind: OtterCanvasNodeKind;
  label: string;
  artId: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  previewAvailable: boolean;
  previewUrl: string | null;
  status: OtterCanvasNodeStatus;
  errorMessage?: string | null;
  minified: boolean;
  crop: OtterCanvasCrop | null;
  opacity: number;
  params?: Record<string, unknown> | null;
  resultCandidates?: OtterCanvasResultCandidate[] | null;
  selectedResultIndex?: number | null;
}

export interface OtterCanvasResultCandidate {
  index: number;
  title?: string | null;
  imageUrl: string;
  thumbnailUrl?: string | null;
  sourcePageUrl?: string | null;
  width?: number | null;
  height?: number | null;
}

export interface OtterCanvasEdge {
  id: string;
  sourceNodeId: string;
  sourcePortId: string | null;
  sourcePoint?: OtterCanvasPoint | null;
  targetNodeId: string;
  targetPortId: string | null;
  targetPoint?: OtterCanvasPoint | null;
}

export interface OtterCanvasSnapshot {
  available: boolean;
  revision: string;
  updatedAt: string | null;
  workflowId: string | null;
  bounds: OtterCanvasBounds;
  nodes: OtterCanvasNode[];
  edges: OtterCanvasEdge[];
  warnings: string[];
}

export interface OtterCanvasLayoutOptions {
  width: number;
  height: number;
  padding: number;
  minimumNodeSize: number;
}

export interface OtterCanvasLayoutNode extends OtterCanvasNode {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface OtterCanvasLayout {
  width: number;
  height: number;
  scale: number;
  worldOriginX: number;
  worldOriginY: number;
  screenOriginX: number;
  screenOriginY: number;
  nodes: OtterCanvasLayoutNode[];
}

export interface OtterCanvasPoint {
  x: number;
  y: number;
}

export interface OtterCanvasEdgeEndpoints {
  source: OtterCanvasPoint;
  target: OtterCanvasPoint;
}

export interface OtterCanvasNodePreviewRuntimeState {
  hasResolvedPreview: boolean;
  previewFailed: boolean;
}

export interface OtterCanvasNodePresentation {
  showPreviewImage: boolean;
  placeholderText: string | null;
  detailText: string | null;
  placeholderTone: "neutral" | "error";
}

export interface OtterCanvasRefreshTriggerInput {
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

export interface OtterWorkflowInstantiationGraph {
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

export interface OtterCanvasViewport {
  scale: number;
  offsetX: number;
  offsetY: number;
}
