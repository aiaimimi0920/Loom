# 引擎源码材料使用说明

本目录材料仅对应 libaria2 下载引擎，不是完整 Loom 组合应用源码交付包。source-manifest.json 的 combinedApplicationSourceComplete 保持 false。

在具有 bash、tar、make、pkg-config 和 x86_64-w64-mingw32 工具链的环境中，从材料目录执行：

```bash
bash ./build.sh /absolute/path/to/new-build /absolute/path/to/aria2-1.37.0.tar.xz /absolute/path/to/bridge.cpp
```

输出目录必须是专用新目录，不使用用户数据目录。构建脚本不会自动安装工具链。编译器版本见 compiler.txt；原始源码、桥接和脚本摘要见 source-manifest.json，DLL 身份见 build-receipt.json。原生构建需要静态 libstdc++/MinGW 链接输入，其源码和许可材料仍需补齐后才能宣称完整原生对应源码交付。

当前材料中的源码归档不作修改；配置参数和最终 DLL 链接命令在 build.sh 中。构建不涉及上传内容扫描，也不执行 AssetLibrary 包载荷。
