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
        // Invalidate paint keys so resume() performs a full frame repaint instead of skipping
        // block updaters and legend rendering when the revision has not changed.
        paintedKey = "";
        legendPaintedKey = "";
        chartPaintedKey = "";
    },
    resume() {
        if (disposed) return;
        suspended = false;
        // Snapshots delivered while suspended do not trigger update(), and control disabled states
        // were cleared on suspend, so repaint the full frame rather than only restarting timers.
        render(snapshotValue);
        // render() reschedules timers whenever the plan key changes (clearScheduledWork reset
        // activeTimerKey above); this fallback covers an early return when snapshotValue is null.
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
