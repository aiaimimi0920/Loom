import assert from "node:assert/strict";
import test from "node:test";

import {
  addWorkflowGraphNode,
  autoTemplateResponse,
  deleteWorkflowGraphNode,
  parseCurlCommand,
  parseRawCommand,
  parseWorkflowYamlLite,
  portsFromMcpToolSchema,
  serializeWorkflowGraphLite,
  updateWorkflowGraphNode,
  type WorkflowGraphLite,
} from "./workflowStudio.ts";

const hasOwn = (value: object, key: string) => Object.prototype.hasOwnProperty.call(value, key);

test("preserves prototype-like JSON keys without changing object prototypes", () => {
  const source = '{"__proto__":{"polluted":"yes"},"constructor":"safe","value":1}';
  const result = autoTemplateResponse(source);
  const output = JSON.parse(result.templatedJson) as Record<string, unknown>;

  assert.equal(Object.getPrototypeOf(output), Object.prototype);
  assert.equal(hasOwn(output, "__proto__"), true);
  assert.equal(hasOwn(output, "constructor"), true);
  assert.equal((Object.prototype as { polluted?: string }).polluted, undefined);
  assert.equal(result.ports.length, 3);
});

test("preserves prototype-like cURL headers as own data properties", () => {
  const result = parseCurlCommand(
    `curl -X POST -H "__proto__: header-value" -H "Content-Type: application/json" --data '{"width":512}' https://example.test/v1`,
  );

  assert.ok(result);
  assert.equal(result.method, "POST");
  assert.equal(result.url, "https://example.test/v1");
  assert.equal(hasOwn(result.headers, "__proto__"), true);
  assert.equal(result.headers.__proto__, "header-value");
  assert.equal(result.suggestedInputs[0]?.name, "width");
  assert.equal(parseCurlCommand("curlfoo https://example.test"), null);
  assert.deepEqual(parseRawCommand("tool\u00a0--flag")?.args, ["--flag"]);
});

test("keeps YAML with keys as own properties instead of invoking __proto__", () => {
  const graph = parseWorkflowYamlLite(`name: safe
nodes:
  - id: step
    uses: tool
    with:
      __proto__: preserved
      constructor: value
`);
  const parameters = graph.nodes[0].with;

  assert.equal(Object.getPrototypeOf(parameters), Object.prototype);
  assert.equal(hasOwn(parameters, "__proto__"), true);
  assert.equal(parameters.__proto__, "preserved");
  assert.equal(parameters.constructor, "value");
});

test("bounds oversized and deeply nested import text without recursive overflow", () => {
  const oversized = "x".repeat(1024 * 1024 + 1);
  assert.equal(parseRawCommand(oversized), null);
  assert.deepEqual(autoTemplateResponse(oversized), { templatedJson: oversized, ports: [] });
  assert.throws(() => parseWorkflowYamlLite(oversized.repeat(4)), /too large/i);

  let nested: unknown = "leaf";
  for (let depth = 0; depth < 70; depth += 1) nested = { next: nested };
  const nestedJson = JSON.stringify(nested);
  assert.deepEqual(autoTemplateResponse(nestedJson), { templatedJson: nestedJson, ports: [] });

  const manyValues = JSON.stringify(new Array(100_001).fill(0));
  assert.deepEqual(autoTemplateResponse(manyValues), { templatedJson: manyValues, ports: [] });
});

test("keeps graph edits immutable and rewires renamed dependencies", () => {
  const graph: WorkflowGraphLite = {
    name: "graph",
    description: "",
    nodes: [
      { id: "a", uses: "first", needs: [], with: {} },
      { id: "b", uses: "second", needs: ["a"], with: { retained: "original" } },
    ],
  };
  const needs = ["external"];
  const parameters = { value: "before" };
  const updated = updateWorkflowGraphNode(graph, "a", { id: "renamed", needs, with: parameters });
  needs.push("mutated");
  parameters.value = "after";

  assert.deepEqual(updated.nodes[0], {
    id: "renamed",
    uses: "first",
    needs: ["external"],
    with: { value: "before" },
  });
  assert.deepEqual(updated.nodes[1].needs, ["renamed"]);
  assert.deepEqual(graph.nodes[1].needs, ["a"]);
  updated.nodes[1].with.retained = "changed";
  assert.equal(graph.nodes[1].with.retained, "original");

  const added = addWorkflowGraphNode(updated, { id: "renamed", uses: "third" });
  assert.equal(added.nodes[2].id, "renamed-2");
  const deleted = deleteWorkflowGraphNode(added, "renamed");
  assert.deepEqual(deleted.nodes.map((node) => node.id), ["b", "renamed-2"]);
  deleted.nodes[0].with.retained = "deleted-copy";
  assert.equal(added.nodes[1].with.retained, "changed");

  assert.throws(
    () => addWorkflowGraphNode(graph, { needs: new Array(4097).fill("source") }),
    /too many dependencies/i,
  );
  const tooManyParameters: Record<string, string> = {};
  for (let index = 0; index < 4097; index += 1) tooManyParameters[`p${index}`] = "value";
  assert.throws(() => updateWorkflowGraphNode(graph, "a", { with: tooManyParameters }), /too many parameters/i);

  const tooManyEdges: WorkflowGraphLite = {
    name: "too-many-edges",
    description: "",
    nodes: Array.from({ length: 5 }, (_, index) => ({
      id: `node-${index}`,
      uses: "tool",
      needs: new Array(4096).fill("source"),
      with: {},
    })),
  };
  assert.throws(() => serializeWorkflowGraphLite(tooManyEdges), /too many dependency edges/i);
});

test("round-trips the public graph and MCP schema facade contracts", () => {
  const graph = parseWorkflowYamlLite(`name: roundtrip
description: contract
nodes:
  - id: source
    uses: sticker
  - id: result
    uses: resize
    needs: [source]
    with:
      width: 512
`);
  assert.deepEqual(parseWorkflowYamlLite(serializeWorkflowGraphLite(graph)), graph);

  const mcp = portsFromMcpToolSchema({
    name: "screenshot_page",
    inputSchema: {
      properties: {
        url: { type: "string", format: "uri" },
        full_page: { type: "boolean", default: true },
      },
    },
  });
  assert.equal(mcp?.suggestedInputs[0].executionType, "image_path");
  assert.equal(mcp?.suggestedInputs[1].executionType, "bool");
  assert.equal(mcp?.suggestedOutputs[0].executionType, "image_buffer");
});
