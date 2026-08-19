# Phase 76: 股票 Surface 悬浮读数、市场涨跌配色与准实时盯盘

日期：2026-08-19

## 状态

三项用户需求已在 Loom 侧完成并被测试锁定：

1. K 线/分时图悬浮显示该点位的完整读数（十字线 + tooltip）；
2. 按市场区分涨跌配色，A 股与港股红涨绿跌，美股等外盘绿涨红跌；
3. 准实时盯盘通道：`stock-api` 新增雪球十档盘口与盘中实时 tape 工具，Art 以
   1 秒起的快速 tick 轮询报价与盘口，Surface 新增盘口面板。

本阶段没有实现服务端主动推送。雪球 web 端行情没有 websocket 通道，实测只有 HTTP
轮询接口；因此这一轮交付的是"准实时"（客户端定时拉取），真正的
daemon -> Surface 推送通道留待后续阶段，与用户"两步都做，本轮先出准实时"的选择一致。

## 1. 仓库边界

本阶段改动全部在 `Loom/` 内，Hook 源码未改动：

```text
art-packages/samples/stock-monitor/
mcp-server-packages/stock-api/
scripts/Build-LoomMcpServerPackages.ps1
scripts/verify-release.ps1
scripts/tests/
apps/daemon/src/lib.rs
```

`apps/daemon/src/lib.rs` 的改动是上一阶段遗留在工作树中的 Surface attachment
清理逻辑，本阶段一并纳入构建与门禁，未新增 daemon 侧股票特判。

## 2. 悬浮读数

`art-packages/samples/stock-monitor/surface/main.js` 增加：

- `indexAtPointer`：把指针坐标映射到最近的一根数据点，使用与绘制同一份
  `chartGeometry`，不做二次插值；
- `drawCrosshair`：在 Canvas 上绘制垂直十字线与该点位标记；
- `chart-tip`：绝对定位的 tooltip，显示时间、开、高、低、收、涨跌、涨跌幅、
  成交量以及均价/MA5（按周期可用性显示）；
- 指针移动经 `requestAnimationFrame` 合流（`hoverFrame`/`hoverPointer`），
  离开图表时清空 `hoverIndex` 并恢复常规渲染。

tooltip 数值走与主面板同一套 `formatNumber`/`formatVolume`/`formatSigned`
格式化函数，因此悬浮读数与顶部指标不会出现口径不一致。

## 3. 市场涨跌配色

中国市场习惯与欧美相反，这是正确性要求而不是主题偏好：

```javascript
const RED_UP_MARKETS = Object.freeze(["SH", "SZ", "BJ", "HK"]);
const paletteFor = (market) => RED_UP_MARKETS.includes(String(market || "").toUpperCase())
    ? { up: COLORS.red, down: COLORS.green, redUp: true }
    : { up: COLORS.green, down: COLORS.red, redUp: false };
```

`paletteFor` 由报价的 `market` 字段驱动，一处解析、全局复用：

- 顶部价格、涨跌额、涨跌幅；
- K 线实体与影线、分时线与填充；
- 悬浮 tooltip 的涨跌行；
- 盘口每一档价格相对昨收的着色；
- 盘口买卖力量条的两段颜色。

`Test-LoomStockMonitorArt.ps1` 用正则锁定 `RED_UP_MARKETS` 常量与
`renderBookSide(refs.bids, ..., palette)` 调用，防止后续改动把配色退回单一主题。

## 4. 雪球接口探测结论

按用户要求实测了雪球行情通道，结论直接决定了本阶段设计：

| 端点 | 匿名可用 | 说明 |
| --- | --- | --- |
| `stock.xueqiu.com/v5/stock/realtime/quotec.json` | 是 | 盘中实时价、均价、成交量额、换手、振幅、市值 |
| `stock.xueqiu.com/v5/stock/realtime/pankou.json` | 是 | A 股十档盘口、买卖比、委差、量比 |
| 其他 v5 行情端点 | 否 | 需要 `xq_a_token` cookie |
| websocket 行情推送 | 不存在 | 雪球 web 端为 HTTP 轮询 |

`quotec.json` 不返回股票名称，且对部分港股代码没有数据，因此雪球不能替换
东方财富成为报价来源。本阶段采用叠加式设计：报价与 K 线仍走既有
`get_stock`/`get_market_series`（eastmoney），雪球只提供盘口与盘中 tape。

## 5. `stock-api` 2.8.0

wrapper 版本 2.8.0，vendored 上游仍固定 `stock-api@2.7.3`，未升级上游、未改动
vendor tree digest 与 Node runtime 固定项。

新增工具：

