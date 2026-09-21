# 瓷砖墙阶段 7：物理屏幕识别与显示控制收尾

2026-09-12。物理识别、独立内容目录刷新、动态 Live 显示控制的本机阶段验收已完成。
收尾验收暴露的 Art 输入拒绝与读取准入两个竞态已修复，最终候选通过源码门禁、
Loom 官方包校验，以及原生 Art E 和 Live C 联合验收。完整管理、调度、长期资源与
双机等余项继续按 [实施计划](TILE_WALL_IMPLEMENTATION_PLAN.md) 推进，整个计划保持实施中。

## 识别合同与管理界面

端点可声明 `display: {name, canIdentify}`。名称来自 Windows 实际枚举，不能为空、
不能含 C0/DEL，最多 256 个 Unicode 字符；两个字段完整出现，拒绝未知字段与 null。
能力声明不授予管理权限，旧严格解析器需要与 daemon、管理端一起升级。

管理员调用 `POST /v1/walls/endpoints/identify`，只传 `endpointId`。设备必须已批准、
启用、在线并声明识别能力。未分配墙面的输出可以识别；冻结或黑场应先恢复。
每个呈现租约最多持有一个易失请求，最长 10 秒，重复请求不延长原期限。识别不写入
持久化目录、不推进 CAS 或几何版本，不丢弃管理页未保存的草稿。

状态包含 `identification: {requestId, remainingMs, applied}`。终端通过
`POST /v1/walls/endpoints/identify/report` 上报 `applied` 或 `dismissed`，必须匹配
当前设备、端点、租约及 UUID。过期结果不能复活请求或确认后继请求，布局、模式、
权限、租约和连接变化会清理识别。此回执与布局确认、物理扫描输出各自独立。

Hook 显示实际屏幕名称、端点信息、像素尺寸、倒计时和关闭按钮。底层内容为 inert，
焦点进入标识且 Tab 被限制在标识内；Escape 关闭标识并保留输出进程。客户端从
状态请求开始时刻计算保守期限，重复快照不能延长或重新打开旧标识，动画帧确认有
500 ms 上限。退出识别后由正常呈现确认恢复输入。

daemon 在识别响应前释放已按下的 Live 键/按钮，立即拒绝本输出的新 Live/Art 输入。
已接受的 Art 工作及其取消权仍归原 owner。完整合同见
[物理识别 API](../protocol/WALL_IDENTIFICATION_API.md)。

原生 A 的管理页曾留下内容目录读取 503 的提示。目录此前只加载一次，重新载入
整个目录又会丢弃草稿。新增独立的“刷新内容来源”，每轮并行读取 Live 和 Art 清单，
整轮结束后才决定重试；失败后间隔 2 秒，最多 3 轮，成功后停止。销毁后取消计时并忽略迟到结果。
持续失败仍显示原因。原生 B/C/D 已验证刷新不重置草稿，真实 Art 来源可以选择。

## 原生失败、定位与回归

- `Hook/artifacts/tile-identification-r8-a-native`：初始 r8 的识别、Art/图片显示
  控制、输出与 daemon 恢复通过，清理无残留；管理截图暴露上述目录提示问题。
- `tile-identification-r8-b-management`：使用修复目录刷新的 Loom 包，识别与管理
  断言通过；追加冻结循环在目录 v13、布局 v12 报告 `frame_unavailable`，命令退出 1。
  原始请求轨迹已在失败前到达 1024 条上限，不能据此断言没有错误。
- `tile-identification-r8-c-trace`：同一产品候选配合末段有界轨迹完整通过。轨迹
  记录布局 v12 的表单 `change` 被 409 拒绝，随后发出 `surface_close` 并重新打开
  Art 附件；本次及时恢复后冻结 v13 成功。它揭示了 B 所在边界的竞态。
