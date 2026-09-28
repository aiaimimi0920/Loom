# Stock Monitor

Stock Monitor is a packaged market-data Art for A-share (`SH`, `SZ`, `BJ`), Hong
Kong (`HK`) and US (`US`) stock codes. It presents quotes, charts, order-book depth,
live tape and favorites through the ordinary Art Surface contract. It exposes
no trading or order-placement operation. Data may be delayed; the output is
informational and is not an investment recommendation or trading instruction.

## Packages and runtime

The Art source is `art-packages/samples/stock-monitor`; its MCP dependency is
`mcp-server-packages/stock-api`. Hook remains a general Surface renderer and does
not need stock-specific IDs or provider logic.

The Art currently pins `neuro.official/stock-api` to `=2.9.0`. That MCP wrapper
vendors upstream npm `stock-api` 2.7.3 and a Node implementation of the pysnowball
0.1.8 REST contract. The MCP package carries Node 22.22.2 for Windows x64; the Art
entry is PowerShell with a one-process limit. No Python adapter process, runtime
`npx`, `npm install` or global Node installation is required.

Upstream source identity, license and digests are owned by `runtime/UPSTREAM.json`,
`runtime/PYSNOWBALL.json` and `runtime/node-runtime.json` in the MCP package.
Package construction checks the pinned inputs; update those records, contracts
and SBOM evidence together when changing a dependency.

## Data sources and credentials

The Art requests aggregate quote/history/favorite data with the `eastmoney`
source selection and live data with `auto`. Live source choices are `auto`,
`xueqiu` and `pysnowball`. In `auto`, tape prefers the anonymous pysnowball-compatible
quote endpoint; depth uses pysnowball when a token exists and Xueqiu otherwise.
A failed pysnowball request falls back to Xueqiu. An explicitly selected
pysnowball depth request requires credentials and reports their absence.

Optional credential `pysnowball_token` is injected as `LOOM_PYSNOWBALL_TOKEN`.
Credential values must not appear in provider metadata, Surface state or formal
outputs. The Art does not require an API key for its default path; this does not
guarantee that an external service permits anonymous access at every moment.

## Refresh and freshness

Default code is `SZ000034`; refresh defaults to 5 seconds with options of 1, 3,
5, 15, 30, 60, 120 and 300 seconds. This is HTTP polling, not server push.
Fast ticks request live data without reloading full history; a single timer
periodically upgrades a tick to a full snapshot refresh on an approximately
60-second cycle. Pending work is not overlapped. An unchanged snapshot revision
does not release the tick lock; a newer revision does.

Closed-market polling has a 30-second minimum. If the host rejects the tick
channel, full refresh uses a minimum 60-second cadence and the channel is probed
again after 5 minutes. Market status uses market timezone, weekday/session rules
and latest trading-day data, not a complete exchange holiday calendar. A fresh
trading tick can indicate an open market. When closed, the primary quote prefers
the latest historical close; the UI does not imply new trades occurred.

`observedAt` is market-data time; `fetchedAt` and `lastUpdatedAt` are retrieval
times. Open-market quote/tape freshness is bounded to 90 seconds and depth to
120 seconds. Missing or timezone-ambiguous observation times are stale. A closed
market's last close is not stale merely because its observation is old.

The MCP wrapper caps provider responses at 5 MiB before parsing and retains at
most 64 successful cache entries: quote TTL 120 seconds, series TTL 15 minutes,
depth TTL 45 seconds. Expired entries are discarded. Wrapper cache metadata does
not replace the Art's observation-time freshness check; the Surface displays
stale status and known age.

## Display and output

Charts include intraday, five-day, calendar and minute periods, volume and MA5,
with a crosshair/tooltip using the same selected data point and formatting as the
chart. History is bounded to 2,000 rows, chart sampling to 240 points, depth to
10 levels, favorites to 8 and the visible history table to the latest 8 rows.
A-share and Hong Kong markets use red-up/green-down; US and other markets use
green-up/red-down. Zero change uses yellow. A depth failure must not erase the
usable aggregate quote.

The sole formal output port is object `quote`. Surface responses reference
collections in authoritative state instead of duplicating history, depth, tape
and favorites in the output; non-Surface responses include those collections.
Preview drawing is not an additional formal output.

## MCP integration contract

`metadata.mcp.calls` supports at most 8 calls with nonempty unique IDs in a single
MCP session. Tool arguments remain checked against `inputSchema`; results are
placed under `frameworkData.mcp.results.<call-id>`. Actual secret-bearing call
arguments are not echoed to the Art runtime or result.

Declared `surfaceActions` select calls and explicitly map permitted payload or
authoritative-state fields. An empty action `calls` list performs a local action
without starting an MCP server. Changing refresh cadence must not fetch a full
market snapshot merely to persist the setting. These are generic framework
contracts, not a stock-specific daemon execution branch.

## Verification and limits

Focused deterministic contracts are `scripts/tests/Test-LoomStockApiMcpServer.ps1`,
`Test-LoomStockMonitorArt.ps1` and `Test-LoomStockMonitorSurface.mjs` in the same
directory. Package checks are `Test-LoomMcpServerPackageContract.ps1` and
`Test-LoomSampleArtPackageContract.ps1`. They cover local protocol, freshness,
cadence, output and packaging behavior with fixtures.

Fixture success does not prove current public-provider availability, token
permissions, quote accuracy or native paired-window acceptance. Those require
separate runtime evidence. Historical Phase 75-77 candidate paths, hashes and
test runs remain in Git tag `cleanup-base-20260928`; use current package metadata
and [release provenance](release-provenance.md) for new delivery.
