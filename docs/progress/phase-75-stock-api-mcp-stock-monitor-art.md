# Phase 75: stock-api MCP 股票可视化 Art

日期：2026-08-15

## 状态

股票可视化 Art、独立 `stock-api` MCP server package、通用 MCP Framework 多调用
能力、安装/升级/发布链、确定性 provider 验证、Loom/Hook 完整源码门禁、Hook R44、
Loom R44 以及 Loom 完整 release verifier 均已完成。

正式 Loom verifier 检查 57 个 checksum 条目，并通过 standalone、Hook canvas、
Hook error preview、all-framework Art store、第三方 plugin boundary、Surface prototype
和 authored Art creation 全部 smoke。Hook R44 的版本和 self-check 也已从正式二进制
重新执行成功。

本阶段没有把未执行的人工原生双端长时 GUI 操作验收写成已通过。行情 provider 的自动化
证据使用 loopback fixture，以便验证语义而不把外部网络波动当成发布判据；真实腾讯、
新浪或东方财富服务在用户环境中的即时可用性仍属于外部运行条件。

## 1. 目的与仓库边界

原有 `股票盯盘` Art 只完成了部分 Surface 和示例数据逻辑。本阶段将它收敛为真正通过
Loom package lifecycle 安装和调用的市场数据 Art：

1. 数据能力替换为 `zhangxiangliang/stock-api@2.7.3`；
2. 以独立 stdio MCP server package 交付，不在运行时执行 `npm install` 或 `npx`；
3. 一次 Art 执行在同一 MCP 会话中取得报价和日 K 线；
4. Surface 显示报价、K 线、成交量与 MA5，同时只发布结构化正式输出；
5. 支持 A 股、港股和美股代码，不提供下单或交易操作；
6. Hook 继续是通用 Surface renderer，不增加股票 Art ID、provider 或协议特判。

源码改动严格位于独立 Loom 仓库。本阶段没有为股票能力修改 Hook 源码；Hook R44 使用
当前 Hook 工作树构建，并对会话开始前已经存在的四处通用 JavaScript Surface 改动完成
全量验证。没有修改 Neuro 下的其他子项目。

## 2. 上游与可复现供应链

上游：

```text
repository: https://github.com/zhangxiangliang/stock-api
package:    stock-api@2.7.3
license:    MIT
gitHead:    9e0bc2c4eed95d1ba49fa163164620ca6fbde53f
```

固定 npm tarball 身份：

```text
sha1:   08eb25cc0248b19cd476e25cce1ad99ea5bd07bd
sha256: 38cce5300e49a9e250196a75969881fcb44c71e0f13ac1cf5e0dc7f8a84589e2
integrity: sha512-Glz7ofEqSViYPmspN6uR2dX9PGneK9V2a/8Tdv3tdMUr0CAm+G4bf4SDNoxgmkwCoGCZF0GV5c55X0Rqayw3pA==
```

正式 MCP ZIP 内 vendor tree 有 104 个文件；canonical tree SHA-256 为：

```text
ca02a1c1c5f36199eab96de7e1b22a3226e84e534dd3d72397b1f73f26d024de
```

同时固定 Windows x64 Node.js `22.22.2`：

```text
node.exe SHA-256:
ae1a50511be58e987483fdbc12125407443926d2d394669ade2352776e920dd3
```

`Build-LoomMcpServerPackages.ps1` 在打包前检查上游版本、license、文件数、vendor tree
digest、Node 版本、Node digest 和 Node license。运行时只使用 package-local
`node.exe` 与 vendored `dist/mcp/server.js`，不会依赖用户 PATH、全局 Node、npm registry
或可变的 latest tag。CycloneDX/SPDX SBOM 同时记录 stock-api 和 Node runtime。

正式 package：

```text
neuro.official/stock-api@2.7.3
transport: stdio
tools:
  get_stock
  get_stocks
  get_klines
  search_stocks
  inspect_stock
credentials: none
```

## 3. 通用 MCP Framework 多调用

`framework-packages/runtime-host/src/mcp.rs` 新增通用 `metadata.mcp.calls` 合约，不是
股票专用分支：

- 每次 execution 最多 8 个 call，call ID 必须非空且唯一；
- 只连接一次 server、执行一次 `tools/list`，然后按声明顺序复用同一 stdio 会话；
- 每个调用继续按 MCP tool `inputSchema` 执行参数边界；
- 多调用结果写入 `frameworkData.mcp.results.<call-id>`，每项包含 `toolName` 和 `result`；
- 保留单个 `toolName` 配置的既有结果格式；
- MCP 实际 arguments 不回显到 Art runtime 或结果；
- 输出、错误和协议数据继续受既有字节数与 redaction 限制。

