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
| A3 | 进行中 | 两机原生基线与对照报告 | `.17` 已通过 PC3 默认 GPU 源 → 本机 Loom → 本机 viewer 的正常发布、晚加入、持续 JPEG 解码和停止清帧；包、实际进程和直连媒体已绑定。不是常驻部署或完整矩阵；仍须反方向、静态文字/滚动/运动对照、1/2/4 viewer、慢端、恢复/撤销及长稳。CPU/GPU 收益、阶段耗时和物理显示缺失须明示 |
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
- **A3 当前停点（2026-10-05 UTC）**：仍须正常入口、精确包/实际进程/CDP owner 绑定，
  再对齐 A1/A2.2；浏览器夹具和旧 EXE 不能认证新 bytes。`.17` 已通过 PC3 默认 GPU 源 →
  本机 Loom → 本机 viewer 的正式发布、晚加入、持续 JPEG 和正常停止清帧；`.18` 已关闭
  独立 Surface 事件收敛缺陷并通过单机原生刷新验收。两项都不等于后续候选的完整矩阵。
- 最新 Device 终态小块：Hook `284cbeec6c2f5b067b011ebd4a61934a5c0d088b` / `.19` 与
  Loom `a82e8cdad4bb8ff9eaf06da3d6acecf1a59f66c9` 已普通推送。有效 admitted session 的
  显式 revoke 使用 sticky provenance 和精确 Policy Close；Hook 收到后停止本地 relay、
  清帧/authority 并拒绝迟到 source recovery。expiry/nonce eviction 不冒充终态。源码、
  fresh 候选和实际验收边界见末尾最新回执；`.19` 已补齐单机原生 viewer disable/清图及
  anti-resurrection，源为显式 synthetic raw fixture，不当成两机截图性能或完整 A3 矩阵。
- 反方向 `.18` 单次 SCP 仅到 partial，未执行；失败日志保留，不盲重传。新候选的原生
  两机撤销、反方向、1/2/4 viewer、慢端、恢复、600 秒长稳和静态文字/滚动/运动对照仍待
  验收。B/C/D 仍是证据触发的有条件后续，不能以新 codec/POC 代替 A3 真正基线。
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

#### A3 接续：方向性对照与精确差分传输（2026-10-04 UTC）

恢复 Session `01a1052c-f657-75d3-93fe-987fbedf59cc` 后，fresh 专用 SSH 认证成功，
Hook/Loom 仍为上述已发布提交、工作区干净；PC3 旧 EXE partial 的长度和 SHA 未变化。
本节记录新的诊断，不关闭 A3，也不改变“保持当前网络”的授权边界。

- **观察到明显方向性差异**：PC3 向本机发送 524,288-byte 合成数据，接收端完整 SHA-256
  匹配；PC3 从开始发送到收到校验确认约 0.060s。本机计时约 2.046s，包含等待 PC3
  构造数据的时间；两者不是跨机时间相减，也不是 LiveRelay FPS 或端到端媒体延迟。
- 相反方向，普通 NAS/Linux `.201` 发送相同合成数据，PC3 约 30.894s 只收到
  97,820 bytes；PC1 经现有 IPv6 link-local 路径发送，约 30.888s 只收到
  224,640 bytes，均触发有界失败。没有修改网络、创建映射或部署常驻服务。
  这些对照削弱了“仅 PC1 Windows 发送端”或“仅 IPv4 路径”的单一解释，但不能
  单独定位 AP、驱动、接收路径或反向 ACK 的责任。
- PC1 临时 TLS socket 的默认接口与显式物理接口对照，分别在 49,152 bytes 超时、
  收完 131,072 bytes 但耗时约 24.922s；后者服务端未及时收到摘要确认，不能标为
  稳定恢复。随后 1 KiB 单在途应用确认探测在 60s 服务端期限内仅传完 208,896 bytes。
  所有参数仅作用于自有 socket，没有把这些管理传输实验写入 Hook/Loom 媒体协议。
- 本机 `pktmon` 只读状态/过滤器查询返回“无法与 PktMon 驱动程序通信。拒绝访问。”，
  因此没有源端网卡级抓包，不能声称已完成双端序列对账。未尝试提升权限或更改抓包驱动。

为减少管理传输量，另行验证了 **Windows 系统 `msdelta.dll` 的二进制差分**：

- PC3 和本机已有完整 `.10` 的 8,976,384 bytes / SHA-256
  `fb4ae2050e12552a58034eaf658b66f7afcf8244e948406b6b685c1b4c7e93d3` 均已实查。
- `.10 → .15` 补丁为 974,475 bytes，SHA-256
  `6fff3a2b35238c609df81b623dcaec0bf67a51339fc01d82604dd1c9881df482`。
  本机重建后与目标 `.15` 完整字节及 SHA 一致，损坏补丁拒绝测试通过。
  仅加载系统绝对路径 DLL；输入 SHA 固定、输出缓冲区固定 8,990,208 bytes，旧包不写入。
- 基于这个更小且已验证的产物做了一次有界 TLS 传输，约 103.593s 后读取超时。
  PC3 停写后的 `hook.10-to-15.delta.part` 为 **415,744 bytes**，SHA-256
  `ee471e1597c0195a3aadeeaa0e629109e52eae654446259d82e0beaf3c0e688d`，与本机
  补丁同长度前缀一致。剩余 558,731 bytes；不能把补丁 partial 当作完整补丁或 EXE。
- 原 407,218-byte EXE partial 和完整 `.10` 均保留。PC3 没有最终 `.15.exe`、
  Hook 进程、下载 helper 或 Issue67 交互任务；未执行差分应用、原生启动、自检或 CDP。
  本机临时监听和 helper 已退出，NAS 的一次性合成数据服务也已退出。
- 详细证据和差分工具只在 `GameEditor/linshi/issue67-a3-resume-20261004`，包括
  `delta-metadata.json`、`ack-delta-receipt.json`、`pc3-final-stable-state.json`、
  `pc3-nas-path-probe.json`、`pc3-ipv6-path-probe.json`、`reverse-path-probe.json`。
  补丁及秘密不提交仓库；本次没有产品代码、依赖或构建变化。
- 文档门禁：checker tests 15/15、strict checker 1180 文件 / 0 违规、development
  manual contract、UTF-8 无 BOM 和 `git diff --check` 通过；11 个既有软例外未改。
  临时 Python/PowerShell 工具语法检查及本机差分重建/损坏拒绝通过；没有独立子代理
  审查结果（启动返回 503），不把主线程核查称为独立审查。

下一步先解决/定位 PC3 下行持续传输，优先补源端受限抓包与收端对账，而非重复上述
对照或放宽 TLS。网络具备新证据后，可从已校验补丁前缀做一次受控续传；当前 helper
故意拒绝覆盖已有 partial，续传必须显式加入稳定前缀检查。只有完整补丁校验、重建 `.15`
并核目标 SHA 后，才进入正常配对、Surface、原生进程/CDP 和发布/受权观看验证。

#### A3 接续：双端抓包对账与 RSC 生效边界（2026-10-04 UTC）

恢复 Session `01a10693-5783-7163-8ba1-2f67730e0b36` 后，发现其最后一次执行因
上游配额错误中断，而不是抓包已完成。旧 Source 服务先于 Receiver 连接超时；旧
Receiver 的过滤器名称被系统显示截断，清理保护拒绝批量移除。fresh 检查确认监视器
未运行、仅有该任务的唯一过滤器；核对端点/端口后已清理，不影响其他抓包。

- 将 Source ready、测试 listener 和 Receiver 启动放入同一有界编排后，取得两端
  `TCP 49877` / `.20 ↔ .136` 合成流抓包。仍仅记录 80-byte prefix、8 MiB circular
  log，Source 150s watchdog 未超期；没有媒体、产品 token 或其他应用流量。
- 同一 131,072-byte 输入：burst 收到 114,688 bytes 后失败，Source 报
  `The write operation timed out`；paced 完整 SHA 匹配，两端约 1.984s，Source 收到
  摘要确认。这是管理链路诊断，不是 LiveRelay 吞吐、显示 FPS 或稳定恢复证明。
- Source NIC/TCPIP 各解析 412 条，Receiver NIC/TCPIP 各解析 269 条；每台主机两个
  抓取点的 TCP 指纹 multiset 相同。两流中 Receiver 已记录的 ACK 指纹均能在 Source
  找到，且没有零窗口 ACK；burst 的 Receiver 数据间隔最高约 10.243s。
- Receiver NIC 原始记录为 Native 802.11，但 PktMon pcapng IDB 标成 Ethernet；仅在
  明确标注的派生副本修正 link type，校验帧控制/LLC/SNAP 后解析，原始 ETL/pcapng 保留。
  burst 的 Receiver 缺少 SYN，且聚合/分段和抓取覆盖会改变记录数，不能将差额直接当成
  物理丢包率。两端日志 `EventsLost=0` / `BuffersLost=0`；Source 唯一 stack drop 是
  paced 结束时的 `INET: FIN-WAIT2`，不能用它解释此前 burst 失败。
- 这些证据继续将调查重点放在到达 Receiver 网络接收点之前或其附近的路径，但尚不能
  确定 AP、驱动、RSC 或其他环节为唯一原因；未根据诊断修改 Hook/Loom 产品代码。
- 基于配对诊断做一次保持 TLS 校验的有界差分续传，先核旧 415,744-byte 前缀，另建
  `hook.10-to-15.resume.part`，保留旧 partial。24.422s 后再次读取超时；最新稳定长度
  **624,640 bytes**，SHA-256
  `2c6196d60e75f3d1f6f573aeb7d327b794533264c535040e24632aec164edf8c`，与本机
  补丁同长度前缀一致，剩余 349,835 bytes。没有完整补丁/`.15.exe`，未应用补丁或启动 Hook。
- 用户明确允许仅对 PC3 WLAN 的 IPv4 RSC 做一次限时关闭/恢复对照。先注册 180s 独立
  恢复保险，再用 `-NoRestart` 关闭；`IPv4Enabled=false`，但
  `IPv4OperationalState=true`，故在流量测试前 fail closed，不能记为有效 RSC A/B。
  已恢复 IPv4 原值，IPv6 始终保留；独立 fresh 检查四项 Enabled/Operational 均为 true，
  WLAN Up、恢复任务为空。需要明确允许适配器短暂重启后，才继续实际生效的 RSC 对照；
  未重启适配器/电脑，未改驱动、防火墙、路由、系统信任或全局 TCP。
- 两端 capture/filter、所有本轮 listener/helper 已退出，PC3 无 Hook/Issue67 交互任务。
  专用 SSH 授权未续期，验收仍未完成，没有把本机 expiry 当作授权失效或自动撤销依据。
