import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (name: string) => readFileSync(new URL(`../components/security/${name}.tsx`, import.meta.url), "utf8");
const panel = read("PluginSecurityPanel"),
  list = read("CredentialFieldList");

test("security controls and credential summaries use the standard package", () => {
  for (const source of [panel, list]) {
    assert.match(source, /from "@neuro\/ui"/);
    assert.doesNotMatch(source, /<(?:button|input|select|textarea)\b|studio-input|icon-button|ghost-button/);
  }
  assert.match(list, /<DataTable\s+label="全局凭据字段"/);
  assert.match(list, /getRowKey=\{credentialFieldId\}/);
  assert.match(list, /rowHeader: true/);
  assert.match(list, /已安全保存`\}>••••••••/);
  assert.doesNotMatch(list, /credential\.value\b|revealPluginCredential/);
});

test("security keeps explicit secret reading and original global credential filtering", () => {
  assert.match(panel, /!credential\.name\.startsWith\("loom-"\)/);
  assert.match(panel, /!credential\.scope\.frameworkId && !credential\.scope\.artId/);
  assert.match(panel, /const revealVersion = \+\+secretRevealVersion\.current/);
  assert.match(panel, /if \(revealVersion !== secretRevealVersion\.current\) return/);
  assert.match(panel, /if \(showPrivateKey\) \{\s*setShowPrivateKey\(false\);\s*setPrivateKey\(""\)/);
  assert.match(panel, /const revealed = await revealPublisherPrivateKey\(baseUrl\)/);
  assert.match(panel, /<Input\s+className="mono-line"\s+readOnly\s+type=\{showPrivateKey \? "text" : "password"\}/);
});

test("security policy, user validation and mutation feedback remain application-owned", () => {
  assert.match(panel, /<Field label="安装策略">/);
  for (const policy of ["require_signed", "require_trusted", "allow_unsigned"])
    assert.ok(panel.includes(`value="${policy}"`));
  assert.match(panel, /trust\.policy === "require_trusted"/);
  assert.ok(panel.includes("/^(?:NU\\d{11}|L\\d{10})$/.test(trustedUserId.trim())"));
  assert.match(panel, /key\.publisherId === userId && !key\.revoked/);
  assert.match(panel, /loading=\{busy === "identity"\}/);
  assert.doesNotMatch(panel, /重置中/);
  assert.match(panel, /<Panel className="trusted-user-library__row" key=\{userId\}/);
});

test("credential entry records the actual trigger before asynchronous reveal", () => {
  assert.match(list, /returnFocus\.current = event\.currentTarget;\s*onEdit\(credential\)/);
  assert.match(list, /returnFocus\.current = event\.currentTarget;\s*onCreate\(\)/);
  assert.match(panel, /createButton=\{credentialCreateButton\}/);
  assert.match(panel, /returnFocusRef=\{credentialReturnFocus\}/);
  assert.equal(panel.match(/await load\(\);\s*setCredentialDraft\(null\)/g)?.length, 2);
  assert.doesNotMatch(panel + list, /setTimeout|addEventListener|body\.style|createPortal/);
});

test("trusted user removal restores orphaned focus after commit without stealing another target", () => {
  assert.match(panel, /removedUserFocus\.current = userId/);
  assert.match(panel, /if \(busy !== null \|\| !removedUserFocus\.current\) return/);
  assert.match(panel, /!trust\.trustedPublishers\.includes\(removedUserFocus\.current\)/);
  assert.match(panel, /removed && document\.activeElement === document\.body/);
  assert.match(panel, /trustedUserInput\.current\?\.focus\(\)/);
});

test("daemon identity owns the complete security session and stale confirmations cannot submit", () => {
  assert.match(panel, /<PluginSecuritySession key=\{baseUrl\} baseUrl=\{baseUrl\}/);
  assert.equal(panel.match(/const version = loadVersion\.current/g)?.length, 3);
  assert.equal(panel.match(/if \(version !== loadVersion\.current\) return/g)?.length, 4);
  assert.match(panel, /loadVersion\.current \+= 1;\s*secretRevealVersion\.current \+= 1/);
});
