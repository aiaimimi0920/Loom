# NAccount 与 AssetLibrary 接入

## 当前范围

2026-10-15，本地实现 NAccount 原生登录、状态、刷新和退出，以及 AssetLibrary 云端目录读取与资源元数据创建 API。尚未完成真实桌面登录验收，也未实现文件上传、下载、安装或 Loom 自动更新。本文件取代旧 `ASSET_LIBRARY_INTEGRATION.md` 中针对历史服务的接线约定。

现有 `/v1/account/*` 是 Platform 设备身份协议，继续用于原有投屏能力。新 `/v1/naccount/*` 独立保存凭据，不替换或迁移 Platform 身份。

## 登录合同

- issuer：`https://naccount-auth.aiaimimi.com`。
- 已登记公开 SPA 应用：`Loom Desktop`；clientId：`E2C4ea99CC5Fee924b9023D76986397f7593282D25AF979d727D26eaB09A0c10`。此值不是秘密。
- 固定回调：`http://127.0.0.1:8765/v1/naccount/callback`。daemon 不在此 origin 时，开始登录返回 `naccount_callback_port_required`，不创建待登录状态。
- 使用 Authorization Code + S256 PKCE，scope 为 `openid profile offline_access`；待登录状态有效期十分钟。
- 浏览器完成授权后，daemon 兑换授权码并查询 `/oauth2/v1/userinfo`，以 `authId` 标识主体，而非邮箱。
- Windows 凭据使用既有 CredentialStore/DPAPI，独立存于 control plane root 下的 `naccount-login`。非 Windows 尚不支持保存此凭据。
- 渲染器仅收到状态、主体显示信息与待登录 URL，不接收 access/refresh token。
- 退出尝试撤销 refresh token，随后清除本机记录；远端撤销失败单独显示，不能宣称远端已撤销。

本机管理员 API：`POST /v1/naccount/status|start|refresh|logout`，请求体 `{}`。公开回调仅接受与本机待登录事务匹配的 state/code。`LOOM_NACCOUNT_CLIENT_ID` 可覆盖公开 clientId，但对应应用必须独立登记相同回调并由 AssetLibrary 接受，不是绕过配置的开关。

## AssetLibrary API

服务地址固定为 `https://assetlibrary.aiaimimi.com`。daemon 保留分页游标，私有请求使用 NAccount access token，不向 UI 返回令牌。以下本机入口均要求 Loom 管理员凭据：

| POST 入口 | 请求体 | 上游接口 |
| --- | --- | --- |
| `/v1/asset-library/catalog` | `{ "after": "" }` | `GET /v1/catalog` |
| `/v1/asset-library/resources` | `{ "after": "" }` | `GET /v1/me/resources` |
| `/v1/asset-library/library` | `{ "after": "" }` | `GET /v1/me/library` |
| `/v1/asset-library/create` | `{ "operationId": "UUID", "kind": "art", "title": "名称" }` | `POST /v1/resources` |

创建的 kind 为 `art`、`capability` 或 `application`。调用者为一次操作生成并保留同一 UUID，重试复用该 `operationId`，上游收到相同 `Idempotency-Key`。创建资源元数据不等于上传文件。

桌面设置页目前提供 NAccount 操作和本人资源分页读取；尚无创建或上传 UI。服务端未配置原生客户端时明确返回错误，不将失败显示为空目录。

## 部署与后续业务边界

NAccount 应用已登记并读回确认；AssetLibrary 的独立 native Bearer 支持和 SpringBok `identity.nativeClientId` 仍是本地改动，尚未正式发布。因此不能仅凭客户端登记声称线上私有目录已贯通。部署必须使用新固定版本，保留网页 client、会话 secret、D1/R2、审核与扫描配置。

当前 AssetLibrary 的 Art 格式是受限静态 PNG ZIP；软件包为受限小包，单载荷约 1 MiB、整包 8 MiB。它们不能直接视为 Loom 原生可执行 Art、真实能力包或桌面更新包合同。后续需分别确定包格式、大小预算、版本映射和上传/下载接口，再连接既有安装器；不把 `application` 元数据当作更新功能完成。

生产扫描准入未改变；发布和下载仍可能被 `SCANNER_CLOUD_NOT_VALIDATED` 拒绝。上传隔离存储、审核、发布、下载、安装和更新是不同验收阶段。

## 本地验收

聚焦测试覆盖 PKCE 与回调、错误端口不创建事务、原有 Platform 身份保持、未登录私有请求拒绝、当前服务路由及分页参数。AssetLibrary 独立测试使用本地 Worker/D1/R2 和合成 NAccount 签名，覆盖三种资源元数据创建、读取和网页会话兼容；不是正式账号或真实包存储验收。
