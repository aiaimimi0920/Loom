import assert from "node:assert/strict";
import test from "node:test";
import { fetchCloudArtCatalog, parseCloudPublicationPage } from "./cloudCatalog.ts";
import { installCloudPackage } from "./cloudPackageInstall.ts";

const art = { id: "publication-art", title: "颜色迁移", label: "0.1.5", kind: "art",
  size: 18670, sha256: "a".repeat(64), policy: "loom-art-zip-format-v1" };

test("Art 商店读取 AssetLibrary 并保留精确发布绑定，不调用旧商店", async (context) => {
  const previous = globalThis.fetch;
  context.after(() => { globalThis.fetch = previous; });
  const calls: string[] = [];
  globalThis.fetch = async (url, options) => {
    calls.push(String(url));
    if (String(url).endsWith("/catalog")) {
      assert.deepEqual(JSON.parse(String(options?.body)), { after: "cursor" });
      return Response.json({ items: [art, { ...art, id: "other", kind: "mcp" }], nextAfter: "next" });
    }
    assert.deepEqual(JSON.parse(String(options?.body)), { publicationId: art.id, kind: "art", sha256: art.sha256 });
    return Response.json({ download: { publicationId: art.id, path: "fixture.zip", size: art.size, sha256: art.sha256 }, installation: {} });
  };
  const result = await fetchCloudArtCatalog("http://127.0.0.1:8765", "cursor");
  assert.equal(result.arts.length, 1);
  assert.equal(result.nextAfter, "next");
  assert.equal(result.arts[0].official, undefined);
  assert.equal(result.arts[0].framework, undefined);
  assert.equal(result.arts[0].latestVersion, "0.1.5");
  await installCloudPackage("http://127.0.0.1:8765", result.arts[0].publication);
  assert.deepEqual(calls.map((url) => new URL(url).pathname), ["/v1/asset-library/catalog", "/v1/asset-library/install"]);
});

test("只有大型应用策略可超过旧预算，错误分页和摘要被拒绝", () => {
  const large = { ...art, kind: "application", policy: "application-large-zip-format-v1", size: 80_166_238 };
  assert.equal(parseCloudPublicationPage({ items: [large], nextAfter: null }).items.length, 1);
  for (const bad of [{ ...large, kind: "art" }, { ...large, policy: "application-zip-format-v1" },
    { ...large, size: 96_000_001 }, { ...art, sha256: "bad" }, null])
    assert.throws(() => parseCloudPublicationPage({ items: [bad], nextAfter: null }), /response_invalid/);
  assert.throws(() => parseCloudPublicationPage({ items: [], nextAfter: 1 }), /response_invalid/);
});

test("无 Art 的页仍保留下一页，网络失败不冒充空目录", async (context) => {
  const previous = globalThis.fetch;
  context.after(() => { globalThis.fetch = previous; });
  globalThis.fetch = async () => Response.json({ items: [{ ...art, kind: "mcp" }], nextAfter: "next" });
  assert.deepEqual(await fetchCloudArtCatalog("http://127.0.0.1:8765"), { arts: [], nextAfter: "next" });
  globalThis.fetch = async () => { throw new Error("offline"); };
  await assert.rejects(fetchCloudArtCatalog("http://127.0.0.1:8765"));
});
