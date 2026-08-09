import assert from "node:assert/strict";
import test from "node:test";

import { buildAuthoredArtPackage, defaultAuthoringValues } from "./artAuthoring.ts";
import type { LoomFramework } from "./loomApi.ts";

const framework = (id: string, fields: LoomFramework["authoringSchema"] extends infer _ ? NonNullable<LoomFramework["authoringSchema"]>["fields"] : never): LoomFramework => ({
  id,
  qualifiedId: `publisher.test/${id}`,
  name: id,
  description: `${id} framework`,
  installed: true,
  enabled: true,
  ready: true,
  readyDetail: "ready",
  version: "0.1.0",
  authoringSchema: {
    schemaVersion: 1,
    title: `${id} Art`,
    fields: fields ?? [],
    inputs: [{ name: "input", label: "Input", type: "string", executionType: "string" }],
    outputs: [{ name: "result", label: "Result", type: "string", executionType: "string" }],
  },
});

test("builds the four built-in authoring execution contracts", () => {
  const fixtures: Array<[string, Record<string, unknown>, string]> = [
    ["process", { runtimeCommand: "ffmpeg", runtimeArgs: "-i\ninput file\noutput" }, "framework_art"],
    ["cloud_api", { endpoint: "https://api.example.com/v1", method: "post" }, "cloud_api"],
    ["mcp", { serverId: "search", toolName: "query" }, "mcp"],
    ["workflow", { workflowId: "workflow-demo" }, "workflow"],
  ];
  for (const [id, values, executionType] of fixtures) {
    const built = buildAuthoredArtPackage(framework(id, []), {
      id: `${id}-art`,
      name: "",
      description: "",
      values,
    });
    assert.equal(built.tool.execution?.type, executionType);
    assert.equal((built.tool.metadata as { dependencies: { framework: string } }).dependencies.framework, `publisher.test/${id}`);
    if (id === "process") {
      assert.deepEqual(built.tool.execution, {
        type: "framework_art",
        framework: "publisher.test/process",
      });
      assert.deepEqual(built.runtime, {
        protocolVersion: "loom.art.runtime.v1",
        entry: { command: "ffmpeg", args: ["-i", "input file", "output"] },
      });
    }
  }
});

test("third-party framework requires and emits an isolated runtime manifest", () => {
  const built = buildAuthoredArtPackage(framework("custom", [
    { id: "runtimeCommand", label: "Runtime", type: "string", required: true },
    { id: "runtimeArgs", label: "Args", type: "string" },
  ]), {
    id: "custom-art",
    name: "Custom",
    description: "",
    values: { runtimeCommand: "runner.exe", runtimeArgs: '--mode "safe mode"' },
  });
  assert.deepEqual(built.tool.execution, {
    type: "framework_art",
    framework: "publisher.test/custom",
  });
  assert.deepEqual(built.runtime, {
    protocolVersion: "loom.art.runtime.v1",
    entry: { command: "runner.exe", args: ["--mode", "safe mode"] },
  });
});

test("secret authoring fields are persisted only as credential bindings", () => {
  const target = framework("cloud_api", [
    { id: "endpoint", label: "Endpoint", type: "string", required: true },
    { id: "apiCredential", label: "Credential", type: "secret", secret: true },
  ]);
  const built = buildAuthoredArtPackage(target, {
    id: "secure-cloud-art",
    name: "Secure",
    description: "",
    values: { endpoint: "https://api.example.com", apiCredential: "provider-key" },
  });
  const authoring = (built.tool.metadata as { authoring: Record<string, unknown> }).authoring;
  assert.deepEqual(authoring.values, { endpoint: "https://api.example.com" });
  assert.deepEqual(authoring.credentialBindings, { apiCredential: "provider-key" });
});

test("secret authoring fields cannot become execution or runtime configuration", () => {
  assert.throws(() => buildAuthoredArtPackage(framework("process", [
    { id: "runtimeCommand", label: "Runtime", type: "secret", secret: true, required: true },
  ]), {
    id: "unsafe-runtime-art",
    name: "Unsafe",
    description: "",
    values: { runtimeCommand: "plaintext-secret" },
  }), /机密字段 runtimeCommand 不能直接作为 Art 执行配置/);
});

test("default values follow field defaults and boolean shape", () => {
  assert.deepEqual(defaultAuthoringValues([
    { id: "method", label: "Method", type: "enum", default: "POST" },
    { id: "enabled", label: "Enabled", type: "boolean" },
  ]), { method: "POST", enabled: false });
});
