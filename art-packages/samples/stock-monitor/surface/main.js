(() => {
    "use strict";

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

    const asObject = (value) => value && typeof value === "object" && !Array.isArray(value) ? value : {};
    const asNumber = (value) => {
        const number = Number(value);
        return Number.isFinite(number) ? number : null;
    };
    const stateOf = (snapshot) => asObject(snapshot && snapshot.authoritativeState);
    const quoteOf = (state) => asObject(state.quote);
    const historyOf = (state) => Array.isArray(state.history) ? state.history : [];
    const favoriteQuotesOf = (state) => Array.isArray(state.favoriteQuotes) ? state.favoriteQuotes : [];
    const viewOf = (snapshot) => VIEW_IDS.includes(snapshot && snapshot.viewId) ? snapshot.viewId : "full";
    const periodOf = (state) => {
        const value = text(state.period, "minute");
        return PERIOD_VALUES.includes(value) ? value : "minute";
    };
    const periodLabelOf = (state) => {
        const value = periodOf(state);
        const found = PERIODS.find((period) => period[0] === value);
        // 标签只从本地封闭表取：state.periodLabel 由另一个进程写入，不能当作可信显示文本。
        return found ? found[1] : "日 K";
    };
    const isIntradayPeriod = (period) => period === "minute" || period === "five-day" || period.indexOf("minute-") === 0;
    const chartRowsOf = (state) => historyOf(state).map((item) => {
        const row = asObject(item);
        return {
            date: text(row.date, ""),
            open: asNumber(row.open),
            close: asNumber(row.close),
            high: asNumber(row.high),
            low: asNumber(row.low),
            volume: asNumber(row.volume) || 0
        };
    }).filter((item) => item.close !== null && item.open !== null && item.high !== null && item.low !== null && item.high >= item.low);
    const downsampleRows = (rows, maxPoints) => {
        if (rows.length <= maxPoints) return rows;
        const result = [];
        for (let index = 0; index < maxPoints; index += 1) {
            const start = Math.floor(index * rows.length / maxPoints);
            const end = Math.max(start + 1, Math.floor((index + 1) * rows.length / maxPoints));
            const bucket = rows.slice(start, end);
            const first = bucket[0];
            const last = bucket[bucket.length - 1];
            result.push({
                date: last.date,
                open: first.open,
                close: last.close,
                high: Math.max(...bucket.map((row) => row.high)),
                low: Math.min(...bucket.map((row) => row.low)),
                volume: bucket.reduce((sum, row) => sum + (row.volume || 0), 0)
            });
        }
        return result;
    };
    // 均线用滚动累加。每点重新 slice/map/reduce 一次，一次绘制要多出三倍于点数的中间数组。
    const movingAverages = (points, intraday) => {
        const values = new Array(points.length);
        let total = 0;
        if (intraday) {
            for (let index = 0; index < points.length; index += 1) {
                total += points[index].close;
                values[index] = total / (index + 1);
            }
            return values;
        }
        for (let index = 0; index < points.length; index += 1) {
            total += points[index].close;
            if (index >= MOVING_AVERAGE_WINDOW) total -= points[index - MOVING_AVERAGE_WINDOW].close;
            values[index] = index < MOVING_AVERAGE_WINDOW - 1 ? null : total / MOVING_AVERAGE_WINDOW;
        }
        return values;
    };
    const revisionOf = (snapshot) => {
        const value = Number(snapshot && snapshot.revision);
        return Number.isFinite(value) ? value : -1;
    };
    // 派生序列按 (代码, 周期, revision, 行数) 缓存。运行时的历史上限是 2000 行，最小刷新间隔
    // 是一秒，而重绘还会被 resize、悬浮和动作拒绝路径额外触发多次；没有缓存时每次都要重新
    // 分配整个序列。revision 只在状态真的变化时前进，所以它同时是正确性边界和失效条件。
    const chartSeriesOf = (state, revision) => {
        const key = text(state.code, text(state.symbol, "")) + "|" + periodOf(state) + "|" + revision + "|" + historyOf(state).length;
        if (seriesCache && seriesCache.key === key) return seriesCache.rows;
        seriesCache = { key: key, rows: chartRowsOf(state) };
        return seriesCache.rows;
    };
    const chartSampleOf = (state, revision, maxPoints, intraday) => {
        const rows = chartSeriesOf(state, revision);
        const key = seriesCache.key + "|" + maxPoints + "|" + (intraday ? "i" : "k");
        if (sampleCache && sampleCache.key === key) return sampleCache;
        const points = downsampleRows(rows, maxPoints);
        sampleCache = { key: key, points: points, averageValues: movingAverages(points, intraday) };
        return sampleCache;
    };
    const text = (value, fallback) => typeof value === "string" && value.trim() ? value : fallback;
    const formatNumber = (value, digits) => {
        const number = asNumber(value);
        return number === null ? "--" : number.toLocaleString("zh-CN", {
            minimumFractionDigits: digits,
            maximumFractionDigits: digits
        });
    };
    const formatSigned = (value, suffix) => {
        const number = asNumber(value);
        if (number === null) return "--";
        const sign = number > 0 ? "+" : "";
        return sign + formatNumber(number, 2) + (suffix || "");
    };
    const formatVolume = (value) => {
        const number = asNumber(value);
        if (number === null) return "--";
        if (Math.abs(number) >= 100000000) return formatNumber(number / 100000000, 2) + " 亿";
        if (Math.abs(number) >= 10000) return formatNumber(number / 10000, 2) + " 万";
        return formatNumber(number, 0);
    };
    const formatTimestamp = (value) => {
        if (typeof value !== "string" || !value.trim()) return "时间未知";
        const parsed = new Date(value);
        return Number.isNaN(parsed.getTime())
            ? value
            : parsed.toLocaleString("zh-CN", { hour12: false });
    };
    const formatClock = (value) => {
        if (typeof value !== "string" || !value.trim()) return "--:--:--";
        const parsed = new Date(value);
        return Number.isNaN(parsed.getTime())
            ? "--:--:--"
            : parsed.toLocaleTimeString("zh-CN", { hour12: false });
    };
    const normalizeCode = (value) => {
        const input = String(value || "").trim().toUpperCase().replace(/\s+/g, "");
        let match = input.match(/^(SH|SZ|BJ)[:._-]?(\d{6})$/);
        if (match) return match[1] + match[2];
        match = input.match(/^(\d{6})[:._-]?(SH|SZ|BJ)$/);
        if (match) return match[2] + match[1];
        if (/^\d{6}$/.test(input)) {
            const market = /^[48]/.test(input) ? "BJ" : /^[569]/.test(input) ? "SH" : "SZ";
            return market + input;
        }
        match = input.match(/^HK[:._-]?(\d{1,5})$/);
        if (match) return "HK" + match[1].padStart(5, "0");
        match = input.match(/^US[:_-]?([A-Z][A-Z0-9.-]{0,19})$/);
        return match ? "US" + match[1] : input;
    };
    const movement = (quote) => {
        const change = asNumber(quote.changePercent);
        if (change === null || change === 0) return "flat";
        return change > 0 ? "positive" : "negative";
    };
    // A 股 / 港股按中国市场惯例红涨绿跌；美股等海外市场绿涨红跌。
    const marketOf = (state, code) => text(state && state.market, String(code || "").slice(0, 2)).toUpperCase();
    const paletteFor = (market) => RED_UP_MARKETS.includes(String(market || "").toUpperCase())
        ? { up: COLORS.red, down: COLORS.green, redUp: true }
        : { up: COLORS.green, down: COLORS.red, redUp: false };
    const movementColor = (kind, palette) => kind === "positive"
        ? palette.up
        : kind === "negative" ? palette.down : COLORS.yellow;
    const deltaColor = (value, palette) => {
        const number = asNumber(value);
        if (number === null || number === 0) return COLORS.text;
        return number > 0 ? palette.up : palette.down;
    };
    const formatPointDate = (value, intraday) => {
        const raw = String(value || "").replace("T", " ").trim();
        if (!raw) return "--";
        if (intraday) return raw.length >= 16 ? raw.slice(0, 16) : raw;
        return raw.slice(0, 10);
    };

    const styleSource = [
        ":root{color-scheme:dark}",
        "html,body{background:transparent}",
        ".stock-shell{min-width:0;min-height:100%;height:100%;overflow:hidden;background:" + COLORS.background + ";color:" + COLORS.text + ";font-family:Segoe UI,Microsoft YaHei,sans-serif;font-size:12px;line-height:1.35;letter-spacing:0;display:grid;grid-template-rows:auto auto minmax(190px,1fr) auto auto auto}",
        ".stock-shell *{box-sizing:border-box;letter-spacing:0}",
        ".stock-header{display:grid;grid-template-columns:minmax(0,1fr) auto;align-items:center;gap:10px;padding:10px 12px 9px;background:" + COLORS.surface + ";border-bottom:1px solid " + COLORS.line + "}",
        ".stock-kicker{font:700 11px/1.2 Consolas,monospace;color:" + COLORS.yellow + ";white-space:nowrap}",
        ".stock-title{margin:2px 0 0;font-size:16px;line-height:1.2;font-weight:700;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-status{justify-self:end;max-width:180px;padding-left:9px;border-left:2px solid " + COLORS.yellow + ";color:" + COLORS.muted + ";font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-status.is-ready{border-color:" + COLORS.green + ";color:" + COLORS.green + "}",
        ".stock-status.is-error{border-color:" + COLORS.red + ";color:" + COLORS.red + "}",
        ".stock-status.is-loading{border-color:" + COLORS.yellow + ";color:" + COLORS.yellow + "}",
        ".stock-controls{display:grid;grid-template-columns:minmax(0,1fr) auto 32px;grid-template-rows:auto auto auto;align-items:center;gap:6px;padding:8px 12px;background:" + COLORS.surface + ";border-bottom:1px solid " + COLORS.line + "}",
        ".stock-control{height:30px;min-width:0;border:1px solid " + COLORS.line + ";border-radius:3px;background:" + COLORS.control + ";color:" + COLORS.text + ";font:600 12px/1 Segoe UI,Microsoft YaHei,sans-serif;outline:none}",
        ".stock-control:hover{background:" + COLORS.controlHover + "}",
        ".stock-control:focus-visible{outline:2px solid " + COLORS.yellow + ";outline-offset:1px}",
        ".stock-symbol{padding:0 9px;font-family:Consolas,monospace}",
        ".stock-intervals{grid-column:1/-1;grid-row:2;display:grid;grid-template-columns:repeat(8,minmax(0,1fr));align-items:center;gap:3px;min-width:0;padding-top:6px;border-top:1px solid " + COLORS.line + "}",
        ".stock-interval-option{height:26px;padding:0 3px;border:1px solid " + COLORS.line + ";border-radius:3px;background:" + COLORS.control + ";color:" + COLORS.muted + ";font:600 11px/1 Segoe UI,Microsoft YaHei,sans-serif;cursor:pointer;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-interval-option:hover{background:" + COLORS.controlHover + ";color:" + COLORS.text + "}",
        ".stock-interval-option.is-active{border-color:" + COLORS.yellow + ";background:" + COLORS.yellow + ";color:#11150a}",
        ".stock-interval-option.is-live.is-active{border-color:" + COLORS.blue + ";background:" + COLORS.blue + ";color:#04121a}",
        ".stock-interval-option:focus-visible,.stock-period:focus-visible{outline:2px solid " + COLORS.yellow + ";outline-offset:1px}",
        ".stock-refresh{display:grid;place-items:center;padding:0;border-color:" + COLORS.yellow + ";background:" + COLORS.yellow + ";color:#11150a;font-size:18px;font-weight:800;cursor:pointer}",
        ".stock-refresh:hover{background:color-mix(in srgb," + COLORS.yellow + " 86%,white)}",
        ".stock-refresh:disabled{cursor:wait;opacity:.62}",
        ".stock-periods{grid-column:1/-1;grid-row:3;display:grid;grid-template-columns:repeat(7,minmax(0,1fr));gap:3px;padding-top:6px;border-top:1px solid " + COLORS.line + "}",
        ".stock-period{min-width:0;height:26px;padding:0 3px;border:1px solid transparent;border-radius:3px;background:transparent;color:" + COLORS.muted + ";font:600 11px/1 Segoe UI,Microsoft YaHei,sans-serif;cursor:pointer;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-period:hover{border-color:" + COLORS.line + ";background:" + COLORS.controlHover + ";color:" + COLORS.text + "}",
        ".stock-period.is-active{border-color:" + COLORS.yellow + ";background:" + COLORS.yellow + ";color:#11150a}",
        ".stock-period:disabled{cursor:wait;opacity:.62}",
        ".quote-board{display:grid;grid-template-columns:minmax(160px,.75fr) minmax(220px,1.25fr);min-height:0;background:" + COLORS.surface + ";border-bottom:1px solid " + COLORS.line + "}",
        ".quote-summary{min-width:0;padding:12px;border-right:1px solid " + COLORS.line + ";display:flex;flex-direction:column;justify-content:center}",
        ".quote-identity{display:flex;align-items:baseline;gap:7px;min-width:0}",
        ".quote-name{font-size:14px;font-weight:700;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".quote-code{color:" + COLORS.muted + ";font:600 11px/1 Consolas,monospace;white-space:nowrap}",
        ".quote-price{margin-top:5px;font:700 34px/1 Consolas,monospace;color:" + COLORS.text + ";white-space:nowrap}",
        ".quote-delta{margin-top:6px;font:700 13px/1.2 Consolas,monospace;white-space:nowrap}",
        ".quote-session{margin-top:9px;color:" + COLORS.muted + ";font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".chart-wrap{position:relative;min-width:0;min-height:160px;overflow:hidden;contain:layout paint;padding:9px 10px 7px;background:" + COLORS.surface + ";touch-action:none}",
        ".chart-legend{position:absolute;z-index:1;top:8px;left:10px;right:10px;display:flex;gap:8px;pointer-events:none;color:" + COLORS.muted + ";font-size:10px}",
        ".legend-item{display:inline-flex;align-items:center;gap:4px;white-space:nowrap}",
        ".legend-line{width:14px;height:2px;background:" + COLORS.blue + "}",
        ".legend-line.candle{background:linear-gradient(90deg," + COLORS.green + " 0 48%," + COLORS.red + " 52% 100%)}",
        ".legend-line.close{background:" + COLORS.blue + "}",
        ".legend-line.average{background:" + COLORS.yellow + "}",
        ".legend-line.volume{background:" + COLORS.blue + ";opacity:.65}",
        ".stock-chart{position:absolute;inset:0;display:block;width:100%;height:100%;min-width:0;min-height:0}",
        ".chart-overlay{position:absolute;inset:0;display:block;width:100%;height:100%;min-width:0;min-height:0;pointer-events:none;z-index:2}",
        ".chart-tip{position:absolute;z-index:3;min-width:126px;max-width:196px;padding:6px 8px;border:1px solid " + COLORS.line + ";border-radius:3px;background:rgba(9,12,17,.95);box-shadow:0 6px 18px rgba(0,0,0,.5);color:" + COLORS.text + ";font:600 10px/1.55 Consolas,monospace;pointer-events:none;visibility:hidden;opacity:0;transition:opacity .08s linear}",
        ".chart-tip.is-visible{visibility:visible;opacity:1}",
        ".chart-tip-title{margin-bottom:3px;padding-bottom:3px;border-bottom:1px solid " + COLORS.line + ";color:" + COLORS.yellow + ";white-space:nowrap}",
        ".chart-tip-row{display:flex;justify-content:space-between;gap:12px;white-space:nowrap}",
        ".chart-tip-key{color:" + COLORS.muted + ";font-weight:600}",
        ".chart-tip-value{font-weight:700}",
        ".market-grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));background:" + COLORS.panel + ";border-bottom:1px solid " + COLORS.line + "}",
        ".market-cell{min-width:0;padding:8px 9px;border-right:1px solid " + COLORS.line + ";border-bottom:1px solid " + COLORS.line + "}",
        ".market-cell:nth-child(4n){border-right:0}",
        ".metric-label{display:block;color:" + COLORS.muted + ";font-size:10px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".metric-value{display:block;margin-top:3px;color:" + COLORS.text + ";font:600 12px/1.2 Consolas,monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".book-board{display:none;gap:6px;padding:8px 12px;background:" + COLORS.panel + ";border-bottom:1px solid " + COLORS.line + "}",
        ".book-board.is-visible{display:grid}",
        ".book-board .is-hidden{display:none}",
        ".book-head{display:grid;grid-template-columns:auto minmax(0,1fr);gap:10px;align-items:baseline}",
        ".book-title{color:" + COLORS.yellow + ";font:700 11px/1.2 Consolas,monospace;white-space:nowrap}",
        ".book-meta{justify-self:end;min-width:0;color:" + COLORS.muted + ";font:600 10px/1.2 Consolas,monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".book-meta.is-stale{color:" + COLORS.yellow + "}",
        ".book-bar{display:flex;height:4px;border-radius:2px;overflow:hidden;background:" + COLORS.control + "}",
        ".book-bar span{display:block;height:100%}",
        ".book-columns{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:4px 10px;min-width:0}",
        ".book-side{display:grid;align-content:start;gap:2px;min-width:0}",
        ".book-row{display:grid;grid-template-columns:auto minmax(0,1fr) auto;gap:6px;align-items:center;padding:2px 5px;border-radius:2px;background:" + COLORS.control + ";font:700 11px/1.35 Consolas,monospace;white-space:nowrap}",
        ".book-tag{color:" + COLORS.muted + ";font-size:10px;font-weight:600}",
        ".book-price{min-width:0;text-align:right;overflow:hidden;text-overflow:ellipsis}",
        ".book-volume{color:" + COLORS.muted + ";font-size:10px;font-weight:600;text-align:right}",
        ".tape-strip{display:flex;flex-wrap:wrap;gap:3px 10px;min-width:0;color:" + COLORS.muted + ";font:600 10px/1.45 Consolas,monospace}",
        ".tape-strip.is-stale strong{color:" + COLORS.yellow + "}",
        ".tape-item{white-space:nowrap}",
        ".tape-item strong{margin-left:4px;color:" + COLORS.text + ";font-weight:700}",
        ".history-board,.favorites-board{display:none;min-width:0;min-height:0;background:" + COLORS.panel + ";border-bottom:1px solid " + COLORS.line + "}",
        ".history-board{grid-template-rows:auto minmax(0,1fr)}",
        ".section-head{display:flex;align-items:center;justify-content:space-between;gap:10px;padding:7px 12px;border-bottom:1px solid " + COLORS.line + ";color:" + COLORS.yellow + ";font:700 11px/1.2 Consolas,monospace}",
        ".history-table{display:grid;grid-template-rows:auto repeat(8,minmax(0,1fr));min-height:0;padding:0 12px 8px}",
        ".history-row{display:grid;grid-template-columns:minmax(120px,1.3fr) repeat(5,minmax(72px,1fr));align-items:center;min-height:0;border-bottom:1px solid " + COLORS.grid + ";font:600 11px/1.2 Consolas,monospace}",
        ".history-row>span{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;text-align:right}",
        ".history-row>span:first-child{text-align:left;color:" + COLORS.muted + "}",
        ".history-row.is-head{color:" + COLORS.muted + ";font-size:10px}",
        ".favorites-board{grid-template-rows:auto minmax(0,1fr)}",
        ".favorites-refresh{height:26px;padding:0 9px;border:1px solid " + COLORS.yellow + ";background:transparent;color:" + COLORS.yellow + ";font:700 10px/1 Consolas,monospace;cursor:pointer}",
        ".favorites-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));grid-auto-rows:minmax(0,1fr);gap:1px;min-height:0;background:" + COLORS.line + "}",
        ".favorite-card{min-width:0;padding:12px;background:" + COLORS.panel + ";display:grid;grid-template-columns:minmax(0,1fr) auto;grid-template-rows:auto auto;align-content:center;gap:5px 10px}",
        ".favorite-name{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:13px;font-weight:700}",
        ".favorite-code{color:" + COLORS.muted + ";font:600 10px/1 Consolas,monospace}",
        ".favorite-price{grid-column:2;grid-row:1/3;align-self:center;font:700 22px/1 Consolas,monospace}",
        ".favorite-delta{font:700 11px/1.2 Consolas,monospace}",
        ".favorites-empty{grid-column:1/-1;display:grid;place-items:center;color:" + COLORS.muted + ";font:600 12px/1.4 Segoe UI,Microsoft YaHei,sans-serif}",
        ".stock-footer{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:10px;align-items:center;padding:7px 12px;background:" + COLORS.surface + ";color:" + COLORS.muted + ";font-size:10px}",
        ".stock-error{min-width:0;color:" + COLORS.red + ";white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-error.is-warning{color:" + COLORS.yellow + "}",
        ".stock-disclaimer{text-align:right;white-space:nowrap}",
        ".stock-shell[data-view=chart-table]{grid-template-rows:auto auto minmax(190px,.95fr) auto minmax(220px,1.05fr) auto}",
        ".stock-shell[data-view=chart-table] .book-board{display:none!important}",
        ".stock-shell[data-view=chart-table] .history-board{display:grid}",
        ".stock-shell[data-view=trade-price]{grid-template-rows:auto auto minmax(126px,.55fr) auto minmax(0,1.45fr) auto}",
        ".stock-shell[data-view=trade-price] .stock-controls{grid-template-rows:auto}",
        ".stock-shell[data-view=trade-price] .stock-intervals,.stock-shell[data-view=trade-price] .stock-periods{display:none}",
        ".stock-shell[data-view=trade-price] .quote-board{grid-template-columns:1fr}",
        ".stock-shell[data-view=trade-price] .quote-summary{border-right:0}",
        ".stock-shell[data-view=trade-price] .chart-wrap{display:none}",
        ".stock-shell[data-view=trade-price] .market-grid{grid-template-columns:repeat(4,minmax(0,1fr))}",
        ".stock-shell[data-view=trade-price] .book-board{display:grid}",
        ".stock-shell[data-view=favorites-summary]{grid-template-rows:auto minmax(0,1fr) auto}",
        ".stock-shell[data-view=favorites-summary] .stock-controls,.stock-shell[data-view=favorites-summary] .quote-board,.stock-shell[data-view=favorites-summary] .market-grid,.stock-shell[data-view=favorites-summary] .book-board,.stock-shell[data-view=favorites-summary] .history-board{display:none!important}",
        ".stock-shell[data-view=favorites-summary] .favorites-board{display:grid}",
        "@media(max-width:560px){.stock-shell{grid-template-rows:auto auto auto minmax(160px,1fr) auto auto auto}.stock-header{padding:9px 10px}.stock-status{max-width:115px}.stock-controls{grid-template-columns:minmax(0,1fr) auto 32px;padding:7px 10px}.stock-intervals{grid-template-columns:repeat(4,minmax(0,1fr))}.stock-periods{grid-template-columns:repeat(4,minmax(0,1fr))}.quote-board{display:contents}.quote-summary{padding:10px;border-right:0;border-bottom:1px solid " + COLORS.line + "}.quote-price{font-size:30px}.chart-wrap{min-height:175px}.market-grid{grid-template-columns:repeat(2,minmax(0,1fr))}.market-cell:nth-child(4n){border-right:1px solid " + COLORS.line + "}.market-cell:nth-child(2n){border-right:0}.book-board{padding:8px 10px}.book-head{grid-template-columns:1fr}.book-meta{justify-self:start}.stock-footer{grid-template-columns:1fr}.stock-disclaimer{text-align:left;white-space:normal}}",
        "@media(max-width:390px){.stock-title{font-size:14px}.stock-status{display:none}.stock-header{grid-template-columns:1fr}.stock-controls{grid-template-columns:minmax(0,1fr) 32px 30px}.stock-refresh{grid-column:2;grid-row:1}}",
        "@media(prefers-reduced-motion:reduce){.stock-shell *{scroll-behavior:auto!important;transition:none!important}}"
    ].join("");

    const markup = [
        '<section class="stock-shell" data-ref="shell" aria-label="股票盯盘">',
        '<header class="stock-header">',
        '<div><div class="stock-kicker">NEURO MARKET WATCH</div><h1 class="stock-title" data-ref="title">股票盯盘</h1></div>',
        '<div class="stock-status" data-ref="status" role="status" aria-live="polite">等待首次刷新</div>',
        '</header>',
        '<div class="stock-controls">',
        '<input class="stock-control stock-symbol" data-ref="symbol" aria-label="股票统一代码" inputmode="text" maxlength="22" value="SZ000034">',
        '<div class="stock-intervals" data-ref="intervals" role="group" aria-label="刷新间隔">',
        INTERVALS.map((seconds) => '<button class="stock-interval-option' + (seconds <= 5 ? ' is-live' : '') + '" type="button" data-interval-value="' + seconds + '"'
            + (seconds <= 5 ? ' title="准实时：每 ' + seconds + ' 秒只拉最新一笔报价"' : '')
            + '>' + INTERVAL_LABELS[seconds] + '</button>').join(""),
        '</div>',
        '<button class="stock-control stock-refresh" data-ref="refresh" type="button" aria-label="刷新行情" title="刷新行情">↻</button>',
        '<div class="stock-periods" data-ref="periods" role="tablist" aria-label="行情周期">',
        PERIODS.map((period) => '<button class="stock-period" type="button" role="tab" data-period-value="' + period[0] + '" aria-selected="false">' + period[1] + '</button>').join(""),
        '</div>',
        '</div>',
        '<section class="quote-board">',
        '<div class="quote-summary">',
        '<div class="quote-identity"><span class="quote-name" data-ref="name">等待行情</span><span class="quote-code" data-ref="code">SZ000034 · SZ</span></div>',
        '<div class="quote-price" data-ref="price">--</div>',
        '<div class="quote-delta" data-ref="delta">等待真实价格</div>',
        '<div class="quote-session" data-ref="session">数据源：stock-api MCP</div>',
        '</div>',
        '<div class="chart-wrap" data-ref="chartWrap">',
        '<div class="chart-legend" data-ref="legend"></div>',
        '<canvas class="stock-chart" data-ref="chart" aria-label="日 K 线与成交量图"></canvas>',
        '<canvas class="chart-overlay" data-ref="overlay" aria-hidden="true"></canvas>',
        '<div class="chart-tip" data-ref="tip" role="tooltip" aria-hidden="true"></div>',
        '</div>',
        '</section>',
        '<section class="market-grid" data-ref="metrics"></section>',
        '<section class="book-board" data-ref="book" aria-label="十档盘口">',
        '<div class="book-head"><span class="book-title" data-ref="bookTitle">十档盘口</span><span class="book-meta" data-ref="bookMeta"></span></div>',
        '<div class="book-bar" data-ref="bookBar" aria-hidden="true"><span data-ref="bookBuyBar"></span><span data-ref="bookSellBar"></span></div>',
        '<div class="book-columns"><div class="book-side" data-ref="bids"></div><div class="book-side" data-ref="asks"></div></div>',
        '<div class="tape-strip" data-ref="tape"></div>',
        '</section>',
        '<section class="history-board" data-ref="historyBoard" aria-label="折线行情表格">',
        '<div class="section-head"><span data-ref="historyTitle">最近行情</span><span data-ref="historyMeta"></span></div>',
        '<div class="history-table" data-ref="historyRows"></div>',
        '</section>',
        '<section class="favorites-board" data-ref="favoritesBoard" aria-label="收藏股票价格汇总">',
        '<div class="section-head"><span>收藏股票价格汇总</span><button class="favorites-refresh" data-ref="favoritesRefresh" type="button">刷新全部</button></div>',
        '<div class="favorites-grid" data-ref="favoritesGrid"></div>',
        '</section>',
        '<footer class="stock-footer"><span class="stock-error" data-ref="error"></span><span class="stock-disclaimer" data-ref="disclaimer">行情可能延迟，不构成投资建议或交易指令</span></footer>',
        '</section>'
    ].join("");

    const metricDefinitions = [
        ["开盘", (quote) => formatNumber(quote.open, 2)],
        ["最高", (quote) => formatNumber(quote.high, 2)],
        ["最低", (quote) => formatNumber(quote.low, 2)],
        ["昨收", (quote) => formatNumber(quote.previousClose, 2)],
        ["周期", (_quote, history, state) => history.length ? periodLabelOf(state) + " · " + history.length + " 条" : periodLabelOf(state)],
        ["最新量", (_quote, history) => history.length ? formatVolume(asObject(history[history.length - 1]).volume) : "--"],
        ["市场", (_quote, _history, state) => text(state.marketStatus, "closed") === "open" ? "交易中" : "休市"],
        ["最近交易日", (_quote, _history, state) => text(state.lastTradingDate, "--")]
    ];

    // 客户端兜底计时器必须晚于宿主的动作预算：优先用运行时回传的真实预算，没有回传时
    // 退回镜像常量。宿主超时既不会推补丁也不会推进 revision，所以这条计时器早于宿主
    // 触发就会在请求仍在运行时解锁控件，允许第二次并发请求。
    const hostBudgetOf = (action) => {
        const published = asObject(stateOf(snapshotValue).actionBudgetsMillis);
        const declared = asNumber(published[action]);
        if (declared !== null && declared > 0) return declared;
        if (action === TICK_ACTION) return TICK_ACTION_TIMEOUT_MILLIS;
        if (action === INTERVAL_COMMIT_ACTION) return INTERVAL_COMMIT_TIMEOUT_MILLIS;
        return ACTION_TIMEOUT_MILLIS;
    };

    const clientDeadlineOf = (action) => Math.min(
        MAX_CLIENT_TIMEOUT_MILLIS,
        Math.max(MIN_CLIENT_TIMEOUT_MILLIS, hostBudgetOf(action) + ACTION_DISPATCH_GRACE_MILLIS)
    );

    const tickChannelEnabled = () => tickDisabledUntil === 0 || Date.now() >= tickDisabledUntil;

    const emitAction = (nodeId, eventName, action, eventClass, payload) => {
        if (disposed || suspended || !snapshotValue) return false;
        const isTickAction = action === TICK_ACTION;
        const isNetworkAction = action !== INTERVAL_COMMIT_ACTION && !isTickAction;
        if (isTickAction && (pending || tickPending)) return false;
        if (isNetworkAction && pending) return false;
        requestCounter += 1;
        const requestId = action + "#" + requestCounter;
        if (isTickAction) {
            tickPending = true;
            tickPendingRevision = Number(snapshotValue.revision) || 0;
            tickRequestId = requestId;
            if (tickTimer !== null) clearTimeout(tickTimer);
            tickTimer = setTimeout(() => {
                tickTimer = null;
                tickPending = false;
                tickPendingRevision = -1;
                tickRequestId = null;
            }, clientDeadlineOf(action));
        }
        if (isNetworkAction) {
            pending = true;
            pendingAction = action;
            pendingPeriod = action === "stock_period_commit" && PERIOD_VALUES.includes(payload && payload.value)
                ? payload.value
                : null;
            pendingRevision = Number(snapshotValue.revision) || 0;
            pendingRequestId = requestId;
            if (pendingTimer !== null) clearTimeout(pendingTimer);
            pendingTimer = setTimeout(() => {
                pendingTimer = null;
                pending = false;
                pendingAction = null;
                pendingPeriod = null;
                pendingRequestId = null;
                if (refs.status) {
                    refs.status.textContent = action === "stock_period_commit" ? "周期切换超时" : "刷新超时";
                    refs.status.className = "stock-status is-error";
                }
            }, clientDeadlineOf(action));
        }
        render(snapshotValue);
        const accepted = NeuroSurface.emit({
            nodeId: nodeId,
            event: eventName,
            action: action,
            class: eventClass,
            // requestId 让运行时把结果标注回状态里，render 才能只解锁真正属于这次动作的 revision。
            payload: Object.assign({}, payload || {}, { requestId: requestId })
        });
        if (!accepted) {
            if (isTickAction) {
                tickPending = false;
                tickPendingRevision = -1;
                tickRequestId = null;
                tickDisabledUntil = Date.now() + TICK_RETRY_COOLDOWN_MILLIS;
                if (tickTimer !== null) clearTimeout(tickTimer);
                tickTimer = null;
            }
            if (isNetworkAction) {
                pending = false;
                pendingAction = null;
                pendingPeriod = null;
                pendingRequestId = null;
                if (pendingTimer !== null) clearTimeout(pendingTimer);
                pendingTimer = null;
            }
            render(snapshotValue);
        }
        return accepted;
    };

    const requestRefresh = () => emitAction("refresh", "click", "stock_refresh", "discrete", {
        code: normalizeCode(refs.symbol && refs.symbol.value)
    });

    // 准实时通道：只拉最新一笔报价，不重取整段 K 线，所以可以按秒级触发。
    const requestTick = () => {
        if (!tickChannelEnabled()) return requestRefresh();
        const accepted = emitAction("refresh", "tick", TICK_ACTION, "discrete", {
            code: normalizeCode(refs.symbol && refs.symbol.value)
        });
        if (accepted) {
            liveTickCount += 1;
            ticksSinceFullRefresh += 1;
            return true;
        }
        // 只有这次拒绝真的关掉了通道才退回整段刷新；被 pending 挡住时保持静默。
        if (!tickChannelEnabled()) return requestRefresh();
        return false;
    };

    const refreshPlan = (intervalSeconds, marketStatusValue) => {
        const normalized = INTERVALS.includes(Number(intervalSeconds))
            ? Number(intervalSeconds)
            : DEFAULT_INTERVAL_SECONDS;
        const marketStatus = text(marketStatusValue, text(stateOf(snapshotValue).marketStatus, "closed"));
        const tickCapable = tickChannelEnabled();
        // 休市时秒级轮询没有新成交，退到 30 秒，避免空转打上游。
        const requested = marketStatus === "open" ? normalized : Math.max(normalized, CLOSED_MARKET_MIN_SECONDS);
        // 没有准实时通道时不能继续按秒级发整段快照（每次四路 MCP 调用），把节奏抬到整段刷新周期。
        const cadence = tickCapable ? requested : Math.max(requested, FULL_REFRESH_SECONDS);
        const usesTick = tickCapable && cadence < FULL_REFRESH_SECONDS;
        return {
            cadence,
            key: normalized + ":" + cadence + ":" + (usesTick ? "tick" : "full"),
            normalized,
            ticksPerFullRefresh: usesTick ? Math.max(1, Math.round(FULL_REFRESH_SECONDS / cadence)) : 0,
            usesTick
        };
    };

    const effectiveIntervalSeconds = (intervalSeconds) => refreshPlan(intervalSeconds).cadence;

    const setRefreshTimer = (intervalSeconds) => {
        const plan = refreshPlan(intervalSeconds);
        activeInterval = plan.normalized;
        activeTimerKey = plan.key;
        ticksSinceFullRefresh = 0;
        if (refreshTimer !== null) clearInterval(refreshTimer);
        refreshTimer = null;
        if (disposed || suspended) return;
        // 单通道：tick 只更新报价，所以每累计满一个整段刷新周期就把这一拍升级为整段刷新补齐
        // K 线。两个独立定时器同频时慢通道会被 tick 永久抢占，K 线从此不再推进。
        refreshTimer = setInterval(() => {
            if (pending || tickPending) return;
            if (plan.usesTick && ticksSinceFullRefresh < plan.ticksPerFullRefresh) requestTick();
            else requestRefresh();
        }, plan.cadence * 1000);
    };

    // 四个更新函数原来都以 replaceChildren() 开头，每帧丢掉并重建约 200 个节点。行数固定或
    // 有上界，所以改成"补齐节点数量、再原地写文本"，只有真正变化的文本才落到 DOM 上。
    const ensureChildren = (host, count, build) => {
        while (host.children.length > count) host.removeChild(host.lastChild);
        while (host.children.length < count) host.appendChild(build(host.children.length));
        return host.children;
    };
    const clearHost = (host) => {
        if (host && host.firstChild) host.replaceChildren();
    };
    const writeText = (node, value) => {
        if (node.textContent !== value) node.textContent = value;
    };

    const buildMetricCell = (index) => {
        const cell = document.createElement("div");
        cell.className = "market-cell";
        const label = document.createElement("span");
        label.className = "metric-label";
        label.textContent = metricDefinitions[index][0];
        const value = document.createElement("strong");
        value.className = "metric-value";
        cell.append(label, value);
        return cell;
    };

    const updateMetrics = (quote, history, state) => {
        const cells = ensureChildren(refs.metrics, metricDefinitions.length, buildMetricCell);
        metricDefinitions.forEach((definition, index) => {
            writeText(cells[index].lastElementChild, definition[1](quote, history, state));
        });
    };

    const buildHistoryRow = () => {
        const row = document.createElement("div");
        row.className = "history-row";
        for (let index = 0; index < HISTORY_ROW_CELLS; index += 1) row.appendChild(document.createElement("span"));
        return row;
    };

    const writeHistoryRow = (row, cells, className) => {
        const nextClass = "history-row" + (className ? " " + className : "");
        if (row.className !== nextClass) row.className = nextClass;
        for (let index = 0; index < HISTORY_ROW_CELLS; index += 1) {
            const cell = row.children[index];
            writeText(cell, cells[index].text);
            // style.color 读回来是 rgb() 形式，和写进去的 #rrggbb 永远不相等，所以这里不比较。
            cell.style.color = cells[index].color || "";
        }
    };

    const updateHistoryTable = (history, state, palette) => {
        if (!refs.historyRows) return;
        const rows = history.slice(-8).reverse();
        const hostRows = ensureChildren(refs.historyRows, HISTORY_ROW_COUNT, buildHistoryRow);
        writeHistoryRow(hostRows[0], HISTORY_HEAD_CELLS, "is-head");
        const intraday = isIntradayPeriod(periodOf(state));
        for (let index = 1; index < HISTORY_ROW_COUNT; index += 1) {
            const value = rows[index - 1];
            if (value === undefined) {
                writeHistoryRow(hostRows[index], HISTORY_EMPTY_CELLS, "is-empty");
                continue;
            }
            const row = asObject(value);
            const open = asNumber(row.open);
            const close = asNumber(row.close);
            const color = open === null || close === null ? COLORS.text : deltaColor(close - open, palette);
            writeHistoryRow(hostRows[index], [
                { text: formatPointDate(row.date, intraday) },
                { text: formatNumber(open, 2) },
                { text: formatNumber(row.high, 2) },
                { text: formatNumber(row.low, 2) },
                { text: formatNumber(close, 2), color: color },
                { text: formatVolume(row.volume) }
            ], "");
        }
        writeText(refs.historyTitle, periodLabelOf(state) + " · 最近 8 条");
        writeText(refs.historyMeta, history.length + " 条数据");
    };

    const buildFavoriteCard = () => {
        const card = document.createElement("article");
        card.className = "favorite-card";
        const name = document.createElement("strong");
        name.className = "favorite-name";
        const code = document.createElement("span");
        code.className = "favorite-code";
        const price = document.createElement("strong");
        price.className = "favorite-price";
        const delta = document.createElement("span");
        delta.className = "favorite-delta";
        card.append(name, code, price, delta);
        return card;
    };

    const updateFavorites = (state) => {
        if (!refs.favoritesGrid) return;
        const favorites = favoriteQuotesOf(state).slice(0, 8);
        if (!favorites.length) {
            const existing = refs.favoritesGrid.firstElementChild;
            if (refs.favoritesGrid.children.length === 1 && existing.classList.contains("favorites-empty")) return;
            const empty = document.createElement("div");
            empty.className = "favorites-empty";
            empty.textContent = "等待 stock-api 返回收藏股票报价";
            refs.favoritesGrid.replaceChildren(empty);
            return;
        }
        const first = refs.favoritesGrid.firstElementChild;
        // 空态节点和卡片形状不同，切回有数据时先清掉它，否则 ensureChildren 会当成卡片写。
        if (first && !first.classList.contains("favorite-card")) refs.favoritesGrid.replaceChildren();
        const cards = ensureChildren(refs.favoritesGrid, favorites.length, buildFavoriteCard);
        favorites.forEach((value, index) => {
            const quote = asObject(value);
            const market = text(quote.market, text(quote.code, "").slice(0, 2));
            const palette = paletteFor(market);
            const color = movementColor(movement(quote), palette);
            const card = cards[index];
            const name = card.children[0];
            writeText(name, text(quote.name, "未知股票"));
            name.title = name.textContent;
            writeText(card.children[1], text(quote.code, "--") + " · " + market);
            const price = card.children[2];
            writeText(price, formatNumber(quote.price, 2));
            price.style.color = color;
            const delta = card.children[3];
            writeText(delta, formatSigned(quote.change, "") + "  " + formatSigned(quote.changePercent, "%"));
            delta.style.color = color;
        });
    };

    const bookLevelsOf = (value) => Array.isArray(value) ? value.filter((row) => asNumber(asObject(row).price) !== null) : [];
    const orderBookOf = (state) => {
        const book = asObject(state.orderBook);
        const bids = bookLevelsOf(book.bids);
        const asks = bookLevelsOf(book.asks);
        return bids.length || asks.length ? { book: book, bids: bids, asks: asks } : null;
    };
    const buildBookRow = () => {
        const line = document.createElement("div");
        line.className = "book-row";
        const label = document.createElement("span");
        label.className = "book-tag";
        const price = document.createElement("span");
        price.className = "book-price";
        const volume = document.createElement("span");
        volume.className = "book-volume";
        line.append(label, price, volume);
        return line;
    };
    const renderBookSide = (host, levels, tag, previousClose, palette) => {
        const rows = ensureChildren(host, levels.length, buildBookRow);
        levels.forEach((row, index) => {
            const level = asObject(row);
            const line = rows[index];
            const label = tag + (asNumber(level.level) || index + 1);
            const priceText = formatNumber(level.price, 2);
            const volumeText = formatVolume(level.volume);
            const orders = asNumber(level.orders);
            writeText(line.children[0], label);
            writeText(line.children[1], priceText);
            line.children[1].style.color = previousClose === null
                ? COLORS.text
                : deltaColor(asNumber(level.price) - previousClose, palette);
            writeText(line.children[2], volumeText);
            line.title = label + " " + priceText
                + " · 委托量 " + volumeText
                + (orders === null ? "" : " · 笔数 " + formatVolume(orders));
        });
    };
    const tapeDefinitions = [
        ["均价", (tape) => formatNumber(tape.avgPrice, 2)],
        ["成交量", (tape) => formatVolume(tape.volume)],
        ["成交额", (tape) => formatVolume(tape.amount)],
        ["换手", (tape) => asNumber(tape.turnoverRate) === null ? "--" : formatNumber(tape.turnoverRate, 2) + "%"],
        ["振幅", (tape) => asNumber(tape.amplitude) === null ? "--" : formatNumber(tape.amplitude, 2) + "%"],
        ["总市值", (tape) => formatVolume(tape.marketCapital)]
    ];
    // 标签是常量，值在 strong 里。原来把标签写进 item.textContent 再 append strong，所以
    // 每帧都要重建整个 span；这里把标签放进独立文本节点，只有值需要改写。
    const buildTapeItem = (index) => {
        const item = document.createElement("span");
        item.className = "tape-item";
        item.append(document.createTextNode(tapeDefinitions[index][0]), document.createElement("strong"));
        return item;
    };
    // 运行时已经判定过新鲜度（stale/ageSeconds/maxAgeSeconds），但界面此前只画一个时钟串，
    // 陈旧记录和最新记录长得一模一样。把判定显式标出来，年龄已知时连秒数一起给。
    // asNumber 不能用在这里：Number(null) 是 0，观测时间缺失的记录会被写成"已陈旧 0 秒"，
    // 也就是刚刚才观测到——正好和运行时 fail-closed 的判定相反。
    const asAgeSeconds = (value) => {
        if (value === null || value === undefined || value === "") return null;
        return asNumber(value);
    };
    const staleLabel = (record) => {
        if (!record || record.stale !== true) return "";
        const age = asAgeSeconds(record.ageSeconds);
        const limit = asAgeSeconds(record.maxAgeSeconds);
        if (age === null) return "已陈旧（观测时间不可用）";
        if (limit === null) return "已陈旧 " + formatNumber(age, 0) + " 秒";
        return "已陈旧 " + formatNumber(age, 0) + "/" + formatNumber(limit, 0) + " 秒";
    };
    const withStale = (base, record) => {
        const label = staleLabel(record);
        return label ? base + " · " + label : base;
    };
    // 报价成功但 K 线抓取失败时，运行时会送来 historyWarning：状态仍然是 ready，可图表画不
    // 出来。借用页脚同一个位置显示，用黄色和 error 的红色区分非致命告警；错误优先，因为报价
    // 本身都没拿到时，抱怨图表没有意义。
    const footerNoticeOf = (state) => {
        const error = typeof state.error === "string" ? state.error.trim() : "";
        if (error !== "") return { text: state.error, warning: false };
        const historyWarning = typeof state.historyWarning === "string" ? state.historyWarning.trim() : "";
        if (historyWarning === "") return { text: "", warning: false };
        return { text: "K 线数据不可用：" + historyWarning, warning: true };
    };
    const updateOrderBook = (state, quote, palette) => {
        if (!refs.book) return;
        const snapshot = orderBookOf(state);
        const tape = asObject(state.liveTape);
        const hasTape = asNumber(tape.price) !== null;
        refs.book.classList.toggle("is-visible", Boolean(snapshot) || hasTape);
        if (!snapshot && !hasTape) {
            clearHost(refs.bids);
            clearHost(refs.asks);
            clearHost(refs.tape);
            return;
        }
        const previousClose = asNumber(quote.previousClose);
        refs.bookBar.classList.toggle("is-hidden", !snapshot);
        refs.asks.classList.toggle("is-hidden", !snapshot);
        refs.bids.classList.toggle("is-hidden", !snapshot);
        if (snapshot) {
            const book = snapshot.book;
            const levels = asNumber(book.levels) || Math.max(snapshot.bids.length, snapshot.asks.length);
            writeText(refs.bookTitle, levels + " 档盘口");
            const buyPercent = asNumber(book.buyPercent);
            const sellPercent = asNumber(book.sellPercent);
            const netVolume = asNumber(book.netVolume);
            const parts = [];
            if (buyPercent !== null && sellPercent !== null) {
                parts.push("买 " + formatNumber(buyPercent, 2) + "% / 卖 " + formatNumber(sellPercent, 2) + "%");
            }
            if (netVolume !== null) parts.push("委差 " + formatSigned(netVolume, ""));
            if (asNumber(book.ratio) !== null) parts.push("量比 " + formatNumber(book.ratio, 2));
            parts.push(withStale(text(book.source, "xueqiu") + " · " + formatClock(book.observedAt), book));
            if (hasTape) {
                const tapeStale = staleLabel(tape);
                if (tapeStale) parts.push("实时逐笔" + tapeStale);
            }
            writeText(refs.bookMeta, parts.join(" · "));
            refs.bookMeta.title = refs.bookMeta.textContent;
            refs.bookMeta.classList.toggle("is-stale", book.stale === true || (hasTape && tape.stale === true));
            const buyShare = buyPercent === null || sellPercent === null
                ? 50
                : Math.max(0, Math.min(100, buyPercent));
            refs.bookBuyBar.style.width = buyShare + "%";
            refs.bookBuyBar.style.background = palette.up;
            refs.bookSellBar.style.width = (100 - buyShare) + "%";
            refs.bookSellBar.style.background = palette.down;
            renderBookSide(refs.bids, snapshot.bids, "买", previousClose, palette);
            renderBookSide(refs.asks, snapshot.asks, "卖", previousClose, palette);
        }
        else {
            writeText(refs.bookTitle, "盘中实时");
            writeText(refs.bookMeta, hasTape
                ? withStale(text(tape.source, "xueqiu") + " · " + formatClock(tape.observedAt) + " · 该市场不提供十档盘口", tape)
                : "");
            refs.bookMeta.title = refs.bookMeta.textContent;
            refs.bookMeta.classList.toggle("is-stale", hasTape && tape.stale === true);
            clearHost(refs.bids);
            clearHost(refs.asks);
        }
        if (!hasTape) {
            clearHost(refs.tape);
            return;
        }
        const items = ensureChildren(refs.tape, tapeDefinitions.length, buildTapeItem);
        tapeDefinitions.forEach((definition, index) => {
            writeText(items[index].lastElementChild, definition[1](tape));
        });
        // 逐笔派生量（均价/成交额/换手）全部来自这一条记录，记录陈旧时这些数字也陈旧。
        const tapeStaleLabel = staleLabel(tape);
        refs.tape.classList.toggle("is-stale", tapeStaleLabel !== "");
        refs.tape.title = tapeStaleLabel;
    };

    const drawChart = () => {
        if (!refs.chart || !snapshotValue) return;
        if (viewOf(snapshotValue) === "trade-price" || viewOf(snapshotValue) === "favorites-summary") {
            chartGeometry = null;
            hideChartTip();
            return;
        }
        const canvas = refs.chart;
        const bounds = canvas.getBoundingClientRect();
        const width = Math.min(MAX_CANVAS_WIDTH, Math.max(260, Math.floor(bounds.width || 520)));
        const height = Math.min(MAX_CANVAS_HEIGHT, Math.max(145, Math.floor(bounds.height || 180)));
        const deviceRatio = Math.min(2, Math.max(1, globalThis.devicePixelRatio || 1));
        const pixelRatio = Math.sqrt(MAX_CANVAS_PIXELS / Math.max(1, width * height));
        const ratio = Math.max(1, Math.min(deviceRatio, pixelRatio));
        // 给 canvas.width/height 赋值会重新分配后备位图，即使赋的是同一个数字。两块画布在
        // 上限尺寸下各约 16 MB，而重绘每秒至少一次，还会被 resize 和悬浮额外触发。只在尺寸真的
        // 变化时赋值；尺寸不变时下面的 clearRect + fillRect 依旧把整块画布清干净。
        const nextWidth = Math.floor(width * ratio);
        const nextHeight = Math.floor(height * ratio);
        if (canvas.width !== nextWidth) canvas.width = nextWidth;
        if (canvas.height !== nextHeight) canvas.height = nextHeight;
        if (refs.overlay) {
            if (refs.overlay.width !== nextWidth) refs.overlay.width = nextWidth;
            if (refs.overlay.height !== nextHeight) refs.overlay.height = nextHeight;
        }
        const context = canvas.getContext("2d");
        if (!context) return;
        context.setTransform(ratio, 0, 0, ratio, 0, 0);
        context.clearRect(0, 0, width, height);
        context.fillStyle = COLORS.surface;
        context.fillRect(0, 0, width, height);

        const left = 10;
        const right = width - 10;
        const top = 30;
        const volumeTop = Math.floor(height * 0.76);
        const bottom = height - 10;
        context.strokeStyle = COLORS.grid;
        context.lineWidth = 1;
        for (let index = 0; index <= 4; index += 1) {
            const y = top + ((volumeTop - top) * index / 4);
            context.beginPath();
            context.moveTo(left, y + 0.5);
            context.lineTo(right, y + 0.5);
            context.stroke();
        }
        for (let index = 0; index <= 4; index += 1) {
            const x = left + ((right - left) * index / 4);
            context.beginPath();
            context.moveTo(x + 0.5, top);
            context.lineTo(x + 0.5, bottom);
            context.stroke();
        }

        const state = stateOf(snapshotValue);
        const period = periodOf(state);
        const periodLabel = periodLabelOf(state);
        const intraday = isIntradayPeriod(period);
        const palette = paletteFor(marketOf(state, text(state.code, text(state.symbol, "SZ000034"))));
        const maxPoints = Math.min(240, Math.max(48, Math.floor((right - left) / (intraday ? 2 : 3))));
        const sample = chartSampleOf(state, revisionOf(snapshotValue), maxPoints, intraday);
        const points = sample.points;
        if (points.length < 2) {
            chartGeometry = null;
            hideChartTip();
            context.fillStyle = COLORS.muted;
            context.font = "11px Segoe UI, Microsoft YaHei, sans-serif";
            context.textAlign = "center";
            context.fillText("等待 " + periodLabel + " 行情", width / 2, top + (volumeTop - top) / 2);
            return;
        }

        let minimum = Math.min(...points.map((point) => point.low));
        let maximum = Math.max(...points.map((point) => point.high));
        if (minimum === maximum) {
            minimum -= 0.01;
            maximum += 0.01;
        }
        const padding = Math.max((maximum - minimum) * 0.08, 0.01);
        minimum -= padding;
        maximum += padding;
        const slotWidth = (right - left) / points.length;
        const xAt = (index) => left + slotWidth * (index + 0.5);
        const yAt = (value) => top + (maximum - value) * (volumeTop - top - 4) / (maximum - minimum);
        const maxVolume = Math.max(...points.map((point) => point.volume || 0), 1);
        const barWidth = Math.max(1, Math.min(12, slotWidth * 0.58));

        points.forEach((point, index) => {
            const color = point.close >= point.open ? palette.up : palette.down;
            const x = xAt(index);
            const barHeight = ((point.volume || 0) / maxVolume) * Math.max(8, bottom - volumeTop - 3);
            context.globalAlpha = 0.34;
            context.fillStyle = color;
            context.fillRect(x - barWidth / 2, bottom - barHeight, barWidth, barHeight);
            context.globalAlpha = 1;
            if (!intraday) {
                context.strokeStyle = color;
                context.lineWidth = 1;
                context.beginPath();
                context.moveTo(x + 0.5, yAt(point.high));
                context.lineTo(x + 0.5, yAt(point.low));
                context.stroke();
                const bodyTop = Math.min(yAt(point.open), yAt(point.close));
                const bodyHeight = Math.max(1, Math.abs(yAt(point.open) - yAt(point.close)));
                context.fillStyle = color;
                context.fillRect(x - barWidth / 2, bodyTop, barWidth, bodyHeight);
            }
        });

        context.beginPath();
        points.forEach((point, index) => {
            const x = xAt(index);
            const y = yAt(point.close);
            if (index === 0) context.moveTo(x, y);
            else context.lineTo(x, y);
        });
        context.strokeStyle = COLORS.blue;
        context.globalAlpha = intraday ? 1 : 0.82;
        context.lineWidth = intraday ? 1.8 : 1.15;
        context.lineJoin = "round";
        context.lineCap = "round";
        context.stroke();
        context.globalAlpha = 1;

        const averageValues = sample.averageValues;
        context.beginPath();
        let started = false;
        averageValues.forEach((value, index) => {
            if (value === null) return;
            const x = xAt(index);
            const y = yAt(value);
            if (started) context.lineTo(x, y);
            else {
                context.moveTo(x, y);
                started = true;
            }
        });
        context.strokeStyle = COLORS.yellow;
        context.lineWidth = 1.25;
        context.lineJoin = "round";
        context.lineCap = "round";
        if (started) context.stroke();

        const compactDate = (value) => {
            const date = String(value || "").replace("T", " ");
            if (intraday && date.length >= 16) return date.slice(5, 16);
            return date.slice(0, 10);
        };
        context.font = "10px Consolas, monospace";
        context.fillStyle = COLORS.muted;
        context.textAlign = "left";
        context.fillText(maximum.toFixed(2), left + 2, top + 10);
        context.fillText(minimum.toFixed(2), left + 2, volumeTop - 5);
        context.fillText(compactDate(points[0].date), left + 2, bottom - 2);
        context.textAlign = "right";
        context.fillText(compactDate(points[points.length - 1].date), right - 2, bottom - 2);

        chartGeometry = {
            points: points,
            averageValues: averageValues,
            palette: palette,
            intraday: intraday,
            periodLabel: periodLabel,
            ratio: ratio,
            width: width,
            height: height,
            left: left,
            right: right,
            top: top,
            volumeTop: volumeTop,
            bottom: bottom,
            slotWidth: slotWidth,
            xAt: xAt,
            yAt: yAt
        };
        if (hoverIndex >= 0) applyHover();
        else clearOverlay();
    };

    const overlayContext = () => {
        if (!refs.overlay) return null;
        const context = refs.overlay.getContext("2d");
        if (!context) return null;
        const ratio = chartGeometry ? chartGeometry.ratio : 1;
        context.setTransform(ratio, 0, 0, ratio, 0, 0);
        context.clearRect(0, 0, refs.overlay.width / ratio, refs.overlay.height / ratio);
        return context;
    };

    const clearOverlay = () => {
        overlayContext();
    };

    const hideChartTip = () => {
        hoverIndex = -1;
        hoverPointer = null;
        if (refs.tip) {
            refs.tip.classList.remove("is-visible");
            refs.tip.setAttribute("aria-hidden", "true");
        }
        clearOverlay();
    };

    const drawCrosshair = (index) => {
        const geometry = chartGeometry;
        const context = overlayContext();
        if (!context || !geometry) return;
        const point = geometry.points[index];
        if (!point) return;
        const x = geometry.xAt(index);
        const y = geometry.yAt(point.close);
        context.save();
        context.strokeStyle = "rgba(217,255,56,0.5)";
        context.lineWidth = 1;
        context.setLineDash([3, 3]);
        context.beginPath();
        context.moveTo(Math.round(x) + 0.5, geometry.top);
        context.lineTo(Math.round(x) + 0.5, geometry.bottom);
        context.stroke();
        context.beginPath();
        context.moveTo(geometry.left, Math.round(y) + 0.5);
        context.lineTo(geometry.right, Math.round(y) + 0.5);
        context.stroke();
        context.setLineDash([]);
        context.beginPath();
        context.arc(x, y, 3.2, 0, Math.PI * 2);
        context.fillStyle = COLORS.yellow;
        context.fill();
        context.lineWidth = 1.3;
        context.strokeStyle = COLORS.background;
        context.stroke();
        context.restore();
    };

    // 提示框用 DOM 构建，不拼 innerHTML：日期这类字段直接来自另一个进程的 JSON，拼进
    // 标记里就会把状态内容当 HTML 执行。颜色只来自本地 COLORS/paletteFor 的封闭集合。
    const tipRow = (key, value, color) => {
        const row = document.createElement("div");
        row.className = "chart-tip-row";
        const keyNode = document.createElement("span");
        keyNode.className = "chart-tip-key";
        keyNode.textContent = key;
        const valueNode = document.createElement("span");
        valueNode.className = "chart-tip-value";
        valueNode.textContent = value;
        if (color) valueNode.style.color = color;
        row.appendChild(keyNode);
        row.appendChild(valueNode);
        return row;
    };

    const buildTipContent = (geometry, index) => {
        const point = geometry.points[index];
        const previous = index > 0 ? geometry.points[index - 1] : null;
        const reference = previous ? previous.close : point.open;
        const change = reference ? point.close - reference : null;
        const changePercent = reference ? ((point.close - reference) / reference) * 100 : null;
        const changeColor = deltaColor(change, geometry.palette);
        const averageValue = Array.isArray(geometry.averageValues) ? geometry.averageValues[index] : null;
        const fragment = document.createDocumentFragment();
        const title = document.createElement("div");
        title.className = "chart-tip-title";
        title.textContent = formatPointDate(point.date, geometry.intraday);
        fragment.appendChild(title);
        if (geometry.intraday) {
            fragment.appendChild(tipRow("价格", formatNumber(point.close, 2), changeColor));
            if (averageValue !== null && averageValue !== undefined) fragment.appendChild(tipRow("均价", formatNumber(averageValue, 2), COLORS.yellow));
        }
        else {
            fragment.appendChild(tipRow("开", formatNumber(point.open, 2)));
            fragment.appendChild(tipRow("高", formatNumber(point.high, 2), deltaColor(point.high - reference, geometry.palette)));
            fragment.appendChild(tipRow("低", formatNumber(point.low, 2), deltaColor(point.low - reference, geometry.palette)));
            fragment.appendChild(tipRow("收", formatNumber(point.close, 2), changeColor));
            if (averageValue !== null && averageValue !== undefined) fragment.appendChild(tipRow("MA5", formatNumber(averageValue, 2), COLORS.yellow));
        }
        fragment.appendChild(tipRow("涨跌", change === null ? "--" : formatSigned(change, ""), changeColor));
        fragment.appendChild(tipRow("涨幅", changePercent === null ? "--" : formatSigned(changePercent, "%"), changeColor));
        fragment.appendChild(tipRow("成交量", formatVolume(point.volume)));
        return fragment;
    };

    const positionTip = (pointerX, pointerY) => {
        const tip = refs.tip;
        const canvas = refs.chart;
        if (!tip || !canvas) return;
        const boundsWidth = canvas.clientWidth || (chartGeometry ? chartGeometry.width : 0);
        const boundsHeight = canvas.clientHeight || (chartGeometry ? chartGeometry.height : 0);
        const tipWidth = tip.offsetWidth;
        const tipHeight = tip.offsetHeight;
        let leftPosition = pointerX + 14;
        if (leftPosition + tipWidth > boundsWidth - 4) leftPosition = pointerX - tipWidth - 14;
        if (leftPosition < 4) leftPosition = 4;
        let topPosition = pointerY - tipHeight - 10;
        if (topPosition < 4) topPosition = pointerY + 16;
        if (topPosition + tipHeight > boundsHeight - 4) topPosition = Math.max(4, boundsHeight - tipHeight - 4);
        tip.style.left = Math.round(leftPosition) + "px";
        tip.style.top = Math.round(topPosition) + "px";
    };

    const indexAtPointer = (offsetX) => {
        const geometry = chartGeometry;
        if (!geometry || !geometry.points.length) return -1;
        const clamped = Math.max(geometry.left, Math.min(geometry.right, offsetX));
        const index = Math.floor((clamped - geometry.left) / geometry.slotWidth);
        return Math.max(0, Math.min(geometry.points.length - 1, index));
    };

    const applyHover = () => {
        const geometry = chartGeometry;
        if (!geometry || !refs.tip) {
            hideChartTip();
            return;
        }
        const pointer = hoverPointer;
        const index = pointer ? indexAtPointer(pointer.x) : Math.min(hoverIndex, geometry.points.length - 1);
        if (index < 0) {
            hideChartTip();
            return;
        }
        hoverIndex = index;
        drawCrosshair(index);
        refs.tip.replaceChildren(buildTipContent(geometry, index));
        refs.tip.classList.add("is-visible");
        refs.tip.setAttribute("aria-hidden", "false");
        const anchorX = pointer ? pointer.x : geometry.xAt(index);
        const anchorY = pointer ? pointer.y : geometry.yAt(geometry.points[index].close);
        positionTip(anchorX, anchorY);
    };

    const handleChartPointer = (event) => {
        if (!chartGeometry || !refs.chart) return;
        const bounds = refs.chart.getBoundingClientRect();
        if (!bounds.width || !bounds.height) return;
        hoverPointer = { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
        if (hoverFrame !== null) return;
        hoverFrame = requestAnimationFrame(() => {
            hoverFrame = null;
            if (disposed || !hoverPointer) return;
            applyHover();
        });
    };

    // 图例同样用 DOM 构建，避免把 periodLabel（来自状态）拼进标记。
    const legendItem = (label, swatchClass, swatchBackground) => {
        const item = document.createElement("span");
        item.className = "legend-item";
        const swatch = document.createElement("i");
        swatch.className = "legend-line " + swatchClass;
        if (swatchBackground) swatch.style.background = swatchBackground;
        item.appendChild(swatch);
        item.appendChild(document.createTextNode(label));
        return item;
    };

    const render = (snapshot) => {
        if (!rootElement || !snapshot) return;
        snapshotValue = snapshot;
        const state = stateOf(snapshot);
        const revision = Number(snapshot.revision);
        // 只有确认这次 revision 属于自己发出的那次动作才解锁。statePatch 是合并语义，
        // 任何后台刷新都会推进 revision，只看 revision 会在请求仍在运行时放开控件。
        const echoedRequestId = text(state.lastRequestId, "");
        const echoedActionId = text(state.lastActionId, "");
        const settledBy = (requestId, actionId) => {
            if (echoedRequestId) return echoedRequestId === requestId;
            // 运行时尚未回声（旧包）时退回按 revision 解锁，否则会永久卡住。
            if (echoedActionId) return echoedActionId === actionId;
            return true;
        };
        if (pending && revision > pendingRevision && settledBy(pendingRequestId, pendingAction)) {
            pending = false;
            pendingAction = null;
            pendingPeriod = null;
            pendingRequestId = null;
            // 整段刷新刚落地，重新开始累计 tick。
            ticksSinceFullRefresh = 0;
            if (pendingTimer !== null) clearTimeout(pendingTimer);
            pendingTimer = null;
        }
        if (tickPending && revision > tickPendingRevision && settledBy(tickRequestId, TICK_ACTION)) {
            tickPending = false;
            tickPendingRevision = -1;
            tickRequestId = null;
            if (tickTimer !== null) clearTimeout(tickTimer);
            tickTimer = null;
        }
        const quote = quoteOf(state);
        const history = historyOf(state);
        const kind = movement(quote);
        const code = normalizeCode(text(state.code, text(state.symbol, "SZ000034")));
        const market = text(state.market, code.slice(0, 2) || "SZ");
        const palette = paletteFor(market);
        const color = movementColor(kind, palette);
        const interval = INTERVALS.includes(Number(state.intervalSeconds)) ? Number(state.intervalSeconds) : DEFAULT_INTERVAL_SECONDS;
        const period = periodOf(state);
        const displayedPeriod = pendingPeriod || period;
        const periodLabel = periodLabelOf(state);
        const marketStatus = text(state.marketStatus, "closed");
        const lastTradingDate = text(state.lastTradingDate, "--");
        const hasQuote = asNumber(quote.price) !== null;
        const hasError = typeof state.error === "string" && state.error.trim().length > 0;
        const activeView = viewOf(snapshot);
        refs.shell.dataset.view = activeView;

        refs.title.textContent = activeView === "favorites-summary"
            ? "收藏股票价格汇总"
            : hasQuote ? text(quote.name, "股票盯盘") : "股票盯盘";
        if (document.activeElement !== refs.symbol) refs.symbol.value = code;
        refs.intervals.querySelectorAll("[data-interval-value]").forEach((button) => {
            const active = Number(button.dataset.intervalValue) === interval;
            button.classList.toggle("is-active", active);
            button.setAttribute("aria-pressed", active ? "true" : "false");
        });
        refs.periods.querySelectorAll("[data-period-value]").forEach((button) => {
            const active = button.dataset.periodValue === displayedPeriod;
            button.classList.toggle("is-active", active);
            button.setAttribute("aria-selected", active ? "true" : "false");
            button.disabled = pending;
        });
        refs.refresh.disabled = pending;
        refs.favoritesRefresh.disabled = pending;
        const pendingText = pendingAction === "stock_period_commit"
            ? "正在切换周期"
            : pendingAction === "stock_symbol_commit" ? "正在加载股票" : "正在刷新";
        refs.status.textContent = pending ? pendingText : hasError ? "行情异常" : text(state.statusText, "等待首次刷新");
        refs.status.title = hasError ? state.error : refs.status.textContent;
        refs.status.className = "stock-status" + (pending ? " is-loading" : hasError ? " is-error" : state.status === "ready" ? " is-ready" : "");
        refs.name.textContent = hasQuote ? text(quote.name, "未知股票") : "等待行情";
        refs.name.title = refs.name.textContent;
        refs.code.textContent = code + " · " + market;
        refs.price.textContent = hasQuote ? formatNumber(quote.price, 2) : "--";
        refs.price.style.color = hasQuote ? color : COLORS.text;
        refs.delta.textContent = hasQuote
            ? formatSigned(quote.change, "") + "  " + formatSigned(quote.changePercent, "%")
            : "等待真实价格";
        refs.delta.style.color = hasQuote ? color : COLORS.muted;
        const timerPlan = refreshPlan(interval, marketStatus);
        const cadence = timerPlan.cadence;
        const cadenceText = "自动 " + (INTERVAL_LABELS[cadence] || cadence + " 秒");
        const liveText = timerPlan.usesTick ? " · 准实时" : "";
        const fetchedAt = text(quote.fetchedAt, text(quote.observedAt, ""));
        const freshnessText = quote.stale ? " · 数据可能陈旧" : "";
        refs.session.textContent = hasQuote
            ? marketStatus === "open"
                ? "交易中 · " + periodLabel + " · 更新 " + formatClock(fetchedAt) + " · " + cadenceText + liveText + freshnessText
                : "休市 · 最近交易日 " + lastTradingDate + " · 最后价 " + formatNumber(quote.price, 2) + " · 更新 " + formatClock(fetchedAt) + " · " + cadenceText + freshnessText
            : "数据源：stock-api MCP";
        refs.session.title = hasQuote
            ? refs.session.textContent + " · 观测时间 " + formatTimestamp(quote.observedAt) + " · 抓取时间 " + formatTimestamp(fetchedAt) + " · 本次会话准实时刷新 " + liveTickCount + " 次"
            : refs.session.textContent;
        // 图例只随周期和市场（涨跌配色）变化，不随行情变化，所以不必每帧重建。
        const legendKey = period + "|" + palette.up + "|" + palette.down;
        if (legendKey !== legendPaintedKey) {
            legendPaintedKey = legendKey;
            const candleGradient = "linear-gradient(90deg," + palette.up + " 0 48%," + palette.down + " 52% 100%)";
            refs.legend.replaceChildren(...(isIntradayPeriod(period)
                ? [legendItem(periodLabel + "价格", "close"), legendItem("均价", "average"), legendItem("成交量", "volume")]
                : [legendItem(periodLabel, "candle", candleGradient), legendItem("收盘", "close"), legendItem("MA5", "average"), legendItem("成交量", "volume")]));
            refs.chart.setAttribute("aria-label", periodLabel + (isIntradayPeriod(period) ? "价格走势与成交量图" : "K 线、MA5 与成交量图"));
        }
        const footerNotice = footerNoticeOf(state);
        refs.error.textContent = footerNotice.text;
        refs.error.title = footerNotice.text;
        refs.error.classList.toggle("is-warning", footerNotice.warning);
        refs.disclaimer.textContent = text(state.disclaimer, "行情可能延迟，不构成投资建议或交易指令");
        // 这四个更新函数只读状态里的行情、盘口、历史和收藏，而这几块只随 revision 变化。视图和
        // 历史长度进 key 是因为切视图会隐藏/显示区块，历史长度决定补空行的数量。上面那些标题、
        // 价格、会话行不进这个门，它们还要反映 pending 之类的纯本地状态。
        const paintRevision = revisionOf(snapshot);
        // 没有可用 revision 时（旧宿主、手工快照）宁可每帧重画，也不要把界面永久钉在第一帧。
        const paintKey = paintRevision < 0 ? "" : paintRevision + "|" + activeView + "|" + history.length;
        if (paintKey === "" || paintKey !== paintedKey) {
            paintedKey = paintKey;
            updateMetrics(quote, history, state);
            updateOrderBook(state, quote, palette);
            updateHistoryTable(history, state, palette);
            updateFavorites(state);
        }
        if (timerPlan.key !== activeTimerKey) setRefreshTimer(interval);
        drawChart();
    };

    const bindEvents = () => {
        refs.refresh.addEventListener("click", requestRefresh);
        refs.favoritesRefresh.addEventListener("click", requestRefresh);
        refs.symbol.addEventListener("keydown", (event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            emitAction("symbol", "change", "stock_symbol_commit", "commit", { value: normalizeCode(refs.symbol.value) });
            refs.symbol.blur();
        });
        refs.symbol.addEventListener("change", () => {
            emitAction("symbol", "change", "stock_symbol_commit", "commit", { value: normalizeCode(refs.symbol.value) });
        });
        refs.intervals.addEventListener("click", (event) => {
            const button = event.target instanceof Element ? event.target.closest("[data-interval-value]") : null;
            if (!(button instanceof HTMLButtonElement)) return;
            const value = Number(button.dataset.intervalValue);
            if (!INTERVALS.includes(value)) return;
            emitAction("interval", "change", INTERVAL_COMMIT_ACTION, "commit", { value: value });
        });
        refs.periods.addEventListener("click", (event) => {
            const button = event.target instanceof Element ? event.target.closest("[data-period-value]") : null;
            if (!(button instanceof HTMLButtonElement)) return;
            const value = button.dataset.periodValue;
            if (!PERIOD_VALUES.includes(value)) return;
            emitAction("period", "change", "stock_period_commit", "commit", { value: value });
        });
        if (!refs.chartWrap) return;
        refs.chartWrap.addEventListener("pointermove", handleChartPointer);
        refs.chartWrap.addEventListener("pointerdown", handleChartPointer);
        refs.chartWrap.addEventListener("pointerleave", hideChartTip);
        refs.chartWrap.addEventListener("pointercancel", hideChartTip);
    };

    // ResizeObserver 在拖动窗口时每帧都回调，而 drawChart 是整图重绘。和悬浮走同一套 rAF
    // 合并：一帧内多次尺寸变化只重绘一次。
    const scheduleChartRedraw = () => {
        if (resizeFrame !== null) return;
        resizeFrame = requestAnimationFrame(() => {
            resizeFrame = null;
            if (!disposed) drawChart();
        });
    };

    const clearScheduledWork = () => {
        if (refreshTimer !== null) clearInterval(refreshTimer);
        if (pendingTimer !== null) clearTimeout(pendingTimer);
        if (tickTimer !== null) clearTimeout(tickTimer);
        refreshTimer = null;
        pendingTimer = null;
        tickTimer = null;
        tickPending = false;
        tickPendingRevision = -1;
        tickRequestId = null;
        ticksSinceFullRefresh = 0;
        activeTimerKey = "";
    };

    const cleanup = () => {
        clearScheduledWork();
        if (hoverFrame !== null) cancelAnimationFrame(hoverFrame);
        hoverFrame = null;
        if (resizeFrame !== null) cancelAnimationFrame(resizeFrame);
        resizeFrame = null;
        hideChartTip();
        chartGeometry = null;
        seriesCache = null;
        sampleCache = null;
        // 下一次 mount 从空 DOM 开始，缓存的 paintKey 会让四个更新函数跳过第一帧。
        paintedKey = "";
        legendPaintedKey = "";
        resizeObserver && resizeObserver.disconnect();
        resizeObserver = null;
        if (adoptedStyleSheet) {
            document.adoptedStyleSheets = document.adoptedStyleSheets.filter((sheet) => sheet !== adoptedStyleSheet);
            adoptedStyleSheet = null;
        }
    };

    const testHooks = globalThis.__LOOM_STOCK_MONITOR_TEST_HOOKS__;
    if (testHooks && typeof testHooks === "object") {
        Object.assign(testHooks, {
            applyRevision(revision) {
                if (tickPending && Number(revision) > tickPendingRevision) {
                    tickPending = false;
                    tickPendingRevision = -1;
                    if (tickTimer !== null) clearTimeout(tickTimer);
                    tickTimer = null;
                }
            },
            beginTick(revision) {
                snapshotValue = { revision: Number(revision) || 0, authoritativeState: { marketStatus: "open" } };
                tickPending = true;
                tickPendingRevision = Number(revision) || 0;
            },
            disableTickChannel(durationMillis) {
                tickDisabledUntil = Date.now() + (Number(durationMillis) || TICK_RETRY_COOLDOWN_MILLIS);
            },
            enableTickChannel() {
                tickDisabledUntil = 0;
            },
            paletteFor,
            refreshPlan,
            viewOf,
            movingAverages,
            chartSampleOf,
            staleLabel,
            footerNoticeOf,
            tickState: () => ({ pending: tickPending, revision: tickPendingRevision })
        });
    }

    NeuroSurface.define({
        mount({ root, snapshot }) {
            rootElement = root;
            snapshotValue = snapshot;
            adoptedStyleSheet = new CSSStyleSheet();
            adoptedStyleSheet.replaceSync(styleSource);
            document.adoptedStyleSheets = [...document.adoptedStyleSheets, adoptedStyleSheet];
            root.innerHTML = markup;
            refs = Object.fromEntries(Array.from(root.querySelectorAll("[data-ref]")).map((element) => [element.dataset.ref, element]));
            bindEvents();
            resizeObserver = new ResizeObserver(scheduleChartRedraw);
            resizeObserver.observe(refs.chart);
            render(snapshot);
            if (!quoteOf(stateOf(snapshot)).price) {
                setTimeout(() => {
                    if (!suspended && !disposed) requestRefresh();
                }, 80);
            }
            return cleanup;
        },
        update({ snapshot }) {
            render(snapshot);
        },
        suspend() {
            suspended = true;
            pending = false;
            pendingAction = null;
            pendingPeriod = null;
            pendingRequestId = null;
            clearScheduledWork();
            hideChartTip();
            // resume 要整帧重画（S8d3-3），所以这里作废两个 paintKey，否则 resume 的 render
            // 会撞上同一个 revision 而跳过四个更新函数和图例。
            paintedKey = "";
            legendPaintedKey = "";
        },
        resume() {
            if (disposed) return;
            suspended = false;
            // suspend 期间到达的快照不会触发 update，控件禁用态也在 suspend 里被清掉了，
            // 所以恢复时必须整帧重画，而不是只重启定时器和画布。
            render(snapshotValue);
            // render 只在计划变化时重排定时器；上面 clearScheduledWork 清空了 activeTimerKey，
            // 所以正常路径由 render 重启，这里只兜底 render 提前返回（无快照）的情况。
            if (activeTimerKey === "") setRefreshTimer(stateOf(snapshotValue).intervalSeconds);
            drawChart();
        },
        dispose() {
            disposed = true;
            cleanup();
            refs = {};
            rootElement = null;
            snapshotValue = null;
        }
    });
})();
