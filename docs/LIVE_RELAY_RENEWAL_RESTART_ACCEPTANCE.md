# A3-N：自然续期与专用 daemon 重启验收

核对日期：2026-10-07 UTC。本文只记录明确候选、场景和证据，不关闭总 Issue #67。
其余工作包见 [执行索引](LIVE_RELAY_ACCEPTANCE_WORKPACKAGES.md)。

## 候选与兼容边界

本次复用 Hook `v0.2.32.24` 内部 dirty candidate 和配套 Loom daemon：

| 产物 | SHA-256 |
| --- | --- |
| Hook EXE | `4824d65de41fdf2a521768979e56a69559bdf893746194ddabffb55de5c3d5a0` |
| Loom daemon EXE | `acf6fa5e70ca5ad826f85c0ef9ff6145adaa7b8d68168081771ead225dd7df44` |

新 Loom 的成员快照提供 actor-scoped `requesterControl`；新 Hook 依此安全恢复
已有成员的 control/input 序号。旧 Hook 首次加入新 Loom 的原协议路径保留；
新 Hook 首次加入旧 Loom 不需要该字段，但已有成员重加入或已有源恢复时，
旧 Loom 缺字段会明确拒绝，而不是回退到 sequence 1。此表是代码契约，
不是声称本轮已经运行全部新旧包组合。部署顺序应先 Loom、后 Hook。

源码仍有未提交修改，不能将上述包当作 clean-source 正式发布；本次仅新增
回归和验收记录，没有再次改变生产运行码，不伪造新的 EXE 构建身份。

## 默认 15 分钟自然到期：目标缺陷通过

证据根：`GameEditor/linshi/issue67-a3n-fixed-renewal-20261007-r3`。
权威复核为 `reconciled-acceptance.json`，保留原 `runner-receipt.json` 失败记录。

- 真实 WGC → Loom HTTPS/WSS → PC3 原生 Hook，193 样本、960.079 秒；无 TTL
  覆盖、系统时钟修改、模拟媒体源或输入控制权申请。
- 本机与 PC3 的旧凭证分别在约 900.047、900.035 秒返回 HTTP 401。
- Source 自动恢复，同 session/device/epoch；4 个 UIA 观察项持续推进，示例序号
  719 → 811 → 823，最终 `stable`，不再持续 `observation_publish_recovering`。
- Viewer 正常关闭旧观看、刷新并重新加入，帧号 19442 → 19682。不是旧观看
  窗口原位自动续期。完整服务端日志 HTTP 409 为 0。
- 原汇总错误地只统计第 900 秒后的签发，漏掉 PC3 在第 876.334 秒的刷新；
  `Hook/src-tauri/src/device_session/cache.rs` 原有 30 秒提前刷新策略解释了该记录。
  独立复核同时要求观察到新签发和旧凭证实际过期，未改原始失败回执。
- 该轮审计提前进入清理，未执行续期后的停止发布按钮步骤；正常程序退出、专用
  进程/监听清理和原日常 Hook 恢复通过。不能改写为完整 runner 全绿。

## 专用 daemon 重启、重新加入与停止：通过

证据根：`GameEditor/linshi/issue67-a3n-daemon-restart-20261007`。
`runner-receipt.json`、`daemon-restart-proof.json`、`reversed-stop-receipt.json`、
`reversed-closed-receipt.json` 和清理记录均通过。

- 用户明确允许暂退日常 Hook；隔离数据和唯一测试目录，不重启常驻服务。
- 按 PID/creation/path 精确核验后终止专用 daemon，停机 5 秒，使用相同 EXE、
  存储和配置重新启动；PID 33088 → 32852，恢复管理 bridge。TLS 代理未替代
  daemon 重启，Hook 两端进程和捕获未重启，也未改变物理网络。
- Source 自动恢复，无需点击重新连接；保留 relay/session/capture/source device
  身份，源帧号 251 → 586，重新建立 4 个 UIA 观察项，后续序号达到 10。
