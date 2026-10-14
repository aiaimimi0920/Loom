# Otter 全局 Loom 连接

## 实现状态

已实现两端连接控制：Loom 身份/策略、连接申请与管理员决策、公开发现，以及
Otter 的全局原生连接 owner、贴图内弹窗、目标历史、设备签名认证和短时连接租约。
`approved` 只表示申请获批；只有 `/activate` 校验真实 Device session 和申请 receipt
后返回 `connected`，Otter 才显示唯一 ID。尚未完成用户原生 UI / 物理局域网验收。

本协议建立的是全局连接控制关系，不把既有本机 Art / 扩展 bridge 转为远程通道，
也不改变旧的本机业务路由。连接成功不代表所有本机专用 Art 已支持远程执行。
跨设备 Art 执行、共享内存替代和扩展资源隔离不属于此连接控制协议的授权范围。

## 已确认的产品边界

- 一个 Otter 进程只选择一个全局 Loom；所有贴图共享连接状态。
- 未连接显示 `-`；连接后显示 Loom ID，不显示 Otter 设备 ID 或短期会话 ID。
- 每次手动批准、密码正确自动批准、已认证设备自动批准，由 Loom 管理员选择。
- 每次手动批准不能被“曾经配对过”绕过；连接授权独立于既有设备配对。
- 历史目标不保存明文密码；原生设备私钥和认证会话不交给 WebView。

## 身份和策略

生产环境复用 `offline-projection-peers.json` 已有签名身份的 `loom-<key sha256>`，
而不是额外创建另一个 Loom 身份，也不是每次启动变化的本机 bridge instance ID。
`settings/loom-connection.json` 记录相同 ID 与策略；损坏或身份不匹配时失败关闭，
不能静默重置身份或清除已有密码。

密码保存为带独立随机盐的 PBKDF2-HMAC-SHA256 摘要（600000 次迭代），沿用现有
私有权限与原子替换。API 和局域网公告只输出 `passwordRequired`，不输出摘要、
盐、管理员 token 或私有 manifest。密码更新缺省表示保留，空字符串表示明确清除。
密码自动审批模式不允许空密码。耗时的密码校验在设备注册表锁外进行。

## HTTP 控制协议

所有路径保留现有 Host/Origin、请求大小与设备 session 验证。远程部署仍要求 HTTPS，
现有 TLS 终结器部署要求不变，不开放明文远程 daemon。

| 路径 | 权限 | 行为 |
| --- | --- | --- |
| `GET /v1/otter/identity` | 公开 | `loom.connection.v1` 身份与非秘密策略信息 |
| `POST /v1/otter/connections/request` | 公开，可附设备认证 | 提交名称、公钥和可选密码 |
| `POST /v1/otter/connections/status` | 私有 receipt | 查询这一条申请 |
| `POST /v1/otter/connections/cancel` | 私有 receipt | 取消这一条申请 |
| `POST /v1/otter/connections/activate` | 匹配的 Device session + receipt | 激活或续期 30 秒连接租约 |
| `GET /v1/otter/connections` | 管理员 | 策略及未过期的待批准申请，无 receipt |
| `PUT /v1/otter/connections/policy` | 管理员 | 更新策略并撤销未消费的连接申请 |
| `POST /v1/otter/connections/decision` | 管理员 | 批准/拒绝指定申请 |

申请响应提供随机 `requestId` 和仅返回一次的高熵 `receipt`。服务端仅持有 receipt
摘要；知道设备公钥或 request ID 不代表持有申请。申请最长 5 分钟、最多 64 条，
匿名申请与真实认证且公钥匹配的设备分别限流为每分钟 20 次。总容量达到 48 条时
仅接纳已认证设备，保留最后 16 个槽位；最终插入仍在锁内复核容量，避免并发超额。
已取消和已拒绝申请保留到过期，以便客户端读取最终状态。过期/未知/错误 receipt
统一返回不可用。

`authenticated` 自动审批必须验证真实 Device session，且提交公钥与该 session 的
设备完全匹配。`password` 自动审批只在配置了密码且校验正确后生效。禁用设备
不会因为提交正确密码而重新启用。管理员批准申请时才批准尚未批准的配对设备。
更改策略与在途校验之间使用 revision 检查，旧策略校验结果不能创建已批准申请。
认证证明绑定原始设备 ID、session 摘要和 epoch；锁外校验结束及最终准入时均复核
会话过期、撤销标记与设备状态。删除后以相同公钥重新配对不能继承旧申请的自动批准。

## 局域网公告

只有显式开启公开发现且配置了 HTTPS origin 时，daemon 才每 2 秒向
`255.255.255.255:49779` 发布不超过 1024 字节的 `loom.connection.v1` JSON。
关闭发现后停止发包。公告线程由 daemon 生命周期持有并在退出时唤醒、join。
IPv4 广播只是目标提示，不是身份证明。客户端必须经 HTTPS 重新核对目标身份，
不得信任公告内提供的身份、密码要求或审批结果。多网卡实际可发现性待原生验收。

## Otter 生命周期

所有贴图复用一个原生 owner。关闭弹窗或设置栏不会断连；“断连”使 generation 失效、
取消旧任务并取消当前 receipt。新连接不能被旧请求的迟到结果覆盖。进程退出后拒绝
新连接；退出或进程崩溃不等待网络，Loom 上的活动租约最多保留 30 秒。

每 5 秒续租并重新验证 Device session、设备 epoch、receipt、策略和租约有效期。
会话即将过期时使用相同设备私钥重新签名，不重新申请管理员批准或静默重新配对。
收到拒绝或网络失败后显示未连接，不自动绕过手动审批重连。历史只保存最近 20 个
成功连接的 origin、Loom ID、设备 ID 与名称，不保存密码或认证 token。

请求已提交但在收到 receipt 前被取消时，客户端无法取消未知 receipt；该未激活申请
由服务端最多 5 分钟过期。它不能令 Otter 显示已连接，也不能自行激活连接租约。

## 验证与手测边界

服务端 HTTP 聚焦测试覆盖审批、密码、认证、配额、撤销和连接租约。Otter 的 opt-in
`loom_connection_paired_signed_approval_and_lease` 使用真实 Otter 签名/HTTP 实现，启动
指定路径的隔离 Loom daemon 验证手动审批、签名认证、续租、取消和已认证自动审批。
测试必须使用独立数据目录，不替用户批准真实设备，不重启已有服务。

远程地址需要现有 HTTPS 部署和可信证书；局域网广播不会取消 TLS 校验。回环本机可
使用 HTTP。公开发现需要在 Loom 管理页面开启并设置 HTTPS origin。
