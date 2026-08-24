// Reusable DOM reconciliation for metrics, history and favorites.
// 四个更新函数原来都以 replaceChildren() 开头，每帧丢掉并重建约 200 个节点。行数固定或
// 有上界，所以改成"补齐节点数量、再原地写文本"，只有真正变化的文本才落到 DOM 上。
const ensureChildren = (host, count, build) => {
    while (host.children.length > count) host.removeChild(host.lastChild);
    while (host.children.length < count) host.appendChild(build(host.children.length));
    return host.children;
};
const clearHost = (host) => {
    if (host && host.firstChild) host.replaceChildren();
};
const writeText = (node, value) => {
    if (node.textContent !== value) node.textContent = value;
};

const buildMetricCell = (index) => {
    const cell = document.createElement("div");
    cell.className = "market-cell";
    const label = document.createElement("span");
    label.className = "metric-label";
    label.textContent = metricDefinitions[index][0];
    const value = document.createElement("strong");
    value.className = "metric-value";
    cell.append(label, value);
    return cell;
};

const updateMetrics = (quote, history, state) => {
    const cells = ensureChildren(refs.metrics, metricDefinitions.length, buildMetricCell);
    metricDefinitions.forEach((definition, index) => {
        writeText(cells[index].lastElementChild, definition[1](quote, history, state));
    });
};

const buildHistoryRow = () => {
    const row = document.createElement("div");
    row.className = "history-row";
    for (let index = 0; index < HISTORY_ROW_CELLS; index += 1) row.appendChild(document.createElement("span"));
    return row;
};

const writeHistoryRow = (row, cells, className) => {
    const nextClass = "history-row" + (className ? " " + className : "");
    if (row.className !== nextClass) row.className = nextClass;
    for (let index = 0; index < HISTORY_ROW_CELLS; index += 1) {
        const cell = row.children[index];
        writeText(cell, cells[index].text);
        // style.color 读回来是 rgb() 形式，和写进去的 #rrggbb 永远不相等，所以这里不比较。
        cell.style.color = cells[index].color || "";
    }
};

const updateHistoryTable = (history, state, palette) => {
    if (!refs.historyRows) return;
    const rows = history.slice(-8).reverse();
    const hostRows = ensureChildren(refs.historyRows, HISTORY_ROW_COUNT, buildHistoryRow);
    writeHistoryRow(hostRows[0], HISTORY_HEAD_CELLS, "is-head");
    const intraday = isIntradayPeriod(periodOf(state));
    for (let index = 1; index < HISTORY_ROW_COUNT; index += 1) {
        const value = rows[index - 1];
        if (value === undefined) {
            writeHistoryRow(hostRows[index], HISTORY_EMPTY_CELLS, "is-empty");
            continue;
        }
        const row = asObject(value);
        const open = asNumber(row.open);
        const close = asNumber(row.close);
        const color = open === null || close === null ? COLORS.text : deltaColor(close - open, palette);
        writeHistoryRow(hostRows[index], [
            { text: formatPointDate(row.date, intraday) },
            { text: formatNumber(open, 2) },
            { text: formatNumber(row.high, 2) },
            { text: formatNumber(row.low, 2) },
            { text: formatNumber(close, 2), color: color },
            { text: formatVolume(row.volume) }
        ], "");
    }
    writeText(refs.historyTitle, periodLabelOf(state) + " · 最近 8 条");
    writeText(refs.historyMeta, history.length + " 条数据");
};

const buildFavoriteCard = () => {
    const card = document.createElement("article");
    card.className = "favorite-card";
    const name = document.createElement("strong");
    name.className = "favorite-name";
    const code = document.createElement("span");
    code.className = "favorite-code";
    const price = document.createElement("strong");
    price.className = "favorite-price";
    const delta = document.createElement("span");
    delta.className = "favorite-delta";
    card.append(name, code, price, delta);
    return card;
};

const updateFavorites = (state) => {
    if (!refs.favoritesGrid) return;
    const favorites = favoriteQuotesOf(state);
    if (!favorites.length) {
        const existing = refs.favoritesGrid.firstElementChild;
        if (refs.favoritesGrid.children.length === 1 && existing.classList.contains("favorites-empty")) return;
        const empty = document.createElement("div");
        empty.className = "favorites-empty";
        empty.textContent = "等待 stock-api 返回收藏股票报价";
        refs.favoritesGrid.replaceChildren(empty);
        return;
    }
    const first = refs.favoritesGrid.firstElementChild;
    // 空态节点和卡片形状不同，切回有数据时先清掉它，否则 ensureChildren 会当成卡片写。
    if (first && !first.classList.contains("favorite-card")) refs.favoritesGrid.replaceChildren();
    const cards = ensureChildren(refs.favoritesGrid, favorites.length, buildFavoriteCard);
    favorites.forEach((value, index) => {
        const quote = asObject(value);
        const market = text(quote.market, text(quote.code, "").slice(0, 2));
        const palette = paletteFor(market);
        const color = movementColor(movement(quote), palette);
        const card = cards[index];
        const name = card.children[0];
        writeText(name, text(quote.name, "未知股票"));
        name.title = name.textContent;
        writeText(card.children[1], text(quote.code, "--") + " · " + market);
        const price = card.children[2];
        writeText(price, formatNumber(quote.price, 2));
        price.style.color = color;
        const delta = card.children[3];
        writeText(delta, formatSigned(quote.change, "") + "  " + formatSigned(quote.changePercent, "%"));
        delta.style.color = color;
    });
};
