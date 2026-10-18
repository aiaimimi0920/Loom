# libaria2 对应源码交付方案

2026-10-17 用户明确接受准备 GPL 兼容的源码交付方案。本文件不是已经完成合规审核、发布源码或批准发行的证明。

## 交付范围

内嵌 libaria2 的发行物按组合程序处理，不以动态加载 DLL 作为免除 GPL 义务的理由。aria2 为 GPL-2.0-or-later；最终组合发行所选 GPL 版本还须与实际链接依赖逐项核对。保留 Loom 自有文件已有 MIT/Apache 声明和第三方各自声明，不把所有源码批量改成 GPL。组合发行说明与原文件许可证须分别表述。

向二进制接收者同时提供与该二进制完全对应的源码归档，而非只链接上游最新版。归档应包含：

1. Loom daemon 及实际链接的内部 crates、清单、锁文件、构建脚本和配置；桌面部分如纳入同一组合发行，提供其匹配源码和构建材料。
2. aria2 1.37.0 已验证的原始源码归档、COPYING、桥接 bridge.cpp、build.sh、PowerShell 构建与安装脚本。
3. 实际静态链接依赖的对应源码及许可声明，工具链与系统库的适用例外单独记录。仅列 Windows DLL 导入名不能证明此项完成。
4. 必要的生成输入与安装信息、明确的构建命令、工具链版本及平台前提；不得依赖未公开的本机绝对目录才能复建。

## 身份与可复建性

源码快照必须取自构建使用的确切文件，包含已纳入构建的未提交修改；不能仅导出 HEAD 后声称与 dirty 工作区二进制对应。归档清单记录每个文件 SHA-256，并绑定二进制摘要、依赖锁文件摘要、构建回执和使用的构建工具版本。

归档采用明确的源码白名单；不得包含账号凭据、下载票据、用户配置、运行数据、.env、临时日志或私有签名密钥。不能直接打包整个工作目录。固定依赖来源后在独立目录执行一次复建，确认材料齐备；若二进制不完全一致，保留差异并调查，不伪造可复现结论。

## 发布检查

- 向接收者提供可下载的匹配源码归档及摘要，并附完整许可证和组合发行说明。
- 对下载引擎的完整静态依赖做清单与许可检查；顶层 SBOM 不能代替完整清单。
- 核对安装信息及修改版本替换条件，不把当前固定摘要加载机制直接宣称为满足所有 GPL 版本的要求。
- 源码材料检查、依赖检查、二进制测试和实际打包全部通过后，再调整 releaseApproved；用户同意方案不自动等于这些检查通过。

截至本文件创建：用户已同意方向；源码交付归档与组合发行许可证核验尚未完成。后续实际进展见下节；未改变仓库 LICENSE，未公开源码，未解除正式发行门禁。

依据：aria2 官方 https://aria2.github.io/；GNU GPL FAQ https://www.gnu.org/licenses/gpl-faq.html#GPLStaticVsDynamic 。本方案为工程交付清单，最终许可适用范围需结合实际发行物复核。

## 2026-10-17 源码快照与离线复建

新增 `scripts/source-delivery.mjs`，通过明确目录白名单导出 Git 已跟踪和未忽略的新文件的当前原字节，不导出 HEAD 代替 dirty 工作区。拒绝越界路径、大小写碰撞、目录链接、覆盖已有输出、源码目录内输出；过滤常见凭据文件和生成目录。每个文件记录 SHA-256，导出后再次读取原文件检查并发变化。此过滤不是公开前秘密信息复核的替代品。

实际材料均在 `C:/Users/Public/nas_home/AI/GameEditor/linshi/`：

- `loom-source-delivery-20261017-v3.zip`：1,546 个文件及清单，3,767,208 字节。已解压到独立目录并逐文件验证。
- `loom-rust-source-dependencies-20261017-v1.tar.gz`：三份 Rust 锁文件的 891 个依赖版本，包含 Cargo 校验文件及原有许可文本。已用导出的 vendor 目录执行 `cargo build --offline --locked -p loom-daemon`。
- `loom-npm-offline-cache-20261017-v1.tar.gz`：Windows 构建用 npm 缓存，未包含缓存日志或用户 npm 配置。独立工作目录实际执行 `npm ci --offline` 和类型检查通过。这是恢复构建材料，不等同于所有 npm 原生构建工具的完整对应源码。
- `Loom-libaria2-sourcebound-dev-20261017/`：从上述快照重新构建的桌面 EXE 与 daemon，构建后再次核对快照和构建工作目录的全部源码摘要；其 `build-receipt.json` 同时绑定二进制和三个材料归档的摘要。

复建为 debug/no-bundle，未自动启动桌面，也没有替换旧包。构建使用当前已安装 Rust/MSVC、Node、Tauri 系统前提，不宣称所有开发机上的 EXE 逐字节可复现。v1/v2 导出曾将名为 credentials 的 Rust 模块误过滤，独立编译真实暴露缺文件；已修复并加保护测试，以 v3 成功复建为准。

`native/libaria2/build.sh` 现在输出链接映射；新构建回执绑定 build.sh 与 link.map 摘要，源码材料导出拒绝缺少这两项的旧回执，不能给旧回执补造事实。既有固定 DLL 使用相同对象重新链接并生成映射后，SHA-256 仍为 `6f8b8cc51aca77af7c2275bca95933f815aac29faf5919e2dd06c49a7cefb732`。重链接首次使用绝对输出路径导致 MinGW 自动 ImageBase 变化；改回原构建的相对输出文件名后摘要一致，未改运行时固定摘要。

GCC/MinGW 精确源包及 Ubuntu 修订摘要写入 `native/libaria2/toolchain-sources.json`，获取工具为 `scripts/Get-LoomNativeSourceMaterials.ps1`。静态运行库的实际版权文件已加入新开发包；不能用单一“MinGW public domain”概括所有 CRT 与 winpthreads 文件。下载完成状态须以实际 `download-receipt.json` 为准，不将 URL 清单当成已经交付源码。

当前 `combinedApplicationSourceComplete` 与 `releaseApproved` 仍为 false：还需完成整个正式发行物的源码/许可证组合复核、完整运行时集合打包、接收者侧验证和正式发布流程。当前快照和归档只在本机，未公开上传。

后续同批次已实际取回并逐一验证五份原生源码材料：`gcc-mingw-w64 24.3`、`gcc-10 10.3.0` 原包与 `10.3.0-14ubuntu1` Ubuntu 修订、`mingw-w64 8.0.0` 原包与 `8.0.0-1` 修订。实际归档为 `C:/Users/Public/nas_home/AI/GameEditor/linshi/loom-native-toolchain-sources-20261017.zip`，包含下载回执、固定来源/摘要清单、获取脚本与当前已安装运行库的版权文本。原始文件总计 94,036,732 字节，SHA-256 全部与固定记录一致；未验证 PGP 签名，不能将哈希匹配称为完整供应链或法律审核通过。

获取期间遇到 Windows TLS 中断、Launchpad 长传输断流及不支持 Range；未关闭证书验证。GCC 原包改从 Ubuntu 官方 archive 的同名同摘要归档恢复，最终完整摘要通过。`-Resume` 仅服务于本源码材料工具，支持跳过已验证文件、保留并继续未完成文件，并拒绝完整但摘要错误或超长的已有文件；不代表 Loom 用户下载已支持跨进程持久续传。
