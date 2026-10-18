# 原生 Art ZIP 与 AssetLibrary 兼容性

2026-10-16：真实交付包 `custom-image-blend-script.zip` 的 ZIP 条目使用反斜杠，
因此 AssetLibrary 严格预检拒绝 `ZIP_PATH_INVALID`。不通过放宽平台路径检查解决。

`Build-LoomSampleArtPackages.ps1` 改为调用 `NativePackageArchive.ps1`，在签名之后
以 `/` 路径写入文件条目，不添加目录条目，不改清单或任何载荷字节。
输出使用 CreateNew，拒绝覆盖与输出位于源目录内的情况；递归读取前拒绝链接目录/文件。

聚焦验证：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/tests/Test-NativePackageArchive.ps1 -OutputRoot C:/Users/Public/nas_home/AI/GameEditor/linshi/loom-native-package-tests
```

此测试核对路径、所有原字节（含清单和签名夹具）、已存在目标不被覆盖，以及输出不能位于输入目录。
签名夹具不冒充真实签名；另外已在 linshi 内生成独立测试 Ed25519 密钥和信任库，
重建实际 Art 样例并解压，通过既有 `loom-plugin validate --trust-store` 验证为 Trusted。
测试信任没有写入用户的 control-plane，也没有变更正式发布者密钥。

实际重新打包的 Art 已通过 AssetLibrary 原生结构预检，证据见兄弟仓库
`AssetLibrary/docs/LOOM_NATIVE_PACKAGES.md`。原始旧包完整保留，没有替换当前用户运行的桌面包。

本修改仅修正包生成兼容性，不表示 AssetLibrary 正式发布/下载准入已完成；
桌面下载后的安装接线、实际权限批准和运行验收仍待完成。
