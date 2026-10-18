import { postJson } from "./loomApi/transport.ts";

export interface CloudPackageSelection { id: string; kind: string; size: number; sha256: string; policy?: string }
export interface CloudDownload { publicationId: string; path: string; size: number; sha256: string; installedEntry?: string }

export function canInstallCloudPackage(selected: CloudPackageSelection): boolean {
  if (selected.kind === "application") return ["application-zip-format-v1", "application-large-zip-format-v1"].includes(selected.policy ?? "");
  if (selected.kind === "mcp") return selected.policy === "loom-mcp-zip-format-v1";
  return ["art", "capability"].includes(selected.kind)
    && [`loom-${selected.kind}-zip-clamav-v1`, `loom-${selected.kind}-zip-format-v1`].includes(selected.policy ?? "");
}

export function parseCloudDownload(value: unknown, selected: CloudPackageSelection): CloudDownload {
  const v = value as Partial<CloudDownload> | null;
  if (!v || v.publicationId !== selected.id || typeof v.path !== "string" || !v.path
      || v.size !== selected.size || v.sha256 !== selected.sha256)
    throw new Error("asset_library_response_invalid");
  return { publicationId: v.publicationId, path: v.path, size: v.size, sha256: v.sha256 };
}

export async function installCloudPackage(baseUrl: string, selected: CloudPackageSelection, taskId?: string) {
  if (!canInstallCloudPackage(selected))
    throw new Error("asset_library_install_selection_invalid");
  const result = await postJson<{ download: unknown; installation: unknown }>(baseUrl,
    "/v1/asset-library/install", { publicationId: selected.id, kind: selected.kind, sha256: selected.sha256,
      ...(taskId ? { taskId } : {}) });
  if (!result || !result.installation || typeof result.installation !== "object")
    throw new Error("asset_library_response_invalid");
  const download = parseCloudDownload(result.download, selected);
  if (selected.kind === "application") {
    const installation = result.installation as { entry?: unknown };
    if (typeof installation.entry !== "string" || !installation.entry)
      throw new Error("asset_library_response_invalid");
    return { ...download, installedEntry: installation.entry };
  }
  return download;
}