```text
get_order_book(code, source="xueqiu")
  -> response.orderBook  十档买卖盘、买卖百分比、委差、量比、档位数、来源、观测时间
  -> response.realtime   现价、均价、成交量、成交额、换手率、振幅、总市值、交易状态
```

实现要点：

- `additionalProperties: false` 且 `source` 为 `enum ["xueqiu"]`，
  runtime-host 的 `normalize_arguments` 会自动丢弃 Art 共享绑定里多余的 `period`；
- 盘口与 tape 两个请求用 `Promise.allSettled` 并发，任一侧成功即可返回；
- 单侧失败时回落到 `orderBookCache` 的上一次成功结果；
- 两侧都失败才抛错，由 MCP 层作为 `isError` 结果返回；
- 请求带雪球 `Referer`，仍复用既有的超时、重试与响应字节上限。

`mcp.server.json` 工具数由 5 增至 7（此前已加入 `get_market_series`），
`tools/list` 与 package contract 同步更新。

## 6. 股票 Art 1.4.0

`neuro.official/custom-stock-monitor` 1.4.0，精确依赖
`neuro.official/stock-api =2.8.0`。

MCP 调用与 Surface action 边界：

```text
calls:
  quote      get_stock(code, source="eastmoney")
  history    get_market_series(code, source="eastmoney", period, count, adjust)
  orderbook  get_order_book(code, source="xueqiu")

surfaceActions:
  stock_refresh          全部三个调用
  stock_symbol_commit    全部三个调用
  stock_period_commit    history
  stock_interval_commit  无（纯本地）
  stock_tick_refresh     quote + orderbook
```

`stock_tick_refresh` 是新增的轻量 action：只取报价与盘口，复用
`authoritativeState` 中已有的 K 线，因此高频轮询不会重复拉取历史序列。

Surface 定时器分两层：

- `refreshTimer` 按用户选择的 tick 间隔触发 `stock_tick_refresh`，
  可选 1/3/5/15/30/60/120/300 秒，默认 5 秒；
- `fullRefreshTimer` 每 60 秒触发一次完整 `stock_refresh`；
- 判定为非交易时段时最小间隔收敛到 30 秒，避免收盘后无意义高频轮询。

Art runtime `runtime/main.ps1` 新增 `ConvertTo-OrderBook`、
`ConvertTo-OrderBookLevels`、`ConvertTo-LiveTape`，上限 10 档
（`$script:MaxOrderBookLevels = 10`）。盘口调用被当作可选增强：
`Try-Get-McpToolContent -CallId "orderbook"` 失败时保留上一次同代码的盘口状态，
tick 仍然返回 `ready` 与刷新后的报价，不会因为盘口不可用而中断盯盘。

## 7. Surface 盘口面板

`.book-board` 为新增的第六个 shell 行，包含：

- 标题（`N 档盘口`）与 meta 行（买/卖百分比、委差、量比、来源、观测时钟）；
- 买卖力量双色条，颜色取自当前市场 palette；
- 买盘在左、卖盘在右的两列档位，每档显示价格、委托量，`title` 追加笔数；
- tape 行：均价、成交量、成交额、换手、振幅、总市值。

降级行为：

- `orderBook` 与 `liveTape` 都缺失时整个面板隐藏，不留空壳；
- 只有 tape 没有盘口（港股、美股）时，标题变为"盘中实时"，
  meta 追加"该市场不提供十档盘口"，仅展示 tape 行。

## 8. 本阶段验证

| Gate | 结果 |
| --- | --- |
| `Test-LoomStockApiMcpServer.ps1` | passed：`version=2.8.0 tools=7 order-book=2-levels tape=xueqiu` |
| `Test-LoomStockMonitorArt.ps1` | passed：`wrapper=2.8.0 upstream=2.7.3 source=eastmoney+xueqiu periods=13 candles=3 tick=1s order-book=2-levels red-up=CN/HK no-trading=true` |
| `Test-LoomMcpServerPackageContract.ps1` | passed：`packages=2 stock-api=2.8.0 upstream=2.7.3` |
| `Test-LoomSampleArtPackageContract.ps1` | passed：7 packages |
| `Test-LoomSampleArtInstallExecution.ps1` | passed：7 Arts / 2 MCP packages |
| `Smoke-LoomStockApiLive.mjs`（真实 provider） | passed：`orderBook levels=5 bestBid=25.53 bestAsk=25.54 buyPercent=49.24`，`liveTape avgPrice=25.199` |

新增的确定性用例：

- loopback fixture 增加 `pankou.json` 与 `quotec.json` 分支，
  接受请求数由 19 增至 21，并断言最后两次请求确实打到雪球两个端点；