- 证据目录：`GameEditor/linshi/issue67-a3-dual-capture-20261004T1200Z`，关键回执为
  `paired/paired-sequence-comparison.json`、`final-prefix-proof.json`、`pc3-final-state.json`、
  `rsc-control/pc3-receipt.json`、`rsc-control/fresh-restoration-check.json`。
  临时工具的摘要/长度拒绝、元数据有界、跨机时钟独立性、NIC/stack 解析对账测试 4/4 通过。
- 本次 Loom 仅有本计划变更：checker tests 15/15、strict checker 1180 文件 / 0 违规、
  development manual contract、UTF-8 无 BOM 和 `git diff --check` 通过；11 个既有软例外
  未改。临时新增/修改工具 11–112 有效行，Python 编译与 PowerShell 语法检查通过；未
  修改依赖或构建相同二进制。独立子代理启动返回 503，无可用独立审查结论。

A3 仍未完成：下一实际产品步骤仍以完整 `.15` SHA 验证为前提，不能用上述配对抓包或
RSC 配置状态代替正常发布、受权观看、原生进程绑定和性能验收。

#### A3 接续：实际 RSC 对照及限时原生准备（2026-10-04 UTC）

恢复 Session `01a106d6-143a-7f40-b9ea-acacd10204f4` 时，发现其后半轮实际已取得新的
RSC 对照，但最后的原生准备因上游 `402 Payment Required` 中断，未进入限时验收。

- 前轮用户明确允许 WLAN 短暂重启后，IPv4 RSC 的 enabled / operational 均为 false，
  IPv6 两项仍为 true。131,072-byte 合成 burst / paced 都完整 SHA 匹配，收端分别约
  0.437s / 1.610s；恢复四项 true 后，burst 在 32,768 bytes 读取超时，paced 完整
  接收但约 18.203s。恢复和旧控制任务清理有独立回执。该有限 A/B 不是长流稳定、媒体
  性能或“RSC 是唯一原因”的证明，不能外推到完整候选或实际 JPEG 观看。
- 用户又明确允许最多 20 分钟临时关闭 IPv4 RSC，以补候选并执行一源一观看端，结束
  恢复。本次先准备本地私有 CA HTTPS 服务、正规 process framework / dashboard Art
  安装、无管理员 token 的共享 HTTPS manifest、远端小脚本 SHA 校验及测试夹具。
  LiveRelay 媒体计划直接走 LAN WSS；loopback Art handshake/control 的管理 tunnel
  与二进制媒体是独立路径。实际未启动 tunnel 或使用旧 mTLS/NLWM/PNG adapter。
- fresh PC3 专用 SSH 认证成功；稳定差分前缀仍为 624,640 bytes / 上节 SHA，WLAN Up，
  RSC 四项 true。随后先登记独立 SYSTEM 恢复保险，再关闭 IPv4 并重启 WLAN；查询确认
  IPv4 两项 false、IPv6 两项 true，holding task Running、restore task Ready。
  恢复脚本在 PC3 本地，启用电池运行，不依赖 SSH；主路径另用 monotonic 1170s 进入
  finally 恢复，独立任务目标为 1200s。调度、休眠、系统时间或驱动阻塞仍可能影响实际
  完成时间，静态保险不等于已经实查恢复成功。
- 从该稳定前缀做一次保持 TLS 校验的 349,835-byte 差分续传，发送端约 22.672s 报
  `The write operation timed out`。管理 SSH 同轮超时，随后 fresh pinned-host 连接明确
  返回 `Permission denied (publickey,password,keyboard-interactive)`；debug 确认固定
  ED25519 host key、正确专用 public key 已 offered，但服务端未接受。配置 expiry 已
  过本机时间，但两机旧时钟明显不同，不能据此断言失权原因；未自行续期或更换 key。
- 未取得远端最新 partial / 重建成功回执，不能确认完整 `.15` 已落盘；旧稳定前缀与
  新 attempt 区分，未重传、覆盖历史 partial 或执行未核候选。尚未启动本轮原生 Hook、
  注册 viewer task、调用 Ctrl+2 发布或加入观看，故 A3 仍未完成。
- 本机发现用户由 Explorer 启动的 `.15` Hook。用户允许临时正常退出后重启，但窗口
  close 仅收进托盘，退出菜单未能自动定位。未强制终止既有 main/watchdog，也未修改
  原应用数据；该现有进程不是本轮 `process-bound` 或原生验收证据。
- 本地 isolated daemon / Art Store / Caddy / transfer helper 已退出，服务 owner 与
  listener fresh 查询均为空。没有更改防火墙、系统信任、驱动、路由、全局 TCP 或创建
  新网络盘。PC3 本轮 RSC 恢复、task 清理和新 partial 核对因 SSH 失权仍待实际回执，
  不把本地清理或独立 watchdog 登记冒充远端恢复已完成。
- 本轮证据与临时工具：`GameEditor/linshi/issue67-a3-native-rsc-20261004T1330Z`；关键
  文件为 `pc3-rsc-effective.json`、`transfer-server.json`、`pc3-fresh-auth-debug.log`、
  `local-rsc-launch-clock.json`、`local-cleanup-audit.json`。临时脚本仅在 linshi；没有
  产品代码、依赖、版本或相同二进制重建。本次文档未提交或推送。
- 本机记录的 holding launch receipt 为 `13:53:05.6600168Z`；等到设定的 20 分钟
  期限之后，于 `14:14:52.3221691Z` 再做一次只读核验，仍为 SSH exit 255 / 上述
  `Permission denied`。因此实际恢复状态和远端自有任务清理仍是 **未验证**，不是
  “保险登记即恢复成功”。恢复脚本保留在 PC3 本地，不依赖已清理的本机服务。
- 临时传输边界测试 4/4、checker tests 15/15、strict checker 1180 文件 / 0 违规、
  development manual contract、PowerShell/Python/Node 语法、UTF-8 无 BOM、两仓库
  diff check 均通过。临时工具由语言感知 lexer 计数为 16–139 有效行，无软上限例外；
  一次只读独立审查确认恢复保险及其调度/休眠/驱动阻塞边界，没有实际恢复通过结论。

下一个闭环先恢复同一专用管理访问并实查 RSC / restore receipt / 自有任务状态，按
owner 清理；然后核新 partial / 完整候选 SHA。只有这些完成后才重新安排明确授权的
限时原生发布/观看，不凭本次管理诊断宣称产品、性能或原生 LAN 验收通过。

#### A3 接续：同钥续授权工具与 fresh 认证停点（2026-10-04 UTC）

用户报告 PC3 应已给予访问权限后，重新通过固定 host key、同一专用 key、源地址
`192.168.15.20` 的物理 SSH 入口验证；最新 `14:52:16.5374821Z` 仍为 exit 255 /
`Permission denied (publickey,password,keyboard-interactive)`。debug 确认原 ED25519
host key、专用公钥已 offered，服务端未接受；远端只读检查没有执行。

- 检查旧授权 ZIP 发现其授权行固定 `expiry-time="20261004062139Z"`，脚本既不拒绝
  过期条目，也不在重复授权时延长 expiry，却仍显示成功。这是已确认的工具缺陷，
  可解释一种“已操作授权但仍被拒绝”的情形；没有远端时钟/授权文件/sshd 日志，
  不能将其确认为本次拒绝的唯一原因。
- 仅在 linshi 制作同一专用公钥的续授权包，从 PC3 实际执行时的 UTC 起算两小时。
  保留原源地址、禁 agent / X11 / PTY、CODE/mjc 与固定 host identity 边界；不换 key，
  不修改 SSH 配置、防火墙、网络、系统时间、RSC 或 Hook。
- 新工具原子替换符合原工具 ownership/options 的同钥行，保留其他文本、owner /
  group / DACL，并留 rollback。只接受已有受保护 DACL；未保护的继承 ACL 在任何
  staging/内容写入前拒绝，绝不通过自动改变目标权限来让测试通过。
- 源码和 ZIP 解压后的自测均 12/12 通过，覆盖过期同钥替换、其他授权保留、保护
  DACL/备份、幂等 grant/revoke、异常选项及 tab 分隔同钥拒绝、锁定失败保留和
  执行时两小时 expiry。源码/解压脚本 parser、UTF-8 无 BOM、白名单与 SHA 核对通过。
  临时源码有效行数均低于 100，无软例外；包只含七个公开文件，不含私钥、测试
  夹具、运行回执或 rollback。该自测不是 PC3 授权、UAC 或 SSH 恢复通过。
- 新包为 `GameEditor/linshi/issue67-pc3-access-renewal-20261004/` 下的
  `PC3-SameKey-Renewal-20261004T145112Z.zip`，SHA-256
  `1bf1c10f08bec38ee06eaae67d70d15869671bfda898c3aab4cf927fdc64e8cf`。
  尚未在 PC3 执行；需要在 PC3 本地解压并运行 `PC3-Authorize.cmd` 后 fresh 验证。
- 本机原 Hook main/watchdog 仍为 PID 62792 / 37328，未强制结束、未重启、未动原
  数据。本次没有启动测试 Hook 或发送 LiveRelay 媒体。按原 PID/creation owner 与
  实际 49873-49877 端口 fresh 核对，本机上一轮服务/transfer helper/listener 均为空。
- 证据保留在同一 renewal linshi 目录：`PACKAGE-20261004T145112Z.json`、两份 selftest
  回执、`temporary-code-lines.json`、`pc3-fresh-readonly-20261004T145215Z.log`、
  `local-final-owner-audit.json`。产品代码、版本、依赖和包没有变化，文档未提交/推送。

A3 仍未完成。访问恢复后必须先核 PC3 实际 RSC 恢复、自有 restore/holding 任务与
最新 partial/完整候选 SHA，再进入受授权的原生配对、发布、受权观看及性能验收。

#### A3 管理入口：用户授权 30 天及开机自动续期（2026-10-04 UTC）

用户明确要求取消每天手动续授权，改为至少 30 天且每次开机自动配置。本次仅制作
同一 PC3 公钥的独立授权包；没有扩大到其他主机、其他来源、其他公钥或网络配置。

- 一次本地 UAC 安装后立即由 SYSTEM 任务授权，后续每次开机延迟一分钟刷新为
  PC3 执行 UTC 加 30 天和一分钟舍入缓冲。属于持续滚动续期，不是安装满 30 天撤销；
  只设开机触发，没有每日触发，连续运行超过期限未重启时仍会到期。
- 可见任务 `Neuro-PC3-DedicatedKey-30DayRenewal` 固定执行
  `C:\ProgramData\NeuroPc3Access` 的受保护脚本；代码、manifest、控制状态和结果
  仅 SYSTEM/Administrators 可写，拒绝 reparse 路径和未知同名任务。不保存密码或私钥。
- 安装器核对 mjc 的直接管理员成员身份及既有 `sshd.exe -T -C` 有效授权文件配置。
  通过当前 generation 的新 SYSTEM 回执确认实际授权操作，不拿旧结果冒充本轮成功。
  撤销先在共享 key mutex 下发布 false；已排队 worker 之后只能撤销，成功后才禁用、
  注销自有任务。控制/结果 JSON 原子发布，固定 initial/previous/pending 备份有界。
