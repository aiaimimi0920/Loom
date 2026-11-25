import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { desktopStyleSource } from "./desktopStyleSource.ts";

const style = (name: string) => readFileSync(new URL(`../styles/${name}.css`, import.meta.url), "utf8");
const uiStyles = readFileSync(new URL(import.meta.resolve("@neuro/ui/styles.css")), "utf8");

test("feedback and modal skins are owned by the consumed standard package", () => {
  assert.doesNotMatch(desktopStyleSource, /\.app-(?:toast|confirm-)|@keyframes app-(?:toast|confirm)-enter/);
  assert.doesNotMatch(desktopStyleSource, /\.framework-dialog(?:\s|\{|:)/);
  for (const name of ["n-toast", "n-dialog", "n-panel"]) {
    assert.match(uiStyles, new RegExp(`\\.${name}\\s*\\{[^}]*background: var\\(--n-panel\\);`));
  }
});

test("retired workspace controls do not return through the old cascade owners", () => {
  assert.doesNotMatch(style("workspace"), /\.app-|\.hero-|\.status-pill|\.signal-button|\.glass-card/);
  assert.doesNotMatch(style("shell"), /\.ghost-button|\.signal-button|\.section-kicker|\.card-kicker/);
  assert.doesNotMatch(style("art-frameworks"), /\.icon-button|\.danger-button|\.flow-strip|\.terminal-list/);
  assert.doesNotMatch(style("workflow-studio"), /\.studio-|\.field-label|\.palette-art|\.method-chip/);
  assert.doesNotMatch(style("settings-shell") + style("settings-shortcuts"), /\.studio-input|\.ghost-button/);
});

test("unused canvas inspector skins cannot override the actual graph facilities", () => {
  assert.doesNotMatch(desktopStyleSource, /\.otter-canvas-(?:inspector|result-picker|metrics|empty|warning)\b/);
  assert.doesNotMatch(desktopStyleSource, /\.advanced-technical-information\b/);
  const graph = style("otter-canvas-surface");
  assert.match(graph, /\.otter-canvas-node\s*\{[^}]*position: absolute;/);
  assert.match(graph, /\.otter-canvas-node--art\s*\{[^}]*--loom-theme-secondary/);
  assert.match(
    graph,
    /\.otter-canvas-edges path\.otter-canvas-edge--active\s*\{[^}]*stroke: var\(--loom-theme-accent\);/,
  );
  assert.match(style("theme-otter-settings"), /\.otter-canvas-surface\s*\{[^}]*color-scheme: dark;/);
});

test("workspace layout and keyboard accessibility remain application responsibilities", () => {
  assert.match(style("workspace"), /\.workspace-scroll\s*\{[^}]*overflow: auto;/);
  assert.match(
    style("workspace"),
    /\.card-grid\s*\{[^}]*grid-template-columns: repeat\(auto-fit, minmax\(220px, 1fr\)\);/,
  );
  assert.match(style("art-frameworks"), /\.loom-framework-dialog__table > \.n-table-scroll\s*\{[^}]*min-height: 0;/);
  assert.match(style("accessibility"), /@media \(forced-colors: active\)[\s\S]*?stroke: Highlight;/);
});
