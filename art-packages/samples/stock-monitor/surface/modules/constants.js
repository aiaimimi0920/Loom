// Constants and mutable lifecycle state shared by the assembled Surface.
const COLORS = Object.freeze({
    background: "#06080d",
    surface: "#090c11",
    panel: "#0e1218",
    control: "#111720",
    controlHover: "#18222c",
    text: "#f7f8ef",
    muted: "#929a9f",
    line: "rgba(255,255,255,0.12)",
    grid: "rgba(255,255,255,0.07)",
    yellow: "#d9ff38",
    green: "#22c55e",
    blue: "#06b6d4",
    red: "#f43f5e"
});
const INTERVALS = [1, 3, 5, 15, 30, 60, 120, 300];
const INTERVAL_LABELS = Object.freeze({
    1: "1s",
    3: "3s",
    5: "5s",
    15: "15s",
    30: "30s",
    60: "1m",
    120: "2m",
    300: "5m"
});
const DEFAULT_INTERVAL_SECONDS = 5;
const FULL_REFRESH_SECONDS = 60;
const CLOSED_MARKET_MIN_SECONDS = 30;
const TICK_ACTION = "stock_tick_refresh";
const INTERVAL_COMMIT_ACTION = "stock_interval_commit";
// Cooldown after a host rejects a tick action; re-probe the near-realtime channel once it expires instead of degrading permanently.
const TICK_RETRY_COOLDOWN_MILLIS = 300000;
const RED_UP_MARKETS = Object.freeze(["SH", "SZ", "BJ", "HK"]);
const PERIODS = Object.freeze([
    ["minute", "Intraday"],
    ["five-day", "5D"],
    ["day", "1D"],
    ["week", "1W"],
    ["month", "1M"],
    ["quarter", "1Q"],
    ["year", "1Y"],
    ["minute-120", "120m"],
    ["minute-60", "60m"],
    ["minute-30", "30m"],
    ["minute-15", "15m"],
    ["minute-5", "5m"],
    ["minute-1", "1m"]
]);
const PERIOD_VALUES = PERIODS.map((period) => period[0]);
const MAX_CANVAS_WIDTH = 2048;
const MAX_CANVAS_HEIGHT = 1024;
const MAX_CANVAS_PIXELS = 4 * 1024 * 1024;
const MAX_HISTORY_ROWS = 2000;
const MAX_BOOK_LEVELS = 10;
const MAX_FAVORITE_QUOTES = 8;
const MAX_UI_TEXT_CHARS = 400;
const MOVING_AVERAGE_WINDOW = 5;
const HISTORY_ROW_CELLS = 6;
const HISTORY_ROW_COUNT = 9;
const HISTORY_HEAD_CELLS = Object.freeze([
    { text: "Time" }, { text: "Open" }, { text: "High" },
    { text: "Low" }, { text: "Close" }, { text: "Volume" }
]);
const HISTORY_EMPTY_CELLS = Object.freeze([
    { text: "--" }, { text: "--" }, { text: "--" },
    { text: "--" }, { text: "--" }, { text: "--" }
]);
// Host-side action budgets mirroring manifest.json metadata.capabilities.surface.actions[].timeoutMs
// and art.runtime.json limits.timeoutMs; used only before the runtime publishes state.actionBudgetsMillis.
const ACTION_TIMEOUT_MILLIS = 50000;
const TICK_ACTION_TIMEOUT_MILLIS = 30000;
const INTERVAL_COMMIT_TIMEOUT_MILLIS = 5000;
// Host deadlines start when execution begins, excluding queueing, process startup, and patch delivery.
// Timed-out actions do not advance revision, so the client fallback must fire after the host gives up.
const ACTION_DISPATCH_GRACE_MILLIS = 8000;
const MIN_CLIENT_TIMEOUT_MILLIS = 3000;
const MAX_CLIENT_TIMEOUT_MILLIS = 180000;
const PENDING_TIMEOUT_MILLIS = ACTION_TIMEOUT_MILLIS + ACTION_DISPATCH_GRACE_MILLIS;
const VIEW_IDS = Object.freeze(["full", "chart-table", "trade-price", "favorites-summary"]);

let rootElement = null;
let snapshotValue = null;
let refs = {};
let refreshTimer = null;
let pendingTimer = null;
let tickTimer = null;
let initialRefreshTimer = null;
let resizeObserver = null;
let adoptedStyleSheet = null;
let suspended = false;
let disposed = false;
let pending = false;
let pendingAction = null;
let pendingPeriod = null;
let pendingRevision = -1;
let pendingRequestId = null;
let tickPending = false;
let tickPendingRevision = -1;
let tickRequestId = null;
let tickDisabledUntil = 0;
let requestCounter = 0;
let liveTickCount = 0;
let ticksSinceFullRefresh = 0;
let activeInterval = DEFAULT_INTERVAL_SECONDS;
let activeTimerKey = "";
let chartGeometry = null;
let hoverIndex = -1;
let hoverFrame = null;
let hoverPointer = null;
let resizeFrame = null;
let seriesCache = null;
let sampleCache = null;
let paintedKey = "";
let legendPaintedKey = "";
let chartPaintedKey = "";
