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
    1: "1 秒",
    3: "3 秒",
    5: "5 秒",
    15: "15 秒",
    30: "30 秒",
    60: "1 分",
    120: "2 分",
    300: "5 分"
});
const DEFAULT_INTERVAL_SECONDS = 5;
const FULL_REFRESH_SECONDS = 60;
const CLOSED_MARKET_MIN_SECONDS = 30;
const TICK_ACTION = "stock_tick_refresh";
const INTERVAL_COMMIT_ACTION = "stock_interval_commit";
// 宿主拒绝一次 tick 后的冷却时间。过期后重新探测准实时通道，而不是整个生命周期都降级。
const TICK_RETRY_COOLDOWN_MILLIS = 300000;
const RED_UP_MARKETS = Object.freeze(["SH", "SZ", "BJ", "HK"]);
const PERIODS = Object.freeze([
    ["minute", "分时"],
    ["five-day", "五日"],
    ["day", "日 K"],
    ["week", "周 K"],
    ["month", "月 K"],
    ["quarter", "季 K"],
    ["year", "年 K"],
    ["minute-120", "120 分钟"],
    ["minute-60", "60 分钟"],
    ["minute-30", "30 分钟"],
    ["minute-15", "15 分钟"],
    ["minute-5", "5 分钟"],
    ["minute-1", "1 分钟"]
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
    { text: "时间" }, { text: "开" }, { text: "高" },
    { text: "低" }, { text: "收" }, { text: "成交量" }
]);
const HISTORY_EMPTY_CELLS = Object.freeze([
    { text: "--" }, { text: "--" }, { text: "--" },
    { text: "--" }, { text: "--" }, { text: "--" }
]);
// 宿主侧动作预算。这些数字镜像 manifest.json 的
// metadata.capabilities.surface.actions[].timeoutMs 与 art.runtime.json 的
// limits.timeoutMs；运行时会把实际生效值通过 state.actionBudgetsMillis 回传，
// 首帧之前（还没有任何回传）才用这里的常量。
const ACTION_TIMEOUT_MILLIS = 50000;
const TICK_ACTION_TIMEOUT_MILLIS = 30000;
const INTERVAL_COMMIT_TIMEOUT_MILLIS = 5000;
// 宿主的截止时间从任务开始计时，排队、进程启动、补丁回传都在这之外；而超时的动作
// 不会推进 revision，所以客户端计时器是唯一的兜底。它必须晚于宿主放弃的时刻，
// 否则会在请求仍在运行时清掉 pending 并误报超时。
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
