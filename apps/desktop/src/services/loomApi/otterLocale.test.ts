import assert from "node:assert/strict";
import test from "node:test";
import { listOtterLocaleDevices, requestOtterLocale } from "./otterLocale.ts";
import { parseOtterLocale, type OtterLocaleState } from "./otterLocaleTypes.ts";

const locale = (): OtterLocaleState => ({
  receiverGeneration: "receiver-1", language: "en", revision: 3,
  supportedLanguages: ["en", "zh-Hans"], active: true, reportedAt: 1000, request: null,
});
const input = {
  requestId: "request-1", receiverGeneration: "receiver-1", expectedRevision: 3, language: "zh-Hans",
};

test("locale client sends one device-scoped request with the displayed revision and generation", async context => {
  const original = globalThis.fetch;
  context.after(() => { globalThis.fetch = original; });
  let calls = 0;
  globalThis.fetch = (async (url, init) => {
    calls += 1;
    assert.equal(String(url), "http://127.0.0.1:18770/v1/devices/device%2Fa/locale-requests");
    assert.equal(init?.method, "POST");
    assert.deepEqual(JSON.parse(String(init?.body)), input);
    return Response.json({ locale: { ...locale(), request: { ...input, status: "pending", expiresAt: 16_000, validForMs: 15_000 } } }, { status: 202 });
  }) as typeof fetch;
  const result = await requestOtterLocale("http://127.0.0.1:18770", "device/a", input);
  assert.equal(calls, 1);
  assert.equal(result.language, "en");
  assert.equal(result.request?.status, "pending");
});

test("locale client does not retry a conflict or interpret HTTP success as applied", async context => {
  const original = globalThis.fetch;
  context.after(() => { globalThis.fetch = original; });
  let calls = 0;
  globalThis.fetch = (async () => {
    calls += 1;
    return Response.json({ error: { message: "locale_revision_conflict" } }, { status: 409 });
  }) as typeof fetch;
  await assert.rejects(requestOtterLocale("http://127.0.0.1:18770", "device", input), /locale_revision_conflict/);
  assert.equal(calls, 1);
});

test("device language state is read from the device list, with absent legacy capability remaining absent", async context => {
  const original = globalThis.fetch;
  context.after(() => { globalThis.fetch = original; });
  globalThis.fetch = (async url => {
    assert.equal(String(url), "http://127.0.0.1:18770/v1/devices");
    return Response.json({ devices: [{ id: "old" }, { id: "new", locale: locale() }], pending: [] });
  }) as typeof fetch;
  const devices = await listOtterLocaleDevices("http://127.0.0.1:18770");
  assert.equal(devices[0].locale, null);
  assert.equal(devices[1].locale?.language, "en");
});

test("locale response validation rejects unsafe revisions, malformed capabilities, and false receipts", () => {
  assert.equal(parseOtterLocale(undefined), null);
  assert.deepEqual(parseOtterLocale(locale()), locale());
  for (const patch of [
    { revision: Number.MAX_SAFE_INTEGER + 1 }, { revision: -1 },
    { active: "true" }, { supportedLanguages: ["en", "en"] },
    { supportedLanguages: ["fr"] }, { receiverGeneration: "../receiver" },
    { request: { ...input, expiresAt: 10, validForMs: 15_001, status: "pending" } },
    { request: { ...input, expiresAt: 10, validForMs: 0, status: "success" } },
  ]) assert.throws(() => parseOtterLocale({ ...locale(), ...patch }));
});
