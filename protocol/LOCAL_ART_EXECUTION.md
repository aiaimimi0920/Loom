# Otter 单侧 Art 执行许可合同

## 当前唯一准备合同（v538，2026-11-04）

配套软件同步升级最新架构；不维护旧协议/模式/程序兼容，不进行版本协商、旧路由探测、降级或新旧互通验收。以下v533及更早段落保留为历史，不再授权其准备路径或兼容流程。当前有效的grant/resource/describe/service等v1基础合同不是旧准备入口，继续按各自安全合同使用。

唯一Device准备入口是 `POST /v3/art/local/prepare-package`，请求协议 `loom.art.local.prepare-package.v3`，严格包含protocolVersion、binding、permissions、services、knownSourceDigest；原16 KiB门不变。响应严格包含protocolVersion、grant、source（Base64/null）、package。null仅表示命中当前源码缓存，每次仍需要新grant；HTTP错误直接终态，不重放到其他入口。

package为Ed25519独立域 `loom.art.local.package.v3` 加NUL的Envelope。payload严格包含protocolVersion=`loom.art.local.package.v3`、package（现用完整grant/devicePublicKey/resources DTO）、ports（inputs/outputs）、visibility（inputs/outputs布尔数组）。显隐数组长度与对应端口精确相等，顺序相同，和端口共同受8 KiB门；总payload、资源、权限与原期限不扩大。不同方向可同名且默认值不同。

默认值只读取不可变安装包每个端口的 `defaultVisible` bool，缺省显示；不读取旧metadata.defaultVisibility表。Otter用户按名覆盖优先，默认值随签名包保存、重开和有限离线携带，隐藏不改变图权限。签发/资源读取仍复核Device、审批、完整包/源码及撤销；离线仍绑定设备key/可信issuer、原期限及完整资源，无Device Session，不获得网络服务权。

Loom Device和LAN均只开放当前准备路径，原请求预算与签名域隔离不变。Otter主链只验当前包，不依次尝试旧证明。历史独立API/夹具尚未全仓删除，但无兼容承诺，不能为其恢复旧服务路由。运行证据及产品未验边界由Otter当前构建台账和本批本地audit维护，不把本合同存在当成产品/发布验收。

## 签名包端口合同（v533）

新增精确 Device POST `/v2/art/local/prepare-package`、请求 `loom.art.local.prepare-package.v2`，签名 payload 为独立 `loom.art.local.package.v2` 域，严格包含 protocolVersion、原 package 和 ports；旧 package.v1 与原权限/设备/审批/包/资源/期限保持，不在同协议偷换语义。ports.inputs/outputs 每侧至多32项，名字UTF-8至多128字节、类型64字节，不接受空白/控制字符/同侧重名；端口JSON至多8KiB，并与资源共享原签名payload预算。

执行前沿原Lease/Control读取 `/v1/art/local/issuer`，新增可选 `supportedPrepareProtocols`；响应至多4096字节，必须与独立可信Pin的algorithm/keyId/公钥一致，不从网络自动授信或新建Lease。旧响应缺能力直接走v1，不能先探测未知v2路由；advertised v2 后任何拒绝（包括404）都失败关闭，不降级重放。原v1 package到resume的精确404回退仍保留。

Local Art 现在从 Loom 已验证的不可变安装包取得输入/输出的真实名字和类型；Otter 接入目录、图像输入、来源配置凭证、自定义主图片输出继续 Add Art、正式结果及保存重建/有限离线。显式零输入/零输出保持为空，不伪造 input_image/output_image；只有无新证明的旧包保留默认端口。首次自定义主输入迁移在候选 Workspace 内提交，冲突保原图；零输入只撤回本次工厂创建且仍完全一致的暂定边，保存恢复不继承删除旧边的权限。端口保存记录仅是包绑定提示，不是执行/离线授权。

本批最终选定 **83项Rust passed**，其中17项新回归；当前选定终态failed=0，不是整库。签名域/篡改/预算、旧能力协商、拒绝不重放、图拓扑、中文自定义输入、真实2×2红像素、Live配置换代拒迟到、零输入不交付像素、自定义rendered输出继续建子Art及Save/窗口重建/有限离线均已验。真实Loom LAN HTTPS跨进程联合在线和停服后离线独立核signedPorts、完整资源、正式像素和正式结果；不完整缓存拒绝，父子不混加。Otter strict all-targets Clippy、两仓fmt、官方版本/strict行数、Otter lib/普通Release/工作区与Tile驱动构建通过；Loom签名安全/daemon测试库通过。66项源码delta均UTF-8无BOM，Rust最大500有效行，无新增行数例外。两个原跨仓grant门及两个SDK示例默认ignored未运行；Loom整仓strict Clippy旧债未重跑，不称两仓全绿。

逐端口显隐/默认值与持久化、可编辑capsule/完整端点键盘仍未完成；普通完整Art/Live/键盘与人工IME、普通双产品、物理多屏/跨机DPI、剩余全量/合计资源长稳、最终完整隔离包和用户全面验收仍开放。Talk外部会话控制构想和旧Web/Tauri混用按用户决定排除。三冻结副线/v464固定包、用户release97保持；不启动Loom原生迁移，不提交推送发布部署，整体迁移active。

## 原生宿主只读观看加入

