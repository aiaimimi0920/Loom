import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { desktopStyleSource as styleSource } from "./desktopStyleSource.ts";

const appSource = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");
const otterCanvasThumbnailSource = readFileSync(
  new URL("../components/otter/OtterCanvasThumbnail.tsx", import.meta.url),
  "utf8",
);
const otterCanvasToolbarSource = readFileSync(
  new URL("../components/otter/OtterCanvasToolbar.tsx", import.meta.url),
  "utf8",
);
const otterCanvasSource = `${otterCanvasThumbnailSource}\n${otterCanvasToolbarSource}`;

test("removes the standalone workflow workbench and its navigation entry points", () => {
  assert.doesNotMatch(appSource, /id: "workflows"/);
  assert.doesNotMatch(appSource, /工作流工作台/);
  assert.doesNotMatch(appSource, /function WorkflowStudioPanel/);
  assert.doesNotMatch(appSource, /activeSection === "workflows"/);
  assert.doesNotMatch(otterCanvasSource, /打开可视化工作流/);
});

test("keeps save workflow beside the live canvas zoom controls", () => {
  assert.match(
    otterCanvasSource,
    /<div className="otter-canvas-toolbar__controls">[\s\S]*?otter-canvas-save-workflow[\s\S]*?className="otter-canvas-zoom"/,
  );
  assert.match(styleSource, /\.otter-canvas-toolbar__controls \{[\s\S]*?position: relative;[\s\S]*?margin-left: auto;/);
  assert.match(styleSource, /\.otter-canvas-save-workflow \{[\s\S]*?min-width: 96px;/);
});

test("keeps Otter zoom controls readable while preserving the fixed dark canvas", () => {
  assert.match(otterCanvasSource, /className="otter-canvas-zoom__label">缩放<\/span>/);
  assert.match(
    otterCanvasSource,
    /className="otter-canvas-zoom__slider"[\s\S]*?type="range"[\s\S]*?min=\{0\}[\s\S]*?max=\{1000\}[\s\S]*?step=\{1\}[\s\S]*?aria-label="画布缩放"/,
  );
  assert.match(
    otterCanvasSource,
    /className="otter-canvas-zoom__value">\{Math\.round\(scale \* 100\)\}%/,
  );
  assert.match(otterCanvasSource, /aria-pressed=\{showMinimap\}/);
  assert.match(styleSource, /\.otter-canvas-zoom \{[\s\S]*?color: var\(--loom-theme-muted\);/);
  assert.match(styleSource, /\.otter-canvas-zoom__label \{[\s\S]*?color: var\(--loom-theme-muted\);/);
  assert.match(styleSource, /\.otter-canvas-zoom__value \{[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.otter-canvas-zoom__slider \{[\s\S]*?background: color-mix\(in srgb, var\(--loom-theme-text\) 28%, transparent\);[\s\S]*?accent-color: var\(--loom-theme-accent-text\);/);
  assert.match(styleSource, /\.otter-canvas-surface \{[\s\S]*?linear-gradient\(135deg, #101722, #18252b 58%, #11171d\);/);
  assert.match(styleSource, /\.otter-canvas-surface \{[\s\S]*?color-scheme: dark;/);
  assert.doesNotMatch(styleSource, /:root\[data-loom-theme="light"\] \.otter-canvas-surface/);
});

test("serializes desktop snapshot refreshes and does not subscribe an offline instance to Otter Bridge", () => {
  assert.match(appSource, /const snapshotSingleFlight = useRef\(createSingleFlightGate\(\)\);/);
  assert.match(appSource, /await refreshSharedSnapshot\(\{/);
  assert.match(appSource, /flight: snapshotSingleFlight\.current,/);
  assert.match(appSource, /latest: snapshotRequestGate\.current,/);
  assert.match(appSource, /signal: abortSignal,/);
  assert.match(
    appSource,
    /snapshot\.connectionState !== "online"[\s\S]*?startOtterBridgeWorkflowSync\(\{[\s\S]*?snapshot\.connectionState\]\);/,
  );
  assert.doesNotMatch(appSource, /void refresh\(\);\s*void startLocalService\(\);/);
  assert.match(appSource, /otterCanvasRefreshTrigger === null \|\| activeSection !== "otter-bridge"/);
});
