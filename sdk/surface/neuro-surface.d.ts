export {};

declare global {
    const NeuroSurface: NeuroSurfaceApi;
}

export type SurfaceRuntimeKind = "declarative" | "javascript" | "shader" | "loom_remote";
export type SurfaceEventClass = "discrete" | "continuous" | "commit" | "local";

export interface SurfaceSize {
    width: number;
    height: number;
}

export interface SurfaceViewDefinition {
    id: string;
    label: string;
    fullSize: SurfaceSize;
}

export interface SurfaceNode {
    id: string;
    type: string;
    props?: unknown;
    layout?: unknown;
    style?: unknown;
    accessibility?: unknown;
    events?: Record<string, string>;
    children?: SurfaceNode[];
}

export interface SurfaceResourceDescriptor {
    resourceId: string;
    kind: "image" | "audio" | "video" | "file" | "binary";
    mime: string;
    size: number;
    width?: number;
    height?: number;
}

export interface SurfaceResourceLease {
    leaseId: string;
    resource: SurfaceResourceDescriptor;
    transport: {
        kind: "shared_memory" | "loom_resource" | "stream";
        handle?: string;
        path?: string;
        streamId?: string;
    };
    expiresAtMs: number;
}

export interface SurfaceSnapshot {
    protocolVersion: "loom.surface.v1";
    instanceId: string;
    attachmentId: string;
    artId: string;
    artVersion: string;
    revision: number;
    runtime?: SurfaceRuntimeKind;
    entryResourceId?: string;
    viewId?: string;
    scene: SurfaceNode;
    authoritativeState?: unknown;
    resources?: SurfaceResourceDescriptor[];
    resourceLeases?: SurfaceResourceLease[];
}

export interface SurfaceEventRequest<TPayload = unknown> {
    nodeId: string;
    event: string;
    action: string;
    class?: SurfaceEventClass;
    payload?: TPayload;
}

export type SurfaceResourceMap = Readonly<Record<string, string>>;

export interface SurfaceMountContext {
    root: HTMLElement;
    snapshot: SurfaceSnapshot;
    resources: SurfaceResourceMap;
    emit: NeuroSurfaceApi["emit"];
}

export interface SurfaceUpdateContext {
    snapshot: SurfaceSnapshot;
    resources: SurfaceResourceMap;
}

export interface SurfaceModule {
    mount(context: SurfaceMountContext):
        | void
        | (() => void | Promise<void>)
        | Promise<void | (() => void | Promise<void>)>;
    update?(context: SurfaceUpdateContext): void | Promise<void>;
    suspend?(): void | Promise<void>;
    resume?(): void | Promise<void>;
    dispose?(): void | Promise<void>;
}

export interface NeuroSurfaceApi {
    define(module: SurfaceModule): void;
    emit<TPayload = unknown>(event: SurfaceEventRequest<TPayload>): boolean;
    snapshot(): SurfaceSnapshot | null;
    resource(resourceId: string): string | undefined;
}
