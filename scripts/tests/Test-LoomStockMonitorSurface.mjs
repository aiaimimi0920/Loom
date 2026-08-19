import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import vm from "node:vm";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const defaultSurfacePath = path.resolve(
  scriptDirectory,
  "../../art-packages/samples/stock-monitor/surface/main.js",
);
const surfacePath = path.resolve(process.argv[2] || defaultSurfacePath);
const source = fs.readFileSync(surfacePath, "utf8");
const hooks = {};
let definition = null;
const context = {
  __LOOM_STOCK_MONITOR_TEST_HOOKS__: hooks,
  NeuroSurface: {
    define(value) {
      definition = value;
    },
    emit() {
      return true;
    },
  },
  clearInterval,
  clearTimeout,
  console,
  setInterval,
  setTimeout,
};
context.globalThis = context;
vm.createContext(context);
vm.runInContext(source, context, { filename: surfacePath });

assert.ok(definition, "Stock Monitor Surface did not register a definition");
assert.equal(typeof hooks.applyRevision, "function", "revision hook is missing");
assert.equal(typeof hooks.refreshPlan, "function", "refresh-plan hook is missing");

assert.deepEqual(
  { ...hooks.refreshPlan(1, "open") },
  { cadence: 1, key: "1:1:tick", normalized: 1, usesTick: true },
);
assert.deepEqual(
  { ...hooks.refreshPlan(1, "closed") },
  { cadence: 30, key: "1:30:tick", normalized: 1, usesTick: true },
);
assert.deepEqual(
  { ...hooks.refreshPlan(60, "open") },
  { cadence: 60, key: "60:60:full", normalized: 60, usesTick: false },
);

hooks.beginTick(7);
assert.deepEqual({ ...hooks.tickState() }, { pending: true, revision: 7 });
hooks.applyRevision(7);
assert.deepEqual(
  { ...hooks.tickState() },
  { pending: true, revision: 7 },
  "an unchanged snapshot revision must not release the tick lock",
);
hooks.applyRevision(8);
assert.deepEqual({ ...hooks.tickState() }, { pending: false, revision: -1 });

const chinaPalette = hooks.paletteFor("SZ");
assert.equal(chinaPalette.redUp, true);
assert.equal(chinaPalette.up, "#f43f5e");
assert.equal(chinaPalette.down, "#22c55e");
const usPalette = hooks.paletteFor("US");
assert.equal(usPalette.redUp, false);
assert.equal(usPalette.up, "#22c55e");
assert.equal(usPalette.down, "#f43f5e");

console.log("Stock Monitor Surface VM contract passed: revision-lock=verified cadence=open/closed palette=CN/US");
