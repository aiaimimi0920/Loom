import assert from "node:assert/strict";
import test from "node:test";
import { deviceImageDelivery, deviceImageLayout, imageTarget, sendDeviceImage } from "./deviceImageTransfer.ts";
import { WALL_PROTOCOL_VERSION, type WallState } from "./wallTypes.ts";

const resourceId = "sha256:" + "a".repeat(64);
const endpointId = "pc3-screen";
function state(): WallState {
  return { protocolVersion: WALL_PROTOCOL_VERSION, revision: 3, layouts: [], endpoints: [{ online: true,
    appliedRevision: null, endpoint: { protocolVersion: WALL_PROTOCOL_VERSION, endpointId,
      deviceId: "paired-pc3", outputId: "screen-1", pixelSize: { width: 1920, height: 1080 },
      renderModes: ["image"], inputCapabilities: [] } }] };
}

test("device image placement preserves aspect ratio, targets one endpoint and replaces only its own layout", () => {
  const initial = state();
  const layout = deviceImageLayout(initial, endpointId, resourceId, { width: 400, height: 400 });
  assert.deepEqual(layout.placements[0].rect, { x: 420, y: 0, width: 1080, height: 1080 });
  assert.equal(layout.tiles.length, 1);
  assert.equal(layout.tiles[0].endpointId, endpointId);
  assert.equal(layout.revision, 4);
  const next = { ...initial, revision: 4, layouts: [layout] };
  assert.equal(deviceImageLayout(next, endpointId, resourceId, { width: 300, height: 100 }).wallId, layout.wallId);
  assert.equal(initial.layouts.length, 0);
  assert.throws(() => deviceImageLayout(initial, endpointId, resourceId, { width: 16384, height: 16384 }), /解码限制/);
  assert.throws(() => deviceImageLayout(initial, endpointId, "file:///secret", { width: 2, height: 2 }), /无效/);
});

test("offline, missing, non-image, occupied and frozen outputs cannot be overwritten", () => {
  const value = state();
  assert.throws(() => imageTarget(value, "another-device"), /离线/);
  value.endpoints[0].online = false;
  assert.throws(() => imageTarget(value, endpointId), /离线/);
  value.endpoints[0].online = true;
  value.endpoints[0].endpoint.renderModes = ["surface_v1"];
  assert.throws(() => imageTarget(value, endpointId), /不支持/);
  value.endpoints[0].endpoint.renderModes = ["image"];
  const layout = deviceImageLayout(value, endpointId, resourceId, { width: 2, height: 2 });
  value.layouts = [{ ...layout, wallId: "existing-user-wall" }];
  assert.throws(() => imageTarget(value, endpointId), /避免覆盖/);
  value.layouts = [{ ...layout, placements: [...layout.placements, { ...layout.placements[0], placementId: "extra" }] }];
  assert.throws(() => imageTarget(value, endpointId), /避免覆盖/);
  value.layouts = [layout];
  value.presentations = [{ wallId: layout.wallId, revision: 4, mode: "black" }];
  assert.throws(() => imageTarget(value, endpointId), /黑场/);
});

test("submission is not delivery; require the exact online endpoint revision acknowledgement", () => {
  const value = state();
  const layout = deviceImageLayout(value, endpointId, resourceId, { width: 2, height: 2 });
  value.layouts = [layout]; value.revision = layout.revision;
  const receipt = { endpointId, wallId: layout.wallId, revision: layout.revision };
  assert.match(deviceImageDelivery(value, receipt), /等待接收端确认/);
  value.endpoints[0].appliedRevision = 3;
  assert.match(deviceImageDelivery(value, receipt), /等待接收端确认/);
  value.endpoints[0].appliedRevision = layout.revision;
  assert.match(deviceImageDelivery(value, receipt), /已确认呈现/);
  value.endpoints[0].online = false;
  assert.match(deviceImageDelivery(value, receipt), /离线/);
  value.layouts[0].revision++;
  assert.match(deviceImageDelivery(value, receipt), /替换或停止/);
});

test("send uploads once, uses catalog CAS and retains existing authenticated API paths", async (context) => {
  const originalFetch = globalThis.fetch, originalBitmap = globalThis.createImageBitmap;
  context.after(() => { globalThis.fetch = originalFetch; globalThis.createImageBitmap = originalBitmap; });
  let value = state(), closed = 0, uploads = 0, writes = 0, mutateDuringUpload = false;
  let cancelDuringUpload: AbortController | undefined;
  globalThis.createImageBitmap = async () => ({ width: 400, height: 400, close() { closed++; } } as ImageBitmap);
  globalThis.fetch = async (url, options) => {
    const path = new URL(String(url)).pathname;
    if (path === "/v1/walls/state") return Response.json(value);
    const body = JSON.parse(String(options?.body));
    if (path === "/v1/surfaces/resources") {
      uploads++; assert.equal(options?.method, "POST");
      assert.equal(body.dataBase64, "AQID");
      if (mutateDuringUpload) value.revision++;
      cancelDuringUpload?.abort();
      return Response.json({ resource: { resourceId, kind: "image", mime: "image/png", size: 3 } });
    }
    assert.equal(path, "/v1/walls/layouts"); assert.equal(options?.method, "PUT");
    writes++; assert.equal(body.baseRevision, value.revision);
    value = { ...value, revision: value.revision + 1, layouts: [body.layout] };
    return Response.json(value);
  };
  const file = new File([new Uint8Array([1, 2, 3])], "image.png", { type: "image/png" });
  const receipt = await sendDeviceImage("http://127.0.0.1:18765", endpointId, file, new AbortController().signal);
  assert.equal(receipt.revision, 4); assert.equal(closed, 1); assert.equal(uploads, 1); assert.equal(writes, 1);
  mutateDuringUpload = true;
  await assert.rejects(sendDeviceImage("http://127.0.0.1:18765", endpointId, file, new AbortController().signal), /已变化/);
  assert.equal(writes, 1);
  mutateDuringUpload = false; cancelDuringUpload = new AbortController();
  await assert.rejects(sendDeviceImage("http://127.0.0.1:18765", endpointId, file, cancelDuringUpload.signal), /abort/i);
  assert.equal(writes, 1); assert.equal(closed, 3);
  const aborted = new AbortController(); aborted.abort();
  await assert.rejects(sendDeviceImage("http://127.0.0.1:18765", endpointId, file, aborted.signal), /abort/i);
  assert.equal(uploads, 3);
});
