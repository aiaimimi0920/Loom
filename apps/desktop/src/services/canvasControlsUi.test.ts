import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { desktopStyleSource } from "./desktopStyleSource.ts";

const read = (name: string) => readFileSync(new URL(`../components/otter/${name}.tsx`, import.meta.url), "utf8");
const toolbar = read("OtterCanvasToolbar"),
  workflow = read("OtterCanvasWorkflowInterface");
const exposure = read("OtterCanvasParamExposure"),
  rename = read("OtterCanvasRenameDialog");
const css = readFileSync(new URL("../styles/otter-canvas-controls.css", import.meta.url), "utf8");

test("canvas peripheral controls consume the standard implementation without adopting graph ownership", () => {
  for (const source of [toolbar, workflow, exposure, rename]) {
    assert.match(source, /from "@neuro\/ui"/);
    assert.doesNotMatch(source, /<(?:input|select|button)\b|ghost-button|signal-button|success-text|error-text/);
  }
  assert.match(workflow, /<Panel\s+className="otter-canvas-workflow-io"/);
  assert.match(css, /\.otter-canvas-surface \{[\s\S]*?isolation: isolate/);
});

test("canvas zoom keeps its nonlinear mapping and original native range contract", () => {
  assert.match(toolbar, /scaleToSliderValue\(scale, MIN_SCALE, MAX_SCALE\)/);
  assert.match(toolbar, /sliderValueToScale\(Number\(event\.target\.value\), MIN_SCALE, MAX_SCALE\)/);
  assert.match(toolbar, /min=\{0\}\s+max=\{1000\}\s+step=\{1\}/);
  assert.match(toolbar, /aria-valuetext=\{`\$\{Math\.round\(scale \* 100\)\}%`\}/);
});

test("canvas actions keep fixed visible labels through loading and minimap selection", () => {
  assert.match(toolbar, /loading=\{saving\}/);
  assert.match(toolbar, /t\("otter\.canvas\.saveWorkflow"\)/);
  assert.match(toolbar, /\{resolvedSaveLabel\}\s*<\/Button>/);
  assert.match(toolbar, /aria-pressed=\{showMinimap\}[\s\S]*?>\s*\{t\("otter\.toolbar\.minimap"\)\}\s*<\/Button>/);
  assert.match(workflow, /loading=\{workflowBusyAction === "desktop"\}/);
  assert.match(workflow, /loading=\{workflowBusyAction === "art"\}/);
  assert.doesNotMatch(toolbar + workflow, /saving\.\.\.|adding\.\.\.|processing\.\.\.|hideMinimap/i);
});

test("exposure flags preserve keyed constant editors and exact parameter value semantics", () => {
  assert.match(exposure, /key=\{row\.key\}/);
  assert.match(exposure, /const value = values\[row\.key\] \?\? ""/);
  assert.match(exposure, /checked=\{isExposed\}/);
  assert.match(exposure, /onExposureChange\(row\.key, event\.target\.checked\)/);
  assert.equal((exposure.match(/disabled=\{isExposed\}/g) || []).length, 3);
  assert.match(exposure, /value=\{value \|\| "false"\}/);
  assert.match(exposure, /min=\{row\.param\.min\}\s+max=\{row\.param\.max\}/);
  assert.match(exposure, /row\.param\.step \?\? \(uiType === "int" \? 1 : undefined\)/);
  assert.equal((exposure.match(/onValueChange\(row\.key, event\.target\.value\)/g) || []).length, 3);
  assert.doesNotMatch(exposure, /\{!isExposed/);
});

test("rename delegates modal lifetime and only submits a nonempty idle form", () => {
  assert.match(rename, /<Dialog\s+title=\{t\("otter\.rename\.title"\)\}/);
  assert.match(rename, /busy=\{busy\}/);
  assert.match(rename, /initialFocusRef=\{inputRef\}/);
  assert.match(rename, /if \(!busy && value\.trim\(\)\) onSubmit\(\)/);
  assert.match(rename, /type="submit"/);
  assert.doesNotMatch(rename, /addEventListener|keydown|setTimeout|previousFocus|body\.style|createPortal|onKeyDown/);
});

test("canvas peripheral CSS cannot restyle standard choices, fields, slider or modal", () => {
  assert.doesNotMatch(css, /\.otter-canvas-(?:param-expose__toggle|rename-dialog|rename-backdrop|workflow-delete)/);
  assert.doesNotMatch(
    css,
    /otter-canvas-zoom__slider(?:\s*\{|:)|accent-color|\.otter-canvas-param-expose__value:disabled/,
  );
  assert.match(css, /\.otter-canvas-param-expose__row\s*\{[\s\S]*?minmax\(0, 1fr\) minmax\(0, 1fr\)/);
  assert.match(css, /@media \(max-width: 700px\)/);
  assert.doesNotMatch(
    desktopStyleSource,
    /\.otter-canvas-zoom input\[type=["']range["']\]|\.otter-canvas-minimap-toggle/,
  );
  assert.match(css, /\.otter-canvas-zoom__value\s*\{[^}]*color: var\(--loom-theme-text\)/);
});

test("derived input metadata scrolls independently and cannot push parameter editors on selection", () => {
  assert.match(css, /\.otter-canvas-workflow-io__group\s*\{[^}]*block-size: 9rem/);
  assert.match(css, /\.otter-canvas-workflow-io__list\s*\{[^}]*grid-auto-rows: max-content;[^}]*overflow: auto/);
  assert.match(workflow, /tabIndex=\{0\} aria-label=\{t\("otter\.interface\.inputsAria"\)\}/);
  assert.match(workflow, /tabIndex=\{0\} aria-label=\{t\("otter\.interface\.outputsAria"\)\}/);
});

test("workflow deletion restores focus after commit and explicit selection cancels that intent", () => {
  assert.match(toolbar, /if \(workflowBusy \|\| !focusAfterDelete\.current\) return/);
  assert.match(toolbar, /if \(isLive\) workflowSelect\.current\?\.focus\(\)/);
  assert.match(toolbar, /focusAfterDelete\.current = false;\s*onWorkflowChange\(event\.target\.value\)/);
  assert.match(toolbar, /focusAfterDelete\.current = true;\s*onDelete\(\)/);
  assert.doesNotMatch(toolbar, /setTimeout|addEventListener/);
});

test("selected node properties use a standard panel without hiding full node identity", () => {
  const properties = read("OtterCanvasNodeProperties");
  assert.match(properties, /<Panel\s+className="otter-canvas-node-props"/);
  assert.match(properties, /aria-live="polite"/);
  assert.match(properties, /NODE_KIND_LABELS\[node\.kind\] \?\? node\.kind/);
  assert.match(properties, /Math\.round\(node\.width\)\} × \{Math\.round\(node\.height\)/);
  assert.match(properties, /title=\{node\.artId\}/);
  assert.match(properties, /title=\{node\.id\}/);
  const block = css.match(/\.otter-canvas-node-props\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.doesNotMatch(block, /background|border|padding|box-shadow|color/);
  const overrides = readFileSync(new URL("../styles/tooling-dialogs.css", import.meta.url), "utf8");
  assert.doesNotMatch(overrides, /\.otter-canvas-node-props/);
});

test("parent canvas error feedback uses standard notice and preserves its snapshot condition", () => {
  const thumbnail = read("OtterCanvasThumbnail");
  assert.match(thumbnail, /error && !snapshot \? <Notice tone="danger">\{error\}<\/Notice> : null/);
  assert.doesNotMatch(thumbnail, /error-text/);
  assert.match(css, /\.otter-canvas-thumbnail > \.n-notice\s*\{[^}]*overflow-wrap: anywhere/);
});

test("optional tools use stable empty metadata instead of causing parameter-effect update loops", () => {
  const thumbnail = read("OtterCanvasThumbnail");
  assert.match(thumbnail, /const EMPTY_TOOLS: LoomToolDefinition\[\] = \[\]/);
  assert.match(thumbnail, /tools = EMPTY_TOOLS/);
  assert.doesNotMatch(thumbnail, /tools = \[\]/);
});