- 源码和 ZIP 解压后的自测均 **32/32** 通过；Windows Task Scheduler COM 原生
  `NewTask(0)/XmlText` 解析通过，未注册任务。本机错误主机实际安装入口拒绝，未请求
  UAC、未创建 ProgramData 安装目录或任务。parser、UTF-8 无 BOM、白名单及摘要通过；
  临时源码有效行数均不超过 125，无软例外。静态独立审查不等于 SYSTEM/重启验收。
- 包为 `GameEditor/linshi/issue67-pc3-access-autostart-20261004/` 下的
  `PC3-30Day-AutoAccess-20261004T153933Z.zip`，SHA-256
  `8fbc118302f2e6f155fb82ac4d2d9b79ab227d2e5a4ec5f853aa0e958e7af23e`。
  15 个公开文件，安装/撤销/状态入口随包提供；不含测试产物或 rollback。
- 最新 `15:46:00.4790707Z` 只读 PC3 预检仍为 SSH exit 255 / permission denied；
  尚未在 PC3 安装，不能宣称任务、30 天授权、真实开机续期、撤销或 SSH 已通过。
  第一次需 PC3 本地解压运行 `PC3-Install-30Day.cmd`，之后不需每日手动配置。
- 本次不改产品代码、版本、依赖或发布包，不启停原 Hook，不改 RSC/SSH 服务/防火墙，
  没有提交或推送。访问恢复后仍按上一停点实查 RSC、自有任务及候选，再续原生 A3。

#### A3 管理入口 Fix1：首次安装任务缺席错误（2026-10-04 UTC）

用户报告运行 `PC3-Install-30Day.cmd` 时 `StartupTask.ps1` 报错，尚未提供具体错误
原文。已复现旧版在根任务目录为空时将 CIM `ObjectNotFound` 当作安装失败；这是
已确认的缺陷，但是否就是用户遇到的错误仍待核对，不能把本地复现当作唯一确诊。

- Fix1 直接查询固定 TaskName / TaskPath，只吞确切的 CIM NotFound；权限不足、
  Scheduler 不可用等错误仍上抛。只读原生不存在任务查询及三类错误夹具均通过。
- 保持原公钥、来源、30 天开机续期与安全边界。只接纳原交付 v1 的固定 manifest
  及逐文件 hash / ACL，更新 `StartupTask.ps1` 和 manifest，其他 runtime 不变；
  在受保护 `state/query-repair-v1` 留原文件快照，不需删除旧安装目录。
  两文件分别原子替换；精确的新脚本 / 旧 manifest 状态只有在旧快照仍匹配时才可
  重跑恢复，不宣称两文件整体事务原子。未知编辑或快照拒绝覆盖。
- 安装失败回执增加 FQID、category、position、stack trace，供核对用户实际错误。
  源码和 ZIP 解压后的公开自测均 **36/36**；另以原交付 ZIP 为真实字节夹具完成
  已知旧版升级、owner/group/DACL 保留、幂等、半提交恢复、未知编辑保护及其他
  runtime 不变的 **6/6** 验证。测试均在 linshi，不注册真实任务，不修改本机安装。
- 新包为 `GameEditor/linshi/issue67-pc3-access-autostart-fix-20261004/` 下的
  `PC3-30Day-AutoAccess-Fix1-20261004T162741Z.zip`，SHA-256
  `5935ce05b4ee726895ad080664f133551ea6be6bef5978ae133513eabf971615`。
  18 个公开文件，不含私钥、旧 ZIP 夹具、修复专项测试或运行产物。parser、UTF-8
  无 BOM、解压逐文件摘要和 ZIP 白名单验证通过，临时源码均不超过 128 有效行。
- 最新 `16:29:18.7786044Z` 固定 host key / 同钥 / 来源的只读连接仍为 SSH exit 255：
  `Permission denied (publickey,password,keyboard-interactive)`。远端安装错误回执未能
  读取，真实 PC3 安装、SYSTEM 操作、开机续期、撤销和 SSH 恢复仍未验收。
- 本次不修改产品源码、依赖、版本或二进制，不启停原 Hook，不改 RSC/SSH 配置/
  防火墙，不提交或推送。原 A3 原生及性能验收仍以实际访问恢复后核查为前提。

#### A3 管理入口 Fix2：中文 ANSI 解码的语法错误（2026-10-04 UTC）

用户再次报告 Fix1 的 `StartupTask.ps1` 为符号报错。此次按真实字节复现编码路径：
Fix1 的 UTF-8 无 BOM 源码按 GBK 936 解码后有 **7 个 parser 错误**，包括
`Unexpected token '}'` / `Missing closing '}'`。原 117 个按 LF 分割的片段变成 114 个；
中文单行注释尾部未配对字节吞掉 LF，下一行 `if` / `while` 并入注释，破坏括号结构。
开发机默认 UTF-8，旧自测漏掉这一 Windows PowerShell 5.1 中文区域读取路径。

- 保留中文和 UTF-8 无 BOM，将所有脚本的中文单行注释改成有 ASCII 闭合边界的
  `<# ... #>` 块注释；没有把中文改成问号或转义，没有要求修改系统区域或系统编码。
  `StartupTask.ps1` 的 here-string 闭合、命令参数引号及任务业务代码保持不变。
- 新增 UTF-8 / GBK 936 / Windows-1252 逐脚本 parser 与非注释 token 等价验证；
  实际加载 GBK 解码的 StartupTask 并调用函数生成、验证任务 XML。原生 PowerShell
  5.1 的 `-File StartupTask.ps1` 也正常退出；这不是 PC3 注册或 SYSTEM 操作证明。
- 安装支持摘要固定的原交付 v1 和 Fix1 runtime 升级，公钥及任务执行路径不变。
  全部文件、ACL 和快照先预检；逐文件原子替换，最后提交 manifest；已更新子集
  只有具备原始快照才可续跑。快照按原 manifest 摘要分目录，保留旧 Fix1 快照。
  未知文件、未知快照或缺少快照的新文件拒绝，测试确认拒绝前不修改 runtime。
- 源码及 ZIP 解压公开自测 **40/40**，两原交付 ZIP 字节夹具的升级专项 **9/9**，
  覆盖两版本升级、全部 owner/group/DACL 与快照保留、幂等、多文件已更新子集恢复、
  未知编辑/快照/缺快照保护、runtime 可执行代码及公钥保留。无真实任务注册。
- 新包为 `GameEditor/linshi/issue67-pc3-access-autostart-fix2-20261004/` 下的
  `PC3-30Day-AutoAccess-Fix2-20261004T165439Z.zip`，SHA-256
  `ad0132462adde5aacc8e528bf81ad7a7f7586ab3a34ce1073f0f1c2b2d75dcbe`。
  20 个公开文件，不含私钥、两个旧 ZIP 夹具或运行产物。parser、UTF-8 无 BOM、
  解压逐文件摘要及白名单均通过；临时源码均不超过 128 有效行，无软例外。
- 未收到用户具体错误原文或 PC3 回执，故能确认此编码缺陷及本地修复，不能确认
  用户错误唯一归因、真实中文区域 PC3 安装、SYSTEM 授权、开机续期或 SSH 已通过。
  本次仍不修改产品源码/依赖/二进制、SSH/RSC/防火墙或原 Hook，不提交或推送。

#### A3 管理入口 Fix3：任务语义误判及 worker 失败回执（2026-10-04 UTC）

用户确认 Fix2 语法错误已解决，但运行时仍无法授权，提到错误含 `/t...`，没有完整
报错或回执。本轮固定 host key / 同钥的只读 SSH 仍为 permission denied，未能读取
PC3 的实际任务或授权结果。历史文件中也没有可用于本轮归因的目标 ACL / effective
sshd 配置回执；未通过猜测重设 ACL、放宽来源或修改 SSH 配置。

- 旧 `Assert-OwnedAccessTaskXml` 逐字比较会输出 `/t:Task/...`。原生 Task Scheduler
  未注册定义证实：SYSTEM 名称与 SID、XML 布尔 `0` 与 `false`、ISO duration `PT60S`
  与 `PT1M`、省略默认 false 的 Hidden 均为合法等价表示，但旧 guard 会误拒绝。
  这是已复现的代码缺陷，不等于已取得 PC3 的具体失败字段或唯一根因。
- Fix3 原生解析/规范化任务定义，按真实 SID、布尔值、duration 和固定 Windows
  绝对路径语义校验；保留精确参数、SYSTEM、最高权限、单一开机触发、retry/time
  limit 和 marker 边界。改变安全值、非SYSTEM、额外 action、非法 schema、DTD
  均拒绝。定向只读复审指出 BootTrigger 内额外重复/日期约束的覆盖缺口；主线程
  用原生定义复现带 PT1M Repetition 的触发器被旧 guard 接受，已补拒绝重复、日期
  边界和额外非零 per-trigger limit 的校验及反例。COM 定义只读且释放，不注册任务。
- 安装窗口/回执显示 stage 与简短字段名；worker 在可信 runtime / 状态通过后，
  在共享锁下先读取 generation，再验证主机/SYSTEM/SSH/目标权限，前置失败可写
  属于本轮请求的失败回执。新增 stage/FQID/position/stack trace；没有新成功回执
  绝不报告已授权。无回执超时另报告实际 LastTaskResult，不跳过保护条件。
- 源码与 ZIP 解压后的公开自测 **48/48**，原交付 v1/Fix1/Fix2 的升级专项 **10/10**。
  已实际在独立受保护夹具执行生产 worker：只适配测试路径/owner，不模拟 SYSTEM，
  错误主机拒绝前能保存本轮 generation、真实身份、阶段与调用栈，授权 staging
  为空。这验证失败链路，不是 SYSTEM 成功授权或真实开机验收。
- 最终包为 `GameEditor/linshi/issue67-pc3-access-autostart-fix3-20261004/` 下的
  `PC3-30Day-AutoAccess-Fix3-20261004T175356Z.zip`，SHA-256
  `b0711355b95ab1fa0dd6f0cc1d796a3f08ff347f6ee5c02bcb460593ea80875c`。
  23 个公开文件，不含私钥、三个旧 ZIP 夹具或运行产物。支持确切旧版本保留快照
  升级，无需清空安装目录；未知改动仍拒绝。parser、三种解码、UTF-8 无 BOM、
  解压摘要及白名单均通过；临时源码最高 162 有效行，无软例外。
- 未在真实 PC3 注册任务、SYSTEM 授权、重启或独立 SSH 验收。不改产品源码/依赖/
  二进制，不启停原 Hook，不修改 RSC/防火墙/SSH 配置，不提交或推送。

#### A3 回到原生验收：候选完整性关闭，正常入口仍未通过（2026-10-04 UTC）

用户要求回到 Hook / Loom 核心测试。本节是新的实机证据，不改写上面授权失效时的历史状态。