Device 专属 `POST /v1/art/local/live-viewer/attach` 使用独立
`loom.art.local.live-viewer.v1`，最多 32KiB。严格字段为 protocolVersion、完整 binding、
原签名 grant Envelope、原 `loom.live.v1` session_ack envelope 及
requireExistingMembership。旧 Surface viewers 端点不接受此对象，也不接受缺附件请求。

当前 Device actor、grant 的完整绑定/签名/在线期限、显式 Preview、当前审批、immutable
包/源码摘要及工具启用均须复核；批准更新与最终加入串行化。仅明确的原生用户菜单意图
使用此入口，Preview 是宿主证明，不给脚本新增联网 API、控制、输入或任意服务权限。
离线恢复宿主不产生 viewer proof；原生宿主仍须当前且首帧已接纳。

加入复用原 Live membership/控制事件/媒体 owner，不产生伪 Surface 附件或另一套会话。
Ready 前宿主关闭/替换取消待加入；Ready 后观看独立于发起面板与工作区，沿原观看
Device Lease、membership、显式停止/撤权/退出结束，不自动续入或以离线许可入网。
该合同本身不证明当前 LAN 已支持全部 Live 控制/媒体路由，亦不代表 Source 发布/控制完成。

v484专属HTTP测试及真实LAN HTTPS/Otter加入父子联合通过，核实际Device、签名宿主、
成员与撤审批拒绝；联合显式声明`mediaTested:false`，不冒充普通可见双产品或完整Live媒体验收。

## 显式单PNG服务结果（v460，定向与真实HTTPS联合通过）

`NeuroSurface.serviceImage(service,input,options?)`返回`{output,image}`；output是原64KiB以内JSON元数据，image为当前会话私有句柄。
options只含timeoutMs及互斥的images或canvases（沿原输入合同），不能指定路径/URL/Base64/owner。省略来源则是纯JSON输入。
新POST `/v1/art/local/service/submit-image-output`为独立`loom.art.local.service.image-output.v1`，顶层严格含protocolVersion、call（旧Submit）、images（可空）。
仅此精确路径继承原图片请求上界；旧submit/submit-images不接受新协议。poll-image-output仍发原Control，cancel沿原端点。
新响应严格含protocolVersion、reply（原Reply）和image；仅成功时image非null，包含width、height、digest、标准Base64 png。
原JSON64KiB/控制96KiB门不变；新响应上界为96KiB加单PNG的Base64上界，单PNG最多2MiB、边长4096、解码16MiB。
Capability成功output必须是对象，content严格一张type=image/mimeType=image/png图片；data为标准Base64或精确PNG Data URL。
content从元数据移除，其他字段继续核原JSON门；不会读output_path、跟随URL、接受非PNG或从effects发布图像。
Loom完整有界解码并核尺寸/digest，任务表指纹包含输出模式；编码存储共享全实例8MiB租约。
原owner/签名/当前设备/批准/Lease和敏感gesture门不变；失败/取消/旧会话迟到不能发布图片。
成功消费后沿原cancel清理终态PNG但不改成功终态、不重新执行；再次索取已释放图片明确409，不凭空恢复或重放。
Otter验身份/序号/摘要/尺寸，经原受控pipe二进制交child；PNG不进入JS JSON或heap，安装后才续接Promise。
输入与结果合用原累计8图片/8MiB PNG/16MiB解码/32MiB Canvas，重复ID/预算/解码失败不替换旧资源；资源随Session回收。
随后drawImage与commitResult沿原Canvas及Application/Save/Checkpoint，不直接修改源图/Editor/canonical。旧Loom不支持新端点时明确失败，不降级。
本批Otter294、protocol11、daemon43项去重定向通过，含四条真实双仓HTTPS联合和新公开示例的隐藏Application/Save恢复；
大于旧JSON门的PNG保原分辨率逐像素核验，digest拒绝、累计原子容量、停止和终态存储释放均覆盖。
两仓产品构建及Otter三项既有产品worker通过；Otter严格Clippy通过，Loom仍101条旧诊断且无新增签名。
单图结果不代表所有旧包/多图结果、敏感服务、普通EXE图像输出可见UI、跨端或最终整包完成。

## Canvas服务输入（v459，定向与真实HTTPS联合通过）

`NeuroSurface.serviceCanvases(service,input,readonly ArtCanvas[],timeoutMs?)`只接当前原生Canvas身份，至多4项，
拒绝零尺寸、重复、伪造和数组getter。调用时同步提交已有绘制命令，后续绘制/resize不改变该次快照；
使用内在width/height，与CSS、DPI或屏幕可见尺寸无关，也允许当前会话的离屏临时Canvas。
PNG只在私有Rust父子通道中传输，不暴露toDataURL/任意字节接口；宿主完整复核原PNG codec/尺寸和像素预算。
单PNG2MiB、合计编码3MiB、原Canvas250ms/8次提交/8192命令/32MiB门均不增加；无变更末帧复用原背板，不重复提交。
外部仍是下节service.images.v1和原权限/任务/取消链，不新增Loom生产端点或自动降级。serviceImages仍只接已安装image句柄。
参数准备拒绝Proxy重入，选择失败不消耗私有调用序号；输出仍有界JSON，不生成敏感gesture或修改原图/Editor/canonical。
本批Otter287、protocol10/daemon38项去重通过，含三种输入真实HTTPS联合及撤审批；另三项普通产品worker显式通过。
这些证据不是普通产品可见菜单、服务图片输出、真实跨端或最终包验收；Loom严格Clippy仍有既有101诊断、无新增签名。

