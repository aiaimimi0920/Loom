// Capability Plugin catalog, permission, and lifecycle clients.
import { getJson, postJson } from "./transport.ts";

export type CapabilityLifecycleStatus =
  | "installed_disabled"
  | "approval_required"
  | "activating"
  | "active"
  | "faulted"
  | "disabling"
  | "upgrading"
  | "rolling_back"
  | "uninstalling";

export interface CapabilityInstalledVersion {
  version: string;
  digest: string;
  relativePath: string;
  trustStatus: string;
  installedAt: string;
  requestedPermissions: string[];
}

export interface CapabilityRuntimeFailures {
  count: number;
  windowStartedAtMs?: number;
  lastFailureAtMs?: number;
  restartNotBeforeMs?: number;
}

export interface CapabilityPluginRecord {
  qualifiedId: string;
  publisherId: string;
  packageId: string;
  name: string;
  description: string;
  enabledIntent: boolean;
  status: CapabilityLifecycleStatus;
  runtimeFailures: CapabilityRuntimeFailures;
  activeDigest?: string;
  previousDigest?: string;
  requestedPermissions: string[];
  versions: CapabilityInstalledVersion[];
}

export interface CapabilityPermissionGrant {
  qualifiedId: string;
  packageDigest: string;
  permissions: string[];
  grantedAt: string;
}

export interface CapabilityApiRequirement {
  minimum: string;
  maximum?: string;
  requiredFeatures: string[];
  optionalFeatures: string[];
}

export interface CapabilityHostCompatibility {
  loomCapabilityApi: CapabilityApiRequirement;
  hookExtensionApi: CapabilityApiRequirement;
  surfaceApi?: CapabilityApiRequirement;
}

export interface CapabilityCatalogArtifact {
  url: string;
  sha256: string;
  bytes: number;
}

export interface CapabilityCatalogEntry {
  qualifiedId: string;
  name: string;
  description: string;
  version: string;
  publisher: { id: string; keyId: string };
  package: CapabilityCatalogArtifact & {
    signature: { algorithm: string; keyId: string };
  };
  sbom: CapabilityCatalogArtifact;
  provenance: CapabilityCatalogArtifact;
  hostCompatibility: CapabilityHostCompatibility;
  permissions: string[];
  diskBytes: number;
}

export interface CapabilityCatalogItem {
  entry: CapabilityCatalogEntry;
  compatible: boolean;
  compatibilityDetail?: string | null;
}

export interface CapabilityCatalogSnapshot {
  configured: boolean;
  publisher?: { id: string; keyId: string };
  generatedAt?: string;
  expiresAt?: string;
  packages: CapabilityCatalogItem[];
  diagnostic?: string;
}

export interface CapabilityPluginSnapshot {
  plugins: CapabilityPluginRecord[];
  diskBytesByPlugin: Record<string, number>;
}

export interface CapabilityInstallReport {
  qualifiedId: string;
  version: string;
  digest: string;
  packageDir: string;
  trustStatus: string;
  installedFiles: string[];
}

const pluginPath = (qualifiedId: string, action: string) =>
  `/v1/capability-plugins/${encodeURIComponent(qualifiedId)}/${action}`;

export async function listCapabilityPlugins(baseUrl: string): Promise<CapabilityPluginSnapshot> {
  const response = await getJson<Partial<CapabilityPluginSnapshot>>(baseUrl, "/v1/capability-plugins");
  return {
    plugins: Array.isArray(response.plugins) ? response.plugins : [],
    diskBytesByPlugin: response.diskBytesByPlugin ?? {},
  };
}

export async function listCapabilityGrants(baseUrl: string): Promise<CapabilityPermissionGrant[]> {
  const response = await getJson<{ grants?: CapabilityPermissionGrant[] }>(
    baseUrl,
    "/v1/capability-plugins/grants",
  );
  return Array.isArray(response.grants) ? response.grants : [];
}

export async function getCapabilityCatalog(baseUrl: string): Promise<CapabilityCatalogSnapshot> {
  const response = await getJson<Partial<CapabilityCatalogSnapshot>>(
    baseUrl,
    "/v1/capability-plugins/catalog",
  );
  return {
    ...response,
    configured: response.configured === true,
    packages: Array.isArray(response.packages) ? response.packages : [],
  };
}

export async function installLocalCapability(
  baseUrl: string,
  zipBase64: string,
): Promise<CapabilityInstallReport> {
  const response = await postJson<{ package?: CapabilityInstallReport }>(
    baseUrl,
    "/v1/capability-plugins/install",
    { zipBase64 },
  );
  if (!response.package) throw new Error("Loom 本地服务没有返回已安装的能力扩展。");
  return response.package;
}

export async function installCatalogCapability(
  baseUrl: string,
  qualifiedId: string,
): Promise<CapabilityInstallReport> {
  const response = await postJson<{ package?: CapabilityInstallReport }>(
    baseUrl,
    "/v1/capability-plugins/catalog/install",
    { qualifiedId },
  );
  if (!response.package) throw new Error("Loom 本地服务没有返回目录安装结果。");
  return response.package;
}

export async function approveCapabilityPermissions(
  baseUrl: string,
  qualifiedId: string,
  digest: string,
  permissions: string[],
): Promise<void> {
  await postJson(baseUrl, pluginPath(qualifiedId, "approve"), { digest, permissions });
}

const mutateCapability = async (
  baseUrl: string,
  qualifiedId: string,
  action: string,
  body: unknown = {},
): Promise<CapabilityPluginRecord> => {
  const response = await postJson<{ plugin?: CapabilityPluginRecord }>(
    baseUrl,
    pluginPath(qualifiedId, action),
    body,
  );
  if (!response.plugin) throw new Error("Loom 本地服务没有返回能力扩展状态。");
  return response.plugin;
};

export const enableCapability = (baseUrl: string, qualifiedId: string, digest?: string) =>
  mutateCapability(baseUrl, qualifiedId, "enable", digest ? { digest } : {});

export const disableCapability = (baseUrl: string, qualifiedId: string) =>
  mutateCapability(baseUrl, qualifiedId, "disable");

export const upgradeCapability = (baseUrl: string, qualifiedId: string, digest: string) =>
  mutateCapability(baseUrl, qualifiedId, "upgrade", { digest });

export const rollbackCapability = (baseUrl: string, qualifiedId: string) =>
  mutateCapability(baseUrl, qualifiedId, "rollback");

export const uninstallCapability = (baseUrl: string, qualifiedId: string) =>
  mutateCapability(baseUrl, qualifiedId, "uninstall");