- fresh 固定主机/同钥 SSH 已成功；PC3 的 Fix3 安装及 SYSTEM 授权回执成功，开机续期任务
  Ready / LastTaskResult=0。真实重启续期尚未测试，用户要求的滚动授权保留，不在验收后撤销。
- PC3 WLAN Up，IPv4/IPv6 RSC 的 enabled/operational 四项均 true；另读取上一轮本地恢复
  回执。原 restore task 已移除，按 action 精确核对后清理剩余的旧 holding task。本轮不改 RSC。
- 远端旧差分已稳定到 870,400 bytes，SHA 与本机同长度前缀匹配。保留旧 partial，在新目录
  一次续传剩余 104,075 bytes，38.782s 完成；974,475-byte 差分完整 SHA 匹配。仅调用系统
  msdelta 重建并再次核验完整 .15：8,990,208 bytes / 上节 02fcca8d... SHA。未重新传整个 EXE。
- 用户既有 Hook PID 62792 通过正式原生托盘菜单“退出”正常退出：本轮读取实际菜单文本与
  动态 numeric ID 后走 muda / Hook app.exit，不猜 ID、不 force kill。两端隔离 .15 实际启动，
  Active Console Session 1，EXE SHA、PID/creation/path 和 WebView CDP 父链均核对；源端重启后
  另核新父链。其他既有 Loom 实例不动。
- 本机测试进程最初继承终端 HTTP_PROXY，LAN 配对请求失败。只为隔离测试进程设置固定
  LAN 地址的 NO_PROXY 并重启；随后两设备均通过共同 HTTPS 的正常配对。没有修改全局代理、
  Windows 根信任、防火墙或产品配置。SSH tunnel 只用于 loopback Art 管理和 CDP，不转发媒体。
- 实际正常入口门禁仍失败：旧 loopback setup 的 workflow instantiate 返回 409
  no_hook_client，观看端无 Art/Surface。源码确认 remote manifest 选择 Surface poll listener，
  而不是旧 workflow WS listener；仅有 extension clients 不等于可接收 workflow 的 Hook。
  不能将这个 setup 路径直接当作远端正常 Art 创建入口，也不以伪造 Surface 绕过。
- 源端 Hook 被最小化时 viewport 位于负坐标；恢复自有窗口后得到真实 region streaming，
  但原窗口选择夹具未取得预期 sourceWindowId，另一次正常双击也未建立预期窗口捕获。
  两次失败均留回执；尚未唯一归因于产品、焦点、DPI 或夹具。未正常发布、加入观看或传送
  LiveRelay 媒体，因此没有 JPEG viewer、物理显示、FPS、资源或长稳通过结论。
- 隔离 Hook 按自身有界生命周期退出，fixture、services 和 management tunnel 已清理；fresh
  两端查询确认测试 listener/PC3 Hook/Issue67 task 均为空，RSC 四项仍 true。按原路径重新启动
  用户 Hook，main PID 60376 / Session 1，SHA 不变；没有手工改写原应用数据或停止既有 Loom。
- 证据目录为 GameEditor/linshi/issue67-a3-native-return-20261004T183749Z；关键回执包括
  delta-continuation.json、pc3-candidate-reconstructed.json、两端 process-binding、
  source-window-publication.json、pc3-final-state-clean.json、management-cleanup-final.json
  和 final-native-receipt.json。临时脚本 parser/Node syntax/Python compile 通过，最大 139
  有效行；本轮没有产品源码、依赖、版本或二进制改动，没有提交或推送。

下一小闭环：先按实际远端产品入口创建并挂载 Surface-capable Art，并隔离原生窗口选择的
焦点/DPI/geometry 证据；然后只验一个源/一个观看端的发布、加入、帧推进及停止。不再重复
授权开发、整包传输或已经关闭的完整候选检查。A3 仍为进行中。

定向只读源码核验补充：空画布的 globalAddNodeMenu 只有声明，没有实际 consumer；当前
正常添加 Art UI 需要先选中一个既有 Unit，使用已配置的 toggle-actions（默认 Shift+1，
不是 Ctrl+E）打开 Actions Menu，选择“设备仪表板”。spawnConnectedNode 从真实 Unit
创建 Art，unitSurfaceController 对支持 Surface 的 Art 自动调用 attachSurface；dashboard
inputs 为空，既有 Unit 是当前 UI 入口要求，不是该 Art 的输入语义要求。下一夹具复用这条
现有入口，不新增“测试专用 Surface”或伪造 attachment。该核验为源码结果，尚未实际跑通。

#### A3 PC3 正常观看入口实测通过；源端退出停点（2026-10-04 UTC）

- 复用完整 .15，不重新授权、传包或构建。PC3 fresh SHA 一致，隔离 main PID 2092 /
  creation 19:27:07.2195620Z / Session 1；19:30 左右实际核过 49880 WebView listener
  PID 14772 到 Hook 2092 的父链。末次 fresh 绑定查询时 1800s 生命周期已结束，不能
  把那次失败写成仍运行；先前父链来自本轮实际工具输出，不是上一实例的绑定。
- PC3 正式托盘截图菜单与 OS 指针框选创建真实 656×406 Unit；Shift+1 打开 Actions，
  选择 canonical 设备仪表板，真实 Art 自动挂载 Surface。无 Mock IPC、graphStore 注入
  或伪造 attachment。正常“刷新”使 Surface revision 1→4，参数面板的“实时投射观看”可见。
- 本机只读 GET 对照实际 daemon 的 instance、attachment、hookNodeId 和 deviceId 成功
  （HTTP 200）。夹具先前两处失败已区分：Art 按钮名包含 ❖ 图标，exact-name selector
  不适用；对 instance: ID 使用 encodeURIComponent 使当前 path_id 查找 404。只修临时
  locator 和经严格 ID 校验的 URL，没有修改产品或放宽权限。revision 推进不是媒体帧推进。
- 本机原 Hook 60376 的日志已记录 tray_quit、tauri_exit_requested 和
  hook_process_exit_cleanup，但未记录 tauri_exit / tauri_run_returned，进程仍在。
  已证自身 relay/capture/input 清理阶段返回，不能把无超时 join 猜成当前根因。正式菜单
  popup 已关闭，tray WM_NULL 仍响应；实际更深阻点尚未确诊。未强杀原实例，已询问单独许可。
- 因源端单实例锁未释放，本轮未启动隔离源、未正常发布/加入，也未传送 LiveRelay 媒体。
  没有 JPEG decoded-submitted、物理显示、FPS、CPU/GPU 或长稳通过结论。A3 仍进行中。
- 本轮 root 为 GameEditor/linshi/issue67-a3-native-ui-20261004T192234Z，核心回执为
  viewer-normal-entry.json、viewer-surface-check.json、original-menu-responsiveness.json、
  management-cleanup.json、pc3-final-state.json。观看实例按生命周期结束，fixture 正常关闭；
  四个精确核 owner/action 的本轮任务已移除，两端测试 listeners 为空，PC3 RSC 四项仍 true，
  持久授权保留。原 Hook 60376/专属 watchdog 15600 未终止，不能宣称已完成原程序恢复。

下一动作：取得卡住原实例的处置许可后按原路径恢复并重新绑定新的隔离源/观看端，只跑
一个源/一个观看端的正常发布、加入、JPEG 帧推进及停止。不要重做授权和候选完整性准备。

#### A3 原生 JPEG 推进通过，停止失效失败（2026-10-05 UTC，修复前历史）

本节更新前面的历史退出停点，**A3 仍进行中，不是无错误的完整闭环验收**。

- Fresh 原 Hook 已换成 PID 59536 / creation 01:03:27.4999930Z。正式托盘菜单归属核对
  后，键盘尝试因 foreground 转至 Edge 中止，没有盲发 Enter；关闭菜单再发送动态读取的
  正式 Quit menu event，01:23:05 回执确认正常退出，无强制终止或绕过单实例锁。
- 复用 Hook v0.2.32.15 / SHA 02fcca8dc75a486500ae6cb928d5e8de9c3df079cc21928ca36d3c0043c467b0
  及既有 Loom daemon。本机新 Hook 33936 / creation 01:25:02.7745390Z，CDP listener
  36008 → 33936；PC3 Hook 6712 / creation 01:24:56.0836750Z，CDP listener 2404 → 6712。
  两端 Session 1、实际路径/SHA/creation/父链均通过并在媒体期间 fresh 复核。
- 本机正常窗口内框选已绑定自有 fixture，但创建捕获返回 live_resource_memory_pressure。
  实测本机可用物理内存约 1–1.5 GiB，低于现有总内存 10% 的保留预算；未终止非本轮
  rustc/其他进程，未放宽门禁。改用 **PC3 源 → 本机 Loom → 本机观看端**，不是原计划的
  本机源 → PC3 收端。反方向和两端独立服务器拓扑仍未验收。
- PC3 正式实时截图菜单、OS 指针框选得到 window/window_message 源，HWND 与该轮
  fixture 一致，656×406；正常“发布到 Loom”通过。本机真实截图 Unit → Actions →
  canonical dashboard → Surface → 参数面板刷新/选择/明确“加入观看”通过。
- 实际 PC3 Hook 的已建立 socket 为 192.168.15.136 → 192.168.15.20:49874，收端 native
  networkScope 为 private_https。SSH 只管理 CDP 和 loopback Art bridge，没有转发媒体，
  没有 mock IPC、graphStore 注入或 PNG adapter，也没有取得 controller/remoteControl 权。
- 收端同源/session/epoch、包版本与实际进程绑定匹配；JPEG decoded-submitted 帧号在
  约 4 秒观察内 3801→3872，实际 img complete 且 natural size 为 656×406。发送 A1
  指纹 427f9fba445c25ab3b53645904f81bd2 与收端源一致；8 秒/16 样本的可比较窗口 7582ms
  中 published/forwarded 均增加 126，binary bytes 增加 2670835，failedWrites 和
  viewerSkippedFrames 差分均为 0。这是 socket-write 采样，不是物理显示 FPS；未校准
  跨机时钟，不计算端到端帧龄或 CPU/GPU 收益。
- WebView 截图的人工复核还显示观看内容位于 viewport 边缘且被裁切，dashboard 上出现
  Surface event dispatch failed。后者的跨 attachment/共享管理上下文原因尚未确诊，
  前者尚未按当前坐标/布局做归因；不能将已解码和帧推进扩张成完整可见布局或整个 Art
  运行无错误的通过结论。这两项保留在 visual-review.json，不为此隐藏错误或改产品。
- **失败 1**：加入后 control_history_reset，媒体仍推进且没有输入权。Hook
  native/live_relay_commands.rs 的新 viewer event_cursor 固定为 0；Loom
  runtime/live_session_events.rs 在 after+1 < oldest 时明确返回 reset，Hook control
  worker 按 reset 失效/撤销。此机制足以解释晚加入后的告警，但尚未完成 bootstrap
  focused regression，不应简单忽略 reset 或跳过真实缺失的控制事件。
