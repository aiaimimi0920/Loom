# Issue #67：新候选组合默认 900 秒自动续期验收

## 结论与版本边界

2026-10-09，PC1 `DESKTOP-MVG82SN` 与 PC3 `CODE` 的隔离测试身份和会话完成
默认 900 秒自动续期、原窗口恢复渲染、正常源端停止及双机清理。
正常停止后的观看端清帧没有获得本轮有效回执，仍待补验。整个 A3 / #67 未完成。

- Hook `.32`：`release/Hook/v0.2.32.32-viewer-renewal-20261009/hook.exe`
  - SHA256：`6911eb0a70b02baf19aa1320c36928c4e5d35cf3aae3852f66f227f62433f840`
- Loom：`release/Loom/issue67-disabled-auth-20261009/loom-daemon.exe`
  - SHA256：`8132707e238e9a1e439a0d6a9543e05c58a8927ff9d7a2c3887b8616ec3df806`

两份都是已有内部候选，本轮重新核验字节，没有重建或重新标记成合并后正式包。
测试没有退出日常 Hook、改 TTL/时钟、物理断网、驱动重置或修改系统证书信任。
媒体经真实 LAN HTTPS/WSS；SSH 只用于管理和 CDP。停止竞态及禁用本轮未重跑。

## 实际结果

- 正常发布、Surface 加入、真实 WGC 采集与可见渲染前置通过：80 样本、59 个不同绘制帧。
- 续期观察 351 样本，末次凭据年龄 962.735 秒；自然 HTTP 401 出现在签发后 900.035 秒。
- 自然 401 完成：`2026-10-09T09:04:21.484Z`；新签名会话 201 完成：
  `2026-10-09T09:04:21.558Z`。挑战 201 与新会话 201 均在拒绝完成之后。
- 到期窗口内 `09:04:21.699Z` 实采图像、presentation、rendering 清空。
- 独立后置复核限定 fresh 201 之后的实际绘制时间；generation 从 1 升到 2。
  `09:04:22.004Z` 至 `09:05:24.231Z` 采到 65 个不同绘制帧，持续推进 62.227 秒。
  原窗口、文档时钟、几何、session/device/epoch 连续性通过；未取得输入控制权。
- PC3 可见访问日志中，自然拒绝至观察结束没有新发起的登记请求。
  更早存在独立登记活动；不声称全应用零登记，也不覆盖窗口前发起、窗口内完成的请求。
- 正常 UI 停止发布后服务端 `closed=true`、`sourceConnected=false`。
- `09:06:34.810Z` 清理复核通过：两机 owned 进程及测试监听端口无残留，专用任务禁用。

这是 WebView2 `browser_element_render` 软件证据，不是物理屏幕呈现或 FPS 测量。

## 观察器缺口与原始证据保留

原 runner 的 `normal stop and clear` 标签和 `stopPassed=true` 实际只覆盖源端停止。
额外只读停止观察器在 `09:04:21.557Z` 将到期时的
`closed/live_viewer_authorization_required` 误当成正常停止，因预期 `errorCode=null`
而退出。因此其失败是取证脚本阶段识别错误，不是续期失败，也不能作为正常停止清帧证据。
原脚本及失败回执未改写；不以原 runner 的总 `passed=true` 掩盖此缺口。

续期帧计数另有独立后置复核，排除提前瞬断重连的帧被计入自然续期稳定窗口。
后置复核 5 项测试通过：真实记录、拒绝提前帧、拒绝旧 generation、缺失清帧、短稳定期。
此前 28 项脚本聚焦检查和启动/传输门禁按本轮已有记录复用，不冒称再次运行。

## 证据入口

本地根：`GameEditor/linshi/issue67-newpair-renewal-20261009`。

- `final-acceptance-receipt.json`：分项结果及原始证据 SHA256 索引。
- `native/automatic-renewal-observation.json`：原始完整样本；SHA256
  `b445ec56bfb43ae3bd830b6aa2587f1f7182be75c1f11a6721f73d77f4c664ab`。
- `native/independent-renewal-timeline-receipt.json`：fresh 201 后的严格帧时间复核。
- `native/independent-normal-stop-receipt.json`：保留失败，不能当正常停止通过。
- `native/reversed-stop-receipt.json`、`native/part-cleanup-verified.json`：源端停止与清理。

只提交脱敏结论；测试身份、凭据、私钥、完整运行目录不进入 Git。

## 正常停止专项的后续启动尝试

2026-10-09T09:22Z 已准备不含 900 秒续期的最小专项，改为正常 UI 停止确认后，
再调用观看端 `closed` 检查，避免把到期终态误认成正常停止。
脚本语法、UTF-8、传输依赖闭包、ValidateOnly 及 PC3 23 文件传输哈希均通过。
但 PC1 启动前可用内存仅 8.286%（2699.742 MiB），低于既定 15% 门槛，
runner 在启动服务及两端候选前退出。未降低门槛、终止其他进程或盲目重试。
本机清理检查通过；额外远端复核确认无 Hook、无测试端口监听、专用任务未创建。
此项仍为环境阻塞、未执行，不是产品失败；不改变此前续期通过结论。
证据：`GameEditor/linshi/issue67-newpair-stop-20261009/final-preflight-receipt.json`。
