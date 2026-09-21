# 瓷砖墙阶段 4：Live 输入与恢复

2026-09-11。已完成端点级 Live 输入及本机原生闭环验证。完整实施计划仍在进行：
Art 输入/呈现、调度、冻结/黑场、真实双机与长时间压力验收仍未完成。

## 候选版本

- Loom：`Neuro/release/Loom/20260911-tile-wall-r5-flow/Loom.exe`。
- 无头服务：同目录 `runtime/loom-daemon.exe`。
- Hook：`Neuro/release/Hook/v0.2.30.5-input/hook.exe`，内部版本 `v0.2.30.5`。
- 使用 `hook.exe --tile` 管理输出；选择物理屏幕后，在 Loom 屏幕墙页放置已有 Live 会话。
  点击画面取得操作权，输出页 Escape 退出。启动及能力限制见
  [Hook 瓷砖终端](../../Hook/docs/TILE_TERMINAL.md)。

这些是 dirty 工作区内部候选，没有公开发布。旧的 `v0.2.30.4`、`v0.2.30.5`、
`20260911-tile-wall-r4-input` 和 `20260911-tile-wall-r5-input` 保留供核查；它们没有包含
本轮全部修正，不作为本阶段推荐候选。

SHA-256：

```text
Loom.exe
e087c5ae78cf81c5a29b92e3e0577d79b931cece56404b8d56701a0718f7ebfa
runtime/loom-daemon.exe
f7fa92cd1eaf65b6fc11f8fccc9012e14f6973ab3f4d3aa0817d52185dbb0b3c
hook.exe
ae795973b676d5a325323ffa5ce8f47f1e3d4ae087c99d67680043ea8a894baa
```

## 实现与修正

端点输入绑定设备身份、呈现租约、布局版本和独立控制 ID。每条可靠边沿必须使用下一
个序列号；同一来源在普通 Live viewer 与全部瓷砖之间只保留一个控制者。输入沿既有
Live 源事件流回源，没有把设备身份伪造成新 viewer，也没有重复启动采集。
API、队列与租约细节见 [WALL_INPUT_API.md](../protocol/WALL_INPUT_API.md)。

本轮用回归或运行证据修复了以下问题：

1. 输入队列同步取消时可能重入，退出流程会提前完成而未等待释放。队列先建立异步
   owner，再执行 drain；对应测试先失败后通过。
2. 串行网络请求阻塞了物理输出丢失检测。新增独立 500 ms 监视循环，共享至多一个
   原生查询；几何/授权失效撤销代际，迟到回复不能恢复旧显示或输入。六秒授权超时
   同时覆盖静态和动态内容。
3. 操作权提示错误地复用了全屏错误遮罩，会遮住画布并触发 pointerleave。现在使用
   不拦截指针的小标签；实际浏览器 hit-test 已验证底层画布仍可操作。
4. WinForms 首次输入处理实测约 100-140 ms，原先 50 ms 会误判超时。普通离散边沿
   允许 250 ms，移动和清理仍保留 50 ms。隐藏/遮挡的 WinForms push button 通过
   明确抬起后的标准按钮通知补全 Click；拖动、取消和失效目标不会触发该动作。
5. Live 事件长轮询持有全局串行路由锁，输入被自己的接收轮询阻塞。真实 HTTP 回归
   首次测得控制请求等待约 867 ms，且轮询在产生事件之前超时。只读 Live 轮询改为
   在自身 store 的条件变量上并发等待；回归确认控制事件可以直接唤醒轮询。

## 原生运行证据

最终证据：`Hook/artifacts/tile-wall-native-input-r5-client-region/summary.json`、
`input-result.json`、`recovery-result.json`、`source-summary.json`、`cleanup.json`。
实际输出截图：
`Hook/output/playwright/tile-input-tile-wall-native-input-r5-client-region.png`。

使用真实 3840 × 2160 物理输出、发布版 Hook 终端及发布版 Loom daemon。源端是独立
WinForms 程序；采集和输入由 Rust 测试进程中的生产 WGC / Live relay / HWND 输入代码
承载。明确选取客户区作为捕获区域，使 fixture 的目标坐标与可见像素一致。终端通过
真实配对认证连接，操作由 Playwright 驱动真实 WebView2/Tauri 页面完成。

全部通过：

- 点击触发真实按钮动作；拖动产生源事件并释放按钮，未被错误补全为点击。
- 键盘产生真实 down/up；滚轮把源滑杆从 40 改为 37。
- 95% 裁剪与 90 度旋转后仍命中相同源按钮。
- 布局替换、浏览器 blur 事件和输出进程强制退出均释放源端按键/按钮。
- 关闭 Hook 管理窗口后，独立输出继续显示变化像素并接受输入。
- 输出进程重启后复用相同 source PID、captureSessionId 和 Live session；源按钮状态
  从 24 次点击继续增长到 25 次，没有新建源程序或采集。
