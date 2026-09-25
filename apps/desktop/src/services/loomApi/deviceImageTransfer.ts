import { uploadWallImage } from "./wallImages.ts";
import { readWallState, saveWallLayout } from "./walls.ts";
import { parseWallLayout } from "./wallValidation.ts";
import { WALL_PROTOCOL_VERSION, type WallLayout, type WallState } from "./wallTypes.ts";

export interface DeviceImageReceipt { endpointId: string; wallId: string; revision: number }
const PREFIX = "device-image:";

// A quick send may replace its own single-image layout, never a user's composed wall.
export function imageTarget(state: WallState, endpointId: string) {
  const target = state.endpoints.find(({ endpoint }) => endpoint.endpointId === endpointId);
  if (!target?.online) throw new Error("目标终端离线，请先在接收端启动 Hook 终端");
  if (!target.endpoint.renderModes.includes("image")) throw new Error("目标终端不支持图片");
  const wall = state.layouts.find((layout) => layout.tiles.some((tile) => tile.endpointId === endpointId));
  if (wall && (!wall.wallId.startsWith(PREFIX) || wall.tiles.length !== 1
    || wall.tiles[0].tileId !== "output" || wall.placements.length !== 1
    || wall.placements[0].placementId !== "image" || wall.placements[0].source.kind !== "image")) {
    throw new Error("目标已用于屏幕墙，请先在屏幕墙中解除分配，避免覆盖现有内容");
  }
  if (wall && state.presentations?.some((item) => item.wallId === wall.wallId)) {
    throw new Error("目标处于冻结或黑场，请先在屏幕墙中恢复显示");
  }
  if (!wall && state.layouts.length >= 64) throw new Error("墙面数量已达上限，请先清理不再使用的墙面");
  return { target, wall };
}

export function deviceImageLayout(state: WallState, endpointId: string, resourceId: string,
  imageSize: { width: number; height: number }): WallLayout {
  const { target, wall } = imageTarget(state, endpointId);
  if (![imageSize.width, imageSize.height].every((n) => Number.isSafeInteger(n) && n > 0 && n <= 16384)
    || imageSize.width * imageSize.height > 16_777_216) throw new Error("图片尺寸超过终端解码限制");
  const { width, height } = target.endpoint.pixelSize;
  const scale = Math.min(width / imageSize.width, height / imageSize.height);
  const w = imageSize.width * scale, h = imageSize.height * scale;
  const bounds = { x: 0, y: 0, width, height };
  return parseWallLayout({ protocolVersion: WALL_PROTOCOL_VERSION,
    wallId: wall?.wallId ?? PREFIX + crypto.randomUUID(), revision: state.revision + 1, bounds,
    tiles: [{ tileId: "output", endpointId, rect: bounds, rotation: "deg0" }],
    placements: [{ placementId: "image", source: { kind: "image", id: resourceId },
      rect: { x: (width - w) / 2, y: (height - h) / 2, width: w, height: h },
      sourceCrop: { x: 0, y: 0, width: 1, height: 1 }, zIndex: 0, interactive: false }],
  });
}

export async function sendDeviceImage(baseUrl: string, endpointId: string, file: File,
  signal: AbortSignal): Promise<DeviceImageReceipt> {
  signal.throwIfAborted();
  const before = await readWallState(baseUrl);
  signal.throwIfAborted();
  imageTarget(before, endpointId);
  if (file.size < 1 || file.size > 16 * 1024 * 1024) throw new Error("图片大小须在 1 字节到 16 MiB 之间");
  const bitmap = await createImageBitmap(file);
  let size: { width: number; height: number };
  try { size = { width: bitmap.width, height: bitmap.height }; } finally { bitmap.close(); }
  signal.throwIfAborted();
  // Validate dimensions before upload; the placeholder is never sent to the daemon.
  deviceImageLayout(before, endpointId, "sha256:" + "0".repeat(64), size);
  const image = await uploadWallImage(baseUrl, file);
  signal.throwIfAborted();
  const current = await readWallState(baseUrl);
  signal.throwIfAborted();
  if (current.revision !== before.revision) throw new Error("设备或布局已变化，请刷新后重新发送");
  const layout = deviceImageLayout(current, endpointId, image.source.id, size);
  // Cancellation can prevent submission, but cannot undo a mutation already in flight.
  await saveWallLayout(baseUrl, before.revision, layout);
  return { endpointId, wallId: layout.wallId, revision: layout.revision };
}

export function deviceImageDelivery(state: WallState, receipt: DeviceImageReceipt): string {
  const wall = state.layouts.find((item) => item.wallId === receipt.wallId);
  if (!wall || wall.revision !== receipt.revision) return "这次发送已被替换或停止";
  const endpoint = state.endpoints.find((item) => item.endpoint.endpointId === receipt.endpointId);
  if (!endpoint?.online) return "已提交，接收端离线，等待重新连接";
  if (state.presentations?.some((item) => item.wallId === receipt.wallId)) return "已提交，目标处于冻结或黑场";
  return endpoint.appliedRevision === receipt.revision ? "接收端已确认呈现图片" : "已提交，等待接收端确认呈现";
}