Surface action 增加声明式 `surfaceActions` 映射。远端参数只能从 action `payload` 或
`authoritativeState` 显式取值；未声明字段不会自动越过 Surface -> MCP 边界。
`calls: []` 表示本地 action，本次不会启动 MCP server。股票 Art 因此可以只在本地修改
刷新周期，而刷新或提交代码时才重新取得报价和 K 线。

为容纳固定 Node runtime，MCP package 边界仍保持有限：ZIP 上限 64 MiB，解压上限
128 MiB，daemon MCP install body 上限 96 MiB，Desktop MCP ZIP 读取上限 64 MiB。
其他 daemon 请求仍使用各自原有的小请求限制。

没有主动收紧未声明 `surfaceActions` 的历史 MCP Art 的通用参数合并语义。股票 Art
显式声明了全部三个 action 的边界，因此不依赖该兼容行为；若以后统一升级旧 MCP Art，
应作为单独的 breaking-contract 审计处理。

## 4. 股票 Art 1.1.0

`neuro.official/custom-stock-monitor` 使用 `mcp` Framework，并精确依赖：

```text
neuro.official/stock-api =2.7.3
```

一次远端执行包含：

```text
quote:
  get_stock(code, source="auto")

history:
  get_klines(code, source="auto", period="day", count=60, adjust="none")
```

上游 `auto` provider 在其能力边界内使用腾讯、新浪和东方财富来源。Art runtime 对
返回值做代码、市场、币种、价格和日 K 线归一化；找不到有效报价或 K 线时返回结构化
`stock_monitor_failed`，不会显示伪造行情。

正式 `quote` 输出包含：

```text
provider / providerVersion / source / sourceName
code / market / name / currency
price / change / changePercent / observedAt
metrics.open / high / low / previousClose
history.period / adjust / rows
disclaimer
```

正式输出不包含 Surface preview 的节点树或 Canvas 内部状态。下游仍只消费 Art 的
`outputs.quote`；Surface snapshot/patch 只用于当前可视化交互。

JavaScript Surface 提供：

- 股票代码输入、刷新按钮以及 30/60/120/300 秒刷新周期；
- 当前价格、涨跌额、涨跌幅、开高低昨收；
- 60 根日 K 线、成交量和 MA5；
- 响应式 Canvas 尺寸与 bounded render；
- declarative fallback；
- 错误态、加载态、数据来源和固定风险提示。

支持代码格式由上游映射能力决定，包括 A 股、港股和美股。Art manifest 明确：

```text
apiKeyRequired=false
trading=false
行情可能延迟，仅用于信息展示，不构成投资建议或交易指令
```

## 5. 安装、启动与确定性验证

Desktop bootstrap、release catalog 和 sample install/execution smoke 都从正式 package
catalog 安装两个 MCP packages：

```text
neuro.official/neuro-image-search
neuro.official/stock-api
```

`Test-LoomStockApiMcpServer.ps1` 直接启动 package-local stdio server，完成
`initialize`、`tools/list`、`get_stock` 与 `get_klines` 协议调用，并验证 5 个工具、
结构化报价以及 3 根 fixture 日 K 线。

provider fixture 覆盖上游实际会访问的腾讯、新浪、东方财富 URL 形状。生产入口只允许
`LOOM_STOCK_API_TEST_BASE_URL` 指向 loopback。完整 daemon E2E 另外生成测试专用 MCP
ZIP，由测试 wrapper 注入临时 loopback 地址；正式 `stock-api.zip` 不被修改。这样既验证
真实 vendored stock-api 解析逻辑，也不会把 CI 绑定到外部行情服务。

`Test-LoomSampleArtInstallExecution.ps1` 在隔离 control plane 中安装 4 个 Framework、
2 个 MCP server 和 7 个 Art package，实际通过 daemon 执行图片搜索和股票报价/K 线，
检查正式输出、Surface action 以及卸载后的 dependency cleanup。

## 6. 2026-08-15 fresh 源码门禁

| Gate | 结果 |
| --- | --- |
| PowerShell / JavaScript syntax | 12 个 PowerShell + 2 个 JavaScript passed |
| stock-api native MCP protocol | 5 tools；quote + 3 candles passed |
| Stock Monitor runtime/Surface contract | passed |
| MCP Framework runtime host | 11 passed |
| Loom daemon targeted package limit | 1 passed |
| Loom Desktop dual-MCP bootstrap | 1 passed |
| Standalone release contract | passed |
| Framework/MCP/Art package contracts | 4 Frameworks / 2 MCP / 7 Arts passed |
| Installed sample execution | 7 Arts / 2 MCP passed |
| Loom workspace Rust serialized | passed；daemon 206，tool registry 125 |
| Loom Desktop frontend | typecheck/build passed；142 tests passed |
| Loom Desktop Rust serialized | 35 passed |
| Hook version/license/product typecheck | passed |
| Hook frontend full suite | 254 files / 1069 tests passed |
| Hook JavaScript Surface browser smoke | 5 scenarios passed |
| Hook Rust | unit 228；connector groups 14/11/1/2/4 passed；1 real Tea test ignored as designed |
| Hook production build | passed |

