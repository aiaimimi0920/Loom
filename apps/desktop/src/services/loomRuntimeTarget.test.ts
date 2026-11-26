import assert from "node:assert/strict";
import test from "node:test";
import {
  getLoomConnectionProfile,
  isDesktopTauriShell,
  resolveBrowserDaemonBaseUrl,
} from "./loomRuntimeTarget.ts";

test("non-browser execution defaults to loopback when Tauri shell is absent", async () => {
  assert.equal(isDesktopTauriShell(), false);
  assert.equal(resolveBrowserDaemonBaseUrl(), "http://127.0.0.1:8765");
  const profile = await getLoomConnectionProfile();
  assert.equal(profile.mode, "remote");
  assert.equal(profile.activeUrl, "http://127.0.0.1:8765");
  assert.equal(profile.hasRemoteToken, true);
});

test("web console uses its HTTPS browser origin, not the local daemon", async () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "window");
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { protocol: "https:", origin: "https://loom.example:9443" } },
  });
  try {
    assert.equal(isDesktopTauriShell(), false);
    assert.equal(resolveBrowserDaemonBaseUrl(), "https://loom.example:9443");
    assert.equal((await getLoomConnectionProfile()).activeUrl, "https://loom.example:9443");
  } finally {
    if (original) Object.defineProperty(globalThis, "window", original);
    else Reflect.deleteProperty(globalThis, "window");
  }
});
