// 只读、无凭据自动回调事件；条件求值和注册/停止由Loom与可信宿主持有。
import type { JsonValue } from "./native-art";
export interface ArtLiveObservation {
    readonly observationId: string;
    readonly sequence: number;
    readonly state: "unknown" | "detected" | "observing" | "stable" | "triggered" | "stale" | "error";
    readonly source: "ui_automation" | "app_adapter" | "vision" | "unknown";
    readonly confidence: "exact" | "high" | "medium" | "low";
    readonly observedAtMs: number;
    readonly stableSinceMs: number | null;
    readonly locator: {
        readonly automationId: string | null;
        readonly name: string | null;
        readonly controlType: string;
        readonly ancestorPath: readonly string[];
        readonly runtimeId: readonly number[] | null;
    } | null;
    readonly value: JsonValue;
    readonly reason: string | null;
}
export interface ArtLiveTriggerEvent {
    readonly sessionId: string;
    readonly epoch: number;
    readonly delivery: {
        readonly deliveryId: string;
        readonly callback: "onLiveTrigger";
        readonly conditionRevision: number;
        readonly observation: ArtLiveObservation;
        readonly sourceDeviceId: string;
        readonly expiresAtMs: number;
    };
}
