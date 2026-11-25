# 本地 Art 的 Live 自动触发合同

当前状态：Otter 原生迁移的 MAIN 所有；v519服务端定向、v520原生定向及真实HTTPS/WSS限定联合通过。
本文件说明合同及下列分层证据，不是普通可见双产品、真实跨机或最终包验收证明。

## 授权与边界

固定端点 `POST /v1/art/local/live-trigger`，协议 `loom.art.local.live-trigger.v1`，
请求最多 32 KiB。只允许已配对 Device 的原有认证/nonce 通道；管理员凭据不能替代 Device。
LAN 只增加这一精确路径，不开放任意 Art、Live 或管理路由。

请求携带完整 `binding`、原签名 `grant` 和 `operation`。
原 `loom.art.local.grant.v1` 的四权限不变；Preview 仅证明当前本地宿主身份，
**不会隐式登记、授权或执行自动回调**。Configure 必须额外明确
`consent: automatic_local_callback`，并绑定精确实例/工作区/代际/包、Live 会话/epoch、
条件修订和宿主在 POST 前生成的独立 `authorizationId`。
宿主不得把 Art 脚本、网络响应或旧 Preview 当作可信用户的配置意图。

Configure 和 Claim/ACK 继续检查当前原生宿主签名、原授权期限、Device、包启用状态和审批。
Configure 还核 immutable 源码。配置后整个审批文档修订变化保守退休旧自动授权；
撤销后重新审批不能复活旧注册，须宿主显式重新配置。
服务/正式结果仍受原各自权限及敏感确认保护；回调不是物理用户手势。
离线恢复票不恢复自动触发授权。

## 操作与结果

`scope` 包含 `sessionId`、`epoch`、`bindingId`、`authorizationId`；
本地目标的 bindingId 必须以 `local-art:` 开头。所有操作匹配完整 grant binding 和 grantId。

- `configure`：`configuration` 含 scope、明确 consent、原 `LiveTriggerCondition` 和
  原 Live actor 控制 `sequence`。只允许已有且在线的 Viewer；不自动加入、取得控制权或注入输入。
  更新要求严格递增 condition revision、全新的 authorizationId 和原控制序号。
- `inspect`：只读对账，不返回 observation payload，不延长任何期限。
- `claim`：至多一次领取当前单槽 payload。**状态先变 Claimed，再写 HTTP 响应**。
  丢回复/取消/崩溃视为未知，不重放 Claim 或从 Inspect 重造执行。
  重复 Claim 可以得到 Claimed 状态，但不会再次得到 payload。
- `ack`：必须匹配已领取 deliveryId。重复同值回执幂等，冲突/旧事件/旧授权回执拒绝。
  回执为 `applied(formalRevision)`、`no_result`、`failed` 或 `cancelled`。
  **Acknowledged 仅是宿主报告，不是服务端证明 canonical 保存、接收端呈现或整链成功。**
- `stop`：只减权，清 payload；有效 Device 加原精确签名身份可在 Art 审批撤销/过期后停止。
  迟到 ACK 不得恢复活动状态。

回复显式携带 protocolVersion、完整 binding、grantId、scope、condition、enabled、status、
固定 reason、deliveryId、receipt，以及仅首个 Claim 可有的 delivery。
delivery 只描述 `onLiveTrigger` 回调、conditionRevision、原始 observation、sourceDeviceId 和固定期限；
不描述点击、不向脚本发 Device 凭据，也不替代宿主在执行/发布正式结果前的 owner 围栏。

## 求值、容量与回收

Surface 与本地 Art 共用 `evaluate_live_trigger_state` 和原条件比较/可信度/稳定时间逻辑。
观察仍只接受真实 Source、原 epoch、逐观察及控制序号、能力声明和原新鲜度校验。
不在 Otter 复制第二个条件引擎，不伪造 Surface attachment，不改普通 Surface action 合同。

两类注册合计沿用每会话最多 64 的门。每本地注册只有一个在途槽，不排队、不覆盖 Ready/Claimed。
超出本协议 16 KiB delivery 门时明确停止并报告 `delivery_limit`，不截断观察；
原观察 64 KiB 门不缩小或放宽。单槽上界最多 64 × 16 KiB，加有界注册元数据。
这不是每个观察分配一个长期任务，也不新增网络线程、timer、worker 或持久队列。

事件最长保留原观察新鲜度 30 秒，且不超过观察本身或原 grant 的剩余期限；
同时使用墙钟高水位和单调期限，重复查询不续期。过期、时钟回退、停止、Viewer 最后连接退出、
审批变更及 Session close 会退休或拒绝执行；payload 在后续求值/请求/断连/close 时释放。
会话空闲无请求时不新增定时扫描线程；状态和内存仍受固定槽上界约束。
最后 Viewer 断开后重连不会自动恢复本地触发；Session close 清除全部注册。

## 所有权与验证

- `crates/loom_protocol/src/local_art_live_trigger*`：版本化 wire 和严格合同。
- `apps/daemon/src/local_art/live_trigger.rs`：原 Device/审批/包检查到原 Live store 的边界。
- `runtime/live_session_trigger_evaluation.rs`：唯一共享条件状态转移。
- `runtime/live_session_local_trigger*.rs`：本地登记、单槽、领取/回执及减权生命周期。
- `tests/local_art_live_trigger_*`：独立固定 oracle 的 HTTP/并发/逻辑时钟/容量验证。

