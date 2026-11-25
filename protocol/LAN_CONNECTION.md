# Otter 局域网连接

Loom 桌面的“Otter 连接”设置提供独立的局域网 HTTPS 入口。默认关闭，默认端口
`8766`；本机管理 HTTP 入口仍为 loopback `8765`，不会随局域网开关暴露。

## 使用

1. 在运行 Loom 的电脑上开启“允许局域网连接”，填写本机局域网 IPv4 地址及端口，点击应用。
2. 开关同时控制局域网公开发现，不再设置独立的“公开发现”或 HTTPS 发现地址。
3. 打开 Otter 的 Loom 连接窗口，自动发现目标，选中目标后点击连接。
4. Loom 启用连接码时，输入正确连接码才能提交请求；未启用时无需输入。
5. 请求通过密码门槛后，再按 Loom 的审批策略处理，不等于立即连接成功。

三种审批策略为：局域网内自动批准、连接过的设备自动批准、手动批准。
不符合自动批准条件时等待手动批准，不直接拒绝。局域网资格只认本进程的 TLS
入口标记，不相信客户端的 IP/来源声明。连接过的设备必须曾成功 activate 且持有
有效的签名设备会话，仅有已知公钥、曾申请或仅获批准不够。所有策略独立验证连接码。
关闭连接码会删除已有密码哈希；重新开启需要设置密码，密码最少 8 字节。

两台电脑必须网络互通。Windows 防火墙需要允许所选 TCP 端口的专用网络入站；
Otter 发现监听 UDP `49779`，Loom 每 2 秒从所选网卡发送一跳广播。防火墙需允许
对应专用网络的发现流量。建议将远端地址限制为 LocalSubnet，不开放公共网络，不关闭防火墙。
本功能不会自动修改系统防火墙。“已监听”不代表另一台电脑已经能穿过防火墙访问。
多网卡时应选择另一台电脑可达的地址。IP 改变后重新应用并在 Otter 重新选择目标。

## 信任与生命周期

连接码仅指用户设置的请求密码，不再是旧 `loom-pair-v1:` 证书包。
广播携带公开身份和公开证书，不携带连接码、密码哈希、管理令牌或私钥。Otter 限制
广播地址与发送者 IP 一致，只把它作为发现提示。仅用户选择目标时执行首次使用信任
（TOFU）：正常校验证书名字、有效期和签名，并核对 Loom ID 后记录该 origin 的证书。
扫描本身不授予信任；已记录身份或证书不允许被后续广播替换；不安装系统根证书。
首次选择依赖可信局域网，并非经独立渠道认证，不能防御首次配对时的主动中间人。
已存在的证书信任文件兼容保留，连接码不承担证书认证功能。

监听设置和私钥使用 daemon 的受限原子 JSON 存储，文件名为 `loom-lan.json`。
重启保留开关和证书；改变绑定 IP 时重新生成证书。启动绑定失败只影响局域网入口，
本机管理继续可用；界面显示错误。禁用关闭监听器，清除 LAN 连接请求并撤销相关
设备会话。Otter 在下一次续租失败时显示断开（正常续租间隔 5 秒，HTTP 超时 8 秒）。
已成功连接记录持久保存；旧数据缺该记录时按未连接过处理，不根据 lastSeen 猜测。
旧“密码正确自动连接”策略在读取时安全降为手动批准，保留原密码，不自动扩大访问。

## 暴露面与资源边界

入口仅允许 RFC1918 IPv4 绑定和私网来源，使用 TLS 1.3。HTTP 适配器只代理
`lan_http`、`lan_http_live`、`lan_http_surface` 的精确白名单：公开身份、连接申请/
状态/取消/激活、设备请求/签名会话、受控 local Art 准备/资源/异步服务/观看加入，
以及 shared/offline/projection v2 和 signed peer 投送合同。白名单不是通用路径前缀，
不会自动开放同目录新增接口。

本地 Art 的 Live 控制另用精确 `POST /v1/art/local/live-viewer/control`，最多32KiB，
只允许 Device/nonce 的 acquire/release/input，不是通用前缀或JS权限。Preview不自动
join/取得控制；仍需有效签名/绑定、当前审批/工具/设备、已有成员、Source输入许可及
原controller/epoch/双序号。Art过期或撤审批后，仅允许有效Device精确身份释放自己
已有的controller；不能重新获取或转发输入。原Surface控制合同不变。

Live 开放 status、sessions 目录/创建、单会话读取/events，以及 viewers/resume/
control/input/observations/triggers/close 的既有精确路由。WSS 只允许
`GET /v1/live/media` 的规范 sessionId/role/可选游标参数，经 Device 会话、nonce
及原媒体握手/成员授权核验后，将 TLS 流移交原媒体 worker；其他 Upgrade 仍拒绝。
媒体连接不占用四个短 HTTP worker，继续服从原媒体连接配额、帧预算及停止回收。

Surface 只开放 `POST /v1/surfaces/attach`、
`POST /v1/surfaces/instances/{raw-id}/lifecycle` 和 `GET /v1/surfaces/stream`。
必须携带 Device 会话及 nonce，JSON 请求最多 16 KiB；stream 只接受规范数字
after/timeoutMs，无重复或未知键，timeoutMs 最多 5000。路径别名/转义拒绝；
设备撤权、nonce 重放、附件归属和生命周期版本仍由原 runtime 核验。
Surface CRUD 管理、任意 events、mount、resources、本机 bridge 与共享内存不开放。
管理 API、浏览器 Origin/Cookie、Bearer 管理令牌、GET 请求体及重定向均不转发。

普通 HTTP 每次连接只处理一个请求，最多 4 个并发 worker，无无限等待队列。
头部最多 16 KiB/48 项，响应最多 64 MiB；默认请求体最多 1 MiB，Art、Surface
和投送按各自既有合同限额，不能将默认值当成所有路由的统一上限。
TLS 握手/请求读取共用 5 秒绝对期限，loopback 后端请求限时 8 秒，响应写入限时
3 秒；这些短 HTTP 期限不冒充已升级媒体流的整个寿命。设备权限、审批与 nonce
校验仍由原服务执行，局域网适配器不会授予管理员身份。

定向测试：`cargo test --locked -p loom-daemon lan_ --lib`，包含真实 HTTPS 证书拒绝、
审批、签名会话、激活、禁用及证书复用。Live/Surface 联合另核真实 HTTPS/WSS、
原生 Application、精确像素及 Source 输入释放；合成捕获和自有窗口消息不等于真实
桌面捕获或普通可见双产品运行，回环测试也不替代物理跨机和最终用户验收。
