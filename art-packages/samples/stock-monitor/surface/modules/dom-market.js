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
            + " · 委托量 " + volumeText
            + (orders === null ? "" : " · 笔数 " + formatVolume(orders));
    });
};
const tapeDefinitions = [
    ["均价", (tape) => formatNumber(tape.avgPrice, 2)],
    ["成交量", (tape) => formatVolume(tape.volume)],
    ["成交额", (tape) => formatVolume(tape.amount)],
    ["换手", (tape) => asNumber(tape.turnoverRate) === null ? "--" : formatNumber(tape.turnoverRate, 2) + "%"],
    ["振幅", (tape) => asNumber(tape.amplitude) === null ? "--" : formatNumber(tape.amplitude, 2) + "%"],
    ["总市值", (tape) => formatVolume(tape.marketCapital)]
];
// 标签是常量，值在 strong 里。原来把标签写进 item.textContent 再 append strong，所以
// 每帧都要重建整个 span；这里把标签放进独立文本节点，只有值需要改写。
const buildTapeItem = (index) => {
    const item = document.createElement("span");
    item.className = "tape-item";
    item.append(document.createTextNode(tapeDefinitions[index][0]), document.createElement("strong"));
    return item;
};
// 运行时已经判定过新鲜度（stale/ageSeconds/maxAgeSeconds），但界面此前只画一个时钟串，
// 陈旧记录和最新记录长得一模一样。把判定显式标出来，年龄已知时连秒数一起给。
// asNumber 不能用在这里：Number(null) 是 0，观测时间缺失的记录会被写成"已陈旧 0 秒"，
// 也就是刚刚才观测到——正好和运行时 fail-closed 的判定相反。
const asAgeSeconds = (value) => {
    if (value === null || value === undefined || value === "") return null;
    return asNumber(value);
};
const staleLabel = (record) => {
    if (!record || record.stale !== true) return "";
    const age = asAgeSeconds(record.ageSeconds);
    const limit = asAgeSeconds(record.maxAgeSeconds);
    if (age === null) return "已陈旧（观测时间不可用）";
    if (limit === null) return "已陈旧 " + formatNumber(age, 0) + " 秒";
    return "已陈旧 " + formatNumber(age, 0) + "/" + formatNumber(limit, 0) + " 秒";
};
const withStale = (base, record) => {
    const label = staleLabel(record);
    return label ? base + " · " + label : base;
};
// 报价成功但 K 线抓取失败时，运行时会送来 historyWarning：状态仍然是 ready，可图表画不
// 出来。借用页脚同一个位置显示，用黄色和 error 的红色区分非致命告警；错误优先，因为报价
// 本身都没拿到时，抱怨图表没有意义。
const footerNoticeOf = (state) => {
    const error = typeof state.error === "string" ? state.error.trim() : "";
    if (error !== "") return { text: boundedText(error, ""), warning: false };
    const historyWarning = typeof state.historyWarning === "string" ? state.historyWarning.trim() : "";
    if (historyWarning === "") return { text: "", warning: false };
    return { text: "K 线数据不可用：" + boundedText(historyWarning, ""), warning: true };
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
        writeText(refs.bookTitle, levels + " 档盘口");
        const buyPercent = asNumber(book.buyPercent);
        const sellPercent = asNumber(book.sellPercent);
        const netVolume = asNumber(book.netVolume);
        const parts = [];
        if (buyPercent !== null && sellPercent !== null) {
            parts.push("买 " + formatNumber(buyPercent, 2) + "% / 卖 " + formatNumber(sellPercent, 2) + "%");
        }
        if (netVolume !== null) parts.push("委差 " + formatSigned(netVolume, ""));
        if (asNumber(book.ratio) !== null) parts.push("量比 " + formatNumber(book.ratio, 2));
        parts.push(withStale(text(book.source, "xueqiu") + " · " + formatClock(book.observedAt), book));
        if (hasTape) {
            const tapeStale = staleLabel(tape);
            if (tapeStale) parts.push("实时逐笔" + tapeStale);
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
        renderBookSide(refs.bids, snapshot.bids, "买", previousClose, palette);
        renderBookSide(refs.asks, snapshot.asks, "卖", previousClose, palette);
    }
    else {
        writeText(refs.bookTitle, "盘中实时");
        writeText(refs.bookMeta, hasTape
            ? withStale(text(tape.source, "xueqiu") + " · " + formatClock(tape.observedAt) + " · 该市场不提供十档盘口", tape)
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
    // 逐笔派生量（均价/成交额/换手）全部来自这一条记录，记录陈旧时这些数字也陈旧。
    const tapeStaleLabel = staleLabel(tape);
    refs.tape.classList.toggle("is-stale", tapeStaleLabel !== "");
    refs.tape.title = tapeStaleLabel;
};
