import assert from "node:assert/strict";
import test from "node:test";
import { Catalogs, setLanguage } from "../../i18n/index.ts";
import { enSettings } from "../../locales/en/settings.ts";
import { zhHansSettings } from "../../locales/zh-Hans/settings.ts";
import type { LoomManagedDevice } from "../../services/loomApi/otterTypes.ts";
import type { OtterLocaleState } from "../../services/loomApi/otterLocaleTypes.ts";
import {
  localeDisplayName,
  localeRequestMessage,
  localeUnavailableReason,
  observeLocaleRequest,
  type TrackedLocaleRequest,
} from "./otterLanguageState.ts";

const tracked = (): TrackedLocaleRequest => ({
  deviceId: "device", requestId: "request", receiverGeneration: "receiver",
  expectedRevision: 1, language: "en", deadline: 15_000, status: "pending",
});
const locale = (): OtterLocaleState => ({
  language: "en", revision: 2, supportedLanguages: ["en", "zh-Hans"],
  active: true, receiverGeneration: "receiver", reportedAt: 1_000,
  request: {
    requestId: "request", receiverGeneration: "receiver", expectedRevision: 1, language: "en",
    expiresAt: 1_700_000_000_000, validForMs: 10_000, status: "applied",
  },
});
const device = (): LoomManagedDevice => ({
  id: "device", name: "Otter", address: "localhost", kind: "computer",
  approval: "approved", createdAt: 1, enabled: true, locale: locale(),
});

test("settings locale shards stay complete and synchronized across English and Simplified Chinese", () => {
  const settingsCatalog = new Catalogs([enSettings], [zhHansSettings]);
  assert.doesNotThrow(() => settingsCatalog.validateCompleteness());
  assert.equal(localeDisplayName("en"), "English");
  assert.equal(localeDisplayName("zh-Hans"), "\u7b80\u4f53\u4e2d\u6587");
  assert.equal(localeDisplayName("custom-tag"), "custom-tag");
});

test("only an exact request receipt establishes successful application", () => {
  assert.equal(observeLocaleRequest(tracked(), locale(), 100).status, "applied");
  const current = locale();
  assert.equal(observeLocaleRequest(tracked(), { ...current, request: null }, 100).status, "pending");
  for (const patch of [
    { requestId: "other" }, { receiverGeneration: "other" },
    { expectedRevision: 0 }, { language: "zh-Hans" },
  ]) {
    const receipt = { ...current.request!, ...patch };
    assert.equal(observeLocaleRequest(tracked(), { ...current, request: receipt }, 100).status, "pending");
  }
});

test("elapsed requests become unknown, not failed, and late exact receipts can reconcile them", (context) => {
  context.after(() => setLanguage("en"));
  setLanguage("en");
  const unknown = observeLocaleRequest(tracked(), null, 15_000);
  assert.equal(unknown.status, "unknown");
  assert.match(localeRequestMessage(unknown), /Outcome unconfirmed/i);
  assert.doesNotMatch(localeRequestMessage(unknown), /failed/i);
  assert.equal(observeLocaleRequest(unknown, locale(), 15_100).status, "applied");
});

test("receiver replacement and conflict never auto-retry or manufacture success", (context) => {
  context.after(() => setLanguage("en"));
  setLanguage("en");
  assert.equal(observeLocaleRequest(tracked(), { ...locale(), receiverGeneration: "replacement" }, 100).status, "unknown");
  const current = locale();
  const conflict = observeLocaleRequest(tracked(), { ...current, request: { ...current.request!, status: "conflict" } }, 100);
  assert.equal(conflict.status, "conflict");
  assert.match(localeRequestMessage(conflict), /Not overwritten automatically/i);
  assert.equal(observeLocaleRequest(conflict, locale(), 100), conflict);
});

test("request deadlines use bounded remaining lifetime, never the server wall clock", () => {
  const current = locale();
  const pending = { ...current, request: { ...current.request!, status: "pending" as const, validForMs: 300 } };
  const updated = observeLocaleRequest(tracked(), pending, 1000);
  assert.equal(updated.deadline, 1300);
  assert.equal(observeLocaleRequest(updated, null, 1300).status, "unknown");
  assert.equal(observeLocaleRequest(updated, pending, 1200).deadline, 1300);
  assert.equal(observeLocaleRequest(tracked(), {
    ...pending, request: { ...pending.request, validForMs: 0 },
  }, 1000).status, "unknown");
});

test("only approved, enabled, active receivers with a supported protocol accept edits", (context) => {
  context.after(() => setLanguage("en"));
  setLanguage("en");
  assert.equal(localeUnavailableReason(device(), true), null);
  assert.match(localeUnavailableReason(device(), false)!, /Loom is disconnected/i);
  assert.match(localeUnavailableReason(undefined, true)!, /Connect and approve/i);
  assert.match(localeUnavailableReason({ ...device(), enabled: false }, true)!, /disabled/i);
  assert.match(localeUnavailableReason({ ...device(), approval: "pending" }, true)!, /approved/i);
  assert.match(localeUnavailableReason({ ...device(), locale: null }, true)!, /newer Otter/i);
  assert.match(localeUnavailableReason({ ...device(), locale: { ...locale(), active: false } }, true)!, /not queued offline/i);
  const pending = { ...locale(), request: { ...locale().request!, status: "pending" as const } };
  assert.match(localeUnavailableReason({ ...device(), locale: pending }, true)!, /waiting for receipt/i);
});
