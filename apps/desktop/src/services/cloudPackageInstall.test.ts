import assert from "node:assert/strict";
import test from "node:test";
import { canInstallCloudPackage, installCloudPackage, parseCloudDownload } from "./cloudPackageInstall.ts";
import { cloudError } from "./naccount.ts";

const selected = { id: "publication", kind: "capability", size: 100, sha256: "a".repeat(64), policy: "loom-capability-zip-clamav-v1" };
const receipt = { publicationId: selected.id, path: "local/package.zip", size: selected.size, sha256: selected.sha256 };
test("下载结果绑定用户选择的发布、大小和摘要", () => {
  assert.equal(canInstallCloudPackage(selected), true);
  assert.equal(canInstallCloudPackage({ ...selected, kind: "application", policy: "application-zip-format-v1" }), true);
  assert.equal(canInstallCloudPackage({ ...selected, kind: "application", policy: "application-large-zip-format-v1", size: 80_000_000 }), true);
  for (const kind of ["art", "capability", "mcp"])
    assert.equal(canInstallCloudPackage({ ...selected, kind, policy: "application-large-zip-format-v1" }), false);
  assert.equal(canInstallCloudPackage({ ...selected, kind: "application", policy: "application-zip-clamav-v1" }), false);
  for (const kind of ["art", "capability", "mcp"])
    assert.equal(canInstallCloudPackage({ ...selected, kind, policy: `loom-${kind}-zip-format-v1` }), true);
  assert.equal(canInstallCloudPackage({ ...selected, policy: "capability-zip-format-v1" }), false);
  for (const policy of ["mcp-zip-format-v1", "loom-mcp-zip-clamav-v1"])
    assert.equal(canInstallCloudPackage({ ...selected, kind: "mcp", policy }), false);
  assert.equal(canInstallCloudPackage({ ...selected, policy: "loom-art-zip-format-v1" }), false);
  assert.equal(canInstallCloudPackage({ ...selected, policy: "capability-zip-clamav-v1" }), false);
  assert.equal(canInstallCloudPackage({ ...selected, kind: "art", policy: "art-zip-clamav-v1" }), false);
  assert.deepEqual(parseCloudDownload(receipt, selected), receipt);
  for (const changed of [{ publicationId: "other" }, { size: 101 }, { sha256: "b".repeat(64) }, { path: "" }])
    assert.throws(() => parseCloudDownload({ ...receipt, ...changed }, selected));
});
test("安装请求仅发送发布 ID、类型和摘要，不发送任意路径或自动批准权限", async (context) => {
  const previous = globalThis.fetch;
  context.after(() => { globalThis.fetch = previous; });
  globalThis.fetch = async (url, options) => {
    assert.equal(url, "http://127.0.0.1:8765/v1/asset-library/install");
    assert.deepEqual(JSON.parse(String(options?.body)), { publicationId: selected.id, kind: selected.kind, sha256: selected.sha256 });
    return Response.json({ download: receipt, installation: { package: { qualifiedId: "fixture/test" } } });
  };
  assert.deepEqual(await installCloudPackage("http://127.0.0.1:8765", selected), receipt);
  await assert.rejects(installCloudPackage("http://127.0.0.1:8765", { ...selected, kind: "application" }), /selection_invalid/);
});
test("安装拒绝不误报为本机身份失效，也不泄露未知错误", () => {
  assert.match(cloudError(new Error("HTTP 403 asset_library_host_install_rejected")), /安装器拒绝/);
  assert.match(cloudError(new Error("asset_library_framework_required")), /框架尚未就绪/);
  assert.equal(cloudError(new Error("unknown sensitive-value")), "请求未完成，请重试连接。");
});
test("应用安装展示实际入口，缺少入口不能误报成功", async (context) => {
  const previous = globalThis.fetch;
  context.after(() => { globalThis.fetch = previous; });
  const application = { ...selected, kind: "application", policy: "application-zip-format-v1" };
  globalThis.fetch = async () => Response.json({ download: receipt, installation: { entry: "C:/apps/app.exe" } });
  assert.equal((await installCloudPackage("http://127.0.0.1:8765", application)).installedEntry, "C:/apps/app.exe");
  globalThis.fetch = async () => Response.json({ download: receipt, installation: {} });
  await assert.rejects(installCloudPackage("http://127.0.0.1:8765", application), /response_invalid/);
});
