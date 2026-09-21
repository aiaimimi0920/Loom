# 瓷砖墙 r9 代码交接与待测清单

2026-09-13。当前计划内的代码实现和相关静态检查已完成，测试等待用户另行安排。
本轮没有执行行为测试、桌面自动化、原生输入探针、长测或发布 smoke，也没有构建
r9 发布包。完整产品交付仍待验收。

任务口径继续以 [实施计划](TILE_WALL_IMPLEMENTATION_PLAN.md) 为准：共 53 项，
34 项已有验收证据，19 项保持未勾选。本轮静态检查没有改变任何验收勾选状态。
[阶段 8](TILE_WALL_STAGE_8_ACCEPTANCE.md) 保留此前运行结果和失败记录。

## 已完成的代码范围

1. 普通 Hook 捕获 Unit 已有用户可见的“发布到 Loom”“停止发布”和收回操作权入口。
   独立来源使用准确的配对设备身份，无需创建无关 Surface；相同捕获的并发发布合并，
   关闭参数面板不停止来源，运行时销毁后迟到返回的 relay 会被关闭。
2. 来源恢复保持原 Loom origin、设备、capture 和公开 Live ID。恢复前退出旧 worker、
   清除操作权并重设控制事件游标，停止与恢复竞态不重放旧输入；已关闭、属于其他
   设备或已经结束的捕获明确报告不可恢复。捕获结束会关闭其发布及相关 worker。
3. Loom 增加启动周期内单调时钟、完整场景准备/生效时刻及应用回执。布局支持
   0 至 10000 ms 的生效延迟；重启保留布局，重新建立时钟和即时场景。Hook 使用
   有界 RTT 样本估计时钟，整个场景就绪并到达保守期限后才显示和开放输入。
4. 墙面媒体独立协商 `loom.wall.media.v1`，使用带公共接收时间戳的 NLWM 封包。
   raw BGRA 保留原始尺寸；PNG 默认最大 640 x 360、10 fps，最大尺寸 1280 x 720，
   payload 上限 4 MiB。daemon 最多同时执行 2 个 PNG 编码，按终端能力授权格式。
   普通 Live 继续使用原有 `loom.live.v1` / NLLV，媒体不写入 Surface 或布局存储。
5. Hook 原生及 TypeScript 读端验证封包、时间、尺寸和 PNG chunk 边界。每来源最多
   3 帧待选加 1 帧已选，按公共时钟和 80 ms 缓冲选择帧，处理迟到、过期、不同刷新率
   和 epoch 更换。最多 4 个未完成消费者，旧 generation 的异步工作仍占用准入额度。
6. 终端控制页可为新输出选择 raw BGRA 或 PNG；子进程分别上报真实能力。保留输入
   取消、generation、租约及顺序保护，并增加跨屏取消和同源操作权竞争的可见说明。
   时钟失效、冻结、黑场、识别和撤权会同步处理呈现回执与输入状态。
7. 已补齐相关 Rust/TypeScript 回归代码及 raw/PNG 共享 golden fixture。GUI 来源
   探针已改为从实际 Unit 参数面板发布、读取公开 relay ID 并从 UI 停止发布。
   这些新增和修改的测试、探针均未执行，Tab、焦点和真实发布路径仍待运行验证。

协议、时间策略和资源上限见 [WALL_TIMING_MEDIA.md](../protocol/WALL_TIMING_MEDIA.md)，
终端用法见 [Hook TILE_TERMINAL.md](../../Hook/docs/TILE_TERMINAL.md)。

## 已完成的静态检查

下表中的 `typecheck:test` 和 `cargo check --all-targets` 只检查或编译测试代码，
没有运行测试用例。命令在对应仓库执行；外部程序通过 RTK 调用。

| 范围 | 检查 | 结果 |
| --- | --- | --- |
| Hook | `npm run typecheck:test` | 退出码 0，最终收尾轮通过 |
| Hook | `npm run lint` | 退出码 0，最终收尾轮通过 |
| Hook | `cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets` | 退出码 0，21.57 秒 |
| Hook | Cargo formatter 与修改的 `include!` 文件的 `rustfmt --check` | 均退出码 0 |
| Loom | `cargo check --locked -p loom_protocol -p loom-daemon --all-targets` | 最后一次退出码 0，5.14 秒 |
| Loom | `npm --prefix apps/desktop run typecheck` | 退出码 0 |
| Loom | `cargo fmt --all -- --check` 与修改的 `include!` 文件的 `rustfmt --check` | 均退出码 0 |
| Hook | `node --experimental-strip-types --check scripts/tests/tile-wall/probeGuiSource.ts` | 退出码 0，仅语法检查 |
| Hook | 官方有效行数 checker，ratchet 模式 | 扫描 1214 个文件，无违规、无警告 |
| Loom | 官方有效行数 checker，strict 模式 | 扫描 1032 个文件，无违规，保留 12 项既有软例外或存量债务 |
| 两仓 | `git diff --check` | 均退出码 0；不暂存未跟踪文件 |

