import assert from "node:assert/strict";
import test from "node:test";
import { setLanguage } from "../i18n/index.ts";
import { cloudError, naccountRequest, parseNAccount } from "./naccount.ts";

test("NAccount view preserves real configuration state without exposing tokens to UI", () => {
  assert.deepEqual(parseNAccount({ status: "signed_out", configured: false }),
    { status: "signed_out", configured: false, remoteRevoked: undefined });
  assert.deepEqual(parseNAccount({ status: "signed_in", principal: "user", name: "Alice", access_token: "secret" }),
    { status: "signed_in", principal: "user", name: "Alice" });
  assert.throws(() => parseNAccount({ status: "signed_in", principal: "", name: "Alice" }));
});

test("NAccount browser sign-in URL matches request state and rejects legacy Platform protocol", (context) => {
  context.after(() => setLanguage("en"));
  setLanguage("en");
  const requestId = "a".repeat(43);
  const authorizationUrl = `https://naccount-auth.aiaimimi.com/oauth2/v1/authorize?state=${requestId}`;
  assert.equal(parseNAccount({ status: "pending", requestId, authorizationUrl }).status, "pending");
  for (const url of [authorizationUrl.replace("/oauth2/v1/authorize", "/loom/authorize"),
    `${authorizationUrl}&state=other`, authorizationUrl.replace("naccount-auth.aiaimimi.com", "other.test")])
    assert.throws(() => parseNAccount({ status: "pending", requestId, authorizationUrl: url }));
  assert.match(cloudError(new Error("asset_library_distribution_unavailable")), /not available yet/i);
});

test("NAccount distinguishes desktop bridge failures from unconfigured state without leaking request payloads", (context) => {
  context.after(() => setLanguage("en"));
  setLanguage("en");
  for (const [message, expected] of [
    ["/v1/naccount/status returned HTTP/1.1 401 Unauthorized", "authentication failed"],
    ["invalid args for command: missing required key baseUrl", "arguments are invalid"],
    ["Command post_loom_daemon_json not found", "sign-in bridge is unavailable"],
    ["Failed to fetch", "Unable to connect to the local Loom service"],
    ["naccount_response_invalid", "invalid sign-in state"],
  ]) assert.ok(cloudError(new Error(message)).includes(expected));
  assert.equal(cloudError(new Error("unknown sensitive-value")), "Request did not complete. Retry the connection.");
});

test("native status request uses Tauri bridge and recovers real configuration after transient failure", async (context) => {
  const names = ["window", "isTauri", "__TAURI_INTERNALS__"];
  const previous = names.map((name) => [name, Reflect.has(globalThis, name), Reflect.get(globalThis, name)] as const);
  const fetch = globalThis.fetch;
  context.after(() => {
    globalThis.fetch = fetch;
    for (const [name, present, value] of previous) {
      if (present) Reflect.set(globalThis, name, value);
      else Reflect.deleteProperty(globalThis, name);
    }
  });
  let calls = 0;
  Reflect.set(globalThis, "window", globalThis);
  Reflect.set(globalThis, "isTauri", true);
  globalThis.fetch = async () => { throw new Error("Native requests must not fall back to unauthenticated fetch"); };
  Reflect.set(globalThis, "__TAURI_INTERNALS__", {
    invoke: async (command: string, args: unknown) => {
      assert.equal(command, "post_loom_daemon_json");
      assert.deepEqual(args, { baseUrl: "http://127.0.0.1:8765", path: "/v1/naccount/status", body: {} });
      if (++calls === 1) throw "HTTP 401 Unauthorized";
      return { status: "signed_out", configured: true };
    },
  });
  await assert.rejects(naccountRequest("http://127.0.0.1:8765", "status"), /401/);
  const view = await naccountRequest("http://127.0.0.1:8765", "status");
  assert.deepEqual(view, { status: "signed_out", configured: true, remoteRevoked: undefined });
  assert.equal(calls, 2);
});
