# 瓷砖墙阶段 3：Live 媒体候选

2026-09-10。本阶段接通既有 Hook Live 会话到独立瓷砖终端的 raw BGRA 媒体，
支持动态裁剪、旋转、同源多位置以及图片叠放。鼠标、拖动、滚轮、键盘回源仍未接入，
终端继续上报空输入能力列表。完整任务尚未完成，下一项是 TW-06 的操作权与输入生命周期。

## 候选与启动

- Loom：`Neuro/release/Loom/20260910-tile-wall-r3-live/Loom.exe`。
- 无头 daemon：同目录 `runtime/loom-daemon.exe`，输出无需依赖 Loom 管理界面。
- Hook：`Neuro/release/Hook/v0.2.30.3/hook.exe`，使用 `--tile` 管理输出，或
  `--tile-output monitor-<64 hex>` 启动指定物理输出。

终端配对后，在 Loom 的“设备管理 → 屏幕墙”选取已经存在的 Live 会话，放置并保存。
多个位置可以选择相同会话。关闭 Hook 瓷砖管理窗口会保留已经启动的独立输出。
启动与能力限制详见 [Hook 瓷砖终端说明](../../Hook/docs/TILE_TERMINAL.md)。

最初的 `20260910-tile-wall-r3` 保留供核查；它尚未包含防止断源后重放旧帧的修正，
本阶段推荐 `20260910-tile-wall-r3-live`。Hook 在这一修正中没有源码变化，复用
`v0.2.30.3`，没有再次分配内部版本。阶段 1、阶段 2 目录均已保留。

SHA-256：

```text
Loom.exe
dd89f51af439fd7f8f249756b6903d3db8064833e49a807dd5c566700ea6e301
runtime/loom-daemon.exe
d04b7cdfe2ae83fce2be1b80f3ecbb58ab485b6b2521472e957a693d8cea7057
hook.exe
ead6583fd1c54c117fe79d5413893c8e80d673a1f5aa6d222c19091199d8b0e5
```

## 实现与权限

Loom 新增 `/v1/walls/live/media`，使用既有 `loom.live.v1` 二进制 WebSocket 帧和
同一个 Live 会话的最新帧缓冲。授权同时绑定真实配对设备、输出端点、呈现租约、布局
版本和可见 Live 来源。其他瓷砖可见的来源不会自动授权给当前瓷砖。

墙面媒体消费者不创建 Surface 附件、不加入 Surface viewer 名单、不生成虚拟设备，
也不占用原有设备的控制/输入序列或 controller。多个独立输出可以复用同一来源，
包括来源和输出在同一设备的情况。客户端只能向媒体连接发送 WebSocket 存活消息。

每轮发送前重新检查设备 token、批准/启用状态、设备 session epoch、呈现租约和布局。
布局变更、来源移除、设备撤销、租约过期或替换都会终止旧连接。等待和媒体写入分别
有 250 ms 上限，不在网络等待期间持有墙面锁。

断源检查发现了旧帧重放风险：终端重连时，服务端保留的最后一帧可能被再次显示。
修正后，墙面媒体要求 source 仍连接，且最后一帧按服务端单调接收时钟未超过 5 秒。
重连不会刷新这份帧年龄；源自行提供的时间戳也不能延长它。

Hook 原生消费者只接收 raw BGRA/sRGB，复用既有 NLLV 解码校验，使用二进制 IPC。
终端按独立渲染循环更新画面；3 秒控制心跳不承担帧率调度。图片与 Live 使用同一
投影、裁剪、旋转和层级顺序，全部可见内容就绪后才确认布局版本。

每个输出最多 4 条唯一 Live 流、每帧最多 16 MiB、单流一个待消费最新帧，
前端串行解码并关闭替换/迟到位图。关闭消费者不会提前释放仍在退出的原生线程名额。
失败清屏并退避重试，全部流失败时不会以 60 Hz 空转重连。

## 自动化验证

- Loom `cargo test -p loom-daemon wall_live`：2 项通过，包含真实 TCP/WebSocket、
  Ed25519 配对、管理员/外设备拒绝、半开边界可见性、旧布局、过期/替换租约、
  token 撤销、断源、超龄缓冲帧重连拒绝以及新帧恢复。
- Hook 原生 `cargo test --manifest-path src-tauri/Cargo.toml wall_live`：1 项通过，
  覆盖重复/旧 epoch 帧、压缩格式和保留位异常拒绝。
- Hook `TileLiveCache`、`TilePresenter`、`TileImageCache`：14 项通过，覆盖迟到
  open/decode 清理、同源去重、代际切换、失败清屏、重试退避和并发预算。
- 两仓编译检查和 Rust formatter 通过；Hook `lint`、`typecheck`、`typecheck:test`
  通过。两仓 `git diff --check` 通过。
- Loom strict 有效行数门禁扫描 995 个文件，保留 12 项既有软例外；Hook ratchet
  扫描 1136 个文件。没有新增软例外或突破 500 有效行的本阶段新文件。
