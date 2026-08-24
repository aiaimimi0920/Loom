// Surface action dispatch, deadlines and refresh scheduling.
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
