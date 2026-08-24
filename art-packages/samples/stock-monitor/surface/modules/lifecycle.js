// DOM event binding, redraw scheduling and cleanup.
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
    if (disposed || suspended || resizeFrame !== null) return;
    resizeFrame = requestAnimationFrame(() => {
        resizeFrame = null;
        if (!disposed && !suspended) drawChart();
    });
};

const clearScheduledWork = () => {
    if (refreshTimer !== null) clearInterval(refreshTimer);
    if (pendingTimer !== null) clearTimeout(pendingTimer);
    if (tickTimer !== null) clearTimeout(tickTimer);
    if (initialRefreshTimer !== null) clearTimeout(initialRefreshTimer);
    refreshTimer = null;
    pendingTimer = null;
    tickTimer = null;
    initialRefreshTimer = null;
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
    chartPaintedKey = "";
    resizeObserver && resizeObserver.disconnect();
    resizeObserver = null;
    if (adoptedStyleSheet) {
        document.adoptedStyleSheets = document.adoptedStyleSheets.filter((sheet) => sheet !== adoptedStyleSheet);
        adoptedStyleSheet = null;
    }
};
