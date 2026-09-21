# 瓷砖墙阶段 5：声明式 Art 与输入生命周期

2026-09-12。声明式 Art 的本机发布版交互、压力和恢复验收已通过。
本阶段使用一块真实物理输出，完整瓷砖墙计划仍未完成。完整清单见
[实施计划](TILE_WALL_IMPLEMENTATION_PLAN.md)。

## 当前候选

- Loom：`Neuro/release/Loom/20260912-tile-wall-r6-art-bounded/Loom.exe`。
- 无头服务：同目录 `runtime/loom-daemon.exe`。
- Hook：`Neuro/release/Hook/v0.2.30.6-art-ordered/hook.exe`，内部版本 `v0.2.30.6`。
- 候选来自 dirty 工作区，没有创建提交或公开发布。Art 重建没有再次递增内部版本。
- 早期 `art-final`、`art-fixed`、`art-bounded` 候选及失败证据全部保留。

SHA-256 与本轮读取的实际文件、provenance 一致：

```text
Loom.exe
50d7bc14870351f8d4d896ec7fb18bfa3b23da3c6fc9570f6ab1fb1c1e74b3b2
runtime/loom-daemon.exe
4ddc9843a4dede7499b704e8e08e82630dc98f79f962e13d4e9d354b1102ca28
hook.exe
a19955423c579db58f4199b8ec590c69e9e739bda2931a44ee5485dc9cba2063
```

## 已实现的边界

墙面复用已有 Art 实例；同一输出上的重复位置共用一个临时附件。声明式场景、资源、
裁剪与旋转复用既有 Surface 和墙面合同。终端不执行 Art 脚本，不接收源程序路径或
管理员凭证。高风险动作使用宿主确认，已接受且可取消的执行使用既有取消合同。
临时预览与正式结果分别检查版本，普通来源和正式结果不随墙面附件关闭而删除。

无关场景更新保留输入元素、焦点和选区。文本草稿等待实际执行结果；HTTP 接受回执
不会提前覆盖本地新输入。绝对值编辑可在队列中等待 30 秒；同一视图内排在本地编辑
后面的提交动作具有 30 秒总等待上限，到达队首后最多用 1.5 秒重新校验并发送。
独立点击仍有 1.5 秒过期保护。编辑失败或执行等待超时会丢弃后续队列并显示原因，
不重发结果不明确的修改。`aria-busy` 包含排队及执行中的本地值编辑。

每个临时附件最多保留 64 个回执。达到容量后回收最早完成的记录，不淘汰未完成动作。
连续动作也计入待完成工作，其 payload 不进入持久化 pending 队列。临时回执和所属
关系不写入普通来源记录；关闭视图只清理该视图历史。重复使用其他附件的 event ID
返回 HTTP 409；同视图的保留 ID 仍可幂等查询。详细合同见
[Wall Surface API](../protocol/WALL_SURFACE_API.md)。

## 原生运行记录

运行入口为 Hook 的 `scripts/tests/Invoke-TileWallArtProbe.ps1`，使用发布二进制、
真实配对、独立控制面、已安装的 PowerShell Art 原型及一块 3840 × 2160 物理输出。
Playwright 驱动真实 WebView2/Tauri 页面。测试没有绕过普通 Hook 全局单例。

- I：`Hook/artifacts/tile-art-r6-i`。40 字符草稿保留，但来源只收到首字符。
  首次执行约 6.7 秒后返回，原有 1.5 秒队列期限导致后续编辑过期。先用回归复现，
  再为绝对值编辑增加有界等待。
- J：`Hook/artifacts/tile-art-r6-j-bounded`。连续输入、焦点和三批跨字段编辑通过；
  批次耗时为 16180 / 14813 / 12313 ms。首次提交排在尚未完成的编辑之后过期，
  未出现确认框。已增加依赖编辑的动作排序及聚焦回归。