- `get_order_book` 断言 2 档买卖盘、最优买 25.53/152340、最优卖 25.54、
  买盘 49.24%、委差 -11455、来源 `xueqiu`，以及 tape 的现价、涨跌幅、均价、换手；
- 盘口失败降级用例 `-OrderBookError`：tick 仍为 `ready`、报价仍然刷新、
  `orderBook.levels` 保留上一次深度。

`Smoke-LoomStockApiLive.mjs` 走真实雪球与东方财富服务，属于开发期取证脚本，
不是发布门禁，未纳入 `verify-release.ps1`，以免把外部行情服务波动当成发布判据。

## 9. 正式发布

### 9.1 Loom R63

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260819-stock-orderbook-live-r63
```

| Item | Value |
| --- | --- |
| source HEAD | `80e2623668d2c4d65812ad11eefa887a0c3bd3e6` |
| `gitDirty` / `sourceGitDirty` | `true` / `true` |
| Desktop ZIP | 94451498 bytes；`38ab389e87d5426a…` |
| `stock-api.zip` | 33496306 bytes；`336f5aff99266ab20e6dc283035b987bf2be233bf4df0fd2b78833b5bd06da71` |
| `custom-stock-monitor.zip` | 27173 bytes；`fc99d2f96208840bd6ffcfb3d87cc3e2e3dad33bcf0eb9d1b72c4a9cfd54503d` |
| package catalog | 4 Frameworks / 2 MCP / 7 Arts |
| checksum entries | 57 |

`verify-release.ps1 -RunSmoke` 结果：

```text
smoke=passed
hookCanvasSmoke=passed
hookErrorPreviewSmoke=passed
frameworkArtStoreHookSmoke=passed
pluginBoundarySmoke=passed
surfacePrototypeSmoke=passed
authoredArtCreationSmoke=passed
```

verifier 原先固定 Stock Monitor 只有 2 个 MCP 调用，本阶段同步为 3 个，并新增
order-book 调用与 `source=xueqiu` 的发布级断言。

### 9.2 Hook R71

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\20260819-stock-orderbook-live-r71\hook.exe
```

| Item | Value |
| --- | --- |
| source HEAD | `1bcf6c1cf0c7b319508969dd55c6ae4d5c8d7660` |
| bytes | `7057408` |
| SHA-256 | `ce714dcb452c33768ecb7159bdef9a9d7db388a9af1b655f87d216b994b73146` |
| `--version` | `hook 0.1.7` |
| `--self-check` | exit 0，`status=ok` |

Hook 源码本阶段未改动，R71 是与 Loom R63 配套重建的当前工作树构建证据，
不是 clean-source 签名发布件，也未做 Authenticode 签名。

## 10. 已知边界

1. 本阶段是准实时轮询，不是服务端推送；雪球 web 端没有 websocket 行情通道。
2. 雪球 `pankou.json` 的十档只对 A 股完整；港股、美股仅有盘中 tape。
3. 雪球匿名端点没有官方 SLA，限流或字段调整属于外部运行条件；盘口被设计为可选
   增强，失败只影响盘口面板，不影响报价与 K 线。
4. 1 秒 tick 是用户可选上限，非默认值；默认 5 秒，非交易时段最小 30 秒。
5. 仍然只做行情展示，不提供交易、委托、账户或投资建议功能。
6. `.loom-art-store-data/mcp-servers/summary.json` 等 store 数据由构建脚本生成，
   不手工编辑。

## 11. 后续阶段（推送通道）

真正的推送通道需要三处协同，本阶段未实施：

```text
crates/loom_mcp        长连接 MCP 会话 + server -> client notifications
apps/daemon            常驻 tick 源，主动推送 Surface revision
Hook / Surface 客户端  复用 loom.hook.subscribe 接收 daemon 推送
```

## 12. 主要实现位置

```text
art-packages/samples/stock-monitor/manifest.json
art-packages/samples/stock-monitor/runtime/main.ps1
art-packages/samples/stock-monitor/surface/main.js
art-packages/samples/stock-monitor/surface/fallback.json
mcp-server-packages/stock-api/mcp.server.json
mcp-server-packages/stock-api/runtime/stock-api-entry.js
scripts/Build-LoomMcpServerPackages.ps1
scripts/verify-release.ps1
scripts/tests/Test-LoomStockApiMcpServer.ps1
scripts/tests/Test-LoomStockMonitorArt.ps1
scripts/tests/Test-LoomMcpServerPackageContract.ps1
scripts/tests/Test-LoomSampleArtPackageContract.ps1
scripts/tests/Test-LoomSampleArtInstallExecution.ps1
scripts/tests/fixtures/StockMonitorApiFixture.ps1
scripts/tests/Smoke-LoomStockApiLive.mjs
```
