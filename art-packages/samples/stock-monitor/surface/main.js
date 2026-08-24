// Stock Monitor Surface registration and test hooks; sourceFiles share its closure.
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
        historyOf,
        favoriteQuotesOf,
        orderBookOf,
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
            initialRefreshTimer = setTimeout(() => {
                initialRefreshTimer = null;
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
        chartPaintedKey = "";
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
    },
    dispose() {
        disposed = true;
        cleanup();
        refs = {};
        rootElement = null;
        snapshotValue = null;
    }
});
