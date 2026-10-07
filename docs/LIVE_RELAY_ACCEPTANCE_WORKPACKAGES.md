# Issue #67：小 PR 与剩余验收工作包

本页是 [优化计划](LIVE_RELAY_OPTIMIZATION_PLAN.md) 的执行索引，不替代原始回执，
也不把拆分任务当作完成验收。状态核对日期：2026-10-06 UTC。

## 当前状态

- A0、A1、A2.1、A2.2 已完成；A3 仍进行中。
- 已进入 main 的实现不重写历史、不追补成未合并 PR。后续使用独立 topic branch，
  一份可独立验证的成果、一仓库一个小 PR，逐项审阅、合并。
- 最新交接基线：Hook `5f812575861e384a4cdcee3f6be5f7596dfa4691`；
  Loom `01df39d84a602e16bdbec0fd73f9c081360b7193`。HEAD 不等于候选包的构建身份。
- `.23` 是内部 dirty candidate；原生证据只绑定原 EXE bytes，不能认证后来的 clean
  build 或正式发布。文档 PR 不改包、不增版本、不伪造二进制验收。
- 用户已明确恢复原资源超限项测试；本页 A3-R 对应项已复测结单。其他场景不随此
  自动扩展授权；无关程序和历史失败根保持不动。

## 已有证据，不重新计为待办

以下是既有记录的索引，不是本次重新执行的测试；版本和范围不能互相替代。

| 范围 | 已有结果及不能推出的结论 |
| --- | --- |
| 发布、加入、停止 | `.17` 默认 GPU 正向最小闭环；`.19` 反向真实 WGC 单观看端观察与停止清帧。不是所有后续版本的完整矩阵 |
| 内容与恢复 | `.20` 静态文字/滚动/运动；`.21` resize、显式重连、专用代理中断恢复。代理中断不是物理断网/网络切换 |
| 撤销与重新配对 | `.21` 双机 source/viewer 禁用与删除；`.23` 显式重新配对、新 deviceId 的 Surface 所有权、旧 relay 不复活。不是独立 token 轮换的完整验收 |
| 多连接和慢端 | daemon 1/2/4 媒体连接与不读取慢 socket 隔离、四连接协议长稳。不是 1/2/4 个独立原生 Hook 观看端 |
| 资源判因 | 历史 256.800781 MiB 失败保留；原尺寸 raw CDP 双机复测 100 样本/697 秒通过，source +13.460938 MiB，该项结单。不是所有负载无泄漏证明 |

固定证据入口：[多连接与早期长稳][connections]、[内容/恢复/撤销][lifecycle]、
[重新配对][repair]、[资源失败和短对照][resources]、[暂停前最后回执][pause]。

## 六组剩余工作包

工作包 ID 是稳定的验收分组，不是 GitHub PR 编号，也不是必须恰好产生六个 PR。
只有有真实改动或可复核证据时才建 PR；跨仓库改动分别建相互链接的 PR。
A3-R 的原资源增长项已关闭，其余工作包未关闭；发现产品缺陷时先提交证据，再以独立修复 PR 处理，不为通过而改阈值。

### A3-R：资源长稳（原增长超限项已结单）

- 本项结果：2026-10-06 UTC，原尺寸 `658×407`，30 秒预热、100 样本、697 秒，
  source/viewer/daemon Private MiB 增长为 13.460938/7.613281/1.364583，句柄、停止、
  清帧及双机清理通过。证据见优化计划末尾结单节；不认证其他负载或后来新包。
- Owner：Hook 原生采集/观看与测试驱动；Loom 汇总验收记录，按真实改动归仓。
- 恢复前提：用户明确恢复；在任何服务/远端候选启动前确认持续充足的内存余量，
  捕获前再次检查，保持 15% 严格前置门禁。不足即中止，不停止无关负载。
- 证据：真实 WGC → Loom → 另一台原生 Hook，正常发布/加入，精确包/进程/CDP owner；
  所有 Playwright setup session 退出后，唯一 raw CDP session 不启用 `Network.enable`，
  HTTP 探测关闭空闲连接，保存实际连接数、methods 和资源子进程 stderr。
- 关闭条件：30 秒预热、600 秒观察，沿用原资源统计口径和 256 MiB/128 handles
  增长门槛；真实帧推进、停止清帧、双机资源清理均通过。不得挑选更有利的采样端点。
- 不足或失败：保留原始失败，不用短测、GC 后数据、单机或 synthetic 源替代；
  判因报告可以单独合并，但本工作包仍未完成。

### A3-V：独立原生多观看端与慢端

- Owner：Hook 原生观看生命周期；必要时由 Loom 单独修复分发边界。
- 证据：真实源、明确独立身份的 1/2/4 原生观看端和真实慢端；记录每个进程/Surface/
  权限/队列/帧推进，区分物理设备、进程、窗口和 socket 数量。
- 关闭条件：慢端、取消或关闭一个消费者不拖停其他人，资源有界、旧帧不过期复活，
  各自清理通过；不能用已有 daemon 多连接报告代替原生结果。
- 允许按 2 端、4 端、慢端分多个 PR；每次只勾选已经验证的子场景。

### A3-L：真实设备丢失

- Owner：Hook WGC/DXGI 资源生命周期及其与 LiveRelay 的接线。
- 证据：真实 device loss 的原生错误与停流/恢复记录、generation 和资源释放；
  普通 resize、静态窗口、撤销或模拟异常都不能冒充真实设备丢失。
- 关闭条件：旧输出失效、无孤儿资源，恢复后重新绑定当前源且权限不复活。
  需要驱动重置/影响日常桌面的操作先取得明确授权；条件不足就保持待办。

