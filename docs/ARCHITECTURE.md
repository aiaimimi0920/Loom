# Loom Architecture

## Otter 统一设备与分享入口

连接申请与设备配对共享设备身份、审批和管理列表。断开撤销连接会话及在线状态，保留
配对；删除才取消配对。桌面 `managed-devices` 保留审批和编辑，不提供平行新增设备或
独立发送图片入口。产品流程见 `docs/OTTER_CONNECTIONS.md`。

`offline_peers/transfers/shared_source.rs` 负责源关联的唯一接收者准入，`shared_import.rs`
将经已信任 peer 导入的邀请接到既有接收日志、读写及撤权链；不复制源关联。
`shared_projection_peer_http.rs` 覆盖本机重导入、跨 peer 唯一接收者及源撤权终态。
`offline_catalog_http.rs` 使用真实 CredentialStore 生成当前与直接互联 Loom 的登录摘要，
校验设备映射、一跳范围和凭据隔离。账号分享按发送／接收角色校验跨账号权限。

## 原生桌面订阅的 Pong 写期限

`desktop/runtime/otter_bridge_subscription/session`继续独占固定事件、认证TLS连接与取消回收。收到Ping后，排队Pong的flush取得独立且仍为250ms的写期限，不沿用可能已经耗尽的读slice；没有增加重试、权限、线程、兼容分支或改变服务端30秒Ping/90秒idle合同。实际TLS回归先强制旧读期限到期并观察旧flush超时，再验证同一连接交付空及非空Pong；原订阅/重连/确认/停止共11项通过。该机制修复不等于证明历史30秒零样本的唯一根因。

普通壳CPU有明显前台上下文差异，当前观测主要落在壳与WebView2而非daemon，具体热点未定位；MCP前台短样本也可升高，不能只归因于Otter同步页。没有据此启动Loom原生化，普通双产品满负载及长稳仍另行验收。

## Invocation取消的进程所有权

`loom_capability_runtime::host::invocation`在取得package锁后复核单次control，惰性初始化返回后若已取消，只回收该次新建的process；未使用的预热persistent进程仍归共享plugin，而非任一Device grant。已派发Command沿原controlled call终止/排空，初始化仍沿manifest timeout。Device撤权HTTP回归先观察真实Command进度，再独立验证撤权后原3秒无poll回收；不会以受理或预热PID代替执行，也不新增设备级全插件kill。该规则不改变协议、授权、资源预算或并发限制。

## Otter当前Local Art准备接缝（v538）

