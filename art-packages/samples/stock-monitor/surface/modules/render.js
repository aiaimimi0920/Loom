// Authoritative snapshot reconciliation and paint scheduling.
const render = (snapshot) => {
    if (!rootElement || !snapshot) return;
    snapshotValue = snapshot;
    const state = stateOf(snapshot);
    const revision = Number(snapshot.revision);
    // Unlock pending controls only when the incoming revision matches the echoed action correlation,
    // because background refreshes also advance revision via statePatch merges.
    const echoedRequestId = text(state.lastRequestId, "");
    const echoedActionId = text(state.lastActionId, "");
    const settledBy = (requestId, actionId) => {
        if (echoedRequestId) return echoedRequestId === requestId;
        // Fall back to actionId or revision-only unlock when running against an older runtime without echoes.
        if (echoedActionId) return echoedActionId === actionId;
        return true;
    };
    if (pending && revision > pendingRevision && settledBy(pendingRequestId, pendingAction)) {
        pending = false;
        pendingAction = null;
        pendingPeriod = null;
        pendingRequestId = null;
        // Reset tick count after a full refresh lands.
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
        ? "Watchlist Summary"
        : hasQuote ? text(quote.name, "Stock Monitor") : "Stock Monitor";
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
        ? "Switching period"
        : pendingAction === "stock_symbol_commit" ? "Loading symbol" : "Refreshing";
    refs.status.textContent = pending ? pendingText : hasError ? "Quote error" : text(state.statusText, "Waiting for initial refresh");
    refs.status.title = hasError ? boundedText(state.error, "") : refs.status.textContent;
    refs.status.className = "stock-status" + (pending ? " is-loading" : hasError ? " is-error" : state.status === "ready" ? " is-ready" : "");
    refs.name.textContent = hasQuote ? text(quote.name, "Unknown Stock") : "Waiting for quote";
    refs.name.title = refs.name.textContent;
    refs.code.textContent = code + " · " + market;
    refs.price.textContent = hasQuote ? formatNumber(quote.price, 2) : "--";
    refs.price.style.color = hasQuote ? color : COLORS.text;
    refs.delta.textContent = hasQuote
        ? formatSigned(quote.change, "") + "  " + formatSigned(quote.changePercent, "%")
        : "Waiting for live price";
    refs.delta.style.color = hasQuote ? color : COLORS.muted;
    const timerPlan = refreshPlan(interval, marketStatus);
    const cadence = timerPlan.cadence;
    const cadenceText = "Auto " + (INTERVAL_LABELS[cadence] || cadence + "s");
    const liveText = timerPlan.usesTick ? " · Near-realtime" : "";
    const fetchedAt = text(quote.fetchedAt, text(quote.observedAt, ""));
    const freshnessText = quote.stale ? " · Data may be stale" : "";
    refs.session.textContent = hasQuote
        ? marketStatus === "open"
            ? "Trading · " + periodLabel + " · Updated " + formatClock(fetchedAt) + " · " + cadenceText + liveText + freshnessText
            : "Closed · Last trading day " + lastTradingDate + " · Last " + formatNumber(quote.price, 2) + " · Updated " + formatClock(fetchedAt) + " · " + cadenceText + freshnessText
        : "Source: stock-api MCP";
    refs.session.title = hasQuote
        ? refs.session.textContent + " · Observed " + formatTimestamp(quote.observedAt) + " · Fetched " + formatTimestamp(fetchedAt) + " · Near-realtime ticks this session: " + liveTickCount
        : refs.session.textContent;
    // Rebuild legend nodes only when period or market palette changes.
    const legendKey = period + "|" + palette.up + "|" + palette.down;
    if (legendKey !== legendPaintedKey) {
        legendPaintedKey = legendKey;
        const candleGradient = "linear-gradient(90deg," + palette.up + " 0 48%," + palette.down + " 52% 100%)";
        refs.legend.replaceChildren(...(isIntradayPeriod(period)
            ? [legendItem(periodLabel + " Price", "close"), legendItem("Avg", "average"), legendItem("Volume", "volume")]
            : [legendItem(periodLabel, "candle", candleGradient), legendItem("Close", "close"), legendItem("MA5", "average"), legendItem("Volume", "volume")]));
        refs.chart.setAttribute("aria-label", periodLabel + (isIntradayPeriod(period) ? " price and volume chart" : " K-line, MA5, and volume chart"));
    }
    const footerNotice = footerNoticeOf(state);
    refs.error.textContent = footerNotice.text;
    refs.error.title = footerNotice.text;
    refs.error.classList.toggle("is-warning", footerNotice.warning);
    refs.disclaimer.textContent = boundedText(state.disclaimer, "Quotes may be delayed, not investment advice or trading instructions");
    // Reconcile quote, order-book, history, and watchlist blocks only when revision, view, or row count changes;
    // header/status/session nodes above stay un-gated so purely local pending transitions still render immediately.
    const paintRevision = revisionOf(snapshot);
    // Repaint every frame when revision is absent rather than pinning the UI to the initial frame.
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