- `tile-identification-r8-d-resync`：修复后的 Hook 已在同样的迟到 `change` 拒绝后
  保留两个 Art 画面并成功冻结 v13。随后等待恢复布局 v14 时，探针的
  `GET /v1/walls/state` 收到 503，命令退出 1；轨迹还记录心跳和图片读取 503。
- `tile-identification-r8-e-admission`：最终 Hook 与新 Loom 准入候选完整通过，
  命令退出 0，`cleanup.json` 为 `passed: true`、`remaining: []`。7 个产品进程
  实际路径均匹配候选，包括输出和 daemon 重启。保留的最后 1024 条请求轨迹中
  没有 503；两次迟到 `surface_event` 409 后仍保留两个 Art 画面。

B/D 的 `cleanup.json` 均为 `remaining: []`；其中 `passed: false` 记录整体运行失败。
C 为 `passed: true` 且清理为空。失败目录与旧候选保留，没有放宽探针超时或忽略缺帧。
请求轨迹现在固定保留最后 1024 项和 512 项 DOM 状态，只记录受限错误码、状态、
事件类别及回执，不记录凭证、表单值或完整请求正文。

E 验证未分配/已分配屏幕识别、重复请求期限、输入屏蔽、Escape 与自动消失、目录
刷新不丢草稿、真实 Loom 管理页、冻结/黑场/恢复、3 次附加循环和几何失效。
输出及 daemon 重启都不重放冻结帧，恢复后仍能操作原来源，普通附件、正式结果
与历史保留。已人工查看识别标识和冻结管理页截图，实际名称与 3840 × 2160 输出一致。

### 输入失败不能销毁仍可读取的帧

`tileSurfaceController.ts` 此前在任意输入异常后调用 `cache.reset`，关闭附件、
丢弃快照，进而使 renderer 清除完整帧。在冻结指令传播期间，最后一个本地 blur
事件仍可能被服务器拒绝，是否赶在附件重建前应用冻结取决于时序。

现在只丢弃同实例未提交的后继输入，并刷新状态；读取授权仍有效时保留附件和像素。
下一次新手势先读取权威序号，失败的操作不重放。服务端要求严格的下一序号，所以
不能在拒绝后盲目递增本地计数；服务器已消费/未消费序号的两条路径分别有回归。
状态读取失去授权、丢屏或租约失效仍清除内容。

`Hook/__tests__/unit/TilePresentationInput.test.ts` 先复现冻结结果为
`frame_unavailable`，修复后验证保帧、后继丢弃、读取撤权清屏及两种序号恢复。
红灯日志为 `tile-wall-r8-input-freeze-red-final.log` 和
`tile-wall-r8-input-sequence-red.log`，最终 6 文件 27 项通过记录为
`tile-wall-r8-retained-resync-tests.log`，均位于 `Hook/.tmp`。

中间 Hook `v0.2.30.8-identification-retained` 已构建，但独立序号回归随后发现问题，
未作为最终候选验收。最终使用 `v0.2.30.8-identification-resync`。

### 短暂读取突发不能被当作执行队列拥堵

读取入口原本有 4 个线程，同一 IP 最多占用 3 个读槽，以给其他地址留出能力。
第四个请求立即被拒绝，即使它已完整发送、此前三个请求即将读完、业务执行队列
仍为空。Loom 管理页、Hook 和探针共享本机地址，会发生这样的短暂突发。

`short_same_peer_read_burst_waits_for_existing_readers` 用真实 TCP 和无需业务队列
的 `/health` 复现了 503：前三个请求只短暂占用读槽，第四个完整请求仍被拒绝。
红灯证据为 `Loom/.tmp/tile-wall-r8-read-burst-red.log`，排除了业务执行队列拥堵。

新增 `connection_read_admission.rs` 保存最多 64 个等待 socket，准入期限为
250 ms。等待者不占用读线程，不扩大同 IP 的 3 个读槽；扫描跳过仍被阻塞的地址，
每轮最多处理一个准入或拒绝，持续过载仍返回原有 503。重复检查不延长期限。
关闭时等待 socket 进入同一个有界 drain，最多处理 64 个候选，并把实际 read
timeout 限制到剩余关闭预算，避免闲置 socket 延长清理。

