// Authoritative snapshot reconciliation and paint scheduling.
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
    refs.status.title = hasError ? boundedText(state.error, "") : refs.status.textContent;
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
    refs.disclaimer.textContent = boundedText(state.disclaimer, "行情可能延迟，不构成投资建议或交易指令");
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
    const chartKey = paintRevision < 0 ? "" : paintRevision + "|" + activeView + "|" + period + "|" + history.length;
    if (chartKey === "" || chartKey !== chartPaintedKey) {
        chartPaintedKey = chartKey;
        drawChart();
    }
};
