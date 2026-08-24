// Art store, management, authoring, publication, and uninstall clients.
import type { LoomArtRuntimeManifest, LoomToolDefinition } from "./coreTypes.ts";
import type { LoomCredentialSummary } from "./plugins.ts";
import { getJson, postJson, putJson } from "./transport.ts";

// A remote art-store catalog entry.
export interface ArtStoreEntry {
  id: string;
  qualifiedId?: string;
  globalId?: string;
  name?: string;
  description?: string;
  framework?: string;
  latestVersion?: string;
  versions?: Array<{ version: string; sha256?: string }>;
  official?: boolean;
}

interface ArtStoreCatalogResponse {
  arts?: ArtStoreEntry[];
}

export async function fetchArtStoreCatalog(baseUrl: string): Promise<ArtStoreEntry[]> {
  const response = await getJson<ArtStoreCatalogResponse>(baseUrl, "/v1/arts/store/catalog");
  return Array.isArray(response.arts) ? response.arts : [];
}

export async function installArtFromStore(
  baseUrl: string,
  artId: string,
  version?: string,
): Promise<void> {
  await postJson(baseUrl, "/v1/arts/store/install", { artId, version });
}

export interface LoomArtManagementParameter {
  id: string;
  label: string;
  parameterType: string;
  required: boolean;
  secret: boolean;
  default?: unknown;
  options?: unknown;
  minimum?: unknown;
  maximum?: unknown;
  step?: unknown;
}

export interface LoomArtInstalledVersion {
  version: string;
  digest: string;
  active: boolean;
}

export interface LoomArtManagement {
  artId: string;
  name: string;
  description: string;
  locallyAuthored: boolean;
  canEditIdentity: boolean;
  currentVersion: string;
  highestVersion: string;
  autoUpdate: boolean;
  installedVersions: LoomArtInstalledVersion[];
  availableVersions: string[];
  parameters: LoomArtManagementParameter[];
  defaults: Record<string, unknown>;
  valueBindings: Record<string, string>;
  credentialBindings: Record<string, string>;
  availableCredentials: LoomCredentialSummary[];
  updateAvailable: boolean;
}

export interface LoomArtManagementSettingsInput {
  name?: string;
  description?: string;
  autoUpdate: boolean;
  defaults: Record<string, unknown>;
  valueBindings: Record<string, string>;
  credentialBindings: Record<string, string>;
  secretValues?: Record<string, string>;
}

export async function getArtManagement(
  baseUrl: string,
  artId: string,
): Promise<LoomArtManagement> {
  return await getJson<LoomArtManagement>(
    baseUrl,
    `/v1/arts/${encodeURIComponent(artId)}/management`,
  );
}

export async function saveArtManagementSettings(
  baseUrl: string,
  artId: string,
  input: LoomArtManagementSettingsInput,
): Promise<LoomArtManagement> {
  return await putJson<LoomArtManagement>(
    baseUrl,
    `/v1/arts/${encodeURIComponent(artId)}/settings`,
    input,
  );
}

export async function updateArtToVersion(
  baseUrl: string,
  artId: string,
  version: string,
): Promise<LoomArtManagement> {
  return await postJson<LoomArtManagement>(
    baseUrl,
    `/v1/arts/${encodeURIComponent(artId)}/update`,
    { version },
  );
}

export async function autoUpdateArts(
  baseUrl: string,
): Promise<{ updated?: unknown[]; errors?: unknown[] }> {
  return await postJson(baseUrl, "/v1/arts/auto-update", {});
}

export async function installArtPackage(baseUrl: string, zipBase64: string): Promise<void> {
  await postJson(baseUrl, "/v1/arts/install", { zipBase64 });
}

export async function createAuthoredArtPackage(
  baseUrl: string,
  tool: LoomToolDefinition,
  runtime?: LoomArtRuntimeManifest,
  options?: {
    files?: Array<{ path: string; content: string }>;
    sourceDirectory?: string;
    sourceDirectoryTarget?: string;
  },
): Promise<LoomToolDefinition | null> {
  const response = await postJson<{ tool?: LoomToolDefinition }>(baseUrl, "/v1/arts/create", {
    tool,
    runtime,
    files: options?.files ?? [],
    sourceDirectory: options?.sourceDirectory,
    sourceDirectoryTarget: options?.sourceDirectoryTarget,
  });
  return response.tool ?? null;
}

export async function publishArt(
  baseUrl: string,
  artId: string,
): Promise<{ artId: string; globalId: string; sha256: string; published: boolean }> {
  return await postJson(baseUrl, "/v1/arts/store/publish", { artId });
}

export interface LoomArtUninstallResult {
  artId?: string;
  uninstalled?: boolean;
  removedMcpServers?: string[];
  retainedMcpServers?: Array<{ packageId?: string; usedByArtIds?: string[] }>;
}

export async function uninstallArtPackage(
  baseUrl: string,
  artIdentity: string,
  options: { removeUnusedMcpServers: boolean },
): Promise<LoomArtUninstallResult> {
  return await postJson<LoomArtUninstallResult>(
    baseUrl,
    `/v1/arts/${encodeURIComponent(artIdentity)}/uninstall`,
    options,
  );
}
