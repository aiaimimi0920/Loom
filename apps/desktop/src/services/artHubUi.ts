import type { LoomFramework, LoomToolDefinition } from "./loomApi";

export type ArtWorkspaceId = "registry" | "store" | "security";

export interface ArtWorkspaceItem {
  id: ArtWorkspaceId;
  label: string;
}

export const artWorkspaceItems: ArtWorkspaceItem[] = [
  { id: "registry", label: "注册表" },
  { id: "store", label: "商店" },
  { id: "security", label: "信任与凭据" },
];

const officialFrameworkExecutionTypes = new Set([
  "cli_wrapper",
  "cloud_api",
  "script",
  "python_art",
  "mcp",
  "workflow",
]);

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function nonEmptyString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

export function frameworkIdentity(framework: LoomFramework): string {
  return framework.qualifiedId || framework.id;
}

export function artFrameworkReference(tool: LoomToolDefinition): string | null {
  const metadata = asRecord(tool.metadata);
  const dependencies = asRecord(metadata?.dependencies);
  const authoring = asRecord(metadata?.authoring);
  const metadataReference = nonEmptyString(dependencies?.framework)
    ?? nonEmptyString(authoring?.frameworkId);
  if (metadataReference) return metadataReference;

  const executionType = nonEmptyString(tool.execution?.type);
  if (executionType === "framework_art") {
    return nonEmptyString(tool.execution?.framework);
  }
  return executionType && officialFrameworkExecutionTypes.has(executionType)
    ? executionType
    : null;
}

function frameworkMatchesReference(
  framework: LoomFramework,
  reference: string,
  frameworks: LoomFramework[],
): boolean {
  if (reference === frameworkIdentity(framework)) return true;
  if (reference !== framework.id) return false;
  return frameworks.filter((candidate) => candidate.id === reference).length === 1;
}

export function filterToolsByFrameworks(
  tools: LoomToolDefinition[],
  frameworks: LoomFramework[],
  selectedFrameworkIds: ReadonlySet<string> | null,
): LoomToolDefinition[] {
  if (selectedFrameworkIds === null) return tools;
  if (selectedFrameworkIds.size === 0) return [];

  const selectedFrameworks = frameworks.filter((framework) => (
    selectedFrameworkIds.has(frameworkIdentity(framework))
  ));
  return tools.filter((tool) => {
    const reference = artFrameworkReference(tool);
    return reference !== null && selectedFrameworks.some((framework) => (
      frameworkMatchesReference(framework, reference, frameworks)
    ));
  });
}

export function nextArtWorkspaceIndex(
  key: string,
  currentIndex: number,
  workspaceCount: number,
): number | null {
  if (workspaceCount <= 0) return null;
  if (key === "ArrowRight") return (currentIndex + 1) % workspaceCount;
  if (key === "ArrowLeft") return (currentIndex - 1 + workspaceCount) % workspaceCount;
  if (key === "Home") return 0;
  if (key === "End") return workspaceCount - 1;
  return null;
}