- 原 Viewer 进入 recovering；正常关闭并重新加入同源后，真实 JPEG 解码提交
  帧号 621 → 721，权限仍为非控制状态。软件帧号不冒充物理屏幕 FPS。
- 正常点击停止发布后服务端 `closed=true/sourceConnected=false`；Viewer
  `connectionState=closed`、`presentation=null`、`remainingImages=0`。
- 自有进程和监听清理完成，日常旧 Hook 按原路径和 SHA 恢复；无无关进程清理。

这关闭的是本候选、单源单观看端、专用 daemon 突然终止后的恢复子场景，
不代表所有重启时序、服务升级迁移或自然到期后停止的同一轮矩阵均已覆盖。

## 新增自动回归

Hook `__tests__/unit/LiveRelaySourceRecovery.test.ts` 新增 6 项：失败恢复至少
5 秒节流；不可恢复/终态不重试；stop、dispose 后迟到结果不复活；在途恢复
不重叠；旧 owner 的结果不覆盖替换后的 generation。与相邻轮询测试合计 18/18：

```powershell
node node_modules/vitest/vitest.mjs run __tests__/unit/LiveRelaySourceRecovery.test.ts __tests__/unit/LiveRelayPollCadence.test.ts --pool threads --maxWorkers 1 --no-file-parallelism
```

这些是 mock API 的控制器回归，不冒充原生 recreated 分支的完整软件集成测试。
生产重建分支由上述真实 daemon 重启场景覆盖；全部错误注入和取消交错仍不能
由一次成功原生运行推出。

### 恢复基线读取的 HTTP 兼容回归

Hook `native/tests/live_relay_renewal_http.rs` 补充 5 项生产 HTTP client 分支测试：

- 首次加入返回 control/input `0/0`，下一控制序号为 1，不读取成员恢复游标。
- 非法 viewer membership 不视为首次加入，不发出 HTTP 请求。
- 已有成员收到旧式快照（实际缺失 `requesterControl`，不是字段为 null）时，
  明确返回 `Loom does not support safe live cursor recovery; upgrade Loom`，不重置序号。
- 新式快照保留 actor 的独立 control/input 位置，未知扩展字段不改动这些位置。
- 游标 GET 收到 401/403/409 时，即使错误体声明 retryable，也只请求一次并拒绝降级。

从 Hook 根目录执行聚焦及相邻 viewer 生命周期回归：

```powershell
rtk proxy cargo test --locked --release --lib --manifest-path src-tauri/Cargo.toml live_relay_viewer_lifecycle_tests -- --test-threads=1
rtk proxy cargo test --locked --release --lib --manifest-path src-tauri/Cargo.toml live_relay_renewal_tests -- --test-threads=1
```

2026-10-07 实测：viewer 生命周期 20/20（包含上述新增 5 项），游标/UIA 续期
回归 2/2。最终执行在直接 include 文件格式检查及重新编译后完成。

夹具只监听随机 loopback 端口，请求和 worker 等待有界，析构停止并 join；没有
真实设备配对、凭证续签、原生 Hook 启动或旧发行包。此证据限定为恢复基线 GET，
不覆盖 cursor 读取后 attachment POST 的并发冲突，不代替四种新旧包组合验收。

## 剩余边界

A3-N 的物理断连/网络切换仍待明确授权和实测；A3-V 的 2/4 个独立原生观看端
及慢端、A3-L 的真实 device loss、A3-P 的受控性能对照、A3-D 的物理呈现证明
均未关闭。Hook 当前 Windows 会话内是应用级单实例，不能把同进程多窗口或
同设备多个 socket 冒充独立原生观看端。B1/B2/C1/D1 仍按实际瓶颈触发，不自动
变成必须追加的功能开发项。没有常驻部署、正式发布、Git 提交或推送。
