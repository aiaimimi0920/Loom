// Bounded canvas chart rendering and geometry capture.
const drawChart = () => {
    if (!refs.chart || !snapshotValue) return;
    if (viewOf(snapshotValue) === "trade-price" || viewOf(snapshotValue) === "favorites-summary") {
        chartGeometry = null;
        hideChartTip();
        return;
    }
    const canvas = refs.chart;
    const bounds = canvas.getBoundingClientRect();
    const width = Math.min(MAX_CANVAS_WIDTH, Math.max(260, Math.floor(bounds.width || 520)));
    const height = Math.min(MAX_CANVAS_HEIGHT, Math.max(145, Math.floor(bounds.height || 180)));
    const deviceRatio = Math.min(2, Math.max(1, globalThis.devicePixelRatio || 1));
    const pixelRatio = Math.sqrt(MAX_CANVAS_PIXELS / Math.max(1, width * height));
    const ratio = Math.max(1, Math.min(deviceRatio, pixelRatio));
    // 给 canvas.width/height 赋值会重新分配后备位图，即使赋的是同一个数字。两块画布在
    // 上限尺寸下各约 16 MB，而重绘每秒至少一次，还会被 resize 和悬浮额外触发。只在尺寸真的
    // 变化时赋值；尺寸不变时下面的 clearRect + fillRect 依旧把整块画布清干净。
    const nextWidth = Math.floor(width * ratio);
    const nextHeight = Math.floor(height * ratio);
    if (canvas.width !== nextWidth) canvas.width = nextWidth;
    if (canvas.height !== nextHeight) canvas.height = nextHeight;
    if (refs.overlay) {
        if (refs.overlay.width !== nextWidth) refs.overlay.width = nextWidth;
        if (refs.overlay.height !== nextHeight) refs.overlay.height = nextHeight;
    }
    const context = canvas.getContext("2d");
    if (!context) return;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.fillStyle = COLORS.surface;
    context.fillRect(0, 0, width, height);

    const left = 10;
    const right = width - 10;
    const top = 30;
    const volumeTop = Math.floor(height * 0.76);
    const bottom = height - 10;
    context.strokeStyle = COLORS.grid;
    context.lineWidth = 1;
    for (let index = 0; index <= 4; index += 1) {
        const y = top + ((volumeTop - top) * index / 4);
        context.beginPath();
        context.moveTo(left, y + 0.5);
        context.lineTo(right, y + 0.5);
        context.stroke();
    }
    for (let index = 0; index <= 4; index += 1) {
        const x = left + ((right - left) * index / 4);
        context.beginPath();
        context.moveTo(x + 0.5, top);
        context.lineTo(x + 0.5, bottom);
        context.stroke();
    }

    const state = stateOf(snapshotValue);
    const period = periodOf(state);
    const periodLabel = periodLabelOf(state);
    const intraday = isIntradayPeriod(period);
    const palette = paletteFor(marketOf(state, text(state.code, text(state.symbol, "SZ000034"))));
    const maxPoints = Math.min(240, Math.max(48, Math.floor((right - left) / (intraday ? 2 : 3))));
    const sample = chartSampleOf(state, revisionOf(snapshotValue), maxPoints, intraday);
    const points = sample.points;
    if (points.length < 2) {
        chartGeometry = null;
        hideChartTip();
        context.fillStyle = COLORS.muted;
        context.font = "11px Segoe UI, Microsoft YaHei, sans-serif";
        context.textAlign = "center";
        context.fillText("等待 " + periodLabel + " 行情", width / 2, top + (volumeTop - top) / 2);
        return;
    }

    let minimum = Infinity;
    let maximum = -Infinity;
    let maxVolume = 1;
    for (const point of points) {
        minimum = Math.min(minimum, point.low);
        maximum = Math.max(maximum, point.high);
        maxVolume = Math.max(maxVolume, point.volume || 0);
    }
    if (minimum === maximum) {
        minimum -= 0.01;
        maximum += 0.01;
    }
    const padding = Math.max((maximum - minimum) * 0.08, 0.01);
    minimum -= padding;
    maximum += padding;
    const slotWidth = (right - left) / points.length;
    const xAt = (index) => left + slotWidth * (index + 0.5);
    const yAt = (value) => top + (maximum - value) * (volumeTop - top - 4) / (maximum - minimum);
    const barWidth = Math.max(1, Math.min(12, slotWidth * 0.58));

    points.forEach((point, index) => {
        const color = point.close >= point.open ? palette.up : palette.down;
        const x = xAt(index);
        const barHeight = ((point.volume || 0) / maxVolume) * Math.max(8, bottom - volumeTop - 3);
        context.globalAlpha = 0.34;
        context.fillStyle = color;
        context.fillRect(x - barWidth / 2, bottom - barHeight, barWidth, barHeight);
        context.globalAlpha = 1;
        if (!intraday) {
            context.strokeStyle = color;
            context.lineWidth = 1;
            context.beginPath();
            context.moveTo(x + 0.5, yAt(point.high));
            context.lineTo(x + 0.5, yAt(point.low));
            context.stroke();
            const bodyTop = Math.min(yAt(point.open), yAt(point.close));
            const bodyHeight = Math.max(1, Math.abs(yAt(point.open) - yAt(point.close)));
            context.fillStyle = color;
            context.fillRect(x - barWidth / 2, bodyTop, barWidth, bodyHeight);
        }
    });

    context.beginPath();
    points.forEach((point, index) => {
        const x = xAt(index);
        const y = yAt(point.close);
        if (index === 0) context.moveTo(x, y);
        else context.lineTo(x, y);
    });
    context.strokeStyle = COLORS.blue;
    context.globalAlpha = intraday ? 1 : 0.82;
    context.lineWidth = intraday ? 1.8 : 1.15;
    context.lineJoin = "round";
    context.lineCap = "round";
    context.stroke();
    context.globalAlpha = 1;

    const averageValues = sample.averageValues;
    context.beginPath();
    let started = false;
    averageValues.forEach((value, index) => {
        if (value === null) return;
        const x = xAt(index);
        const y = yAt(value);
        if (started) context.lineTo(x, y);
        else {
            context.moveTo(x, y);
            started = true;
        }
    });
    context.strokeStyle = COLORS.yellow;
    context.lineWidth = 1.25;
    context.lineJoin = "round";
    context.lineCap = "round";
    if (started) context.stroke();

    const compactDate = (value) => {
        const date = String(value || "").replace("T", " ");
        if (intraday && date.length >= 16) return date.slice(5, 16);
        return date.slice(0, 10);
    };
    context.font = "10px Consolas, monospace";
    context.fillStyle = COLORS.muted;
    context.textAlign = "left";
    context.fillText(maximum.toFixed(2), left + 2, top + 10);
    context.fillText(minimum.toFixed(2), left + 2, volumeTop - 5);
    context.fillText(compactDate(points[0].date), left + 2, bottom - 2);
    context.textAlign = "right";
    context.fillText(compactDate(points[points.length - 1].date), right - 2, bottom - 2);

    chartGeometry = {
        points: points,
        averageValues: averageValues,
        palette: palette,
        intraday: intraday,
        periodLabel: periodLabel,
        ratio: ratio,
        width: width,
        height: height,
        left: left,
        right: right,
        top: top,
        volumeTop: volumeTop,
        bottom: bottom,
        slotWidth: slotWidth,
        xAt: xAt,
        yAt: yAt
    };
    if (hoverIndex >= 0) applyHover();
    else clearOverlay();
};
