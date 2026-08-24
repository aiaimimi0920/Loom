// Resolves preview addresses and derives stable UI presentation from snapshot/runtime state.

import type {
  HookCanvasNode,
  HookCanvasNodePresentation,
  HookCanvasNodePreviewRuntimeState,
  HookCanvasSnapshot,
} from "./types.ts";

export function keepNewestHookCanvasSnapshot(
  previous: HookCanvasSnapshot | null,
  next: HookCanvasSnapshot,
): HookCanvasSnapshot {
  if (previous?.available && !next.available) return previous;
  return previous?.revision === next.revision ? previous : next;
}

export function resolveHookCanvasPreviewUrl(baseUrl: string, node: HookCanvasNode): string | null {
  if (!node.previewAvailable || !node.previewUrl) return null;
  const normalizedBaseUrl = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  return new URL(node.previewUrl, normalizedBaseUrl).toString();
}

// The daemon-relative path keeps its revision cache token for native preview loading.
export function hookCanvasPreviewPath(node: HookCanvasNode): string | null {
  if (!node.previewAvailable || !node.previewUrl) return null;
  return node.previewUrl;
}

export function getHookCanvasNodePresentation(
  node: HookCanvasNode,
  runtime: HookCanvasNodePreviewRuntimeState,
): HookCanvasNodePresentation {
  if (node.kind === "art" && node.status === "error") {
    return {
      showPreviewImage: false,
      placeholderText: "执行失败",
      detailText: node.errorMessage?.trim() || null,
      placeholderTone: "error",
    };
  }
  if (!runtime.hasResolvedPreview || runtime.previewFailed) {
    return {
      showPreviewImage: false,
      placeholderText: "预览不可用",
      detailText: null,
      placeholderTone: "neutral",
    };
  }
  return {
    showPreviewImage: true,
    placeholderText: null,
    detailText: null,
    placeholderTone: "neutral",
  };
}
