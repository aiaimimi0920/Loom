import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

// Cargo check does not run the Tauri CLI's cross-language version preflight.
// Keep that packaging invariant covered by the normal frontend test gate.
const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

function majorMinor(version: string): string {
  const match = /^(\d+\.\d+)\.\d+/.exec(version);
  assert.ok(match, `Invalid locked version: ${version}`);
  return match[1];
}

test("locked Tauri API and CLI match the native runtime major/minor", () => {
  const cargo = read("../src-tauri/Cargo.lock");
  const rust = /\[\[package\]\]\s+name = "tauri"\s+version = "([^"]+)"/.exec(cargo)?.[1];
  assert.ok(rust, "Missing native Tauri lock entry");
  const lock = JSON.parse(read("../package-lock.json")) as {
    packages: Record<string, { version?: string }>;
  };
  for (const name of ["@tauri-apps/api", "@tauri-apps/cli"]) {
    const version = lock.packages[`node_modules/${name}`]?.version;
    assert.ok(version, `Missing npm lock entry: ${name}`);
    assert.equal(majorMinor(version), majorMinor(rust), `${name} must align with Rust Tauri`);
  }
  const manifest = read("../src-tauri/Cargo.toml");
  const pin = /^tauri\s*=\s*\{[^}]*\bversion\s*=\s*"([^"]+)"/m.exec(manifest)?.[1];
  assert.equal(pin, `=${rust}`, "Native Tauri manifest must preserve its exact lock pin");
});
