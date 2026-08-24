import assert from "node:assert/strict";
import test from "node:test";

import { desktopStyleSource } from "./desktopStyleSource.ts";

test("provides a visible keyboard focus fallback for native interactive controls", () => {
  assert.match(
    desktopStyleSource,
    /:where\(button, a\[href\], input, select, textarea, summary, \[role="button"\]\):focus-visible \{[\s\S]*?outline: 2px solid var\(--loom-theme-accent-text\);/,
  );
});

test("disables cross-feature motion when the operating system requests it", () => {
  assert.match(
    desktopStyleSource,
    /@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?transition-duration: 0\.01ms !important;[\s\S]*?\.window-control--loading \.shell-icon,[\s\S]*?\.hook-shortcut-list,[\s\S]*?\.mcp-busy-indicator \{[\s\S]*?animation: none !important;/,
  );
});

test("keeps confirmation dialogs scrollable at high zoom and exposes forced-color focus", () => {
  assert.match(
    desktopStyleSource,
    /\.app-confirm-backdrop \{[\s\S]*?overflow-y: auto;/,
  );
  assert.match(
    desktopStyleSource,
    /\.app-confirm-dialog \{[\s\S]*?max-height: calc\(100dvh - 32px\);[\s\S]*?overflow-y: auto;/,
  );
  assert.match(
    desktopStyleSource,
    /@media \(forced-colors: active\) \{[\s\S]*?outline-color: Highlight;/,
  );
});
