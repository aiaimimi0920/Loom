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
    const INTERVALS = [5, 15, 30, 60];

    let rootElement = null;
    let snapshotValue = null;
    let refs = {};
    let refreshTimer = null;
    let pendingTimer = null;
    let resizeObserver = null;
    let suspended = false;
    let disposed = false;
    let pending = false;
    let pendingRevision = -1;
    let activeInterval = 15;

    const asObject = (value) => value && typeof value === "object" && !Array.isArray(value) ? value : {};
    const asNumber = (value) => {
        const number = Number(value);
        return Number.isFinite(number) ? number : null;
    };
    const stateOf = (snapshot) => asObject(snapshot && snapshot.authoritativeState);
    const quoteOf = (state) => asObject(state.quote);
    const trendOf = (state) => Array.isArray(state.trend) ? state.trend : [];
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
    const formatAmount = (value) => {
        const number = asNumber(value);
        if (number === null) return "--";
        if (Math.abs(number) >= 100000000) return formatNumber(number / 100000000, 2) + " 亿";
        if (Math.abs(number) >= 10000) return formatNumber(number / 10000, 2) + " 万";
        return formatNumber(number, 0);
    };
    const formatVolume = (value) => {
        const number = asNumber(value);
        if (number === null) return "--";
        return formatNumber(number / 10000, 2) + " 万手";
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
        ".chart-wrap{position:relative;min-width:0;min-height:160px;padding:9px 10px 7px;background:" + COLORS.surface + "}",
        ".chart-legend{position:absolute;z-index:1;top:8px;left:10px;right:10px;display:flex;gap:12px;pointer-events:none;color:" + COLORS.muted + ";font-size:10px}",
        ".legend-item{display:inline-flex;align-items:center;gap:4px;white-space:nowrap}",
        ".legend-line{width:14px;height:2px;background:" + COLORS.blue + "}",
        ".legend-line.price{background:" + COLORS.green + "}",
        ".stock-chart{display:block;width:100%;height:100%;min-height:145px}",
        ".market-grid{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));background:" + COLORS.panel + ";border-bottom:1px solid " + COLORS.line + "}",
        ".market-cell{min-width:0;padding:8px 9px;border-right:1px solid " + COLORS.line + ";border-bottom:1px solid " + COLORS.line + "}",
        ".market-cell:nth-child(6n){border-right:0}",
        ".metric-label{display:block;color:" + COLORS.muted + ";font-size:10px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".metric-value{display:block;margin-top:3px;color:" + COLORS.text + ";font:600 12px/1.2 Consolas,monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-footer{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:10px;align-items:center;padding:7px 12px;background:" + COLORS.surface + ";color:" + COLORS.muted + ";font-size:10px}",
        ".stock-error{min-width:0;color:" + COLORS.red + ";white-space:nowrap;overflow:hidden;text-overflow:ellipsis}",
        ".stock-disclaimer{text-align:right;white-space:nowrap}",
        "@media(max-width:560px){.stock-shell{grid-template-rows:auto auto auto minmax(160px,1fr) auto auto}.stock-header{padding:9px 10px}.stock-status{max-width:115px}.stock-controls{grid-template-columns:minmax(0,1fr) 82px 32px;padding:7px 10px}.quote-board{display:contents}.quote-summary{padding:10px;border-right:0;border-bottom:1px solid " + COLORS.line + "}.quote-price{font-size:30px}.chart-wrap{min-height:175px}.market-grid{grid-template-columns:repeat(3,minmax(0,1fr))}.market-cell:nth-child(6n){border-right:1px solid " + COLORS.line + "}.market-cell:nth-child(3n){border-right:0}.stock-footer{grid-template-columns:1fr}.stock-disclaimer{text-align:left;white-space:normal}}",
        "@media(max-width:390px){.stock-title{font-size:14px}.stock-status{display:none}.stock-header{grid-template-columns:1fr}.stock-controls{grid-template-columns:minmax(0,1fr) 76px 30px}.market-grid{grid-template-columns:repeat(2,minmax(0,1fr))}.market-cell:nth-child(3n){border-right:1px solid " + COLORS.line + "}.market-cell:nth-child(2n){border-right:0}}",
        "@media(prefers-reduced-motion:reduce){.stock-shell *{scroll-behavior:auto!important;transition:none!important}}"
    ].join("");

    const markup = [
        '<section class="stock-shell" aria-label="股票盯盘">',
        '<header class="stock-header">',
        '<div><div class="stock-kicker">NEURO MARKET WATCH</div><h1 class="stock-title" data-ref="title">股票盯盘</h1></div>',
        '<div class="stock-status" data-ref="status" role="status" aria-live="polite">等待首次刷新</div>',
        '</header>',
        '<div class="stock-controls">',
        '<input class="stock-control stock-symbol" data-ref="symbol" aria-label="沪深 A 股代码" inputmode="text" maxlength="12" value="000034">',
        '<select class="stock-control stock-interval" data-ref="interval" aria-label="刷新间隔"><option value="5">5 秒</option><option value="15">15 秒</option><option value="30">30 秒</option><option value="60">60 秒</option></select>',
        '<button class="stock-control stock-refresh" data-ref="refresh" type="button" aria-label="刷新行情" title="刷新行情">↻</button>',
        '</div>',
        '<section class="quote-board">',
        '<div class="quote-summary">',
        '<div class="quote-identity"><span class="quote-name" data-ref="name">等待行情</span><span class="quote-code" data-ref="code">000034 · SZ</span></div>',
        '<div class="quote-price" data-ref="price">--</div>',
        '<div class="quote-delta" data-ref="delta">等待真实价格</div>',
        '<div class="quote-session" data-ref="session">数据源：东方财富</div>',
        '</div>',
        '<div class="chart-wrap">',
        '<div class="chart-legend"><span class="legend-item"><i class="legend-line price" data-ref="priceLegend"></i>价格</span><span class="legend-item"><i class="legend-line"></i>均价</span></div>',
        '<canvas class="stock-chart" data-ref="chart" aria-label="分时价格与成交量图"></canvas>',
        '</div>',
        '</section>',
        '<section class="market-grid" data-ref="metrics"></section>',
        '<footer class="stock-footer"><span class="stock-error" data-ref="error"></span><span class="stock-disclaimer" data-ref="disclaimer">仅用于行情观察，不构成交易指令</span></footer>',
        '</section>'
    ].join("");

    const metricDefinitions = [
        ["今开", "open", (value) => formatNumber(value, 2)],
        ["最高", "high", (value) => formatNumber(value, 2)],
        ["最低", "low", (value) => formatNumber(value, 2)],
        ["昨收", "previousClose", (value) => formatNumber(value, 2)],
        ["成交量", "volumeLots", formatVolume],
        ["成交额", "amount", formatAmount],
        ["换手率", "turnoverRate", (value) => formatNumber(value, 2) + (asNumber(value) === null ? "" : "%")],
        ["量比", "volumeRatio", (value) => formatNumber(value, 2)],
        ["振幅", "amplitude", (value) => formatNumber(value, 2) + (asNumber(value) === null ? "" : "%")],
        ["市盈率", "peDynamic", (value) => formatNumber(value, 2)],
        ["市净率", "pb", (value) => formatNumber(value, 2)],
        ["流通市值", "floatMarketCap", formatAmount]
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
        }, 16000);
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

    const requestRefresh = () => emitAction("refresh", "click", "stock_refresh", "discrete", {});

    const setRefreshTimer = (intervalSeconds) => {
        const normalized = INTERVALS.includes(Number(intervalSeconds)) ? Number(intervalSeconds) : 15;
        activeInterval = normalized;
        if (refreshTimer !== null) clearInterval(refreshTimer);
        refreshTimer = null;
        if (!disposed && !suspended) {
            refreshTimer = setInterval(() => {
                if (!pending) requestRefresh();
            }, normalized * 1000);
        }
    };

    const updateMetrics = (quote) => {
        refs.metrics.replaceChildren();
        metricDefinitions.forEach((definition) => {
            const cell = document.createElement("div");
            cell.className = "market-cell";
            const label = document.createElement("span");
            label.className = "metric-label";
            label.textContent = definition[0];
            const value = document.createElement("strong");
            value.className = "metric-value";
            value.textContent = definition[2](quote[definition[1]]);
            cell.append(label, value);
            refs.metrics.append(cell);
        });
    };

    const drawChart = () => {
        if (!refs.chart || !snapshotValue) return;
        const canvas = refs.chart;
        const bounds = canvas.getBoundingClientRect();
        const width = Math.max(260, Math.floor(bounds.width || 520));
        const height = Math.max(145, Math.floor(bounds.height || 180));
        const ratio = Math.min(2, Math.max(1, globalThis.devicePixelRatio || 1));
        canvas.width = Math.floor(width * ratio);
        canvas.height = Math.floor(height * ratio);
        const context = canvas.getContext("2d");
        if (!context) return;
        context.setTransform(ratio, 0, 0, ratio, 0, 0);
        context.clearRect(0, 0, width, height);
        context.fillStyle = COLORS.surface;
        context.fillRect(0, 0, width, height);

        const left = 8;
        const right = width - 8;
        const top = 28;
        const volumeTop = Math.floor(height * 0.74);
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
        const quote = quoteOf(state);
        const rawTrend = trendOf(state);
        const points = rawTrend.map((item) => {
            const row = asObject(item);
            return {
                price: asNumber(row.price),
                average: asNumber(row.average),
                volume: asNumber(row.volumeLots)
            };
        }).filter((item) => item.price !== null);
        if (points.length < 2) {
            context.fillStyle = COLORS.muted;
            context.font = "11px Segoe UI, Microsoft YaHei, sans-serif";
            context.textAlign = "center";
            context.fillText("等待真实分时行情", width / 2, top + (volumeTop - top) / 2);
            return;
        }

        const values = [];
        points.forEach((point) => {
            values.push(point.price);
            if (point.average !== null) values.push(point.average);
        });
        const previous = asNumber(quote.previousClose);
        if (previous !== null) values.push(previous);
        let minimum = Math.min.apply(null, values);
        let maximum = Math.max.apply(null, values);
        if (minimum === maximum) {
            minimum -= 0.01;
            maximum += 0.01;
        }
        const padding = Math.max((maximum - minimum) * 0.08, 0.01);
        minimum -= padding;
        maximum += padding;
        const xAt = (index) => left + (right - left) * index / Math.max(1, points.length - 1);
        const yAt = (value) => top + (maximum - value) * (volumeTop - top - 4) / (maximum - minimum);

        const maxVolume = Math.max.apply(null, points.map((point) => point.volume || 0).concat([1]));
        context.fillStyle = "rgba(146,154,159,0.34)";
        const barWidth = Math.max(1, (right - left) / points.length * 0.72);
        points.forEach((point, index) => {
            const barHeight = ((point.volume || 0) / maxVolume) * Math.max(8, bottom - volumeTop - 3);
            context.fillRect(xAt(index) - barWidth / 2, bottom - barHeight, barWidth, barHeight);
        });

        const drawLine = (key, color, lineWidth) => {
            context.beginPath();
            let started = false;
            points.forEach((point, index) => {
                const value = point[key];
                if (value === null) return;
                const x = xAt(index);
                const y = yAt(value);
                if (!started) {
                    context.moveTo(x, y);
                    started = true;
                } else {
                    context.lineTo(x, y);
                }
            });
            context.strokeStyle = color;
            context.lineWidth = lineWidth;
            context.lineJoin = "round";
            context.lineCap = "round";
            context.stroke();
        };
        drawLine("average", COLORS.blue, 1.2);
        drawLine("price", movementColor(movement(quote)), 1.8);

        context.font = "10px Consolas, monospace";
        context.fillStyle = COLORS.muted;
        context.textAlign = "left";
        context.fillText(maximum.toFixed(2), left + 2, top + 10);
        context.fillText(minimum.toFixed(2), left + 2, volumeTop - 5);
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
        const kind = movement(quote);
        const color = movementColor(kind);
        const symbol = text(state.symbol, "000034");
        const market = text(state.market, "SZ");
        const interval = INTERVALS.includes(Number(state.intervalSeconds)) ? Number(state.intervalSeconds) : 15;
        const hasQuote = asNumber(quote.price) !== null;
        const hasError = typeof state.error === "string" && state.error.trim().length > 0;

        refs.title.textContent = hasQuote ? text(quote.name, "股票盯盘") : "股票盯盘";
        if (document.activeElement !== refs.symbol) refs.symbol.value = symbol;
        refs.interval.value = String(interval);
        refs.refresh.disabled = pending;
        refs.status.textContent = pending ? "正在刷新" : hasError ? "行情异常" : text(state.statusText, "等待首次刷新");
        refs.status.title = hasError ? state.error : refs.status.textContent;
        refs.status.className = "stock-status" + (pending ? " is-loading" : hasError ? " is-error" : state.status === "ready" ? " is-ready" : "");
        refs.name.textContent = hasQuote ? text(quote.name, "未知股票") : "等待行情";
        refs.name.title = refs.name.textContent;
        refs.code.textContent = symbol + " · " + market;
        refs.price.textContent = hasQuote ? formatNumber(quote.price, 2) : "--";
        refs.price.style.color = hasQuote ? color : COLORS.text;
        refs.delta.textContent = hasQuote
            ? formatSigned(quote.change, "") + "  " + formatSigned(quote.changePercent, "%")
            : "等待真实价格";
        refs.delta.style.color = hasQuote ? color : COLORS.muted;
        refs.priceLegend.style.background = color;
        refs.session.textContent = hasQuote
            ? text(quote.marketStateLabel, "行情") + " · " + text(quote.timestamp, "时间未知") + " · 东方财富"
            : "数据源：东方财富";
        refs.error.textContent = hasError ? state.error : "";
        refs.error.title = hasError ? state.error : "";
        refs.disclaimer.textContent = text(state.disclaimer, "仅用于行情观察，不构成交易指令");
        updateMetrics(quote);
        if (interval !== activeInterval) setRefreshTimer(interval);
        drawChart();
    };

    const bindEvents = () => {
        refs.refresh.addEventListener("click", requestRefresh);
        refs.symbol.addEventListener("keydown", (event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            emitAction("symbol", "change", "stock_symbol_commit", "commit", { value: refs.symbol.value.trim() });
        });
        refs.symbol.addEventListener("change", () => {
            emitAction("symbol", "change", "stock_symbol_commit", "commit", { value: refs.symbol.value.trim() });
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
    };

    NeuroSurface.define({
        mount({ root, snapshot }) {
            rootElement = root;
            snapshotValue = snapshot;
            const style = document.createElement("style");
            style.textContent = styleSource;
            root.innerHTML = markup;
            root.prepend(style);
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
