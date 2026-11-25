# Neuro 组件库接入

Loom 的 React 表现层使用独立 `NeuroDesign` 仓库中的 `@neuro/ui`，不复制标准组件源码。Rust、daemon、IPC、业务 API 和任务生命周期不属于组件库。

## 本地依赖与构建

设计系统位于 `C:\Users\Public\nas_home\NeuroDesign`，应用 `apps/desktop/package.json` 使用 `file:../../../../../../NeuroDesign/packages/react/neuro-ui`。新机器及 CI 应同时检出设计系统，保持相对目录关系，先安装并构建库，再安装应用依赖。

在设计系统安装开发依赖后，首次接入或检出路径变化时，先在 Loom 的 `apps/desktop` 执行：

```powershell
npx --yes npm@11 install --install-links --ignore-scripts --prefer-offline --no-audit --no-fund @neuro/ui@file:../../../../../../NeuroDesign/packages/react/neuro-ui
```

核对 manifest、lockfile 的根依赖与本地包 `resolved` 均为新路径，且外部包锁定版本未变化。`.npmrc` 保持 `install-links=true`；仅路径改变时元数据可能不变，不能只靠 `ui:sync` 更新安装地址。随后日常同步执行：

```powershell
npx --yes npm@11 run ui:sync
npm run typecheck
npm test
```

`ui:sync` 显式选择 `--package @neuro/ui`，只构建该 React 主题库，不构建其他主题、预览或原生组件。同步比较全部库产物和元数据，按需刷新已安装的本地包，并校验 React / React DOM 唯一性。普通 npm 安装可能保留同版本的旧 `file:` 产物，不能代替同步。不要在同一应用并行执行其他 npm 安装、测试或构建。应用成品不依赖设计系统源目录；更新库后须重新构建应用。

### Clean CI checkouts

The four desktop UI jobs (`ci.yml/windows`, `build-windows.yml/build`,
`release-tag.yml/release`, and `compile-windows.yml/compile`) check out Loom at
`AI/GameEditor/Neuro/Loom` and public `aiaimimi0920/NeuroDesign` at `NeuroDesign`,
pinned to `02420e8d129cf952c6bee3cda96ab7ebc95c2d67`. Both checkouts disable
credential persistence. The application manifest and lockfile remain unchanged.

With Node.js `22.22.2` and npm `11.21.1`, each job runs `npm ci --no-audit --no-fund`
and `node scripts/build.mjs --library --theme neuro` in the design repository before
installing Loom. Shell defaults and Action cache, upload, attestation, and publication
paths account for the nested checkout. Non-UI jobs retain independent root checkouts.
`scripts/tests/neuro-ui-ci-layout.test.cjs` guards the pin, actual `file:` resolution,
build order, and Action paths through `Test-GitHubActionsContract.ps1`.

`Compile Windows` builds the daemon, CLI, plugin CLI, and desktop executables with
locked dependencies, without bundling or uploading executables. It retains only a
commit-bound compilation receipt. This does not replace package/runtime verification,
the security workflows, or libaria2 release approval. The existing Loom OSV inventory
does not cover the independent NeuroDesign development dependency lock.

## 接入边界