12 项读取相关测试通过，覆盖真实短突发、持续过载、关闭响应、跨地址处理、
绝对期限、容量和 socket 释放；另外 8 项生命周期/并发测试及 40 项 wall 测试通过。
日志分别为 `tile-wall-r8-read-burst-green.log`、
`tile-wall-r8-admission-lifecycle-tests.log` 和 `tile-wall-r8-admission-wall-tests.log`。

## 动态 Live 原生 A 与最终 B/C

`Hook/artifacts/tile-live-display-r8-a-native` 使用初始 r8 daemon/Hook，命令退出 0。
原生 WinForms 客户区 WGC 与生产回源代码验证：识别释放已按下的键/按钮、Escape
保留输出、冻结期间源帧继续推进但完整 canvas SHA-256 不变、暂停时源输入被拒绝、
黑场为 `[0, 0, 0, 255]`、黑场后冻结报告缺帧、恢复后取得新动态像素和原生输入。

显示控制过程中来源帧计数从 48 增至 88；最终清理时为 121，工作线程全部 join，
按键/按钮计数均为零。冻结 canvas SHA-256 为
`9abacd01805040fb79ec76b99ef6822a5f820588e49ceaae088c4872e10a8318`。
27 次输入到可观测源效果的采样，中位数 109.0814 ms、P95 137.6038 ms、最大
312.5536 ms，包含 50 ms 轮询，不代表物理显示延迟或同步。输出崩溃、原来源重连
与断源清屏也通过。清理记录跟踪 30 个进程身份，`remaining: []`。

最终候选的 `Hook/artifacts/tile-live-display-r8-b-admission` 同样退出 0，显示控制、
原生输入、输出恢复及清理通过；但重启输出的初次 `actualExe` 为 null，前后资源
快照也未保存其路径。该运行的行为证据保留，路径审计使用后续 C。

`Invoke-TileWallNativeInputProbe.ps1` 现在将运行期间已经通过 PID/创建时间校验的
路径采样回填到对应仍存活的 owned root 记录，保留最晚一次相同 PID 的启动归属。
复用已有 CIM 采样，不增加轮询或改变终止范围；PowerShell parser、进程身份契约
和行数门禁通过。本阶段 C 时脚本从 177 增至 183 有效行，产品二进制未因此重建；
后续管理与长测扩展的当前计数见 [阶段 8](TILE_WALL_STAGE_8_ACCEPTANCE.md)。

`Hook/artifacts/tile-live-display-r8-c-path-evidence` 完整复验退出 0。5 个产品进程
实际路径全部匹配最终候选，原生来源仍为生产 WGC/输入代码的 Rust 测试进程。
识别释放按键/按钮、冻结保持完整 canvas、暂停拒绝输入、黑场、缺帧回执、恢复
新动态画面、输出崩溃释放、原来源重连与断源清屏全部通过。

C 显示控制期间来源帧计数从 199 增至 453，最终清理为 684；工作线程全部 join，
按键/按钮为零。冻结 canvas SHA-256 为
`eda6e6804f31403089d0267dc950f6d04036c413a00f7f43e8161b3a1d385d58`。
27 次输入到可观测源效果的采样，中位数 119.9805 ms、P95 143.3275 ms、最大
172.1478 ms，包含 50 ms 轮询，不代表物理显示延迟。清理跟踪 30 个进程身份，
`passed: true`、`remaining: []`。B 的冻结/恢复截图已人工查看，C 保留独立截图与像素断言。

## 当前候选与门禁

