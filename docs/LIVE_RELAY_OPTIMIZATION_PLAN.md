# 跨设备实时投射优化计划（Issue #67）

本计划是后续 AI 的接手入口。需求来自 [Issue #67](https://github.com/aiaimimi0920/Loom/issues/67)
（不是 PR），2026-10-04 UTC 联网核对时仍为 open、无评论。用户已授权按小任务开发、
验证、提交并推送；这不包含公开 Release、扩大设备权限或清理其他人的运行实例。

## 目标与边界

重点是 **Hook 已有实时截图 → Loom LiveRelay → 明确受权的另一设备观看端** 的
流畅性、延迟和稳定性。先得到可复现的阶段证据，再选择有收益的优化，不重建采集。
观看、远程输入、双向编辑的权限互不替代。

- 保留 JPEG/raw 混合版本回退、独立消费者生命周期、latest-frame 和取消/撤销门禁。
- QR 正式 PNG、结构化编辑日志及屏幕墙 NLWM/公共 80ms 时间轴不混入 LiveRelay 实验。
- 暂不引入 H.264、WebRTC/P2P、NAT 映射、音频或新常驻服务；它们需要独立收益证据。
- socket 写入、接收、解码提交、合成、物理显示分别报告；未校准跨机时钟不能直接相减。

## 已核实的起点（不得重复计为本轮成果）

2026-10-04 UTC fetch 后：Loom `e754f66ca5ad0caf1d1307168803918671a6e6f6`、
Hook `6a334c7bcced65c17ab6a37404768d0d27d8941d`，均为干净 main，与 origin/main 一致。

| 已有能力 | 当前 owner / 证据范围 |
| --- | --- |
| JPEG 连接协商、压缩直通、旧端懒转换 | `runtime/live_media_representation.rs`；`tests/live_jpeg.rs`；不是 H.264 |
| 2–3 帧 ring、最新帧分发、独立 socket 与写超时 | `runtime/live_session_media.rs`、`live_media_websocket.rs` |
| 累计字节、跳帧、写失败及一个最新发送样本 | `runtime/live_media_diagnostics.rs`；没有全帧分位数/物理显示证据 |
| Hook 80ms start-to-start 预算、单在途解码、`decoded_submitted` | Hook `src/services/liveRelayController.ts`、`liveRelayPresentation.ts` |
| 屏幕墙同帧 PNG 共享缓存 | `runtime/wall_png_cache.rs`，提交 `93977f4`；4 profile/帧、32 MiB 全局保留预算；不代表实机 CPU 收益已验收 |

上述 Rust 路径相对 `apps/daemon/src/`。本机手测包已另行交付，不将旧包或历史桥接演示
冒充本计划的产品原生两机性能基线。

## 分块任务与完成标准

状态只用 `待办 / 进行中 / 已完成 / 有条件后续`。一个小任务完成即提交、推送并核远程 SHA。

| ID | 状态 | 交付与 owner | 验收 / 下一步条件 |
| --- | --- | --- | --- |
| A0 | 已完成 | Loom：本计划、文档索引 | 实际源码对照 Issue；先提交计划再开发 |
| A1 | 已完成 | Loom：[只读诊断采样 CLI](LIVE_RELAY_MEASUREMENT.md) 与 21 项聚焦测试 | 仅 GET 已授权会话；累计差分、重置分段、重复样本去重；时间/响应/样本有界；脱敏、超时、取消、拒绝重定向；真实 loopback HTTP/CLI 通过；不声称完整 A 基线 |
| A2.1 | 已完成 | Hook：正常发布和受权加入入口 | 真实 mounted Surface 绑定的显式加入、最多 4 个在途请求、同会话去重和迟到清理；不自动申请输入权；组件/控制器与 Chromium 验证通过，原生两机仍属 A3 |
| A2.2 | 已完成 | Hook：收端单槽证据与外部包绑定合同；Loom：采样对齐/交接 | source/epoch/frame、接收计数、decoded-submitted 固定白名单；编译版本/候选 provenance 与 SHA 复核；实际运行进程及原生观看绑定留 A3，不把 daemon/DOM 采样当显示 FPS |
| A3 | 进行中 | 两机原生基线与对照报告 | 本机原生 Surface 前置通过，PC3 fresh SSH 已认证；origin-scoped CA 与两机临时共同 HTTPS 安全边界已验证，但不是常驻部署或原生媒体验收；新候选完整性及原生进程绑定见最新回执。仍须静态文字/滚动/运动、1/2/4 viewer、慢端、恢复/撤销；记录网络/包/CPU/内存/阶段耗时/字节，GPU/物理显示缺失须明示 |
| B1 | 有条件后续 | Hook：呈现调度/IPC 预算优化 | A2/A3 证明轮询或搬运为瓶颈后，一次只改一个变量；JPEG/raw 同尺寸同内容对照，保留单在途/取消 |
| B2 | 有条件后续 | Loom：分发/兼容转换优化 | A3 证明瓶颈后处理；慢消费者不拖其他人，旧 epoch/撤销优先；可测收益不足则不采用 |
| C1 | 有条件后续 | Hook 为主：Windows GPU 视频编码 POC | 仅在 A/B 不足且可测收益成立时设计完整协商、decoder、关键帧依赖、late join、fallback、许可；不只开启枚举 |
| D1 | 有条件后续 | Loom：现有 PNG 共享缓存对照验收 | 不重写 `93977f4`；仅在实际屏幕墙负载需要时比较 1/2/4 同/异 profile，独立 header/权限/时序不退化 |

A3 是实验矩阵，不要求在每次小改动后重跑整个矩阵。先验证 changed owner 和邻近风险；
阶段性运行时大任务再构建新的不可变 release 手测包，不为纯文档/采样脚本伪造二进制发布。

## A1 设计约束

复用现有 `GET /v1/live/sessions/{sessionId}` 和 `mediaDiagnostics`，不新建 daemon API。
凭证仅从环境变量读取，不作为命令行值，不落盘；远程只接受可信 HTTPS，HTTP 仅限 IP loopback。
工具不配对、不授予观看/输入权限、不发布图像、不控制/停止会话。

只输出固定字段白名单：时间范围、累计差分、帧队列/连接数及观察到的最新发送样本统计。
禁止输出原始快照、窗口标题、图像/观察值、token、完整 URL 或原始服务端错误正文。
`lastForward` 是跨 viewer 共用的单槽；采样会漏掉中间写入，统计必须明确标为
“观察到的发送样本”，不能宣称总体 p95、网络丢包、接收成功或屏幕 FPS。

## Piik 参考与许可

固定参考提交 `1b9f5bd2a27eb32d35e81e8f2c5d8d6e952bbd0a`；本轮已联网阅读：

- [有界诊断 recorder](https://github.com/TNTcraftHIM/Piik/blob/1b9f5bd2a27eb32d35e81e8f2c5d8d6e952bbd0a/internal/diagnostics/recorder.go)：限制记录/文件，明确历史缺失；A1 采用边界原则，不复制轮转/删除实现。
- [共享编码 owner](https://github.com/TNTcraftHIM/Piik/blob/1b9f5bd2a27eb32d35e81e8f2c5d8d6e952bbd0a/src/client/media/browser-encoding-pool.ts)：源/codec/profile/budget 匹配、消费者独立；不把其 WebRTC 栈搬入 Loom。

目前只参考设计原则，无 Piik 实质代码复制、无依赖引入。将来复制代码或引入 SDK 时另行核对
MIT 通知、第三方许可、固定版本、校验和与依赖安全门禁。

## 提交与接手约定

1. 从本文件确认下一项，核当前 child 仓库 HEAD/status 和远端；保留 Neuro/siblings 改动。
2. 修改前测有效行数；新增模块按职责控制在 500 有效行以内，继续执行严格 checker。
3. 小任务提交包含实现、聚焦测试、必要合同/用法及本计划状态更新，不夹带下一任务半成品。
4. 使用对应独立仓库 main 的普通 push；并发远端变化先复核，不 force push。
5. 提交消息注明任务 ID 和 `Refs #67`。本文件当前任务完成所在提交即该任务源码锚点，
   用 `git log --oneline -- docs/LIVE_RELAY_OPTIMIZATION_PLAN.md` 定位，不在同一提交里虚写自身 SHA。
6. 每项记录命令、真实结果、未验证项和下一动作；本地详细日志留 `GameEditor/linshi`，
   必要摘要进仓库。不提交凭证、采样原始私有数据、node_modules、target 或 release 产物。

## A1 验证与逐文件复核（2026-10-04 UTC）

- `node --test scripts/tests/live-relay-measurement.test.mjs scripts/tests/measure-live-relay.test.mjs`：21/21 通过。
- `node --check`：下表 5 个新增脚本/测试全部通过。
- `node --test scripts/tests/effective-code-lines.test.mjs`：15/15 通过。
- `node scripts/effective-code-lines.mjs --mode strict --json <linshi-output.json>`：1180 文件，0 个超过 700；未修改既有例外。
- `git diff --check`：通过。无 Rust/前端生产改动、依赖变化；未重跑无关编译/OSV，也不为采样工具重建 EXE。
- 根目录没有独立脚本 formatter 配置；手动复核格式并运行 Node 语法检查，未冒称运行不存在的 formatter。

| 新文件 | 有效行数（修改前均 0） | 复核重点 |
| --- | ---: | --- |
| `scripts/live-relay-measurement.mjs` | 110 | 白名单、source 绑定、整数精度、计数重置、样本去重；摘要最多 6000 样本，O(n log n) |
| `scripts/measure-live-relay.mjs` | 141 | GET-only、凭证仅环境、TLS/重定向、1 MiB 响应、单在途、总 deadline、reader/timer/文件清理 |
| `scripts/tests/live-relay-measurement.test.mjs` | 90 | 差分、失败写入、epoch/回退、脱敏、边界和精度 |
| `scripts/tests/measure-live-relay.test.mjs` | 155 | 真实 HTTP/CLI、拒绝异常响应、取消/关闭、专属子进程 5s 超时、独占证据文件 |
| `scripts/tests/live-relay-measurement-fixture.mjs` | 15 | 两套测试共享最小响应数据；含私有哨兵用于泄漏回归，不是生产转发层 |

新增文本均为 UTF-8 无 BOM；无新软上限例外、无既有大文件增长。详细本地证据：
`GameEditor/linshi/issue67-a1-20261004/{focused-tests.tap,effective-lines.json,file-review.json}`。

## 当前交接

- A0：已完成并推送计划提交 `047d6e52a9c9d6ed2b8b8b18429a2c54d76150e8`。
- A1：工具/用法/21 项聚焦测试已完成；本状态更新随 A1 实现一同提交，使用上述 git log 定位其源码锚点。
- A2.1：Hook `5857e77c79f7ab6f24d1a214a3001e512bc5a2fa`，内部迭代 `v0.2.32.13`。
  Live Unit 中的发布入口改称“实时投射”；支持 Surface 的 Art 参数面板可刷新/选择/加入。
  当前必须复用已挂载的 Surface，不生成伪造 attachment，不把配对直接等同于观看/输入授权。
- A2.2：Hook `db029ef244ca2b945dddfb90c4a66f59a359452e`，内部迭代 `v0.2.32.14`。
  [收端诊断合同](https://github.com/aiaimimi0920/Hook/blob/db029ef244ca2b945dddfb90c4a66f59a359452e/docs/LIVE_RELAY_DIAGNOSTICS.md)
  明确有界读取、时钟口径、源/帧关联和 EXE/实际进程身份检查；DOM 不伪造包 SHA。
- 下一动作 **A3**：用本轮不可变候选从正常入口完成一个源/一个受权观看端的原生两机闭环，
  先核实际进程路径/PID/开始时间/SHA 与 CDP owner，再对齐 A1/A2.2 证据。未绑定的浏览器
  夹具不能冒充新包的原生显示；之后按需扩大多 viewer、慢端、恢复/撤销和负载矩阵。
  本机原生 Surface 前置已通过；用户确认 PC3 地址后，专用 key 的 fresh SSH 认证已实证通过。
  当前不再等待 PC3 授权或 IP。共同 HTTPS 的客户端信任合同及两机临时安全边界已验证，
  详见末尾最新回执；它不是常驻入口。先闭合新候选远端完整性，再绑定桌面/CDP 和真实
  Surface/合法会话；管理传输、旧 bridge 和各机独立 loopback 服务不等于原生两机媒体链路。
- 仍未验收：正常产品入口两机闭环、真实观看 FPS/帧龄、CPU/GPU 收益、受限网络和长稳。
- 前期 A0/A2.1 辅助子代理因上游 503 没有有效结果；A2.2 已完成独立只读审查和最终增量
  复核。两者分开记录，不把前期直接核查冒充独立评审。

## A2.1 验证与逐文件复核（2026-10-04 UTC）

- `UnitLiveViewer.test.tsx`、`LiveRelayPresentation.test.ts`、`LiveRelayPollCadence.test.ts`：43/43 通过。
  新入口覆盖显式加入、真实绑定、无自动控制、缺失/已销毁 Surface、面板关闭、generation/
  attachment 换代、owner 销毁、错误重试、关闭/离线/本地源过滤、跨面板去重及在途上限。
- `npm run typecheck`、`typecheck:test`、`lint`、`git diff --check` 通过；checker 测试 16/16；
  strict 行数门禁 1371 文件、全部不超过 500。未新增依赖或修改 Rust。
- Chromium 实际渲染产品组件，模拟 IPC：30 个长标题会话、键盘选择、加入状态及 250px
  组件在窄/宽视口下无横向溢出；不是完整桌面、真实网络或显示性能证据。
  首次临时 Vite harness 扫描了旧产物而超时，改用 linshi 内隔离 root/cache；第二次发现
  harness 的 poll 返回了不同 session ID，修正测试夹具后通过，未为测试错误改产品代码。
- 前端没有独立 formatter 配置；遵循相邻格式，运行 ESLint/类型检查，不声称执行不存在的 formatter。
- 有效行数：`UnitParamsPanel.tsx` 384→389（只接线）、`liveRelayController.ts` 342→359
  （加入在途生命周期）、`UnitLivePublication.tsx` 71→71；新组件 74、CSS 16、测试 138、fixture 27。
- 逐文件复核：UI 字段按文本输出；鉴权仍由 native/Loom 执行；没有新增 timer、持久历史、凭证
  或媒体复制；关闭面板不 dispose 全局 owner。在途响应以单独新 relay ID 清理，已有 viewer 不受影响。
- 本地证据在 `GameEditor/linshi/issue67-a2-20261004`；未对其他运行实例做停止/重置。

## A2.2 验证与逐文件复核（2026-10-04 UTC）

- 最终四组聚焦回归 54/54：白名单、非法数值/ID、relay/session/epoch/frame 一致性、
  旧帧保留、真实 controller stop、Surface 加入与解码/轮询邻近路径。
- 应用/测试 TypeScript 检查、ESLint、Rust formatter、strict 行数门禁与 diff 检查通过。
  checker 测试 16/16，1373 个文件全部不超过 500 有效行；新诊断模块 44、测试 91。
- 完整串行前端测试 442 文件/2047 项通过（946.87 秒，主要为环境初始化）；之后最后的
  ID 字符限制/relay 校验增量以 54 项聚焦回归及重新执行的 typecheck/lint 覆盖。
  初步怀疑长时间无结果后尝试有身份保护的停止时，进程已自行退出；没有实际终止它。
- Chromium 运行真实组件与 decoder：JPEG/raw 实际像素读回、epoch 变化、解码失败保留
  旧像素、closed 失效与 stop 移除槽通过。IPC 为夹具；不是原生网络、进程绑定或物理 FPS。
  首轮临时 harness 的 250px root 截断了全局窗口；修正夹具容器和依赖 alias 后复核通过，
  没有为夹具错误修改产品布局。
- 依赖安全合同及联网 Enforce OSV 通过：4 lockfiles、1651 packages、0 未抑制 ID，
  扫描器过滤 19 个现有受控例外；不是宣称依赖完全没有 advisory。未新增依赖/修改锁文件。
- 逐文件复核：诊断固定字段、ASCII ID/安全数值有界，禁止像素/标题/URL/错误正文/凭证；
  单槽覆盖，无新 timer/媒体复制/持久历史。generation 仍由原 controller 在读帧和解码后
  复核；sourceIdentity 仅沿合法加入传递。window/store/types/controller 只做 1–6 行接线，
  所有相关文件不超过 401 有效行。独立只读审查未发现确认缺陷，指出的 stop 测试缺口已补。
- 本地详细证据放在 `GameEditor/linshi/issue67-a22-continuation-*`。构建/产物回执见下方；
  不发布内部 tag/公开 Release，不停止或替换既有 Loom，保留 Neuro 根仓库所有原有改动。

### A2.2 内部候选产物回执

- Hook `v0.2.32.14`，源码 `db029ef244ca2b945dddfb90c4a66f59a359452e`，provenance 为
  `channel=internal`、`gitDirty=false`；沿用该轮已分配版本，没有为重建再次增加 revision。
- Neuro 内不可变目录：`release/Hook/v0.2.32.14/issue67-a22-20261004T045412Z-db029ef`。
- `hook.exe`：8,980,480 bytes，SHA-256
  `bf8b7c678158a8f98d17297e3da7ce966cfe66833824bc77d22fb2c0548d45a7`。
- 本轮 fresh Tauri production/release build、实际 SHA/provenance 核对、绑定该 SHA 的
  headless self-check 及 `PreflightOnly` 均通过。既有大 JS chunk warning 未在本任务改写。
- 证据状态为 **artifact-verified**；`process-bound` / `native-viewer-observed` 未验证。
  预检不会启动原生界面，自检也不证明 LiveRelay。没有跑新包的两机、600 秒 soak 或输入验收。
- 本轮 Loom 只有两份测量/计划文档变化，不重建相同 runtime，也不将 Hook 检查冒充 Loom 验收。

## A3 本机原生前置与授权边界（2026-10-04 UTC）

本小块只验证精确候选的原生启动、正式 Surface 操作和清理。A3 整体仍为进行中，
没有源端实时投射或另一台设备的原生 LiveRelay 观看证据；不把 Surface dashboard 当 viewer。

### 候选与已通过的观察

- Hook 使用上述 `v0.2.32.14` 不可变候选；实际 EXE SHA-256 与 A2.2 绑定一致。
- Loom 使用 `Neuro/release/Loom/local-20261004T022015Z-e754f66c`，包来源为
  `e754f66ca5ad0caf1d1307168803918671a6e6f6`，不是当前文档提交的 SHA。
  `runtime/loom-daemon.exe` SHA-256 为
  `b2f1c3edaeeae5c853f9c9096f0236a31d23289aa8781a13166c107027d93478`。
  仅验证既有包，没有改生产代码、分配新版本或重复构建。
- 调用现有 `Hook/scripts/Invoke-HookLoomSurfaceCandidateAcceptance.ps1`，显式传入两个
  候选路径及期望 SHA，`-DurationSeconds 60 -WarmupSeconds 0`，隔离控制面和应用数据。
  外层回执时间 `05:22:46.2970888Z` 至 `05:24:14.0255469Z`，内外 `summary.json`
  均为 `status=passed`、`passed=true`；执行工具最终 exit code 为 0。没有重复运行本次验收。
- 真实 Tauri/WebView2 启动通过。初次主进程 PID `40228`，开始于 `05:22:51.4526850Z`；
  重启主进程 PID `49396`，开始于 `05:24:04.3759690Z`。回执记录的两次实际路径均为
  该不可变 Hook EXE；第二实例 exit code 为 0，原主进程仍存活，未绕过单实例 mutex。
- 正式 Surface 刷新和交互 probe 通过，revision `1→4`，重启后 `5→8`；设置持久化通过。
  60 秒本机观察记录 33 个内存样本、增长门禁无违规；不据此推断 LiveRelay CPU/GPU 收益。
- 两次退出均为 0，退出后的 Hook 进程和 debug listener 列表为空，强制清理 PID 列表为空。
  外层 cleanup 通过：隔离 daemon、Art Store 已停止，store/daemon/bridge listener 均为空；
  收取最终回执后再次查询，本机没有残留 `hook.exe`。没有停止其他既有实例。

### 证据边界与接手动作

- 本机证据目录：`GameEditor/linshi/issue67-a3-20261003-222017`。包绑定见
  `package-bindings.json`，原生回执见 `native-surface/summary.json` 与
  `native-surface/hook-native/summary.json`，详细输出见 `native-surface.log`。
  `native-precondition-review.json` 是固定白名单复核摘要，不复制原始 Surface/配对数据入仓库。
- 尚未核验 CDP listener 到 Hook 主进程的父链；尚未采到原生观看端
  `data-live-relay-diagnostic`。不能将本次结果标为 `native-viewer-observed`，也不以包路径/PID
  替代完整的 viewer `process-bound` 合同。两机 FPS、帧龄、带宽、CPU/GPU、600 秒长稳、
  慢 viewer、恢复/撤销和返回输入仍未验收，原有 A3 实验矩阵不缩减。
- PC3 历史专用授权撤销回执确认 `dedicatedKeyPresent=false`、其他 key/ACL 保留；
  当时 fresh pinned-key SSH 返回 255/Permission denied。本次没有尝试该撤销 key、恢复授权
  或修改远端。旧配对及 mTLS/NLWM PNG bridge 显示证据不能作为当前授权或原生 LAN 基线。
- 现有 PC2 管理通道本轮只读查询成功，`uname -s` 返回 `Linux`（见 `pc2-os.log`）；
  它不能直接充当本轮 Windows Hook/WebView2 观看端，未安排 Wine 等替代路线。
- 已向用户请求：仅本轮临时恢复 PC3 专用 SSH 授权、结束后再次撤销，或指定另一台
  已授权 Windows 测试机。获得明确答复前不做远端权限变更，也不重复消耗本机已通过证据。
- 授权齐备后先完成一个源/一个受权 viewer：源端真实 `Ctrl+2` 后从参数面板发布到 Loom；
  收端使用真实已挂载的 Surface-capable Art 刷新/选择/加入，分别绑定 EXE/SHA/PID/开始时间
  和 CDP owner，联合采集 A1 发送采样及 A2.2 收端单槽，再验证更新与停止。不能绕过鉴权
  或使用旧 bridge/PNG tile adapter。证明瓶颈后才进入 B/C 条件优化。
- 现有 `Hook/scripts/tests/Invoke-LiveUnitNativeProbe.ps1` 强制输出位于 `Hook/artifacts`；
  不要将 joint runner 的 `-LiveUnitProbe` 与本节 linshi `ArtifactRoot` 直接组合。后续选择
  已满足输出门禁的调用方案，不能为方便测试擅自放宽门禁。
- 本小块交接门禁：checker 测试 15/15、Loom development manual contract、UTF-8 无 BOM
  和 `git diff --check` 通过；strict checker 扫描 1180 个文件、0 违规，11 个既有软上限
  例外未改动。仅修改本计划，Markdown 不计源代码有效行数；未重复前端/Rust 全量测试、
  原生验收或构建。门禁回执见该证据目录的 `documentation-validation.json`。

### PC3 已授权后的连接复核（2026-10-04 05:44–05:46 UTC）

本节保留当时的失败证据；当前连接结论已由下面的 fresh 认证成功更新。

- 用户明确表示“我现在给了你pc3的权限”；不再将本轮状态记为等待用户授权。
  访问权限范围仍仅限本轮测试，不扩展到其他 key、防火墙、现有应用实例或主机重启。
- 复用既有专用 SSH 配置，`BatchMode=yes`、`StrictHostKeyChecking=yes`，禁止连接复用。
  旧入口直连 TCP 已建立，但返回 `kex_exchange_identification: Connection closed by remote host`
  和 exit 255；尚未收到服务端 SSH banner，未进入认证，也未实际验证主机密钥。
  这不是 `Permission denied`，不能判断新授权无效或专用 key 是否已恢复。
- 通过现有、严格认证的 NAS 管理通道做一次 SSH TCP 转发对照，返回
  `channel 0: open failed: connect failed: No route to host`；NAS 对旧目标的只读 route 查询
  成功，但邻居状态为 `FAILED`。只证明该路径不可达，不断言目标关机、IP 改变或唯一故障原因。
- 本轮证据：`GameEditor/linshi/issue67-a3-pc3-20261003-224314` 内的
  `fresh-ssh-diagnostic.{json,log}`、`fresh-ssh-via-nas.{json,log}` 和 `nas-cached-route.log`。
  receipt 区分配置中的严格主机校验与尚未执行到的实际校验；用户授权与独立连接结果分记。
- 已询问 PC3 当前地址/SSH 端口或新的连接方式，获得有效入口后再做一次固定主机密钥的
  fresh preflight，然后直接接续两机小闭环。没有修改远端、恢复 key、创建监听/转发常驻实例，
  没有启动测试应用或重复已通过的本机验收；不把连接失败当产品 LiveRelay 回归。

### PC3 地址确认后的 fresh 认证与时钟口径（2026-10-04 UTC）

- 用户再次确认地址为 `192.168.15.136`。物理以太网绑定探测收到
  `SSH-2.0-OpenSSH_for_Windows_9.5`；fresh 专用 SSH exit 0，实际核对固定 ED25519 主机密钥，
  并以 `publickey` 认证成功。没有恢复或续期 key，没有更改路由、代理或防火墙。
- 远端为 `CODE / mjc`。WTS API 确认 Active Console Session 1，explorer 属于该 session；
  复核时无 Hook、LockApp 或 Issue67 测试任务。PC3 没有 `Z:`，其 `nas_home/AI/GameEditor/linshi`
  是普通本地目录；本轮使用新的独立目录，不创建网络盘映射或替换历史候选。
- 首次认证回执的本机 UTC 约 `06:30:21Z`，同次远端返回约前一日 `21:03:00Z`。
  这是明显时钟差异的观察，不是时钟校准。没有修改系统时间，不以跨机时间戳相减计算延迟。
- 旧 expiry 为 `2026-10-04T06:21:39Z`。旧回执字段 `dedicatedAuthorizationExpired` /
  `dedicatedAuthorizationKnownExpired` 仅表示本机时钟超过配置值，不是远端授权已失效的结论；
  服务端实际接受了该专用 key。保留原回执，以新 `authorization-clock-basis-correction.json`
  补充字段口径，不能因本机时间而自动续期或重新要求授权。
- 认证/桌面证据在 `GameEditor/linshi/issue67-a3-pc3-retry-20261003-232903`；后续管理传输和
  时钟口径修正在 `GameEditor/linshi/issue67-a3-pc3-native-20261003-234741`。小文件校验落盘和
  SSH 认证不能替代完整 EXE 校验，更不能标为 `native-viewer-observed`。
- 两端现有默认 manifest 都指向各自 `http://127.0.0.1:8765`，并非共同服务入口。
  原生跨机路径仍需正常受信任的 HTTPS Loom、合法设备会话和真实 Surface attachment。
  不把 `LOOM_TLS_TERMINATED=1` 当作已有 TLS，不关闭证书校验，也不假定 Windows 私有根证书
  会被 Hook 的 Rustls/WebPKI 客户端接受。用户已确认目前没有共同 HTTPS 地址；这不是已获
  公网部署、全局信任或防火墙扩权授权，不请求发送凭证。

### A3 PC3 候选传输停点与下一块（2026-10-04 UTC）

- 本轮只尝试传输上述精确 `v0.2.32.14` 候选。provenance 和有 SHA 门禁的临时 launcher
  小文件已校验落盘，但 8,980,480-byte EXE 尚未完整传输，**没有启动 launcher 或 Hook**，
  没有注册原生测试任务、启动 CDP listener、绑定观看进程或采到原生 viewer 诊断。
- fresh SSH 小命令可用，但大块 SCP/SFTP 出现连接重置；直接 artifact HTTP 和私有 CA
  校验的 HTTPS 文件下载也出现读取超时。这些只是管理传输，未承载 LiveRelay 媒体或凭证；
  临时 TLS 信任仅限该 Python 客户端的证书文件，没有安装系统根证书、扩大防火墙或改全局路由。
- 物理网卡绑定、4096-byte buffer / 单在途请求的 SCP 在 90 秒上限前取得约 2.86 MB。
  最初目录快照的长度与哈希读取存在在途写入竞态，不能据其宣称数据损坏。停止写入后另存
  `pc3-scp-stable-partial.json`：长度前后均为 2,863,104，SHA-256 为
  `f2a07a8007594923dc470aa1ff6a434f503722691cb7bf14547f5beed86270a4`，与本机候选的同长度前缀一致。
- 因该新证据只沿已验证路径做了一次有界续传，未重头反复上传。最终约 26.6 秒后返回
  exit 255 / `Timeout, server 192.168.15.136 not responding.`，见 `verified-prefix-resume.json`。
  这说明当前管理链路尚不足以可靠完成传包；没有证据将唯一原因归给 Wi-Fi、代理、Defender 或产品。
- 续传后的稳定只读回执 `pc3-final-after-resume.json` 记录 3,665,920-byte partial；未达到完整
  EXE 大小，不能安装或启动。没有活动 SFTP、Hook 或测试任务；后续恢复必须重新核稳定长度、
  同长度前缀 SHA 和最终完整 SHA，不能只看“续传退出”或复用在途目录快照。
- 最后只读复核仍为 Active Console Session 1，无 Hook/LockApp/Issue67 测试任务。
  本机临时 artifact listener 和管理 helper 已按有界生命周期退出，见 `local-final-readiness.json`；
  没有停止既有 Loom 或其他应用。一次合并清理命令被执行策略拒绝，未执行；未完成产物保留在
  本轮独立 linshi 目录，不能手动运行或标为已安装。临时传输私钥保留受限 ACL，不纳入 Git。
  本轮专用 SSH 授权未续期、未提前撤销；验收尚未结束，接续完成后仍仅撤销该专用 key 行并
  实测 fresh SSH 拒绝，保留其他 key/ACL。不能用本机 expiry 标志冒充已完成撤销。
- 当前阶段不是 PC3 `artifact-verified`、viewer `process-bound` 或 `native-viewer-observed`。
  本轮没有新的两机媒体、性能、长稳或输入结论，不重跑已通过的本机 Surface 前置，也不改调度/codec。
- 下一小块先闭合共同 HTTPS Loom 的安全部署合同及入口：复用正常鉴权/配对、明确客户端证书
  信任、请求与媒体 WebSocket origin、TLS terminator 和回滚边界；不得只改 manifest URL 或
  `LOOM_TLS_TERMINATED` 凑通过。与此同时仅在管理链路恢复后续传并核完整 EXE SHA，之后才做
  Active Console 进程/CDP 父链绑定，再沿真实发布与受权加入入口采 A1/A2.2。A3 整体继续进行中。

### A3 最新回执：origin-scoped CA 与共同 HTTPS 安全边界（2026-10-04 UTC）

本节更新此前“共同 HTTPS 尚未闭合”的停点；上述 `.14` 原生前置及失败传输记录保持
原样，不用新包冒用旧 EXE 的运行证据。A3 整体仍为进行中。

- Hook 实现提交 `d8cee86bdaaf8d3d858f7fc970d7487c78cc50f7` 已普通推送到 `main`，
  fresh `ls-remote` 于 `10:00:37Z` 核对同一 SHA。先前 Git TLS 传输失败确实未发布；
  显式使用现有本机代理和单命令 HTTP/1.1 后成功，未修改全局 Git/代理或关闭 TLS 校验。
- [Hook 信任合同](https://github.com/aiaimimi0920/Hook/blob/d8cee86bdaaf8d3d858f7fc970d7487c78cc50f7/docs/SECURITY_BOUNDARIES.md#origin-scoped-loom-https-trust)：
  进程同时设置 `HOOK_LOOM_TLS_ORIGIN` 和绝对路径 `HOOK_LOOM_TLS_CA_FILE`，仅指定
  canonical HTTPS origin 及对应 WSS origin 获得额外 CA，其他 origin 沿用 WebPKI。
  HTTP/WSS 共用标准 Rustls 校验，scoped HTTP 禁止重定向；没有 dangerous verifier、
  系统根证书安装或远程明文放行。CA 为本地 regular certificate-only PEM，最多
  32768 bytes / 8 张证书，缓存身份包含 origin 和 CA digest。
- 配置错误会阻断**全部 trust-aware client construction，包括 loopback**，不是仅阻断
  配置目标；进程 `OnceLock` 固定成功或失败状态，修正/轮换必须重启 Hook。proxy-only
  remote-image/Tea/voice 调用及 `--no-default-features` loopback manifest gate 未放宽。
- 与该源码对应的 TLS 10/10、network proxy 7/7、device session 19/19、Loom connector
  5/5、wall live 1/1 通过；另有 `--no-default-features` connector 5/5、all-targets
  compile、formatter、strict 行数门禁和 dependency contract 通过。真实联网 OSV Enforce
  为 4 lockfiles / 1654 packages / 0 未豁免 ID，沿用 19 项既有例外，未新增豁免。
  `talk_connector::` filter 为 0 tests，仅有编译覆盖；本轮未重跑全量前端或 native suite。
- 首次 TLS 夹具失败已实查为 Windows accepted socket 继承 listener nonblocking 状态，
  服务端报告 `Interrupted handshake (WouldBlock)` / `os error 10035`。仅修正夹具 socket
  模式和有界 shutdown/join 后通过，没有放宽产品证书校验或用重试掩盖。

#### 新不可变候选

- Hook 内部版本 `v0.2.32.15`；目录
  `Neuro/release/Hook/v0.2.32.15/issue67-scoped-https-20261004`。
  fresh Tauri production/release build 已完成，公开 SemVer 仍为 `0.2.32`。
- `hook.exe`：8,990,208 bytes，SHA-256
  `02fcca8dc75a486500ae6cb928d5e8de9c3df079cc21928ca36d3c0043c467b0`；
  provenance 为上述 `d8cee86...`、`gitDirty=false`、`channel=internal`。
  本机 headless `--self-check` exit 0、`status=ok`；不把自检当原生观看证明。
- 新包未发布 public Release/tag；不重复构建，不替换 `.14` 包或旧 PC3 partial。
  本轮 PC3 新候选仍未完整校验，未启动 EXE 或注册测试任务；详见下面的有界传输回执。

#### 两机真实 HTTPS 探测及清理

- 临时普通 Caddy TLS terminator 监听 `https://192.168.15.20:49874`，反向代理到独立
  loopback daemon `127.0.0.1:49873`；仅允许 `.20/.136` 来源，拒绝外部 browser Origin，
  后端 Host 固定为 loopback。没有开发 NeuroServer、使用旧 mTLS/NLWM 媒体桥接，或用
  `LOOM_TLS_TERMINATED=1` 冒充 TLS；管理员 token 只在本机 daemon 进程环境，未发给 PC3。
- 使用已校验的既有 Loom runtime，daemon SHA-256 仍为
  `b2f1c3edaeeae5c853f9c9096f0236a31d23289aa8781a13166c107027d93478`。
  Caddy `v2.11.7` 来源和官方摘要已联网核实，工具只留 linshi，不引入源码依赖。
- 两端诊断客户端均实测 `/health=200`、未授权 `/status=401`、带正确升级头及
  `loom.live.jpeg.v1` 子协议但未授权的 media WSS 请求 `401`、外部 Origin `403`，
  默认不信任该 CA 的客户端拒绝证书。回执 `local.passed=true`、`remote.passed=true`。
  全部是 Python 诊断客户端，没有实际 `101`、媒体、Hook viewer 或性能结论。
- 公共 CA 经 pinned SSH 和 SHA 校验落盘 PC3；CA 私钥未持久化，leaf key 仅留本机受限 ACL
  目录。证书约 `2026-10-05T09:24:42Z` 到期，复用前核 metadata，不修改两机系统时间。
  已观察到两机时钟差，禁止跨机时间戳相减计算延迟。
- 临时 daemon/terminator 已按路径、PID、creation time 绑定清理，回执
  `remainingListeners=[]`、`remainingOwned=[]`；该 URL **不是常驻可用服务**。
  未改防火墙、系统信任、全局网络或用户既有运行实例。

本轮详细证据：`GameEditor/linshi/issue67-scoped-https-20261004`，关键文件为
`hook-published-receipt.json`、`focused-test-results.json`、`package-binding.json`、
`isolated-lan-https-receipt.json`。下一步仍是新包 PC3 完整性、真实 Surface attachment/
合法设备会话、原生进程/CDP 父链绑定，再从正常发布/受权加入入口采 A1/A2.2。源端、
观看端、更新/停止、性能、多 viewer、长稳和返回输入均不能由上述诊断替代。

#### PC3 新候选有界传输与当前网络事实（2026-10-04 10:03 UTC 起）

- 沿已验证的普通 Caddy HTTPS 路径临时开放精确 `/candidate/hook.exe`，只限 `.20/.136`，
  不开放目录、其他路径或 browser Origin；CA 校验、大小/SHA 门禁和拒绝重定向保持。
  独立 `.part` 只有完整校验成功才原子改名为 EXE，没有传输管理员凭证或 LiveRelay 媒体。
- 整文件读取超时，留下 14,471-byte partial；停止写入后本地同长度前缀 SHA 完全一致。
  随后做一次有区分力的 1024/8192/16384/65536-byte 标准 HTTP Range 对照，四项均为
  `206`、完整长度和匹配前缀 SHA；这仅证明这些小响应，不能推断大传输已恢复。
- 单连接、单在途 8192-byte Range 传输在第二请求读取 headers 时超时，仅留下
  8192 bytes。沿“小响应新连接已通过”的证据只做一次有界 fresh-connection 续传，
  先校验该稳定前缀，没有重头重传或覆盖旧 `.14`；13 个新请求后 body 读取再次超时。
- 最终稳定 `hook.v0.2.32.15.ranges.part` 为 110,258 bytes；长度前后相同，SHA-256
  `5ae93ecaa00e9c05c4f309cc8206ad367b5ada8de2c7b88095941f6588741547`，
  与新候选的同长度前缀一致。远端无 downloader 残留、无最终 `.15.exe`，故不是 PC3
  `artifact-verified`、`process-bound` 或 `native-viewer-observed`，不运行不完整文件。
- fresh 只读网络实查：`.136` 属于 interface 4 / Intel Wi-Fi 6 AX200 WLAN，状态 `Up`、
  link rate `433.3 Mbps`；Realtek PCIe GbE interface 11 为 `Disconnected / 0 bps`，
  仅有 link-local IPv4。适配器累计 packet error/discard 字段均为 0；这些事实不证明
  Wi-Fi、网线、驱动、TCP 或 Defender 中任何一个是唯一原因，不能把管理超时当产品回归。
- 三轮临时 artifact server 都按 owner 身份清理，`remainingListeners=[]` /
  `remainingOwned=[]`。没有改系统网络、信任或防火墙，没有创建共享/临时网络盘。
  本轮不再盲重试；优先定位持续传输路径或在用户报告新的网络状态后做 fresh 有界对照，
  再核完整新候选。专用 SSH 授权仍保留用于接续，未续期/提前撤销，任务结束仍只撤销专用行。
- 新证据：`verified-candidate-transfer-receipt.json`、`candidate-range-probe-receipt.json`、
  `verified-range-candidate-transfer-receipt.json`、`fresh-range-candidate-transfer-receipt.json`、
  `final-candidate-prefix-proof.json`、`pc3-actual-lan-interfaces.json`。失败回执和 partial 保留。
- 本次 Loom 仅更新本计划；checker tests 15/15、development manual contract、strict
  checker（1180 文件、0 违规、11 项既有软上限例外未改）、UTF-8 无 BOM 和 diff 检查通过。
  无 Loom 代码或依赖变化，不重复编译、扫描或构建相同 runtime，不把 Hook 的检查外推给 Loom。

#### 保持现有网络的 TCP 层定位与更新停点（2026-10-04 UTC）

用户明确选择“保持当前网络继续定位”。本节替代前述 110,258-byte 当前停点，保留那些
历史失败回执，不把有线切换当唯一前提，也不扩展为网卡/路由/防火墙调整授权。

- 用本地 Windows SDK `mstcpip.h` 的 `SIO_TCP_INFO / TCP_INFO_v0` 只读采集自有 socket
  状态；88-byte 结构及 loopback 收字节计数夹具通过。没有全局 TCP/offload 变更。
  第一次同内容 131,072-byte TLS 对照，两端完整 SHA 匹配；burst 接收为 13.641s，
  paced 为 2.344s。发送端分别观察到累计 12 / 1 次 timeout episode。这是有限诊断，
  不是稳定吞吐、媒体时延或性能优化收益证明。
- 基于该新证据仅做一次 paced artifact 续传：先核稳定的 110,258-byte 前缀，单请求、
  1 KiB / 10ms 发送节奏、20s socket / 500s 总 deadline、严格 TLS、Range/ETag/大小/
  SHA 门禁；不改 Hook/Loom 媒体协议或调度。约 104.062s 后 body 再次读取超时。
- 最新稳定 `hook.v0.2.32.15.ranges.part` 为 **407,218 bytes**，SHA-256
  `d144faba9ca856e93f988623a86fb378cee642120f6389c9acb83b2796081067`；
  独立停写复核的长度前后相同，与新候选同长度前缀匹配，远端无 downloader 残留。
  仍无完整 `.15.exe`，没有启动 Hook、自检、交互任务、CDP 或原生观看。
- 该 artifact 连接发送端累计 `BytesRetrans=394574`、`TimeoutEpisodes=49`，
  `Cwnd=1460`、`SndWnd=65280`；收端最后可读 `RcvWnd=65535`。实证存在 TCP 重传/
  停顿，不是 CA 拒绝或 Hook 解码错误，也没有证据归因为接收应用不读取导致零窗口。
  底层责任点仍未确定，不能直接宣称网卡、AP、网线、Defender 或 Npcap 有缺陷。
- 在 PC3 做一次只覆盖 `.20/.136`、TCP `49877` 的 packet monitor 对照：开始前 monitor
  未运行且无 filter，限定 80-byte packet prefix、8 MiB circular log，只使用上述合成
  校验数据，不记录其他应用流量、媒体或管理员凭证。第二轮 burst 在 16,384 bytes
  读取超时，paced 131,072 bytes 完整 SHA 匹配、4.953s；再次说明小成功不等于长流恢复。
- PC3 TCP/IP 组件记录包含重复累计 ACK、序列缺口和约 9.409s 的数据间隔：例如 ACK
  offset 3921 重复六次，同时观察到后续 offset 5381 的 segment。抓取未报告 stack drop，
  `EventsLost=0`、`BuffersLost=0`；这不排除抓取点之前或反向 ACK 路径的问题。
  component/SYN 覆盖有限，分析器不把未解码 NIC layout 或缺少 SYN 的流伪装成无包/无丢失。
- capture 和唯一自建 filter 已清理；fresh final status 为“数据包监视器没有运行”、
  filters 为“无”，所有自有 TLS listener/process 已退出。第一次 capture runner 因
  PowerShell 5.1 默认按 ANSI 读取 UTF-8 无 BOM 文件而误判中文 gate，未开始 capture；
  改为显式 UTF-8 读取并创建 ScriptBlock 后正常执行，未改中文源码为问号或 Unicode 转义。
- 两档 ICMP（32 bytes / DF 1472 bytes）都无回复，只能记录 ICMP 探测不可用，不能据此
  宣称 MTU blackhole。没有为使 ping 成功修改防火墙，也未更新驱动或关闭 offload。
- 新证据：`tls-transport-diagnosis-receipt.json`、`paced-candidate-transfer-receipt.json`、
  `paced-candidate-server.json`、`paced-final-prefix-proof.json`、
  `tls-transport-receiver-capture-receipt.json`、`pc3-packet-capture-evidence-excerpt.log`。
  ETL/pcapng/完整诊断仅在 PC3 独立 linshi；本机保留脱敏摘要和 owner 清理回执。
  下一步保持当前网络，继续区分正向数据/反向 ACK 路径和抓取点之前的责任，避免盲目
  重传 EXE 或改产品 codec。A3 的完整候选、正常原生发布/观看及性能验收仍未完成。
