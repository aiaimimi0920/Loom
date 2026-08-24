// Framework catalog, lifecycle, packaged bootstrap, and package-upgrade clients.
import { isTauri } from "@tauri-apps/api/core";
import { getJson, invokeJsonViaTauri, postJson } from "./transport.ts";

export interface LoomFrameworkPublisher {
  id: string;
  name?: string | null;
  website?: string | null;
  keyId?: string | null;
}

export interface LoomFrameworkAuthoringOption {
  value: unknown;
  label: string;
}

export interface LoomFrameworkAuthoringField {
  id: string;
  label: string;
  type: "string" | "number" | "boolean" | "enum" | "path" | "secret" | "json" | string;
  required?: boolean;
  default?: unknown;
  options?: LoomFrameworkAuthoringOption[];
  placeholder?: string | null;
  minimum?: number | null;
  maximum?: number | null;
  step?: number | null;
  secret?: boolean;
}

export interface LoomFrameworkAuthoringPort {
  name: string;
  label: string;
  type: string;
  executionType: string;
  required?: boolean;
  exposePort?: boolean;
}

export interface LoomFrameworkAuthoringSchema {
  schemaVersion: number;
  title: string;
  description?: string | null;
  fields?: LoomFrameworkAuthoringField[];
  inputs?: LoomFrameworkAuthoringPort[];
  outputs?: LoomFrameworkAuthoringPort[];
}

export interface LoomFrameworkPermissionPolicy {
  network?: {
    domains?: string[];
    allowLocalhost?: boolean;
    allowPrivateNetworks?: boolean;
  };
  filesystem?: { read?: string[]; write?: string[] };
  process?: { spawn?: boolean; maxProcesses?: number | null };
  gpu?: boolean;
  clipboard?: boolean;
  credentials?: string[];
}

export interface LoomFrameworkResourceLimits {
  timeoutSeconds?: number | null;
  memoryMiB?: number | null;
  maxProcesses?: number | null;
  stdoutMiB?: number | null;
  stderrMiB?: number | null;
}

// One Art execution framework's package, authoring, and readiness status.
export interface LoomFramework {
  id: string;
  qualifiedId?: string;
  name: string;
  description: string;
  installed: boolean;
  enabled: boolean;
  ready: boolean;
  readyDetail: string;
  version?: string | null;
  runtimeDir?: string | null;
  publisher?: LoomFrameworkPublisher | null;
  trustStatus?: "trusted" | "verified" | "unsigned" | "invalid" | "revoked" | string;
  declaredPermissions?: string[];
  permissionPolicy?: LoomFrameworkPermissionPolicy;
  resources?: LoomFrameworkResourceLimits;
  authoringSchema?: LoomFrameworkAuthoringSchema | null;
}

interface LoomFrameworksResponse {
  frameworks?: LoomFramework[];
}

interface LoomFrameworkResponse {
  framework?: LoomFramework;
}

const frameworkStatusWeight = (framework: LoomFramework): number => (
  (framework.installed ? 16 : 0)
  + (framework.ready ? 8 : 0)
  + (framework.enabled ? 4 : 0)
  + (framework.version ? 2 : 0)
  + (framework.runtimeDir ? 1 : 0)
);

const uniqueFrameworks = (frameworks: LoomFramework[]): LoomFramework[] => {
  const byIdentity = new Map<string, LoomFramework>();
  for (const framework of frameworks) {
    const identity = framework.qualifiedId?.trim() || framework.id;
    const existing = byIdentity.get(identity);
    if (!existing || frameworkStatusWeight(framework) > frameworkStatusWeight(existing)) {
      byIdentity.set(identity, framework);
    }
  }
  return [...byIdentity.values()];
};

export async function listFrameworks(baseUrl: string): Promise<LoomFramework[]> {
  const response = await getJson<LoomFrameworksResponse>(baseUrl, "/v1/frameworks");
  return Array.isArray(response.frameworks) ? uniqueFrameworks(response.frameworks) : [];
}

export async function installFramework(baseUrl: string, id: string): Promise<LoomFramework | null> {
  const response = isTauri()
    ? await invokeJsonViaTauri<LoomFrameworkResponse>("install_packaged_framework", { baseUrl, id })
    : await postJson<LoomFrameworkResponse>(
      baseUrl,
      `/v1/frameworks/${encodeURIComponent(id)}/install`,
      {},
    );
  return response.framework ?? null;
}

export interface LoomPackagedArtBootstrapResult {
  available: boolean;
  applied: boolean;
  catalogHash?: string | null;
  frameworkIds: string[];
  artIds: string[];
}

export async function bootstrapPackagedArts(
  baseUrl: string,
): Promise<LoomPackagedArtBootstrapResult> {
  if (!isTauri()) {
    return {
      available: false,
      applied: false,
      catalogHash: null,
      frameworkIds: [],
      artIds: [],
    };
  }
  return await invokeJsonViaTauri<LoomPackagedArtBootstrapResult>("bootstrap_packaged_arts", {
    baseUrl,
  });
}

export async function uninstallFramework(baseUrl: string, id: string): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(
    baseUrl,
    `/v1/frameworks/${encodeURIComponent(id)}/uninstall`,
    {},
  );
  return response.framework ?? null;
}

export async function enableFramework(baseUrl: string, id: string): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(
    baseUrl,
    `/v1/frameworks/${encodeURIComponent(id)}/enable`,
    {},
  );
  return response.framework ?? null;
}

export async function disableFramework(baseUrl: string, id: string): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(
    baseUrl,
    `/v1/frameworks/${encodeURIComponent(id)}/disable`,
    {},
  );
  return response.framework ?? null;
}

export async function installFrameworkPackage(
  baseUrl: string,
  zipBase64: string,
): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(baseUrl, "/v1/frameworks/install", {
    zipBase64,
  });
  return response.framework ?? null;
}

export async function upgradeFrameworkPackage(
  baseUrl: string,
  id: string,
  zipBase64: string,
): Promise<LoomFramework | null> {
  const response = await postJson<LoomFrameworkResponse>(
    baseUrl,
    `/v1/frameworks/${encodeURIComponent(id)}/upgrade`,
    { zipBase64 },
  );
  return response.framework ?? null;
}
