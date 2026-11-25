import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { desktopStyleSource } from "./desktopStyleSource.ts";

const foundation = readFileSync(new URL("../styles/foundation.css", import.meta.url), "utf8");

test("Loom aliases consume the standard Provider palette without a second preset", () => {
  const block = foundation.match(/\.neuro-root\s*\{([^}]+)\}/)?.[1];
  assert.ok(block);
  const aliases = [...block.matchAll(/--loom-([\w-]+):\s*var\(--n-([\w-]+)\);/g)];
  assert.equal(aliases.length, 19);
  assert.match(block, /--loom-theme-panel: var\(--n-panel\);/);
  assert.match(block, /--loom-theme-danger: var\(--n-danger\);/);
  assert.doesNotMatch(foundation, /--neuro-|--n-[\w-]+\s*:|#[\da-f]{3,8}\b|rgba?\(/i);
  const documentBox = foundation.match(/html,\s*body,\s*#root\s*\{([^}]+)\}/)?.[1];
  assert.ok(documentBox);
  assert.doesNotMatch(documentBox, /var\(--(?:loom|n)-/);
  for (const name of ["body", ":root"]) {
    const blocks = [...foundation.matchAll(new RegExp(`${name}\\s*\\{([^}]+)\\}`, "g"))];
    assert.ok(blocks.length);
    for (const match of blocks) assert.doesNotMatch(match[1], /var\(--(?:loom|n)-/);
  }
});

test("old form and feedback skins cannot take over standard controls", () => {
  assert.doesNotMatch(
    desktopStyleSource,
    /studio-input|studio-textarea|studio-json|ghost-button|signal-button|\.icon-button/,
  );
  assert.doesNotMatch(desktopStyleSource, /\.otter-shortcut-add-art[^}]*:disabled/);
  assert.doesNotMatch(desktopStyleSource, /\.workflow-node|\.settings-link|\.port-summary|\.mcp-package-card/);
  assert.doesNotMatch(foundation, /:is\([^)]*\binput\b|:not\([^)]*\.n-input/);
});
