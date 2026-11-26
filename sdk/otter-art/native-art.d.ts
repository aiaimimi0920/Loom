// Otter single-side execution SDK; neither merges legacy loom.surface.v1 nor declares browser, file, or arbitrary network capabilities.
import type { ArtCanvas, ArtDocument, ArtElement, ArtImage } from "./native-dom";
import type { ArtLiveTriggerEvent } from "./native-live-trigger";
export type { ArtCanvas, ArtDocument, ArtElement, ArtImage } from "./native-dom";
export type { ArtLiveTriggerEvent, ArtLiveObservation } from "./native-live-trigger";

export type JsonValue = null | boolean | number | string
    | readonly JsonValue[] | { readonly [key: string]: JsonValue };

export interface ImageInput {
    readonly kind: "image";
    readonly resourceId: string;
    readonly mimeType: "image/png";
    readonly width: number;
    readonly height: number;
}

export interface PackageResource {
    readonly resourceId: string;
    readonly mimeType: "image/png" | "text/plain" | "application/json" | "application/octet-stream";
    readonly size: number;
    readonly digest: string;
    readonly width: number;
    readonly height: number;
}

export interface ArtSceneNode {
    readonly id: string;
    readonly type: string;
    readonly children?: readonly ArtSceneNode[];
}

export interface ArtSnapshot {
    readonly revision: number;
    readonly scene: ArtSceneNode;
    readonly state: {
        readonly inputs: Readonly<Record<string, ImageInput>>;
        readonly package?: Readonly<Record<string, PackageResource>>;
    };
}

export interface ArtMountContext {
    readonly root: ArtElement;
    readonly snapshot: ArtSnapshot;
    readonly resources: Readonly<Record<string, string>>;
}

export interface ArtUpdateContext {
    readonly snapshot: ArtSnapshot;
    readonly resources: Readonly<Record<string, string>>;
}

export interface ArtModule {
    mount(context: ArtMountContext): void | (() => void | Promise<void>)
        | Promise<void | (() => void | Promise<void>)>;
    update?(context: ArtUpdateContext): void | Promise<void>;
    suspend?(): void | Promise<void>;
    resume?(): void | Promise<void>;
    dispose?(): void | Promise<void>;
    /** Invoked only by explicit trusted-host live-trigger grants; not a click event, grants no user gesture, and does not auto-resume timers. */
    onLiveTrigger?(event: ArtLiveTriggerEvent): void | Promise<void>;
}

export type ArtOutput = { readonly kind: "value"; readonly value: JsonValue }
    | { readonly kind: "canvas"; readonly nodeId: string };

export type ServiceImageOptions = { readonly timeoutMs?: number } & (
    { readonly images?: readonly ArtImage[]; readonly canvases?: never }
    | { readonly canvases?: readonly ArtCanvas[]; readonly images?: never });
export interface ServiceImageResult {
    readonly output: JsonValue;
    readonly image: ArtImage;
}

export interface NativeArtApi {
    define(module: ArtModule): void;
    snapshot(): ArtSnapshot | null;
    resource(resourceId: string): string | undefined;
    image(resourceId: string): ArtImage;
    service(service: string, input: JsonValue, timeoutMs?: number): Promise<JsonValue>;
    /** Accepts only currently installed ArtImage handles as ordered service resourceRefs (up to 4 images); result remains JSON. */
    serviceImages(service: string, input: JsonValue, images: readonly ArtImage[], timeoutMs?: number): Promise<JsonValue>;
    /** Synchronously freezes current Canvas bitmaps and forwards them asynchronously to the image service; ignores CSS sizing and screen capture, returning JSON. */
    serviceCanvases(service: string, input: JsonValue, canvases: readonly ArtCanvas[], timeoutMs?: number): Promise<JsonValue>;
    /** Explicit single-PNG output reusing installed images or call-time Canvas snapshots; the returned handle counts against the session image budget. */
    serviceImage(service: string, input: JsonValue, options?: ServiceImageOptions): Promise<ServiceImageResult>;
    /** Requests user confirmation from the trusted host; not self-approval, accepts no gesture token, and rejects calls while offline. */
    serviceWithConfirmation(service: string, input: JsonValue, options: ServiceImageOptions & { readonly imageOutput: true }): Promise<ServiceImageResult>;
    serviceWithConfirmation(service: string, input: JsonValue, options?: ServiceImageOptions & { readonly imageOutput?: false }): Promise<JsonValue>;
    /** Returning true only confirms the operation was queued; invalid arguments may throw synchronously while permissions and ports are still validated asynchronously by the host. */
    commitResult(outputs: Readonly<Record<string, ArtOutput>>): true;
}

declare global {
    const NeuroSurface: NativeArtApi;
    const document: ArtDocument;
}
