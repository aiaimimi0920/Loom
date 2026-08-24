import assert from "node:assert/strict";
import test from "node:test";
import { normalizeHttpsExternalUrl } from "./externalUrl.ts";

test("accepts normalized HTTPS external links", () => {
  assert.equal(
    normalizeHttpsExternalUrl("https://github.com/example/project?tab=readme#usage"),
    "https://github.com/example/project?tab=readme#usage",
  );
});

test("rejects unsafe external link schemes and embedded credentials", () => {
  for (const url of [
    "javascript:alert(1)",
    "file:///C:/Windows/System32/calc.exe",
    "data:text/html,unsafe",
    "http://example.com/source",
    "https://user:secret@example.com/source",
    "not a url",
  ]) {
    assert.throws(() => normalizeHttpsExternalUrl(url));
  }
});