v519 HTTP 测试通过测试 owner 直接登记 Viewer 媒体在线状态；
该历史成绩不是实际 WSS 媒体、Otter JS 执行、正式结果/Save/Checkpoint 或普通双产品验收。
v519收口时仍需由MAIN完成原生菜单、独立自动授权、回调/取消与正式结果接线，
这部分后续v520的实现与真实HTTPS/WSS联合见下节；不改写v519服务端限定成绩。

## v519 实际验证边界

input2选定相关Rust完整名按crate去重75 passed、1默认ignored，非整库；protocol39/daemon36，其中新增协议3项、服务12项。原Art观看/控制、Surface触发/观察、媒体断连清理与LAN边界回归通过，普通daemon构建及--version/--help通过。HTTP测试用owner直接登记Viewer在线，不是真实WSS或Otter联合。20份Rust最大457有效行；官方格式、版本、Otter全仓strict行数、两仓diff及精确UTF-8无BOM/空白门通过。
input1新服务11项中10过1失败：stale夹具未同步stableSince，原wire先拒400而非freshness409；input2修成一致旧时间，保留409断言和原生产新鲜度校验。审查另补每事件墙钟expiresAt校验，覆盖Ready和Claimed墙钟前跳先于单调期限的过期回归，不改系统时钟或放宽超时。input1失败与全部回执保留，input2只改两份生产叶和一份测试叶，protocol完整输入/固定SHA与成功回执精确复用。审计器初版误将默认ignored当失败，修正后只解析原日志，未重跑测试。
Loom全仓strict行数仍被旧ArtEditDialog.tsx于2026-10-31到期的例外阻塞；daemon/protocol strict Clippy仍各101/10条旧诊断，按完整签名、全部primary位置、次数及前态源码SHA核实无新增。普通daemon有原otter_canvas/document.rs::read未使用警告，其源码未变；不称严格门或两仓全绿。

## v520 原生接线与实际联合

原生显式配置→原Live唯一控制worker Configure/Claim→签名child onLiveTrigger→原Application/Results实际事务→ACK及Stop/回收已接通。独立不可逆自动授权绑定完整实例/工作区/包、会话/epoch与条件revision；Preview不等授权，重连不复活旧gate。单槽/16KiB、原期限与共同容量门不变，不新增线程、timer、持久队列、权限或WebView。
用户在既有观看系统菜单配置严格Condition并明确“允许自动回调”；停止先撤本地gate。回调不伪造click/gesture、不请求焦点或恢复暂停timer。Applied只在正式端口原子提交后产生，formalRevision为当前执行实例成功提交序号，不是磁盘版本；NoResult只在实际present后报告。原Results最终predicate再核Runtime/Lease及固定期限，Stop与canonical事务共用无Win32/网络/脚本回调的短锁。
input4相关Rust按crate/完整名去重87 passed、7默认ignored，非整库；新自动回调子门虽默认ignored，已由真实LAN父门显式运行且父子不混加。真实签名child验证暂停回调/缺handler/迟到拒绝；隐藏Application验证提交前无Applied、取消保canonical、正式像素及Save恢复、实际present后NoResult。原本地Art缓存/有效授权离线/结果/宿主退休和Live观看/控制、普通固定SHA EXE工作区5项直接回归通过；前台退出门未运行。
新真实Loom LAN HTTPS/WSS联合6.656秒通过：只改自有离屏EDIT value-seven→value-eight，独立像素[17,83,211,255]、公开SDK同份源码签名执行、data计数1及deliveryId/观察匹配、原Results/Save恢复、Editor不变、独立Device Inspect确认服务端acknowledged/applied(1)，显式Stop及两端owner/媒体permit回收。经产品菜单的同一提交接缝，不宣称真实用户点击了模态菜单；无真实设备或用户程序输入注入。
Otter strict Clippy、lib/普通product/workspace及Loom测试lib构建通过，耗时158.110/408.172/184.781/140.235/50.453秒；官方格式、版本、SDK类型/JS语法、Otter全仓strict行数、两仓diff及精确UTF-8无BOM/空白门通过。44份Rust及5份JS/TS增量均≤500有效行，最大470；不重跑未变SDK等静态面。Loom strict Clippy101条原诊断及旧ArtEditDialog.tsx到期例外仍失败，签名/位置/次数和前态SHA无新增，不称两仓全绿。
本批仅关闭本地Art自动触发的源码/定向/隐藏Application及真实LAN联合接线，不是普通可见双产品、真实跨机或最终包验收。v518全局前台身份失败、v517可见混合门/v511历史键盘根因仍开放，前台自动化继续暂停；人工IME候选窗/旋转活动EDIT、物理多屏/跨机/DPI热插拔、长稳、其余迁移退出条件、最终隔离整包与用户全面手测仍开放。三旧副线、v464固定手测包与真实profile/设备保持；旧Web/Tauri混用不重列必做，总迁移active。
