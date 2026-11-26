// Bounded state readers, chart derivation, formatting and market palette helpers.
const asObject = (value) => value && typeof value === "object" && !Array.isArray(value) ? value : {};
const asNumber = (value) => {
    const number = Number(value);
    return Number.isFinite(number) ? number : null;
};
const stateOf = (snapshot) => asObject(snapshot && snapshot.authoritativeState);
const quoteOf = (state) => asObject(state.quote);
const historyOf = (state) => {
    const history = Array.isArray(state.history) ? state.history : [];
    return history.length > MAX_HISTORY_ROWS ? history.slice(-MAX_HISTORY_ROWS) : history;
};
const favoriteQuotesOf = (state) => {
    const favorites = Array.isArray(state.favoriteQuotes) ? state.favoriteQuotes : [];
    return favorites.length > MAX_FAVORITE_QUOTES ? favorites.slice(0, MAX_FAVORITE_QUOTES) : favorites;
};
const viewOf = (snapshot) => VIEW_IDS.includes(snapshot && snapshot.viewId) ? snapshot.viewId : "full";
const periodOf = (state) => {
    const value = text(state.period, "minute");
    return PERIOD_VALUES.includes(value) ? value : "minute";
};
const periodLabelOf = (state) => {
    const value = periodOf(state);
    const found = PERIODS.find((period) => period[0] === value);
    // Resolve labels exclusively from the local closed table instead of trusting cross-process state.periodLabel.
    return found ? found[1] : "1D";
};
const isIntradayPeriod = (period) => period === "minute" || period === "five-day" || period.indexOf("minute-") === 0;
const chartRowsOf = (history) => history.map((item) => {
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
        const first = rows[start];
        const last = rows[end - 1];
        let high = -Infinity;
        let low = Infinity;
        let volume = 0;
        for (let cursor = start; cursor < end; cursor += 1) {
            high = Math.max(high, rows[cursor].high);
            low = Math.min(low, rows[cursor].low);
            volume += rows[cursor].volume || 0;
        }
        result.push({
            date: last.date,
            open: first.open,
            close: last.close,
            high,
            low,
            volume
        });
    }
    return result;
};
// Rolling accumulator avoids allocating intermediate slice/map/reduce arrays per point on every redraw.
const movingAverages = (points, intraday) => {
    const values = new Array(points.length);
    let total = 0;
    if (intraday) {
        for (let index = 0; index < points.length; index += 1) {
            total += points[index].close;
            values[index] = total / (index + 1);
        }
        return values;
    }
    for (let index = 0; index < points.length; index += 1) {
        total += points[index].close;
        if (index >= MOVING_AVERAGE_WINDOW) total -= points[index - MOVING_AVERAGE_WINDOW].close;
        values[index] = index < MOVING_AVERAGE_WINDOW - 1 ? null : total / MOVING_AVERAGE_WINDOW;
    }
    return values;
};
const revisionOf = (snapshot) => {
    const value = Number(snapshot && snapshot.revision);
    return Number.isFinite(value) ? value : -1;
};
// Memoize derived series by (code, period, revision, row count) so resize, hover, and rejected-action
// redraws reuse the 2,000-row projection until authoritative state advances revision.
const chartSeriesOf = (state, revision) => {
    const history = historyOf(state);
    const key = text(state.code, text(state.symbol, "")) + "|" + periodOf(state) + "|" + revision + "|" + history.length;
    if (seriesCache && seriesCache.key === key) return seriesCache.rows;
    seriesCache = { key: key, rows: chartRowsOf(history) };
    return seriesCache.rows;
};
const chartSampleOf = (state, revision, maxPoints, intraday) => {
    const rows = chartSeriesOf(state, revision);
    const key = seriesCache.key + "|" + maxPoints + "|" + (intraday ? "i" : "k");
    if (sampleCache && sampleCache.key === key) return sampleCache;
    const points = downsampleRows(rows, maxPoints);
    sampleCache = { key: key, points: points, averageValues: movingAverages(points, intraday) };
    return sampleCache;
};
const text = (value, fallback) => typeof value === "string" && value.trim()
    ? value.slice(0, MAX_UI_TEXT_CHARS)
    : fallback;
const boundedText = (value, fallback, maxChars = MAX_UI_TEXT_CHARS) => {
    const normalized = text(value, fallback);
    return normalized.length > maxChars ? normalized.slice(0, maxChars) : normalized;
};
const formatNumber = (value, digits) => {
    const number = asNumber(value);
    return number === null ? "--" : number.toLocaleString("en-US", {
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
    if (Math.abs(number) >= 1000000000) return formatNumber(number / 1000000000, 2) + "B";
    if (Math.abs(number) >= 1000000) return formatNumber(number / 1000000, 2) + "M";
    if (Math.abs(number) >= 1000) return formatNumber(number / 1000, 2) + "K";
    return formatNumber(number, 0);
};
const formatTimestamp = (value) => {
    if (typeof value !== "string" || !value.trim()) return "Time unavailable";
    const normalized = value.slice(0, MAX_UI_TEXT_CHARS);
    const parsed = new Date(normalized);
    return Number.isNaN(parsed.getTime())
        ? normalized
        : parsed.toLocaleString("en-US", { hour12: false });
};
const formatClock = (value) => {
    if (typeof value !== "string" || !value.trim()) return "--:--:--";
    const parsed = new Date(value.slice(0, MAX_UI_TEXT_CHARS));
    return Number.isNaN(parsed.getTime())
        ? "--:--:--"
        : parsed.toLocaleTimeString("en-US", { hour12: false });
};
const normalizeCode = (value) => {
    const input = String(value || "").slice(0, 64).trim().toUpperCase().replace(/\s+/g, "");
    let match = input.match(/^(SH|SZ|BJ)[:._-]?(\d{6})$/);
    if (match) return match[1] + match[2];
    match = input.match(/^(\d{6})[:._-]?(SH|SZ|BJ)$/);
    if (match) return match[2] + match[1];
    if (/^\d{6}$/.test(input)) {
        const market = /^[48]/.test(input) ? "BJ" : /^[569]/.test(input) ? "SH" : "SZ";
        return market + input;
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
// A-share and Hong Kong markets render gains red and losses green; US and other overseas markets render gains green and losses red.
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
