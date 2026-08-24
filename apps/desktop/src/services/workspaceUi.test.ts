import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { desktopStyleSource as styleSource } from "./desktopStyleSource.ts";

const appSource = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");
const hookCanvasThumbnailSource = readFileSync(
  new URL("../components/hook/HookCanvasThumbnail.tsx", import.meta.url),
  "utf8",
);
const hookCanvasToolbarSource = readFileSync(
  new URL("../components/hook/HookCanvasToolbar.tsx", import.meta.url),
  "utf8",
);
const hookCanvasSource = `${hookCanvasThumbnailSource}\n${hookCanvasToolbarSource}`;

test("removes the standalone workflow workbench and its navigation entry points", () => {
  assert.doesNotMatch(appSource, /id: "workflows"/);
  assert.doesNotMatch(appSource, /工作流工作台/);
  assert.doesNotMatch(appSource, /function WorkflowStudioPanel/);
  assert.doesNotMatch(appSource, /activeSection === "workflows"/);
  assert.doesNotMatch(hookCanvasSource, /打开可视化工作流/);
});

test("keeps save workflow beside the live canvas zoom controls", () => {
  assert.match(
    hookCanvasSource,
    /<div className="hook-canvas-toolbar__controls">[\s\S]*?hook-canvas-save-workflow[\s\S]*?className="hook-canvas-zoom"/,
  );
  assert.match(styleSource, /\.hook-canvas-toolbar__controls \{[\s\S]*?position: relative;[\s\S]*?margin-left: auto;/);
  assert.match(styleSource, /\.hook-canvas-save-workflow \{[\s\S]*?min-width: 96px;/);
});

test("keeps Hook zoom controls readable while preserving the fixed dark canvas", () => {
  assert.match(hookCanvasSource, /className="hook-canvas-zoom__label">缩放<\/span>/);
  assert.match(
    hookCanvasSource,
    /className="hook-canvas-zoom__slider"[\s\S]*?type="range"[\s\S]*?min=\{0\}[\s\S]*?max=\{1000\}[\s\S]*?step=\{1\}[\s\S]*?aria-label="画布缩放"/,
  );
  assert.match(
    hookCanvasSource,
    /className="hook-canvas-zoom__value">\{Math\.round\(scale \* 100\)\}%/,
  );
  assert.match(hookCanvasSource, /aria-pressed=\{showMinimap\}/);
  assert.match(styleSource, /\.hook-canvas-zoom \{[\s\S]*?color: var\(--loom-theme-muted\);/);
  assert.match(styleSource, /\.hook-canvas-zoom__label \{[\s\S]*?color: var\(--loom-theme-muted\);/);
  assert.match(styleSource, /\.hook-canvas-zoom__value \{[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.hook-canvas-zoom__slider \{[\s\S]*?background: color-mix\(in srgb, var\(--loom-theme-text\) 28%, transparent\);[\s\S]*?accent-color: var\(--loom-theme-accent-text\);/);
  assert.match(styleSource, /\.hook-canvas-surface \{[\s\S]*?linear-gradient\(135deg, #101722, #18252b 58%, #11171d\);/);
  assert.match(styleSource, /\.hook-canvas-surface \{[\s\S]*?color-scheme: dark;/);
  assert.doesNotMatch(styleSource, /:root\[data-loom-theme="light"\] \.hook-canvas-surface/);
});

test("serializes desktop snapshot refreshes and does not subscribe an offline instance to Hook Bridge", () => {
  assert.match(appSource, /const snapshotSingleFlight = useRef\(createSingleFlightGate\(\)\);/);
  assert.match(appSource, /return await snapshotSingleFlight\.current\.run\(async \(\) => \{/);
  assert.match(
    appSource,
    /snapshot\.connectionState !== "online"[\s\S]*?startHookBridgeWorkflowSync\(\{[\s\S]*?snapshot\.connectionState\]\);/,
  );
  assert.doesNotMatch(appSource, /void refresh\(\);\s*void startLocalService\(\);/);
  assert.match(appSource, /hookCanvasRefreshTrigger === null \|\| activeSection !== "hook-bridge"/);
});