- Hook：`Neuro/release/Hook/v0.2.30.8-identification-resync/hook.exe`，内部身份
  `v0.2.30.8`，公开版本仍为 `0.2.30`，`gitDirty: true`、`uiAccess: false`。
  内部版本只分配一次。构建、provenance 一致性、`--self-check`、`--help`、
  `--tile-outputs` 已通过；真实输出为 3840 × 2160。
- 新 Loom 候选目录为 `Neuro/release/Loom/20260912-tile-wall-r8-admission`，由
  官方构建脚本生成；`verify-release.ps1 -RunSmoke` 退出 0，50 个文件校验和
  standalone、Hook canvas、error preview、framework Art store、plugin boundary、
  Surface prototype、authored Art 共 7 组 smoke 全部通过。
- 原 Loom `20260912-tile-wall-r8-management` 已通过 50 文件校验和 7 组官方
  smoke。初始 `20260912-tile-wall-r8-identification`、各阶段旧包均保留。

最终产物摘要如下，原生 E 与 Live B/C 记录的 Hook/daemon 摘要与文件一致。

| 文件 | 字节 | SHA-256 |
| --- | ---: | --- |
| Hook `hook.exe` | 8466944 | `65a341ca2039e020d277b091472f4c509cd52117f10434ab12b4d89e290a5d8e` |
| Loom `Loom.exe` | 10324992 | `5b9b08cf55ff7db67e1fa4d19c858c9f99d11edbd96e89a5366f3b2ad2c0abcc` |
| Loom `runtime/loom-daemon.exe` | 25834496 | `1a9fc1bc3289ce97c1ef88cafeaba63a5ebb648153d8da0b5d68944c8e569bb1` |

构建与官方校验日志为 Loom `.tmp/tile-wall-r8-admission-package.log` 和
`.tmp/tile-wall-r8-admission-verify.log`。本轮没有修改依赖清单、锁文件或发布脚本，
沿用此前相同依赖状态的安全扫描证据，没有新增豁免。

Hook 最终 27 项显示/Art 回归、生产与测试类型检查、lint 和 ratchet 行数门禁通过。
原有识别/协议/presenter 16 项、原生 wall client 10 项的证据保留。Loom 协议
8 项、store 19 项、管理服务 5 项及来源目录 3 项通过；受准入改动影响的读取、
生命周期和 wall 组已重新执行。Rust formatter 与 include fragment formatter 通过。

Loom strict checker 扫描 1027 个文件，12 项既有软例外，无新增违规；checker
契约测试 15 项通过。Hook 扫描 1189 个文件，没有超过 500 有效行的文件。主要
改动的有效行数如下，未申请新例外：

| 文件 | 有效行 |
| --- | ---: |
| Hook `tileSurfaceController.ts` | 235，修复前后不增长 |
| Hook `TilePresentationInput.test.ts` | 83 |
| Hook `presentationTrace.ts` | 78 |
| Hook `Invoke-TileWallNativeInputProbe.ps1` | 本阶段 C 为 183，路径采样修复前 177 |
| Loom `connection_read_admission.rs` | 79 |
| Loom `tests/connection_read_admission.rs` | 199 |
| Loom `connection_dispatch.rs` | 479，改动前 466 |
| Loom `daemon_lifecycle.rs` | 403，改动前 389 |
| Loom `apps/daemon/src/lib.rs` | 261，改动前 259 |

## 仍需完成

没有暂存、提交或公开发布；用户已有 Hook、Loom/daemon 和旧 release 保留。

完整管理放置、普通 Hook GUI 来源、来源恢复全覆盖、场景调度/媒体时钟、长期资源、
跨瓷砖手势、多人和多终端仍按实施计划推进。当前只核验一块物理输出，没有据此
宣称两台实体电脑拼接、物理同步、Frame Lock 或 Genlock。双机验收仍需第二台
Windows 电脑的局域网地址及已授权远程执行入口。C 结束时采样未发现普通 Hook
进程，用户原有 Loom/daemon 仍在旧候选路径运行；后续状态以阶段 8 的记录为准。