- **失败 2**：正常停止发布后 daemon 已 closed=true/sourceConnected=false，但收端
  30 秒内未失效；01:43:53 最终仍 recovering/control_poll_failed，保留 JPEG frame 7130。
  Hook native/live_relay_websocket.rs 的 viewer resume 失败走重试分支，control poll
  失败也只报错重试，未区分永久关闭与暂时断线；frontend 对 recovering 仍保留已提交帧。
  下一修复应识别并传播权威 terminal 状态，不得为了本次测试把所有 recovering 都当 closed。
- 夹具问题保留独立失败回执：首次 Ctrl+2 在 readiness 期间被非本轮物理输入取消；改用
  正式菜单及 fresh DOM/log readiness。后续 live: ID 的 colon URL assertion 和包版本
  v 前缀断言已修，仅重读既有 relay，没有重复发布/加入来制造通过。创建 Art 不会自动
  选中，正常点击 refresh 后再开参数面板；这些均未修改产品代码。
- 两端自有 fixture 正常关闭；PC3 测试 Hook 正常退出，5 个核对完整 action/user/root 的
  临时任务已移除。隔离 Hook/daemon/store/Caddy 和 management tunnel 均已清理，测试
  listeners 为空；RSC IPv4/IPv6 enabled/operational 仍四项 true，持久 SSH 授权未改。
  原 Hook 按原 EXE/hash 重启为 53260 / creation 01:49:47.7301850Z，Session 1，WebView2
  和专属 watchdog 子进程已启动。清理曾发现 RTK wrapper 提前退出而 SSH 子进程仍在，
  精确复核 parent/creation/完整命令后仅停止本轮 management tunnel，不全局杀 SSH。
- 证据根：GameEditor/linshi/issue67-a3-native-media-20261005T011121Z。总回执
  final-native-receipt.json 为 partial_failed_stop_invalidation / a3Complete=false；核心
  文件为 reversed-observe-receipt.json、sender-measurement.json、stopped-viewer-final-state.json、
  pc3-native-binding.json、pc3-direct-media-sockets.json、original-restored.json 和
  pc3-pc3-final-state.json。原始设备/会话身份与截图仅留受信本地证据，不发布到远端。

下一项只修复/验证 **terminal closure 传播和 late-join control bootstrap**，复测同一最小
闭环后再讨论性能优化、多 viewer 或长稳；不重做授权、候选传输和已完成的独立入口门禁。

#### A3 第一轮修复与 UIA 晚加入遗漏（2026-10-05 UTC）

- Hook `d169be802f6cd60a2ec6cab79b42367dda8d8a4b` 已提交并推送 main，内部迭代
  `v0.2.32.16`。viewer 使用同次 attach revision 对应的 `viewer_joined` 作为控制游标
  锚点；只跳过加入前历史，锚点丢失或加入后缺事件仍拒绝。控制轮询失败后仅通过已鉴权、
  session/epoch/member 匹配且明确 `closed=true` 的快照确认终止，不把断网或普通 404 当关闭。
  终止清帧与接收帧统一 state → frames 锁序，迟到连接/控制回调不复活已停止 viewer。
- 聚焦回归先证明停止后回调会把 closed 改回 recovering，再验证修复。该提交 LiveRelay
  Rust 19 passed / 3 ignored，前端 4 文件 54 tests、类型检查、lint、编译/格式、严格行数通过。
  本轮真实 OSV Enforce 扫描 4 lockfiles / 1654 packages / 0 未抑制 ID，19 既有受控例外，
  未新增依赖或豁免。独立只读代理因 503 无结果，不冒称完成独立评审。
- 第一候选 `release/Hook/v0.2.32.16/issue67-viewer-lifecycle-20261005/hook.exe`，
  SHA `64f124f49220e1f330baeb6efb68ac696a53869c096970ef92f2003408e5e7ba`，8995328 bytes，
  clean-source provenance 与 self-check 通过。该候选保留，但不作为晚加入验收通过包。
- 实机仍为 PC3 `.15` 源 → 既有 Loom → 本机 `.16` viewer。两端 Session 1、路径、SHA、
  creation 和 CDP 父链已绑定；正式菜单框选、发布、真实 Surface 参数面板加入及首帧 JPEG
  已解码。没有媒体 SSH 转发，没有输入权。**停止发布后失效通过**：daemon closed，native
  closed / errorCode=null / controllerOwned=false，presentation=null，remainingImages=0。
- 晚加入暴露另一遗漏：`control_event_rejected` / `live observation sequence must be exactly 1`。
  attach 已跳过历史，但 runtime observations 仍为空，而源端 UIA sequence 已达数百。
  此外 `.15` 源 capture/lastFrameAtMs 与 UIA 继续变化，JPEG frameId 停在 4；GPU 采集路径
  原因未确诊，不声称已修。下一轮只用既有 `HOOK_LIVE_GPU_PREVIEW=0` 隔离 JPEG 兼容路径。
- 两端测试实例/fixture、服务、管理 tunnel、临时任务均已清理；PC3 RSC 四项 true，持久
  SSH 授权保留。本机原 `.15` 恢复为 PID 54132 / creation 02:39:06.3566620Z。
  原始证据在 `GameEditor/linshi/issue67-a3-native-fixed-20261005`，包括失败的
  `reversed-join-receipt.json`、`fresh-relay-inspection.json`、通过的 stop/closed 回执及清理回执。
- Hook 追加提交 `a40f0f20cc8c423fcbe9948dbe91e7e2a80831c3` 已推送 main：启动 worker 前从
  同一 attach snapshot 初始化观察 map 与摘要。严格拒绝缺失/非法/重复/超过 256 项的快照，
  复用观察 payload 验证；后续仍要求每个观察的 sequence 连续，不接纳任意高序号。
  回归覆盖 sequence 501 → 502、后续 gap/重复/new-ID 非 1 拒绝、正常 503，以及非法边界。
  最新 LiveRelay 21 passed / 3 ignored；cargo check --all-targets、显式 include rustfmt、
  checker tests 53/53、strict 1380 文件无 >500、UTF-8/BOM 与 diff check 通过。
  实质源码有效行：viewer_state 179、commands 401、lifecycle tests 253，未触及旧债上限。

#### A3 晚加入与停止清帧最小闭环通过（2026-10-05 UTC，最新停点）

本节仅完成上轮要求修复的两个真实缺陷，不将整个 Issue #67 或 A3 矩阵标为完成。

- 新不可变包：`release/Hook/v0.2.32.16/issue67-observation-bootstrap-20261005/hook.exe`，
  8994816 bytes，SHA `87656dad5237af0f8d4f7c20816df3edf9f35345836e394d1e5e0f55c1376e73`。
  fresh production/release 构建完成，provenance 绑定 `a40f0f2`、gitDirty=false、internal 通道；
  exact SHA/product-version self-check 通过。同一未完成迭代只重建，不再次分配 revision；旧包未覆盖。
- 拓扑为 PC3 `.15` 源 → 既有 Loom daemon/private-CA HTTPS → 本机 `.16` viewer。证书到期
  09:24:42Z，启动时 fresh 确认仍有效；未修改系统信任、防火墙、网络驱动或持久 SSH 授权。
  PC3 隔离进程显式 `HOOK_LIVE_GPU_PREVIEW=0`；这是既有 JPEG 兼容路径，不是默认 GPU 路径验收。
- 两端路径/SHA/creation/Session 1/CDP 父链在媒体期间复核：PC3 Hook 13932 / creation
  02:53:23.6226520Z，CDP listener 16740 → 13932；本机 53596 / creation
  02:58:11.6585240Z，CDP listener 39588 → 53596。PC3 实际已建立连接为
  192.168.15.136 → 192.168.15.20:49874；SSH 仅管理 CDP/Art bridge，不转发媒体。
- 正式菜单与 OS 框选绑定自有 window/window_message fixture，656×406，正常发布。
  本机正常截图 Unit → Actions → 设备仪表板真实 Surface → 参数面板显式加入。
  加入前 15 项 observations，最大 sequence=185、累计 sequence=2775，超过 256 条控制历史。
  viewer 接续 UIA 至 188，无 control_history_reset / control_event_rejected；没有取得输入权。
- 同一次 join 的 JPEG decoded-submitted 在约 4 秒内 frameId 5486 → 5557，img complete、
  natural size 656×406；native connected / errorCode=null / reconnectCount=0 / private_https。
  本轮截图可见完整观看区域；这只是原生显示及推进证据，不换算物理 FPS 或跨机端到端帧龄。
- 同一会话正常“停止发布”后，daemon closed=true/sourceConnected=false；viewer native closed，
  errorCode=null、controllerOwned=false、observationState=closed、presentation=null、
  remainingImages=0。至停止共 receivedFrames=1050，无重连；截图确认旧 JPEG 已移除。
  关闭态仍显示已标记关闭的 UIA 摘要/框线，不把它们称为实时数据。
- 独立 Surface 在结束截图再次出现 `Surface event dispatch failed`，原始失败保留，
  未因 LiveRelay gate 通过就声称整个 Art 无错误。默认 GPU JPEG 停滞也仍是独立待定位项。
- 清理夹具曾因远端 `Close-Fixture.ps1` 默认 source、调用遗漏 `-Role viewer` 中断；已定位
  并用精确路径/创建时间归属的交互清理任务正常关闭两端 fixture 与 Hook，不强杀原程序。
  原验收任务与补救清理任务按 action/user/root 核对后均移除；两端测试监听为空，PC3 无残留
  Hook，RSC 四项 true。本机原 `.15` 已恢复为 49900 / creation 03:03:22.1020330Z。
- 证据根：`GameEditor/linshi/issue67-a3-observation-fixed-20261005`。核心文件是
  `reversed-join-receipt.json`、`reversed-stop-receipt.json`、`reversed-closed-receipt.json`、
  `source-direct/binding.json`、`pc3-final-binding.json`、`pc3-direct-media-sockets.json`、
  `headless-smoke/headless-summary.json`、`original-restored.json` 与两端清理回执。
  `final-native-receipt.json` 为 late_join_and_terminal_closure_verified / a3Complete=false。
  设备/会话身份、凭据和截图仍只存受信本地证据，不提交远端。
- Loom 本轮仅更新交接文档，保留并接续既有历史修改；15 项 checker tests、strict 1180 文件
  和 Neuro development-standard contract 通过。既有 11 项 501–700 行例外未变；不伪造 Loom
  新编译，也不拿 Hook 检查结果冒充 Loom 产品全量验收。

## A3 默认 GPU 下的编码需求修复（2026-10-05 UTC，源码与候选包）

- 恢复会话 `01a109ca-d630-7f32-8ff4-8ed276ba86f7` 最后的下一步。原会话因预算 402
  中断，留下 `.17` 版本分配和预算回归的局部修改；本轮接续同一迭代，不再次增加 revision。