- K：`Hook/artifacts/tile-art-r6-k-ordered`。使用当前两份候选。连续输入和首批跨字段
  编辑后，Art 返回 `surface_action_timeout`：动作超过其声明的 10000 ms 预算。
  终端显示固定的 `wall_surface_action_failed`，后续编辑没有继续执行。

K 的失败发生在确认、取消、完整资源/层级、压力和重启恢复阶段之前。
K 的 `art-failure.json`、`rapid-input-trace.json` 和
`process-samples.json` 保留了失败状态及进程证据。

失败后独立测量空 PowerShell 启动为 7891 / 6469 ms，表单脚本为 7047 / 6568 ms；
改用本机临时目录启动仍为 6941 ms。同期 12 个逻辑 CPU 的采样负载均为 100%，
可用内存约 2.1 GiB。当前证据支持宿主负载使 10 秒执行预算不足，未据此放宽 Art
预算或重试修改操作。上述批次和进程启动耗时均不代表物理显示延迟。

I、J 的清理记录 `remaining` 为空。K 清理时有一个 WebView2 子进程未在 5 秒内退出；
随后按记录中的 PID 复查，该进程已不存在。K 的原始失败清理记录保留，没有改写为
成功。用户已有 Hook、Loom 及其他任务进程未被停止。

L：`Hook/artifacts/tile-art-r6-l-ordered`。完整 Art 交互和输出重启恢复通过；压力到
第 7 批、30 个不同回执时，采样脚本读取已退出进程的 `TotalProcessorTime` 空值而
失败，清理 `remaining` 为空。已用聚焦回归复现同一个 `TotalMilliseconds` 错误。
采样现在检查创建时间、跳过退出/不可用测量并释放临时进程句柄；清理身份的记录与
资源采样独立，测量失败不会失去子进程清理责任。Live probe 复用相同修正。

最终 M：`Hook/artifacts/tile-art-r6-m-ordered/summary.json`，全部阶段通过：

- 40 字符连续键入保留最新草稿与焦点；跨字段编辑的源状态正确，三批耗时为
  13114 / 13455 / 13574 ms。
- 拒绝确认不产生正式结果；批准后取消的动作没有迟到提交；再次批准产生包含
  `Tile Art` 和 `Real process runtime` 的正式结果，`resultRevision = 1`。
- 已安装 dashboard Art 的资源经受限资源通路解码。此原型资源为 1 × 1 像素，
  该证据不代表大图片吞吐量或解码压力已验收。
- 同实例两处放置共用一个临时附件；透明图片只在低层 Art 上混合一次，高层 Art
  遮住中间媒体。95% 裁剪及 90 度旋转后的输入仍到达正确来源。
- 后续编辑没有替换上次正式结果；关闭管理窗口后仍可交互；输出强制退出后普通
  来源保留，临时附件及其回执清理。新输出接回相同普通附件并继续编辑。
- 压力阶段运行 273281 ms，18 批跨字段编辑，观察到 74 个不同回执，最大保留
  64 条；附件数量保持为普通来源加一个墙面视图，正式结果不变。
- daemon 停止后场景与媒体清空；同一持久化目录重启后恢复场景、资源和正式结果，
  并再次接受输入。输出正常退出后普通来源保留，墙面回执历史清空。

M 的 daemon、重启 daemon、管理窗口、输出与重启输出实际可执行路径均与候选一致，
二进制摘要与 `summary.json` 一致。最终 `cleanup.json` 为 `passed: true`、
`remaining: []`。截图位于
`Hook/output/playwright/tile-art-tile-art-r6-m-ordered.png`，包含有意旋转的混合布局。

## 压力资源观测

`process-samples.json` 共 250 次采样，其中压力阶段 149 次，覆盖约 273 秒。
以下 CPU 数据使用同一 PID 与创建时间的进程累计 CPU 时间差，按一个逻辑核心的
100% 归一；WebView2 行为 6 个持续存活进程的总和。短命 PowerShell/Framework
子进程的完整 CPU 生命周期未计入此表。

