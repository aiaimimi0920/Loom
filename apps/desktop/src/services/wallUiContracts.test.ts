import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (name: string) => readFileSync(new URL(`../components/devices/${name}.tsx`, import.meta.url), "utf8");
const main = read("WallManagementPanel");
const editor = read("WallLayoutEditor");
const endpoints = read("WallEndpointList");
const geometry = read("WallGeometryFields");
const display = read("WallPresentationControls");
const images = read("WallImageImport");
const css = readFileSync(new URL("../styles/walls.css", import.meta.url), "utf8");

test("wall controls consume the standard package while retaining the domain SVG and native file facility", () => {
  for (const source of [main, editor, endpoints, geometry, display, images]) {
    assert.match(source, /from "@neuro\/ui"/);
    assert.doesNotMatch(source, /<(?:button|select|textarea|details)\b|ghost-button|signal-button|studio-input/);
    assert.doesNotMatch(source, /[\u4e00-\u9fff]/);
  }
  for (const source of [main, editor, endpoints, geometry, display]) assert.doesNotMatch(source, /<input\b/);
  assert.match(editor, /<WallLayoutPreview layout=\{layout\}/);
  assert.match(images, /ref=\{filePicker\}\s+hidden\s+type="file"/);
  assert.match(images, /event\.target\.value = ""/);
});

test("numeric fields preserve unique labels, blank NaN and native step/valueAsNumber semantics", () => {
  assert.match(geometry, /const id = useId\(\)/);
  assert.match(geometry, /<Field label=\{label\}>\s*<Input\s+id=\{id\}/);
  assert.match(geometry, /step=\{integer \? 1 : "any"\}/);
  assert.match(geometry, /value=\{Number\.isFinite\(value\) \? value : ""\}/);
  assert.match(geometry, /onChange\(event\.currentTarget\.valueAsNumber\)/);
  assert.doesNotMatch(geometry, /valueAsNumber \|\||Number\(event|parseInt/);
});

test("small wall navigation keeps pressed buttons and larger catalogs use one controlled select", () => {
  assert.match(main, /state\.layouts\.length > 6 \? \(\s*<Field label=\{t\("devices\.wall\.wallSelectLabel"\)\}>/);
  assert.match(main, /value=\{draft\?\.persisted \? draft\.layout\.wallId : ""\}/);
  assert.match(main, /if \(layout\) void abandon\(\(\) => catalog\.select\(layout\)\)/);
  assert.match(main, /aria-pressed=\{draft\?\.layout\.wallId === layout\.wallId\}/);
  assert.match(main, /aria-label=\{tf\("devices\.wall\.editWallAriaLabel", \{ id: layout\.wallId \}\)\}/);
  assert.match(main, /title=\{layout\.wallId\}/);
});

test("layout rows delegate collapse while preserving stable identity, source refs and permission restrictions", () => {
  assert.match(editor, /<Disclosure\s+className="wall-row"\s+key=\{tile\.tileId\}\s+defaultOpen/);
  assert.match(editor, /<Disclosure\s+className="wall-row"\s+key=\{p\.placementId\}\s+defaultOpen/);
  assert.match(editor, /disabled=\{p\.source\.kind === "image"\}/);
  assert.match(editor, /interactive: event\.target\.checked/);
  assert.match(editor, /current\.filter\(\(item\) => item\.source\.id !== entry\.source\.id\), entry\]\.slice\(-64\)/);
  assert.match(editor, /layout\.placements\.length >= 256/);
  assert.match(editor, /layout\.tiles\.length >= 64/);
  assert.match(editor, /t\("devices\.wall\.editor\.missingSourceNotice"\)/);
});

test("display remains explicit commands and reports actual versioned endpoint receipts", () => {
  assert.doesNotMatch(display, /SegmentedControl|RadioGroup|onValueChange/);
  assert.match(display, /onClick=\{\(\) => void onMode\(wallId, next\)\}/);
  assert.match(display, /endpoint\.presentation\?\.revision === control\.revision/);
  assert.match(display, /endpoint\.presentation\.outcome === "applied"/);
  assert.match(display, /endpoint\.appliedRevision === layout\.revision/);
  assert.match(display, /t\("devices\.wall\.presentation\.inputPausedWarning"\)/);
  assert.match(endpoints, /Boolean\(control \|\| identification\)/);
  assert.match(endpoints, /onClick=\{\(\) => void identify\(endpoint\.endpointId\)\}/);
});

test("wall editing retains conflicts, validation, scheduling guards and explicit destructive confirmation", () => {
  assert.match(main, /parseWallLayout\(draft\.layout\)/);
  assert.match(main, /draft\.baseRevision !== state\.revision/);
  assert.match(main, /catalog\.save\(activationDelayMs\)/);
  assert.match(main, /disabled=\{!schedulable\}/);
  assert.match(main, /activationDelayMs > 0 && !schedulable/);
  assert.match(main, /requestAppConfirmation\(\{\s*title: t\("devices\.wall\.confirmDelete\.title"\)/);
  assert.match(main, /loading=\{catalog\.sourcesLoading\}/);
  assert.match(main, /\{t\("devices\.wall\.saveLayoutAction"\)\}\s*<\/Button>/);
  assert.match(images, /loading=\{busy\}/);
  assert.match(images, /ticket === generation\.current/);
});

test("wall CSS no longer supplies choice, field, disclosure or focus skins", () => {
  assert.doesNotMatch(
    css,
    /\[aria-pressed|\.studio-input|\.wall-checkbox|:focus-visible|accent-color|\.wall-(?:muted|warning|error|online)/,
  );
  assert.match(css, /\.wall-endpoints\s*\{[\s\S]*?grid-auto-rows: max-content/);
  assert.match(css, /\.wall-row\s*\{\s*min-width: 0/);
  assert.match(css, /\.wall-preview__tile/);
});

test("wall confirmations cannot issue an old action after daemon or online context changes", () => {
  assert.match(main, /const generation = useRef\(0\)/);
  assert.match(main, /\[baseUrl, online\]/);
  assert.match(main, /if \(current !== generation\.current\) return;\s*action\(\)/);
  assert.match(
    main,
    /current !== generation\.current\s*\)\s*return;\s*focusAfterClose\.current = true;\s*await catalog\.remove\(\)/,
  );
  assert.match(main, /onClick=\{\(\) => void removeWall\(\)\}/);
});

test("removed or closed wall editors return focus only after the committed update", () => {
  assert.match(main, /useLayoutEffect\(\(\) => \{\s*if \(busy \|\| !focusAfterClose\.current\) return/);
  assert.match(main, /if \(!draft && online\)/);
  assert.match(main, /!createButton\.current\.disabled\) createButton\.current\.focus\(\)/);
  assert.match(main, /else wallSelector\.current\?\.focus\(\)/);
  assert.match(main, /ref=\{createButton\}/);
  assert.doesNotMatch(main, /setTimeout|addEventListener|keydown|keyCode/);
});

test("save feedback tracks only layout saving and ignores old context completion", () => {
  assert.match(main, /const \[saving, setSaving\] = useState\(false\)/);
  assert.match(main, /generation\.current\+\+;\s*setSaving\(false\)/);
  assert.match(main, /await catalog\.save\(activationDelayMs\)/);
  assert.match(main, /if \(current === generation\.current\) setSaving\(false\)/);
  assert.match(main, /loading=\{saving\}/);
  assert.match(main, /onClick=\{\(\) => void saveLayout\(\)\}/);
  assert.doesNotMatch(main, /loading=\{busy\}/);
});