- Hook 发布二进制 `--self-check` 返回 `status: ok`，`--tile-outputs` 返回实际
  3840 × 2160 输出。内部版本为 `v0.2.30.3`，公开版本仍为 `0.2.30`。

最终 Loom 候选通过官方 `verify-release.ps1 -RunSmoke`：50 个文件校验通过，
standalone、Hook canvas、error preview、framework Art store、plugin boundary、
Surface prototype、authored Art 共 7 组 smoke 全部通过。Hook 实际二进制摘要与
`build-provenance.json` 一致；两个候选均如实记录 `gitDirty = true`。

## 发布版运行证据

在隔离配置、设备身份和控制面目录内运行实际发布版 Loom daemon 与 Hook 终端。
终端实际 Canvas 为 3840 × 2160，CSS viewport 为 2560 × 1440，DPR 为 1.5。
来源为通过真实 Live 创建和发布接口发送的 40 × 40 NLLV 动态四象限测试程序；
它使用一个有效的来源 Surface 附件，墙面消费者没有来源附件。
这证明发布版媒体接入和显示链路，不代表本轮重新验收了真实窗口 WGC 采集。

修正后的最终候选已重复验证：

- 完整 Live：红/青交替的左上象限持续变化，绿、蓝、黄象限正确。
- 右半裁剪：输出为绿、绿、黄、黄。
- 90 度旋转：输出为绿、黄、红/青、蓝，没有镜像。
- 同源双位置：两处对应采样保持一致并共同变化；仍只有一个 Live 会话、一个发布源，
  `mediaConnections = 2`（来源 + 墙面消费者），`controllerLeases = 0`。
- 图片叠放：右半幅为不透明紫色图片，左半幅继续显示动态 Live；版本得到确认。
- 关闭 Hook 管理进程后，独立输出仍继续接收动态画面。
- 停止媒体来源后清屏，连续 12 秒共 120 次检查均无旧帧重现，覆盖多次自动重试。

初始候选另有未存在来源清屏/null 确认及恢复引用的记录。最终候选在来源重新连接并
发布更大 frame ID 后恢复动态画面，布局 revision 7 得到确认。输出按 Escape 退出后，
端点立即离线、`appliedRevision = null`，墙面仍保留；Live 会话和来源仍存在，
媒体连接数从 2 降至 1，证明关闭输出没有关闭来源。

随后停止本轮来源测试程序和 daemon。复核两批测试的 Hook/daemon PID 及 64706、
64707、63828、63829 端口均无残留。本机原有用户进程没有被结束。

证据与复现脚本：

- `Hook/artifacts/tile-live-release-20260910-final/`：最终候选进程路径、布局确认、
  各模式像素记录、断源连续黑场记录；其中配对身份和 manifest 含私有凭证，不复制到发布包。
- `Hook/output/playwright/tile-live-source.ps1`、`configure-tile-live.ps1`、
  `inspect-tile-live.ts`、`inspect-tile-live-black.ts`：隔离媒体测试脚本。
- `Hook/output/playwright/tile-live-final.png`：最终候选实际输出截图。
- `Loom/target/wall-live-tests.log`、`tile-wall-r3-live-build.log`、
  `tile-wall-r3-live-verify.log`：测试、构建与发布验证日志。
- `Hook/artifacts/wall-live-tests.log`、`wall-live-frontend-tests.log`、
  `tile-wall-r3-build.log`：Hook 原生、前端测试及发布构建日志。

## 尚未完成

TW-06 的点击、拖动、滚轮、键盘和异常释放仍需实现。既有 Live controller 按设备
分配，多个同设备瓷砖不能独立争用这一身份的序列；下一阶段需要明确端点级控制 owner，
同时保持传统源程序的一套键鼠状态和已有来源权限。

Art 交互、跨瓷砖手势、多用户交接、真实双机局域网、网络故障恢复、物理同步和延迟/
资源压力测量仍未验收。只有一块物理屏幕；本轮没有把合成媒体测试当作双机或真实源
程序点击成功。H.264/HDR、原生触摸/笔及无计算能力终端的解码出口也未完成。
当前 16 MiB 单帧限制不接收 4K raw BGRA 原始帧；系统 DNS 解析和浏览器位图解码
仍不能主动取消，已有并发名额在它们实际结束前不会被无限补充。

两个仓库均从 dirty 工作区构建：Loom HEAD 为
`cdb3dc825679e24c40cb2377159eb4b9ebda7a0d`，Hook HEAD 为
`9815e27ab3863d98b8bae80822228e03a2e2d264`。未创建提交、暂存或公开发布；无关工作区
改动、用户现有 Hook/Loom 进程以及旧候选目录均予保留。
收尾时 Loom 有 22 个 tracked 修改和 1614 个 untracked 文件，Hook 有 7 个 tracked
修改和 97 个 untracked 文件，两仓 staged 均为 0；这些数字包含此前工作区内容，
不能当成本阶段新增文件数量。
