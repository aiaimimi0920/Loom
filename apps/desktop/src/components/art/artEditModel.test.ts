import assert from "node:assert/strict";
import test from "node:test";
import { currentLanguage, setLanguage, uiFormatFor, uiTextFor } from "../../i18n/index.ts";
import type { LoomArtManagement, LoomArtManagementParameter, LoomCredentialSummary } from "../../services/loomApi.ts";
import { buildArtEditSettings, credentialMatchesArtParameter, type ArtEditSettingsDraft } from "./artEditModel.ts";

const parameter = (
  id: string,
  parameterType = "string",
  required = true,
  secret = false,
): LoomArtManagementParameter => ({
  id,
  label: id,
  parameterType,
  required,
  secret,
});
const credential = (name: string, valueType: LoomCredentialSummary["valueType"]): LoomCredentialSummary => ({
  name,
  valueType,
  scope: {},
  protection: "fixture",
});
const management = (parameters: LoomArtManagementParameter[], canEditIdentity = true): LoomArtManagement => ({
  artId: "fixture-art",
  name: "Art",
  description: "",
  locallyAuthored: true,
  canEditIdentity,
  currentVersion: "1.0.0",
  highestVersion: "1.1.0",
  autoUpdate: false,
  installedVersions: [],
  availableVersions: ["1.0.0", "1.1.0"],
  parameters,
  defaults: {},
  valueBindings: {},
  credentialBindings: {},
  availableCredentials: [],
  updateAvailable: true,
});
const draft = (patch: Partial<ArtEditSettingsDraft> = {}): ArtEditSettingsDraft => ({
  name: "  Custom Name  ",
  description: " Original description ",
  autoUpdate: false,
  parameterValues: {},
  valueBindings: {},
  useGlobalValues: {},
  secretDrafts: {},
  ...patch,
});

test("requires management and preserves identity permissions without extra payload fields", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));

  setLanguage("en");
  assert.throws(() => buildArtEditSettings(null, [], draft()), {
    message: "Art management settings have not loaded yet.",
  });
  setLanguage("zh-Hans");
  assert.throws(() => buildArtEditSettings(null, [], draft()), {
    message: uiTextFor("zh-Hans", "art.edit.error.notLoaded"),
  });

  assert.deepEqual(buildArtEditSettings(management([]), [], draft()), {
    name: "Custom Name",
    description: " Original description ",
    autoUpdate: false,
    defaults: {},
    valueBindings: {},
    credentialBindings: {},
  });
  assert.deepEqual(buildArtEditSettings(management([], false), [], draft({ autoUpdate: true })), {
    autoUpdate: true,
    defaults: {},
    valueBindings: {},
    credentialBindings: {},
  });
});

test("keeps zero, false and optional overrides while omitting empty optional values", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));
  setLanguage("en");

  const model = management([
    parameter("zero", "number"),
    parameter("off", "boolean"),
    parameter("empty", "string", false),
    parameter("optional", "integer", false),
    parameter("null", "string", false),
  ]);
  const result = buildArtEditSettings(
    model,
    [],
    draft({ parameterValues: { zero: 0, off: false, empty: "  ", optional: 4, null: null } }),
  );
  assert.deepEqual(result.defaults, { zero: 0, off: false, optional: 4 });
  for (const value of [undefined, null, "", " \n "]) {
    assert.throws(
      () =>
        buildArtEditSettings(management([parameter("required")]), [], draft({ parameterValues: { required: value } })),
      { message: "required is required." },
    );
  }
});

test("parses JSON drafts without changing already typed JSON or text", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));
  setLanguage("en");

  const model = management([parameter("json", "json"), parameter("text")]);
  for (const [text, expected] of [
    ["false", false],
    ["0", 0],
    ['{"items":[1]}', { items: [1] }],
    ["null", null],
  ]) {
    const input = draft({ parameterValues: { json: text, text: " unchanged " } });
    const result = buildArtEditSettings(model, [], input);
    assert.deepEqual(result.defaults, { json: expected, text: " unchanged " });
    assert.equal(input.parameterValues.json, text);
  }
  assert.deepEqual(
    buildArtEditSettings(management([parameter("json", "json")]), [], draft({ parameterValues: { json: [1, 2] } }))
      .defaults,
    { json: [1, 2] },
  );
  assert.throws(
    () => buildArtEditSettings(management([parameter("json", "json")]), [], draft({ parameterValues: { json: "{" } })),
    { message: "json is not valid JSON." },
  );
});

