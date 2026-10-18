# AssetLibrary 原生包下载安装

2026-10-17 四类分发目标续进（未发布新桌面包）：云安装入口增加 MCP，接受 `kind=mcp` 和 `loom-mcp-zip-format-v1`，不接受软件封装或不存在的 MCP 扫描策略。
下载仍使用真实票据、流式保存和摘要复验，随后复用 `install_mcp_server_package`；前端显示原生 MCP 的下载安装入口。
同一 qualified id 新版安装保留凭据绑定和启用状态，版本目录独立保存。后续调用读取新配置；已执行中的旧调用不被强行中止，不宣称所有旧缓存进程立即退出。

本次修复既有 MCP 更新失败一致性问题：包生命周期锁内保存旧 active 原字节，配置提交失败恢复旧 active，内存和 servers.json 保持旧状态；配置验证提前到激活前。
ID 冲突不再调用删除整包家族的卸载函数，保留全部历史版本。该合同覆盖可捕获的提交失败；不是跨文件断电原子事务保证。

本地真实跨仓库验收已通过：AssetLibrary 上传两版 → 格式检查（未扫描）→ 独立 trial 主体审核 → 授权另一 trial 主体 → Loom 实际 HTTP 下载 → 宿主安装 → stdio initialize/tools/list/tools/call，分别返回 1.0.0 和 1.1.0。
证据：`C:/Users/Public/nas_home/AI/GameEditor/linshi/assetlibrary-local-trial-ZjqZfM/mcp-update-evidence.json`，同目录 `mcp-update-cargo.log`。
账号为合成身份，未运行 AV，不是线上真实双账号验收；用户真实实例、信任库和凭据均未修改。
应用程序安装更新及四类生产联合验收仍未完成。下文两类包限制和旧待办为历史批次。

本轮复测：MCP 包生命周期 23 项通过；daemon cloud_install 6 项通过、1 项旧真实 Art 材料测试忽略；资源库客户端 7 项通过、1 项旧专用 trial 测试忽略。
新增 MCP HTTP 两版联合验证单独执行并通过，不能将上述 ignored 计入通过数。
前端服务 3 项、TypeScript、前端构建、Rust 格式及 strict 行数检查通过；没有新 EXE 或生产部署。
云资源列表现在接受 mcp，下载列表显示版本标签，MCP 安装后显示正确类型；后台透传经过 UUID 校验的 resourceId 版本筛选。

最新交付：AssetLibrary `0.4.8 / a11999ade90b775cb72c33105320e9e5ec9918c5` 已通过 SpringBok 原实例升级和独立 verify。新桌面包为 `linshi/loom-unscanned-20261016-r1/package/manual-20261016-unscanned-native-art-r1/Loom.exe`，来自包含当前未提交修改的独立源码快照，1,713 个源文件前后摘要一致；50 项包完整性通过。新包 daemon 在独立临时状态安装 process 和颜色迁移 Art，实际产生 314 字节 8×8 PNG，证据同目录上两级的 `color-art-runtime/execution.json`。这不是实际 A/B 账号云端下载验收，用户信任库及运行客户端未替换，完整 native smoke 未运行。以下未交付描述为此前批次历史。

2026-10-16 后续用户明确允许未扫描包分发，扫描器开发和研究暂停。安装入口已同时接受 `loom-{kind}-zip-format-v1`，不放宽发布者签名、框架、摘要或权限检查。对应后端聚焦 4 项通过、1 项真实 Art 材料依赖测试忽略，前端 3 项及 TypeScript 通过；当前尚未重新交付桌面包，不能据此宣称 A 上传、B 下载并实际使用已完成。下文扫描器生产验收作为阶段前置的描述已由这一决定取代。

2026-10-16：新增管理端 `POST /v1/asset-library/install` 和桌面“下载并安装”入口。
本地联合业务链已验证；尚未发布新桌面包，也未完成生产 scanner 准入和线上闭环。

## 合同

请求只有 `publicationId`、`kind`（art/capability）、`sha256`。不接受本机路径、
`bundledCatalog`、自动权限批准或自动启用参数。入口仍只允许 daemon 管理员凭据。

每次安装重新通过 NAccount 访问正式 AssetLibrary、查询当前发布并申请下载票据。
下载保持流式写入、8 MiB 上限及 SHA-256 核验；保存后安装前再次有界读取并复验摘要，拒绝链接和字节变化。
服务端返回的 kind、摘要、发布 ID 和 `loom-{kind}-zip-clamav-v1` 策略必须匹配选择。
旧 PNG 素材包、旧软件封装和 application 保留下载入口，不展示原生安装按钮。

Capability 复用既有生命周期路由，包括变更锁和清单广播；安装不等于权限批准或启用。
Art 复用原安装器和清单广播，不传 `bundledCatalog` 且传空内置包 allowlist。
云端扫描结果不提升发布者信任，不关闭签名、框架兼容和权限检查。
失败时保留已完成下载的文件，前端区分资源变化、文件变化、框架未就绪和宿主拒绝，不误报为需要重新登录。

## 已完成的验证

- `cloud_install` 聚焦测试：4 项通过，覆盖输入边界、任意路径/自动授权参数拒绝、安装前篡改、管理端认证和内置 Art 信任旁路拒绝。
- 能力插件使用编译出的真实进程；未批准时启用被拒绝，明确批准后 `/v1/invoke` 成功。
- 真实签名 Art 和实际 process 框架安装后执行图像混合，生成 120 字节 PNG。
- 前端服务和 NAccount 邻近测试 7 项通过，TypeScript 和前端构建通过。
- 原生请求预算测试通过：安装/下载 120 秒，未扩大普通本机路由的预算。
- Rust 格式、Loom strict 有效行数、`git diff --check` 通过。

首轮管理员负例使用了无认证的测试配置，导致断言失败；已改为显式测试 token 后重验通过。
首轮真实 Art 执行发现测试进程缺少 `LOOM_CONTROL_PLANE_ROOT`，已在独占测试中绑定并用 RAII 恢复环境后通过。
以上修正仅针对测试环境，不修改用户当前运行实例的配置。

## 跨仓库联合验收

AssetLibrary 的 `scripts/verify-loom-download.mjs` 保留原三类下载验收；可追加隔离签名夹具目录，
对 Capability / Art Node 运行原生联合验收。测试用回环地址入口仅存在于 Rust `cfg(test)` 中，生产地址不可配置为该入口。

最新证据：`C:/Users/Public/nas_home/AI/GameEditor/linshi/assetlibrary-local-trial-1bhBHn/loom-integration.json`。
同目录 `art-granted/cargo.log`、`capability-granted/cargo.log` 包含实际运行结果；
两类包分别通过 granted、revoked、unlisted，共 6 个联合场景。

真实部分：Worker、D1、R2、HTTP 字节传输、下载校验、原生 ZIP、Ed25519、宿主安装器、插件/框架进程和运行输出。
模拟部分：账号签发、AV 响应、部署准入。信任库为隔离测试库，不改用户真实信任。
撤销/下架只证明后续下载及新安装被拒绝，不宣称远程卸载或停止此前已安装的插件。

## 尚未完成

正式 scanner 安全/云验收、真实独立审核和发布、线上增量迁移/部署、新桌面包交付与真实 UI 点击验收。
前端构建和测试不能代替上述事项；目前不可宣称正式站点全流程可用。