## 图像输入服务扩展（v458，定向及双仓HTTPS通过）

新增显式`NeuroSurface.serviceImages(service,input,[imageHandle],timeoutMs)`，只接当前mount安装的不可变image句柄，
不接受Canvas、URL、脚本base64、文件路径、外来resource/lease或gesture。旧service仍是原JSON合同；两入口共享单operation
一个在途/最多32次调用及原60秒绝对期限。PNG只从原宿主已验证输入/包资源读取，按句柄数组顺序传给服务resourceRefs。

Device专属POST `/v1/art/local/service/submit-images`使用独立`loom.art.local.service.images.v1`：
顶层严格含protocolVersion、call（原service.v1 Submit完整对象）、images（严格`{"png":"<标准Base64>"}`数组）。
至多4图，单图4MiB、累计8MiB；编码长度/字符/解码长度在分配解码前复核。请求体上限是原96KiB加对应Base64上界，
仅该精确POST路径使用，查询/后缀/其他路由不继承。新消费者不将404或错误自动降级成无图service调用。
旧submit/poll/cancel和Reply不接受新增字段；图像调用的poll/cancel仍用原service.v1 owner/grant，无图片重传。

准入沿原issuer/完整binding/当前设备/审批/immutable包及源码，新增图片不会绕过服务白名单；
接收图片的Capability插件和命令均须当前Trusted且显式授予`hook.unit.image.read`。local_art/service_images
复用原Extension的upload权限/解码/Surface lease和CapabilityResourceBroker只读暂存，不复制业务实现或开放通用管理端点。
原任务表幂等指纹包含有序图片内容摘要；重复submit不重执行，改图或将同call改为纯JSON均409。
暂存包含在原任务监督期限内；成功/失败/取消/撤权/进程排空后才释放broker和上传lease，终态表不持图片。
PNG签名在暂存后复核，实际解码仍归服务，不声称服务端做了完整codec验证。

返回值目前仍是有界JSON，不接受图片输出资源或自动工作区effects；敏感命令仍缺真实gesture时拒绝。
旧Loom可继续服务旧JSON调用，新图像端点不受支持会明确失败；回退源码不需要数据迁移、删除缓存或改旧授权。
本批Otter263、protocol10/daemon37项去重定向运行通过；原JSON/新图片双仓HTTPS联合均显式通过，
包含真实Device Session、签名Capability进程、逐字节PNG及取消/撤审批和暂存租约回收。
两仓产品构建及Otter实际产品Canvas/状态worker通过；这些worker门不等于产品EXE可见服务交互。
Otter严格Clippy/lib通过；Loom严格Clippy仍有既有101/10诊断，未观察到新增，不声称Loom全仓通过。
本合同仍不包含Canvas服务输入、图片输出和敏感gesture，也不关闭可见联合、真实跨端、长稳及最终包验收。

## 原生作者入口

独立类型和直接可运行的JS示例见 [sdk/otter-art](../sdk/otter-art/README.md)，不合并旧loom.surface.v1全局。
当前作者API为define/snapshot/resource/image/service/serviceImages/serviceCanvases/serviceImage/commitResult；声明只覆盖核心子集，不是浏览器或完整SDK。
service仍是下述有界JSON合同，不传图片输出资源、不伪造敏感gesture、不自动提交工作区effects。
同份JS示例由Otter显式测试读取并独立核摘要，经原签名包/真实child/隔离TLS和隐藏Application验证；
默认ignored不计通过，最终状态以该批实际回执为准，不能由类型检查推导真实产品或跨端通过。
v457同份公开示例已显式运行通过200/503、停服后新Application有限离线、独立2x2/42正式结果与Save恢复；
Otter相关82项去重运行及严格Clippy/lib构建通过。TypeScript5.9.3无DOM严格正反例通过；没有修改Loom生产Rust或依赖，未重编未变daemon。

## 显式包资源扩展（v454已完成选定合同、运行及跨进程离线验证）

新增Device专属POST `/v1/art/local/prepare-package`，请求沿原cached完整binding/范围，
protocolVersion为`loom.art.local.prepare-package.v1`；响应严格含protocolVersion、原grant、source（Base64/null）和package。
package是独立Ed25519域`loom.art.local.package.v1`+NUL的Envelope；载荷严格为protocolVersion、同次完整grant、
devicePublicKey和resources。devicePublicKey为空只支持在线；非空必须为当前设备规范公钥且原offlineUntilMs非零，
显式允许同设备在原期限内建立新宿主绑定。旧grant.v1/resume.v1和旧prepare不改变签名域或换绑语义。

metadata.otterExecution可声明resources，元素为`{"entry":"assets/config.json","resource":{"id":"config","mimeType":"application/json","size":13,"digest":"<SHA-256>","dimensions":[0,0]}}`。
entry仅包内路径，不进入wire；resolver、发布者信任、依赖锁、设备和审批照旧。源码复用已有`<entry>.sources.json`
有界拼接（最多32个依赖），sourceDigest必须为实际拼接字节的摘要，仍限1MiB/UTF-8/无NUL。
声明资源的新包经旧准备端点明确409拒绝，不能静默返回缺资源程序；describe与服务仍重验同一包/源码合同。

