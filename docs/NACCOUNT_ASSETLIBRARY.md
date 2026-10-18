# NAccount 与 AssetLibrary 接入

## 2026-10-18 Art 商店原生验收缺口修复

后续用户提供修复包原生截图：Art 商店正常显示颜色迁移 Art 的两个发布记录，旧 HTTP 400 不再出现。此项标记为人工确认的原生目录展示通过；按钮仍为安装，不能由截图推断安装或实际运行通过。用户随后授权提交、推送本轮相关源码；正式发行、运行中自更新与完整对应源码交付仍不因此自动完成。

用户手动运行完整开发包后，Art 商店仍请求旧 `/v1/arts/store/catalog`，返回 `art_store_not_configured`。此前设置页 AssetLibrary 接入成功不代表商店入口已接通。现将商店目录改为 `/v1/asset-library/catalog`，只展示 Art 发布项并保留分页；安装绑定所选 publication/kind/SHA-256，调用现有 `/v1/asset-library/install`，不修改旧商店地址、不绕过权限或宿主校验。目录不提供框架或官方认证事实，不推测或伪造相关字段。非原生 Art 格式不显示可安装状态。

同时修复下载资源库列表仍固定 8 MiB 的客户端校验，仅大型 application 策略接受 96 MB，其他类型预算保持。新增三项回归，前端全套 251 项通过、0 失败、0 跳过；类型、前端构建、严格行数和 diff 检查通过。正式公开目录已只读核实存在颜色迁移 Art 0.1.4/0.1.5。

新开发包 `C:/Users/Public/nas_home/AI/GameEditor/linshi/Loom-art-store-dev-20261018-v1/Loom.exe` 使用 Tauri `build --debug --no-bundle` 重新嵌入前端；1,550 文件源码快照构建后与工作区逐项一致。daemon、libaria2 与模块沿用上一完整包，未覆盖用户正在运行的目录。新 EXE SHA-256 为 `5baaedca50b8c1d18b988ad170626bc21af3f6e78f3fce0e1154542a7fb01d43`。原生页面点击复验仍待用户切换新版，不能将上述测试或修复包视为实际 UI 验收通过；运行中自更新及完整对应源码发行仍未完成。

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

2026-10-16 补齐实际 AssetLibrary Worker/D1/R2 联合验收：Art、Capability、Application
各验证授权后保存、撤销后拒绝及下架后拒绝，共九项通过。`worker_integration.rs` 是
显式 ignored 测试，由 AssetLibrary 的 `scripts/verify-loom-download.mjs` 运行；
普通单仓测试不依赖兄弟仓库。身份/AV/部署准入使用独占本地 trial 假设，正式 origin
和生产门禁未修改。证据在 `../../linshi/assetlibrary-local-trial-LURd8b/loom-integration.json`；
不能将此结果称为真实线上下载或原生桌面交互通过。

聚焦测试覆盖 PKCE 与回调、错误端口不创建事务、原有 Platform 身份保持、未登录私有请求拒绝、当前服务路由及分页参数。AssetLibrary 独立测试使用本地 Worker/D1/R2 和合成 NAccount 签名，覆盖三种资源元数据创建、读取和网页会话兼容；不是正式账号或真实包存储验收。

## 2026-10-16 真实桌面刷新故障与新包

实际运行的 `manual-20261016-naccount-callback-fix` 在重新登录后可读取正式私有资源，
但点击“检查登录状态”提示“请求未完成”。该按钮执行 refresh，错误后的“重试连接”
仅执行 status，不可将错误消失当作刷新成功。直接调用同一运行 daemon 的 refresh
约 5.36 秒后返回 HTTP 200 / signed_in；旧包基线桌面传输默认仅等待三秒。
当前源码已包含云操作 90 秒、下载 120 秒的有界预算；不是账号服务需要重新部署。

新增真实回环延迟响应回归：同一四秒响应用旧三秒预算失败，正式云操作调用成功，
服务线程全部收尾。两项超时测试、两项 daemon NAccount 测试、四项前端 NAccount
测试通过；前端类型检查/构建、Tauri 构建、rustfmt、严格行数及 diff 检查通过。
扩展桌面 runtime 测试为 52 通过、1 失败：诊断元数据预期 Otter 仓库 URL，实际得到
Neuro URL。该项未修复，不宣称全部回归通过。

新包 `manual-20261016-live-refresh-fix-r2` 基于本地 HEAD `8d32bd5` 和保留的工作区
改动构建，不是 clean 正式发布。构建前后 1,702 个源码文件逐项一致，保留源码 ZIP、
逐文件摘要及真实构建日志；官方包验证检查 47 个文件通过，运行 smoke 未执行。
首轮 NoZip 产物未通过需要 CLI/SDK ZIP 的完整包校验，保留为中间产物，不交付为完整包。

证据及最终包位于 `../../linshi/loom-live-e2e-20261016-082911/`，新包位于其
`package/manual-20261016-live-refresh-fix-r2/`。本文追加发生于源码冻结后，不属于
构建时源码快照。当前尚未替换用户运行程序；新包实际桌面刷新/重启恢复仍待验收，
生产扫描准入仍阻断线上下载。不将重新登录后的目录读取说成下载或安装成功。

### 同日新包真实切换验收

用户明确允许短暂重启后，核对旧路径并停止旧桌面/daemon，启动上述 r2 包。
新桌面与 daemon 实际进程路径均指向 r2；原账号无需重新登录，设置页自动刷新完成，
手动点击“检查登录状态”后正常结束、按钮恢复、没有旧请求失败提示。
随后实际点击“读取云端资源”，显示正式资源“线上验收 Art 20261015-1720 已改名”。
本次通过的是保留凭据的进程重启恢复及刷新后私有目录读取，不是整机重启或长期稳定性测试。

新下载入口实际点击后显示“AssetLibrary 暂未开放分发。”，不显示为空列表；
正式 readyz 同时读回 distribution blocked / SCANNER_CLOUD_NOT_VALIDATED。
因此客户端刷新故障已完成真实桌面复验，生产下载仍未通过，退出撤销本次未测试。
原包、登录和配置保留，新包继续运行；没有云部署或生产准入变更。
独立回执：`../../linshi/loom-live-e2e-20261016-082911/native-switch-verification.json`。
