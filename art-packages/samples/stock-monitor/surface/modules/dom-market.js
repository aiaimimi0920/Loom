// Order-book, live-tape and freshness rendering.
const bookLevelsOf = (value) => Array.isArray(value)
    ? value.slice(0, MAX_BOOK_LEVELS).filter((row) => asNumber(asObject(row).price) !== null)
    : [];
const orderBookOf = (state) => {
    const book = asObject(state.orderBook);
    const bids = bookLevelsOf(book.bids);
    const asks = bookLevelsOf(book.asks);
    return bids.length || asks.length ? { book: book, bids: bids, asks: asks } : null;
};
const buildBookRow = () => {
    const line = document.createElement("div");
    line.className = "book-row";
    const label = document.createElement("span");
    label.className = "book-tag";
    const price = document.createElement("span");
    price.className = "book-price";
    const volume = document.createElement("span");
    volume.className = "book-volume";
    line.append(label, price, volume);
    return line;
};
const renderBookSide = (host, levels, tag, previousClose, palette) => {
    const rows = ensureChildren(host, levels.length, buildBookRow);
    levels.forEach((row, index) => {
        const level = asObject(row);
        const line = rows[index];
        const label = tag + (asNumber(level.level) || index + 1);
        const priceText = formatNumber(level.price, 2);
        const volumeText = formatVolume(level.volume);
        const orders = asNumber(level.orders);
        writeText(line.children[0], label);
        writeText(line.children[1], priceText);
        line.children[1].style.color = previousClose === null
            ? COLORS.text
            : deltaColor(asNumber(level.price) - previousClose, palette);
        writeText(line.children[2], volumeText);
        line.title = label + " " + priceText
            + " · Vol " + volumeText
            + (orders === null ? "" : " · Orders " + formatVolume(orders));
    });
};
const tapeDefinitions = [
    ["Avg", (tape) => formatNumber(tape.avgPrice, 2)],
    ["Volume", (tape) => formatVolume(tape.volume)],
    ["Amount", (tape) => formatVolume(tape.amount)],
    ["Turnover", (tape) => asNumber(tape.turnoverRate) === null ? "--" : formatNumber(tape.turnoverRate, 2) + "%"],
    ["Amplitude", (tape) => asNumber(tape.amplitude) === null ? "--" : formatNumber(tape.amplitude, 2) + "%"],
    ["Mkt Cap", (tape) => formatVolume(tape.marketCapital)]
];
// Keep the static label in its own text node so frame updates only mutate the <strong> value element.
const buildTapeItem = (index) => {
    const item = document.createElement("span");
    item.className = "tape-item";
    item.append(document.createTextNode(tapeDefinitions[index][0]), document.createElement("strong"));
    return item;
};
// Surface the runtime's staleness verdict alongside the clock string. Guard null/empty values before
// calling asNumber() because Number(null) is 0, which would mislabel an ageless fail-closed record as 0s old.
const asAgeSeconds = (value) => {
    if (value === null || value === undefined || value === "") return null;
    return asNumber(value);
};
const staleLabel = (record) => {
    if (!record || record.stale !== true) return "";
    const age = asAgeSeconds(record.ageSeconds);
    const limit = asAgeSeconds(record.maxAgeSeconds);
    if (age === null) return "Stale (observation time unavailable)";
    if (limit === null) return "Stale " + formatNumber(age, 0) + "s";
    return "Stale " + formatNumber(age, 0) + "/" + formatNumber(limit, 0) + "s";
};
const withStale = (base, record) => {
    const label = staleLabel(record);
    return label ? base + " · " + label : base;
};
// When quote refresh succeeds without K-line data, surface historyWarning in the footer in warning
// yellow while keeping fatal quote errors higher priority in red.
const footerNoticeOf = (state) => {
    const error = typeof state.error === "string" ? state.error.trim() : "";
    if (error !== "") return { text: boundedText(error, ""), warning: false };
    const historyWarning = typeof state.historyWarning === "string" ? state.historyWarning.trim() : "";
    if (historyWarning === "") return { text: "", warning: false };
    return { text: "K-line data unavailable: " + boundedText(historyWarning, ""), warning: true };
};
const updateOrderBook = (state, quote, palette) => {
    if (!refs.book) return;
    const snapshot = orderBookOf(state);
    const tape = asObject(state.liveTape);
    const hasTape = asNumber(tape.price) !== null;
    refs.book.classList.toggle("is-visible", Boolean(snapshot) || hasTape);
    if (!snapshot && !hasTape) {
        clearHost(refs.bids);
        clearHost(refs.asks);
        clearHost(refs.tape);
        return;
    }
    const previousClose = asNumber(quote.previousClose);
    refs.bookBar.classList.toggle("is-hidden", !snapshot);
    refs.asks.classList.toggle("is-hidden", !snapshot);
    refs.bids.classList.toggle("is-hidden", !snapshot);
    if (snapshot) {
        const book = snapshot.book;
        const levels = asNumber(book.levels) || Math.max(snapshot.bids.length, snapshot.asks.length);
        writeText(refs.bookTitle, levels + "-Level Order Book");
        const buyPercent = asNumber(book.buyPercent);
        const sellPercent = asNumber(book.sellPercent);
        const netVolume = asNumber(book.netVolume);
        const parts = [];
        if (buyPercent !== null && sellPercent !== null) {
            parts.push("Bid " + formatNumber(buyPercent, 2) + "% / Ask " + formatNumber(sellPercent, 2) + "%");
        }
        if (netVolume !== null) parts.push("Net " + formatSigned(netVolume, ""));
        if (asNumber(book.ratio) !== null) parts.push("Ratio " + formatNumber(book.ratio, 2));
        parts.push(withStale(text(book.source, "xueqiu") + " · " + formatClock(book.observedAt), book));
        if (hasTape) {
            const tapeStale = staleLabel(tape);
            if (tapeStale) parts.push("Live tape " + tapeStale);
        }
        writeText(refs.bookMeta, parts.join(" · "));
        refs.bookMeta.title = refs.bookMeta.textContent;
        refs.bookMeta.classList.toggle("is-stale", book.stale === true || (hasTape && tape.stale === true));
        const buyShare = buyPercent === null || sellPercent === null
            ? 50
            : Math.max(0, Math.min(100, buyPercent));
        refs.bookBuyBar.style.width = buyShare + "%";
        refs.bookBuyBar.style.background = palette.up;
        refs.bookSellBar.style.width = (100 - buyShare) + "%";
        refs.bookSellBar.style.background = palette.down;
        renderBookSide(refs.bids, snapshot.bids, "B", previousClose, palette);
        renderBookSide(refs.asks, snapshot.asks, "A", previousClose, palette);
    }
    else {
        writeText(refs.bookTitle, "Intraday Live");
        writeText(refs.bookMeta, hasTape
            ? withStale(text(tape.source, "xueqiu") + " · " + formatClock(tape.observedAt) + " · Ten-level depth unavailable for this market", tape)
            : "");
        refs.bookMeta.title = refs.bookMeta.textContent;
        refs.bookMeta.classList.toggle("is-stale", hasTape && tape.stale === true);
        clearHost(refs.bids);
        clearHost(refs.asks);
    }
    if (!hasTape) {
        clearHost(refs.tape);
        return;
    }
    const items = ensureChildren(refs.tape, tapeDefinitions.length, buildTapeItem);
    tapeDefinitions.forEach((definition, index) => {
        writeText(items[index].lastElementChild, definition[1](tape));
    });
    // Derived tape metrics come from the same live record, so badge them stale together.
    const tapeStaleLabel = staleLabel(tape);
    refs.tape.classList.toggle("is-stale", tapeStaleLabel !== "");
    refs.tape.title = tapeStaleLabel;
};
