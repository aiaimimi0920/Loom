// Chart overlay, pointer interaction, tooltip and legend rendering.
const overlayContext = () => {
    if (!refs.overlay) return null;
    const context = refs.overlay.getContext("2d");
    if (!context) return null;
    const ratio = chartGeometry ? chartGeometry.ratio : 1;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, refs.overlay.width / ratio, refs.overlay.height / ratio);
    return context;
};

const clearOverlay = () => {
    overlayContext();
};

const hideChartTip = () => {
    hoverIndex = -1;
    hoverPointer = null;
    if (refs.tip) {
        refs.tip.classList.remove("is-visible");
        refs.tip.setAttribute("aria-hidden", "true");
    }
    clearOverlay();
};

const drawCrosshair = (index) => {
    const geometry = chartGeometry;
    const context = overlayContext();
    if (!context || !geometry) return;
    const point = geometry.points[index];
    if (!point) return;
    const x = geometry.xAt(index);
    const y = geometry.yAt(point.close);
    context.save();
    context.strokeStyle = "rgba(217,255,56,0.5)";
    context.lineWidth = 1;
    context.setLineDash([3, 3]);
    context.beginPath();
    context.moveTo(Math.round(x) + 0.5, geometry.top);
    context.lineTo(Math.round(x) + 0.5, geometry.bottom);
    context.stroke();
    context.beginPath();
    context.moveTo(geometry.left, Math.round(y) + 0.5);
    context.lineTo(geometry.right, Math.round(y) + 0.5);
    context.stroke();
    context.setLineDash([]);
    context.beginPath();
    context.arc(x, y, 3.2, 0, Math.PI * 2);
    context.fillStyle = COLORS.yellow;
    context.fill();
    context.lineWidth = 1.3;
    context.strokeStyle = COLORS.background;
    context.stroke();
    context.restore();
};

// 提示框用 DOM 构建，不拼 innerHTML：日期这类字段直接来自另一个进程的 JSON，拼进
// 标记里就会把状态内容当 HTML 执行。颜色只来自本地 COLORS/paletteFor 的封闭集合。
const tipRow = (key, value, color) => {
    const row = document.createElement("div");
    row.className = "chart-tip-row";
    const keyNode = document.createElement("span");
    keyNode.className = "chart-tip-key";
    keyNode.textContent = key;
    const valueNode = document.createElement("span");
    valueNode.className = "chart-tip-value";
    valueNode.textContent = value;
    if (color) valueNode.style.color = color;
    row.appendChild(keyNode);
    row.appendChild(valueNode);
    return row;
};

const buildTipContent = (geometry, index) => {
    const point = geometry.points[index];
    const previous = index > 0 ? geometry.points[index - 1] : null;
    const reference = previous ? previous.close : point.open;
    const change = reference ? point.close - reference : null;
    const changePercent = reference ? ((point.close - reference) / reference) * 100 : null;
    const changeColor = deltaColor(change, geometry.palette);
    const averageValue = Array.isArray(geometry.averageValues) ? geometry.averageValues[index] : null;
    const fragment = document.createDocumentFragment();
    const title = document.createElement("div");
    title.className = "chart-tip-title";
    title.textContent = formatPointDate(point.date, geometry.intraday);
    fragment.appendChild(title);
    if (geometry.intraday) {
        fragment.appendChild(tipRow("价格", formatNumber(point.close, 2), changeColor));
        if (averageValue !== null && averageValue !== undefined) fragment.appendChild(tipRow("均价", formatNumber(averageValue, 2), COLORS.yellow));
    }
    else {
        fragment.appendChild(tipRow("开", formatNumber(point.open, 2)));
        fragment.appendChild(tipRow("高", formatNumber(point.high, 2), deltaColor(point.high - reference, geometry.palette)));
        fragment.appendChild(tipRow("低", formatNumber(point.low, 2), deltaColor(point.low - reference, geometry.palette)));
        fragment.appendChild(tipRow("收", formatNumber(point.close, 2), changeColor));
        if (averageValue !== null && averageValue !== undefined) fragment.appendChild(tipRow("MA5", formatNumber(averageValue, 2), COLORS.yellow));
    }
    fragment.appendChild(tipRow("涨跌", change === null ? "--" : formatSigned(change, ""), changeColor));
    fragment.appendChild(tipRow("涨幅", changePercent === null ? "--" : formatSigned(changePercent, "%"), changeColor));
    fragment.appendChild(tipRow("成交量", formatVolume(point.volume)));
    return fragment;
};

const positionTip = (pointerX, pointerY) => {
    const tip = refs.tip;
    const canvas = refs.chart;
    if (!tip || !canvas) return;
    const boundsWidth = canvas.clientWidth || (chartGeometry ? chartGeometry.width : 0);
    const boundsHeight = canvas.clientHeight || (chartGeometry ? chartGeometry.height : 0);
    const tipWidth = tip.offsetWidth;
    const tipHeight = tip.offsetHeight;
    let leftPosition = pointerX + 14;
    if (leftPosition + tipWidth > boundsWidth - 4) leftPosition = pointerX - tipWidth - 14;
    if (leftPosition < 4) leftPosition = 4;
    let topPosition = pointerY - tipHeight - 10;
    if (topPosition < 4) topPosition = pointerY + 16;
    if (topPosition + tipHeight > boundsHeight - 4) topPosition = Math.max(4, boundsHeight - tipHeight - 4);
    tip.style.left = Math.round(leftPosition) + "px";
    tip.style.top = Math.round(topPosition) + "px";
};

const indexAtPointer = (offsetX) => {
    const geometry = chartGeometry;
    if (!geometry || !geometry.points.length) return -1;
    const clamped = Math.max(geometry.left, Math.min(geometry.right, offsetX));
    const index = Math.floor((clamped - geometry.left) / geometry.slotWidth);
    return Math.max(0, Math.min(geometry.points.length - 1, index));
};

const applyHover = () => {
    const geometry = chartGeometry;
    if (!geometry || !refs.tip) {
        hideChartTip();
        return;
    }
    const pointer = hoverPointer;
    const index = pointer ? indexAtPointer(pointer.x) : Math.min(hoverIndex, geometry.points.length - 1);
    if (index < 0) {
        hideChartTip();
        return;
    }
    hoverIndex = index;
    drawCrosshair(index);
    refs.tip.replaceChildren(buildTipContent(geometry, index));
    refs.tip.classList.add("is-visible");
    refs.tip.setAttribute("aria-hidden", "false");
    const anchorX = pointer ? pointer.x : geometry.xAt(index);
    const anchorY = pointer ? pointer.y : geometry.yAt(geometry.points[index].close);
    positionTip(anchorX, anchorY);
};

const handleChartPointer = (event) => {
    if (!chartGeometry || !refs.chart) return;
    const bounds = refs.chart.getBoundingClientRect();
    if (!bounds.width || !bounds.height) return;
    hoverPointer = { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
    if (hoverFrame !== null) return;
    hoverFrame = requestAnimationFrame(() => {
        hoverFrame = null;
        if (disposed || !hoverPointer) return;
        applyHover();
    });
};

// 图例同样用 DOM 构建，避免把 periodLabel（来自状态）拼进标记。
const legendItem = (label, swatchClass, swatchBackground) => {
    const item = document.createElement("span");
    item.className = "legend-item";
    const swatch = document.createElement("i");
    swatch.className = "legend-line " + swatchClass;
    if (swatchBackground) swatch.style.background = swatchBackground;
    item.appendChild(swatch);
    item.appendChild(document.createTextNode(label));
    return item;
};
