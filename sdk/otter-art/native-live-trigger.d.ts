// Read-only, credential-free live-trigger event payload; condition evaluation, registration, and teardown remain owned by Loom and the trusted host.
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