资源最多16项；PNG沿原8图片、单项4MiB、累计8MiB及解码16MiB门，非PNG单项256KiB、累计仍受原512KiB资源JSON门。
支持image/png、text/plain、application/json、application/octet-stream；非图片dimensions为[0,0]，PNG尺寸由IHDR及child WIC再次核对。
POST `/v1/art/local/resource`只接受protocolVersion=`loom.art.local.resource.v1`、原grant和resourceId，最多32KiB。
每次重验当前设备/审批/精确immutable包/源码和实际资源size/digest，返回同协议、grantId/resourceId/digest/标准Base64 bytes。
没有任意URL、路径、通用文件/网络能力或新执行权限；下载串行且共享十秒资源截止与原许可更早期限。

Otter把资源映射到snapshot.state.package[id]，描述中的resourceId为`art-package:<id>`；text/plain与application/json
经NeuroSurface.resource返回原UTF-8字符串，octet-stream返回标准Data URL，PNG经NeuroSurface.image安装句柄消费。
与art-input资源共同核512KiB JSON、8图片/解码门及原Canvas32MiB，不扩配额，不吞资源或缩小图片。
磁盘资源按最多1MiB分块，和源码共用原32项/32MiB与额外1MiB暂存，不另设缓存池、不驱逐旧内容。
只有全部源码/资源提交后才原子发布完整包证明到原32KiB离线槽；缺任意块/篡改/到期/设备或pin不匹配均拒绝恢复。
缓存失败可保留已获授权的在线执行，但不发布离线资格；在线拒绝或网络失败不自动降级。
新域证明内资源清单不可移除后充作旧单入口resume；离线仍没有Device Session，服务固定offline失败。
公开Schema为schemas/local-art-package.v1.schema.json。此实现不等于可见UI、任意旧包兼容、真实跨端或最终整包验收。

状态：本批Otter668项相关passed/5默认ignored，Loom protocol9/security6/daemon25项passed，另5项默认ignored；没有部署声明。
三项HTTPS联合入口另显式通过，含在线服务/撤审批、单入口停服恢复，以及完整资源包在线正式42/2x2像素→首进程退出→Loom停服→缺块拒绝→新进程离线恢复。
后续只修显式ignored联合夹具，未变主体/产物按哈希复用；实际双仓产品构建及Otter Canvas worker通过，不是可见产品菜单验收。
daemon严格Clippy仍有相同42未改文件101诊断，protocol直接严格检查仍有相同3未改文件10诊断，本批文件无新诊断。
完整资源包的本合同及有限离线已经选定验证，不等于可见产品UI、完整SDK、真实跨端、长稳或最终包验收。
这不是 A1 端到端完成证明。旧 `loom.hook.v1`、`loom.surface.v1` 不改变语义，也不自动广告新能力。

## 协议与执行归属

- 独立协议 `loom.art.local.grant.v1`，SDK 标识 `otter.art.v1`；双方明确支持才可使用。
- `executionOwner=otter`：本合同对应的 Art JS 只在 Otter 所属设施执行，不移交 Loom 执行。
- `resultOwner=otter_workspace`：本地文档正式提交仍经 Otter Save/Checkpoint owner。
  许可并不允许脚本直接写会话文件；预览不是正式结果，也不改变远程服务自身的结果提交合同。
- 基础grant描述一个有界JS入口；多文件拼装和包资源使用上节显式package扩展，SDK及服务仍遵守各自授权/结果合同。
  不能由此宣称所有旧包兼容、Shader 已迁移或权限沙箱已完成。

公开 wire Schema 为 `schemas/local-art-grant.v1.schema.json`；`$defs.grant` 描述签名载荷。
本新安全合同对未知字段和重复字段失败关闭，不回退为旧 Surface 或 device token。
原生消费者使用自身 DTO，不依赖 Loom 私有 Rust 模块或服务器文件路径。

## 签名、信任与资源身份

外层仅有 `algorithm=ed25519`、`keyId`、`payload`、`signature`。
payload 与 signature 都使用标准有 padding Base64；外层最多 24 KiB，解码后 payload 最多 16 KiB。
签名消息精确为 UTF-8 `loom.art.local.grant.v1`、一个 NUL 字节、payload 解码后的原始字节。
消费者先按独立可信 issuer pin 验签，再解析载荷，不将 JSON 重排后验签。

响应不包含可信公钥来源。keyId 只是查找已信任 issuer 的标识，不能单凭同名选择 peer 提供的公钥。
签发者密钥不是默认包发布者密钥；必须另行接通认证握手/配对、密钥轮换与撤销信任。
当前通用签发函数只对调用者已授权的载荷签名，不自行从设备登录或包安装推导授权。

三种凭据必须分开：

1. `packageDigest` 是 Loom immutable package 的 canonical digest，不是 ZIP bytes 的 SHA-256。
2. `source.digest` 是分发 JS entry bytes 的 SHA-256，`resourceId` 必须为 `sha256:{digest}`。
   entry 可为 Loom 有界拼接后的源码，不能将其 digest 冒充整个包 digest。
