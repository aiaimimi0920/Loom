import assert from "node:assert/strict";
import { test } from "node:test";
import { parseAccountView } from "./accountLogin.ts";

test("account UI admits only a matching authorization origin and discards unexpected secret fields", () => {
  const pending = { status: "pending", origin: "https://platform.example", requestId: "a".repeat(64),
    authorizationUrl: "https://platform.example/loom/authorize?requestId=a", fingerprint: "b".repeat(16),
    expiresAtMs: 1_800_000_000_000, seed: "must-not-reach-ui", codeVerifier: "must-not-reach-ui" };
  assert.equal(JSON.stringify(parseAccountView(pending)).includes("must-not-reach-ui"), false);
  for (const url of ["https://evil.example/loom/authorize", "https://u:p@platform.example/loom/authorize", "javascript:alert(1)"]) {
    assert.throws(() => parseAccountView({ ...pending, authorizationUrl: url }));
  }
  assert.throws(() => parseAccountView({ ...pending, expiresAtMs: "tomorrow" }));
});

test("account UI rejects incomplete signed-in responses and preserves offline logout result", () => {
  assert.throws(() => parseAccountView({ status: "signed_in", origin: "https://platform.example", session: {} }));
  assert.deepEqual(parseAccountView({ status: "signed_out", remoteRevoked: false }), { status: "signed_out", remoteRevoked: false });
});
