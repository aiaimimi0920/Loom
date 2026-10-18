# 内置 libaria2 下载引擎

2026-10-17：已从外部 aria2c 进程切换到 Windows x64 进程内 libaria2。最新已接通 UI 控制、清理与账号取消，迁移依赖并通过实际 OSV 检查，生成隔离 debug 验收包；详见 docs/LIBARIA2_DELIVERY_VALIDATION.md。未批准正式完整发行。下列初次接入记录中的未完成状态以该续进记录为准。

## 当前实现

native/libaria2/bridge.cpp 提供窄 C ABI；Rust 的 asset_library/aria2_native.rs 在唯一下载线程中加载 download-engine/loom_aria2.dll，管理库与下载会话。没有 aria2c 子进程，没有 JSON-RPC 服务。队列有界，任务顺序处理，每个任务最多四连接。

DLL 由 aria2 1.37.0 官方源码构建，裁剪 BT、Metalink、WebSocket、NLS 及非必要第三方依赖，TLS 使用 Windows 系统实现。当前文件 2,089,984 字节；之前 aria2c.exe 为 5,649,408 字节，仅引擎文件约缩小 63%，不是整个 Loom 安装包缩小比例，也不是吞吐量提升证明。

加载前核对固定摘要、绝对路径、文件类型及大小；持有拒绝写入/删除的文件句柄覆盖 DLL 使用期。依赖搜索限制为系统目录。库初始化一次，逐任务停止并排空事件循环后释放会话；daemon 正常结束时关闭库，随后卸载 DLL。强杀或系统崩溃不在正常清理保证内。

远端地址、Bearer 和下载票据仍由 Loom 的短生命周期 localhost relay 保管。只转发必要 Range/If-Range，拒绝远端重定向；引擎使用显式空代理、不读取 netrc、不执行下载后脚本。保留宿主身份、权限及信任检查，不涉及上传病毒扫描。

最终文件仍经长度和 SHA-256 复验，再流式写入既有安装路径，并非零拷贝。原整包 8 MiB 预算和同步安装接口保持。失败或超时保留独占任务目录，避免工作线程尚在收尾时删除文件；尚未提供残留目录回收策略。

原生回调的进度、暂停、继续和取消已通过真实传输测试，但暂停/继续尚未接入生产 UI/API；没有跨进程持久续传。不能把 libaria2 支持的功能都表述为 Loom 已交付。

## 构建与打包

security/libaria2-runtime.json 固定源码与 DLL 摘要，当前 DLL SHA-256 为 `6f8b8cc51aca77af7c2275bca95933f815aac29faf5919e2dd06c49a7cefb732`。

scripts/Build-LoomLibAria2.ps1 使用现有 WSL/MinGW 工具链，在新目录验证源码归档并构建，保存配置、日志、导入表和回执。固定时间戳不等于已经证明跨工具链可重现。脚本先快照桥接源码与构建脚本；已在独立 repro-v3 目录完整重建，DLL 摘要与首次固定构建一致。该结果仅覆盖本机同一工具链，不外推跨机器可复现。

scripts/Install-LoomLibAria2.ps1 验证构建回执、桥接源码和 DLL 摘要，部署 DLL、COPYING、回执及 metadata。相同目标可重复验证，不覆盖不同内容。正式包目录为 runtime/download-engine/，接入 supportFiles 摘要、ZIP 清单及顶层 SBOM。旧 Install-LoomAria2.ps1 和 aria2-runtime.json 仅保留历史实现，不再由活动打包流程引用。

releaseApproved=false：正式打包拒绝，-DevelopmentOnly 仅用于隔离开发。libaria2 的 GPL 许可与 Loom 分发安排及对应源码交付仍需完成审查，没有修改 Loom 项目许可证。系统 DLL 导入清单及顶层 SBOM 不等于完整原生供应链核验。

## 本地证据与边界

证据根目录：C:/Users/Public/nas_home/AI/GameEditor/linshi/loom-libaria2-bundle。

- native-test.log：固定 DLL 的真实 Range 下载、进度、暂停/继续、取消、两个顺序任务、中文目录、DLL 文件锁及正常卸载测试通过。
- runtime/download-engine/：已实际准备四个运行文件，不含 aria2c.exe；重复安装核验和正式发行拒绝门禁通过。
- sbom/：当前 libaria2 集成的 CycloneDX/SPDX 生成通过；发行 dry-run 通过，不等于实际构建安装包。
- daemon 编译检查、资源库单元测试 9 项通过，2 项忽略；原生测试另行显式运行通过。严格有效行数检查通过。
- 应用与 MCP 的真实 Worker/D1/R2 双版本下载、安装及程序/stdio 使用通过；最新证据见 AssetLibrary 的 docs/PLATFORM_DISTRIBUTION_ACCEPTANCE.md。身份是合成 trial 主体，不是生产双账号。
- 三个 Cargo 锁文件已定向更新 quinn-proto 至 0.11.18、rustls 至 0.23.46；实际 OSV 重扫已不再报告这两包告警，但仍有 unsafe-libyaml 无维护与 zip 2.4.2 三项告警。证据：C:/Users/Public/nas_home/AI/GameEditor/linshi/loom-libaria2-bundle/osv-closeout.json。未添加豁免，该检查不覆盖 libaria2 原生依赖。
- StandaloneReleaseContract 和原生归档测试已通过：移除 NativePackageArchive.ps1 中不兼容现有 PowerShell 5.1 ASCII-safe 合同的中文注释，ZIP 字节和签名语义不变，设计说明保留于文档。
- 用户已接受准备 GPL 兼容源码交付方案，见 docs/LIBARIA2_SOURCE_DELIVERY.md。引擎源码材料已保存到 C:/Users/Public/nas_home/AI/GameEditor/linshi/loom-libaria2-source-materials-v2；明确不等于组合应用完整对应源码，发行门禁未解除。

未运行公网吞吐量基准，未替换正在运行的 Loom，未提交、推送或生产部署。四类资源的线上完整更新验收仍未完成。