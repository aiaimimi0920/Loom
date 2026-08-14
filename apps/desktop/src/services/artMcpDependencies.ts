import type { LoomMcpServer, LoomToolDefinition } from "./loomApi";

export type ArtMcpDependencyStatus = "ready" | "credentials_required" | "disabled" | "missing";

export interface ArtMcpDependencyState {
  dependencyId: string;
  server: LoomMcpServer | null;
  status: ArtMcpDependencyStatus;
}

const recordValue = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;

const stringValue = (value: unknown): string => typeof value === "string" ? value.trim() : "";

export function artMcpDependencyIds(tool: LoomToolDefinition): string[] {
  const metadata = recordValue(tool.metadata);
  const dependencies = recordValue(metadata?.dependencies);
  const servers = Array.isArray(dependencies?.mcpServers) ? dependencies.mcpServers : [];
  return [...new Set(servers
    .map(recordValue)
    .map((dependency) => stringValue(dependency?.id))
    .filter(Boolean))];
}

function serverMatchesDependency(server: LoomMcpServer, dependencyId: string): boolean {
  return server.id === dependencyId
    || server.serverId === dependencyId
    || server.package?.qualifiedId === dependencyId;
}

export function resolveArtMcpDependencies(
  tool: LoomToolDefinition,
  servers: LoomMcpServer[],
): ArtMcpDependencyState[] {
  return artMcpDependencyIds(tool).map((dependencyId) => {
    const server = servers.find((candidate) => serverMatchesDependency(candidate, dependencyId)) ?? null;
    if (!server) return { dependencyId, server, status: "missing" };
    if (server.enabled === false) return { dependencyId, server, status: "disabled" };
    if (server.credentialRequired && !server.credentialBound) {
      return { dependencyId, server, status: "credentials_required" };
    }
    return { dependencyId, server, status: "ready" };
  });
}
