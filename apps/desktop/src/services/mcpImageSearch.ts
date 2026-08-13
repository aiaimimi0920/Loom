import type { HookCanvasNode } from "./hookCanvas.ts";
import type { LoomMcpServer, LoomToolDefinition } from "./loomApi.ts";

export const IMAGE_SEARCH_ART_ID = "custom-image-search";
export const IMAGE_SEARCH_SERVER_ID = "brave-search";

const isRecord = (value: unknown): value is Record<string, unknown> =>
  Boolean(value) && typeof value === "object" && !Array.isArray(value);

const isImageInputPort = (value: unknown) => {
  if (!isRecord(value)) return false;
  const type = typeof value.type === "string" ? value.type : "";
  const executionType =
    typeof value.executionType === "string"
      ? value.executionType
      : typeof value.execution_type === "string"
        ? value.execution_type
        : "";
  return type === "image" || executionType.startsWith("image_");
};

export function buildImageSearchServerConfig(
  braveApiKey: string,
  existing?: LoomMcpServer,
): LoomMcpServer {
  return {
    id: existing?.id || IMAGE_SEARCH_SERVER_ID,
    name: "Brave Search",
    description: "通过 Brave Search 搜索网页、本地、图片、视频、新闻和摘要结果。",
    command: "npx",
    args: ["-y", "@brave/brave-search-mcp-server", "--transport", "stdio"],
    env: {
      ...(existing?.env || {}),
      BRAVE_API_KEY: braveApiKey,
    },
    enabled: existing?.enabled ?? true,
  };
}

export function buildImageSearchArtDefinition(
  serverId = IMAGE_SEARCH_SERVER_ID,
): LoomToolDefinition {
  return {
    id: IMAGE_SEARCH_ART_ID,
    name: "图片搜索",
    description: "通过 Brave Search MCP 搜索图片并返回可预览结果。",
    enabled: true,
    execution: {
      type: "mcp",
      serverId,
      toolName: "brave_image_search",
    },
    outputs: [
      {
        name: "output",
        label: "output",
        type: "image",
        execution_type: "image_buffer",
      },
    ],
    params: [
      {
        id: "query",
        label: "搜索词",
        widget: "text",
        default: "",
        data_type: "string",
      },
      {
        id: "count",
        label: "数量",
        widget: "number",
        default: 4,
        min: 1,
        max: 20,
        step: 1,
        data_type: "number",
      },
      {
        id: "safesearch",
        label: "安全搜索",
        widget: "text",
        default: "off",
        data_type: "string",
      },
      {
        id: "spellcheck",
        label: "拼写检查",
        widget: "checkbox",
        default: true,
        data_type: "bool",
      },
      {
        id: "result_index",
        label: "结果索引",
        widget: "number",
        default: 0,
        min: 0,
        max: 19,
        step: 1,
        data_type: "number",
      },
    ],
    metadata: {
      dependencies: {
        framework: "mcp",
      },
      presentation: {
        icon: "#1677ff",
      },
    },
  };
}

export function canExecuteHookCanvasNodeManually(
  node: HookCanvasNode,
  tool?: LoomToolDefinition,
): boolean {
  if (node.kind !== "art" || !node.artId || !tool || tool.id !== node.artId) {
    return false;
  }
  const inputs = Array.isArray(tool.inputs) ? tool.inputs : [];
  return !inputs.some(isImageInputPort);
}

export function buildImageSearchExecutionRequest(
  node: HookCanvasNode,
  selectedIndex?: number,
): {
  nodeId: string;
  artId: string;
  params: Record<string, unknown>;
} {
  const params = isRecord(node.params) ? { ...node.params } : {};
  if (typeof selectedIndex === "number" && Number.isFinite(selectedIndex) && selectedIndex >= 0) {
    params.result_index = Math.floor(selectedIndex);
  }
  return {
    nodeId: node.id,
    artId: node.artId || IMAGE_SEARCH_ART_ID,
    params,
  };
}
