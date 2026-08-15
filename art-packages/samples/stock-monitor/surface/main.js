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
    const INTERVALS = [30, 60, 120, 300];
    const MAX_CANVAS_WIDTH = 2048;
    const MAX_CANVAS_HEIGHT = 1024;
    const MAX_CANVAS_PIXELS = 4 * 1024 * 1024;

    let rootElement = null;
    let snapshotValue = null;
    let refs = {};
    let refreshTimer = null;
    let pendingTimer = null;
    let resizeObserver = null;
    let adoptedStyleSheet = null;
    let suspended = false;
    let disposed = false;
    let pending = false;
    let pendingRevision = -1;
    let activeInterval = 60;

    const asObject = (value) => value && typeof value === "object" && !Array.isArray(value) ? value : {};
    const asNumber = (value) => {
        const number = Number(value);
        return Number.isFinite(number) ? number : null;
    };
    const stateOf = (snapshot) => asObject(snapshot && snapshot.authoritativeState);
    const quoteOf = (state) => asObject(state.quote);
    const historyOf = (state) => Array.isArray(state.history) ? state.history : [];
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
    const movementColor = (kind) => kind === "positive"
        ? COLORS.green
        : kind === "negative" ? COLORS.red : COLORS.yellow;

    const styleSource = [
        ":root{color-scheme:dark}",
        "html,body{background:transparent}",
        ".stock-shell{min-width:0;min-height:100%;height:100%;overflow:auto;background:" + COLORS.background + ";color:" + COLORS.text + ";font-family:Segoe UI,Microsoft YaHei,sans-serif;font-size:12px;line-height:1.35;letter-spacing:0;display:grid;grid-template-rows:auto auto minmax(160px,1fr) auto auto}",
        ".stock-shell *{box-sizing:border-box;letter-spacing:0}",
        ".stock-header{display:grid;grid-template-columns:minmax(0,1fr) auto;align-items:center;gap:10px;padding:10px 12px 9px;background:" + COLORS.surface + ";border-bottom:1px solid " + COLORS.line + "}",
        ".stock-kicker{font:700 11px/1.2 Consolas,monospace;color:" + COLORS.yellow + ";white-space:nowrap}",
        ".stock-title{margin:2px 0 0;font-size:16px;line-height:1.2;font-weight:700;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-status{justify-self:end;max-width:180px;padding-left:9px;border-left:2px solid " + COLORS.yellow + ";color:" + COLORS.muted + ";font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-status.is-ready{border-color:" + COLORS.green + ";color:" + COLORS.green + "}",
        ".stock-status.is-error{border-color:" + COLORS.red + ";color:" + COLORS.red + "}",
        ".stock-status.is-loading{border-color:" + COLORS.yellow + ";color:" + COLORS.yellow + "}",
        ".stock-controls{display:grid;grid-template-columns:minmax(110px,170px) 92px 32px;align-items:center;gap:6px;padding:8px 12px;background:" + COLORS.surface + ";border-bottom:1px solid " + COLORS.line + "}",
        ".stock-control{height:30px;min-width:0;border:1px solid " + COLORS.line + ";border-radius:3px;background:" + COLORS.control + ";color:" + COLORS.text + ";font:600 12px/1 Segoe UI,Microsoft YaHei,sans-serif;outline:none}",
        ".stock-control:hover{background:" + COLORS.controlHover + "}",
        ".stock-control:focus-visible{outline:2px solid " + COLORS.yellow + ";outline-offset:1px}",
        ".stock-symbol{padding:0 9px;font-family:Consolas,monospace}",
        ".stock-interval{padding:0 7px}",
        ".stock-refresh{display:grid;place-items:center;padding:0;border-color:" + COLORS.yellow + ";background:" + COLORS.yellow + ";color:#11150a;font-size:18px;font-weight:800;cursor:pointer}",
        ".stock-refresh:hover{background:color-mix(in srgb," + COLORS.yellow + " 86%,white)}",
        ".stock-refresh:disabled{cursor:wait;opacity:.62}",
        ".quote-board{display:grid;grid-template-columns:minmax(160px,.75fr) minmax(220px,1.25fr);min-height:0;background:" + COLORS.surface + ";border-bottom:1px solid " + COLORS.line + "}",
        ".quote-summary{min-width:0;padding:12px;border-right:1px solid " + COLORS.line + ";display:flex;flex-direction:column;justify-content:center}",
        ".quote-identity{display:flex;align-items:baseline;gap:7px;min-width:0}",
        ".quote-name{font-size:14px;font-weight:700;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".quote-code{color:" + COLORS.muted + ";font:600 11px/1 Consolas,monospace;white-space:nowrap}",
        ".quote-price{margin-top:5px;font:700 34px/1 Consolas,monospace;color:" + COLORS.text + ";white-space:nowrap}",
        ".quote-delta{margin-top:6px;font:700 13px/1.2 Consolas,monospace;white-space:nowrap}",
        ".quote-session{margin-top:9px;color:" + COLORS.muted + ";font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".chart-wrap{position:relative;min-width:0;min-height:160px;overflow:hidden;contain:layout paint;padding:9px 10px 7px;background:" + COLORS.surface + "}",
        ".chart-legend{position:absolute;z-index:1;top:8px;left:10px;right:10px;display:flex;gap:12px;pointer-events:none;color:" + COLORS.muted + ";font-size:10px}",
        ".legend-item{display:inline-flex;align-items:center;gap:4px;white-space:nowrap}",
        ".legend-line{width:14px;height:2px;background:" + COLORS.blue + "}",
        ".legend-line.candle{background:linear-gradient(90deg," + COLORS.green + " 0 48%," + COLORS.red + " 52% 100%)}",
        ".legend-line.average{background:" + COLORS.yellow + "}",
        ".stock-chart{position:absolute;inset:0;display:block;width:100%;height:100%;min-width:0;min-height:0}",
        ".market-grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));background:" + COLORS.panel + ";border-bottom:1px solid " + COLORS.line + "}",
        ".market-cell{min-width:0;padding:8px 9px;border-right:1px solid " + COLORS.line + ";border-bottom:1px solid " + COLORS.line + "}",
        ".market-cell:nth-child(4n){border-right:0}",
        ".metric-label{display:block;color:" + COLORS.muted + ";font-size:10px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".metric-value{display:block;margin-top:3px;color:" + COLORS.text + ";font:600 12px/1.2 Consolas,monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-footer{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:10px;align-items:center;padding:7px 12px;background:" + COLORS.surface + ";color:" + COLORS.muted + ";font-size:10px}",
        ".stock-error{min-width:0;color:" + COLORS.red + ";white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-disclaimer{text-align:right;white-space:nowrap}",
        "@media(max-width:560px){.stock-shell{grid-template-rows:auto auto auto minmax(160px,1fr) auto auto}.stock-header{padding:9px 10px}.stock-status{max-width:115px}.stock-controls{grid-template-columns:minmax(0,1fr) 82px 32px;padding:7px 10px}.quote-board{display:contents}.quote-summary{padding:10px;border-right:0;border-bottom:1px solid " + COLORS.line + "}.quote-price{font-size:30px}.chart-wrap{min-height:175px}.market-grid{grid-template-columns:repeat(2,minmax(0,1fr))}.market-cell:nth-child(4n){border-right:1px solid " + COLORS.line + "}.market-cell:nth-child(2n){border-right:0}.stock-footer{grid-template-columns:1fr}.stock-disclaimer{text-align:left;white-space:normal}}",
        "@media(max-width:390px){.stock-title{font-size:14px}.stock-status{display:none}.stock-header{grid-template-columns:1fr}.stock-controls{grid-template-columns:minmax(0,1fr) 76px 30px}}",
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
        '<select class="stock-control stock-interval" data-ref="interval" aria-label="刷新间隔"><option value="30">30 秒</option><option value="60">60 秒</option><option value="120">2 分钟</option><option value="300">5 分钟</option></select>',
        '<button class="stock-control stock-refresh" data-ref="refresh" type="button" aria-label="刷新行情" title="刷新行情">↻</button>',
        '</div>',
        '<section class="quote-board">',
        '<div class="quote-summary">',
        '<div class="quote-identity"><span class="quote-name" data-ref="name">等待行情</span><span class="quote-code" data-ref="code">SZ000034 · SZ</span></div>',
        '<div class="quote-price" data-ref="price">--</div>',
        '<div class="quote-delta" data-ref="delta">等待真实价格</div>',
        '<div class="quote-session" data-ref="session">数据源：stock-api MCP</div>',
        '</div>',
        '<div class="chart-wrap">',
        '<div class="chart-legend"><span class="legend-item"><i class="legend-line candle"></i>日 K</span><span class="legend-item"><i class="legend-line average"></i>MA5</span><span class="legend-item"><i class="legend-line"></i>成交量</span></div>',
        '<canvas class="stock-chart" data-ref="chart" aria-label="日 K 线与成交量图"></canvas>',
        '</div>',
        '</section>',
        '<section class="market-grid" data-ref="metrics"></section>',
        '<footer class="stock-footer"><span class="stock-error" data-ref="error"></span><span class="stock-disclaimer" data-ref="disclaimer">行情可能延迟，不构成投资建议或交易指令</span></footer>',
        '</section>'
    ].join("");

    const metricDefinitions = [
        ["开盘", (quote) => formatNumber(quote.open, 2)],
        ["最高", (quote) => formatNumber(quote.high, 2)],
        ["最低", (quote) => formatNumber(quote.low, 2)],
        ["昨收", (quote) => formatNumber(quote.previousClose, 2)],
        ["K 线", (_quote, history) => history.length ? history.length + " 日" : "--"],
        ["最新量", (_quote, history) => history.length ? formatVolume(asObject(history[history.length - 1]).volume) : "--"],
        ["实际源", (quote) => text(quote.source, "--")],
        ["观测时间", (quote) => formatTimestamp(quote.observedAt)]
    ];

    const emitAction = (nodeId, eventName, action, eventClass, payload) => {
        if (disposed || suspended || !snapshotValue) return false;
        if (action === "stock_refresh" && pending) return false;
        pending = true;
        pendingRevision = Number(snapshotValue.revision) || 0;
        if (pendingTimer !== null) clearTimeout(pendingTimer);
        pendingTimer = setTimeout(() => {
            pendingTimer = null;
            pending = false;
            if (refs.status) {
                refs.status.textContent = "刷新超时";
                refs.status.className = "stock-status is-error";
            }
        }, 32000);
        render(snapshotValue);
        const accepted = NeuroSurface.emit({
            nodeId: nodeId,
            event: eventName,
            action: action,
            class: eventClass,
            payload: payload || {}
        });
        if (!accepted) {
            pending = false;
            if (pendingTimer !== null) clearTimeout(pendingTimer);
            pendingTimer = null;
            render(snapshotValue);
        }
        return accepted;
    };

    const requestRefresh = () => emitAction("refresh", "click", "stock_refresh", "discrete", {
        code: normalizeCode(refs.symbol && refs.symbol.value)
    });

    const setRefreshTimer = (intervalSeconds) => {
        const normalized = INTERVALS.includes(Number(intervalSeconds)) ? Number(intervalSeconds) : 60;
        activeInterval = normalized;
        if (refreshTimer !== null) clearInterval(refreshTimer);
        refreshTimer = null;
        if (!disposed && !suspended) {
            refreshTimer = setInterval(() => {
                if (!pending) requestRefresh();
            }, normalized * 1000);
        }
    };

    const updateMetrics = (quote, history) => {
        refs.metrics.replaceChildren();
        metricDefinitions.forEach((definition) => {
            const cell = document.createElement("div");
            cell.className = "market-cell";
            const label = document.createElement("span");
            label.className = "metric-label";
            label.textContent = definition[0];
            const value = document.createElement("strong");
            value.className = "metric-value";
            value.textContent = definition[1](quote, history);
            cell.append(label, value);
            refs.metrics.append(cell);
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
        const context = canvas.getContext("2d");
        if (!context) return;
        context.setTransform(ratio, 0, 0, ratio, 0, 0);
        context.clearRect(0, 0, width, height);
        context.fillStyle = COLORS.surface;
        context.fillRect(0, 0, width, height);

        const left = 10;
        const right = width - 10;
        const top = 28;
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
        const points = historyOf(state).slice(-60).map((item) => {
            const row = asObject(item);
            return {
                date: text(row.date, ""),
                open: asNumber(row.open),
                close: asNumber(row.close),
                high: asNumber(row.high),
                low: asNumber(row.low),
                volume: asNumber(row.volume)
            };
        }).filter((item) => item.open !== null && item.close !== null && item.high !== null && item.low !== null && item.high >= item.low);
        if (points.length < 2) {
            context.fillStyle = COLORS.muted;
            context.font = "11px Segoe UI, Microsoft YaHei, sans-serif";
            context.textAlign = "center";
            context.fillText("等待 stock-api 日 K 线", width / 2, top + (volumeTop - top) / 2);
            return;
        }

        let minimum = Math.min.apply(null, points.map((point) => point.low));
        let maximum = Math.max.apply(null, points.map((point) => point.high));
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

        const maxVolume = Math.max.apply(null, points.map((point) => point.volume || 0).concat([1]));
        const candleWidth = Math.max(2, Math.min(12, slotWidth * 0.58));
        points.forEach((point, index) => {
            const color = point.close >= point.open ? COLORS.green : COLORS.red;
            const x = xAt(index);
            const barHeight = ((point.volume || 0) / maxVolume) * Math.max(8, bottom - volumeTop - 3);
            context.globalAlpha = 0.34;
            context.fillStyle = color;
            context.fillRect(x - candleWidth / 2, bottom - barHeight, candleWidth, barHeight);
            context.globalAlpha = 1;

            context.strokeStyle = color;
            context.lineWidth = 1;
            context.beginPath();
            context.moveTo(x + 0.5, yAt(point.high));
            context.lineTo(x + 0.5, yAt(point.low));
            context.stroke();
            const bodyTop = Math.min(yAt(point.open), yAt(point.close));
            const bodyHeight = Math.max(1, Math.abs(yAt(point.open) - yAt(point.close)));
            context.fillStyle = color;
            context.fillRect(x - candleWidth / 2, bodyTop, candleWidth, bodyHeight);
        });

        const movingAverage = points.map((_point, index) => {
            if (index < 4) return null;
            const values = points.slice(index - 4, index + 1).map((point) => point.close);
            return values.reduce((sum, value) => sum + value, 0) / values.length;
        });
        context.beginPath();
        let started = false;
        movingAverage.forEach((value, index) => {
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
        context.stroke();

        context.font = "10px Consolas, monospace";
        context.fillStyle = COLORS.muted;
        context.textAlign = "left";
        context.fillText(maximum.toFixed(2), left + 2, top + 10);
        context.fillText(minimum.toFixed(2), left + 2, volumeTop - 5);
        context.fillText(points[0].date, left + 2, bottom - 2);
        context.textAlign = "right";
        context.fillText(points[points.length - 1].date, right - 2, bottom - 2);
    };

    const render = (snapshot) => {
        if (!rootElement || !snapshot) return;
        snapshotValue = snapshot;
        if (pending && Number(snapshot.revision) > pendingRevision) {
            pending = false;
            if (pendingTimer !== null) clearTimeout(pendingTimer);
            pendingTimer = null;
        }
        const state = stateOf(snapshot);
        const quote = quoteOf(state);
        const history = historyOf(state);
        const kind = movement(quote);
        const color = movementColor(kind);
        const code = normalizeCode(text(state.code, text(state.symbol, "SZ000034")));
        const market = text(state.market, code.slice(0, 2) || "SZ");
        const interval = INTERVALS.includes(Number(state.intervalSeconds)) ? Number(state.intervalSeconds) : 60;
        const hasQuote = asNumber(quote.price) !== null;
        const hasError = typeof state.error === "string" && state.error.trim().length > 0;

        refs.title.textContent = hasQuote ? text(quote.name, "股票盯盘") : "股票盯盘";
        if (document.activeElement !== refs.symbol) refs.symbol.value = code;
        refs.interval.value = String(interval);
        refs.refresh.disabled = pending;
        refs.status.textContent = pending ? "正在刷新" : hasError ? "行情异常" : text(state.statusText, "等待首次刷新");
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
        refs.session.textContent = hasQuote
            ? "stock-api / " + text(quote.source, "unknown") + " · " + formatTimestamp(quote.observedAt)
            : "数据源：stock-api MCP";
        refs.error.textContent = hasError ? state.error : "";
        refs.error.title = hasError ? state.error : "";
        refs.disclaimer.textContent = text(state.disclaimer, "行情可能延迟，不构成投资建议或交易指令");
        updateMetrics(quote, history);
        if (interval !== activeInterval) setRefreshTimer(interval);
        drawChart();
    };

    const bindEvents = () => {
        refs.refresh.addEventListener("click", requestRefresh);
        refs.symbol.addEventListener("keydown", (event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            emitAction("symbol", "change", "stock_symbol_commit", "commit", { value: normalizeCode(refs.symbol.value) });
        });
        refs.symbol.addEventListener("change", () => {
            emitAction("symbol", "change", "stock_symbol_commit", "commit", { value: normalizeCode(refs.symbol.value) });
        });
        refs.interval.addEventListener("change", () => {
            const value = Number(refs.interval.value);
            emitAction("interval", "change", "stock_interval_commit", "commit", { value: value });
        });
    };

    const clearScheduledWork = () => {
        if (refreshTimer !== null) clearInterval(refreshTimer);
        if (pendingTimer !== null) clearTimeout(pendingTimer);
        refreshTimer = null;
        pendingTimer = null;
    };

    const cleanup = () => {
        clearScheduledWork();
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
            clearScheduledWork();
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
