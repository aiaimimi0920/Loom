import { postJson } from "./loomApi/transport.ts";
import type { ArtStoreEntry } from "./loomApi/arts.ts";
import type { CloudPackageSelection } from "./cloudPackageInstall.ts";

export interface CloudPublication extends CloudPackageSelection {
  title: string;
  label: string;
}
export interface CloudPublicationPage { items: CloudPublication[]; nextAfter: string | null }
export interface CloudArtEntry extends ArtStoreEntry { publication: CloudPublication }

export function parseCloudPublicationPage(value: unknown): CloudPublicationPage {
  const page = value as Partial<CloudPublicationPage> | null;
  if (!page || !Array.isArray(page.items) || page.items.length > 100
      || (page.nextAfter !== null && (typeof page.nextAfter !== "string" || !page.nextAfter || page.nextAfter.length > 512)))
    throw new Error("asset_library_response_invalid");
  for (const item of page.items) {
    const limit = item?.kind === "application" && item.policy === "application-large-zip-format-v1"
      ? 96_000_000 : 8 * 1024 * 1024;
    if (!item || typeof item.id !== "string" || !item.id
        || typeof item.title !== "string" || typeof item.label !== "string"
        || !["art", "capability", "mcp", "application"].includes(item.kind)
        || !Number.isSafeInteger(item.size) || item.size <= 0 || item.size > limit
        || typeof item.sha256 !== "string" || !/^[0-9a-f]{64}$/.test(item.sha256)
        || (item.policy !== undefined && typeof item.policy !== "string"))
      throw new Error("asset_library_response_invalid");
  }
  return page as CloudPublicationPage;
}

export async function fetchCloudArtCatalog(baseUrl: string, after = "") {
  const page = parseCloudPublicationPage(await postJson<unknown>(baseUrl, "/v1/asset-library/catalog", { after }));
  const arts: CloudArtEntry[] = page.items.filter((item) => item.kind === "art").map((publication) => ({
    id: publication.id, name: publication.title, latestVersion: publication.label,
    publication,
  }));
  // 目录未提供发布者认证或框架元数据，不猜测 official/framework。
  return { arts, nextAfter: page.nextAfter };
}