test("matches credential types exactly with number accepting integers and secrets requiring text", () => {
  const accepted: Record<string, string[]> = {
    number: ["number", "integer"],
    integer: ["integer"],
    boolean: ["boolean"],
    json: ["json"],
    enum: ["string"],
    string: ["string"],
    path: ["string"],
  };
  const types: LoomCredentialSummary["valueType"][] = ["string", "number", "integer", "boolean", "json"];
  for (const [kind, matches] of Object.entries(accepted))
    for (const type of types) {
      assert.equal(
        credentialMatchesArtParameter(parameter("p", kind), credential("global", type)),
        matches.includes(type),
        `${kind}/${type}`,
      );
      assert.equal(
        credentialMatchesArtParameter(parameter("p", kind, true, true), credential("global", type)),
        type === "string",
        `secret ${kind}/${type}`,
      );
    }
});

test("global value references replace local defaults and reject missing or incompatible bindings", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));
  setLanguage("en");

  const model = management([parameter("limit", "number")]);
  const input = draft({
    parameterValues: { limit: 999 },
    useGlobalValues: { limit: true },
    valueBindings: { limit: "LIMIT" },
  });
  assert.deepEqual(buildArtEditSettings(model, [credential("LIMIT", "integer")], input).valueBindings, {
    limit: "LIMIT",
  });
  assert.deepEqual(buildArtEditSettings(model, [credential("LIMIT", "integer")], input).defaults, {});
  for (const available of [[], [credential("LIMIT", "string")], [credential("other", "number")]]) {
    assert.throws(() => buildArtEditSettings(model, available, input), {
      message: "The global value for limit does not exist or has an incompatible type.",
    });
  }
  assert.throws(() => buildArtEditSettings(model, [], draft({ useGlobalValues: { limit: true } })), {
    message: "Select a global value for limit.",
  });
});

test("stored secrets survive empty drafts, replacements retain raw bytes, and global mode sends no local value", () => {
  const model = management([parameter("token", "string", true, true)]);
  const stored = { useGlobal: false, credential: "", storedCredential: "loom-art-secret-old", value: "" };
  const unchanged = buildArtEditSettings(model, [], draft({ secretDrafts: { token: stored } }));
  assert.deepEqual(unchanged.credentialBindings, { token: "loom-art-secret-old" });
  assert.equal("secretValues" in unchanged, false);
  const replacement = buildArtEditSettings(
    model,
    [],
    draft({ secretDrafts: { token: { ...stored, value: "  new\n" } } }),
  );
  assert.deepEqual(replacement.secretValues, { token: "  new\n" });
  assert.deepEqual(replacement.credentialBindings, unchanged.credentialBindings);
  const global = buildArtEditSettings(
    model,
    [credential("TOKEN", "string")],
    draft({
      secretDrafts: { token: { ...stored, useGlobal: true, credential: "TOKEN", value: "must-not-send" } },
    }),
  );
  assert.deepEqual(global.credentialBindings, { token: "TOKEN" });
  assert.equal("secretValues" in global, false);
});

test("new secrets require nonempty local input or a compatible global reference", (context) => {
  const previousLanguage = currentLanguage();
  context.after(() => setLanguage(previousLanguage));
  setLanguage("en");

  const model = management([parameter("token", "string", true, true)]);
  const empty = { useGlobal: false, credential: "", storedCredential: "", value: "" };
  assert.throws(() => buildArtEditSettings(model, [], draft()), { message: "token is required." });
  assert.throws(() => buildArtEditSettings(model, [], draft({ secretDrafts: { token: { ...empty, value: "  " } } })), {
    message: "token is required.",
  });
  assert.throws(
    () => buildArtEditSettings(model, [], draft({ secretDrafts: { token: { ...empty, useGlobal: true } } })),
    { message: "Select a global secret for token." },
  );
  for (const available of [[], [credential("TOKEN", "number")]]) {
    assert.throws(
      () =>
        buildArtEditSettings(
          model,
          available,
          draft({ secretDrafts: { token: { ...empty, useGlobal: true, credential: "TOKEN" } } }),
        ),
      { message: "The global secret for token does not exist or is not a text credential." },
    );
  }
  setLanguage("zh-Hans");
  assert.throws(
    () => buildArtEditSettings(model, [], draft({ secretDrafts: { token: { ...empty, useGlobal: true } } })),
    { message: uiFormatFor("zh-Hans", "art.edit.error.selectGlobalSecret", { label: "token" }) },
  );
  assert.deepEqual(
    buildArtEditSettings(management([parameter("token", "string", false, true)]), [], draft()).credentialBindings,
    {},
  );
  assert.deepEqual(
    buildArtEditSettings(model, [], draft({ secretDrafts: { token: { ...empty, value: "secret" } } })).secretValues,
    { token: "secret" },
  );
});
