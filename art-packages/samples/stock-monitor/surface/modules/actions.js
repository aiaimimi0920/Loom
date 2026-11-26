// Surface action dispatch, deadlines and refresh scheduling.
// Keep the client fallback deadline later than the host action budget: prefer the runtime-published
// budget and fall back to mirrored constants before the first response arrives. Host timeouts emit
// neither patches nor revision bumps, so firing earlier would unlock controls while a request is still in flight.
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
                refs.status.textContent = action === "stock_period_commit" ? "Period switch timed out" : "Refresh timed out";
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
        // Echo requestId through authoritativeState so render() only unlocks on the revision produced by this action.
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

// Near-realtime channel: fetches only the latest order book/tape without refetching full K-lines.
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
    // Fall back to a full refresh only when rejection disabled the tick channel; stay quiet if blocked by pending.
    if (!tickChannelEnabled()) return requestRefresh();
    return false;
};

const refreshPlan = (intervalSeconds, marketStatusValue) => {
    const normalized = INTERVALS.includes(Number(intervalSeconds))
        ? Number(intervalSeconds)
        : DEFAULT_INTERVAL_SECONDS;
    const marketStatus = text(marketStatusValue, text(stateOf(snapshotValue).marketStatus, "closed"));
    const tickCapable = tickChannelEnabled();
    // Closed markets produce no new trades at second-level cadence; floor at 30s to avoid idle upstream load.
    const requested = marketStatus === "open" ? normalized : Math.max(normalized, CLOSED_MARKET_MIN_SECONDS);
    // Without the tick channel, raise cadence to the full-refresh period so 1s polling never triggers four MCP calls per second.
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
    // Single timer channel: promote every N-th tick to a full refresh so K-lines advance without a
    // separate slow timer getting perpetually starved by in-flight ticks.
    refreshTimer = setInterval(() => {
        if (pending || tickPending) return;
        if (plan.usesTick && ticksSinceFullRefresh < plan.ticksPerFullRefresh) requestTick();
        else requestRefresh();
    }, plan.cadence * 1000);
};
