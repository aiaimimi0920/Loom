import assert from "node:assert/strict";
import test from "node:test";
import { setLanguage } from "../../i18n/index.ts";
import { loadProjectionSettings, parseProjectionSettings, projectionDeviceKey, projectionGroupNameError, saveProjectionSettings } from "./projectionSettings.ts";

const peerId = "loom-" + "a".repeat(64);
test("group name limit counts UTF-8 bytes and rejects blank or control-only edits", (context) => {
  context.after(() => setLanguage("en"));
  assert.equal(projectionGroupNameError("\u6d4b".repeat(42) + "ab"), "");
  assert.match(projectionGroupNameError("\u6d4b".repeat(43)), /128 UTF-8 bytes/);
  assert.match(projectionGroupNameError("  "), /cannot be empty/);
  assert.match(projectionGroupNameError("Team" + String.fromCharCode(127)), /control characters/);
  setLanguage("zh-Hans");
  assert.match(projectionGroupNameError("  "), /\u8bbe\u5907\u7ec4\u540d\u79f0/);
});
const document = { storageVersion: 1, revision: 4,
  groups: [{ groupId: "team", name: "Team", members: [{ deviceId: "same" }, { peerId, deviceId: "same" }] }],
  rules: [{ deviceId: "receiver", policy: "reject", whitelist: { devices: [], groups: ["team"], users: [] }, blacklist: [] }],
};
test("settings directory preserves local and foreign identities with partial peers", async (context) => {
  context.mock.method(globalThis, "fetch", async () => Response.json({ ...document, peerDirectory: { status: "partial" },
    targets: [{ route: "shared_loom", deviceId: "same", name: "Local" },
      { route: "offline_peer", deviceId: "opaque", remoteDeviceId: "same", peerId, peerName: "Peer", name: "Remote" }] }));
  const result = await loadProjectionSettings("http://localhost:8765");
  assert.equal(result.directoryStatus, "partial");
  assert.equal(result.devices.length, 2);
  assert.notEqual(projectionDeviceKey(result.devices[0]), projectionDeviceKey(result.devices[1]));
  assert.equal(result.devices[1].deviceId, "same");
  assert.equal(result.settings.groups[0].members.length, 2);
});
test("settings save sends only revisioned administrative fields", async (context) => {
  let sent: unknown;
  context.mock.method(globalThis, "fetch", async (_url: string, options: RequestInit) => {
    assert.equal(options.method, "PUT"); sent = JSON.parse(String(options.body));
    return Response.json({ ...document, revision: 5 });
  });
  const result = await saveProjectionSettings("http://localhost:8765", parseProjectionSettings(document));
  assert.equal(result.revision, 5);
  assert.deepEqual(sent, { expectedRevision: 4, groups: document.groups, rules: document.rules });
});
test("invalid settings versions, duplicate rules and oversized groups are rejected", () => {
  for (const value of [{ ...document, storageVersion: 2 }, { ...document, revision: Number.MAX_SAFE_INTEGER + 1 },
    { ...document, rules: [document.rules[0], document.rules[0]] }, { ...document, groups: Array(33).fill(document.groups[0]) },
    { ...document, rules: [{ ...document.rules[0], policy: "allow_all" }] }]) {
    assert.throws(() => parseProjectionSettings(value));
  }
});
