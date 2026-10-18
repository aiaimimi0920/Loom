import { postJson } from "./loomApi/transport.ts";

export interface CloudTransferStatus {
  taskId: string;
  control: 0 | 1 | 2;
  total: number;
  completed: number;
  speed: number;
}
export function parseTransferStatus(value: unknown, taskId: string): CloudTransferStatus {
  const v = value as CloudTransferStatus | null;
  if (!v || v.taskId !== taskId || ![0, 1, 2].includes(v.control)
    || ![v.total, v.completed, v.speed].every(n => Number.isSafeInteger(n) && n >= 0)
    || v.total > 8 * 1024 * 1024 || v.completed > 8 * 1024 * 1024)
    throw new Error("asset_library_response_invalid");
  return v;
}
export async function controlCloudDownload(baseUrl: string, taskId: string,
  action: "status" | "pause" | "resume" | "cancel") {
  return parseTransferStatus(await postJson(baseUrl, "/v1/asset-library/download-control", { taskId, action }), taskId);
}
