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
| A2 | 待办 | Hook/Loom：正常发布和受权加入入口 + 收端证据采集 | 核对当前 UI/连接设置，绑定 source/epoch/frame 与包 SHA；收端记录真实 decoded-submitted 和缺口，不把 daemon 采样当显示 FPS；按需补最小接线 |
| A3 | 待办 | 两机原生基线与对照报告 | 静态文字/滚动/运动，1/2/4 viewer、慢 viewer、断线恢复、停止撤销；记录网络/包/CPU/内存/阶段耗时/字节；硬件 GPU 或物理显示无法测时明确缺失 |
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
- 下一动作 **A2**：从 Hook 正常发布/加入入口核对当前调用链与身份/包绑定；先检查
  `src/services/liveRelayController.ts`、`liveRelayPresentation.ts`、`src-tauri/src/native/live_relay_commands.rs`。
  在既有 decoded-submitted 合同上补收端证据/接线，若入口已有闭环就复用，不重新建功能。
  A1 的 synthetic loopback 测试只能证明采样器可用，不能替代 A2/A3 的原生两机验收。
- 仍未验收：正常产品入口两机闭环、真实观看 FPS/帧龄、CPU/GPU 收益、受限网络和长稳。
- 本轮辅助子代理因上游 503 未得到有效结果；直接源码核查不标为独立交叉评审。