Loom Rust 的第一次并行 Windows/NAS 全量测试出现两个 timing-sensitive fixture 失败；
两项都单独连续通过 3 次，随后 `--test-threads=1` 的完整 workspace fresh run 全绿。
因此本阶段发布依据是串行完整结果，不把第一次并行失败隐藏或误记为产品失败。

额外执行的 Hook `npm run typecheck:test` 仍会报告分布在既有测试 fixture 中的历史类型
问题。它不是当前 Hook 正式 verify gate；正式产品 typecheck、1069 项运行测试、浏览器
预算 smoke 和 production build 均通过。本阶段没有借股票任务扩大范围去重写这些历史
test-only fixtures。

## 7. 正式发布

### 7.1 Hook R44

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\20260815-stock-api-mcp-r44\hook.exe
```

| Item | Value |
| --- | --- |
| source HEAD | `f6e01f3910a434d49ad611d023861ac7660b4908` |
| bytes | `7036416` |
| SHA-256 | `f25bbc6292abaf91006eafe5e1db9d8dd17fd714713f024d1cbfa41db7206369` |
| Authenticode | `NotSigned` |
| `--version` | `hook 0.1.7` |
| `--self-check` | exit 0，`status=ok` |

Hook 工作树在本阶段开始前已有四个未提交的通用 JavaScript Surface 改动，因此 R44
不是 clean-source 签名发布件；它是经过完整门禁的当前工作树构建证据。

### 7.2 Loom R44

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260815-stock-api-mcp-r44
```

| Item | Value |
| --- | --- |
| source HEAD | `2b77b982014f7d76e4228a1af7ccf66126cb3d09` |
| `gitDirty` / `sourceGitDirty` | `true` / `true` |
| `Loom.exe` SHA-256 | `a80c398c17b33bbcb6ec1042221374124c7733363e445ff5b11a840bb2200231` |
| `runtime/loom-daemon.exe` SHA-256 | `aeb06ab865182129b30e39d51003efbfc0313750547c017d9c1a434f46ff0981` |
| Desktop ZIP SHA-256 | `144bfd369c58d141e7d874d0f986cf68834c1f5d0764407b67e11038d57803d1` |
| `stock-api.zip` | 33489669 bytes；`255e00e4eca2b8efb30cea6126684efdcb524f5ec19e5796d8af2821191f2f24` |
| `custom-stock-monitor.zip` | 17605 bytes；`3d9487a81bd6d6840106c1464c68fc12b568f8d760043e32f991c504dbb56f0d` |
| package catalog | 4 Frameworks / 2 MCP / 7 Arts |
| checksum entries | 57 |

完整 verifier fresh 结果：

```text
smoke=passed
hookCanvasSmoke=passed
hookErrorPreviewSmoke=passed
frameworkArtStoreHookSmoke=passed
pluginBoundarySmoke=passed
surfacePrototypeSmoke=passed
authoredArtCreationSmoke=passed
```

manifest 如实记录 dirty source。本阶段没有把该候选描述为 clean-source publication，
也没有提交、重置或覆盖用户现有工作树。

## 8. 已知边界与后续维护

1. 这是行情展示 Art，不提供证券交易、账户、委托、撤单或投资建议功能。
2. 发布门禁使用确定性 loopback provider；外部 provider 的限流、区域访问和即时可用性
   不由 Loom 控制。
3. Hook R44 未做 Authenticode 签名。
4. Hook 的额外 test-fixture TypeScript 检查仍有历史问题，但正式产品门禁已通过。
5. 若升级 stock-api 或 Node，必须更新固定来源、digest、license、vendor tree、SBOM、
   package contract、native MCP 测试和正式 release；不能静默跟随 latest。
6. 若未来要改变旧 MCP Surface Art 的未声明 action 参数行为，应单独做兼容性审计，
   不应在股票 Art 中引入 host/Hook 特判。

## 9. 主要实现位置

```text
framework-packages/runtime-host/src/mcp.rs
mcp-server-packages/stock-api/
art-packages/samples/stock-monitor/
crates/loom_mcp/src/package.rs
apps/daemon/src/lib.rs
apps/desktop/src-tauri/src/lib.rs
scripts/Build-LoomMcpServerPackages.ps1
scripts/New-LoomSbom.ps1
scripts/build-release.ps1
scripts/verify-release.ps1
scripts/tests/Test-LoomStockApiMcpServer.ps1
scripts/tests/Test-LoomStockMonitorArt.ps1
scripts/tests/Test-LoomSampleArtInstallExecution.ps1
scripts/tests/Test-LoomMcpServerPackageContract.ps1
scripts/tests/Test-LoomSampleArtPackageContract.ps1
scripts/tests/Test-StandaloneReleaseContract.ps1
scripts/tests/fixtures/StockMonitorApiFixture.ps1
```