- 根因为正常 GPU 呈现抑制 JPEG，而 relay 仅读取不可变 JPEG 最新帧快照，未声明自己仍需
  编码帧。Hook `4afe4bd17d1f86ab1fbc1250e0c0f93d9496b4a7` 让源发布 worker 持有
  `EncodedFrameConsumer`；断线恢复期间保留，worker 退出或 spawn 失败释放。多发布者按引用
  持有需求，旧采集注册的释放不影响新注册；预算先于采集线程启动注册，避免立即发布竞态。
- GPU 预览没有关闭。编码需求只取消 GPU-only 的 JPEG 抑制，并独立于本地可见性参与
  既有采集/CPU 预算；单 CPU permit、像素量、帧率上限、公平调度和编码耗时冷却不放宽。
  没有协议、权限、队列容量或网络策略变更。
- 修改前的 hidden-encoded-consumer 回归实际失败于仍返回 1 秒采集间隔。最终源码
  `live_gpu::` 26 项、`live_capture_` 13 项、`live_relay_` 21 项通过；各组未执行的原生/外部
  测试分别 6/1/3 项仍明确忽略，不合并成全量通过。`cargo check --all-targets`、Rust 格式
  及 include 文件格式、53 项 checker/相邻脚本测试、strict 1381 文件和 diff check 通过。
- 额外显式执行 production capture worker 的原生回归，未设置 JPEG 兼容开关：WGC 自有窗口
  → GPU 预览保持 presenting → relay 所读最新 JPEG 可解码且画面变化。两份需求时 1 秒新增
  17 帧，释放一份后 1 秒新增 17 帧，两阶段均出现两种解码画面；释放最后一份后新增 0 帧。
  关闭 native plane 后，静态 JPEG 回退 frame 35 → 36，epoch 保持 1。此数据不是监视器 FPS
  或跨机帧龄，也不是新 EXE 的两机端到端验收。
- 源码有效行数：预算 255、预算测试 185、GPU worker 498（未增长）、relay WebSocket worker
  400、capture worker 440、原生测试 owner 435、新编码需求原生测试 104；全部不超过 500。
- 本轮原生测试只创建并销毁自有窗口/采集线程，不退出日常 Hook，不连接 PC3，也未改变
  持久 SSH/RSC、系统信任或防火墙。前轮两机 JPEG 兼容通过的证据保留，不拿它认证本次新包。
- 当前仍需新候选两机默认 GPU 正式发布/观看/停止复核；A3 总体验收与独立 Surface 错误不关闭。
  本地源码、原生回归和构建回执位于 `GameEditor/linshi/issue67-gpu-encoded-demand-20261005`。
- `.17` 独立候选已 fresh 构建（505.12 秒，exit 0），未覆盖旧包：
  `release/Hook/v0.2.32.17/issue67-gpu-encoded-demand-20261005/hook.exe`，8996864 bytes，
  SHA `ff1f37fa7f7a457cfe3651e6db562ac0fdbf3f6dc76b4538685c9926a3845f9d`。
  provenance 绑定上述 Hook commit、gitDirty=false、internal；exact-hash/product-version
  `--self-check` 通过。仅保留既有 frontend chunk >500 kB 警告，不冒称全套发布验收。
- 发布前 fresh 联网 OSV Enforce 验证 4 lockfiles / 1654 packages / 0 未豁免漏洞 ID；19 项
  既有受控例外不变，依赖安全契约通过。本轮未改依赖、例外或公开版本，也未发布公共 release。
- Loom 仅更新本交接文档：15 项 checker tests、strict 1180 文件、development-manual
  契约和 Neuro 通用开发契约通过；11 项既有软上限例外未变，不重编译无代码变化的 Loom。

## A3 新包默认 GPU 双机最小闭环通过（2026-10-05 UTC，当前停点）

本轮完成上节留下的 **新候选默认 GPU 正常发布、观看、停止清帧**，不将整个 A3 或 Issue #67
标为完成。复用已构建的 `.17`，没有修改 Hook 源码、分配版本或重复构建。

- 两端均运行上述 `.17` / SHA `ff1f37fa7f7a457cfe3651e6db562ac0fdbf3f6dc76b4538685c9926a3845f9d`。
  PC3 单次有界传输约 49 秒，完整 8996864 bytes 和 SHA 校验通过后才执行。传输包装的
  PowerShell Process.ExitCode 为空，保留原脚本失败，不用它判定网络失败或重传；独立远端
  全量校验及 rename 回执确认完整性。两端 GPU 环境开关均显式清空，走 unset/default。
- 拓扑为 PC3 `.17` 源 → 本机既有 Loom daemon/private-CA HTTPS → 本机 `.17` viewer。
  证书启动时仍有效，NotAfter 为 09:24:42Z；没有修改系统信任、防火墙或网络驱动。
  源端实际 socket 为 192.168.15.136 → 192.168.15.20:49874；SSH 仅用于管理 CDP 和 Art bridge。
- 正式实时截图菜单和 OS 框选绑定自有 window/window_message fixture，656×406，正常发布。
  本机正常区域截图 Unit → Actions → 设备仪表板真实 Surface → 参数面板刷新/选择/加入；
  没有 IPC mock、graphStore/attachment 注入或输入权。加入前 15 项 observations，最大
  sequence=170、累计 2550；viewer 接续至 173，无 control_history_reset/control_event_rejected。
- 源端通过生产 DOM 的 data-live-gpu-preview/submitted/error 与同一 capture/relay 状态交叉
  采样，没有包装 invoke 或主动 configure GPU。16 次、约 8247ms 的观察内始终 gpu-mirror，
  GPU submitted 6389→6583、capture JPEG frameId 4374→4523、source relay receivedFrames
  4368→4518、viewer decoded-submitted JPEG frameId 4372→4521。capture/relay/viewer 均为
  同源 epoch 1；viewer complete、656×406、private_https、errorCode=null、reconnectCount=0。
  这些是原生状态与解码提交推进，不是物理显示 FPS、CPU/GPU 收益或跨机端到端延迟。
- 媒体期间重新核对两端 EXE/SHA/creation/Session 1/CDP 父链：源 Hook 3672、CDP owner
  见 pc3-media-binding.json；本机 Hook 53932，CDP 37012 → 53932。正常“停止发布”后
  daemon closed=true/sourceConnected=false；viewer native closed/errorCode=null，
  presentation=null、remainingImages=0、controllerOwned=false，累计接收 1103 帧。
- 截图复核确认观看窗口未越出 viewport、真实 fixture 可见，正常 UIA 摘要会遮挡部分画面。
  关闭后旧 JPEG 已移除，保留明确标为关闭的 UIA 摘要/框线；独立 dashboard 再次出现
  Surface event dispatch failed，仍未确诊，不扩张为整个 Art 无错误。
- 本机首次截图等待窗口枚举超时，随后被非本轮物理点击取消，失败日志保留。恢复使用正常
  区域截图已激活的输入链与 OS 拖选；仅临时 harness 的普通区域模式不再等待窗口枚举，
  Live/window 模式仍保留该门禁。没有把这次恢复称为窗口枚举缺陷已修复。
- 两端 fixture 和测试 Hook 正常关闭；临时计划任务按精确 action/root 和账户 SID 核对后
  移除，远端清理失败不会阻断本机恢复。服务、审批循环、管理 tunnel 均结束；两端测试
  listeners 为空，PC3 无 Hook 残留，RSC 四项 true 与运行前一致，持久 SSH 授权未改。
  原日常 `.15` 按原路径/SHA 恢复为 59252 / creation 04:21:43.3228180Z，WebView2 和
  watchdog 子进程存在，未覆盖原包或日常数据。
- 证据根：`GameEditor/linshi/issue67-a3-gpu-native-20261005T0411Z`。核心为
  default-gpu-measure.json、reversed-join/stop/closed-receipt.json、两端 binding、
  pc3-direct-media-sockets.json、visual-review.json 和清理/恢复回执。机器交叉验证产出
  final-native-receipt.json：packagedDefaultGpuTwoHostVerified=true、a3Complete=false。
  22 个临时脚本经语言感知计数最高 140 有效行，UTF-8 无 BOM；私有身份、图像和原始日志不提交。
- Loom 只更新本文：15 项 checker tests、strict 1180 文件、development-manual 和 Neuro
  通用规范契约、全部临时 PowerShell/Node 语法检查及 diff check 通过；11 项既有软上限例外
  未变。Hook main 保持原提交且 clean，不冒称重跑无改动的编译、OSV 或完整发布矩阵。

下一步先区分独立 Surface 错误与 A3 剩余基线矩阵，再选择一个有界闭环；默认 GPU 的本次
发布/观看/停止门禁不再重复。反方向、多 viewer、慢端、恢复/撤销、长稳和性能对照仍未验收。

## 独立 Surface 事件收敛修复及原生验收（2026-10-05 UTC，当前停点）

续接会话 `01a10a16-ce4d-79a0-b117-aa94ada174e6` 因 402 中断后的最后任务。本轮关闭上述
可复现的独立 `Surface event dispatch failed` 小任务，不重复 `.17` 的默认 GPU 双机门禁，
也不将 A3 总矩阵或 Issue #67 标为完成。

- 旧实机日志同时存在两个问题：事件型 Surface 被普通 `execute_art` 执行，报
  `surfaceAction invocation is required`；真实刷新事件已在 daemon 中 `succeeded`，但 Hook
  使用 Device 凭据 GET 管理员 full-instance endpoint 做收敛，与 Loom 权限合同不匹配。
  UI 又将 native 字符串错误换成笼统文案。旧 `.17` 失败截图、日志及 ack/revision 证据保留。
- Hook `3fe321ccca36a2eab080cb8b7c070ec7371f0df6` 对 Device 会话复用已有的设备过滤
  `/v1/surfaces/stream`；必须等本事件 terminal succeeded ack，再恢复最终 snapshot。
  instance/attachment/event/request/generation 精确绑定；其他事件、旧 generation 不完成
  本动作，新 generation、失败、历史丢失和 25 秒总超时均 fail closed。body/message 有界。
  loopback/admin 原路径保留，没有放宽 Loom 权限、协议或系统信任。
- 参数持久化保留；event-only Surface 不再通过参数/上游/手动路径触发普通 Art 执行，明确
  声明 formal execution 的 hybrid/workflow Art 仍按原路径运行。UI 保留经 native sanitization
  的字符串错误，异步 Unit/generation 归属保护不变。Loom 产品源码没有修改。
- routing 回归修改前 7 项中实际 5 失败；修改后前端聚焦及相邻 7 文件 / 30 tests、Rust
  default `--lib loom_hook_listener_subscription_tests` 37 项、no-default `--lib surface_device_stream`
  6 项通过。两种 feature 的 all-target check/clippy、两套 TS typecheck、严格 ESLint、Rust
  格式、53 项 checker/相邻脚本测试与 diff check 通过；clippy 仍有存量警告，不称零警告。
  首次未限定 `--lib` 的 Rust 测试因 E0463 / required rlib formats unavailable 失败，失败记录
  保留；聚焦测试、all-target 编译和 release 链接通过，不宣称完整 Rust suite 已通过。