### A3-N：物理网络与重新授权

- Owner：Hook 重连与 Loom 会话/授权分别负责，协议变化必须列新旧组合与合入顺序。
- 证据：实际断连/网络切换、daemon 重启及独立新 token/重新授权场景逐项记录；
  复用已验证的显式重新配对结论，不把它当作全部 token 场景已覆盖。
- 关闭条件：当前授权下恢复、源身份不漂移；停止、撤销、租约到期及旧 owner 的
  迟到恢复均 fail closed，不重放旧输入、不自动授予控制。
- 2026-10-06 补充 daemon 软件层回归：隔离 registry 将租约缩短至 500 ms 后等待真实时钟
  到期，已建立的 source/viewer WebSocket 均关闭、旧 token 重连被拒绝；viewer 到期不拖停
  有效 peer，且不误报显式撤销。`cargo test --locked -p loom-daemon live_media_device_auth
  -- --test-threads=1`：10/10。仅软件层，不替代默认 TTL 双机原生自然到期验收。
- 2026-10-07 补充同设备签名续签软件回归：`live_media_device_renewal.rs` 经真实
  challenge/session HTTP 签名签发不同 token；source/viewer 的隔离旧租约到期后，
  新 token 可恢复相同 device/session/epoch，旧 token 的升级与恢复仍被拒绝。
  断连清除既有控制租约，续接不自动获输入权、不重置控制序号；关闭会话、非成员
  和禁用设备保持拒绝。上述同一聚焦命令为 14/14（新增 4 项）。
  本项不改生产 TTL/协议/Hook，媒体帧为测试夹具；不是默认 15 分钟双机原生证明。
  PC3 管理 HTTP 与 SSH 本轮均不可达，未启动原生候选；自然到期、原生新 token
  续接和物理网络/daemon 重启子场景仍待验，不用本软件回归关闭 A3-N。
- 影响物理网络、共享服务或管理通道的操作另行确认；一项故障场景一个证据 PR，
  专用代理的已有通过记录不能关闭物理网络子项。

### A3-P：端到端性能对照

- Owner：Hook 与 Loom 按采集/编码/IPC/分发/解码阶段分别提供证据。
- 证据：相同尺寸、内容、清晰度与网络条件下，对照 JPEG/raw，覆盖静态文字、滚动、
  运动及目标观看端数；报告帧龄、卡顿、CPU/GPU、网络字节、资源与清理。
- 关闭条件：能把瓶颈和收益归到具体阶段；跨机时钟带校准误差，离散采样不能称为
  全帧 p95。数据不足先补测量，不凭编码枚举或 payload 下降宣称整体更流畅。

### A3-D：真实呈现证据

- Owner：Hook 实际接收/解码/呈现后端；Loom 只汇总自己可证明的发送层。
- 证据：received、decoded-submitted、合成与物理显示分别标注，关联当前
  source/epoch/frame/generation；覆盖坏图、隐藏、换源、迟到回调和停止。
- 关闭条件：目标设备有可复核的真实画面证据与测量误差；软件计数不冒充屏幕 FPS，
  若只有软件层证据则只完成该层，物理呈现仍待办。

## 有条件后续与总 Issue 关闭

B1（呈现调度）、B2（分发/转换）、C1（GPU 视频编码）、D1（屏幕墙缓存对照）
不是四项必须实现的欠账。按 A3 的瓶颈或实际负载触发，每项记录“采用/调整/不采用”
及证据；未触发不能伪称已实现。D1 复用已有缓存，不重复建设。

每份 PR 必须写清：工作包 ID、单一交付、仓库 owner、基线/实际包身份、通过条件、
实测命令与结果、未覆盖范围、依赖 PR/合入顺序及回退方式。文档/测量 PR 可独立合并；
产品修复须有聚焦回归和对应原生验证，不能只凭文档通过宣称产品问题已解决。

Loom PR 用 `Refs #67`，Hook PR 用 `Refs aiaimimi0920/Loom#67`，避免跨仓库误关联。
禁止使用自动关闭总 Issue 的关键字。已完成的小 PR 可合并关闭；合并不是等待整个 A3，
但关闭 PR 而不合并也不是功能交付完成。撤回改动使用新的 revert 提交，不重写 main。

只有六组必经验收、原计划的兼容/权限/资源/许可门槛，以及 B1/B2/C1/D1 的有据决策
都完成，才能人工复核关闭 #67。控制按钮/键鼠 UI、未测设备/网络、常驻部署与正式发布
继续明确列为未覆盖项，不因某个小 PR 合并而获得验收结论。

[connections]: https://github.com/aiaimimi0920/Loom/blob/01df39d84a602e16bdbec0fd73f9c081360b7193/docs/LIVE_RELAY_OPTIMIZATION_PLAN.md#L1293-L1469
[lifecycle]: https://github.com/aiaimimi0920/Loom/blob/01df39d84a602e16bdbec0fd73f9c081360b7193/docs/LIVE_RELAY_OPTIMIZATION_PLAN.md#L1470-L1890
[repair]: https://github.com/aiaimimi0920/Loom/blob/01df39d84a602e16bdbec0fd73f9c081360b7193/docs/LIVE_RELAY_OPTIMIZATION_PLAN.md#L1917-L1974
[resources]: https://github.com/aiaimimi0920/Loom/blob/01df39d84a602e16bdbec0fd73f9c081360b7193/docs/LIVE_RELAY_OPTIMIZATION_PLAN.md#L1975-L2091
[pause]: https://github.com/aiaimimi0920/Loom/blob/01df39d84a602e16bdbec0fd73f9c081360b7193/docs/LIVE_RELAY_OPTIMIZATION_PLAN.md#L2118-L2183
