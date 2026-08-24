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
const packageRoot = path.dirname(path.dirname(surfacePath));
const manifestPath = path.join(packageRoot, "manifest.json");
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const javascriptVariant = manifest.metadata.capabilities.surface.variants.find(
  (variant) => variant.runtime === "javascript" && path.resolve(packageRoot, variant.entry) === surfacePath,
);
assert.ok(javascriptVariant, "Stock Monitor JavaScript Surface variant is missing");
const descriptorPath = surfacePath + ".sources.json";
const descriptor = JSON.parse(fs.readFileSync(descriptorPath, "utf8"));
assert.equal(descriptor.schemaVersion, 1, "Surface source descriptor schema mismatch");
const sourceFiles = descriptor.sourceFiles || [];
assert.ok(sourceFiles.length > 0 && sourceFiles.length <= 32, "Surface source descriptor size is invalid");
assert.equal(new Set(sourceFiles).size, sourceFiles.length, "Surface source files must be unique");
assert.ok(
  sourceFiles.every((sourceFile) => sourceFile.endsWith(".js") && sourceFile !== javascriptVariant.entry),
  "Surface source files must be JavaScript modules and must not repeat entry",
);
assert.deepEqual(sourceFiles, [
  "surface/modules/constants.js",
  "surface/modules/data.js",
  "surface/modules/template.js",
  "surface/modules/actions.js",
  "surface/modules/dom-summary.js",
  "surface/modules/dom-market.js",
  "surface/modules/chart.js",
  "surface/modules/chart-interaction.js",
  "surface/modules/render.js",
  "surface/modules/lifecycle.js",
]);
const packagePrefix = packageRoot.endsWith(path.sep) ? packageRoot : packageRoot + path.sep;
const readPackageSource = (relativePath) => {
  const resolved = path.resolve(packageRoot, relativePath);
  assert.ok(resolved.startsWith(packagePrefix), "Surface source escaped its package: " + relativePath);
  return fs.readFileSync(resolved, "utf8");
};
const source = sourceFiles.length
  ? '(() => {\n"use strict";\n'
    + [...sourceFiles.map(readPackageSource), fs.readFileSync(surfacePath, "utf8")].join("\n;\n")
    + "\n;\n\n})();\n"
  : fs.readFileSync(surfacePath, "utf8");
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
assert.equal(typeof hooks.viewOf, "function", "view resolver hook is missing");
assert.equal(hooks.viewOf({ viewId: "favorites-summary" }), "favorites-summary");
assert.equal(hooks.viewOf({ viewId: "unknown" }), "full");

assert.equal(typeof hooks.disableTickChannel, "function", "tick-channel hook is missing");

assert.deepEqual(
  { ...hooks.refreshPlan(1, "open") },
  { cadence: 1, key: "1:1:tick", normalized: 1, ticksPerFullRefresh: 60, usesTick: true },
);
assert.deepEqual(
  { ...hooks.refreshPlan(1, "closed") },
  { cadence: 30, key: "1:30:tick", normalized: 1, ticksPerFullRefresh: 2, usesTick: true },
);
assert.deepEqual(
  { ...hooks.refreshPlan(60, "open") },
  { cadence: 60, key: "60:60:full", normalized: 60, ticksPerFullRefresh: 0, usesTick: false },
);

