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
    const TICK_TIMEOUT_MILLIS = 12000;
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
    const ACTION_TIMEOUT_MILLIS = 50000;
    const PENDING_TIMEOUT_MILLIS = ACTION_TIMEOUT_MILLIS + 2000;

    let rootElement = null;
    let snapshotValue = null;
    let refs = {};
    let refreshTimer = null;
    let fullRefreshTimer = null;
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
    let tickPending = false;
    let tickSupported = true;
    let liveTickCount = 0;
    let activeInterval = DEFAULT_INTERVAL_SECONDS;
    let chartGeometry = null;
    let hoverIndex = -1;
    let hoverFrame = null;
    let hoverPointer = null;

    const asObject = (value) => value && typeof value === "object" && !Array.isArray(value) ? value : {};
    const asNumber = (value) => {
        const number = Number(value);
        return Number.isFinite(number) ? number : null;
    };
    const stateOf = (snapshot) => asObject(snapshot && snapshot.authoritativeState);
    const quoteOf = (state) => asObject(state.quote);
    const historyOf = (state) => Array.isArray(state.history) ? state.history : [];
    const periodOf = (state) => {
        const value = text(state.period, "minute");
        return PERIOD_VALUES.includes(value) ? value : "minute";
    };
    const periodLabelOf = (state) => {
        const value = periodOf(state);
        const found = PERIODS.find((period) => period[0] === value);
        return text(state.periodLabel, found ? found[1] : "日 K");
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
        let match = input.match(/^(SH|SZ)[:._-]?(\d{6})$/);
        if (match) return match[1] + match[2];
        match = input.match(/^(\d{6})[:._-]?(SH|SZ)$/);
        if (match) return match[2] + match[1];
        if (/^\d{6}$/.test(input)) {
            return (/^[569]/.test(input) ? "SH" : "SZ") + input;
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
        ".stock-shell{min-width:0;min-height:100%;height:100%;overflow:auto;background:" + COLORS.background + ";color:" + COLORS.text + ";font-family:Segoe UI,Microsoft YaHei,sans-serif;font-size:12px;line-height:1.35;letter-spacing:0;display:grid;grid-template-rows:auto auto minmax(160px,1fr) auto auto auto}",
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
        ".book-bar{display:flex;height:4px;border-radius:2px;overflow:hidden;background:" + COLORS.control + "}",
        ".book-bar span{display:block;height:100%}",
        ".book-columns{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:4px 10px;min-width:0}",
        ".book-side{display:grid;align-content:start;gap:2px;min-width:0}",
        ".book-row{display:grid;grid-template-columns:auto minmax(0,1fr) auto;gap:6px;align-items:center;padding:2px 5px;border-radius:2px;background:" + COLORS.control + ";font:700 11px/1.35 Consolas,monospace;white-space:nowrap}",
        ".book-tag{color:" + COLORS.muted + ";font-size:10px;font-weight:600}",
        ".book-price{min-width:0;text-align:right;overflow:hidden;text-overflow:ellipsis}",
        ".book-volume{color:" + COLORS.muted + ";font-size:10px;font-weight:600;text-align:right}",
        ".tape-strip{display:flex;flex-wrap:wrap;gap:3px 10px;min-width:0;color:" + COLORS.muted + ";font:600 10px/1.45 Consolas,monospace}",
        ".tape-item{white-space:nowrap}",
        ".tape-item strong{margin-left:4px;color:" + COLORS.text + ";font-weight:700}",
        ".stock-footer{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:10px;align-items:center;padding:7px 12px;background:" + COLORS.surface + ";color:" + COLORS.muted + ";font-size:10px}",
        ".stock-error{min-width:0;color:" + COLORS.red + ";white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-disclaimer{text-align:right;white-space:nowrap}",
        "@media(max-width:560px){.stock-shell{grid-template-rows:auto auto auto minmax(160px,1fr) auto auto auto}.stock-header{padding:9px 10px}.stock-status{max-width:115px}.stock-controls{grid-template-columns:minmax(0,1fr) auto 32px;padding:7px 10px}.stock-intervals{grid-template-columns:repeat(4,minmax(0,1fr))}.stock-periods{grid-template-columns:repeat(4,minmax(0,1fr))}.quote-board{display:contents}.quote-summary{padding:10px;border-right:0;border-bottom:1px solid " + COLORS.line + "}.quote-price{font-size:30px}.chart-wrap{min-height:175px}.market-grid{grid-template-columns:repeat(2,minmax(0,1fr))}.market-cell:nth-child(4n){border-right:1px solid " + COLORS.line + "}.market-cell:nth-child(2n){border-right:0}.book-board{padding:8px 10px}.book-head{grid-template-columns:1fr}.book-meta{justify-self:start}.stock-footer{grid-template-columns:1fr}.stock-disclaimer{text-align:left;white-space:normal}}",
        "@media(max-width:390px){.stock-title{font-size:14px}.stock-status{display:none}.stock-header{grid-template-columns:1fr}.stock-controls{grid-template-columns:minmax(0,1fr) 32px 30px}.stock-refresh{grid-column:2;grid-row:1}}",
        "@media(prefers-reduced-motion:reduce){.stock-shell *{scroll-behavior:auto!important;transition:none!important}}"
    ].join("");

    const markup = [
        '<section class="stock-shell" aria-label="股票盯盘">',
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

    const emitAction = (nodeId, eventName, action, eventClass, payload) => {
        if (disposed || suspended || !snapshotValue) return false;
        const isTickAction = action === TICK_ACTION;
        const isNetworkAction = action !== "stock_interval_commit" && !isTickAction;
        if (isTickAction && (pending || tickPending)) return false;
        if (isNetworkAction && pending) return false;
        if (isTickAction) {
            tickPending = true;
            if (tickTimer !== null) clearTimeout(tickTimer);
            tickTimer = setTimeout(() => {
                tickTimer = null;
                tickPending = false;
            }, TICK_TIMEOUT_MILLIS);
        }
        if (isNetworkAction) {
            pending = true;
            pendingAction = action;
            pendingPeriod = action === "stock_period_commit" && PERIOD_VALUES.includes(payload && payload.value)
                ? payload.value
                : null;
            pendingRevision = Number(snapshotValue.revision) || 0;
            if (pendingTimer !== null) clearTimeout(pendingTimer);
            pendingTimer = setTimeout(() => {
                pendingTimer = null;
                pending = false;
                pendingAction = null;
                pendingPeriod = null;
                if (refs.status) {
                    refs.status.textContent = action === "stock_period_commit" ? "周期切换超时" : "刷新超时";
                    refs.status.className = "stock-status is-error";
                }
            }, PENDING_TIMEOUT_MILLIS);
        }
        render(snapshotValue);
        const accepted = NeuroSurface.emit({
            nodeId: nodeId,
            event: eventName,
            action: action,
            class: eventClass,
            payload: payload || {}
        });
        if (!accepted) {
            if (isTickAction) {
                tickPending = false;
                tickSupported = false;
                if (tickTimer !== null) clearTimeout(tickTimer);
                tickTimer = null;
            }
            if (isNetworkAction) {
                pending = false;
                pendingAction = null;
                pendingPeriod = null;
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
        if (!tickSupported) return requestRefresh();
        const accepted = emitAction("refresh", "tick", TICK_ACTION, "discrete", {
            code: normalizeCode(refs.symbol && refs.symbol.value)
        });
        if (accepted) liveTickCount += 1;
        else if (!tickSupported) return requestRefresh();
        return accepted;
    };

    const effectiveIntervalSeconds = (intervalSeconds) => {
        const normalized = INTERVALS.includes(Number(intervalSeconds))
            ? Number(intervalSeconds)
            : DEFAULT_INTERVAL_SECONDS;
        const marketStatus = text(stateOf(snapshotValue).marketStatus, "closed");
        // 休市时秒级轮询没有新成交，退到 30 秒，避免空转打上游。
        return marketStatus === "open" ? normalized : Math.max(normalized, CLOSED_MARKET_MIN_SECONDS);
    };

    const setRefreshTimer = (intervalSeconds) => {
        const normalized = INTERVALS.includes(Number(intervalSeconds))
            ? Number(intervalSeconds)
            : DEFAULT_INTERVAL_SECONDS;
        activeInterval = normalized;
        if (refreshTimer !== null) clearInterval(refreshTimer);
        if (fullRefreshTimer !== null) clearInterval(fullRefreshTimer);
        refreshTimer = null;
        fullRefreshTimer = null;
        if (disposed || suspended) return;
        const cadence = effectiveIntervalSeconds(normalized);
        const usesTick = cadence < FULL_REFRESH_SECONDS;
        refreshTimer = setInterval(() => {
            if (pending) return;
            if (usesTick) requestTick();
            else requestRefresh();
        }, cadence * 1000);
        if (!usesTick) return;
        // 秒级 tick 只更新报价，仍需要一条慢速通道补齐 K 线。
        fullRefreshTimer = setInterval(() => {
            if (!pending) requestRefresh();
        }, FULL_REFRESH_SECONDS * 1000);
    };

    const updateMetrics = (quote, history, state) => {
        refs.metrics.replaceChildren();
        metricDefinitions.forEach((definition) => {
            const cell = document.createElement("div");
            cell.className = "market-cell";
            const label = document.createElement("span");
            label.className = "metric-label";
            label.textContent = definition[0];
            const value = document.createElement("strong");
            value.className = "metric-value";
            value.textContent = definition[1](quote, history, state);
            cell.append(label, value);
            refs.metrics.append(cell);
        });
    };

    const bookLevelsOf = (value) => Array.isArray(value) ? value.filter((row) => asNumber(asObject(row).price) !== null) : [];
    const orderBookOf = (state) => {
        const book = asObject(state.orderBook);
        const bids = bookLevelsOf(book.bids);
        const asks = bookLevelsOf(book.asks);
        return bids.length || asks.length ? { book: book, bids: bids, asks: asks } : null;
    };
    const renderBookSide = (host, levels, tag, previousClose, palette) => {
        host.replaceChildren();
        levels.forEach((row, index) => {
            const level = asObject(row);
            const line = document.createElement("div");
            line.className = "book-row";
            const label = document.createElement("span");
            label.className = "book-tag";
            label.textContent = tag + (asNumber(level.level) || index + 1);
            const price = document.createElement("span");
            price.className = "book-price";
            price.textContent = formatNumber(level.price, 2);
            price.style.color = previousClose === null
                ? COLORS.text
                : deltaColor(asNumber(level.price) - previousClose, palette);
            const volume = document.createElement("span");
            volume.className = "book-volume";
            volume.textContent = formatVolume(level.volume);
            const orders = asNumber(level.orders);
            line.title = tag + (asNumber(level.level) || index + 1) + " " + formatNumber(level.price, 2)
                + " · 委托量 " + formatVolume(level.volume)
                + (orders === null ? "" : " · 笔数 " + formatVolume(orders));
            line.append(label, price, volume);
            host.append(line);
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
    const updateOrderBook = (state, quote, palette) => {
        if (!refs.book) return;
        const snapshot = orderBookOf(state);
        const tape = asObject(state.liveTape);
        const hasTape = asNumber(tape.price) !== null;
        refs.book.classList.toggle("is-visible", Boolean(snapshot) || hasTape);
        if (!snapshot && !hasTape) {
            refs.bids.replaceChildren();
            refs.asks.replaceChildren();
            refs.tape.replaceChildren();
            return;
        }
        const previousClose = asNumber(quote.previousClose);
        refs.bookBar.classList.toggle("is-hidden", !snapshot);
        refs.asks.classList.toggle("is-hidden", !snapshot);
        refs.bids.classList.toggle("is-hidden", !snapshot);
        if (snapshot) {
            const book = snapshot.book;
            const levels = asNumber(book.levels) || Math.max(snapshot.bids.length, snapshot.asks.length);
            refs.bookTitle.textContent = levels + " 档盘口";
            const buyPercent = asNumber(book.buyPercent);
            const sellPercent = asNumber(book.sellPercent);
            const netVolume = asNumber(book.netVolume);
            const parts = [];
            if (buyPercent !== null && sellPercent !== null) {
                parts.push("买 " + formatNumber(buyPercent, 2) + "% / 卖 " + formatNumber(sellPercent, 2) + "%");
            }
            if (netVolume !== null) parts.push("委差 " + formatSigned(netVolume, ""));
            if (asNumber(book.ratio) !== null) parts.push("量比 " + formatNumber(book.ratio, 2));
            parts.push(text(book.source, "xueqiu") + " · " + formatClock(book.observedAt));
            refs.bookMeta.textContent = parts.join(" · ");
            refs.bookMeta.title = refs.bookMeta.textContent;
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
            refs.bookTitle.textContent = "盘中实时";
            refs.bookMeta.textContent = hasTape
                ? text(tape.source, "xueqiu") + " · " + formatClock(tape.observedAt) + " · 该市场不提供十档盘口"
                : "";
            refs.bookMeta.title = refs.bookMeta.textContent;
            refs.bids.replaceChildren();
            refs.asks.replaceChildren();
        }
        refs.tape.replaceChildren();
        if (!hasTape) return;
        tapeDefinitions.forEach((definition) => {
            const item = document.createElement("span");
            item.className = "tape-item";
            item.textContent = definition[0];
            const value = document.createElement("strong");
            value.textContent = definition[1](tape);
            item.append(value);
            refs.tape.append(item);
        });
    };

    const drawChart = () => {
        if (!refs.chart || !snapshotValue) return;
        const canvas = refs.chart;
        const bounds = canvas.getBoundingClientRect();
        const width = Math.min(MAX_CANVAS_WIDTH, Math.max(260, Math.floor(bounds.width || 520)));
        const height = Math.min(MAX_CANVAS_HEIGHT, Math.max(145, Math.floor(bounds.height || 180)));
        const deviceRatio = Math.min(2, Math.max(1, globalThis.devicePixelRatio || 1));
        const pixelRatio = Math.sqrt(MAX_CANVAS_PIXELS / Math.max(1, width * height));
        const ratio = Math.max(1, Math.min(deviceRatio, pixelRatio));
        canvas.width = Math.floor(width * ratio);
        canvas.height = Math.floor(height * ratio);
        if (refs.overlay) {
            refs.overlay.width = canvas.width;
            refs.overlay.height = canvas.height;
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
        const points = downsampleRows(chartRowsOf(state), maxPoints);
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

        let runningCloseTotal = 0;
        const averageValues = intraday
            ? points.map((point, index) => {
                runningCloseTotal += point.close;
                return runningCloseTotal / (index + 1);
            })
            : points.map((_point, index) => {
                if (index < 4) return null;
                const values = points.slice(index - 4, index + 1).map((point) => point.close);
                return values.reduce((sum, value) => sum + value, 0) / values.length;
            });
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

    const tipRow = (key, value, color) => '<div class="chart-tip-row"><span class="chart-tip-key">' + key
        + '</span><span class="chart-tip-value"' + (color ? ' style="color:' + color + '"' : '') + '>' + value + '</span></div>';

    const buildTipContent = (geometry, index) => {
        const point = geometry.points[index];
        const previous = index > 0 ? geometry.points[index - 1] : null;
        const reference = previous ? previous.close : point.open;
        const change = reference ? point.close - reference : null;
        const changePercent = reference ? ((point.close - reference) / reference) * 100 : null;
        const changeColor = deltaColor(change, geometry.palette);
        const averageValue = Array.isArray(geometry.averageValues) ? geometry.averageValues[index] : null;
        const rows = ['<div class="chart-tip-title">' + formatPointDate(point.date, geometry.intraday) + '</div>'];
        if (geometry.intraday) {
            rows.push(tipRow("价格", formatNumber(point.close, 2), changeColor));
            if (averageValue !== null && averageValue !== undefined) rows.push(tipRow("均价", formatNumber(averageValue, 2), COLORS.yellow));
        }
        else {
            rows.push(tipRow("开", formatNumber(point.open, 2)));
            rows.push(tipRow("高", formatNumber(point.high, 2), geometry.palette.up));
            rows.push(tipRow("低", formatNumber(point.low, 2), geometry.palette.down));
            rows.push(tipRow("收", formatNumber(point.close, 2), changeColor));
            if (averageValue !== null && averageValue !== undefined) rows.push(tipRow("MA5", formatNumber(averageValue, 2), COLORS.yellow));
        }
        rows.push(tipRow("涨跌", change === null ? "--" : formatSigned(change, ""), changeColor));
        rows.push(tipRow("涨幅", changePercent === null ? "--" : formatSigned(changePercent, "%"), changeColor));
        rows.push(tipRow("成交量", formatVolume(point.volume)));
        return rows.join("");
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
        refs.tip.innerHTML = buildTipContent(geometry, index);
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

    const render = (snapshot) => {
        if (!rootElement || !snapshot) return;
        snapshotValue = snapshot;
        if (Number(snapshot.revision) > pendingRevision) {
            if (pending) {
                pending = false;
                pendingAction = null;
                pendingPeriod = null;
                if (pendingTimer !== null) clearTimeout(pendingTimer);
                pendingTimer = null;
            }
            if (tickPending) {
                tickPending = false;
                if (tickTimer !== null) clearTimeout(tickTimer);
                tickTimer = null;
            }
        }
        const state = stateOf(snapshot);
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

        refs.title.textContent = hasQuote ? text(quote.name, "股票盯盘") : "股票盯盘";
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
        const cadence = effectiveIntervalSeconds(interval);
        const cadenceText = "自动 " + (INTERVAL_LABELS[cadence] || cadence + " 秒");
        const liveText = cadence < FULL_REFRESH_SECONDS && tickSupported ? " · 准实时" : "";
        refs.session.textContent = hasQuote
            ? marketStatus === "open"
                ? "交易中 · " + periodLabel + " · 更新 " + formatClock(quote.observedAt) + " · " + cadenceText + liveText
                : "休市 · 最近交易日 " + lastTradingDate + " · 最后价 " + formatNumber(quote.price, 2) + " · 更新 " + formatClock(quote.observedAt) + " · " + cadenceText
            : "数据源：stock-api MCP";
        refs.session.title = hasQuote
            ? refs.session.textContent + " · 观测时间 " + formatTimestamp(quote.observedAt) + " · 本次会话准实时刷新 " + liveTickCount + " 次"
            : refs.session.textContent;
        const candleSwatch = '<i class="legend-line candle" style="background:linear-gradient(90deg,' + palette.up + ' 0 48%,' + palette.down + ' 52% 100%)"></i>';
        refs.legend.innerHTML = isIntradayPeriod(period)
            ? '<span class="legend-item"><i class="legend-line close"></i>' + periodLabel + '价格</span><span class="legend-item"><i class="legend-line average"></i>均价</span><span class="legend-item"><i class="legend-line volume"></i>成交量</span>'
            : '<span class="legend-item">' + candleSwatch + periodLabel + '</span><span class="legend-item"><i class="legend-line close"></i>收盘</span><span class="legend-item"><i class="legend-line average"></i>MA5</span><span class="legend-item"><i class="legend-line volume"></i>成交量</span>';
        refs.chart.setAttribute("aria-label", periodLabel + (isIntradayPeriod(period) ? "价格走势与成交量图" : "K 线、MA5 与成交量图"));
        refs.error.textContent = hasError ? state.error : "";
        refs.error.title = hasError ? state.error : "";
        refs.disclaimer.textContent = text(state.disclaimer, "行情可能延迟，不构成投资建议或交易指令");
        updateMetrics(quote, history, state);
        updateOrderBook(state, quote, palette);
        if (interval !== activeInterval) setRefreshTimer(interval);
        drawChart();
    };

    const bindEvents = () => {
        refs.refresh.addEventListener("click", requestRefresh);
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
            emitAction("interval", "change", "stock_interval_commit", "commit", { value: value });
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

    const clearScheduledWork = () => {
        if (refreshTimer !== null) clearInterval(refreshTimer);
        if (fullRefreshTimer !== null) clearInterval(fullRefreshTimer);
        if (pendingTimer !== null) clearTimeout(pendingTimer);
        if (tickTimer !== null) clearTimeout(tickTimer);
        refreshTimer = null;
        fullRefreshTimer = null;
        pendingTimer = null;
        tickTimer = null;
        tickPending = false;
    };

    const cleanup = () => {
        clearScheduledWork();
        if (hoverFrame !== null) cancelAnimationFrame(hoverFrame);
        hoverFrame = null;
        hideChartTip();
        chartGeometry = null;
        resizeObserver && resizeObserver.disconnect();
        resizeObserver = null;
        if (adoptedStyleSheet) {
            document.adoptedStyleSheets = document.adoptedStyleSheets.filter((sheet) => sheet !== adoptedStyleSheet);
            adoptedStyleSheet = null;
        }
    };

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
            resizeObserver = new ResizeObserver(drawChart);
            resizeObserver.observe(refs.chart);
            render(snapshot);
            setRefreshTimer(stateOf(snapshot).intervalSeconds);
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
            clearScheduledWork();
            hideChartTip();
        },
        resume() {
            if (disposed) return;
            suspended = false;
            setRefreshTimer(stateOf(snapshotValue).intervalSeconds);
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
