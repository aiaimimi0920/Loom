// Desktop DTOs for the independent public loom.wall.v1 contract.
export const WALL_PROTOCOL_VERSION = "loom.wall.v1";
export interface WallRect { x: number; y: number; width: number; height: number }
export type WallRotation = "deg0" | "deg90" | "deg180" | "deg270";
export interface WallDisplayInfo { name: string; canIdentify: boolean }
export interface WallIdentification { requestId: string; remainingMs: number; applied: boolean }
export interface WallEndpoint {
  protocolVersion: typeof WALL_PROTOCOL_VERSION;
  endpointId: string;
  deviceId: string;
  outputId: string;
  pixelSize: { width: number; height: number };
  renderModes: ("image" | "raw_bgra" | "h264" | "surface_v1")[];
  inputCapabilities: ("pointer" | "wheel" | "keyboard" | "text" | "touch" | "pen")[];
  display?: WallDisplayInfo;
  scheduledPresentation?: boolean;
}
export interface WallTile {
  tileId: string;
  endpointId: string;
  rect: WallRect;
  rotation: WallRotation;
}
export interface WallSource { kind: "live" | "image" | "surface"; id: string }
export interface WallPlacement {
  placementId: string;
  source: WallSource;
  rect: WallRect;
  sourceCrop: WallRect;
  zIndex: number;
  interactive: boolean;
}
export interface WallLayout {
  protocolVersion: typeof WALL_PROTOCOL_VERSION;
  wallId: string;
  revision: number;
  bounds: WallRect;
  tiles: WallTile[];
  placements: WallPlacement[];
}
export interface WallEndpointStatus {
  endpoint: WallEndpoint;
  online: boolean;
  appliedRevision: number | null;
  presentation?: WallPresentationReport;
  identification?: WallIdentification;
  scene?: WallSceneReport;
}
export interface WallScene { wallId: string; revision: number; preparedAtMs: number; activateAtMs: number }
export interface WallTiming { clockId: string; serverTimeMs: number; scenes: WallScene[] }
export interface WallSceneReport { revision: number; prepared: boolean; appliedAtMs: number | null; clockUncertaintyMs: number | null }
export type WallPresentationMode = "running" | "frozen" | "black";
export interface WallPresentation {
  wallId: string;
  revision: number;
  mode: Exclude<WallPresentationMode, "running">;
}
export interface WallPresentationReport {
  revision: number;
  outcome: "applied" | "frame_unavailable";
}
export interface WallState {
  protocolVersion: typeof WALL_PROTOCOL_VERSION;
  revision: number;
  endpoints: WallEndpointStatus[];
  layouts: WallLayout[];
  presentations?: WallPresentation[];
  timing?: WallTiming;
}
export interface WallSourceOption {
  source: WallSource;
  label: string;
  status: string;
}
