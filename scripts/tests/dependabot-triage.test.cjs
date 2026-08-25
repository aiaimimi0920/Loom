const assert = require("node:assert/strict");
const test = require("node:test");

const triage = require("../../.github/scripts/dependabot-triage.cjs");

const dependencies = {
  development: new Set(["typescript", "@tauri-apps/cli"]),
  runtime: new Set(["react", "@tauri-apps/api"]),
};

function files(...names) {
  return names.map((filename) => ({ filename }));
}

test("version parsing is conservative for major, minor, patch, grouped, and unknown updates", () => {
  assert.equal(triage.updateType("Bump actions/checkout from 5 to 7"), "major");
  assert.equal(triage.updateType("Bump crate from 1.2.3 to 1.3.0"), "minor");
  assert.equal(triage.updateType("Bump crate from 1.2.3 to 1.2.4"), "patch");
  assert.equal(triage.updateType("Bump crate from 2.0.0-rc.10 to 2.0.0-rc.13"), "patch");
  assert.equal(triage.updateType("Bump the frontend group with 3 updates"), "grouped");
  assert.equal(triage.updateType("Refresh dependencies"), "unknown");
});

test("supply-chain, runtime, and unknown changes always require human review", () => {
  const action = triage.classify(
    "Bump actions/checkout from 5 to 7",
    files(".github/workflows/ci.yml"),
    dependencies,
  );
  assert.deepEqual([action.scope, action.update, action.disposition], ["supply-chain", "major", "needs-human"]);
  const runtime = triage.classify(
    "Bump react from 19.0.0 to 19.0.1",
    files("apps/desktop/package.json", "apps/desktop/package-lock.json"),
    dependencies,
  );
  assert.deepEqual([runtime.scope, runtime.disposition], ["runtime", "needs-human"]);
  const unknown = triage.classify("Refresh dependencies", files("unknown.lock"), dependencies);
  assert.deepEqual([unknown.scope, unknown.disposition], ["unknown", "needs-human"]);
});

test("only isolated non-sensitive development patch or minor updates become review candidates", () => {
  const tooling = triage.classify(
    "Bump typescript from 5.9.3 to 5.9.4",
    files("apps/desktop/package.json", "apps/desktop/package-lock.json"),
    dependencies,
  );
  assert.deepEqual([tooling.scope, tooling.update, tooling.disposition], ["tooling", "patch", "review-candidate"]);
  const nativeTooling = triage.classify(
    "Bump @tauri-apps/cli from 2.11.2 to 2.11.4",
    files("apps/desktop/package.json", "apps/desktop/package-lock.json"),
    dependencies,
  );
  assert.equal(nativeTooling.disposition, "needs-human");
  const majorTooling = triage.classify(
    "Bump typescript from 5.9.3 to 7.0.2",
    files("apps/desktop/package.json", "apps/desktop/package-lock.json"),
    dependencies,
  );
  assert.equal(majorTooling.disposition, "needs-human");
});
