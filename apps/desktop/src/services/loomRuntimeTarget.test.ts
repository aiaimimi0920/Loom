import assert from "node:assert/strict";
import test from "node:test";
import {
  getLoomConnectionProfile,
  isDesktopTauriShell,
  resolveBrowserDaemonBaseUrl,
} from "./loomRuntimeTarget.ts";

test("web target defaults to browser origin when Tauri shell is absent", async () => {
  assert.equal(isDesktopTauriShell(), false);
  assert.equal(resolveBrowserDaemonBaseUrl(), "http://127.0.0.1:8765");
  const profile = await getLoomConnectionProfile();
  assert.equal(profile.mode, "remote");
  assert.equal(profile.activeUrl, "http://127.0.0.1:8765");
  assert.equal(profile.hasRemoteToken, true);
});
