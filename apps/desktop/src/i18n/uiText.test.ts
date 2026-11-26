import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  Catalogs,
  currentLanguage,
  DEFAULT_DISPLAY_LANGUAGE,
  DISPLAY_LANGUAGES,
  displayLanguageName,
  parseDisplayLanguage,
  parsePersistedDisplayLanguage,
  resolveCatalogLocale,
  setLanguage,
  uiFormatFor,
  uiText,
  uiTextFor,
} from "./index.ts";

function collectSourceFiles(dir: string): string[] {
  const results: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...collectSourceFiles(full));
    } else if (entry.isFile() && /\.(ts|tsx)$/.test(entry.name)) {
      results.push(full);
    }
  }
  return results;
}

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/(^|[^:\\])\/\/.*$/gm, "$1");
}

test("canonical DisplayLanguage codes round-trip and normalize persisted aliases", () => {
  assert.equal(DEFAULT_DISPLAY_LANGUAGE, "en");
  for (const lang of DISPLAY_LANGUAGES) {
    assert.equal(parseDisplayLanguage(lang), lang);
    assert.ok(displayLanguageName(lang).length > 0);
  }
  assert.equal(parseDisplayLanguage("zh-CN"), null);
  assert.equal(parseDisplayLanguage("EN"), null);

  assert.equal(parsePersistedDisplayLanguage("zh-CN"), "zh-Hans");
  assert.equal(parsePersistedDisplayLanguage("zh-Hans-CN"), "zh-Hans");
  assert.equal(parsePersistedDisplayLanguage("en-US"), "en");
  assert.equal(parsePersistedDisplayLanguage(""), "en");
  assert.equal(parsePersistedDisplayLanguage("fr"), "en");

  assert.equal(resolveCatalogLocale("en"), "en-US");
  assert.equal(resolveCatalogLocale("zh-Hans"), "zh-CN");
});

test("bundled interface catalogs are complete and valid", () => {
  const catalogs = Catalogs.embedded();
  catalogs.validateCompleteness();
  assert.equal(uiTextFor("en", "common.save"), "Save");
  assert.equal(uiTextFor("zh-Hans", "common.save"), "\u4fdd\u5b58");
});

test("missing translation falls back to English and translations may omit English arguments", () => {
  const catalogs = new Catalogs(
    [{ "sample.label": "Source", "sample.greeting": "Hello { $name }" }],
    [{ "sample.greeting": "\u4f60\u597d" }],
  );
  assert.equal(catalogs.text("zh-Hans", "sample.label"), "Source");
  assert.equal(catalogs.format("en", "sample.greeting", { name: "Loom" }), "Hello Loom");
  assert.equal(catalogs.format("zh-Hans", "sample.greeting", { name: "Loom" }), "\u4f60\u597d");
});

test("invalid catalog definitions and bad call arguments fail closed", () => {
  assert.throws(() => new Catalogs([{ a: "1" }, { a: "2" }], []), /duplicate interface message/);
  assert.throws(() => new Catalogs([{ a: "{" }], []), /Unclosed placeholder/);
  assert.throws(() => new Catalogs([{ a: "Fine" }], [{ a: "{ $extra }" }]), /undeclared parameter/);
  assert.throws(() => new Catalogs([{ a: "Fine" }], [{ extra: "Nope" }]), /no English source/);

  const catalogs = new Catalogs([{ msg: "Hello { $name }" }], [{ msg: "Hi { $name }" }]);
  assert.throws(() => catalogs.text("en", "msg"), /Unknown or parameterized/);
  assert.throws(() => catalogs.format("en", "msg", {}), /Incorrect arguments/);
});

test("active language updates uiText lookups and resets to English", () => {
  try {
    setLanguage("zh-Hans");
    assert.equal(currentLanguage(), "zh-Hans");
    assert.equal(uiText("common.save"), "\u4fdd\u5b58");
  } finally {
    setLanguage("en");
  }
  assert.equal(currentLanguage(), "en");
  assert.equal(uiText("common.save"), "Save");
});

test("desktop source tree contains zero raw CJK outside zh-Hans catalogs and all literal message IDs exist", () => {
  const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const zhHansDir = path.join(srcRoot, "locales", "zh-Hans") + path.sep;
  const catalogs = Catalogs.embedded();
  let checkedCalls = 0;

  for (const file of collectSourceFiles(srcRoot)) {
    if (file.startsWith(zhHansDir)) {
      continue;
    }
    const content = readFileSync(file, "utf8");
    const cjkMatch = content.match(/[\u4e00-\u9fff]/);
    assert.equal(
      cjkMatch,
      null,
      `Unexpected raw CJK character "${cjkMatch?.[0] ?? ""}" in ${path.relative(srcRoot, file)}`,
    );

    if (file.endsWith(".test.ts") || file.endsWith(".test.tsx")) {
      continue;
    }
    const code = stripComments(content);
    for (const match of code.matchAll(/\b(?:t|uiText)\(\s*"([^"]+)"\s*\)/g)) {
      const id = match[1];
      const params = catalogs.sourceParameters(id);
      assert.ok(params !== undefined, `${path.relative(srcRoot, file)} references missing static message "${id}"`);
      assert.equal(params.size, 0, `${path.relative(srcRoot, file)} calls static lookup on parameterized message "${id}"`);
      checkedCalls += 1;
    }
    for (const match of code.matchAll(/\buiTextFor\(\s*[^,]+,\s*"([^"]+)"\s*\)/g)) {
      const id = match[1];
      const params = catalogs.sourceParameters(id);
      assert.ok(params !== undefined, `${path.relative(srcRoot, file)} references missing static message "${id}"`);
      assert.equal(params.size, 0, `${path.relative(srcRoot, file)} calls static lookup on parameterized message "${id}"`);
      checkedCalls += 1;
    }
    for (const match of code.matchAll(/\b(?:tf|uiFormat)\(\s*"([^"]+)"\s*,/g)) {
      const id = match[1];
      const params = catalogs.sourceParameters(id);
      assert.ok(params !== undefined, `${path.relative(srcRoot, file)} references missing formatted message "${id}"`);
      checkedCalls += 1;
    }
    for (const match of code.matchAll(/\buiFormatFor\(\s*[^,]+,\s*"([^"]+)"\s*,/g)) {
      const id = match[1];
      const params = catalogs.sourceParameters(id);
      assert.ok(params !== undefined, `${path.relative(srcRoot, file)} references missing formatted message "${id}"`);
      checkedCalls += 1;
    }
  }

  assert.ok(checkedCalls > 500, `Expected source guard to inspect >500 literal UI calls, got ${checkedCalls}`);
});