- `LoomUiProvider` 监听真实 `data-loom-theme` 和系统模式，不持久化业务设置。`inheritTypography` 保留未迁移页面排版；Otter 画布的独立深色规则不变。
- `AppFeedback` 保留通知发布、Promise 确认接口和 FIFO 队列；`@neuro/ui` 只负责展示与通用交互。
- 每个确认请求独立读取 `optionDefault`，接受/取消都返回当前实际选项。无 viewport 时立即拒绝；viewport 卸载时拒绝所有待处理请求，并保留各自默认选项。
- `ConfirmDialog` 承担焦点圈定、Escape、滚动锁定与返回焦点；Loom 只传入队列身份和原始返回目标。
- MCP 工具栏操作/筛选、Art 作者字段和端口编辑已使用标准基础控件。隐藏文件输入、字段数值转换、工作流绑定和序列化仍由 Loom 管理。
- MCP 服务/商店导航使用标准可组合页签，保留方向键、独立搜索和未激活面板 DOM。服务/商店卡片操作、分页、徽标和空态也已统一；启停/测试/安装、凭据限制和并发锁仍由 `McpHub` 管理，不移入库。
- MCP 两种卡片根均使用标准 `Panel`，启停/来源/诊断用标准 `Badge`，不再使用 glass-card 或旧 Art 根类与着色。业务 CSS 只负责响应式网格、摘要、身份及操作布局；卡片用最小高度和内容固有行高，不用固定高度让状态挤入操作区。长错误局部省略，完整诊断保留于 title 和可访问文本；loading 不移动卡片和操作。旧页签/按钮/徽标/卡片皮肤不再接管标准控件，其他 Art/设备仍使用的样式原样保留。
- MCP 服务编辑和凭据配置已迁入标准 `Dialog`、输入/选择/复选及按钮。`initialFocusRef` 指定首个字段，`busy` 统一阻止关闭；不再保留手写 Tab/Escape、焦点恢复、body 滚动锁定或外部 body portal。
- 本地/远程连接切换、参数和环境变量解析、必填槽位、URL 校验、凭据不回显及替换/清空的提交仍由原应用代码负责。应用 CSS 只保留 MCP 表单布局、窄屏适配和内部滚动边界，不再复写标准弹窗皮肤。
- MCP 普通字段和凭据标签使用标准 Field，七条验证反馈使用 Notice；输入与弹窗关联到实际存在的诊断 ID，保留原校验与提交规则。凭据无槽位使用标准 EmptyState。保存按钮不随 busy 改名；成功关闭在应用动作解锁后发生，避免返回仍被禁用的入口。提交失败保留草稿；提交成功但刷新失败仍保留真实错误、不重复变更。
- 框架管理使用标准弹窗、DataTable、按钮和结果提示；框架身份是稳定行 key，名称列是行标题。安装立即执行、卸载二次确认、按真实框架身份禁用并发操作、ZIP 读取和同文件重选仍由 Loom 负责。关闭窗口不取消由外部控制器持有的框架操作，重新打开清除未提交的卸载确认；长表在弹窗内部滚动，操作标签与 loading 几何保持稳定。
- 能力权限审查使用标准弹窗和按钮，保留发布者、版本、完整摘要、请求权限及摘要绑定风险说明。批准启用/更新/回滚仍交给原应用控制器；执行中禁止关闭，长权限列表内部滚动，底部取消/批准操作保持可见。
- 共享常规、MCP、Art、代理和缓存表单使用标准 `Input`、`Select`、`Switch`、`Button`；“关于”页操作和日志级别也使用标准组件。保留 Loom/Otter 各自的草稿、数值转换、自动保存、清理确认和原生回调；Otter 设置是 Loom 中的宿主管理界面，不是 neuro-mini 实现层。
- 缓存清理的加载状态由标准按钮显示，标签和几何保持稳定，全组并发禁用不变。旧浅色、系统浅色及 disabled 原生控件覆盖已退出，标准组件自行管理主题和状态。
- 设置主工作区使用标准页签和受控 `Disclosure`，保留重新选择当前应用时重置分区、互斥分区和独立快捷键组状态。关闭分区仍卸载内容，草稿和自动保存由原控制器持有。
- 快捷键和 Art 快速绑定使用标准 `Dialog`、按钮、选择、复选框和冲突提示；三槽位、手势修饰键、去重和冲突检查不搬入库。原生按键捕获按钮刻意留在应用，通过弹窗 `onEscapeKeyDown` 仅允许该输入器捕获带修饰键的 Escape；普通 Escape、焦点圈定/返回和滚动清理由标准弹窗管理。
- Art 快速绑定的普通选择字段使用标准 Field 的标签与独立 ID，不再保留独立 label 皮肤；捕获槽标签、按键语义和三槽位布局仍属应用，不混入通用字段规范。
- 账号、NAccount、AssetLibrary 资源/下载和下载控制使用标准输入、按钮、复制、结果提示和空态。保留配置限制、校验码、显式打开授权页、远端撤销未完成提示、独立分页游标、三方包安装资格及摘要校验。复制拒绝由标准组件显示真实失败，不调用 resolve 掩盖错误；协议、轮询/取消、主体身份与任务生命周期仍归 Loom。
- 能力扩展的目录/已安装导航、卡片、schema 表单、受控折叠、徽标、空态和操作已统一。Field 为每个插件字段实例生成独立 HTML ID，布尔开关保持 DOM 与几何；权限状态同时显示已授权/待授权，不只依赖颜色。未知类型的数据保留、期望修订、包摘要、签名目录、ZIP 准入与生命周期控制器不进入库。插件设置失败后重试清除上一次错误，仍保留草稿及失败反馈。
- Art 创建向导的四模式导航、字段、脚本入口、端口折叠、结果提示和 loading 已统一；共享作者字段与输入/参数/输出行使用标准 Field、Panel 和基础控件。cURL/命令解析、MCP 发现、Python/art.json 读取、动态字段数值转换及流程参数绑定仍由原控制器负责，不搬入库。
- 创建向导控制器按职责拆分：useArtScriptSource 持有脚本读取/推断及请求失效，useArtMcpDiscovery 持有工具发现/schema 导入及请求失效，useArtWorkflowInterface 负责流程接口加载。跨模式草稿、模板重置、预览策略和提交接口仍由主控制器持有；业务 effect 顺序与卸载清理不变，不将应用生命周期转交给组件库。
- 流程预览策略的必要节点和预览节点至多 6 项平铺，更多使用可搜索多选及下拉单选；预览节点强制必要，切换预览不清除既有必要节点，改变可见选项不丢暂时缺席的必要 ID。端口选择占位保持稳定，选择不重建整组控件或移动焦点。
- 创建弹窗使用标准 Dialog，不再手写 Tab/Escape、重复 focus timeout 或模态皮肤。固定框架仅滚动字段与端口，创建操作保持可达；从已卸载工作流入口发起时，关闭返回当前 Art 创建按钮。标准 loading 保留标签和几何，创建中禁止关闭；关闭后晚到的源码、MCP 或流程读取不会污染新向导。旧向导自对齐和渐变规则已退出，避免内容缩到最小宽度。
- Art 编辑组件的参数、引用、机密、身份、版本及模态已使用标准控件；旧 framework-dialog 祖先皮肤、手写模态生命周期及 Registry 父层重复 focus timeout 已退出。固定弹窗内部滚动、底部操作保持可达；异步读取后可编辑名称获得焦点，busy 禁止关闭。管理读取按编辑会话失效，关闭或另开一条 Art 后，旧读取不得覆盖新窗口的管理身份或重定向其保存。
- 编辑值的 literal/global 两分支保留 DOM 并共享 grid track；非活动分支 hidden、inert 且 disabled，既保持 JSON 行高度，也不让隐藏必填控件阻止提交。机密已保存状态同样保留实际占位，不用固定像素猜测大字号徽标高度。复选、状态切换和 loading 不重建字段或改变组几何。
- Art 编辑验证与 settings 序列化在应用的 artEditModel 中，hydration 和原子提交仍由应用 hook 管理。保留必填/可选、0/false、JSON 解析、全局值类型匹配、作用域过滤、只读身份省略、已保存机密不回显/不误清空，以及先保存后更新版本的 Registry 顺序；组件库不处理这些规则。
- Art 主工作区使用标准可组合页签，三个面板保留 DOM，非活动面板显式隐藏。注册表、商店、发布列表使用标准 Panel、Badge、Notice、EmptyState 和操作；真实 Art 内容布局保留，旧共享卡片、图标操作及模态皮肤按真实消费者退出，不接管标准 Panel 皮肤。
- 框架筛选至多 6 项平铺，更多使用可搜索 MultiSelect；注册表/商店共享选择，搜索草稿独立保留。只有身份集合确实完整才归一化为全选；暂缺的已选 ID 不能导致其他明确排除项重新选中。勾选保留 DOM、焦点与工具栏几何。
- 发布使用标准 Dialog 的焦点、busy、Escape、外部点击和滚动清理；长作者列表内部滚动，固定操作标签不随 loading 变化。外部创建请求可替换发布窗，但旧会话的晚到成功/失败反馈不能出现在新会话；发布业务仍完成并同步真实状态，不因 UI 关闭伪装成取消。
- Art 创建包组装、schema/fallback、Python 源码读取和刷新顺序由 useArtCreationSubmission 持有；通用组件库不接管包身份、机密绑定、MCP 卸载依赖选项、商店分页、下载摘要或安装资格。
- 设备工作区使用标准页签；配对设备与屏幕墙保留挂载，其他四页仍按原条件挂载及身份 key 重建。设备入口、状态、空态及添加/批准/编辑弹窗使用标准组件，应用 CSS 只保留网格、内容排版、窄屏和内部滚动。待批准设备至多 6 项平铺复选，更多使用可搜索多选下拉；勾选、启停和 loading 不改变控件或弹窗几何。
- useManagedDevicesController 留在应用，持有 4 秒静默轮询、草稿、顺序批准、本机保护及 API 变更。daemon/在线会话变化或卸载时使旧读取、变更回执和待确认删除失效；已发出的 daemon 操作仍真实执行，但不污染新页面，未发出的后续批准不再启动。作用域切换重置对应草稿与弹窗，不将业务取消语义交给组件库。
- 凭据字段弹窗保留 string/number/integer/boolean/json、格式切换清值、0/false 和原提交条件；校验、scope、秘密读取/保存仍由安全页负责。异步 reveal 前显式记录触发按钮；保存/删除等待刷新完成后再关闭，删除原行后返回添加字段按钮。标准 Dialog/ConfirmDialog 在当前动作进入 busy 并被禁用时将焦点交给模态本身，让后续 Tab 继续由 Radix 管理，不增加手写键盘循环或延迟焦点脚本。
- 安全页全局凭据摘要使用标准 DataTable，行身份仍由 credentialFieldId 定义；只显示掩码，不在列表读取秘密。表格独立限高并可原生横向键盘滚动，窄屏添加/编辑入口仍可经 Tab 到达。策略、公私钥字段、可信用户操作和状态使用标准 Field、Input、Select、IconButton、Badge、Panel 与 EmptyState，不再保留旧凭据表格/按钮/字段皮肤。
- 安全 UI 按 daemon baseUrl 持有独立会话；更换 daemon 会清除旧草稿、私钥和 busy，晚到读取/已发出变更不污染新会话。凭据删除、移除可信用户与重置密钥在确认后重新核对会话，已经发出的变更仍真实执行。移除用户提交后仅在焦点已失去时返回可信用户输入，不抢占其他存活目标；重置的 loading 标签和几何固定。原全局 scope 过滤、身份正则、revoked key 计数、秘密 reveal/隐藏清理及保存/删除顺序仍由应用维护，没有改变 API 或信任协议。
- Device connections, LAN, peer Looms, projection rules and device groups use standard Field, input/select, Switch, Disclosure, Panel, Badge, Notice and action components. Long approval labels use a dropdown; short receiver-policy choices use segmented radios. Up to six device/group choices remain inline, with searchable selection for larger sets. Page CSS owns domain layout, narrow-screen behavior and scrolling, not control skins.
- 投射设备选择保留本机/远端完整身份、原引用顺序和重复项；目录暂缺的已选设备可取消，但取消后保留候选 DOM 并禁止重新绑定，显式目录重读才可清除候选。协议上限按序列化的 64 个引用计数，不能只计算去重后的身份。完整身份保留在 title 和搜索文本中，不从显示名称反推标识。
- 连接码不回显，启用且留空时省略密码更新，明确关闭时发送空串清除。关闭仅将输入 hidden、inert、disabled，保留真实高度且不参与原生必填校验；窄屏改为纵向排列，避免保留提示被挤压。轮询、审批、CAS、信任中断确认、失败草稿和晚到响应隔离仍由原应用控制器负责。
- 屏幕墙字段、输出卡片、操作、状态、图片导入和瓷砖/内容折叠使用标准组件。墙面至多 6 项平铺，更多用受控 Select；保留数值空值 NaN、原生 step、只读墙面身份、稳定行 key 和隐藏原生文件选择。应用 CSS 仅持有领域网格、滚动、窄屏和原 SVG 预览，不再接管按钮、输入、折叠或焦点皮肤。
- 布局保存、识别和冻结/黑场/恢复仍是原控制器的显式 API 命令；保存 loading 只表示布局保存，不以全局 busy 冒充。确认等待期间切换 daemon/在线状态会使未发出的动作失效，已提交变更仍真实执行；删除或关闭编辑区后，焦点在实际提交后返回创建入口。原 CAS、草稿锁定、调度能力、完整来源引用和精确输出回执保持应用所有权，轮询和来源刷新不会自动重置冲突草稿。
- 工作流画布工具栏使用标准 Field、Select、Range、Button 与反馈；保留原缩放映射、0–1000 原生步长和 5%–600% 范围。保存及缩略图按钮标签固定，loading/pressed 不改变邻居几何；删除提交后回到仍存活的工作流选择框，显式切换选择会清除该焦点意图。
- 工作流接口面板和参数外露编辑使用标准 Panel、Badge、Checkbox、Input、Select。每个外露项是与常量编辑器绑定的独立字段，不是可整体折叠成一个平面多选菜单的候选目录。布尔字符串、数值空串、0/false、metadata 的 min/max/step 和原 bundle/实例化序列化不变；勾选只禁用相应常量字段，保留 DOM、草稿与焦点。接口输入/输出摘要独立限高且可用原生键盘滚动，摘要条目增减不会推移参数编辑区。
- 重命名使用标准 Dialog 和受控原生表单，初焦、Tab、Escape、滚动清理和返回交由组件库；busy 期间不关闭、不重复提交，空白名称不能通过 Enter 绕过。画布、节点、viewport 和纹理仍由原应用负责；外围旧缩放/输入/模态覆盖已退出，缩放读数按应用语义文本色显示。
- 只读节点属性根与父层普通错误反馈分别使用标准 Panel/Notice，不再保留重复面板皮肤。节点类型回退、尺寸取整和原 error && !snapshot 条件保持不变，长节点/能力身份可读取完整 title，窄屏属性单列。可选 tools 默认使用稳定空数组，避免参数派生 effect 在无元数据时循环更新；没有改动原图形、viewport、工作流请求或生命周期。
- App 导航、设备入口、返回、侧栏折叠、刷新和三个窗口操作使用标准 NavigationItem/IconButton；删除冗余页头眉题、导航状态皮肤和独立刷新动画。窗口命令、drag region、双击和设置返回仍由 Loom 接线，刷新沿用原单飞和请求失效控制；loading 保持名称和几何。
- 侧栏窄屏默认折叠与跨阈值更新由同一个 React 状态管理，不再由 CSS 强制覆盖，所有宽度均允许手动展开/收起且 aria-expanded 与实际宽度一致。监听随 App 卸载清理；真实 App 浏览器验证导航 DOM/焦点、原生命令接线、刷新失败重试及卸载后停止轮询，未执行真实原生窗口验收。
- L16A 已清理 20 份 CSS 中的 364 个无消费者选择器分支及两段旧反馈动画，保留每条存活规则的声明、顺序和媒体条件；生产 TypeScript 未改。反馈、模态和输入可访问性契约现在验证实际消费的标准包，不再要求不存在的旧控件皮肤。127 项相关回归、真实 App 29 项交互/清理事实及 36 组明暗/系统浅色/宽窄屏/大字号样式与几何对照通过。
- L16B 已完成剩余动态/组合选择器与重复主题映射审计。foundation 的 19 个应用语义变量在 `.neuro-root` 内引用标准 `--n-*` token，不再维护第二套配色；html/body 仅持有文档 reset 与启动阶段的系统 Canvas/CanvasText，不向上读取子 Provider 变量。等宽文字统一使用 `--n-mono`；通用 focus、reduced-motion 和 forced-colors 由标准包负责，图形边的高对比度规则仍在应用。
- 最终原生控件审计仅剩五个隐藏文件输入、ShortcutSlotsEditor 的业务按键捕获与 OtterCanvasNode 的图形目标。已退出的 art-creator 样式模块及框架旧表格皮肤删除，其他 584 条保留规则的声明、顺序与媒体条件有审计。最终 21 文件 137 项相关回归、真实 App 29 项壳层事实、15 项主题/表格事实、42 项框架行为（含 24 组布局）与 21 项高对比度/减少动态效果/字体事实通过；未修改图形生命周期或实际 daemon/原生行为，也不据此宣告整仓门禁或发布通过。