3. 新签名许可是执行范围证明；Device token 和 resource lease 仍是各自服务端状态型凭据，不能代替它。

源码不得内嵌控制载荷。Otter 在产生 Permit 前核对大小（1 byte 至 1 MiB）、完整 SHA-256、UTF-8 与无 NUL。
缓存/磁盘恢复仍须验证这些条件；基础源码缓存不自行授予资源或离线权限，完整资源使用上节显式包证明及共享预算。

## 精确绑定与权限

artId 使用真实 immutable package 的 `publisher/art` 身份，两段分别遵守包组件规则；不接受冒号别名、路径遍历或 Windows 保留名称。

签名绑定 origin、requestId、deviceId、workspaceId、instanceId、generation、artId、artVersion、packageDigest、sdkVersion。
origin 来自已认证连接的规范 origin，消费时与请求完全一致，不依据响应重定向或猜测别名。
artVersion 是 resolver 已确认的精确版本字符串；wire 长度/字符校验不替代 resolver 的 SemVer 校验。
generation 为非零、JavaScript 安全整数。新请求必须由宿主生成新 requestId，不由脚本指定或重用旧缓存身份。

权限为 `local_state`、`preview`、`loom_service`、`workspace_result` 的显式不重复子集，无通配符、文件/进程/任意网络权限。
`workspace_result` 是新增的显式正式结果权限，不由 preview、服务成功或 resultOwner 字段推导；旧审批不自动扩权。
不认识该枚举的旧客户端应拒绝新许可，不能删除未知权限后继续执行。原协议与签名域不改义。
Otter 正在接入 `NeuroSurface.commitResult(outputs)`：结果采用独立 `otter.art.result.v1`，每 operation 最多一份，
只接受有界 JSON 值或当前已验证 Canvas 的引用；不接收脚本指定的工作区、请求、路径或网络资源身份。
返回 true 仅表示排队，绝不代表磁盘保存成功。正式端口仍由当前实例的宿主核验并经原 Save/Checkpoint 持久化。
显式workspace_result/当前Canvas结果及Save接线已通过相关回归；不是可见产品或完整SDK验收声明。
启用 loom_service 时必须有非空服务 ID 白名单（最多 32 项），未启用时白名单必须为空。
Otter 按自己已保存的请求核对完整权限/服务范围，不从收到的 grant 反向构造 Expected。
服务请求仍要消费该服务当前授权、敏感确认、取消及正式结果门禁；这不是代理任意远程调用的 bearer token。

## 有效期、离线与终止

### 显式离线恢复扩展（单入口源码已定向及跨进程联合验证）

旧grant.v1、prepare.v1/v2不改变绑定或响应语义。新增设备专属POST
`/v1/art/local/prepare-resumable`，请求的protocolVersion为
`loom.art.local.prepare-resumable.v1`，其余binding/permissions/services/knownSourceDigest沿v2。
响应包含同名protocolVersion、原grant、必需source（Base64或null）、必需resume（独立Envelope或null）。
只有当前设备绑定公钥、显式审批offlineTtlMs非零且全部原prepare准入再次通过，才签发resume。
payload严格为`{"protocolVersion":"loom.art.local.resume.v1","devicePublicKey":"<Base64>","grant":{}}`，
其中grant与同次返回的原grant逐字段相同；独立签名域为`loom.art.local.resume.v1`+NUL。
Schema为`schemas/local-art-resume.v1.schema.json`。旧grant签名不能充当resume签名。

resume明确允许宿主在离线时为同origin/device公钥/精确Art版本、包digest、SDK、源码和范围建立新请求身份；
不允许替换这些稳定身份，不发出伪造的v1 grant，不向Loom服务发送重建后的内存绑定。
期限仍取原offlineUntilMs和expiresAtMs最小值（当前服务端原有效期最多5分钟），重启不能续期。
它不是Device token，离线本地租约没有网络Session；Control、执行Control、媒体和DNS准备均不能从它取得凭据。
有loom_service范围的Art可运行本地部分；该模式服务调用返回固定offline失败，不做网络尝试、不静默删权限。

Otter只在新Application中没有在线会话/尝试/显式失效记录时，从用户OpenArt意图尝试离线恢复；
容量、在线拒绝、网络失败都不自动降级。先读已有profile设备身份（不新建/替换），按当前受信pin验签，
核设备公钥及ID、完整源码、宿主新代际，再沿原有界Tasks/Runtime/正式结果与Save接线。
在线describe前先原子减权该Art缓存；显式配对/续签后台联网前持久化origin截止，旧签发时间凭证不能复活。
授权清除、切换或重新授权先停止本地窗口；撤信任继续撤销同owner租约。
显式清除另经原有界授权worker持久化origin截止，已接受意图不被随后退出取消；普通Select只停止本地会话。
身份加载仅Missing允许旧在线兼容，锁冲突/损坏失败关闭；签发时间小于或等于cutoff均拒绝，磁盘失败明确报错。

