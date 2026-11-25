import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import { desktopStyleSource } from "./desktopStyleSource.ts";
const uiStyles = readFileSync(new URL(import.meta.resolve("@neuro/ui/styles.css")), "utf8");

test("provides a visible keyboard focus fallback for native interactive controls", () => {
  assert.match(
    uiStyles,
    /\.neuro-root :where\(button, a, input, select, textarea, summary, \[tabindex\]\):focus-visible\s*\{[^}]*outline: 2px solid var\(--n-signal\);/,
  );
});

test("disables cross-feature motion when the operating system requests it", () => {
  assert.match(
    uiStyles,
    /@media \(prefers-reduced-motion: reduce\)\s*\{[^}]*\.neuro-root \*[^}]*transition: none !important;/,
  );
  assert.match(
    uiStyles,
    /@media \(prefers-reduced-motion: reduce\)\s*\{[^}]*\.neuro-root \*[^}]*animation: none !important;/,
  );
});

test("keeps confirmation dialogs scrollable at high zoom and exposes forced-color focus", () => {
  assert.match(uiStyles, /\.n-overlay\s*\{[^}]*position: fixed;[^}]*inset: 0;/);
  assert.match(uiStyles, /\.n-dialog\s*\{[^}]*max-height: calc\(100dvh - 32px\);[^}]*overflow-y: auto;/);
  assert.match(uiStyles, /@media \(forced-colors: active\)\s*\{[\s\S]*?outline-color: Highlight;/);
  assert.match(desktopStyleSource, /@media \(forced-colors: active\)\s*\{[^}]*stroke: Highlight;/);
});