- 关闭来源后实际 canvas 清空旧像素。源线程正常 join，源输入计数归零，输出 Escape
  退出；进程树清理记录的 remaining 为空。

记录了 24 个“发出点击到读到源程序效果”的样本：中位数 **74.3 ms**，P95
**122.9 ms**，最大 **233.8 ms**。这包括 50 ms 状态采样间隔，不是物理触摸或显示
光子延迟。另保留进程 CPU/内存/句柄快照，未将这些快照当作长期资源稳定性证明。

daemon、管理窗口、初始输出和重启输出的实际进程路径均与候选路径一致。没有绕过
普通 Hook 的全局单例；用户现有 Hook/Loom 进程保留。源端测试进程不等同于普通
Hook GUI 的完整启动验收；该路径仍需在现有普通 Hook 正常退出后补充。

## 门禁与代码规模

- Loom：4 项 `wall_input` 真实 HTTP 测试和保守路由分类测试通过；Rust formatter、
  strict 行数门禁通过，1000 个文件保留 12 项既有软例外，没有新增例外。
- Hook：输入队列/呈现生命周期 14 项测试通过；按钮手势 2 项、既有超时释放 1 项、
  relay 输入 2 项通过；最终原生 source 测试通过。lint、测试类型检查、独立 probe
  TypeScript 检查、Rust formatter 和 ratchet 行数门禁通过，扫描 1146 个文件。
- 行数 checker 测试：Loom 15 项；Hook checker/bootstrap 17 项通过。
- 两仓依赖安全契约和真实 OSV 扫描通过，沿用已有例外，没有新增豁免。
- Loom 官方 `verify-release.ps1 -RunSmoke` 校验 50 个文件，standalone、Hook canvas、
  error preview、framework Art store、plugin boundary、Surface prototype、authored Art
  共 7 组 smoke 全部通过。Hook `--self-check` 返回 `ok`，摘要与 provenance 一致。
- Neuro 通用开发规范契约通过。两仓 `git diff --check` 通过；Loom 测试仍报告既有
  `remove_test_dir` 未使用警告，不影响测试结果。

本轮实质修改/新增的主要手写文件有效行数：

| 仓库与文件 | 有效行 |
| --- | ---: |
| Hook `tileInputController.ts` / 对应测试 | 109 / 99 |
| Hook `tilePresenter.ts` / 对应测试 | 127 / 111 |
| Hook `TileTerminal.tsx` / `TileTerminal.css` | 73 / 16 |
| Hook `native/live_source_input.rs` | 438 |
| Hook `native/live_source_window.rs` | 423 |
| Hook `native/live_source_button.rs` | 149 |
| Hook `native/tests/wall_input_source_acceptance.rs` | 227 |
| Hook `src-tauri/src/lib.rs` | 262 |
| Hook `Invoke-TileWallNativeInputProbe.ps1` | 166 |
| Hook `probeSession.ts` / `prepareSource.ts` / `probeNativeInput.ts` | 55 / 50 / 203 |
| Loom `runtime/connection_dispatch.rs` | 466 |
| Loom `tests/wall_input_http.rs` | 392 |

所有文件均低于 500 行且为 UTF-8 无 BOM。既有 input owner 从 412 增至 438 行，用于
接入独立按钮策略及区分输入/清理时限；窗口 owner 从 408 增至 423 行，其中包含新增
手势字段及 formatter 展开。队列、取消、跨线程 HWND、UIPI、缓存和资源生命周期已按
本轮职责审查。无关旧债没有被当成本轮已清偿内容。

## 后续边界

完整清单见 [实施计划](TILE_WALL_IMPLEMENTATION_PLAN.md)。Art Surface 呈现及 action /
确认/取消、低能力终端的完整协商出口、跨终端多人操作、冻结/黑场、场景调度、媒体时钟、
显示反馈延迟、流量和长期压力测试继续保留为未完成。当前只有一个已核验的物理输出；
逻辑设备身份和同设备双输出 HTTP 测试不能代替两台实体终端的局域网联合验收。

两仓仍为独立 dirty 工作区、无 staged 变更，未创建提交或公开发布。Loom HEAD 为
`cdb3dc825679e24c40cb2377159eb4b9ebda7a0d`，Hook HEAD 为
`9815e27ab3863d98b8bae80822228e03a2e2d264`。测试凭据只保留在隔离证据目录，不进入发布包。
