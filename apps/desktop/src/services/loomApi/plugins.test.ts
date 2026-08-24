// Plugin trust, credential, and publisher identity contracts.
import assert from "node:assert/strict";
import test from "node:test";

import {
  deletePluginCredential,
  getPublisherIdentity,
  listPluginCredentials,
  listPluginTrust,
  registerPublisherIdentity,
  revealPluginCredential,
  revealPublisherPrivateKey,
  revokePluginPublisher,
  rotatePublisherIdentity,
  savePluginCredential,
  setPluginTrustPolicy,
  trustPluginPublisher,
  trustPluginUser,
  untrustPluginUser,
} from "../loomApi.ts";

test("plugin trust credentials and publisher identity helpers preserve their contracts", async (context) => {
  const originalFetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = originalFetch;
  });
  const seen: Array<{ method: string; path: string; body: Record<string, unknown> | null }> = [];
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const value = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const path = new URL(value).pathname;
    const method = String(init?.method ?? "GET").toUpperCase();
    const body = typeof init?.body === "string"
      ? JSON.parse(init.body) as Record<string, unknown>
      : null;
    seen.push({ method, path, body });
    if (path === "/v1/plugin-trust") {
      return Response.json({ publishers: [] });
    }
    if (path === "/v1/plugin-trust/publishers") {
      return Response.json({ publishers: [{ ...body, revoked: false }] });
    }
    if (path === "/v1/plugin-trust/revoke") {
      return Response.json({
        publishers: [{ publisherId: body?.publisherId, keyId: body?.keyId, publicKey: "key", revoked: true }],
      });
    }
    if (path === "/v1/plugin-trust/policy") {
      return Response.json({ publishers: [], policy: body?.policy, trustedPublishers: [] });
    }
    if (path === "/v1/plugin-trust/users") {
      return Response.json({ publishers: [], policy: "require_trusted", trustedPublishers: [body?.userId] });
    }
    if (path === "/v1/plugin-trust/users/remove") {
      return Response.json({ publishers: [], policy: "require_trusted", trustedPublishers: [] });
    }
    if (method === "GET" && path === "/v1/plugin-credentials") {
      return Response.json({ credentials: [] });
    }
    if (method === "POST" && path === "/v1/plugin-credentials") {
      return Response.json({
        credential: { name: body?.name, valueType: body?.valueType, scope: body?.scope, protection: "dpapi" },
      });
    }
    if (path === "/v1/plugin-credentials/delete") {
      return Response.json({ deleted: true });
    }
    if (path === "/v1/plugin-credentials/reveal") {
      return Response.json({
        credential: {
          name: body?.name,
          value: "revealed-secret",
          valueType: "string",
          scope: body?.scope,
          protection: "dpapi",
        },
      });
    }
    if (method === "GET" && path === "/v1/publisher-identity") {
      return Response.json({
        identity: {
          schemaVersion: 1,
          userId: "L0000000000",
          currentKeyId: "key-1",
          publicKey: "public-key",
        },
        hasPrivateKey: true,
      });
    }
    if (path === "/v1/publisher-identity/register" || path === "/v1/publisher-identity/rotate") {
      return Response.json({
        identity: {
          schemaVersion: 1,
          userId: "L0000000000",
          currentKeyId: path.endsWith("rotate") ? "key-2" : "key-1",
          publicKey: "public-key",
        },
        hasPrivateKey: true,
      });
    }
    if (path === "/v1/publisher-identity/private-key") {
      return Response.json({ keyId: "key-1", privateKey: "private-key", publicKey: "public-key" });
    }
    throw new Error(`Unexpected plugin security route: ${method} ${path}`);
  }) as typeof fetch;

  assert.deepEqual(await listPluginTrust("http://127.0.0.1:18773"), {
    schemaVersion: undefined,
    publishers: [],
    policy: "allow_unsigned",
    trustedPublishers: [],
  });
  const trusted = await trustPluginPublisher("http://127.0.0.1:18773", {
    publisherId: "publisher.alpha",
    keyId: "release-key",
    publicKey: "base64-key",
  });
  assert.equal(trusted.publishers[0]?.publisherId, "publisher.alpha");
  const revoked = await revokePluginPublisher(
    "http://127.0.0.1:18773",
    "publisher.alpha",
    "release-key",
  );
  assert.equal(revoked.publishers[0]?.revoked, true);
  assert.equal((await setPluginTrustPolicy("http://127.0.0.1:18773", "require_signed")).policy, "require_signed");
  assert.deepEqual(
    (await trustPluginUser("http://127.0.0.1:18773", "L0000000000")).trustedPublishers,
    ["L0000000000"],
  );
  assert.deepEqual(
    (await untrustPluginUser("http://127.0.0.1:18773", "L0000000000")).trustedPublishers,
    [],
  );
  assert.deepEqual(await listPluginCredentials("http://127.0.0.1:18773"), []);
  const credential = await savePluginCredential("http://127.0.0.1:18773", {
    name: "api_key",
    value: "write-only-secret",
    valueType: "string",
    scope: { frameworkId: "publisher.alpha/shared-framework", artId: "publisher.alpha/shared-art" },
  });
  assert.equal(credential?.name, "api_key");
  assert.equal("value" in (credential ?? {}), false);
  assert.equal(
    (await revealPluginCredential("http://127.0.0.1:18773", "api_key", {
      frameworkId: "publisher.alpha/shared-framework",
      artId: "publisher.alpha/shared-art",
    }))?.value,
    "revealed-secret",
  );
  await deletePluginCredential(
    "http://127.0.0.1:18773",
    "api_key",
    { frameworkId: "publisher.alpha/shared-framework", artId: "publisher.alpha/shared-art" },
  );

  const saveRequest = seen.find((entry) => entry.path === "/v1/plugin-credentials" && entry.method === "POST");
  assert.equal(saveRequest?.body?.value, "write-only-secret");
  assert.equal(saveRequest?.body?.valueType, "string");
  assert.deepEqual(saveRequest?.body?.scope, {
    frameworkId: "publisher.alpha/shared-framework",
    artId: "publisher.alpha/shared-art",
  });
  assert.deepEqual(await getPublisherIdentity("http://127.0.0.1:18773"), {
    identity: {
      schemaVersion: 1,
      userId: "L0000000000",
      currentKeyId: "key-1",
      publicKey: "public-key",
    },
    hasPrivateKey: true,
  });
  assert.equal((await registerPublisherIdentity("http://127.0.0.1:18773")).identity?.currentKeyId, "key-1");
  assert.equal((await rotatePublisherIdentity("http://127.0.0.1:18773")).identity?.currentKeyId, "key-2");
  assert.equal((await revealPublisherPrivateKey("http://127.0.0.1:18773")).privateKey, "private-key");
});