Loom只为当前v3 Device请求签发不可变包的完整资源、端口及逐方向显隐；Otter单侧执行和持有限离线证明。local_art/source_ports拥有当前defaultVisible投影，package/cached拥有唯一prepare请求，LAN adapter承担相同16 KiB准入。旧协议/模式/程序兼容已由用户取消，不协商或降级；授权、签名、撤销与预算不变。详见[当前合同](../protocol/LOCAL_ART_EXECUTION.md#当前唯一准备合同v5382026-11-04)。

## Otter v520 本地Art自动触发原生接缝

原生显式配置→原Live唯一控制worker Configure/Claim→签名child onLiveTrigger→原Application/Results实际事务→ACK及Stop/回收已接通。独立不可逆自动授权绑定完整实例/工作区/包、会话/epoch与条件revision；Preview不等授权，重连不复活旧gate。单槽/16KiB、原期限与共同容量门不变，不新增线程、timer、持久队列、权限或WebView。
用户在既有观看系统菜单配置严格Condition并明确“允许自动回调”；停止先撤本地gate。回调不伪造click/gesture、不请求焦点或恢复暂停timer。Applied只在正式端口原子提交后产生，formalRevision为当前执行实例成功提交序号，不是磁盘版本；NoResult只在实际present后报告。原Results最终predicate再核Runtime/Lease及固定期限，Stop与canonical事务共用无Win32/网络/脚本回调的短锁。
真实HTTPS/WSS联合的服务端Applied/Stop已与Otter原Results/Save独立核对；不等普通可见产品或真实跨机验收，见[完整合同](LOCAL_ART_LIVE_TRIGGER.md)。

## 本地 Art Live 触发服务（Otter v519，服务端定向通过）

新增版本化 Device-only 显式自动回调合同，原 grant 四权限与普通 Surface 合同不变。
本地与 Surface 复用唯一条件状态转移、共享 64 注册门；本地登记单槽领取/回执及停止回收归 Live store。
详细授权、失效/未知不重放及产品未接边界见[本地 Art Live 触发合同](LOCAL_ART_LIVE_TRIGGER.md)。

## 原生Tile本地值联合接缝（Otter v512）

Loom本批仅tile_surface_local_native_joint专属cfg(test)叶及原action joint最小include变化，生产协议/权限/executor/依赖不变。put_snapshot按原合同仅更新指定attachment；夹具对Source与Tile独立view分别发布同源/新权威快照，不改服务广播语义。
当前274项相关原生/普通工作区Rust完整名去重passed、6 ignored、0当前终态failed，非整库。9条真实Loom LAN父调用通过，共15个独立会话，不把父/子重复累计；新本地值鼠标/键盘联合分别13.875/13.079秒，核零网络动作、同源保值/新权威采用、恰好一次仅含当前文本的表单、实际framework/Art正式结果、已paint以及保Source/回收view和线程。旧文本/离散值/Slider的键盘与鼠标、点击/确认/拒绝/取消联合均重验。真实WIC/D2D像素使用独立常量及既有权威Select基准；自有离屏HWND不冒充普通可见产品或最终交换链。
v511的键盘/Tab接线及当前回归虽已通过，input6的values LAN派发超时仍缺根因证据；后续成功不等生产修复，不发布该批INTEGRATION_VALIDATED，也不把旧失败改写为通过。 本地cache与字段预算归Otter，服务权威不转移；完整失败/产物复用/101旧Clippy与过期行数例外见Otter WINDOWS_NATIVE_BUILD的v512记录。

## 原生Tile文本联合接缝（Otter v510）

Loom本批仅cfg(test)的tile_surface_text_native_joint、原action joint最小include及外置tile-text-handler；生产协议/权限/executor/依赖/配置不变。四个独立原30秒Source会话核实际中文emoji/IME合成边沿、textarea CRLF、number和无handler当前表单；独立handler记录与常量oracle核input/commit/discrete、权威/正式/已paint、保来源附件及清理。原生EDIT/Group/队列均由Otter持有，不转移服务权威。失败、精确产物复用及101旧Clippy/过期行数例外见Otter WINDOWS_NATIVE_BUILD的v510记录。本批关闭Tile文本/textarea/number、IME合成边沿及文本草稿表单按钮的隔离闭环，不等完整键盘/Tab导航、已安装输入法候选窗人工验收、旋转活动EDIT视觉、全部值控件本地draft/完整表单。inputCapabilities仍为pointer。普通Tile输出EXE可见链、最终交换链读回、物理多屏/跨机/热插拔/真实DPI、本地Art Live触发合同、长稳、其余迁移退出条件、最终完整隔离包和用户全面手测仍开放；旧Web/Tauri混用互通已按用户确认排除。

## 原生 Slider 联合接缝（Otter v509）

Loom本批只有cfg(test)的tile_surface_slider_native_joint/include和外置tile-slider-handler；生产协议、权限、executor、依赖和配置均不变。handler独立解析实际continuous/commit payload，联合核50→100、唯一最终commit、144次move有界合并、权威/正式/已paint以及保源/清理；原30秒父门不改。Otter异步依赖等待由其原唯一Art worker/32槽队列拥有，不转移服务权威；详细输入/失败/复用和101旧Clippy诊断及过期行数例外见Otter WINDOWS_NATIVE_BUILD的v509记录。只覆盖声明了 input/change 的 Slider 鼠标交互；无 handler 仍只读，文本/IME/完整键盘与完整表单仍开放，inputCapabilities 保持 pointer。普通 Tile 输出 EXE 可见链、最终交换链读回、物理多屏/跨机/热插拔/真实 DPI、本地 Art Live 触发合同、长稳和最终整包/用户全面手测仍开放；旧 Web/Tauri 混用互通已按用户确认排除，不再计为本轮待办。

## Tile 离散值联合接缝（Otter v508）

本批 Loom 生产协议、权限与 executor 不变；tile_surface_value_native_joint 由原 actions 专属叶登记，复用真实 LAN 配对/Wall binding 与受管 framework/Art。外置 tile-value-handler 独立解析实际 commit payload，oracle核恰好 true/b 两次执行、正式101/102和原生已paint场景，取消/当前项零执行，最终保源并回收view/线程。测试在原30秒内等 patch、正式结果与成功ACK共同终态，不把异步中间态当失败或改生产来刷绿。只覆盖声明了 change 动作的离散值；无 handler 仍只读，没有新增本地 draft，不广告文本、IME、连续 Slider 或完整键盘导航，inputCapabilities 仍为 pointer。普通 Tile 输出 EXE 可见链、最终交换链像素读回、物理多屏/跨机/热插拔/真实 DPI、本地 Art 触发、长稳及最终整包/用户全面手测仍开放。 Loom全仓strict旧Clippy/过期行数例外仍阻塞；详细当前输入、失败与回执见 Otter WINDOWS_NATIVE_BUILD 的 v508 记录，没有新普通daemon包。

## v507 三类动态Tile联合

tile_mixed_joint_tests/tile_mixed_native_joint专属cfg(test)叶复用原真实Source系统菜单、签名配对/续签、Loom LAN HTTPS/WSS、Tile Runtime和唯一原生呈现。静态图片通过新资源/layout、Live通过原Source受预算纹理、Art通过原snapshot/资源分别换像素；独立手写PBGRA核三路首帧和变化后同一已paint帧，不以帧号增长冒充内容变化。冻结保实际Frame Arc并停Tile媒体但保Source，黑场/恢复沿原链；停止/撤权核view/像素/Shared/线程/媒体permit清理和源附件/权威值7保留。
本批相关Rust完整名去重70 passed/0 ignored，无终态失败，不是整库；26份input1实际定向回执经input2仅两专属新叶delta/测试体与生产未变/SHA精确复用。八条真实LAN联合在input2实跑：三类动态共同更新后停止8.688秒、变化后撤权9.032秒、三类冻结/黑场/恢复/停止16.563秒，分别为三个独立完整Source会话。另两旧Live、两旧Art及原点击/确认/取消联合重验通过。7份cfg(test) Rust最大421有效行，生产协议/权限/预算/依赖和后端选择不变。strict Otter all-target Clippy128.422秒、Otter lib294.047秒、Loom lib58.875秒通过；格式/版本/Otter strict行数/两仓diff通过。Loom strict Clippy仍101旧签名/主位置/次数无新增，全仓strict行数仍被旧ArtEditDialog.tsx于2026-10-31到期例外阻塞，不延期/绕过，不称两仓全绿。普通Tile输出EXE可见链/最终交换链读回、物理多屏/跨机/热插拔/真实DPI、完整Art值控件/IME/键盘、本地Art触发、长稳、其余迁移退出条件、最终完整包和用户全面手测仍开放。自有离屏NOACTIVATE HWND actual present不是上述验收。Loom只有cfg(test)叶与include登记变化，没有新普通daemon包；集中证据见Otter WINDOWS_NATIVE_BUILD，三旧副线冻结。

## v506 Tile Art 点击与显式确认

Loom lan_http_wall仅精确开放原Wall Surface event/confirmation/cancel Device POST；原Wall binding/lease/nonce/命中/包完整性/审批及动作executor权威保持，不开放管理员或任意Surface路由，不新增依赖/权限/资源预算。runtime/wall_surface_routes只增加cfg(test)失败诊断。
新真实Loom LAN→原生管理器配对/独立自有离屏NOACTIVATE HWND真实WM_LBUTTONDOWN/UP→原executor/真实framework与Art进程联合23.390秒通过，沿原30秒父门：None不拒绝、待确认经真实识别/同view恢复后显式Reject不执行，Approve后权威/正式结果及实际paint文本101，慢执行后显式Cancel且晚结果不改变101，执行记录恰fs，view/控制/线程回收且保源附件。选择回调复用生产choose，不冒称实际MessageBox点击；不是普通可见双产品、最终交换链读回或物理扫描。
相关Rust完整名去重175 passed / 0终态failed / 1 ignored，不是整库；25组定向在input9实际通过后，input10仅两份Loom专属联合夹具/外置handler变化，按完整delta、测试体/生产未变及产物SHA复用原回执，不伪称新库整套重跑。11条Tile真实LAN联合和普通固定SHA EXE工作区5项均在input10实际运行。Loom库input10重建，未构建普通daemon整包；真实失败保留：generation=0为Loom合法初态，移除本批误加正数限制并补0→1旧动作退休测试，没有伪造初始1。input6/7/8慢动作失败先用test-only受理/派发证据排除未命中，input9服务原错明确首轮handler在安装包cwd写executions.txt导致锁定包完整性校验拒绝。input10仅把夹具TEMP/TMP以ENV_LOCK下RAII设到独占root、handler写该root，沿原受管环境白名单，不放宽包完整性/鉴权；独立实际handler101/102、fs和包cwd未变化oracle通过，原30秒门不延长。
strict Clippy实际重验仍101旧签名/主位置/次数无新增、过期行数例外仍阻塞。完整Art值控件/IME/键盘、本地Art触发、真实Loom图片/Live/Art三类动态共同联合、普通Tile输出EXE可见链/最终交换链读回、物理多屏/跨机/热插拔、资源长稳、其余迁移退出条件、最终整包和用户全面手测仍开放。输入/耗时/完整证据见Otter WINDOWS_NATIVE_BUILD，三旧副线/固定手测包不改。

Otter v505新增原生Tile物理输出识别；Loom lan_http_wall仅将既有POST /v1/walls/endpoints/identify/report加入精确Device/nonce LAN白名单，不开放管理员identify，不改Wall业务、TTL、租约/授权或预算。新增tile_identification_native_joint专属cfg(test)叶复用原管理器配对/真实LAN服务/独立原生输出；独立核未分配layout的applied=false→实际可见paint→applied=true/layout application=null、关闭及自然10秒过期后恢复、撤权marker/线程回收。
两新联合15.687/4.203秒及原八Tile联合通过，当前Loom相关18 Rust去重通过，Otter总相关166通过/1前台退出ignored；不是全库、普通daemon/Tile EXE可见双产品、最终交换链读回或物理多屏跨机。Loom lib50.031秒重编通过；strict daemon Clippy实际重跑仍101旧诊断且逐签名/位置/次数无新增，旧ArtEditDialog过期行数例外仍阻塞全仓strict门。Art输入能力仍空，三类动态共同联合、长稳和最终包开放；输入/失败/完整边界见Otter WINDOWS_NATIVE_BUILD，三旧副线/固定手测包不改。

Otter v504只改客户端Tile调度/准备发布围栏，Loom生产与测试库源码均未变。以同Grant独立第三Lease隔离惰性Art串行worker和原Live轮询，不增加服务权限/预算/协议；原八条Tile真实LAN与两Source/Viewer联合全部用新Otter child重验通过。Otter相关147 Rust通过/1前台门ignored；慢Art超过5秒、原Live连接持续Ping及三类完整画布恢复另由可控TLS/WSS夹具核验，不冒充真实Loom三类动态共同联合、最终交换链或物理跨端。Loom固定库与既有101旧Clippy诊断/过期行数例外按完整输入/SHA精确复用，不称重新执行全仓静态门。证据与未验边界见Otter WINDOWS_NATIVE_BUILD；Art输入/识别、普通可见双产品、长稳及最终包仍开放。

v503 Wall Art LAN接缝：lan_http_wall 只开放原终端所需精确 Surface open/state/image/close Device POST，沿原 Wall binding、呈现Lease、nonce、view归属和资源授权，不开放管理/任意事件/输入。Otter只消费声明式snapshot与哈希图片，不在终端执行JS；当前广告 image/raw_bgra/surface_v1/scheduledPresentation，输入能力空。新增 tile_surface_native_joint 专属cfg(test)叶复用原首次配对/真实LAN服务/独立原生输出，源内容由独立声明和像素oracle核验。
Art更新/冻结/黑场/恢复/停止17.203秒及撤权8.484秒联合通过，唯一view/图片更新/实际原生提交、view/像素/线程回收且源实例保留；原六Tile及Source/Viewer输入释放/H264共同回归通过。最终相关Loom34项Rust去重通过，全部联合对应Otter input6；Loom库按input5完整delta/SHA复用，不是普通daemon交付或跨机/最终交换链读回。测试曾误读原#[serde(skip)] ephemeral，已改核公开descriptor精确归属，保原失败和完整回收/保源断言，未改生产wire。
Otter最终696项相关Rust通过/7 ignored，不是整库；Client精确响应门及Transport暂态映射缺口经窄修重验。Loom strict Clippy仍101原诊断无新增，strict全仓行数仍因旧ArtEditDialog例外到期受阻。单张Art同步HTTP仍可能使同线程Live超过原期限，慢网隔离/三类动态共同联合、Art输入/确认/取消、识别、普通Tile输出EXE、多屏/跨机/热插拔、长稳及最终包未关闭；完整输入/失败/SHA/耗时见Otter WINDOWS_NATIVE_BUILD，旧副线不重开。

v502 Wall Live LAN接缝：lan_http_wall只新增精确GET /v1/walls/live/media的Device/nonce与严格query/profile准入，必填endpointId/leaseId/revision/sessionId/format，raw禁止缩放，PNG≤1280×720/10fps；LAN刻意要求单一loom.wall.media.v1，列表/重复或未知query/encoded alias/浏览器头均拒绝。lan_http_live仅共享query单次解码与升级头校验，不开放管理布局、资源上传、Surface/input。原WallLiveGrant、Device重放、permit、source新鲜度、编码后再授权/epoch围栏不变；Wall socket复用LiveMediaSocket/LanMediaStream泛型接缝，101前配置TLS媒体缓冲/期限，无新网络框架、权限或预算。
新增cfg(test) tile_live_native_joint复用原Wall真实LAN与首次管理器配对/续签，实际Source菜单发布后由独立原生Tile自行注册/租约/取图/媒体/心跳。混合raw/PNG首帧/实际raw新帧、冻结/黑场/恢复/停止14.453秒，撤权清理5.094秒通过，未加入Surface Viewer/未授controller，最后media permit归零；原四Tile联合和Source/Viewer输入释放/H264共同回归也通过。相关Loom32项Rust去重通过，源固定像素/原生present不是最终交换链读回/普通双产品或物理跨机。当前strict Clippy仍101原诊断无新增，旧ArtEditDialog行数例外到期仍阻塞全仓门；不新增普通daemon交付包。输入/测试夹具竞态修复与完整边界见Otter WINDOWS_NATIVE_BUILD，Art/输入/识别Tile、多屏同步、长稳和最终包继续开放。

v501只扩展cfg(test) tests/tile_native_joint，生产路由/Wall权威/鉴权/预算不变。复用原首次配对/真实LAN HTTPS与独立原生输出夹具，新增尺寸/位置改变、缺席/另一ID拒绝/原ID返回及恢复后撤权门：原生Output owner自行排空旧线程/像素并重建，服务核唯一endpoint、2×2→4×2登记/layout3、缺席offline、返回新租约且catalog不重复推进。两新联合6.078/9.016秒、两旧图片/撤权联合17.015/7.329秒均通过；当前Loom相关25项去重通过。测试只注入客户端枚举快照，不改物理显示或真实配置，源像素/实际原生提交不冒充最终交换链读回或物理扫描。
唯一Loom改动Rust为365有效行，strict Clippy仍101旧诊断无新增、strict行数旧ArtEditDialog例外到期仍阻塞；本批没有新普通daemon包。Otter全部相关产物因生产控件指针修正重新构建，首轮失败及输入身份见Otter WINDOWS_NATIVE_BUILD。恢复实现的隔离验收不关闭普通双产品/物理拔插/跨机/多屏同步、完整Tile/长稳/最终包；旧副线保持冻结。

v500 仅新增 cfg(test) tests/tile_native_joint 及 wall_http 的 include 登记，复用真实 daemon/LAN HTTPS、Device 签名配对和 Wall runtime，不改生产路由/鉴权/预算。Otter 管理器/独立输出测试进程自己完成首次配对、批准/续签、注册/租约、图片读取和真实原生提交；父 oracle 仅上传固定RGBA/写布局，独立核源PBGRA、scene activation/appliedRevision、冻结/黑场/恢复与停止/撤权后 offline/资源回收。两项联合17.016/7.750秒通过，不是普通daemon/Tile EXE可见双产品、多屏同步或物理跨机。
本批相关Loom23项去重通过，其中两项新真实联合、21项未变边界在input1通过后按完整输入/产物SHA复用。input1资源上传201被测试误写200的失败保留，input2只修/格式化新测试叶，重编Loom库并重验新联合；未修改生产合同/期限以刷绿。两份Rust最大379有效行；strict Clippy仍101旧诊断无新增，全仓strict行数旧ArtEditDialog例外到期仍阻塞。完整输入/回执与源像素/窗口提交/最终物理扫描边界见Otter WINDOWS_NATIVE_BUILD，完整Tile/长稳/最终包仍开放。

Otter v499 的原生静态图片 Tile 仅新增 lan_http_wall 与专属原始HTTP测试，lib/lan_http作最小登记。只允许GET /v1/walls/state及POST endpoints/register、connect、heartbeat、disconnect、images/read六个精确路由，Device会话与nonce必需，沿原512KiB请求门；不开放管理布局、上传、Surface/Live/输入或任意转发。原runtime继续独占授权、重放、CAS、呈现租约和资源归属，没有新网络/权限框架。
Loom本批54项相关库测试通过/1显式PNG比较基准ignored，含两新LAN边界门；普通Otter EXE工作区、客户端TLS/原生自有窗口链另在Otter验证，不拼成真实Loom静态图片联合或可见双产品验收。Loom库按input2固定SHA复用，未构建新的普通daemon包；strict Clippy仍101原诊断、全仓strict行数旧ArtEditDialog例外到期仍失败。完整Tile/双机/多屏同步/长稳/最终联合包继续开放。

v490媒体退休修复：runtime/live_session_media只允许已核Source/Viewer身份的false清理穿过closed tombstone，true仍active-only；保留活动controller过期规则，不在close时伪清真实连接计数。tests/live_media_connection_cleanup以真实RoleGuard/Permit保护双viewer/Source逐项释放、伪身份拒绝和禁止重开；原联合沿同一30秒总期限等待服务端role/permit归零。当前全仓行数门另因旧ArtEditDialog.tsx例外于2026-10-31到期而受阻，未延例外或调时；本批变更Rust均小于500有效行，不冒称完整仓库门禁通过。

v490新增本地Art宿主的显式原生Live控制合同：POST /v1/art/local/live-viewer/control，loom.art.local.live-control.v1，32KiB，Device/nonce-only，操作仅acquire/release/input。Preview不自动join或控制，仍需精确有效签名/binding、当前Art审批/工具/Device、已有Viewer成员与Source输入许可；原Live store独占controller排他、epoch、actor control/input双序号。
local_art/live_host共享attach/control授权，acquire/续租核immutable源码，input不逐键重复读JS；最终审批与store mutation在原policy锁内串行。release是纯减权，Art撤审批或过期后只允许有效Device/签名/精确身份释放自己已有controller，不允许再acquire/input；原Surface合同不改，不开放JS输入API。local_art/live_control和runtime/live_session_local_control仅适配原业务，不新增网络/输入框架。
新增协议/HTTP测试、17项Viewer Application以及真实Art HTTPS联合通过：原生acquire2/input3:1，撤Art审批后acquire/input拒绝，release4仍成功且保membership；此联合明确mediaTested:false、sourceExecutionTested:false。原Surface→实际Source输入worker/释放联合另作回归通过，不能拼成普通双产品或物理跨机验收。相关122项Rust通过（当前组合80项实跑、42项input2未变原生夹具按完整输入/产物SHA复用）、10项显式门未运行，protocol local_art17项在input2通过后字节未变；strict Clippy仍101条daemon/10条protocol既有诊断无新增。集中身份/输入/运行证据见Otter WINDOWS_NATIVE_BUILD，完整迁移/资源长稳/最终包继续开放。

v489仅为原生Live viewer的合法Surface宿主新增精确LAN传输接缝：lan_http_surface允许POST attach/实例lifecycle及GET设备stream，拒绝别名/重复或未知参数/管理/任意事件/mount/资源；要求Device会话和nonce，Surface JSON请求16KiB。原runtime仍核当前设备、nonce重放、附件归属与生命周期，原LAN Host/浏览器/管理凭据/GET body边界不变，无新网络或输入框架。
live_viewer_source_native_joint不再直接写controller/input：真正配对的两Device、真实Surface公共attach/active、原生Viewer菜单与HTTPS control/input、Source原worker、自有离屏WndProc及原WSS媒体贯通，3.886秒通过。独立核80字节NLLV/2x2固定BGRA，两轮A/B+Mouse释放均发生于Source仍发布时，观看保留；原Surface Mounted1/Active2/Inactive3、actor9/input4和最终连接/owner回收均核验。
本批21项Rust实跑通过（Loom18/Otter3），另344项未变模块/工作区结果按完整delta及产物SHA复用，去重365 passed/0 failed/24 ignored，不是整库重跑。input4 Loom库75.555秒/普通daemon40.498秒，input5只修生命周期oracle后库27.654秒；普通daemon version/help通过，生产未变复用。严格Clippy input5仍101条旧诊断无新增；两仓格式/严格行数及Otter严格Clippy通过，集中证据见Otter WINDOWS_NATIVE_BUILD的本批收口。
input2 profile夹具错误、input3 LAN缺口和input4 Inactive版本oracle失败均保留，未增加超时或放宽授权/资源门。本地Art控制proof、观察/H264/恢复、普通可见双产品/物理跨机、长稳及最终联合包仍开放；本次是生产库服务和lib-test Application的隔离联合，不是普通daemon双产品或跨端业务验收。以下旧缺口只保留历史边界。

v487仅扩展cfg(test) live_source_native_joint，Loom生产路由/授权/预算不变。保留只读联合，并新增原生Source交互联合：控制侧直接使用原权威store构造membership/controller/input，Source仍经真正配对Device/LAN HTTPS events→原Application单槽桥→原Session唯一窗口消息worker，自有离屏NOACTIVATE WndProc实收A键与鼠标按下/释放。媒体沿原WSS worker独立核80字节NLLV/2x2 BGRA精确[17,83,211,255]；停止后closed、source_connected=false、media_connections=0，输入owner回收且保留本地采集。
两项父子联合1.055/1.404秒通过，相关Rust341项完整名去重通过（Otter库334/Loom2/普通Otter工作区5）、0失败、24 ignored；未变测试及产物按完整delta/SHA复用，不称全部在新测试库重跑。Loom测试库129.799秒构建通过，严格Clippy33.313秒仍101条旧诊断无新增，不称全仓静态通过，普通daemon不重复构建。Otter严格Clippy/所需构建通过，两仓职责文档与严格行数等收口见main-live-source-input-1/final-audit.json。
控制侧store夹具不是viewer公共HTTP/原生控制UI验收，合成WARP捕获不是真桌面WGC、普通可见双产品或物理跨端；结束时未在前台也不等持续焦点监测。当前Source键码仅vk:NN，Web键码互通、原生viewer控制权/输入UI、观察/trigger、H264 Source、重连/epoch恢复、完整LiveRelay/Tile、长稳和最终联合包仍开放。以下v486无输入描述保留历史边界，不覆盖当前实现。

v486仅新增cfg(test) live_source_native_joint与local_art_http的一处登记，不改Loom生产路由/鉴权/预算。联合父门驱动原生Otter Application系统菜单，通过真正配对Device/LAN HTTPS创建Source及原WSS媒体worker，独立核80字节NLLV/2x2 BGRA精确[17,83,211,255]和无输入能力；原菜单停止后closed、source_connected=false、media_connections=0。父子联合1.760秒通过，输入为合成WARP纹理，不是用户桌面WGC、普通daemon双产品、物理跨端或远程输入/恢复验收。
本批相关Rust270项去重通过（Otter库264/Loom1/普通Otter工作区5）、0失败、9 ignored；两仓strict行数、严格Otter Clippy及所需构建通过，Loom测试库重编65.469秒，生产daemon不重复构建。Loom strict Clippy仍101条既有诊断且签名/次数无新增，不称全仓静态通过。集中证据main-live-source-1/final-audit.json；远程控制、H264 Source、重连/epoch恢复、可见双端/物理跨机、完整LiveRelay/Tile和最终联合包仍开放。

lan_http_live独占既有Live HTTPS精确路由/参数与Device-only升级白名单，拒绝query deviceId、别名、重复/未知字段、管理/浏览器凭据及GET body；lan_http的原Host/请求预算保持。已验LAN TLS stream经runtime/live_media_transport的私有trait移交原媒体grant/nonce/会员/permit/worker与视频协商循环，不作透明代理，不新建媒体框架/整帧缓存；升级后释放四个短HTTP worker。lan_listener通过OnceLock/Weak绑定runtime，原stop传入媒体IO，不在registry锁内join退休listener。session路径复用原decoder单次解码，编码斜杠保同一身份。
v485真实LAN HTTPS/WSS与原生Otter联合2.353秒通过：配对/目录/加入/控制/原生精确GPU帧、source+五viewer后HTTP可用、nonce重放409、Device禁用Close1008、LAN停用/重启后角色/permit归0及禁用viewer仍401；专属lan_http_live_tests与live_lan_native_joint保护入口/资源边界。该联合使用隔离loopback LAN listener和测试publisher，不是普通daemon双产品、Windows真实source捕获/控制、物理跨机器或H264 LAN完整矩阵。Otter Device endpoint已修为header-only，未为测试放宽Loom边界。
本批相关Rust405项去重通过（Loom166/Otter库234/普通Otter工作区5）、16 ignored；两仓strict行数与所需构建通过，Loom strict Clippy仍101条既有诊断无新增签名，不称全仓静态通过。Loom input3后源码未变，后续Otter生产query修复及旧测试peer修复分别重建/重验并按manifest/SHA复用未变成绩；失败保留。集中证据main-live-lan-1/STATUS.md及final-audit.json；普通source产品、可见跨端/长稳及最终联合包仍开放。

projection_edit_application_joint/oracle在cfg(test)复用双真实Loom服务、pinned HTTPS及既有有界Otter子进程owner，驱动两原生Application消费者走原菜单/原调度/默认Checkpoint/后台ack；独立核初始化、two_way双端自动修改、双端保存重开、one_way本地冲突/显式采用远端重开及停止保图。
权威revision/模式/对象坐标以及32x32纯蓝basis、三份红框/蓝底重开与终态PNG使用独立wire/像素oracle，不借Otter新merge或renderer生成expected。v482联合17.469秒通过，本批相关29项Rust去重通过（Loom14/Otter库10/普通Otter工作区5），0失败；1前台退出门未运行，Loom严格Clippy仍101条原诊断且无新增签名。
本批只增测试/最小登记，不改Loom生产路由/鉴权/预算；peer仍signed loopback HTTP，两个服务不是普通daemon业务EXE，Application消费者不是普通双产品可见验收。自动服务预算阈值未在该联合实跑，物理跨机/全链TLS/长稳/最终联合包仍开放；两次Otter夹具同步/外部焦点变化失败及输入身份分别保留，证据main-projection-edit-application-joint-1。

projection_edit_native_joint复用既有双真实Loom/peer及HTTPS夹具，启动固定SHA的原生Otter事务测试子进程；用独立固定文档oracle核六操作、原请求重放、shared/offline合并、checkpoint/墓碑与冲突保留。
lan_http的两条既有edit路由统一遵守原v1合同256KiB，不扩展路由/鉴权/PNG或peer预算；对应边界测试仍拒绝别名、管理凭据及超限输入。
此门不把测试显式ack当成Application保存或可见协作产品完成；Otter ingress为真实HTTPS，peer仍signed loopback HTTP，跨机器/全链TLS另验。
v471父门4.875秒/原生child0.75秒通过；shared无凭据401与offline独立403、固定源2x2及原始RGBA像素分别精确核验，不以接受任意状态/尺寸刷绿。相关Rust227项去重通过，严格Clippy仍101条旧诊断无新增签名。

projection_offline_product_joint复用原双真实Loom夹具，以单个有界原生集成测试子进程驱动两个固定SHA的普通Otter产品；从授权receiver独立核条带像素/尺寸/digest及持久停止。
普通菜单reset/flip、同profile重开和正常退出由Otter既有UIA测试owner核验；v470完整普通双Otter闭环通过，三份receiver PNG独立核全像素/尺寸/digest，停止后read410且终态重开保图。
本批相关12项去重通过，Job与Art按未变输入复用；不新增生产协议/路由，peer仍signed loopback HTTP，不将生产库服务冒充普通daemon业务EXE、全链TLS或物理跨机/最终包验收。严格Clippy仍101条既有诊断无新增。
projection_offline_product_process仅拥有Windows测试子进程的Job；登记/握手先于产品spawn，父失败也回收整棵自有树；专属测试核关闭只影响assigned child，不结束其他进程。

lan_http显式放行既有offline device和signed peer静态端点，仍拒绝管理路由/别名、Bearer/cookie/origin及任意转发；沿原PNG、peer challenge和响应预算，认证仍由原device/pinned proof owner执行。
projection_offline_native_joint/fixture只拥有两个真实Loom及独立Otter测试子进程，以源分辨率PNG独立oracle核首帧、保存后实际帧回执、持续更新、同profile重开、停止/peer撤销。
Otter至各Loom为真实HTTPS，peer互传复用协议允许的signed loopback HTTP；不改真实配置，不等于全链TLS、普通EXE可见/物理跨机器或正式联合发布验收。
projection_offline_joint_proof仅拥有隔离身份的独立wire oracle，四种peer HTTPS成功/响应验签、篡改/重放拒绝和offer幂等已通过；不公开私钥，不复用生产message构造。
两条双Loom原生联合门已验证保存呈现、更新、重开与停止/撤销保图，五份PNG独立复核，相关Loom41/Otter3项去重通过。source read仍metadata-only；普通daemon只另验构建/version/help，严格Clippy仍101条旧诊断无新增，不称全仓或跨端验收。

projection_native_joint是显式双仓HTTPS测试owner，复用projection_http的真实设备/服务夹具，
从Otter原菜单创建并持续更新到独立接收身份读取PNG，再核停止与checkpoint恢复；不修改生产路由。
ProjectionUpdate保持必需digest及deny_unknown_fields；Otter补齐该字段，缺字段仍以400拒绝。
真实联合父子门已显式通过：独立接收身份读取revision2的16x8绿色PNG，逐像素/metadata/digest一致，
knownRevision省略snapshot，停止并checkpoint恢复后read为410；缺digest仍400拒绝。
本批相关Otter132/Loom31项去重通过，另普通Otter产品两个worker通过；生产未变的Loom lib/普通产品按输入哈希复用。
Loom严格Clippy仍101条既有诊断、无新增签名；原夹具路径失败保留，不把隐藏Application视为可见接收端、跨设备或最终联合发布验收。

敏感本地Art服务新增独立confirmation协议；service_confirmation只持原Task的Gate与精确CommandScope，
确认等候、运行、图片结果编码共用原绝对期限和容量。host/confirmation解析登记并核scope，旧gesture也绑定scope防换代复用。
可信用户确认由Otter原生宿主完成，脚本只请求询问、不接收token；缺宿主、拒绝、取消、到期、撤权/换代不执行或发布迟到结果。
旧service/images/image-output、设备认证、审批与资源owner保持。定向及五条真实双仓HTTPS联合已通过，
最新组合runtime53/protocol12/daemon38、Otter57项去重通过，两仓普通产品构建及Otter三个既有worker通过。
Loom严格Clippy仍有daemon101/protocol10旧诊断且签名/次数未增；runtime另4条来自前态字节未改文件，未冒称全仓通过。
证据main-art-service-confirmation-1；程序化测试宿主批准不是普通EXE可见用户确认或最终联合包验收。

v460 local_art/service_image_output实现显式单PNG结果适配，不嗅探路径/URL、不应用effects；
旧service.v1输出64KiB不扩容，image-output请求和响应使用独立envelope，原任务幂等键包含输出模式。
service_tasks持有8MiB编码总预算的终态图片租约；消费后沿原cancel释放，撤权/过期/shutdown同样释放且迟到完成不复活。
Otter私有二进制安装/Canvas/正式结果和保存仍归Otter；本批protocol11/daemon43、Otter294项去重定向通过，
包含四条真实HTTPS联合和新SDK示例隐藏Application正式结果/Save恢复；两仓产品构建及Otter三项既有worker通过。
Otter严格Clippy通过，Loom仍101条既有诊断且无新增签名；非可见产品、真实跨端或最终包声明。

v459 Canvas服务输入由Otter在本地调用时冻结并按原codec验证，Loom仍只接既有service.images.v1。
local_art_joint新增显式Canvas模式，沿原Device Session/批准/签名Capability进程核原尺寸像素与撤审批；三种输入HTTPS联合门均已通过。
本批protocol10/daemon38项去重通过；daemon严格Clippy仍有既有101诊断且无新增签名，不声称全仓静态通过。
没有为Canvas创建另一资源/权限/服务协议，图片输出和敏感gesture仍另列未完。

v458图像服务输入由local_art/service_images拥有调用期Surface upload与Capability broker租约，
复用runtime/capability_extension_resources的权限/解码/注册实现；不开放通用Device资源管理或执行入口。
独立service.images.v1请求只扩精确submit-images，控制/JSON结果/任务表/监督和清理仍沿原service owner；
图片指纹进入幂等键，成功/失败/取消/撤权后不保留图片到终态表。实际codec归服务，图片输出和gesture不在本合同。
本批protocol10/daemon37项去重定向及原JSON/新图片实际双仓HTTPS联合通过，覆盖真实签名插件和撤审批；
两仓产品构建及Otter实际产品Canvas/状态worker通过，不代替可见产品或最终包。
严格Clippy仍失败：daemon101/protocol10既有诊断，未观察到新增；本批触及http_request/route_dispatch两个旧诊断文件，原诊断表达式/签名未改。

`sdk/otter-art` 是独立 `otter.art.v1` 作者入口：native-art/native-dom/native-canvas 声明当前已核核心能力，
examples/packaged-image-service.js 直接作为实际分发源码，tests/types.ts 在无 DOM/Node 环境保护正反类型合同。
旧 sdk/surface 不改义；类型不授予权限，也不把服务 JSON 回包广告成图片输出或敏感 gesture 合同。
同份示例的签名包/隔离TLS/隐藏Application/正式结果/Save验收归 Otter；可见产品、真实跨端与最终包仍分别验收。
v457同份示例已显式验证200/503及停服后有限离线新Application、独立像素/42值与保存恢复；
Otter82项去重相关运行和严格Clippy/lib构建通过，公开SDK的TypeScript5.9.3无DOM正反例通过。Loom生产Rust及依赖未改。

v454包资源扩展由local_art/package.rs拥有独立证明和受认证资源分发，source_resources.rs拥有包内路径和实际字节验证；
共享原immutable resolver、单解析槽、当前设备/审批与issuer，不增加包管理器或运行设施。Otter继续拥有本地执行和缓存。
本批Loom40项相关passed/5默认ignored，三项HTTPS联合入口另显式通过；新完整资源包在首个Otter退出及Loom停服后，
由新Otter进程验证缺块拒绝及完整缓存的正式42/2x2像素结果，无Device Session。实际双仓产品构建与Otter Canvas worker通过。
严格Clippy仍有未改daemon/protocol文件旧诊断，本批文件无新诊断；见protocol/LOCAL_ART_EXECUTION.md，不代表可见UI、跨端或最终联合发布。

本地Art的local_art/cached.rs仅拥有显式prepare.v2请求及响应字节省略；原prepare持有设备/审批/
immutable resolver/源码验证/签发完整事务。命中knownSourceDigest才返回source:null，否则返回完整Base64。
旧prepare.v1和签名grant.v1均不改义；客户端源码缓存不等于离线许可。本批15项本地Art HTTP、3项LAN边界
和1项真实HTTPS联合测试通过；daemon严格Clippy仍有未改文件46/55项旧诊断，不是全仓或最终包验收。

Color-transfer 样例的 `runtime/shader_payload.py` 拥有显式 Shader ABI 输出：宿主请求
`shader_abi=otter.hlsl.v1` 时返回 `type/abi/source/uniforms/textures`，原生源码为
`color_transfer.hlsl`；未知 ABI 拒绝，不静默降级。结构化 `params` 优先；Hook 平铺执行把未声明的
`shader_abi` 分入 `inputs` 时读取该值，与已兼容的 `output_mode` 分流一致。未指定 ABI 的旧浏览器合同仍返回 GLSL。
`scripts/tests/test_color_transfer_shader.py` 通过真实进程入口验证两种请求形状及拒绝分支；原生编译、显示与正式 PNG 归 Otter。

本地 Art 的 `local_art/describe.rs` 提供独立、设备审批约束的精确包描述；复用原 immutable resolver、manifest/源码准入和
prepare 的解析容量槽。描述不授予执行权、不输出内部包路径；prepare 仍完整复核，旧 capability metadata 不自动升级为本地执行授权。

Loom is an independent Rust workspace and public repository. It is designed as
a headless runtime first: reusable library crates define behavior, while the
daemon, CLI, and desktop remain application shells.

## Boundaries

- Neuro Gateway owns provider routing, credentials, relay APIs, browser-worker
  assets, and model/provider implementation details.
- Neuro Platform owns public website, account, identity, quota, entitlement, and
  operator service policy.
- Otter owns foreground capture and desktop integration behavior.
- This Loom repository owns Art/Framework/Capability/MCP package orchestration,
  Otter workflow synchronization and projection, durable capability run evidence,
  and global AI Gateway relay client calls.

The desktop is a thin Tauri/React shell over the Loom daemon; it does not copy
the ArtLoom monolithic desktop-local backend.

The daemon owns `control-plane/settings/settings.json`. Its persisted `general`
and `loom_cache` fields use snake_case. Desktop startup reads those canonical
fields explicitly before the WebView is ready; camelCase belongs to the Tauri
IPC DTOs, not the on-disk document. Startup readers do not accept IPC-name aliases,
and keep the existing bounded regular-file and integer-range checks.

### Historical migration references

Loom adapted Rust runtime patterns from NeuroLoom and workflow concepts from
ArtLoom into this independent workspace. The completed source mapping, original
reference paths, and parity history are preserved in Git at
`ade6992:docs/MIGRATION_MAP.md`; the original planning matrix is at
`cleanup-base-20260928:docs/analysis/loom-source-migration-matrix.md`.
Use the boundaries below for current behavior. Changes follow
[the development manual](DEVELOPMENT.md), and packaged acceptance follows
[release provenance](release-provenance.md); old migration smoke results do not
validate a new binary.

## Crates

- `loom_core`: shared IDs, errors, result type, and run/session status contracts.
- `loom_durable`: synchronous HTTP `RunEvidenceStore` boundary and bundled SQLite
  persistence.
- `loom_workflow_store` and `loom_workflow_runtime`: persisted Otter/Art workflow
  YAML graphs, topological validation, and bounded Art DAG execution.
- `loom_protocol`: language-neutral framework/Art manifest, process+JSON ABI,
  lockfile, permission, diagnostics, and authoring contracts embedded from the
  public v1 Schemas.
- `loom_plugin_security`: canonical package digests, Ed25519 signatures,
  publisher trust, and revocation policy.
  `local_art_grant` 对已明确授权的独立 Art 执行载荷签名，不复用包签名作为执行许可；
  wire/范围校验由 `loom_protocol::local_art` 拥有。daemon `local_art` 拥有认证设备的显式审批、精确包源码解析及签发路由，`offline_peers/local_art_issuer` 仅桥接持久签发身份；客户端可信 pin、缓存与 UI 自动挂载尚未接入，见 `protocol/LOCAL_ART_EXECUTION.md`。
- `loom_process`: bounded child-process execution, output capture, timeout,
  process-tree termination, and resource diagnostics.
- `loom_capability_runtime`: Capability Plugin 的签名准入、进程调用与结果校验。
  `InvocationControl` 是单次调用 owner，支持登记前取消和最多 4 KiB 的最新真实进度槽；
  `process/call` 持有取消唤醒、请求身份隔离与进程回收，进度不进入最终响应队列。
  取消不计入崩溃退避，终态后不可再取消；强制取消会终止所拥有的 runtime，
  使用同一进程的并发调用也可能失败（默认每插件单调用）。初始化阶段仍受原初始化期限约束。
  此设施本身不授予 Art 调用权。daemon `local_art/service` 负责专属路由与准入，
  `service_lease` 负责当前授权复核，`service_tasks` 负责实例级有界任务/防重放/退出，
  `service_execution` 复用 runtime 并隔离工作区 effects。Otter 异步 SDK 仍待接入。
- `loom_tool_registry`: publisher-qualified framework/Art registry, secure ZIP
  install, immutable versions, dependency/runtime locks, rollback, lifecycle
  recovery, credentials, and host-mediated network policy.
- `loom_gateway`: bounded OpenAI-compatible client boundary for calling an
  external AI Gateway across loopback, private LAN (RFC1918/ULA), or remote HTTPS.

## Applications

- `loom-daemon`: headless runtime host with local health/status, package, Otter,
  and AI Gateway relay APIs.
- `loom`: HTTP CLI client with `status`, `health`, `frameworks list`, `arts list`,
  `workflows list`, `mcp list`, and `ai status`.
- `loom-plugin`: independent Plugin SDK CLI for scaffolding, signing,
  validating, packing, conformance testing, and trust-store administration.
- `apps/desktop`: Tauri desktop shell published as the single user-facing
  `Loom.exe` entry in Windows desktop packages.

### Windows package boundary

The desktop shell and daemon remain separate processes, but the release layout
keeps the daemon and its owned support files under one internal runtime tree:

```text
Loom.exe
runtime/
  loom-daemon.exe
packages/
  frameworks/*.zip
  arts/*.zip
capabilities/              # optional, separately downloaded packages
  neuro.official-ocr.capability.zip
```

The `capabilities/` lines describe a separately hosted artifact set, not files
inside the default Loom desktop ZIP. The release verifier rejects OCR models and
other optional plugin payloads from the default runtime.

`Loom.exe` resolves its daemon in this order: an explicit
`LOOM_DAEMON_EXECUTABLE`, `runtime/loom-daemon.exe` beside the packaged shell,
then the development `target/debug/loom-daemon.exe` fallback. The daemon's
executable-relative resource discovery therefore remains unchanged in the
packaged layout.

Framework, Art, and Capability Plugin ZIPs stay outside both executables. OCR
models and inference binaries are not part of the default Loom runtime tree;
the signed OCR capability package is installed explicitly through the capability
catalog. The desktop validates the
packaged catalogs and applies the six repo-owned sample Arts once per catalog
hash through the public framework and Art install APIs. The migration marker is
stored in the writable control plane, not in the release directory. It records
completion of the current catalog bootstrap; it does not translate an obsolete
package layout. This gives
the registry useful initial cards without introducing framework or Art-specific
branches into Loom execution.

The CLI is intentionally a separate release artifact named
`Loom-CLI-<versionId>-windows-x64.zip`. That ZIP contains only `loom.exe`; it
is not placed beside `Loom.exe` in the desktop package. This separates the
normal desktop entry from scripting and headless operator tooling without
merging the application processes.

## Durable runtime model

Loom stores HTTP capability run and event evidence through
`loom_durable::RunEvidenceStore`, which provides both in-memory and bundled-SQLite
implementations:

1. Library constructors such as `DaemonConfig::localhost` use the in-memory
   run store by default. The real `loom-daemon.exe` explicitly selects SQLite.
2. The SQLite run store defaults to
   `<LOOM_CONTROL_PLANE_ROOT>\runs\loom-runs.sqlite3`; `LOOM_RUN_STORE_PATH`
   overrides the file. It enables foreign keys, WAL journaling, full synchronous
   writes, and a five-second busy timeout. Schema version 1 stores canonical run
   JSON separately from ordered event field payloads.
3. Run insertion and every run/event transition commit in one transaction. Store
   startup performs `PRAGMA quick_check`, validates all run and event JSON, and
   rejects unsupported future schemas. Existing `running` records are changed to
   `failed` with `daemon_restarted` and receive `run_interrupted` in one recovery
   transaction. Interrupted model or tool calls are never replayed automatically.

## Workflow runtime model

`loom_workflow_store` and `loom_workflow_runtime` persist and execute Otter/Art
workflow graphs (`loom.workflow.v1` YAML and JSON representations) with topological
validation, port wiring, cancellation, and durable execution diagnostics.

## Plugin runtime model

Frameworks and Arts are control-plane packages, not code linked into Loom's
default binaries. The four repo-owned framework IDs are `process`, `cloud_api`,
`mcp`, and `workflow`. Command, script, and Python Arts all use `process` and
select their package-local entry through `art.runtime.json`. A third-party
package uses the same public `loom.framework.v1` process+JSON ABI and canonical
`publisher/id` identity as the repo-owned packages.

The package roots are:

```text
<control-plane>/
  frameworks/<publisher>/<id>/
    versions/<version>-<digest-prefix>/
    active.json
    locks/
    lifecycle.json
  arts/<publisher>/<id>/
    versions/<version>-<digest-prefix>/
    active.json
    locks/
    state/
    cache/
    outputs/
  capabilities/
    registry.json
    grants.json
    config/
    .lifecycle/
    <publisher>/<id>/versions/<version>-<digest-prefix>/
```

Install and upgrade validate a securely extracted staging tree, signature and
trust policy, dependency resolution, self-test, and canonical digest before an
immutable version becomes active. Activation is journaled. Rollback repeats
identity, signature/trust/revocation, digest, lockfile, and runtime checks.
Uninstall first atomically renames the live package to a same-parent tombstone,
then commits registry removal; startup restores or deletes interrupted
tombstones according to the durable registry state.

The official Capability catalog is a detached signed JSON document. Its entries
pin the package URL, byte length, SHA-256, package signing key, SBOM, provenance,
host API requirements, permissions, and installed disk estimate. The daemon
fetches it through the bounded outbound client and accepts only the
`neuro.official` catalog publisher. Catalog install re-fetches and revalidates
all evidence before secure extraction. The Desktop OCR quick card calls this
generic catalog path; it is presentation, not an OCR-specific daemon endpoint.

Otter does not load plugin code and does not contain per-plugin branches. It
consumes generic capability metadata from Loom. HTTP and `loom.hook.v1`
executions all run inside Loom and create durable run
evidence. Package-backed Arts are revalidated immediately before every launch,
so an active pointer, payload, lockfile, signature, or revocation change cannot
bypass the normal HTTP execution path.

The source immutability invariant is explicit: framework/Art install,
execution, upgrade, rollback, disable, uninstall, and crash recovery may write
only to control-plane package/state roots and evidence stores. They never edit
the Loom or Otter repository/source tree. Plugin authors therefore require no
source access to either product.

Permissions combine enforced and declared boundaries. Loom enforces package
containment, immutable code versus writable state, bounded process trees,
timeout/output limits, scoped credential brokering, and
host-mediated HTTP policy. Direct arbitrary process network/filesystem access,
GPU, and clipboard are not fully OS-denied by current Job Object/PID-namespace
isolation. Windows Job Objects also enforce memory and active-process limits;
Linux PID namespaces expose those declarations as advisory. Linux managed
launches require Bubblewrap and pidfds; unsupported isolation fails closed, as
documented in [Linux process containment](LINUX_PROCESS_ISOLATION.md).
`LOOM_PLUGIN_PERMISSION_MODE=audit` is the current default;
`strict` fails closed for those unenforceable declarations. The doctor API and
Desktop expose the same enforcement matrix rather than claiming a complete OS
sandbox.

Release builds generate the Plugin SDK, CycloneDX and SPDX SBOMs, and build
provenance as independently checksummed evidence. The public Release exposes the
desktop ZIP and the Plugin SDK ZIP, each with its checksum sidecar; the CLI and
remaining evidence stay in the workflow/package verification boundary. Formal
release workflows require a clean source tree and request GitHub build/SBOM
attestations.

## Integration model

`loom_gateway` is intentionally a client boundary. Loom forwards model work to
an external AI Gateway and does not duplicate provider routing, credential
selection, browser-worker execution, or quota logic.

Generic plugin processes use `loom_process` controls and the permission mode
described above; this is not equivalent to a complete AppContainer or Linux
namespace sandbox.

### Daemon request execution model

`LoomDaemon` retains sole ownership of the `TcpListener`. Workers receive
already parsed `RequestJob` values and share an `Arc<DaemonRuntime>`; the
listener is not shared with route workers.

The production `loom-daemon.exe` uses a bounded request executor with four
workers and a queue capacity of thirty-two by default. `LOOM_DAEMON_WORKERS`
may be set from `1` through `32`, and `LOOM_DAEMON_QUEUE_CAPACITY` from `1`
through `1024`; empty values use the defaults. Invalid non-empty values fail
before the listener is bound. The safe `/status` field is:

```json
{
  "requestExecutor": {
    "mode": "bounded_workers",
    "workers": 4,
    "queueCapacity": 32
  }
}
```

`DaemonConfig::localhost(...)` and other library constructors remain `inline`
by default (`workers: 1`, `queueCapacity: 0`). The production binary selects
the bounded configuration explicitly, keeping embedded callers and library
tests stable while packaged operation has bounded resource usage.

The listener still performs the existing request parsing, header/body size
checks, and read timeout handling on the accept thread. Parsed jobs are submitted
with non-blocking `try_send` and dispatched according to a conservative route policy.
Concurrent-safe routes include `/health`, `/status`, `/v1/capabilities`,
`/v1/ai/status`, `/v1/ai/models`, `/v1/ai/chat/completions`, asset-library
downloads and download control, run reads/events, run stop/retry, invocation
cancellation, and `/v1/invoke`. `/health` and `/status` are reserved probes and
bypass the normal queue so they remain available during worker pressure. Surface
stream and Live event long-polls execute on the dedicated stream worker pool. All
other file-backed control-plane routes use one serialized route lock while still
running on a worker.

The queue capacity counts jobs waiting in addition to active worker jobs.
Queue saturation returns HTTP 503 with `daemon_busy` and `retryable: true`
before route execution; the accept loop does not wait for a worker slot. It
therefore creates no run or event. If shutdown arrives while an already
accepted connection is still being read, the executor sender closes before
submission and the request receives `daemon_shutting_down` with the same
retryable 503 shape. Worker panics and route errors are contained per request;
they do not terminate the accept loop.

The packaged Windows binary translates Ctrl+C, Ctrl+Break, and console close,
logoff, or shutdown events into the existing shutdown channel. The serve loop
then stops accepting connections, closes the sender, drains accepted and queued
jobs, and joins all request workers.

### Global AI Gateway relay

`loom-daemon` owns the centralized AI Gateway relay boundary in
`apps/daemon/src/ai_gateway.rs` and `apps/daemon/src/runtime/ai_gateway_routes.rs`:

1. Configuration can be supplied by `LOOM_GATEWAY_*` environment variables or
   persisted dynamically via `PUT /v1/ai/config` (`settings/ai-gateway.json` plus
   CredentialStore token isolation).
2. `loom_gateway` validates origin security (loopback HTTP/HTTPS, private LAN
   RFC1918/ULA HTTP/HTTPS with proxy bypass, or remote HTTPS) and forwards
   requests to `/v1/models` and `/v1/chat/completions`.
3. Direct relay routes (`/v1/ai/*`) and durable capability invocation
   (`POST /v1/invoke` with `ai.chat.v1` and `loom.runtime.probe.v1`) run on
   concurrent workers without blocking `/health` or `/status`.

## Loom and Otter Art protocol

Loom and Otter communicate through `loom.hook.v1`, with `loom.surface.v1` for
distributed interactive surfaces. Loom is the execution and resource authority;
Otter renders capability-declared nodes and sends typed actions. The public Rust
types and JSON Schema are in `crates/loom_protocol/src/hook.rs` and
`protocol/schemas/otter-message.v1.schema.json`. There is no alternate ArtLoom
workflow or execution adapter.

The official OCR Capability Plugin's versioned attachment details, including the optional source-tagged estimated baseline,
CTC-timestep-aligned character/word spans, and raw-text correction evidence, are
documented in [`OCR_CONTRACT.md`](OCR_CONTRACT.md). The detector remains
line-based; span geometry comes from real recognition timesteps projected into
that line quad and is not presented as independent pixel-level glyph detection.


### 跨仓实际服务联合门（input-2 定向通过，非完整产品验收）

- Otter art_service_joint_tests.rs 仅在显式环境指定的隔离Loom夹具中运行，使用真实设备挑战/签名续签、原Prepared下载/验签和真实受限child；不模拟服务回复、不写用户配置。
- Loom local_art_joint.rs 持有独立daemon、签名插件和精确包审批，协调Otter测试进程完成调用/null及服务端撤权拒绝；有界等待、仅回收自建进程，失败保留夹具证据。
