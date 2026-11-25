import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = (name: string) => readFileSync(new URL(`../components/settings/${name}.tsx`, import.meta.url), "utf8");

test("account and cloud settings consume standard controls without legacy skins", () => {
  for (const name of ["AccountSettingsPanel", "NAccountSettingsPanel", "CloudResourcesPanel", "CloudDownloadsPanel", "CloudTransferControls"]) {
    const text = source(name);
    assert.match(text, /import \{[^}]*Button[^}]*Notice[^}]*\} from "@neuro\/ui"/);
    assert.doesNotMatch(text, /<(?:button|input|select)\b|className="(?:primary-button|ghost-button|studio-input)"/);
    assert.match(text, /<Notice tone="danger">\{error\}<\/Notice>/);
  }
});

test("account presentation preserves explicit authorization, cancellation and polling lifetimes", () => {
  const account = source("AccountSettingsPanel"), naccount = source("NAccountSettingsPanel");
  assert.match(account, /maxLength=\{80\}/);
  assert.match(account, /perform\("start", \{ origin, deviceName \}\)/);
  assert.match(account, /perform\("poll", \{ requestId: view.requestId \}\)/);
  assert.match(account, /CopyButton[^\n]*onCopy=\{\(\) => navigator.clipboard.writeText\(view.authorizationUrl\)\}/);
  assert.doesNotMatch(account, /writeText\([^\n]*\.catch/);
  for (const text of [account, naccount]) {
    assert.match(text, /window.open\(view.authorizationUrl, "_blank", "noopener,noreferrer"\)/);
    assert.match(text, /window.clearTimeout\(timer\)/);
    assert.match(text, /remoteRevoked === false && <Notice tone="warning">/);
    assert.match(text, /generation.current\+\+/);
  }
  assert.match(naccount, /disabled=\{busy \|\| !view.configured\}/);
  assert.match(naccount, /<CloudResourcesPanel key=\{`\$\{baseUrl\}:\$\{view.principal\}`\}/);
});

test("resource empty pages keep independent opaque pagination and actual install eligibility", () => {
  const resources = source("CloudResourcesPanel"), downloads = source("CloudDownloadsPanel");
  assert.match(resources, /<EmptyState title="暂无资源"/);
  assert.match(resources, /read\(page.nextCursor!\)/);
  assert.match(downloads, /<EmptyState title="暂无可下载内容"/);
  assert.match(downloads, /read\(page.nextAfter!\)/);
  assert.match(downloads, /canInstallCloudPackage\(item\) && <Button/);
  assert.match(downloads, /parseCloudDownload\(await postJson/);
  assert.match(downloads, /installCloudPackage\(baseUrl, item, id\)/);
  assert.match(downloads, /<CloudTransferControls key=\{taskId\}/);
});

test("download controls retain task identity, stale-response guards and timer cleanup", () => {
  const controls = source("CloudTransferControls");
  assert.match(controls, /controlCloudDownload\(baseUrl, taskId, "status"\)/);
  assert.match(controls, /controlCloudDownload\(baseUrl, taskId, action\)/);
  assert.match(controls, /expected === revision.current/);
  assert.match(controls, /stopped = true; alive.current = false; revision.current\+\+; clearTimeout\(timer\)/);
  assert.equal((controls.match(/disabled=\{busy \|\| status.control === 1\}/g) || []).length, 2);
});
