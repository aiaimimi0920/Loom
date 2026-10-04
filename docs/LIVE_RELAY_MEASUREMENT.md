# LiveRelay 有界基线采样

这是 [Issue #67 优化计划](LIVE_RELAY_OPTIMIZATION_PLAN.md) 的 **A1 工具**，不是完整的
两机性能验收。它只读取既有 `GET /v1/live/sessions/{sessionId}`，不改变采集、媒体分发、
观看者、控制权、设备配对或当前运行实例。Node.js 22 即可运行，无新增依赖或服务。

## 使用

先通过正常 Hook 产品入口发布并让受权观看端加入；明确所用 Loom origin、session ID 和
source/viewer 包 SHA。不要用 QR、屏幕墙或临时桥接结果替代 LiveRelay 两机基线。

操作者在进程环境变量 `LOOM_MEASURE_AUTHORIZATION` 中提供**已经获准**读取该会话的
`Device <device-session-token>` 或 `Bearer <administrator-token>`。优先复用已有成员权限；
工具不会获取或提升权限。不要把 token 放进命令行、脚本、截图或仓库。

```powershell
# LOOM_MEASURE_AUTHORIZATION 已由当前受信进程环境提供，不在命令中填写凭证。
node .\scripts\measure-live-relay.mjs `
  --base-url https://loom.example.test `
  --session-id live-session-example `
  --duration-seconds 60 `
  --interval-ms 500 `
  --timeout-ms 3000 `
  --output C:\Users\Public\nas_home\AI\GameEditor\linshi\live-relay-sample-unique.json
```

`--base-url` 必须是 origin，不允许用户名、路径、query 或 fragment。远程要求正常校验证书
的 HTTPS；私有 CA 可用 Node 标准 `NODE_EXTRA_CA_CERTS`，不能关闭 TLS 校验。
HTTP 仅允许 IP loopback（如 `http://127.0.0.1:PORT` 或 `http://[::1]:PORT`）。
不会跟随重定向。session ID 接受 1–160 个 ASCII 字母/数字及 `_`、`-`、`.`、`:`；
当前 HTTP 单段路由不接受 `/`，`.` 和 `..` 也会拒绝。

输出父目录必须存在，文件必须不存在；采用独占创建，绝不覆盖旧报告。所有文本 UTF-8 无 BOM。
退出码 0 表示采样正常结束且至少有两个样本，**不表示投射性能通过**。网络/鉴权/数据错误、
Ctrl+C、样本不足或写文件失败返回非零。能保留的部分报告会标记 `failed`、`cancelled` 或
`insufficient-data`；错误仅记录固定类别，不输出私有响应、URL 或原始异常。

## 边界

| 项目 | 上限 / 行为 |
| --- | --- |
| 采样时长 | 默认 60 秒，最多 600 秒；总 deadline 可取消正在读取的响应 |
| 间隔 | 默认 500ms，100–10000ms；一次只有一个在途 GET，超时不重试 |
| 单次请求 | 默认 3000ms，100–5000ms，覆盖响应体读取 |
| 响应 | 最多 1 MiB，既检查 Content-Length，也检查实际流字节；固定缓冲 |
| 保留 | 最多 6000 个字段白名单样本；无图像、OCR/观察值、窗口标题或原始快照 |
| 身份 | 会话/源绑定与 viewer 用 SHA-256 截断指纹标识；epoch/frame ID 保留 |
| 数值 | 仅接受非负安全整数；超过 JavaScript 精确整数范围时拒绝，不悄悄四舍五入 |
| 资源 | 超时/取消关闭响应 reader，清理 timer/信号监听与文件句柄，不停止产品进程 |

首次样本只建立基线，不把会话历史累计量算入本次结果。源身份变化直接失败；epoch 变化
或任意计数回退会重新分段，不能跨重启拼出负数/夸大收益。报告保留分段次数和所有白名单
快照，差分只覆盖同 epoch、无回退的相邻快照区间。

## 如何读结果

- `counterDelta`：采样窗口内已接受的源帧/字节、socket 写入/字节、失败、跳帧及 ring eviction。
- `rates`：仅以可比较区间时长计算源发布率和 socket 写入率/字节率；`receiverFps` 固定为 null。
- `observedForwards`：只统计累计写入数改变时看到的 `lastForward`，避免静态帧重复采样使分位数偏斜。
  同一区间发生多次写入时只看到最后一个；`unobservedWriteAttempts` 明确记录未观察到的写入数。
- `queueAgeMs/adaptationMs/socketWriteMs` 的 min/p50/p95/max **只代表观察到的发送样本**，
  不是全部帧的总体分位数。它们使用 daemon 的单调时钟，不是跨设备端到端耗时。
- 当前 daemon 的 `queueAgeMs` 在兼容转换后取样，**包含 adaptationMs**；两者不能再相加。
  本工具只如实说明既有语义，不顺手改变运行时计时合同。
- `forwardedFrames` 汇总所有 viewer，不能当唯一源帧数；ring eviction / viewer skip 不是网络丢包。
- 成功 socket write 不等于接收、解码或显示；失败写入的部分字节没有计入成功字节。

Hook A2.2 已提供[收端单槽与实际进程包绑定合同](https://github.com/aiaimimi0920/Hook/blob/db029ef244ca2b945dddfb90c4a66f59a359452e/docs/LIVE_RELAY_DIAGNOSTICS.md)。
使用相同 session/source 和 epoch/frame 对齐；两端都可能漏掉中间帧，不能拼成全帧统计。
下一步 A3 必须另行绑定收端 decoded-submitted、正常产品入口、实际进程包 SHA、设备/网络/负载，
并采集 CPU/GPU、卡顿、恢复、撤销等证据。没有这些数据，不宣传“更流畅”、帧龄下降或 CPU 降幅。

## 验证与复现

```powershell
node --test scripts/tests/live-relay-measurement.test.mjs scripts/tests/measure-live-relay.test.mjs
node --check scripts/live-relay-measurement.mjs
node --check scripts/measure-live-relay.mjs
node scripts/effective-code-lines.mjs --mode strict --json <linshi-output.json>
```

Windows 本地运行测试前将 `TEMP`/`TMP` 指到 `GameEditor/linshi` 的任务目录；测试会清理
自己创建的临时目录。测试包含真实 loopback HTTP/CLI、鉴权、重定向拒绝、超大响应、慢响应、
取消、脱敏、独占写文件、重复样本、重置/epoch 分段与安全整数边界；合成服务不代表真实两机画面。
