// Inventory references existing owners. Only formal image outputs are selectable.
import { getJson, errorMessage } from "./transport.ts";
import { validWallId, wallRecord } from "./wallValidation.ts";
import type { WallLayout, WallSourceOption } from "./wallTypes.ts";

export function layoutImageSources(layouts: readonly WallLayout[]): WallSourceOption[] {
  const images = new Map<string, WallSourceOption>();
  for (const layout of layouts) for (const placement of layout.placements) {
    if (placement.source.kind !== "image") continue;
    images.set(placement.source.id, { source: placement.source,
      label: `图片 · ${placement.source.id.slice(7, 23)}`, status: `墙面 ${layout.wallId} 引用` });
  }
  return [...images.values()];
}

function entries(value: unknown, key: string): unknown[] {
  const items = wallRecord(value)[key];
  if (!Array.isArray(items) || items.length > 4096) throw new Error("内容目录数量或格式无效");
  return items;
}
export function liveWallSources(value: unknown): WallSourceOption[] {
  return entries(value, "sessions").flatMap((value) => {
    const record = wallRecord(value), session = wallRecord(record.session);
    if (record.closed === true) return [];
    if (!validWallId(session.sessionId)) throw new Error("Live 会话 ID 无效");
    return [{ source: { kind: "live" as const, id: session.sessionId }, label: `Live · ${session.sessionId}`,
      status: record.sourceConnected === true ? "来源已连接" : "来源离线" }];
  });
}
export function surfaceWallSources(value: unknown): WallSourceOption[] {
  const result: WallSourceOption[] = [];
  for (const item of entries(value, "instances")) {
    const instance = wallRecord(item), descriptor = wallRecord(instance.descriptor);
    if (!validWallId(descriptor.instanceId)) throw new Error("Surface 实例 ID 无效");
    result.push({ source: { kind: "surface", id: descriptor.instanceId },
      label: `Art · ${typeof descriptor.artId === "string" ? descriptor.artId.slice(0, 160) : descriptor.instanceId}`,
      status: descriptor.instanceId });
    if (!instance.latestResult) continue;
    const outputs = wallRecord(wallRecord(instance.latestResult).outputs);
    for (const [port, value] of Object.entries(outputs)) {
      const output = wallRecord(value);
      if (output.kind !== "resource") continue;
      const resource = wallRecord(output.resource);
      if (resource.kind !== "image" || typeof resource.resourceId !== "string"
        || !/^sha256:[0-9a-f]{64}$/.test(resource.resourceId)) continue;
      result.push({ source: { kind: "image", id: resource.resourceId },
        label: `图片 · ${descriptor.instanceId} / ${port.slice(0, 160)}`, status: "正式输出" });
    }
  }
  return result;
}
export async function readWallSources(baseUrl: string): Promise<{ sources: WallSourceOption[]; errors: string[] }> {
  const results = await Promise.allSettled([
    getJson<unknown>(baseUrl, "/v1/live/sessions").then(liveWallSources),
    getJson<unknown>(baseUrl, "/v1/surfaces/instances").then(surfaceWallSources),
  ]);
  const sources = new Map<string, WallSourceOption>(), errors: string[] = [];
  results.forEach((result, index) => {
    if (result.status === "rejected") errors.push(`${index === 0 ? "Live" : "Art / 图片"}：${errorMessage(result.reason)}`);
    else result.value.forEach((entry) => sources.set(`${entry.source.kind}:${entry.source.id}`, entry));
  });
  return { sources: [...sources.values()], errors };
}
