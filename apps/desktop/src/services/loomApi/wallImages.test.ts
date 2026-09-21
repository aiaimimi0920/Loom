import assert from "node:assert/strict";
import test from "node:test";
import { uploadWallImage } from "./wallImages.ts";

test("image import uploads source bytes without any layout mutation or transport override", async (context) => {
  const original = globalThis.fetch;
  context.after(() => { globalThis.fetch = original; });
  const id = `sha256:${"a".repeat(64)}`;
  const calls: string[] = [];
  globalThis.fetch = async (url, options) => {
    calls.push(String(url));
    assert.equal(options?.method, "POST");
    assert.deepEqual(JSON.parse(String(options?.body)), { kind: "image", mime: "image/png", dataBase64: "AP8BAg==" });
    return Response.json({ resource: { resourceId: id, kind: "image", mime: "image/png", size: 4 } });
  };
  const source = await uploadWallImage("http://127.0.0.1:18765", new File([new Uint8Array([0, 255, 1, 2])], "a.png", { type: "image/png" }));
  assert.deepEqual(source.source, { kind: "image", id });
  assert.deepEqual(calls, ["http://127.0.0.1:18765/v1/surfaces/resources"]);
});

test("image import rejects oversized input before reading, non-raster MIME and forged resource descriptors", async (context) => {
  const original = globalThis.fetch;
  context.after(() => { globalThis.fetch = original; });
  let calls = 0;
  globalThis.fetch = async () => { calls++; return Response.json({ resource: { resourceId: "file:///private", kind: "image" } }); };
  await assert.rejects(uploadWallImage("http://127.0.0.1:18765", { size: 16 * 1024 * 1024 + 1 } as File), /16 MiB/);
  await assert.rejects(uploadWallImage("http://127.0.0.1:18765", new File(["<svg/>"], "a.svg", { type: "image/svg+xml" })), /支持/);
  assert.equal(calls, 0);
  await assert.rejects(uploadWallImage("http://127.0.0.1:18765", new File(["bad"], "a.png", { type: "image/png" })), /无效/);
  assert.equal(calls, 1);
});
