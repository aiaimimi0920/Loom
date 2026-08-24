import assert from "node:assert/strict";
import { readdirSync } from "node:fs";
import test from "node:test";

import {
  desktopStyleEntrySource,
  desktopStyleModules,
  desktopStyleSource,
} from "./desktopStyleSource.ts";

const expectedImportOrder = [
  "./styles/foundation.css",
  "./styles/shell.css",
  "./styles/workspace.css",
  "./styles/art-hub.css",
  "./styles/art-frameworks.css",
  "./styles/art-registry.css",
  "./styles/art-editing.css",
  "./styles/workflow-studio.css",
  "./styles/art-creator.css",
  "./styles/hook-canvas-controls.css",
  "./styles/hook-canvas-surface.css",
  "./styles/settings-shell.css",
  "./styles/settings-shortcuts.css",
  "./styles/settings-panels.css",
  "./styles/devices.css",
  "./styles/responsive.css",
  "./styles/tooling-art.css",
  "./styles/tooling-dialogs.css",
  "./styles/theme-shell-art.css",
  "./styles/theme-hook-settings.css",
  "./styles/mcp-hub.css",
  "./styles/mcp-dialogs.css",
  "./styles/accessibility.css",
] as const;

test("loads every owned desktop stylesheet exactly once in cascade order", () => {
  assert.deepEqual(
    desktopStyleModules.map(({ relativePath }) => relativePath),
    expectedImportOrder,
  );
  assert.deepEqual(
    [...readdirSync(new URL("../styles/", import.meta.url))]
      .filter((name) => name.endsWith(".css"))
      .sort(),
    expectedImportOrder.map((relativePath) => relativePath.slice("./styles/".length)).sort(),
  );
});

test("keeps the entry import-only and feature modules independently owned", () => {
  const entryWithoutComments = desktopStyleEntrySource
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^@import\s+"[^"]+";\s*$/gm, "")
    .trim();
  assert.equal(entryWithoutComments, "");
  for (const { relativePath, source } of desktopStyleModules) {
    assert.match(source, /^\/\* Owns [^\n]+ \*\//, `${relativePath} is missing its ownership comment`);
    assert.doesNotMatch(source, /^[ \t]*@import\b/m, `${relativePath} must not own cascade ordering`);
  }
  assert.match(desktopStyleSource, /:root\s*\{/);
  assert.match(desktopStyleSource, /\.mcp-server-dialog\s*\{/);
});