离线目录最多32个槽（含origin减权记录），单槽32KiB、总1MiB，暂存额外最多32KiB；
独占锁、拒reparse、原子替换、未知文件计费，不主动驱逐或删除旧内容。源码仍归原32项/32MiB缓存。
启动接受前持久化墙钟高水位；发现回退则将该槽不可逆减权，执行内沿原Permit及本地租约的墙钟/单调门。
这保护合作进程的正常重启，不提供可信硬件时钟，不抵抗同用户恶意回滚整个profile或尚未观察到的关机时钟篡改。
显式联合门确实先在线签发、退出首个Otter进程、停止Loom，再启动第二个Otter进程；没有Device Session，
真实child捕获服务Promise的offline失败后仍提交正式42结果。不是产品EXE可见菜单或完整资源包验收。
离线期间无法实时获知服务端新撤权，最多持续到原离线期限。完整包资源使用上节显式扩展；可见跨端/长稳/最终包仍另验。

- issuedAtMs、expiresAtMs 使用毫秒时间戳；有效期严格为正且不超过 15 分钟，过期时刻本身即拒绝。
- offlineUntilMs=0 禁止离线；否则必须晚于签发且不超过 expiresAtMs，不能因缓存重新下载而延长。
- Otter 会话内同时检查墙钟和自准入开始的单调时间；离线期限也有独立单调上限。
- 已观察到时钟回退、撤权、换 origin/包/设备/工作区/实例/代际、停用或过期，Permit 不可逆终止。
  晚到成功、恢复在线或改回旧身份不能复活同一个 Permit。
- 离线时无法凭空得知服务端新撤权；可用性只持续到已签发的离线期限或本地先收到的撤权。
- 原grant.v1的Permit仅有会话内绑定，不能把磁盘旧grant换绑；跨进程恢复须使用上节独立resume及当前持久issuer信任。
  resume.v1本身不含资源清单；完整包重启须package.v1及全部缓存，不提供可信硬件时钟或即时离线撤权分发。

## 生产源码准入路由

- `GET /v1/art/local/issuer`：仅认证设备可获取签发者标识、公钥和算法。返回公钥不建立信任；客户端必须另行获得可信 pin。
- `POST /v1/art/local/describe`：设备认证，严格请求 `{"artId":"publisher/art"}`，最多 16 KiB；不接受别名、额外或重复字段。
- `POST /v1/art/local/prepare`：仅认证设备可请求；请求含完整 `binding`、`permissions`、`services`，拒绝未知字段，JSON 上限 16 KiB。
- `POST /v1/art/local/prepare-cached`：设备认证的显式v2，仍执行原prepare全部准入与签发事务；见下节。
- `GET/PUT /v1/art/local/approvals`：仅管理员可读写显式执行审批。PUT 使用 `expectedRevision` 和完整 `approvals` 列表；旧 revision 拒绝为 409。

审批保存在根目录的 `settings/local-art-approvals.json`，精确绑定 deviceId、artId、artVersion、packageDigest，
并限制 permissions、services、validUntilMs、offlineTtlMs。无文件默认拒绝；损坏文件失败关闭，不自动重置。
设备配对、安装成功或拥有旧 Surface token 都不能自动获得此审批。

prepare 复用 immutable package resolver 的精确版本、digest、publisher trust 和依赖锁验证，不修改 active version。
已验证 manifest 必须包含 `metadata.otterExecution`，例如：

```json
{"sdkVersion":"otter.art.v1","entry":"local.js","sourceDigest":"<source SHA-256>","permissions":["local_state","preview"],"services":[]}
```

entry 必须为包内 `.js` 相对路径，无反斜杠、冒号、空段或点路径；读取复用 canonical containment，
最多 1 MiB，要求 UTF-8、无 NUL，且源码 SHA-256 与已验证 manifest 一致。
响应为 `{"protocolVersion":"loom.art.local.prepare.v1","grant":{...},"source":"<standard Base64>"}`；源码不嵌入签名 payload。
当前签发上限 5 分钟，并进一步截断至审批及设备会话有效期；离线期限不能超过该有效期。

源码解析前先检查审批；签发阶段持审批锁复核相同审批，并复核设备会话身份、撤销、有效期和工具 enabled 状态。
同步 resolver 同时只准入一个 prepare，竞争返回 429；这不等于对 resolver 历史递归工作量提供总预算。
publisher trust 和依赖锁在解析阶段检查，未与其所有写入口建立原子快照，不能声称撤信任与在途签发原子互斥。
删除审批阻止后续签发，不代表立即远程撤销所有已签发离线许可。

签发复用 daemon 持久 identity，但使用独立签名域；私钥不通过路由暴露。workspace/instance 由 Otter owner 提交，
服务端只验证设备归属，不证明客户端工作区存在，也不允许客户端从响应反向构造 Expected。
实现责任在 `apps/daemon/src/local_art/{mod,policy,source}.rs`；issuer 桥接在 `offline_peers/local_art_issuer.rs`。

### 显式prepare.v2与持久源码缓存

旧prepare请求/响应不增加字段、不静默改义。新端点请求严格为：

```json
{"protocolVersion":"loom.art.local.prepare.v2","binding":{},"permissions":[],"services":[],"knownSourceDigest":"<64 lowercase hex>"}
```

