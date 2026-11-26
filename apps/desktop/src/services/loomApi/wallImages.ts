import { uiFormat, uiText } from "../../i18n/index.ts";
import { postJson } from "./transport.ts";
import type { WallSourceOption } from "./wallTypes.ts";

export const WALL_IMAGE_ACCEPT = "image/png,image/jpeg,image/webp,image/bmp,image/gif";
const MAX_BYTES = 16 * 1024 * 1024;

/** Uploads immutable source bytes; placement and layout CAS remain explicit editor actions. */
export async function uploadWallImage(baseUrl: string, file: File): Promise<WallSourceOption> {
  if (file.size < 1 || file.size > MAX_BYTES) throw new Error(uiText("devices.wall.image.error.sizeRange"));
  if (!WALL_IMAGE_ACCEPT.split(",").includes(file.type)) throw new Error(uiText("devices.wall.image.error.unsupportedFormat"));
  const bytes = new Uint8Array(await file.arrayBuffer());
  if (bytes.length !== file.size) throw new Error(uiText("devices.wall.image.error.incompleteRead"));
  const chunks: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += 32768) {
    chunks.push(String.fromCharCode(...bytes.subarray(offset, offset + 32768)));
  }
  const response = await postJson<{ resource?: { resourceId?: unknown; kind?: unknown; mime?: unknown; size?: unknown } }>(
    baseUrl, "/v1/surfaces/resources", { kind: "image", mime: file.type, dataBase64: btoa(chunks.join("")) });
  const resource = response?.resource;
  if (!resource || resource.kind !== "image" || resource.mime !== file.type || resource.size !== file.size
    || typeof resource.resourceId !== "string" || !/^sha256:[a-f0-9]{64}$/.test(resource.resourceId)) {
    throw new Error(uiText("devices.wall.image.error.invalidResource"));
  }
  return {
    source: { kind: "image", id: resource.resourceId },
    label: uiFormat("devices.wall.source.imageLabel", { name: file.name.slice(0, 120) }),
    status: uiText("devices.wall.source.statusImported"),
  };
}
