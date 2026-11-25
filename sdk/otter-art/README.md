# Otter 原生 Art 作者入口

本目录描述显式 SDK 标识 `otter.art.v1`，与旧 `sdk/surface` 的 `loom.surface.v1`
分别使用，不能把两份全局声明混入同一个 TypeScript 项目。运行设施归 Otter，Loom 负责精确包、审批、
签名和受认证服务；完整许可合同见 [LOCAL_ART_EXECUTION.md](../../protocol/LOCAL_ART_EXECUTION.md)。

## 编写与检查

JS 可用 `/// <reference path="../native-art.d.ts" />` 启用本目录类型；TS 可导入其中的类型。
产物必须是无运行时 `import`/`require` 的 JS，直接调用 `NeuroSurface.define(...)`；
类型声明不会被注入执行环境，不授予权限。不要启用 `lib.dom` 或 Node 类型来掩盖缺少的宿主能力。
`tests/tsconfig.json` 使用 ES2022、strict、无 DOM 和无自动发现的 `@types`。

从 Loom 根目录，使用已经安装的 desktop TypeScript 执行（不需要新增 SDK 依赖）：

```powershell
node apps/desktop/node_modules/typescript/lib/tsc.js -p sdk/otter-art/tests/tsconfig.json
node --check sdk/otter-art/examples/packaged-image-service.js
```

`tests/types.ts` 同时保护正例和负例。`@ts-expect-error` 必须被编译器消费；
URL/伪造图片、非 JSON 服务输入、任意 base64 正式图片、旧 emit、浏览器/Node、未支持路径方法均应拒绝。
声明只覆盖已核验的核心树、按钮、事件和 Canvas 绘制子集，不声称完整 DOM/Canvas/Web API 类型覆盖。
例如没有 `quadraticCurveTo`、`ellipse`、`getImageData`，也不能把 Canvas 自身当作 `drawImage` 的输入。
选择器只支持当前原生的简单选择器，不接受任意浏览器 CSS；路径不支持跨变换状态累积。

## 同份可运行示例

`examples/packaged-image-service.js` 是直接分发的源文件，不是打包器模板或可直接安装的 Art 包。
它创建“运行”按钮、原分辨率 Canvas 和短状态；按按钮后调用已批准服务，最后提交正式图像与 JSON 值。
mount 返回移除监听器的清理函数。源文件使用以下已验证资源和接口：

- `snapshot.state.package.config`：JSON 资源，内容为 `{"answer":42}`；`resource()` 返回原 UTF-8 字符串。
- `snapshot.state.package.picture`：PNG 资源；`image(resourceId)` 返回当前会话安装的私有图片句柄。
- `service("capability:vendor:service.run", {answer:42}, 5000)`：成功示例返回 JSON `null`。
- 正式输出端口 `output_image` 为 image，`answer`/`mode` 为 value，必须与包的端口声明一致。

包还需要精确 `metadata.otterExecution`、源码摘要、资源摘要/尺寸、发布者信任、设备授权和显式审批；
本示例至少请求 `local_state`、`preview`、`loom_service`、`workspace_result`，服务白名单必须精确包含上述 ID。
源码摘要是实际分发 JS 原字节的 SHA-256，不是整个包的 canonical digest。不要把样例身份当作真实生产服务。

`service()`只接受有界JSON；`serviceImages(service,input,[NeuroSurface.image(resourceId)],timeoutMs)`另接当前安装的不可变句柄。
图片按数组顺序成为Capability的resourceRefs/stagedResources，最多4图、单图4MiB/合计8MiB，接收插件与命令均须获批image.read权限。
这不是Canvas快照API；不能传Canvas、URL、伪造对象或base64。新服务端缺端点会明确失败，不静默丢图回退为JSON调用。

`serviceCanvases(service,input,[canvas],timeoutMs)`是显式的当前Canvas输入入口：按调用时的内在位图快照，
后续绘制或resize不改变在途内容；CSS/DPI不缩放源图，也不读取屏幕。至多4个非零、不同的当前Canvas，可为离屏临时Canvas。
编码使用原单PNG2MiB/编码帧3MiB门，绘制仍累计原250ms/8提交/8192命令/32MiB预算；超限明确失败，不缩图或放宽预算。
PNG不交JS字符串，没有toDataURL或路径/任意字节上传；原serviceImages的已安装句柄语义不变。
三个入口共用同一 operation 至多一个在途调用，单次期限为 1..60000ms，且受原 operation/授权更早期限约束。
参数准备期间拒绝重入，Canvas选择失败不占用服务序号；原尺寸/透明像素、后续修改隔离、取消和真实Loom HTTPS输入已定向验证。
普通Otter产品worker也通过，但不是产品可见交互或最终整包验收；上述三个入口输出仍为JSON。
这些入口不携带图片输出资源、敏感 gesture、通用文件/网络权限，也不会自动应用服务 effects。
示例只捕获固定 `offline`、`service_unavailable`、`service_transport`、`service_busy`，仍可提交本地图片；
认证、身份和信任等失败不应被伪装成服务成功。有限离线必须已经获批并有完整缓存，不能以网络失败自动降级或续期。

`commitResult()` 返回 `true` 仅表示当前 operation 单槽排队；宿主仍复核原实例、来源、授权、端口和预算。
它不是磁盘保存回执；结果由 Otter 原 Results/Save/Checkpoint owner 接纳，插件不能指定工作区路径。
较大图片用已安装 `image()`，不转换成巨大内联字符串；所有资源/解码/Canvas 均沿原预算，超限明确失败而非缩图。

