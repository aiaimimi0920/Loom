# 设备会话鉴权错误合同

设备使用 Ed25519 身份，经 `POST /v1/devices/requests` 登记，再由管理员批准。
登记不等于批准；新登记设备为 `pending` 且 `enabled=true`。
同公钥重复登记不得解除管理员禁用或创建替代身份。

## Challenge 与签发

`POST /v1/device-sessions/challenges` 成功返回 `201`；客户端签名后通过
`POST /v1/device-sessions` 请求短期会话，成功同样返回 `201`。
两阶段都检查当前设备授权；已撤销的 challenge 不可用于签发新凭据。

| 情况 | HTTP / error.code | 客户端处理 |
| --- | --- | --- |
| 设备不存在 | `404 / device_not_found` | 终止此次签发，不自动创建替代身份 |
| 设备已禁用，无论是否曾获批 | `403 / device_disabled` | 终止此次签发，不进入批准等待轮询 |
| 设备未获批且未禁用 | `403 / device_not_authorized` | 保留现有有界批准等待行为 |
| 缺少配对公钥 | `409 / device_key_missing` | 终止此次签发 |

禁用检查优先于批准状态。保留待批准的旧错误码，使既有 Hook 可以继续首次
配对；为禁用使用独立错误码，使既有 Hook 将其视为不可重试签发错误。
升级 Loom 前，旧服务仍可能将禁用返回为 `device_not_authorized`，本次合同不
声称旧服务已具有此区分。

这里约束的是一次设备签发及其批准等待循环，不等于所有上层后台任务都已停止。
例如 Surface 连接恢复仍有独立退避周期；不要把返回此错误后的低频探测，或
其它模块的设备登记，归为观看端自动续期成功。LiveRelay 已接入媒体的设备被
禁用后仍通过原有 `live_media_device_revoked` 终止，其旧窗口不得自动恢复。
