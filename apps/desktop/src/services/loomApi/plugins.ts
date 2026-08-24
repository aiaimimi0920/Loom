// Plugin trust, credential, and publisher-identity contracts and clients.
import { getJson, postJson } from "./transport.ts";

export interface LoomPublisherTrustRecord {
  publisherId: string;
  keyId: string;
  publicKey: string;
  revoked: boolean;
}

export type LoomPluginTrustPolicy = "allow_unsigned" | "require_signed" | "require_trusted";

export interface LoomPluginTrustStore {
  schemaVersion?: number;
  publishers: LoomPublisherTrustRecord[];
  policy: LoomPluginTrustPolicy;
  trustedPublishers: string[];
}

function normalizePluginTrustStore(response: Partial<LoomPluginTrustStore>): LoomPluginTrustStore {
  return {
    schemaVersion: response.schemaVersion,
    publishers: Array.isArray(response.publishers) ? response.publishers : [],
    policy: response.policy ?? "allow_unsigned",
    trustedPublishers: Array.isArray(response.trustedPublishers) ? response.trustedPublishers : [],
  };
}

export async function listPluginTrust(baseUrl: string): Promise<LoomPluginTrustStore> {
  const response = await getJson<LoomPluginTrustStore>(baseUrl, "/v1/plugin-trust");
  return normalizePluginTrustStore(response);
}

export async function trustPluginPublisher(
  baseUrl: string,
  record: Omit<LoomPublisherTrustRecord, "revoked"> & { revoked?: boolean },
): Promise<LoomPluginTrustStore> {
  const response = await postJson<LoomPluginTrustStore>(baseUrl, "/v1/plugin-trust/publishers", {
    ...record,
    revoked: record.revoked ?? false,
  });
  return normalizePluginTrustStore(response);
}

export async function revokePluginPublisher(
  baseUrl: string,
  publisherId: string,
  keyId: string,
): Promise<LoomPluginTrustStore> {
  const response = await postJson<LoomPluginTrustStore>(baseUrl, "/v1/plugin-trust/revoke", {
    publisherId,
    keyId,
  });
  return normalizePluginTrustStore(response);
}

export async function setPluginTrustPolicy(
  baseUrl: string,
  policy: LoomPluginTrustPolicy,
): Promise<LoomPluginTrustStore> {
  return normalizePluginTrustStore(await postJson<LoomPluginTrustStore>(
    baseUrl,
    "/v1/plugin-trust/policy",
    { policy },
  ));
}

export async function trustPluginUser(
  baseUrl: string,
  userId: string,
): Promise<LoomPluginTrustStore> {
  return normalizePluginTrustStore(await postJson<LoomPluginTrustStore>(
    baseUrl,
    "/v1/plugin-trust/users",
    { userId },
  ));
}

export async function untrustPluginUser(
  baseUrl: string,
  userId: string,
): Promise<LoomPluginTrustStore> {
  return normalizePluginTrustStore(await postJson<LoomPluginTrustStore>(
    baseUrl,
    "/v1/plugin-trust/users/remove",
    { userId },
  ));
}

export interface LoomCredentialScope {
  frameworkId?: string;
  artId?: string;
}

export type LoomCredentialValueType = "string" | "number" | "integer" | "boolean" | "json";

export interface LoomCredentialSummary {
  name: string;
  valueType: LoomCredentialValueType;
  scope: LoomCredentialScope;
  expiresAt?: string | null;
  protection: string;
}

export interface LoomCredentialDetails extends LoomCredentialSummary {
  value: string;
}

export interface LoomCredentialInput {
  name: string;
  value: string;
  valueType: LoomCredentialValueType;
  scope?: LoomCredentialScope;
  expiresAt?: string | null;
}

interface LoomCredentialsResponse {
  credentials?: LoomCredentialSummary[];
}

interface LoomCredentialResponse {
  credential?: LoomCredentialSummary;
}

export async function listPluginCredentials(baseUrl: string): Promise<LoomCredentialSummary[]> {
  const response = await getJson<LoomCredentialsResponse>(baseUrl, "/v1/plugin-credentials");
  return Array.isArray(response.credentials) ? response.credentials : [];
}

export async function savePluginCredential(
  baseUrl: string,
  input: LoomCredentialInput,
): Promise<LoomCredentialSummary | null> {
  const response = await postJson<LoomCredentialResponse>(baseUrl, "/v1/plugin-credentials", input);
  return response.credential ?? null;
}

export async function deletePluginCredential(
  baseUrl: string,
  name: string,
  scope: LoomCredentialScope = {},
): Promise<void> {
  await postJson(baseUrl, "/v1/plugin-credentials/delete", { name, scope });
}

export async function revealPluginCredential(
  baseUrl: string,
  name: string,
  scope: LoomCredentialScope = {},
): Promise<LoomCredentialDetails | null> {
  const response = await postJson<{ credential?: LoomCredentialDetails }>(
    baseUrl,
    "/v1/plugin-credentials/reveal",
    { name, scope },
  );
  return response.credential ?? null;
}

export interface LoomPublisherIdentity {
  schemaVersion: number;
  userId: string;
  currentKeyId: string;
  publicKey: string;
}

export interface LoomPublisherIdentityState {
  identity: LoomPublisherIdentity | null;
  hasPrivateKey: boolean;
}

export async function getPublisherIdentity(baseUrl: string): Promise<LoomPublisherIdentityState> {
  const response = await getJson<Partial<LoomPublisherIdentityState>>(baseUrl, "/v1/publisher-identity");
  return {
    identity: response.identity ?? null,
    hasPrivateKey: response.hasPrivateKey === true,
  };
}

export async function registerPublisherIdentity(baseUrl: string): Promise<LoomPublisherIdentityState> {
  return await postJson<LoomPublisherIdentityState>(baseUrl, "/v1/publisher-identity/register", {});
}

export async function rotatePublisherIdentity(baseUrl: string): Promise<LoomPublisherIdentityState> {
  return await postJson<LoomPublisherIdentityState>(baseUrl, "/v1/publisher-identity/rotate", {});
}

export async function revealPublisherPrivateKey(baseUrl: string): Promise<{
  keyId: string;
  privateKey: string;
  publicKey: string;
}> {
  return await postJson(baseUrl, "/v1/publisher-identity/private-key", {});
}
