// MCP server lifecycle, registry, test, call, and package-plan clients.
import type { McpRegistryResponse } from "../mcpMarketplace.ts";
import type {
  LoomMcpCallToolResponse,
  LoomMcpServer,
  LoomMcpTestResult,
  McpPackageCheckResult,
  McpPackageInstallPlan,
} from "./mcpTypes.ts";
import { deleteJson, getJson, postJson, putJson } from "./transport.ts";

export async function deleteMcpServer(baseUrl: string, serverId: string): Promise<void> {
  await deleteJson(baseUrl, `/v1/mcp/servers/${encodeURIComponent(serverId)}`);
}

export async function saveMcpServer(baseUrl: string, server: LoomMcpServer): Promise<LoomMcpServer> {
  const response = await putJson<{ server?: LoomMcpServer }>(
    baseUrl,
    `/v1/mcp/servers/${encodeURIComponent(server.id)}`,
    server,
  );
  return response.server ?? server;
}

export async function installMcpServerPackage(
  baseUrl: string,
  zipBase64: string,
): Promise<LoomMcpServer> {
  const response = await postJson<{ server?: LoomMcpServer }>(baseUrl, "/v1/mcp/servers/install", {
    zipBase64,
  });
  if (!response.server) throw new Error("Loom 本地服务没有返回已安装的 MCP 服务。");
  return response.server;
}

export async function setMcpServerEnabled(
  baseUrl: string,
  serverId: string,
  enabled: boolean,
): Promise<LoomMcpServer> {
  const response = await putJson<{ server?: LoomMcpServer }>(
    baseUrl,
    `/v1/mcp/servers/${encodeURIComponent(serverId)}/enabled`,
    { enabled },
  );
  if (!response.server) throw new Error("Loom 本地服务没有返回 MCP 服务状态。");
  return response.server;
}

export async function updateMcpServerCredentials(
  baseUrl: string,
  serverId: string,
  values: Record<string, string>,
  clear: string[] = [],
): Promise<LoomMcpServer> {
  const response = await putJson<{ server?: LoomMcpServer }>(
    baseUrl,
    `/v1/mcp/servers/${encodeURIComponent(serverId)}/credentials`,
    { values, clear },
  );
  if (!response.server) throw new Error("Loom 本地服务没有返回 MCP 凭据状态。");
  return response.server;
}

export async function fetchMcpRegistry(
  baseUrl: string,
  options: {
    search?: string;
    limit?: number;
    cursor?: string | null;
    updatedSince?: string;
    includeDeleted?: boolean;
    refresh?: boolean;
  } = {},
): Promise<McpRegistryResponse> {
  const params = new URLSearchParams();
  if (options.search?.trim()) params.set("search", options.search.trim());
  if (typeof options.limit === "number") params.set("limit", String(options.limit));
  if (options.cursor?.trim()) params.set("cursor", options.cursor.trim());
  params.set("version", "latest");
  if (options.updatedSince?.trim()) params.set("updated_since", options.updatedSince.trim());
  if (options.includeDeleted) params.set("include_deleted", "true");
  if (options.refresh) params.set("refresh", "true");
  const suffix = params.toString();
  return await getJson<McpRegistryResponse>(baseUrl, `/v1/mcp/registry${suffix ? `?${suffix}` : ""}`);
}

export async function testMcpConnection(
  baseUrl: string,
  server: LoomMcpServer,
): Promise<LoomMcpTestResult> {
  return await postJson<LoomMcpTestResult>(baseUrl, "/v1/mcp/test", server);
}

export async function testInstalledMcpServer(
  baseUrl: string,
  serverId: string,
): Promise<LoomMcpTestResult> {
  return await postJson<LoomMcpTestResult>(
    baseUrl,
    `/v1/mcp/servers/${encodeURIComponent(serverId)}/test`,
    {},
  );
}

export async function callMcpTool(
  baseUrl: string,
  server: Pick<LoomMcpServer, "transport" | "command" | "args" | "env" | "url" | "headers">,
  toolName: string,
  toolArgs: Record<string, unknown> = {},
): Promise<LoomMcpCallToolResponse> {
  return await postJson<LoomMcpCallToolResponse>(baseUrl, "/v1/mcp/call", {
    transport: server.transport ?? "stdio",
    command: server.command,
    args: server.args ?? [],
    env: server.env ?? {},
    url: server.url ?? "",
    headers: server.headers ?? {},
    toolName,
    toolArgs,
  });
}

export async function checkMcpPackageInstalled(
  baseUrl: string,
  moduleName: string,
): Promise<McpPackageCheckResult> {
  return await postJson<McpPackageCheckResult>(baseUrl, "/v1/mcp/package/check", { moduleName });
}

export async function buildMcpPackageInstallPlan(
  baseUrl: string,
  packageName: string,
): Promise<McpPackageInstallPlan> {
  return await postJson<McpPackageInstallPlan>(baseUrl, "/v1/mcp/package/install-plan", { packageName });
}