- 所有新增/实质修改源码不超过 500 有效行：Device session 277、loom_hook 接线 42、事件
  owner 279、新 stream owner 200、新 Rust 测试 120、Surface UI 181、参数 owner 327、
  新错误显示测试 50、新 execution routing 测试 69。Hook strict 扫描 1385 文件，无 >500 文件。
- `.18` 官方 clean-source fresh build 已完成（612.66 秒，exit 0），不重复构建或覆盖旧包：
  `release/Hook/v0.2.32.18/issue67-surface-device-convergence-20261005/hook.exe`，9003520 bytes，
  SHA `c6e88dcfcc955f07f9dd99020024eb6670ef34dc71e7f193a2a84c36fb0e6c0b`。
  provenance 绑定上述 commit、gitDirty=false、internal；exact-hash/product-version self-check
  通过。fresh 联网 OSV Enforce 为 4 lockfiles / 1654 packages / 0 未豁免漏洞 ID，19 项
  既有受控例外未改；不是 public release。
- 原生验收只在本机进行：独立 appdata/daemon/store、私有 CA 的真实 HTTPS Device 授权，
  正式截图菜单 + OS 框选 → Actions → 设备仪表板 → 两次真实刷新，无 IPC mock 或状态注入。
  snapshot/DOM revision 1→4→7、result revision 1→2，两次精确事件 ack 均 succeeded，
  native 均记录 `transport=device_stream`，resource leases 为 1/2；无普通 execute_art、
  Surface 错误文案或原 invocation 错误。截图人工复核确认真实 fixture、设备列表和刷新按钮可见。
- 验收后重新核对 `.18` EXE/hash/creation/Session 1/CDP 父链：Hook 27624 / creation
  06:57:58.7678240Z，CDP 3456 → 27624，监听仅 loopback。首次托盘驱动未读到菜单、另一轮
  验证器额外 full-instance HTTP 查询未成功，均保留失败、不算通过；后一查询未记录 status，
  不臆断根因。修正临时驱动有界等待与验证器只读隔离 daemon 持久状态后，新 one-shot 目录
  完整通过；观察精确 event/request/attachment/generation，不降低产品授权门槛。
- 夹具和候选经正常产品生命周期关闭；服务与审批循环结束、测试监听为空。原日常 `.15`
  按原路径/hash 恢复为 55804 / creation 06:58:36.9317910Z，watchdog 52900 存在。未强杀
  日常程序、绕过 mutex、操作 PC3、修改防火墙/系统证书信任/网络驱动或持久访问。
- 源码/构建证据根为 `GameEditor/linshi/issue67-surface-dispatch-20261005`；最终原生证据根为
  `GameEditor/linshi/issue67-surface-native-20261005T0650Z`。关键文件为 source-validation.json、
  build-result.json、headless-summary.json，以及 native-surface-receipt.json、source-direct/binding.json、
  surface-verified.png、visual-review.json、services-final.json、cleanup-final.json、original-restored.json
  和 final-native-receipt.json（packagedSurfaceDeviceEventsVerified=true、a3Complete=false）。
  私有身份、凭据、截图及原始日志不提交。
- Loom 仅更新本交接文档：15 项 checker tests、strict 1180 文件、development-manual 与
  Neuro 通用开发契约、两仓 diff check 通过；11 项既有软上限例外未变。不重编译无产品代码
  变化的 Loom，也不以本次单机 Surface 验收替代双机/A3 剩余门禁。

下一步从 A3 尚未验收的反方向、多 viewer、慢端、恢复/撤销、长稳和性能对照中选择一个
有界闭环。本轮 `.18` 仅证明独立 Surface 修复，不把 `.17` 的双机结果移植为 `.18` 全矩阵通过。

## A3 接续：Device 媒体撤销安全修复（2026-10-05 UTC，服务端小块完成）

用户要求提交推送并继续剩余开发。Hook `.18` 与上一节 Loom 文档已在各自 main 发布；
本小块修复 Loom 已建立媒体连接的授权脱节，不将 A3 或 Issue #67 标为完成。

- 反方向原生准备使用 fresh 固定 host key SSH，PC3 Active Session 1、无 Hook/LockApp
  或测试监听；没有放宽内存准入。但一次 `.18` SCP 仅传到 393216-byte partial，实际报
  `Timeout, server 192.168.15.136 not responding` / `Couldn't send packet: Broken pipe`。
  包装 ExitCode 为空，不能当成功；未完整校验、rename 或执行候选，没有启动原生任务、
  隧道或测试服务。SFTP 子进程和 parent 随后自行退出，partial 可读、监听为空；没有
  强杀、盲重传或更改持久 SSH/RSC、系统网络/信任。证据根为
  `GameEditor/linshi/issue67-a3-reverse-native-20261005T0725Z`，失败日志与
  `pc3-transfer-cleanup.json` 保留。这不是反方向原生通过。
- 根因来自实际源码：普通 LiveRelay 仅在 handshake 校验 Device token，worker 只保留
  Device ID 并复核 LiveSession membership；DeviceRegistry 的 token revoke、disable、
  delete 不会移除该 membership。修改前两个真实 source/viewer socket 回归均失败，
  撤销后仍未 close，最终 read timeout `10060`；不以握手拒绝冒充旧连接停流。
- 新私有 `LiveMediaDeviceGrant` 只保留 token digest、Device ID 和 registry Arc，复核
  session 存在/身份/expiry、Device enabled/approved、epoch；锁失败 fail closed。
  不保留明文 token，不重复消费 handshake nonce；先释放 registry 锁，再查 membership。
  worker connected 前、source 阻塞 read 返回后/publish 前、viewer wait/adaptation 后/send
  前复核。原 membership/epoch/cancellation 和 role/connection permit Drop 清理保留。
  管理员 admission 仍要求已认证管理员，不将 None grant 暴露为匿名入口。
- 新 5 项测试全部通过：source 仅 token revoke 后旧 socket close 且 lastFrameId 不推进，
  正常 viewer 保留；idle viewer revoke 后 close、独立 peer 仍收到下一帧；真实管理 HTTP
  disable/delete 停止既有 source 并拒绝新握手；7 种过期/身份/设备失效状态与 poisoned
  registry fail closed。复核 nonce 不增加。测试是 daemon + TCP/WebSocket/Ed25519 配对，
  registry mutation 仅为回归控制输入，不冒充原生管理 UI。接收总等待有界，正常 fixture
  shutdown 必须成功；首轮 fixture E0425 和诊断接线 E0308 失败记录保留。
- 相邻 14 项 LiveRelay（含以上 5 项、管理员 fanout/resume 与独立 viewer diagnostics）、
  2 项 Wall grant、daemon all-target check/clippy、Cargo/include Rust 格式、15 项行数 checker
  测试、strict 1182 文件、Loom/Neuro 开发契约和 diff check 通过。首次 clippy 缺少 pinned
  1.95.0 component，安装后通过；存量 warnings 保留，不称零警告。最后一次测试错误类型
  收窄后再次运行 5 项聚焦测试和 all-target clippy，通过且未保留该新增 warning。
- 真实联网 OSV 扫描 4 lockfiles / 1341 package records，0 未豁免漏洞；9 项既有配置
  exceptions 未改，扫描过滤 10 个 advisory（含 alias）。依赖/manifest/lockfile 无修改。
  源码有效行数：lib 285→287、WebSocket 421→433、diagnostics 281→284、wall_http
  371→371、新 grant 49、新测试 291；11 项既有软上限例外未变。
- 独立只读审查未发现本范围新增认证绕过、锁序死锁或 worker/permit 泄漏。撤销语义是
  **失效复核后停止继续处理媒体并退出连接**，不是与在途 decode/publish/send 线性化的
  零字节撤销。250ms socket timeout 不等于无条件 SLA，mutex/encoding/调度仍依赖系统推进。
  source 断开不会伪造 session closed，暂时断网仍允许保留最后画面。
- 源码提交 `39808e19723f589206385339a6b4a35f6384cbd7` 已普通推送到 Loom main，
  `ls-remote` 一致。从该 clean commit fresh 执行
  `cargo build --locked --release -p loom-daemon`，263.22 秒 / exit 0。新独立组件候选为
  `release/Loom/issue67-a3-device-media-revocation-20261005T0820Z-daemon/runtime/loom-daemon.exe`，
  36954112 bytes，SHA `3addc3af7925fc7edc1e35d64493c1f31439cd2e74c5c5084bf29c90c53e336e`。
  component manifest 绑定 sourceGitDirty=false、实际 toolchain/命令、4 个 lockfile hashes；
  该 manifest 的 smoke pending 是构建时状态，完成后的独立 component-verification 提供
  包级结论。不改写已构建 payload 或旧发布包。
- 精确候选的实际进程 PID 17144 / creation `08:27:57.5063330Z`、路径/hash 与独立 loopback
  listener owner 均已绑定。真实 HTTP/Ed25519 配对/Device session/WebSocket 检查通过：
  管理 PUT disable 与 DELETE 后旧 source 显式 Close、sourceConnected=false、lastFrameId
  固定在 1、旧 credential 新 handshake 被拒绝，session 不伪 closed；管理员 cookie admission
  正常，坏 Device token + 有效管理员 cookie 仍被拒绝。帧是明确标记的 2×2 synthetic raw
  fixture，不是截图或 native viewer；没有 packaged viewer、多机、物理显示或性能结论。
- 首次包级 harness 在隔离 APPDATA 后找不到用户 site 的 cryptography，未进入协议断言，
  是工具环境失败、不是产品失败；原失败回执和清理状态保留。确认原因后显式绑定原已安装
  dependency site，在 fresh one-shot root 复核同一 EXE，通过；未重编译、安装项目依赖、
  重置旧 marker 或共享日常数据。两次自有测试进程均退出、测试 listeners 为空；服务 root
  使用 current user/System/Administrators 限定 ACL。日常 `.15` main/watchdog 未停止。
- 交付范围是 **daemon component candidate**，不是完整桌面 official release：未运行整包
  `build-release.ps1` / `verify-release.ps1 -RunSmoke`，不包含 Loom.exe、完整桌面/SDK/SBOM，
  没有部署为常驻服务或发布 GitHub Release。Hook 产品源码/`.18` 不变，不重复构建。
  源码/构建/包级证据根为 `GameEditor/linshi/issue67-a3-media-revocation-20261005`，关键回执是
  source-gates.json、after-lines.json、release-build-result.json、candidate-binding.json、
  packaged-socket-smoke-fix1-receipt.json；原始身份/凭据/数据不提交。

尚未关闭：Hook Device 撤销后清旧画面的终态 UX、真实两机撤销、反方向、多 viewer/慢端、
恢复、600 秒长稳、静态文字/滚动/运动及性能对照。服务端关闭 socket 不能作为 Hook 清帧
证据；B/C 优化仍须真实瓶颈，不猜测 codec 收益。