binding与范围沿原grant合同；不接受缓存的grant或设备/issuer替换字段。服务器重新读取并验证当前源码，
签发绑定本次请求的新grant，响应`{"protocolVersion":"loom.art.local.prepare.v2","grant":{},"source":null}`
仅当knownSourceDigest等于实际源码摘要；不相等则source为完整标准Base64源码。source字段必须存在。
缓存声明不是授权，不减少任何设备、审批、resolver、源码和签发复核；原16KiB请求/单解析槽不变。
HTTPS与设备白名单只精确增加该POST端点，不开放approvals/通用invoke/查询参数/管理身份。

Otter在既有profile的art-source-cache保存验签后的原始JS，索引包含origin/device/art/version/
packageDigest/SDK与独立可信issuer的keyId/publicKey。每次仍实时describe、生成新的请求身份并完整验签；
不会把磁盘grant换绑定复用。v2明确404才一次回退v1，其他错误不得执行缓存。
缓存最多32项/32MiB，单项1MiB，暂存额外最多1MiB；独占锁、拒reparse point、同目录原子替换，
未知文件/崩溃tmp计入容量，不主动驱逐内容。不存授权凭据，缓存故障只失去传输优化。
本功能不提供离线describe、跨重启设备租约/授权恢复或完整资源包缓存。

### 精确描述，不等于执行许可

describe 仅查询当前 enabled 安装条目的精确版本和 canonical digest；没有该设备对该版本/摘要的有效审批即拒绝，
不回退到其他已审批版本。随后复用 prepare 的 immutable resolver、publisher trust、依赖锁、manifest 和源码验证，
并与 prepare 共用单个解析槽；竞争返回 429。返回完整审批范围，必须是 manifest 的合法子集，不静默取交集。

成功响应严格为以下结构，六个 descriptor 字段均必需：

```json
{"protocolVersion":"loom.art.local.describe.v1","descriptor":{"artId":"publisher/art","artVersion":"1.0.0","packageDigest":"<canonical package SHA-256>","sdkVersion":"otter.art.v1","permissions":["local_state","preview"],"services":[]}}
```

响应不包含安装路径、源码、grant、issuer 公钥、设备凭据或工作区/实例身份。解析后持审批锁复核完整审批内容，
复核设备会话代际/有效期和当前 enabled 安装身份；不宣称与所有安装/撤信任写入口原子互斥。
消费者限制响应至 16 KiB，拒绝未知/重复字段、不同 artId、未知 SDK 和非法权限/服务组合；origin/device 来自原连接及租约，
workspace/instance/generation 来自当前宿主。描述可以变旧，不能离线当许可使用；prepare 必须再次执行原全部准入与签发检查。

## 客户端及完整端到端接入仍须满足

### 显式敏感确认扩展（实现、定向与真实HTTPS联合已通过）

独立`loom.art.local.service.confirmation.v1`，Device专属POST `service/submit-confirmable`、
`service/poll-confirmable`和`service/confirm`；旧service.v1/images/image-output合同及cancel不变。
submit严格包含protocolVersion、call（原Submit）、images与imageOutput；控制严格包含protocolVersion、
control（原Control）和decision。poll的decision为null；confirm为`{challenge,approve}`，不是脚本入口。
响应为protocolVersion、原reply、image或null、confirmation或null，挑战不进入插件progress。
Challenge固定nonce、fingerprint、pluginId、commandId、packageDigest、scopeId与expiresAtMs；
fingerprint是JSON元组`[input,timeoutMs,imageOutput,true,[SHA256(各图片标准Base64字符串)]]`的UTF-8 SHA-256。
宿主核本地不可变输入、原owner、序号、挑战稳定及期限，再展示可信参数和实际图像预览；没有宿主批准默认拒绝。
同一Task/Gate只接受原挑战，重复相同决定幂等，反向决定拒绝，终态不可再次批准或重放。
Gate持有当前CommandScope与贡献generation；runtime准入在命令登记锁内再次核精确scope，旧gesture也绑定scope。
普通命令不强制弹窗；requiresUserGesture命令只有可信宿主批准后才沿原runtime控制执行。
原全局/设备任务和在途容量、资源暂存、InvocationControl及取消/shutdown继续复用，没有第二套任务/资源表。
单调期限在scope读取前固定，不持任务表锁等待包初始化；确认等待、执行与结果图片编码共用原期限。
撤权/命令换代/到期或取消后不得提交迟到JSON/图像，终态保留原防重放语义；停止先取消再排空worker。
运行验证和真实可见UI仍分别登记，不把类型、HTTP夹具或程序化测试宿主批准当作真实用户确认。
v461最新组合输入runtime53/protocol12/daemon38、Otter57项去重通过；确认/旧JSON/图片/Canvas/图片输出五条真实HTTPS联合均已显式运行。
原生默认拒绝/长参数全文/线程焦点、取消/到期/撤权/命令换代和原设备容量、图片暂存及shutdown均有选定回归。
两仓普通产品构建、Otter三个既有worker与严格Clippy通过；Loom严格Clippy保留daemon101/protocol10旧诊断和runtime4条未改文件诊断。
本批新增op_ref已修复重验，未靠放宽门限或删除测试通过；这不关闭普通EXE可见用户确认、真实跨端、长稳或最终整包。

### 异步服务批次（服务端、Otter 接线及 HTTPS 联合门已定向验证）

