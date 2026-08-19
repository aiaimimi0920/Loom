# Phase 77: Stock API pysnowball adapter and live freshness

Date: 2026-08-19

## Outcome

This phase keeps the existing aggregate market path and adds a bounded
pysnowball-compatible live provider inside the existing `stock-api` MCP
wrapper. A separate Python MCP server was intentionally not introduced:
Loom's framework execution context has one MCP server per Art request, and the
release builder does not ship a Python runtime. The adapter therefore mirrors
the upstream REST contract in Node and never executes Python code.

## Provider boundary

`get_stock` and `get_market_series` remain the existing Eastmoney/Sina/Tencent
aggregation path. `get_order_book` accepts `auto`, `xueqiu`, or `pysnowball`:

- `auto` prefers pysnowball's anonymous `quotec` for the realtime tape;
  credentialed `pankou` is used when `LOOM_PYSNOWBALL_TOKEN` is configured;
  the existing Xueqiu-compatible request remains the fallback.
- `pysnowball` uses the upstream v0.1.8 endpoint and Cookie semantics. The
  anonymous tape remains available without a token; depth reports the explicit
  credential requirement when no Cookie is configured.
- `xueqiu` preserves the previous anonymous request path.

The wrapper reports provider metadata without returning credential values. The
vendored metadata pins pysnowball 0.1.8 at commit
`e85fe550c5daed4ad1429d1f4e048dab239df921`, Apache-2.0, and records that only
an API-compatible Node adapter is shipped.

## Freshness and failure behavior

Provider responses are capped at 5 MiB before JSON parsing. Successful quote,
series, and live responses carry `fetchedAt`, cache TTL, cache age, and `stale`.
Expired process-local cache entries are discarded rather than reused.

Stock Monitor runtime behavior is now explicit:

- A tick requests only `orderbook`; the authoritative quote and selected
  history remain available from the Surface snapshot.
- A fresh live tape replaces the top price only during an open session.
- A closed session displays the latest trading-day history close, even if a
  provider's current quote is newer or a live tape is stale.
- Cached orderbook/tape is reused only within the bounded age window.
- `observedAt` describes market data time; `fetchedAt` and `lastUpdatedAt`
  describe the local retrieval time.

## Surface behavior

The Surface VM contract locks the following invariants:

- an unchanged snapshot revision does not clear the tick lock;
- a newer revision clears it;
- open and closed refresh cadence follows the configured interval and closed
  market floor;
- Chinese markets render red-up/green-down, while US markets render
  green-up/red-down.

## Verification

The deterministic gates passed after the changes:

- `Test-LoomStockMonitorSurface.mjs`
- `Test-LoomStockApiMcpServer.ps1`
- `Test-LoomStockMonitorArt.ps1`
- `Test-LoomMcpServerPackageContract.ps1`
- `Test-LoomSampleArtPackageContract.ps1`

The external-network smoke also passed for `SZ000034`: the existing aggregate
quote path resolved through Eastmoney, while `auto` selected the anonymous
pysnowball-compatible realtime tape and retained the Xueqiu-compatible depth
fallback without a configured token.

The final paired artifacts are:

- Loom `20260819-stock-pysnowball-freshness-r64`; its 57-file manifest,
  checksums, standalone runtime, Hook canvas/error preview, framework Art
  store, plugin boundary, Surface prototype, and authored Art smoke all pass.
- Hook `20260819-stock-pysnowball-freshness-r72`; the focused Surface contract
  suite passes 23 tests, and the release EXE/portable ZIP both report 0.1.7.

The release verifier was updated to require the new `auto` order-book routing
contract instead of the previous hard-coded `xueqiu` value. Phase 77 introduced
no additional Hook source edits; the paired Hook artifact was built from its
existing dirty worktree.