ArtEdit、Registry 和创建向导控制器拆分后，新改源码均不超过 500 有效行，已删除它们不再需要的旧行数例外，没有延期或伪造批准。创建向导主控制器为 459 有效行，三个职责 hook 分别为 188、93、118 行；65 个原声明、5 个业务 effect 顺序、返回接口及两个请求失效语句核对一致，81 项回归和 49 项浏览器行为/清理事实通过。

设置主控制器同样从 676 降到 474 有效行，缓存操作和快捷键编辑分别由应用私有 useSettingsCacheControls（141 行）、useSettingsShortcutEditors（217 行）持有。读取、自动保存队列/回滚、运行时应用、草稿与 shortcutsRef 仍由主控制器管理，54 个保留声明、7 个原 effect 顺序和完整返回接口不变。缓存确认后复核 mounted owner；已发出的清理仍完成，但卸载清除延迟重读，晚到结果不再安排重读。64 项相关回归、原完整快捷键脚本与 19 项缓存/确认/有序保存浏览器事实通过，全部 native 边界为隔离合成适配，没有清理真实用户缓存。设置控制器旧例外已删除；整仓 strict 下一阻碍为未修改的 art-packages/samples/image-search/runtime/main.ps1 到期例外。本批未改依赖安全例外或 Rust 依赖，现有门禁继续按原规则执行，不能因为 UI 迁移绕过审批或漏洞检查。