服务请求/控制/任务回复采用独立 `loom.art.local.service.v1`，代码位于
`loom_protocol::local_art::service`。完整 owner 包含原 binding、grantId、宿主生成的 callId 和精确 service ID；
submit/poll/cancel 必须同时消费原签名许可、认证 Device 会话及当前审批/包启用状态。不能只凭 callId 读取结果或取消，
不能将通用 `/v1/invoke` 或 `/v1/runs` 加到 Device 白名单来替代此边界。

Submit 携带有界 JSON input 与 timeoutMs；请求最大 96 KiB，input/output 最大 64 KiB，真实 progress 最大 4 KiB，
单次 timeoutMs 为 1..60000，实际运行期限还必须取命令自身期限与剩余许可的最小值。不得把重试当作重新执行授权。
任务回复有 accepted/running/succeeded/failed/cancelled，单任务 sequence 单调；成功必须带 output（JSON null 合法），
失败只带固定错误代码，未完成/取消不能带 output。消费者必须检查原 owner 和状态代次，正式工作区结果另走原提交 owner。

capability runtime 已通过 InvocationControl 保留真实有界进度与终态，服务任务从该快照生成回复；没有伪造进度。
Otter 私有 Promise 已在 JS 栈之外挂起/续接，排除宿主等待而不重置原执行预算；已下载 Prepared 向 signed Local
移交同一 Permit/Lease 与固定 origin Client，短授权锁不跨 HTTP。真实 child＋回环 TLS 测试与下述真实 daemon 联合门分别留证。
专属任务 owner、执行/取消/回收和 HTTP 路由已完成服务端定向验证，
服务端新增 Device 专属 POST `/v1/art/local/service/submit`、`poll`、`cancel`，
每次都以当前 daemon issuer 验签并复核完整 owner、当前设备会话、精确审批和包启用；submit 另复核 immutable 包信任与源码摘要。
不会开放通用 `/v1/invoke`。服务 ID `capability:<publisher>:<plugin.command>` 精确映射已启用的 Plugin 命令，
例如 `capability:publisher.example:local-service.run` 对应 `publisher.example/local-service.run`；不进入 core/admin dispatcher。
命令原有 schema、包权限和敏感 gesture 门继续生效；该 JSON 服务入口不伪造 gesture，不携带资源租约，不自动应用 effects 或提交工作区正式结果。

任务表由各 DaemonRuntime 实例持有：全局最多 128 条/16 在途，每设备最多 32 条/4 在途；满额返回 429，
无排队重试。终态保留至原许可到期，重复同 owner/input/timeout 返回已有任务，改变输入返回 409，不静默重放副作用。
工作线程使用 Loom 内部生成的 invocation ID，不采用客户端 callId 取消其他调用。
独立期限覆盖初始化与执行；超时先取消并等待清理，再发布终态。周期维护复核撤权并取消任务，poll 和最终结果发布也复核授权。
退出先取消并 join 任务，再关闭 runtime；终态不复活，取消后的迟到响应不覆盖结果。
成功 JSON null 是有输出的成功值，wire 中 `output:null` 保留；无输出状态省略 output 字段。
Otter 只有已下载、已验签并持有原 Client 的 Prepared 可注入服务 dispatcher；直接 Local::mount 无该上下文仍拒绝服务权限。
offline 服务暂拒绝；可见进度、远端主动取消、敏感/资源服务与正式结果仍须补齐，不据此宣称完整 SDK 已验收。

跨仓联合门由 `apps/daemon/src/tests/local_art_joint.rs` 与 Otter 的 `art_service_joint_tests.rs` 配合：
Loom 独立测试 daemon 实际安装/批准/启用签名插件，Otter 经真实设备签名续签和 prepare 下载/验签后执行异步 Art，
随后验证调用输出、JSON null 和服务端撤销审批。该门只在显式指定测试 EXE 时运行，测试身份/日志位于隔离临时目录；
失败保留证据，父进程有界回收其自建 Otter 进程。joint-live-2 显式运行已通过（不是默认 ignored 通过）：实际设备签名续签、
签名插件调用、首次 calls=1、后续 JSON null、服务端撤权后拒绝下一调用及 Otter 退出均有断言。
首次明文 HTTP 夹具被 Otter 正确拒绝，失败证据保留；没有为测试放宽生产客户端。联合门改走现有 HTTPS listener，
仅在测试中绑定 loopback，证书由隔离 daemon 生成并显式信任。`lan_http.rs` 精确允许 issuer GET、describe/prepare POST
及 service submit/poll/cancel POST，分别沿用16KiB/96KiB请求上限；不允许 approvals、通用invoke、额外查询或管理认证。
本输入路由/Art回归13通过、联合入口1通过、原LAN边界2通过；两仓格式和行数通过，Otter严格Clippy通过。
Loom严格Clippy仍有与此前一致的既有诊断，本次文件无新增诊断；不把定向运行通过写成全库/最终包/多机验收。

签发路由必须先核认证设备、明确启用/许可、精确 resolver/trust/依赖锁及源码资源映射。
客户端必须把可信 issuer、当前 device Lease、工作区/实例代际和主动取消接入 Permit；不能用硬编码 false 忽略撤权。
挂载前与结果交付前都检查 Permit；实例 registry 仍负责唯一激活、一次性请求接纳与旧结果隔离。
新的 grant 本身不是全局防重放数据库，也不启动 QuickJS。异步服务、正式保存和端到端 Art 用例仍需独立验证。
