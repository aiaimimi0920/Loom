// Otter单侧执行SDK；不合并旧loom.surface.v1，也不声明浏览器/文件/任意网络权限。
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
    /** 只由可信宿主显式自动授权触发；不是click，不授gesture或自动恢复timer。 */
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
    /** 仅当前安装的image句柄，按数组顺序成为服务resourceRefs；最多4图，结果仍为JSON。 */
    serviceImages(service: string, input: JsonValue, images: readonly ArtImage[], timeoutMs?: number): Promise<JsonValue>;
    /** 同步冻结当前Canvas位图，异步交原图片服务；不使用CSS尺寸或屏幕截图，结果仍为JSON。 */
    serviceCanvases(service: string, input: JsonValue, canvases: readonly ArtCanvas[], timeoutMs?: number): Promise<JsonValue>;
    /** 显式单PNG结果；复用当前安装图片或调用时Canvas输入，结果句柄占同一会话图片预算。 */
    serviceImage(service: string, input: JsonValue, options?: ServiceImageOptions): Promise<ServiceImageResult>;
    /** 请求可信宿主询问用户；这不是批准，不接受gesture token，离线仍拒绝服务。 */
    serviceWithConfirmation(service: string, input: JsonValue, options: ServiceImageOptions & { readonly imageOutput: true }): Promise<ServiceImageResult>;
    serviceWithConfirmation(service: string, input: JsonValue, options?: ServiceImageOptions & { readonly imageOutput?: false }): Promise<JsonValue>;
    /** true仅表示本operation已排队；参数可能立即抛错，权限/端口仍由宿主后验，不等于接纳或保存成功。 */
    commitResult(outputs: Readonly<Record<string, ArtOutput>>): true;
}

declare global {
    const NeuroSurface: NativeArtApi;
    const document: ArtDocument;
}
