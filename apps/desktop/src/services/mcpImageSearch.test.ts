import assert from "node:assert/strict";
import test from "node:test";

import type { HookCanvasNode } from "./hookCanvas.ts";
import type { LoomToolDefinition } from "./loomApi.ts";
import {
  IMAGE_SEARCH_ART_ID,
  IMAGE_SEARCH_SERVER_ID,
  buildImageSearchArtDefinition,
  buildImageSearchExecutionRequest,
  buildImageSearchServerConfig,
  canExecuteHookCanvasNodeManually,
} from "./mcpImageSearch.ts";

test("buildImageSearchServerConfig targets Brave Search MCP with the provided key", () => {
  const server = buildImageSearchServerConfig("brave-key");

  assert.equal(server.id, IMAGE_SEARCH_SERVER_ID);
  assert.equal(server.command, "npx");
  assert.deepEqual(server.args, ["-y", "@brave/brave-search-mcp-server", "--transport", "stdio"]);
  assert.equal(server.env?.BRAVE_API_KEY, "brave-key");
});

test("buildImageSearchArtDefinition declares image output and result_index parameter", () => {
  const tool = buildImageSearchArtDefinition();

  assert.equal(tool.id, IMAGE_SEARCH_ART_ID);
  assert.equal(tool.execution?.type, "mcp");
  assert.equal(tool.execution?.serverId, IMAGE_SEARCH_SERVER_ID);
  assert.equal(tool.execution?.toolName, "brave_image_search");
  assert.equal((tool.outputs?.[0] as { type?: string })?.type, "image");
  assert.equal(
    (tool.params ?? []).some((param) => (param as { id?: string }).id === "result_index"),
    true,
  );
});

test("canExecuteHookCanvasNodeManually only enables generator-like image-search nodes", () => {
  const imageSearchTool = buildImageSearchArtDefinition();
  const resizeTool: LoomToolDefinition = {
    id: "resize",
    name: "Resize",
    execution: { type: "script" },
    inputs: [{ name: "image", type: "image", executionType: "image_path" }],
  };
  const node: HookCanvasNode = {
    id: "node-1",
    kind: "art",
    label: "图片搜索",
    artId: IMAGE_SEARCH_ART_ID,
    x: 0,
    y: 0,
    width: 80,
    height: 80,
    previewAvailable: false,
    previewUrl: null,
    status: "ready",
    minified: false,
    crop: null,
    opacity: 1,
    params: { query: "cats", count: 2 },
  };

  assert.equal(canExecuteHookCanvasNodeManually(node, imageSearchTool), true);
  assert.equal(canExecuteHookCanvasNodeManually({ ...node, artId: "resize" }, resizeTool), false);
});

test("buildImageSearchExecutionRequest merges node params with selected result index", () => {
  const node: HookCanvasNode = {
    id: "node-1",
    kind: "art",
    label: "图片搜索",
    artId: IMAGE_SEARCH_ART_ID,
    x: 0,
    y: 0,
    width: 80,
    height: 80,
    previewAvailable: false,
    previewUrl: null,
    status: "ready",
    minified: false,
    crop: null,
    opacity: 1,
    params: { query: "cats", count: 2 },
  };

  assert.deepEqual(buildImageSearchExecutionRequest(node, 1), {
    nodeId: "node-1",
    artId: IMAGE_SEARCH_ART_ID,
    params: {
      query: "cats",
      count: 2,
      result_index: 1,
    },
  });
});