Loom 编译仍有既有的 `WallPresentationOutcome`、`event_ack` 和测试 helper
`remove_test_dir` 未使用警告；没有把带警告的结果记录为无警告通过。

日志相对各自仓库根目录保存：

- Hook：`.tmp/tile-code-first-r9-ready-check.log`、
  `.tmp/tile-code-first-r9-closure-types.log`、`.tmp/tile-code-first-r9-closure-lint.log`、
  `.tmp/tile-code-first-r9-closure-fmt.log`、`.tmp/tile-code-first-r9-closure-includes-fmt.log`、
  `.tmp/tile-code-first-r9-probe-syntax.log`。
- Loom：`.tmp/tile-code-first-r9-closure-check.log`、
  `.tmp/tile-code-first-r9-desktop-types.log`、`.tmp/tile-code-first-r9-closure-fmt.log`、
  `.tmp/tile-code-first-r9-closure-includes-fmt.log`。
- 两仓各自的 `.tmp/tile-code-first-r9-lines.json` 和 `.tmp/tile-code-first-r9-lines.log`。
  formatter 和语法检查成功时日志可以为空，退出码已单独核对。

复用两仓官方 lexer 对当前 dirty 源码范围计数：Hook 124 个文件，最大 454 有效行；
Loom 110 个文件，最大 495 有效行。这包含此前瓷砖实现，不把全部文件归为本轮新增。
主要改动文件的有效行数如下；本轮没有新增行数例外。

| 文件（相对 Neuro） | 有效行数 |
| --- | ---: |
| `Hook/src/services/liveRelayController.ts` | 299 |
| `Hook/src/services/tileImageRenderer.ts` | 166 |
| `Hook/src/services/tileMediaQueue.ts` | 63 |
| `Hook/src/services/tileSceneGate.ts` | 34 |
| `Hook/src/components/UnitLivePublication.tsx` | 71 |
| `Hook/src-tauri/src/wall_live/socket.rs` | 175 |
| `Hook/src-tauri/src/wall_live/packet.rs` | 146 |
| `Hook/scripts/tests/tile-wall/probeGuiSource.ts` | 83 |
| `Loom/apps/daemon/src/runtime/wall_live_media.rs` | 266 |
| `Loom/apps/daemon/src/runtime/wall_media_encoding.rs` | 199 |
| `Loom/apps/daemon/src/wall_store/timing.rs` | 188 |
| `Loom/crates/loom_protocol/src/wall/media.rs` | 239 |

对相关 dirty 源码、配置、fixture 和文档进行了严格 UTF-8 解码与 BOM 检查，均通过；
本交接文档及最后更新的文档另行复核编码和空白。媒体 fixture 的两份文件为
`Loom/protocol/fixtures/wall-media.v1.json` 与
`Hook/__tests__/fixtures/wall/wall-media.v1.json`，SHA-256 相同：

```text
207af66d5ff1cc2b9e2a1c4677f1f0224a1bcf8cb6bdade810ef297130c575b0
```

## 2026-09-19：本机逻辑验证结果

按用户要求，本轮测试锚定当前 Windows 测试机，只验证 Hook/Loom 的协议、状态机、
资源所有权、媒体和输入生命周期逻辑。没有使用第二台机器，没有执行 Linux 验证、
局域网联合验收、双物理屏幕同步、物理显示扫描延迟、600 秒桌面长测或 r9 发布包验收。

- Hook 墙面相关单元测试：17 个测试文件，82 / 82 通过。
- Hook 全部 `__tests__/unit`：226 个测试文件，1135 / 1135 通过。
- Hook 全部 `__tests__/integration`：165 个测试文件，558 / 558 通过。
- Loom `loom_protocol` 墙面协议测试：10 项通过。
- Loom `loom-daemon` `wall_store::`：21 项通过。
- Loom `loom-daemon` `wall_http::`：19 项通过。
- Loom daemon 相关 `wall_` 测试集合：43 项通过。
- Hook 原生 `wall_live` 过滤测试：1 项通过；使用独立临时 Cargo target 目录重建，
  避免旧缓存缺少 `reqwest`、`hyper` 和 `windows` rlib 的构建形态问题。
- Hook `typecheck:test`、`lint`、`cargo check --all-targets`：通过。
- Loom protocol/daemon `cargo check --all-targets`、桌面端 typecheck：通过；保留 3 项
  既有 Rust 未使用代码警告。
- Hook 与 Loom `git diff --check`：通过。

本轮首次聚焦运行发现 `TilePresenter` 的两个代码级问题并已修复：无时钟宿主调用不再
显式传入 `undefined` 场景参数；物理输出丢失路径保持清帧并释放呈现租约。修复后 Hook
墙面聚焦测试和完整 unit/integration 范围均重新通过。

