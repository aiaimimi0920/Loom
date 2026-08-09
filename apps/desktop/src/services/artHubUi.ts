import type { ArtStoreEntry, LoomFramework, LoomToolDefinition } from "./loomApi";

export type ArtWorkspaceId = "registry" | "store" | "security";

export interface ArtWorkspaceItem {
  id: ArtWorkspaceId;
  label: string;
}

export type ArtDisplayLocale = "zh-CN" | "en-US";

export interface ArtPublisherIdentity {
  id: string;
  name: string;
  icon: string | null;
  initials: string;
}

export interface ArtDisplayIdentity {
  locale: ArtDisplayLocale;
  publisher: ArtPublisherIdentity;
  englishName: string;
  globalId: string | null;
  localizedName: string;
  localizedDescription: string;
}

export const artWorkspaceItems: ArtWorkspaceItem[] = [
  { id: "registry", label: "注册表" },
  { id: "store", label: "商店" },
  { id: "security", label: "密钥与安全" },
];

const officialFrameworkExecutionTypes = new Set([
  "process",
  "cloud_api",
  "mcp",
  "workflow",
]);

const officialFrameworkDisplayNames: Readonly<Record<string, string>> = {
  cloud_api: "云端",
  mcp: "MCP",
  process: "脚本",
  workflow: "流程",
};

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function nonEmptyString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function artPublisherInitials(value: string): string {
  const words = value.trim().split(/\s+/u).filter(Boolean);
  if (words.length > 1) {
    return words.slice(0, 2).map((word) => Array.from(word)[0] ?? "").join("").toUpperCase();
  }
  return (Array.from(words[0] ?? "Art")[0] ?? "A").toUpperCase();
}

function localizedMetadataValue(
  localization: Record<string, unknown> | null,
  collectionName: "names" | "descriptions",
  fieldName: "name" | "description",
  locale: ArtDisplayLocale,
): string | null {
  if (!localization) return null;
  const directLocale = asRecord(localization[locale])
    ?? asRecord(localization[locale.startsWith("zh") ? "zh" : "en"]);
  const directValue = nonEmptyString(directLocale?.[fieldName]);
  if (directValue) return directValue;

  const values = asRecord(localization[collectionName]);
  const exactValue = nonEmptyString(values?.[locale])
    ?? nonEmptyString(values?.[locale.startsWith("zh") ? "zh" : "en"]);
  if (exactValue) return exactValue;

  const defaultLocale = nonEmptyString(localization.defaultLocale);
  return defaultLocale ? nonEmptyString(values?.[defaultLocale]) : null;
}

export function artDisplayLocale(language?: string | null): ArtDisplayLocale {
  return language?.trim().toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

export function artPublisherIconSource(icon: string | null | undefined): string | null {
  const normalized = icon?.trim();
  if (!normalized) return null;
  return /^(?:https:\/\/|data:image\/(?:png|jpe?g|webp|gif);base64,)/iu.test(normalized)
    ? normalized
    : null;
}

export function artPackageIdentity(tool: LoomToolDefinition): string | null {
  const metadata = asRecord(tool.metadata);
  const artPackage = asRecord(metadata?.artPackage);
  return nonEmptyString(artPackage?.qualifiedId);
}

export function artDisplayIdentity(
  tool: LoomToolDefinition,
  globalId?: string | null,
  language?: string | null,
): ArtDisplayIdentity {
  const locale = artDisplayLocale(language);
  const metadata = asRecord(tool.metadata);
  const packageSecurity = asRecord(metadata?.packageSecurity);
  const publisher = asRecord(metadata?.publisher) ?? asRecord(packageSecurity?.publisher);
  const authoring = asRecord(metadata?.authoring);
  const art = asRecord(metadata?.art);
  const artPackage = asRecord(metadata?.artPackage);
  const localization = asRecord(metadata?.localization);
  const publisherId = nonEmptyString(publisher?.id)
    ?? nonEmptyString(authoring?.owner)
    ?? "local.user";
  const publisherName = nonEmptyString(publisher?.name)
    ?? nonEmptyString(authoring?.owner)
    ?? (locale === "zh-CN" ? "本地用户" : "Local user");
  const publisherIcon = nonEmptyString(publisher?.icon)
    ?? nonEmptyString(metadata?.publisherIcon);
  const localizedName = localizedMetadataValue(localization, "names", "name", locale)
    ?? tool.name
    ?? tool.id;
  const localizedDescription = localizedMetadataValue(
    localization,
    "descriptions",
    "description",
    locale,
  ) ?? tool.description ?? "";
  const englishName = nonEmptyString(art?.englishName)
    ?? tool.id;
  const resolvedGlobalId = [
    nonEmptyString(art?.globalId),
    nonEmptyString(artPackage?.globalId),
    nonEmptyString(metadata?.globalId),
    nonEmptyString(globalId),
  ].find((candidate): candidate is string => Boolean(candidate && /^NA\d{11}$/u.test(candidate))) ?? null;

  return {
    locale,
    publisher: {
      id: publisherId,
      name: publisherName,
      icon: publisherIcon,
      initials: artPublisherInitials(publisherIcon && !artPublisherIconSource(publisherIcon)
        ? publisherIcon
        : publisherName),
    },
    englishName,
    globalId: resolvedGlobalId,
    localizedName,
    localizedDescription,
  };
}

export function frameworkIdentity(framework: LoomFramework): string {
  return framework.qualifiedId || framework.id;
}

export function officialFrameworkDisplayName(reference: string | null | undefined): string | null {
  const normalized = reference?.trim();
  if (!normalized) return null;
  const id = normalized.startsWith("neuro.official/")
    ? normalized.slice("neuro.official/".length)
    : normalized.includes("/")
      ? null
      : normalized;
  return id ? officialFrameworkDisplayNames[id] ?? null : null;
}

export function frameworkFilterLabel(framework: LoomFramework): string {
  const officialName = officialFrameworkDisplayName(frameworkIdentity(framework));
  const fallbackName = framework.name.trim().replace(/\s*框架\s*$/u, "").trim();
  return officialName ?? (fallbackName || framework.id);
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

export function isLocallyAuthoredTool(tool: LoomToolDefinition): boolean {
  const metadata = asRecord(tool.metadata);
  const packageSecurity = asRecord(metadata?.packageSecurity);
  return asRecord(metadata?.authoring) !== null && asRecord(packageSecurity?.publisher) === null;
}

export function filterArtStoreEntries(
  entries: ArtStoreEntry[],
  frameworks: LoomFramework[],
  selectedFrameworkIds: ReadonlySet<string> | null,
  searchText: string,
  officialOnly: boolean,
): ArtStoreEntry[] {
  const query = searchText.trim().toLocaleLowerCase();
  const selectedFrameworks = selectedFrameworkIds === null
    ? null
    : frameworks.filter((framework) => selectedFrameworkIds.has(frameworkIdentity(framework)));

  return entries.filter((entry) => {
    if (officialOnly && entry.official !== true) return false;
    if (selectedFrameworks !== null) {
      const reference = nonEmptyString(entry.framework);
      if (!reference || !selectedFrameworks.some((framework) => (
        frameworkMatchesReference(framework, reference, frameworks)
      ))) {
        return false;
      }
    }
    if (!query) return true;
    return [
      entry.id,
      entry.qualifiedId,
      entry.globalId,
      entry.name,
      entry.description,
      entry.framework,
    ].some((value) => value?.toLocaleLowerCase().includes(query));
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