## 运行验收边界

### Live 自动回调（v520，原生限定联合通过）

`onLiveTrigger(event)`只在已有Live观看的可信原生菜单中，用户明确配置条件并选择“允许自动回调”后派发。
见[完整合同](../../docs/LOCAL_ART_LIVE_TRIGGER.md)。Preview、打开Art、加入观看和离线票都不等自动授权。
条件由Loom唯一引擎求值；事件只含sessionId/epoch及delivery，不含grant、token或自行登记API。
回调不是click/物理gesture，不请求焦点或恢复暂停timer，敏感服务仍须原可信宿主确认。
`examples/live-trigger.js`为同份源码示例：声明data正式value端口，记录真实观察及一次回调计数；
需要local_state/preview/workspace_result精确包和审批，不能直接当安装包或生产服务。
只执行一次领取的有界payload；停止、失联、换代、过期和未知领取不重放，不持久恢复旧自动授权。
Applied ACK只在Otter原Results实际事务成功后产生，formalRevision是本执行实例的成功提交序号，
不是磁盘版本或远端呈现证明。v520同份live-trigger.js已通过真实Loom HTTPS/WSS、签名child、原Application正式结果/Save及服务端Applied/Stop联合；类型和隐藏控件/取消门通过，普通可见产品、真实跨机与最终包未验。

### 显式敏感服务确认（已通过选定定向与真实HTTPS联合）

`serviceWithConfirmation(service,input,{timeoutMs?,images?,canvases?,imageOutput?})`仅请求可信宿主询问，
不是批准。images与canvases仍互斥；imageOutput=true成功返回`{output,image}`，否则返回JSON。
不能传approve、userGestureToken、挑战或宿主身份。旧service系列不会自动获得敏感命令许可。
原生宿主显示服务/包/完整参数和实际上传图像的有界缩略图及来源/尺寸；允许本次或拒绝，默认拒绝，
无宿主观察通道也拒绝。该提示为非模态窗口，不抢焦点；等待消耗原调用期限，不创建额外授权。
停止、到期、设备/执行审批撤销、命令包/scope换代都使原票据失效；插件进度不能生成挑战。
提示阶段沿同一个任务poll观察远端终态，图片在真正批准后才向Capability暂存，结果仍沿原owner/预算与Save链。
参数区Enter和Escape默认拒绝，长参数保留全文；创建不自动设置焦点。
v461已通过签名child/TLS、原生隐藏HWND/宿主重验、五条实际双仓HTTPS及相关模型/取消撤权回归。
最新组合Otter57/runtime53/protocol12/daemon38项去重通过，另统计不重复累加三个已包含的产品worker；两仓产品构建通过。
Loom严格Clippy仍有已留证旧诊断，不声称全仓通过。这不表示普通EXE可见用户确认、真实跨端或最终包已经验收。

v460新增显式`serviceImage(service,input,{images?,canvases?,timeoutMs?})`及同份`examples/service-image-result.js`，已通过选定运行验收。
images与canvases互斥，省略则仅传JSON；成功返回`{output,image}`，output为有界元数据，image可直接drawImage或再次作为serviceImages输入。
服务使用单张PNG content合同（见上链接），不读取output_path/URL，不自动应用effects；旧端点不支持时明确失败，不静默降级。
结果PNG不进入JS Base64，单项2MiB/边长4096；与原输入共享当前Session累计8图/8MiB PNG/16MiB解码和32MiB Canvas预算。
资源由Session统一回收，超过累计门只拒绝调用，不替换旧句柄或缩图；本批不提供任意字节导入或显式单图释放API。
新Application显式门使用OTTER_ART_IMAGE_EXAMPLE/OTTER_ART_IMAGE_SHA256核同份源码，已实际验证正式像素、原图不变、Save恢复及shutdown。
本批Otter294、protocol11、daemon43项去重通过，包含四条真实双仓HTTPS联合、新输出串联输入、取消与终态图片释放；
两仓产品构建及Otter三项既有产品worker回归通过，不把既有worker门当作新输出的可见产品交互。
Otter严格Clippy通过，Loom仍101条旧诊断且无新增签名。未运行的ignored测试不计通过，也不声明完整SDK或最终包验收。

Otter 的 `application_local_art_sdk_tests.rs` 读取本示例原字节，按调用者独立冻结的 SHA-256 核对，
经原签名包、真实 child、隔离 TLS 和隐藏 Application 验证服务成功、503 隔离、有限离线、独立 2×2 像素与正式值、
原图不变和保存恢复。该显式 ignored 门需要 `OTTER_ART_SDK_EXAMPLE`（绝对路径）和 `OTTER_ART_SDK_SHA256`（小写摘要），
普通测试默认忽略不等于运行通过。必须在新测试 EXE 的 `--list` 中核对完整名后显式运行 `--exact --ignored`。

v458图片输入专属测试已通过实际双仓HTTPS、签名Capability进程、逐字节PNG、撤审批和暂存租约回收；
同份公开示例仍走原JSON服务，另显式回归通过，不把它冒充图片服务示例或可见产品验收。

类型检查和隐藏 Application 不证明普通 EXE 可见交互、真实 Loom 产品联合、资源型/敏感服务的完整合同、
所有旧包兼容、跨端、长稳或最终发布包；这些仍分别验收。
