import assert from "node:assert/strict";
import test from "node:test";

import {
  approveCapabilityPermissions,
  disableCapability,
  enableCapability,
  getCapabilityCatalog,
  installCatalogCapability,
  installLocalCapability,
  listCapabilityPlugins,
  rollbackCapability,
  uninstallCapability,
  upgradeCapability,
} from "./capabilities.ts";

const jsonResponse = (value: unknown) => new Response(JSON.stringify(value), {
  status: 200,
  headers: { "content-type": "application/json" },
});

test("capability client exposes the complete explicit lifecycle", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => { globalThis.fetch = originalFetch; });
  const requests: Array<{ path: string; body: unknown }> = [];
  const qualifiedId = "publisher.example/text-tools";
  const digestV1 = "1".repeat(64);
  const digestV2 = "2".repeat(64);
  const plugin = {
    qualifiedId,
    publisherId: "publisher.example",
    packageId: "text-tools",
    name: "Text Tools",
    description: "",
    enabledIntent: false,
    status: "installed_disabled",
    runtimeFailures: { count: 0 },
    requestedPermissions: [],
    versions: [],
  };
  globalThis.fetch = async (input, init) => {
    const path = new URL(String(input)).pathname;
    const body = init?.body ? JSON.parse(String(init.body)) : null;
    requests.push({ path, body });
    if (path.endsWith("/catalog/install")) {
      return jsonResponse({ package: { qualifiedId, version: "2.0.0", digest: digestV2 } });
    }
    if (path.endsWith("/install")) {
      return jsonResponse({ package: { qualifiedId, version: "1.0.0", digest: digestV1 } });
    }
    return jsonResponse({ plugin });
  };
  const baseUrl = "http://127.0.0.1:46321";

  await installLocalCapability(baseUrl, "data:application/zip;base64,AA==");
  await approveCapabilityPermissions(baseUrl, qualifiedId, digestV1, ["hook.notice.show"]);
  await enableCapability(baseUrl, qualifiedId, digestV1);
  await disableCapability(baseUrl, qualifiedId);
  await installCatalogCapability(baseUrl, qualifiedId);
  await approveCapabilityPermissions(baseUrl, qualifiedId, digestV2, [
    "hook.notice.show",
    "hook.unit.attachments.read",
  ]);
  await upgradeCapability(baseUrl, qualifiedId, digestV2);
  await rollbackCapability(baseUrl, qualifiedId);
  await uninstallCapability(baseUrl, qualifiedId);

  assert.deepEqual(requests.map((request) => request.path), [
    "/v1/capability-plugins/install",
    `/v1/capability-plugins/${encodeURIComponent(qualifiedId)}/approve`,
    `/v1/capability-plugins/${encodeURIComponent(qualifiedId)}/enable`,
    `/v1/capability-plugins/${encodeURIComponent(qualifiedId)}/disable`,
    "/v1/capability-plugins/catalog/install",
    `/v1/capability-plugins/${encodeURIComponent(qualifiedId)}/approve`,
    `/v1/capability-plugins/${encodeURIComponent(qualifiedId)}/upgrade`,
    `/v1/capability-plugins/${encodeURIComponent(qualifiedId)}/rollback`,
    `/v1/capability-plugins/${encodeURIComponent(qualifiedId)}/uninstall`,
  ]);
  assert.deepEqual(requests[5].body, {
    digest: digestV2,
    permissions: ["hook.notice.show", "hook.unit.attachments.read"],
  });
  assert.deepEqual(requests[6].body, { digest: digestV2 });
});

test("capability clients normalize absent list fields", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => { globalThis.fetch = originalFetch; });
  globalThis.fetch = async (input) => {
    const path = new URL(String(input)).pathname;
    if (path.endsWith("/catalog")) return jsonResponse({ configured: false });
    return jsonResponse({});
  };

  assert.deepEqual(await listCapabilityPlugins("http://127.0.0.1:46321"), {
    plugins: [],
    diskBytesByPlugin: {},
  });
  assert.deepEqual(await getCapabilityCatalog("http://127.0.0.1:46321"), {
    configured: false,
    packages: [],
  });
});

test("capability lifecycle paths encode qualified identities and preserve exact grants", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => { globalThis.fetch = originalFetch; });
  const requests: Array<{ url: string; body: unknown }> = [];
  globalThis.fetch = async (input, init) => {
    requests.push({
      url: String(input),
      body: init?.body ? JSON.parse(String(init.body)) : null,
    });
    return jsonResponse({
      plugin: {
        qualifiedId: "publisher.example/text-tools",
        publisherId: "publisher.example",
        packageId: "text-tools",
        name: "Text Tools",
        description: "",
        enabledIntent: true,
        status: "active",
        runtimeFailures: { count: 0 },
        requestedPermissions: [],
        versions: [],
      },
    });
  };

  const qualifiedId = "publisher.example/text-tools";
  const digest = "a".repeat(64);
  const permissions = ["hook.notice.show", "hook.unit.attachments.write"];
  await approveCapabilityPermissions("http://127.0.0.1:46321", qualifiedId, digest, permissions);
  await enableCapability("http://127.0.0.1:46321", qualifiedId, digest);

  assert.match(requests[0].url, /publisher\.example%2Ftext-tools\/approve$/);
  assert.deepEqual(requests[0].body, { digest, permissions });
  assert.match(requests[1].url, /publisher\.example%2Ftext-tools\/enable$/);
  assert.deepEqual(requests[1].body, { digest });
});
