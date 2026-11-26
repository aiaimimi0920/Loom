// Projects marketplace choices into installed server configuration and health state.

import { uiFormat, uiText } from "../../i18n/index.ts";
import type { LoomMcpServer } from "../loomApi.ts";
import { dedupeMarketplaceServers } from "./helpers.ts";
import type {
  McpMarketplaceHealth,
  McpMarketplaceTestSnapshot,
  McpMarketServer,
} from "./types.ts";

export function mergeRegistryAndCuratedMarketplace(
  registryServers: readonly McpMarketServer[],
  curatedServers: readonly McpMarketServer[],
): McpMarketServer[] {
  return dedupeMarketplaceServers([...registryServers, ...curatedServers]);
}

export function buildMarketplaceServerConfig(
  marketItem: McpMarketServer,
  existing?: LoomMcpServer,
  installOptionId?: string,
): LoomMcpServer {
  const option = marketItem.installOptions.find((candidate) => candidate.id === installOptionId) ||
    marketItem.installOptions.find((candidate) => candidate.transport === existing?.transport &&
      (candidate.transport === "stdio" ? candidate.command === existing?.command : candidate.url === existing?.url)) ||
    marketItem.installOptions.find((candidate) => candidate.transport === existing?.transport) ||
    marketItem.installOptions[0];
  if (!option) throw new Error(uiFormat("mcp.error.noInstallOption", { id: marketItem.id }));
  return {
    id: existing?.id || marketItem.id,
    name: marketItem.name,
    description: marketItem.description,
    transport: option.transport,
    command: option.command,
    args: [...option.args],
    env: mergeMarketplaceEnv(option.env, existing?.env),
    url: option.transport === "streamable-http" && existing?.transport === "streamable-http" && existing.url
      ? existing.url
      : option.url,
    headers: mergeMarketplaceEnv(option.headers, existing?.headers),
    enabled: existing?.enabled ?? marketItem.defaultEnabled ?? true,
  };
}

export function getMarketplaceHealth(
  marketItem: McpMarketServer,
  configuredServer?: LoomMcpServer,
  testSnapshot?: McpMarketplaceTestSnapshot,
): McpMarketplaceHealth {
  const installOption = marketItem.installOptions.find((option) =>
    configuredServer?.transport === option.transport &&
    (option.transport === "stdio" ? option.command === configuredServer.command : option.url === configuredServer?.url)) ||
    marketItem.installOptions.find((option) => configuredServer?.transport === option.transport) ||
    marketItem.installOptions[0];
  const requiredEnvKeys = installOption?.requiredEnvKeys || [];
  const requiredHeaderKeys = installOption?.requiredHeaderKeys || [];
  const requiredEnvPresent =
    (requiredEnvKeys.length === 0 || requiredEnvKeys.every((key) => Boolean(configuredServer?.env?.[key]?.trim()))) &&
    (requiredHeaderKeys.length === 0 || requiredHeaderKeys.every((key) => Boolean(configuredServer?.headers?.[key]?.trim())));
  const tags: McpMarketplaceHealth["tags"] = [];

  if (configuredServer) {
    tags.push({ label: uiText("mcp.health.installed"), tone: "success" });
    tags.push({
      label: configuredServer.enabled === false ? uiText("mcp.status.disabled") : uiText("mcp.status.enabled"),
      tone: configuredServer.enabled === false ? "neutral" : "success",
    });
  }

  if (installOption?.requiresManualConfiguration) {
    tags.push({ label: uiText("mcp.health.configRequired"), tone: "warning" });
  }

  if (requiredEnvKeys.length > 0 || requiredHeaderKeys.length > 0) {
    tags.push({
      label: requiredEnvPresent ? uiText("mcp.health.secretConfigured") : uiText("mcp.health.secretMissing"),
      tone: requiredEnvPresent ? "success" : "error",
    });
  }

  if (testSnapshot?.status === "success") {
    tags.push({ label: uiFormat("mcp.health.toolsDiscovered", { count: testSnapshot.toolCount }), tone: "success" });
  } else if (testSnapshot?.status === "error") {
    tags.push({ label: uiText("mcp.health.testFailed"), tone: "error" });
  }

  return {
    configured: Boolean(configuredServer),
    requiredEnvPresent,
    toolCount: testSnapshot?.status === "success" ? testSnapshot.toolCount : undefined,
    lastTested: testSnapshot?.testedAt,
    lastError: testSnapshot?.status === "error" ? testSnapshot.error : undefined,
    tags,
  };
}

function mergeMarketplaceEnv(
  marketEnv: Record<string, string>,
  existingEnv: Record<string, string> = {},
): Record<string, string> {
  const merged: Record<string, string> = { ...existingEnv };
  Object.entries(marketEnv).forEach(([key, value]) => {
    const existingValue = merged[key];
    if (existingValue === undefined || (existingValue.trim() === "" && value.trim() !== "")) {
      merged[key] = value;
    }
  });
  return merged;
}
