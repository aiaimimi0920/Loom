# NAccount 与 AssetLibrary 接入

## 当前范围

2026-10-16，NAccount 原生登录、状态、刷新和退出，以及 AssetLibrary 云端目录读取与资源元数据创建 API 已实现。用户在 `manual-20261016-naccount-callback-fix` 包完成真实桌面登录；随后实际核验 signed_in、刷新后主体保持，以及刷新前后私有资源目录读取成功。新增资源库及受控 ZIP 下载实现已有本地测试，生产端尚未开放分发，不能声称线上成功下载。退出与重启恢复尚未做本轮真实验收；文件上传 UI、安装或 Loom 自动更新尚未实现。本文件取代旧 `ASSET_LIBRARY_INTEGRATION.md` 中针对历史服务的接线约定。

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
| `/v1/asset-library/download` | `{ "publicationId": "UUID v4" }` | 公开版本详情 → 私有票据 → 授权包体 |

创建的 kind 为 `art`、`capability` 或 `application`。调用者为一次操作生成并保留同一 UUID，重试复用该 `operationId`，上游收到相同 `Idempotency-Key`。创建资源元数据不等于上传文件。

桌面设置页提供 NAccount 操作、本人资源分页读取及“下载资源库”。下载资源库按 `nextAfter` 翻页（与私有目录的 `nextCursor` 不同），空页有游标时仍可继续。尚无创建或上传 UI。服务端未配置原生客户端或拒绝分发时明确返回错误，不将失败显示为空目录。

下载从正式固定 origin 查询 publication，再申请绑定票据，票据仅留在 daemon 内存并放入请求头。包体以 64 KiB 缓冲写入本机 control-plane 下的 `asset-library-downloads`，上限 8 MiB；实际长度和 SHA-256 与服务端版本一致才取得最终 ZIP 文件名，失败清理本次创建的临时文件，不覆盖既有文件。不解压、不安装、不执行；中断须手动重新下载，未实现续传。桌面普通云操作预算为 90 秒，下载操作为 120 秒（元数据/票据各 15 秒，内容流 60 秒）；其余本机操作仍为原预算。

## 部署与后续业务边界

NAccount 应用已登记；AssetLibrary v0.4.6 已通过 SpringBok 部署并读回确认 Loom 原生客户端绑定。2026-10-16 的真实 Loom 登录主体已成功读取线上私有资源目录，返回 1 项资源，字段符合桌面视图要求；刷新凭据后再次读取成功。部署细节以 AssetLibrary 仓库的 `docs/LOOM_CLIENT_DEPLOYMENT.md` 为准，保留网页 client、会话 secret、D1/R2、审核与扫描配置。

当前 AssetLibrary 的 Art 格式是受限静态 PNG ZIP；软件包为受限小包，单载荷约 1 MiB、整包 8 MiB。它们不能直接视为 Loom 原生可执行 Art、真实能力包或桌面更新包合同。后续需分别确定包格式、大小预算、版本映射和上传/下载接口，再连接既有安装器；不把 `application` 元数据当作更新功能完成。

生产扫描准入未改变；发布和下载仍可能被 `SCANNER_CLOUD_NOT_VALIDATED` 拒绝。上传隔离存储、审核、发布、下载、安装和更新是不同验收阶段。

本轮真实检查中，资源库与公开目录返回 `asset_library_distribution_unavailable`，未绕过准入。公开目录首次请求曾返回 `asset_library_unavailable`，一次复查恢复为明确的准入拒绝；不据此声称网络稳定性已验收。

## 浏览器回调修复

真实 OAuth 授权回跳携带 `Sec-Fetch-Site: cross-site`，曾被本机通用检查提前拒绝。现在仅允许固定 `/v1/naccount/callback` 的 GET 顶层导航（navigate/document）进入后续 state/PKCE 校验。普通 API、iframe、跨站脚本请求、POST 回调、异常 Origin 和重复请求头仍拒绝。HTTP 回归覆盖该路径，用户已在修复包完成真实登录；无需修改 NAccount 的正式配置。

## 本地验收

聚焦测试覆盖 PKCE 与回调、错误端口不创建事务、原有 Platform 身份保持、未登录私有请求拒绝、当前服务路由及分页参数。AssetLibrary 独立测试使用本地 Worker/D1/R2 和合成 NAccount 签名，覆盖三种资源元数据创建、读取和网页会话兼容；不是正式账号或真实包存储验收。
