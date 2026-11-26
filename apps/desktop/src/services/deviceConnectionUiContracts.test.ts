import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = (name: string) => readFileSync(new URL(`../components/devices/${name}`, import.meta.url), "utf8");
const lan = source("LoomLanSettings.tsx");
const connection = source("OtterConnectionSettings.tsx");
const peers = source("ProjectionPeersPanel.tsx");
const settings = source("ProjectionSettingsPanel.tsx");
const groups = source("ProjectionGroupsEditor.tsx");
const rules = source("ProjectionRulesEditor.tsx");
const checklist = source("ProjectionDeviceChecklist.tsx");
const management = source("DeviceManagementPanel.tsx");

test("connection and projection controls use the standard package without parallel native control skins", () => {
  for (const text of [lan, connection, peers, settings, groups, rules, checklist]) {
    assert.match(text, /from "@neuro\/ui"/);
    assert.doesNotMatch(text, /<(?:button|input|select|textarea|details)\b|studio-input|ghost-button|signal-button/);
    assert.doesNotMatch(text, /[\u4e00-\u9fff]/);
  }
  assert.doesNotMatch(management, /DeviceImageTransferPanel|\u53d1\u9001\u56fe\u7247|[\u4e00-\u9fff]/);
});

test("connection code keeps its DOM and reserved slot without participating in validation while disabled", () => {
  assert.match(connection, /<div hidden=\{!passwordEnabled\} inert=\{!passwordEnabled\}>/);
  assert.match(connection, /disabled=\{!passwordEnabled\}/);
  assert.match(connection, /required=\{passwordEnabled && !view\?\.loom\.passwordRequired\}/);
  assert.match(
    source("OtterConnectionSettings.css"),
    /\[hidden\][\s\S]*?display: block !important;\s+visibility: hidden/,
  );
  assert.match(connection, /!passwordEnabled \? \{ password: "" \} : password \? \{ password \} : \{\}/);
  assert.match(connection, /setPassword\(""\);\s+setNotice\(t\("devices\.otterConnection\.savedNotice"\)\)/);
  assert.match(
    source("OtterConnectionSettings.css"),
    /\.otter-connection-settings \.otter-connection-secret-slot > \[hidden\]/,
  );
});

test("long approval labels use a select while short receive policies use a uniquely named radio group", () => {
  assert.match(connection, /<Field label=\{t\("devices\.otterConnection\.approvalLabel"\)\}>\s*<Select/);
  for (const value of ["manual", "lan", "authenticated"]) assert.ok(connection.includes(`value="${value}"`));
  assert.doesNotMatch(connection, /value="password"/);
  assert.match(rules, /useId\(\)/);
  assert.match(rules, /<SegmentedControl</);
  for (const value of ["auto", "confirm", "reject"]) assert.ok(rules.includes(`value: "${value}"`));
  assert.match(rules, /props\.groups\.length > 6 \? \(\s*<MultiSelect/);
  assert.match(rules, /values=\{rule\.whitelist\.groups\}/);
  assert.match(rules, /key=\{rule\.deviceId \+ ":whitelist"\}/);
  assert.match(rules, /key=\{rule\.deviceId \+ ":blacklist"\}/);
});

test("device choices retain exact identity, order and absent candidates during local edits", () => {
  assert.match(checklist, /choices\.length > 6 \? \(\s*<MultiSelect/);
  assert.match(checklist, /orientation="vertical"/);
  assert.match(checklist, /new Map\(props\.devices\.map\(\(device\) => \[projectionDeviceKey\(device\), device\]\)\)/);
  assert.match(checklist, /for \(const device of \[\.\.\.props\.value, \.\.\.dismissed\]\)/);
  assert.match(checklist, /const retained = props\.value\.filter/);
  assert.match(checklist, /props\.onChange\(\[\.\.\.retained, \.\.\.added\]\)/);
  assert.match(checklist, /!checked\.has\(key\) && \(!available\.has\(key\) \|\| props\.value\.length >= 64\)/);
  assert.match(checklist, /title=\{device\.peerId \?/);
  assert.match(checklist, /searchText:/);
  assert.doesNotMatch(checklist, /JSON\.parse|key\.split/);
});

test("projection settings preserve validation, CAS failure drafts and explicit refresh confirmation", () => {
  assert.match(settings, /view\.settings\.groups\.some\(\(group\) => !!projectionGroupNameError\(group\.name\)\)/);
  assert.match(settings, /await requestAppConfirmation\(\{\s*title: t\("devices\.projectionSettings\.confirmRefresh\.title"\)/);
  assert.match(settings, /saveProjectionSettings\(baseUrl, view\.settings\)/);
  assert.match(settings, /groups: rule\.whitelist\.groups\.filter/);
  assert.match(settings, /tf\("devices\.projectionSettings\.error\.unconfirmedDraftKept"/);
  assert.match(groups, /new TextEncoder|projectionGroupNameError\(group\.name\)/);
  assert.match(groups, /key=\{group\.groupId\}/);
  assert.match(peers, /message: trustWarning/);
  assert.match(peers, /if \(!accepted \|\| !validGeneration\(generation\)\) return/);
  assert.match(peers, /readOnly=\{editing\}/);
});

test("loading labels remain fixed and application-owned requests retain cancellation and polling cleanup", () => {
  assert.match(lan, /loading=\{busy\}>\s*\{t\("devices\.common\.apply"\)\}/);
  assert.match(connection, /loading=\{busy\}>\s*\{t\("devices\.common\.save"\)\}/);
  assert.match(settings, /loading=\{operation === "save"\}/);
  assert.match(connection, /clearInterval\(timer\)/);
});

test("long identities remain contextual accessible names rather than expanding action labels", () => {
  assert.match(peers, /aria-label=\{tf\("devices\.projectionPeers\.editPeerAriaLabel", \{ name: peer\.name \}\)\}/);
  assert.match(peers, /aria-label=\{tf\("devices\.projectionPeers\.verifyPeerAriaLabel", \{ name: peer\.name \}\)\}/);
  assert.match(groups, /aria-label=\{tf\("devices\.projectionGroups\.removeAriaLabel", \{ name: group\.name \}\)\}/);
  assert.doesNotMatch(peers, /\{peer\.name\}\s*<\/Button>/);
  assert.doesNotMatch(groups, /\{group\.name\}\s*<\/Button>/);
  assert.match(source("ProjectionSettingsPanel.css"), /\.projection-group,[\s\S]*?min-width: 0/);
});