// A host that refuses the tick action must also slow the cadence down: a 1-second full
// refresh is four upstream calls per second, which is the load the tick channel avoids.
hooks.disableTickChannel();
assert.deepEqual(
  { ...hooks.refreshPlan(1, "open") },
  { cadence: 60, key: "1:60:full", normalized: 1, ticksPerFullRefresh: 0, usesTick: false },
  "an unavailable tick channel must raise the cadence to the full-refresh period",
);
hooks.enableTickChannel();
assert.equal(
  hooks.refreshPlan(1, "open").usesTick,
  true,
  "the tick channel must be probed again once the cooldown is cleared",
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

// The moving average is now a rolling accumulator instead of a slice/map/reduce per point.
// Compare it against the naive definition it replaced, because an off-by-one in the window
// would draw a plausible-looking but wrong MA5 line.
assert.equal(typeof hooks.movingAverages, "function", "moving-average hook is missing");
const closes = [10, 11, 9, 12, 13, 15, 14, 12, 11, 16, 18, 17];
const points = closes.map((close) => ({ close: close }));
const naiveDaily = closes.map((_close, index) => {
  if (index < 4) return null;
  const window = closes.slice(index - 4, index + 1);
  return window.reduce((sum, value) => sum + value, 0) / window.length;
});
const naiveIntraday = closes.map((_close, index) => {
  const window = closes.slice(0, index + 1);
  return window.reduce((sum, value) => sum + value, 0) / window.length;
});
const rollingDaily = hooks.movingAverages(points, false);
const rollingIntraday = hooks.movingAverages(points, true);
assert.equal(rollingDaily.length, closes.length, "MA5 must produce one value per point");
naiveDaily.forEach((expected, index) => {
  if (expected === null) {
    assert.equal(rollingDaily[index], null, "MA5 must stay empty for the first four points");
    return;
  }
  assert.ok(
    Math.abs(rollingDaily[index] - expected) < 1e-9,
    "MA5 at index " + index + " is " + rollingDaily[index] + ", expected " + expected,
  );
});
naiveIntraday.forEach((expected, index) => {
  assert.ok(
    Math.abs(rollingIntraday[index] - expected) < 1e-9,
    "intraday average at index " + index + " is " + rollingIntraday[index] + ", expected " + expected,
  );
});

// The derived chart series is memoized on (code, period, revision, row count). A repeated
// draw at the same revision must hand back the same object; a new revision must not.
assert.equal(typeof hooks.chartSampleOf, "function", "chart-sample hook is missing");
const sampleState = {
  code: "SZ000034",
  period: "day",
  history: closes.map((close, index) => ({
    date: "2026-08-0" + ((index % 9) + 1),
    open: close - 1,
    close: close,
    high: close + 1,
    low: close - 2,
    volume: 1000 + index,
  })),
};
const firstSample = hooks.chartSampleOf(sampleState, 4, 240, false);
assert.equal(firstSample.points.length, closes.length);
assert.equal(
  hooks.chartSampleOf(sampleState, 4, 240, false),
  firstSample,
  "the same revision must reuse the cached sample instead of rebuilding the series",
);
assert.notEqual(
  hooks.chartSampleOf(sampleState, 5, 240, false),
  firstSample,
  "a new revision must invalidate the cached sample",
);

assert.equal(typeof hooks.historyOf, "function", "bounded history hook is missing");
assert.equal(typeof hooks.favoriteQuotesOf, "function", "bounded favorites hook is missing");
assert.equal(typeof hooks.orderBookOf, "function", "bounded order-book hook is missing");
const oversizedHistory = Array.from({ length: 2200 }, (_value, index) => ({ index }));
assert.equal(hooks.historyOf({ history: oversizedHistory }).length, 2000);
assert.equal(hooks.historyOf({ history: oversizedHistory })[0].index, 200);
assert.equal(hooks.favoriteQuotesOf({ favoriteQuotes: Array.from({ length: 20 }) }).length, 8);
const boundedBook = hooks.orderBookOf({
  orderBook: {
    bids: Array.from({ length: 30 }, (_value, index) => ({ price: index + 1 })),
    asks: Array.from({ length: 30 }, (_value, index) => ({ price: index + 1 })),
  },
});
assert.equal(boundedBook.bids.length, 10);
assert.equal(boundedBook.asks.length, 10);
assert.notEqual(
  hooks.chartSampleOf(sampleState, 5, 120, false),
  hooks.chartSampleOf(sampleState, 5, 240, false),
  "a different point budget must produce its own sample",
);

// The runtime already decides staleness and no longer synthesizes an observation time, so the
// panel must say so: an ageless record is the fail-closed case and must not read as fresh.
assert.equal(typeof hooks.staleLabel, "function", "stale-label hook is missing");
assert.equal(hooks.staleLabel(null), "");
assert.equal(hooks.staleLabel({ stale: false, ageSeconds: 900 }), "");
assert.equal(hooks.staleLabel({ stale: true, ageSeconds: 132, maxAgeSeconds: 90 }), "已陈旧 132/90 秒");
assert.equal(hooks.staleLabel({ stale: true, ageSeconds: 132 }), "已陈旧 132 秒");
assert.equal(
  hooks.staleLabel({ stale: true, ageSeconds: null, maxAgeSeconds: 90 }),
  "已陈旧（观测时间不可用）",
  "a record whose age is unknown must be labelled, not silently drawn as if it were current",
);

// A quote that arrived without its chart stays status=ready, so the footer is the only place
// the failure can surface. It must not be dressed as a fatal error, and a real error wins.
assert.equal(typeof hooks.footerNoticeOf, "function", "footer-notice hook is missing");
assert.deepEqual({ ...hooks.footerNoticeOf({}) }, { text: "", warning: false });
assert.deepEqual(
  { ...hooks.footerNoticeOf({ historyWarning: "  upstream history failure  " }) },
  { text: "K 线数据不可用：upstream history failure", warning: true },
);
assert.deepEqual(
  { ...hooks.footerNoticeOf({ error: "行情获取失败", historyWarning: "upstream history failure" }) },
  { text: "行情获取失败", warning: false },
  "a fatal error must keep the footer slot and its error styling",
);

const sourceText = source;
assert.ok(
  sourceText.includes('".book-meta.is-stale{color:" + COLORS.yellow')
    && sourceText.includes('".tape-strip.is-stale strong{color:" + COLORS.yellow')
    && sourceText.includes('".stock-error.is-warning{color:" + COLORS.yellow'),
  "the stale badge and the non-fatal warning must render in the warning color, not the error color",
);
assert.ok(
  /refs\.bookMeta\.classList\.toggle\("is-stale"/.test(sourceText)
    && /refs\.tape\.classList\.toggle\("is-stale"/.test(sourceText),
  "both the order book meta line and the tape strip must toggle the stale class they style",
);
assert.ok(
  !/\n\s*canvas\.width = /.test(sourceText),
  "canvas.width must only be assigned when the pixel size actually changed",
);
assert.ok(
  sourceText.includes("if (canvas.width !== nextWidth)")
    && sourceText.includes("if (canvas.height !== nextHeight)"),
  "the canvas resize must be gated on a size change to avoid reallocating the backing bitmap",
);
assert.ok(
  sourceText.includes("new ResizeObserver(scheduleChartRedraw)"),
  "resize redraws must go through the animation-frame coalescer, not straight into drawChart",
);
assert.ok(
  sourceText.includes("if (disposed || suspended || resizeFrame !== null) return;")
    && sourceText.includes("if (!disposed && !suspended) drawChart();"),
  "a suspended Surface must not allocate or execute a full chart redraw",
);
assert.ok(
  !/replaceChildren\(\);\s*\n\s*levels\.forEach/.test(sourceText),
  "the order book must reuse its rows instead of rebuilding them every frame",
);
assert.ok(
  sourceText.includes("MAX_HISTORY_ROWS = 2000")
    && sourceText.includes("MAX_BOOK_LEVELS = 10")
    && sourceText.includes("MAX_FAVORITE_QUOTES = 8"),
  "the Surface must independently bound host-supplied collections",
);
assert.ok(
  sourceText.includes("chartKey !== chartPaintedKey")
    && sourceText.includes("initialRefreshTimer")
    && sourceText.includes("clearTimeout(initialRefreshTimer)"),
  "chart paints and delayed initial refreshes must be lifecycle-gated",
);
assert.ok(
  !/Math\.(min|max)\(\.\.\.points\.map/.test(sourceText),
  "chart extrema must be derived in one bounded pass without intermediate arrays",
);

console.log("Stock Monitor Surface VM contract passed: revision-lock=verified cadence=open/closed/no-tick palette=CN/US ma5=rolling sample-cache=verified stale-badge=verified history-warning=verified");