## A3 接续：Device 终态停流与清帧（2026-10-05 UTC，跨仓库源码及候选完成）

- Hook `284cbeec6c2f5b067b011ebd4a61934a5c0d088b` 与 Loom
  `a82e8cdad4bb8ff9eaf06da3d6acecf1a59f66c9` 已分别普通推送到 main，远端 SHA 一致。
  本节为源码/产物回执，不把 A3 或完整开发计划标为完成。
- Loom admitted Device session 新增共享的内存 revoke marker；显式 revoke、成功持久化的
  disable/delete 才置位，expiry/nonce eviction 不置位。grant 只持有 digest、registry 和
  marker，不保留明文 token，不重复消费 nonce，也不增加永久 tombstone 或持久 schema。
  disable/delete 在同一 registry 锁内先持久化成功再撤销；失败回滚不再误删原 session。
- 精确终态信号为 WebSocket Policy (1008) / `live_media_device_revoked`。Hook 仅在
  Device-authenticated 原媒体 socket 收到该组合时停止本地 relay、清 authority/native frames、
  保留 sticky revoked error；source recovery 在进入、join 后和 replace 前拒绝迟到复活。
  普通 Close、expiry、网络异常及 HTTP 401/403/404 不猜成永久 revoke。source 先读控制消息再
  选择/编码/发送；send/Ping 失败后只做一次受现有 socket deadlines 约束的控制读取。
- terminal helper 不在 worker 内 self-join，显式 disposal 回收 JoinHandle。前端沿已有 closed
  snapshot 清图、释放 URL、停止 poll，已知 closed/generation 不提交迟到 decode；decode await
  会暂停该 relay poll，因此不承诺 native terminal 到 JS 清图的零延迟，也不增加每帧 IPC。
  local relay closed 不伪造 Loom LiveSession closed，本地 capture 不冒充 source-session end。
- 协议边界已同步两仓库：已被 expiry/nonce 删除的 session 无法取得后来新置的 marker，仍
  fail closed 但可能普通 Close；网络可能丢 Close。在途帧不与 revoke 线性化，不宣称严格毫秒
  SLA。epoch bump/approve 仍是可恢复失效；坏 Device credential + 好 admin cookie 不降级。
- Hook 修改前真实 viewer socket 回归失败 `authoritative Device revocation did not stop viewer`；
  修改后邻近 native LiveRelay 26 passed / 3 explicitly ignored，最后行为增量 owner 14 passed，
  no-default lifecycle 9 passed。包含真实 viewer 清帧/不恢复、普通 Policy expiry 保留旧帧与
  recovery，以及完整生产 source worker：真实 socket 收到至少两帧、synthetic JPEG producer
  仍在持续生产时 revoke 后 stop/sticky error/单连接成立。不是 WGC capture 或完整 Rust suite。
- Hook 两种 feature 配置的 all-target check/clippy、Cargo/include formatter、前端四组 46 项
  聚焦测试、两套 TS typecheck、行数 checker 53 项及 strict 1388 文件通过。Loom media 17 项、
  Wall grant 2 项、all-target check/clippy、formatter、checker 15 项及 strict 1183 文件通过；
  两仓库/Neuro 开发契约与 diff check 通过。所有新/改源码不超过 500 有效行，最高为测试
  `part_38.rs` 490；存量 warnings 保留。fixture callback ABI 的大错误类型仅有说明性的 scoped
  lint allow，之后最终 clippy 通过。一次中间 viewer fixture 未接受首帧的失败原日志保留；
  后续诊断单测、邻近 26 项与最终 14 项通过，不将未确诊的单次失败归因为环境。
- 真联网 OSV：Hook 4 locks / 1654 packages、Loom 4 locks / 1341 records，均 0 未豁免 ID；
  原有 19/9 项配置例外不改，不声称依赖零 advisory。独立只读审查未发现新增认证绕过、
  expiry 误终态、确定锁序死锁或 self-join；审查指出的 send-first 与 producer 已结束覆盖缺口
  已修复并纳入最后 owner 回归。
- Loom clean source fresh `cargo build --locked --release -p loom-daemon`：约 466 秒、exit 0。
  独立组件候选 `release/Loom/issue67-a3-terminal-device-revocation-20261005-daemon/runtime/loom-daemon.exe`，
  36955648 bytes，SHA-256 `8e36846df26cd7df7539bfe1c56942da735e8f0bf02a45e5cac28595e1b24d84`。
  该 EXE 的真实 loopback listener/process 绑定、HTTP/Ed25519/Device/source WS smoke 通过：
  disable/delete 后原 socket 1008/exact reason、lastFrameId 固定 1、sourceConnected=false、旧
  token 新握手被拒，admin admission 和坏 Device 不降级保留，LiveSession 不伪 closed。
  明确使用 2×2 synthetic raw fixture，不是截图/native viewer/两机；自有进程已退出，listeners
  为空。构建 manifest 保留当时 pending，新增 component-verification 提供最终包级结论。
- Hook 本轮只分配一次内部版本 `.19`，公开 SemVer 仍 0.2.32。fresh 官方 production/release
  build 约 538 秒、exit 0，clean provenance/source SHA 与以上 Hook main 匹配。不可变候选
  `release/Hook/v0.2.32.19/issue67-a3-terminal-device-revocation-20261005/hook.exe`：9005056 bytes，
  SHA-256 `80188bafb585a0927084704ed5637f5b28cfa676deb4f2c4ab7b153e5ff0adb7`；官方
  `Invoke-HookHeadlessReleaseSmoke.ps1` 绑定同一 SHA、status=passed。既有 JS chunk warning 保留。
  这是内部 Hook EXE + daemon component candidate，不是完整 joint/official release，未部署
  常驻或发布公共 GitHub Release，旧 `.17/.18` 原生结果不认证新 bytes。
- 本轮证据根 `GameEditor/linshi/issue67-a3-terminal-revocation-20261005`：两 source-gates、
  final-owner-tests、after-lines、daemon-candidate-binding、packaged-terminal-socket-receipt、
  hook-build-result、hook-headless/headless-summary；凭据和原始私有数据不提交。构建期间未
  停止其他 owner 的 Gateway build，日常 `.15` main/watchdog 未退出或被替换。

下一小块：旧 scoped TLS leaf 已过期；生成 fresh 隔离 leaf，不改系统时间/信任/防火墙，再
用精确 `.19` + 新 daemon 做有界原生 viewer revoke/清图。随后推进反方向、多 viewer/慢端、
恢复、600 秒和静态文字/滚动/运动对照；B/C/D 仍按实际瓶颈或负载证据决定，不凭空做 POC。

### A3 `.19` 原生观看端撤销回执（2026-10-05 UTC）

- 使用上述 `.19` 和新 daemon 同一 EXE bytes，没有重复构建或增加内部版本。新隔离 TLS
  leaf 有效至 `2026-10-05T14:09:19Z`，仅指定 HTTPS origin 追加 CA，未改系统信任/时间/
  防火墙；服务 root 使用当前用户/System/Administrators ACL，PC3 本轮未触碰。
- 原生 Hook main PID 41232 / creation `10:25:50.1054950Z`、Session 1、实际 EXE SHA、CDP
  listener PID 43000 的父链均已绑定。daemon 使用同一组件候选，隔离服务/管理桥 owner 记录
  留在 services-ready；管理桥只用于 Art/Surface，不转发 LiveRelay 媒体。
- 在实际 Hook 从截图 Unit 的 ActionsMenu 创建并挂载真实 Surface，执行 refresh 后使用
  参数面板“刷新列表 / 选择实时投射 / 加入观看”，没有替换 IPC 或绕过入口。媒体连接
  `networkScope=private_https`；A2.2 单槽绑定 `.19`、source/relay/session，raw 64×48 实际解码
  提交、图像 naturalWidth/Height 和帧推进通过；加入未取得输入控制。
- 源是明确的 synthetic raw protocol fixture，经真实 HTTP/Ed25519/Device session/source WS
  admission 持续发送；不是另一台原生 Hook capture，也不是 GPU/JPEG/物理显示性能基线。
  正常 Surface/UI 与产品 native viewer 是真实组件，不将协议夹具包装成完整两机验收。
- 通过管理 PUT disable 原生 viewer 的 admitted Device 后，native status 为 closed +
  `live_media_device_revoked`，`poll_live_relay_frame(afterFrameId=0).frame=null`，前端 presentation
  为 null、`img` 数量为 0；撤销前后截图已人工检查。显式 reconnect 返回 stopping，被拒绝；
  继续观察 4 秒，receivedFrames/reconnectCount 不增长、sticky error 保留。独立源继续前进
  47 帧、sourceConnected=true、LiveSession.closed=false，不把 viewer revoke 当 source end。
  单次“请求开始到观测 native closed”约 166ms，包含请求/持久化/轮询，非严格 SLA/分位数。
- 三次先行失败记录保留，均未到 terminal gate：首轮 viewer 管理桥沿用不存在的 49882，
  日志确认地址与服务 49875 错配，修正后实际 Surface mount 成功；第二轮辅助 Surface GET
  使用 `%3A` 路径得到 404，实际 `path_id` 返回 raw suffix，改用校验过的 raw instance UUID
  与当前 native 合同一致；第三轮菜单 helper 报 `Expected one owned menu popup`。复核已有
  helper 源码后改用现存的 3 秒有界 readiness/2 秒 query 版本，再在 fresh one-shot root 通过，
  不武断确诊第三轮失败的唯一原因。没有重置旧 marker 或为这些 harness 问题改产品代码。
- Windows Computer Use native pipe 两次返回 `系统找不到指定的文件。 (os error 2)`，未执行其
  app input；复用既有 scoped product acceptance helper 与原生 WebView2 CDP。只读预检确认
  input desktop 为 Default、可见 LockApp 窗口为 0；仅进程存在不冒充锁屏，也没有操作认证屏。
- 所有自有 candidate、fixture、服务正常退出/按已绑定 owner 清理，五个测试 listener 为空。
  日常 `.15` 经身份绑定的正常菜单退出后按原路径/hash恢复；最终 main PID 952 / watchdog
  23436，creation `10:26:33.8739080Z`。不能再声称日常旧 PID 始终未退出；其原 bytes 未变。
- 成功证据根 `GameEditor/linshi/issue67-a3-native-revoke-fix3-20261005T1030Z`：
  native-revoke-result、native-continuation-receipt、viewer-direct/binding、cleanup-final、
  original-restored 和 before/revoked 截图。原失败根均保留，凭据/截图/原始数据不提交仓库。

此小块关闭的是 **精确 `.19` 原生观看端 disable → 停流/清图/拒复活**；delete/token-only 的
原生、多机撤销、反方向、1/2/4 viewer、慢端、恢复、600 秒及三类内容对照仍未完成。
源码无新增改动，文档-only 检查和 scoped push 后继续 A3，不为本回执再构建同一二进制。