Hook 默认 `npm test` 入口没有作为完整结果使用：它会递归收集 `artifacts/` 下历史候选
中的第三方测试文件并长时间挂起。随后使用明确的 `__tests__/unit`、`__tests__/integration`
范围完成了同等产品测试；该入口问题与本轮 Hook 墙面逻辑无关。

## 等待用户安排的测试

以下各组都未在本轮执行；静态通过不能补足这些验收。用户通知开始测试后，再按
依赖顺序安排。涉及真实键鼠和物理输出的运行需要单独的桌面使用时间。

1. 聚焦行为回归：两仓媒体 golden 解析、真实 PNG 解码、协议拒绝路径、时钟与场景
   回执失效、帧队列与过期策略、独立来源授权、发布/停止/恢复竞态、输入取消与交接。
   Hook 的 `TileLiveCache`、`TileMediaQueue`、`WallMediaProtocol`、`TileTiming`、
   `LiveRelayPublication`、`TilePresentation`、`TileInputController` 等用例以及原生
   packet 测试，Loom 的协议、`wall_live_http`、presentation 和恢复测试均待执行。
2. 普通 GUI 来源联合运行：真实捕获、打开 Unit 参数、从可见按钮发布、放置到墙面、
   回源交互、停止发布及来源关闭；验证原 capture、Live ID 和源应用状态连续。
   daemon 重启、输出重启、断网、撤权、已关闭来源和并发停止需要分别覆盖。
3. raw/PNG 和定时呈现：实际模式与能力一致、混合图片/Live/Art 的整场景切换、时钟
   过期/重建、快速布局更新、帧选择、慢解码与慢网络隔离，以及可见操作权竞争。
4. 完整 600 秒资源长测和断线压力：补足此前失败的完整时长，检查动态画面、真实输入、
   CPU、私有内存、句柄、worker 与流量。共享桌面需要约 12 分钟不操作键鼠，保留
   `pointerleave` 和 generation 等安全取消；失败场景不得通过重放点击掩盖。
5. 多输出和实际显示环境：稳定输出 ID、首次配对并发、DPI、旋转、热插拔、丢屏、
   接缝裁剪、输出窗口与控制窗口分离，以及退出后的资源归属。
6. 两台实体电脑的局域网联合验收：图片/Live/Art 跨屏、不同内容并发交互、同源竞争、
   跨屏手势安全交接、断线恢复、关闭管理窗口后的无头运行。需要第二台电脑及实际
   网络环境；单机模拟不能补足该项。
7. 物理测量：同源跨屏的显示时间差、输入到真实显示反馈的延迟、NIC 媒体流量和硬件
   配置。`wall_live_stats`、`data-media-stats`、源效果采样及发送计数均不能替代
   显示器扫描输出或网卡实测，当前没有 Frame Lock/Genlock 保证。
8. 发布门禁：执行两仓所需行为测试和依赖安全/发布合同检查，构建 r9 候选至正式
   release 根目录，再验证包内容、摘要、进程路径、自检、smoke 和关键联合场景。
   现有 r8 二进制无法证明 r9 的新增代码正确。

19 个未勾选项按原计划分布为：TW-04 4 项、TW-05 2 项、TW-06 2 项、TW-07 1 项、
TW-08 5 项、TW-09 5 项。上述分组只安排验证，不改变原任务或验收标准。

## 源码、候选和 Git 状态

Hook 源码身份为 `version-state.json` 的 `publicVersion: 0.2.30`、
`internalRevision: 9`，即 `v0.2.30.9`；该内部版本只分配一次，没有因静态检查重新分配。
Loom 源码配套包含新的 wall timing/media 合同，需与 Hook 和管理 UI 一起升级。

当前保留的既有候选目录仍为 `Neuro/release/Hook/v0.2.30.8-identification-resync`
与 `Neuro/release/Loom/20260912-tile-wall-r8-admission`，已重新确认目录存在。
本轮没有构建新发布二进制或修改旧候选，也没有启动或停止用户的 Hook/Loom 产品进程。

两个独立仓库仍基于 dirty 工作区，未暂存、提交或发布。收尾时分别核对：

- Hook HEAD：`9815e27ab3863d98b8bae80822228e03a2e2d264`；26 个 tracked 修改、
  573 个 untracked 文件、0 个暂存项。
- Loom HEAD：`cdb3dc825679e24c40cb2377159eb4b9ebda7a0d`；41 个 tracked 修改、
  1667 个 untracked 文件（含本文）、0 个暂存项。

这些数量包含原有源码、截图和本地证据。Neuro 根仓及其他子项目已有修改继续保留，
本轮写入范围为 Hook 和 Loom。最终测试结果、r9 产物和完整交付状态待后续追加证据。