| 进程组 | 平均 CPU | 私有内存首/末/峰值 MiB | 句柄首/末/峰值 |
| --- | ---: | ---: | ---: |
| Loom daemon | 5.0% | 6.3 / 8.0 / 8.2 | 163 / 154 / 165 |
| Hook 输出 | 7.2% | 6.5 / 7.0 / 7.0 | 386 / 385 / 390 |
| WebView2 | 19.2% | 222.6 / 275.1 / 277.2 | 2781 / 2810 / 2847 |

18 批输入到权威源完成的中位数为 13762 ms，P95/最大值为 17370 ms；每批包含
两个字段的 input/change 及实际 PowerShell 执行。此测量不代表物理显示延迟。
WebView2 私有内存首末增加约 52.6 MiB；本次短样本没有证明长期内存稳定性，也不能
单凭进程私有内存认定泄漏。TW-08 的更广压力、稳态资源和显示延迟验收继续保持未完成。

## 已通过的门禁

- Hook 最新输入队列、控制器和事件回归：3 个测试文件，12 项通过。
  类型检查、测试类型检查、lint、独立 probe TypeScript 检查通过。
- Hook 行数门禁扫描 1174 个文件，没有超过 500 有效行的文件。
- 进程采样回归、真实存活进程测量及 4 个相关 PowerShell 脚本语法检查通过。
- Loom：4 项新增回执测试、30 项 wall 测试通过；受影响的 `surface_` 库测试
  97 项通过、265 项过滤。覆盖双方 event ID 越权、连续动作清理、确认持久化排除，
  以及连续/离散动作各 192 次完成后的容量和普通历史保留。
- Loom formatter 和 include 片段的 rustfmt 检查通过；strict 行数门禁扫描
  1013 个文件，保留 12 项既有有效软例外，没有新增例外。
- Loom 官方包校验 50 个文件，7 组 smoke 全部通过：standalone、Hook canvas、
  error preview、framework Art store、plugin boundary、Surface prototype、authored Art。
- 当前 Hook 包通过构建、摘要/provenance 一致性、`--help`、`--self-check` 和
  `--tile-outputs`；自检返回 `ok`，实际输出枚举为 3840 × 2160。
- 两仓此前通过的依赖安全契约、真实 OSV 扫描及 Neuro 通用规范契约仍适用于当前
  未改变的依赖与发布工具。最终差异、链接和编码审计在本阶段收尾执行。

主要修改文件的当前有效行数：

| 仓库与文件 | 有效行 |
| --- | ---: |
| Hook `tileSurfaceController.ts` / `tileSurfaceEvent.ts` | 210 / 34 |
| Hook `TileSurfaceLayers.tsx` | 89 |
| Hook 输入队列 / 控制器 / 事件测试 | 138 / 118 / 27 |
| Hook `DeclarativeSurface.tsx` | 454 |
| Hook `probeArt.ts` / `stressArt.ts` / Art probe 启动脚本 | 226 / 62 / 179 |
| Loom `surface_store/ephemeral_events.rs` / 对应测试 | 110 / 247 |
| Loom Surface executor / wall 请求跟踪 | 441 / 165 |
| Loom wall Surface routes / views / HTTP 测试 | 263 / 305 / 398 |

## 后续验收

本阶段上述发布版 Art 路径已有完整通过结果。后续源码修改需要重新运行受影响的
聚焦测试及相应原生阶段；已有的单屏结果不能替代真实双机、多用户和长期资源验收。

冻结/黑场/恢复、物理识别与管理、普通 Hook GUI 源、完整来源恢复、调度/媒体时钟、
更广的资源测试、真实双机和多人验收仍按实施计划保留为未完成。普通 Hook GUI 源
验收需要现有实例正常退出；双机验收需要第二台实体电脑及已授权的远程执行方式。
