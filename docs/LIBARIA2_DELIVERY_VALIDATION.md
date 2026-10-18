# libaria2 交付续进记录

2026-10-17 本地开发批次；未提交、推送或部署，未恢复上传扫描器。

## 下载控制

新增管理员鉴权 POST /v1/asset-library/download-control，字段 taskId/action，action 为 status、pause、resume、cancel。下载和安装接口接受可选 UUID v4 taskId；旧客户端未提供时内部生成。最多八个活动任务，键绑定数据根和当前 token 摘要，不保存票据。

取消是终态；完成文件通过长度和摘要校验后以原子状态封存，迟到取消不能误报成功。工作线程只清理失败或无人接收的任务，成功交接后由调用方清理。取消在最终落盘交界发生时删除本次 ZIP。清理只处理任务目录内两个已知文件，不递归删除未知内容；文件被外部锁定等异常仍可能留下待处理文件。

账号读取与注册和退出/刷新使用一致锁顺序。退出或刷新会取消未完成下载，包括旧客户端发出的请求；已完成下载之后的本地安装不属于下载取消范围。UI 显示实时计数和请求状态，不将“请求暂停”冒充引擎确认。75 秒传输截止、90 秒等待及 8 MiB 包预算保留；跨进程持久续传未实现。

## 依赖迁移

- ZIP 固定为 9.0.4，适配可失败的文件名读取和 SimpleFileOptions。原始路径在 enclosed_name 规范化前校验；解码错误清除不完整输出。首轮回归暴露的盘符规范化和提前长度拒绝差异已修复并重测。
- serde_yaml 依赖别名指向 noyalib-serde-yaml 0.0.58，移除 unsafe-libyaml。适配字符串 mapping 键与 TaggedValue 访问；键仍计入节点预算。非字符串映射键由新解析器拒绝，不声称与旧解析器所有语法完全等价。既有工作流和 frontmatter 测试通过，未批量重写持久化 YAML。
- 回退需要恢复对应 manifest、锁文件和窄 API 适配，不通过删除或重写用户数据回退。混用版本不改变既有文件存储位置。

## 验证

证据根为 C:/Users/Public/nas_home/AI/GameEditor/linshi/loom-libaria2-bundle。

- migration-tests.log：7 个模块合计 425 项通过，覆盖 Art-store、MCP、共享解包、工具安装器、工作流和 agent。
- osv-migration.json：四份锁文件实际联网扫描通过，沿用原有例外，未新增豁免；不代表完整原生供应链或无未知漏洞。
- root workspace/all-targets 编译检查及独立 runtime-host 编译检查通过。
- daemon 资源库测试 10 项通过、2 项专用测试默认忽略；真实 DLL 测试另外显式运行通过，涵盖控制注册器、暂停/继续、不可恢复取消、文件清理和正常卸载。Range 断言改为有界等待分段请求，避免首次进度早于分段建立的测试竞态。
- 桌面服务测试 5 项、TypeScript 类型检查、前端构建通过。原生桌面 UI 尚未实测。
- 依赖迁移后再次通过实际 libaria2 应用/MCP 双版本下载、安装和程序/stdio 使用。证据分别为 C:/Users/Public/nas_home/AI/GameEditor/linshi/assetlibrary-local-trial-00u87n/application-update-evidence.json 和 C:/Users/Public/nas_home/AI/GameEditor/linshi/assetlibrary-local-trial-S2UGH1/mcp-update-evidence.json；身份仍是本地合成主体。
- 发行合同、依赖合同、严格行数和 diff 检查通过。

## 开发验收包

C:/Users/Public/nas_home/AI/GameEditor/linshi/Loom-libaria2-dev-20261017/Loom.exe 为本轮 Tauri debug 构建，配套当前 daemon 和固定 DLL，daemon --version 已执行通过。未自动启动、未覆盖旧包。

这是非正式的开发验收包：没有完整内置 Art/框架/运行时集合，仍使用既有用户配置路径。README.md 写明启动、端口、取消和测试边界，build-receipt.json 记录摘要并标记 formalRelease=false。正式完整发行和完整组合对应源码交付仍未完成，releaseApproved 未开启。

## 后续源码绑定交付批次

同日后续生成 `C:/Users/Public/nas_home/AI/GameEditor/linshi/Loom-libaria2-sourcebound-dev-20261017.zip` 及同名目录。它包含从 1,546 文件源码快照重新编译的桌面 EXE 和 daemon，而非复制上节旧二进制；全部源码在构建后再次核验。三份 Rust 锁文件的 891 个依赖版本已 vendor，daemon 和桌面 Rust 部分用本地依赖离线构建通过；npm 缓存另经 `npm ci --offline` 和类型检查验证。

新包携带静态运行库版权文本，回执绑定源码 ZIP、Rust 源码归档、npm 缓存归档与本次二进制。源码 ZIP 已在独立目录解压并逐文件验摘要。实际 DLL 专用测试在该源码快照上重新编译并运行，1 项通过，覆盖暂停/继续/取消及清理。没有进行原生桌面窗口交互验收，仍是非正式 debug 包，缺少完整内置模块集合。

`loom-libaria2-sourcebuild-v4` 从原始 aria2 归档重新编译成功，输出 SHA-256 与固定 DLL 一致；`loom-libaria2-source-materials-v4.zip` 包含有摘要绑定的 build.sh、link.map 与源码。未将 GCC/MinGW 源码 URL 当成交付完成；精确获取进展见 `LIBARIA2_SOURCE_DELIVERY.md`。

