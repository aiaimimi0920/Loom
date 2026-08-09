import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const appSource = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");
const hookCanvasSource = readFileSync(
  new URL("../components/hook/HookCanvasThumbnail.tsx", import.meta.url),
  "utf8",
);
const styleSource = readFileSync(new URL("../styles.css", import.meta.url), "utf8");

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

test("serializes desktop snapshot refreshes and does not subscribe an offline instance to Hook Bridge", () => {
  assert.match(appSource, /const snapshotSingleFlight = useRef\(createSingleFlightGate\(\)\);/);
  assert.match(appSource, /return await snapshotSingleFlight\.current\.run\(async \(\) => \{/);
  assert.match(
    appSource,
    /snapshot\.connectionState !== "online"[\s\S]*?startHookBridgeWorkflowSync\(\{[\s\S]*?snapshot\.connectionState\]\);/,
  );
});