随后五份 GCC/MinGW 原始源码与 Ubuntu 修订全部实际下载并通过大小/摘要校验，已打包为 `loom-native-toolchain-sources-20261017.zip`；不包含未经执行的 PGP 或法律审核结论。对 build.sh 的真实篡改夹具也验证了导出器在产生输出前拒绝摘要不符。

源码导出器 5 项测试通过；原生源码材料拒绝覆盖、损坏续传、超长文件、旧回执的合同通过。发行布局、发行合同、路径安全、篡改检测、smoke 模块与 GitHub Actions 合同全部通过；实际在线 OSV 再次通过，沿用已有例外，没有新增豁免。

完整 Loom 托管仍是独立未完成项：Web 8 MiB、上传预留/表约束 16 MiB、检查与版本表 8 MiB、ZIP 单文件 1 MiB、Loom 下载/应用安装 8 MiB，以及共享解包压缩输入 64 MiB 限制均存在。上传/下载字节通道已有流式处理，但 Worker 检查、网页摘要和 Loom 安装仍整包缓冲；因此本轮没有盲目提高所有常量。现有便携应用的版本切换也不是正在运行的 Loom 自更新器。

## 2026-10-18 完整模块开发包与生产 MCP 更新

新增 `C:/Users/Public/nas_home/AI/GameEditor/linshi/Loom-libaria2-complete-dev-20261018.zip`，80,423,635 字节，SHA-256 `f78fbf6764fb5f5443c1b894a9e53fedbc50881d39a9fbf72c57b9326333d72d`。保留源码绑定 debug 桌面/daemon/DLL，从当前模块源码重新构建四个 framework、两个 MCP、七个 Art；没有用旧发行包的模块替代本次构建。process 内含 Python，stock-api 内含固定摘要的 Node。首轮 Windows PowerShell 因环境下 Get-FileHash 不可用失败，切换实际可用的 PowerShell 7 后三类模块构建成功，原失败日志保留。

13 份模块 ZIP 与 summary 的长度及摘要一致。独立解压到 `linshi/loom-complete-receiver-20261018` 后逐一核对 47 个载荷文件，daemon --version 实际成功。新回执 runtimePayloadComplete=true 仅表示上述发行模块集合齐备，不代表完整业务验收。formalRelease=false、desktopUiVerified=false、combinedApplicationSourceComplete=false 保持；未放开正式发行门禁。包中 base-build-receipt.json 保留原 EXE 对应源码身份，module-inputs.json 单独记录模块源码及运行时输入；运行时二进制清单不冒充完整对应源码。

真实生产 MCP 消费验证使用独立临时 daemon（18765）和已认证的 B 账号，没有替换旧 Loom。在正式 AssetLibrary 获取 1.0.0 和 1.1.1 的独立授权，正常下载/安装回执均为 downloadEngine=libaria2，随后 stdio 工具分别返回两个真实版本值，旧版保留。进程模块确认实际 DLL 摘要与固定值相符。证据为 `linshi/loom-production-consumer-20261018/mcp-production-evidence.json` 及两版 install/execution JSON；syntheticIdentity=false，原生桌面 UI 仍未验证。

## 2026-10-18 大包客户端后续增量

新增 application-large-zip-format-v1 专用 96,000,000 字节下载预算及文件句柄安装路径，旧策略预算不变。Windows 下载句柄拒绝并发写入/删除；大包摘要与文件摘要分块验证，应用解压沿用路径、链接、重复项和膨胀保护。桌面入口识别新策略，但这不是 Loom 正在运行时的自替换/重启器。

安装聚焦 10 项通过、2 项默认忽略，包含约 80 MB 的真实 EXE 应用双版本安装、运行差异和旧版保留；下载/控制 5 项通过、1 项默认忽略；共享 ZIP 保护 6 项通过；桌面服务 4 项和类型检查通过。真实固定 DLL 专用测试亦通过，新增 80,000,000 字节 Range 下载与内容验证，原暂停/继续/取消/卸载仍通过。

大流量首轮实际发现 os error 10035：Windows accept 继承非阻塞监听模式，write_all 在发送缓冲区满时提前失败。生产 relay 和测试服务现显式切换为阻塞连接，仍保留超时；没有靠增加重试次数掩盖错误。新源码尚未进入上节旧开发 EXE；没有执行生产 0012、发布大包或宣称完整 Loom 自更新/原生 UI/正式对应源码发行完成。

修复后 relay 三项回归通过：真实 80,000,000 字节转发及接收端短暂停读、固定对象/Range/凭据隔离、重定向拒绝。严格有效行数检查通过，未新增超限文件；git diff --check 通过。

## 新大包开发交付及完整包联合验证

`linshi/Loom-large-dev-20261018-v1` 已包含本轮重新构建的 desktop 与 daemon；源码快照 `linshi/loom-large-source-20261018-v1` 有 1,548 个文件，构建前导出、构建后逐文件对照一致。build-receipt.json 记录实际 EXE/DLL/模块摘要。模块沿用完整开发包，非重新构建全部模块；formalRelease、desktopUiVerified、selfUpdateVerified、combinedApplicationSourceComplete 均为 false。

资源库封装 `linshi/assetlibrary-software-package-h0R5YF/package.zip` 为 80,166,238 字节，SHA-256 `922b112abae075dc433d7737f905b17f2b5526143e70e3c27c796079e3475efb`。已在隔离本地真实 Worker/D1/R2 下完成上传、检查、审核、发布、授权，经 libaria2 下载后文件式安装，并实际运行包内 daemon --version。证据位于 `linshi/assetlibrary-local-trial-zC8P9q` 与 `linshi/loom-daemon-contract-cloud-complete-loom-63092-1`。合成身份和本地服务不等于公网验收；当前用户实例未停止或替换，运行中 Loom 自更新仍未实现。
