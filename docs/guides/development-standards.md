# 开发规范

本文约束后续新增与修改代码。当前已实现范围以源码、[会话中心与节点所有权](../design/node-session-ownership.md)及 [transport 架构](../architecture/transport.md)、[Agent / UI 扩展架构](../architecture/extensions.md) 为准；设计目标、类型预留、原型与测试 fixture 不构成能力已实现的证据。分批迁移过程已移入[历史记录](../archive/node-session-migration.md)，不得把其中的阶段限制当作当前架构或剩余任务。

## 1. 代码职责与依赖

- `work-node-api`：纯 DTO、typed `NodeApi`、会话能力/配置与资源/realtime 契约；`work-core::api` 重导出它，`work-core` 另负责客户端事件规范化、运行投影和惰性设计。两者不持有节点 owner。
- `work-client`：客户端连接、认证、传输适配和消息投影。`WorkspaceClient` 统一本地门面与远程访问；本地可以进程内调用，不强制绕行 HTTP。保留的 `AgentClient` / `prepare_agent_session` 是兼容入口，不得作为新 UI 业务入口。
- `work-node-runtime`：实际唯一 workspace owner、RPC/ACP、会话绑定、原生资源和源码服务；`work-node` 仅兼容导出。不得新建第二份 owner 或把桌面本机改成回环 HTTP。
- `work-node-service`：同 owner 的 HTTP/SSE/WebSocket adapter，`work-http-runtime` 提供认证/共享辅助；`work-server` 仅兼容导出并保留集成测试/examples。不另建 owner 或第二套执行状态。
- `work-node-host` 实现无 GUI 单 owner 初始化、组件注册、HTTP 配置、数据锁及协作退出；`work-host` 组合应用门面和本机 client。GUI 与 `work-cli serve` / `work-ui --headless` 共用，不另建会话库或执行系统。显式远程桌面是纯客户端，不能在连接失败后抢锁创建 owner。CLI 新业务走 typed 节点方法，直接 Agent 模式仅保留为显式兼容诊断。
- `work-ui/features/*`：领域交互与宿主控制的渲染；跨功能协调放在 `app/`，可测试的状态判断优先抽为纯函数。
- 正式应用不得依赖 `design/prototype/`、测试 fixture、邻接仓库或仅本机存在的开发资源。测试 harness 不进入正式安装包。
- 不为尚无真实协议的功能创建通用 Plugin Trait、独立服务或动态加载器；先确认上游契约，再增加最小适配。

## 1.1 会话中心分层：新增功能的硬约束

依赖方向固定为 **客户端 → node 应用服务 → runtime / capability adapters**，共享节点契约放在 `work-node-api`，客户端投影放在 `work-core`。先从用户工作流定义 typed 应用方法，再复用已有协议；不得为了抽象对称扩展未实现的能力。

- **客户端是平等的节点消费者**：Web 与 Desktop 平级，都可连接其他节点，但默认使用当前节点；Desktop 默认本机owner，Web 默认提供入口的节点，不能将默认入口永久锁定为唯一可用节点。移动客户端不托管本机节点，只连接其他设备的节点；手机浏览器仍按 Web 规则处理。三端使用相同会话语义，Desktop 额外托管 Node 不产生另一套执行协议。会话不要求先创建独立 Agent 定义。
- **所有权唯一**：本地门面与 Server 必须共享现有 owner（同一个 Arc），不可分别持有 Manager、进程、权限表、journal 或会话锁；投影与客户端连接恢复仍归 client，不搬入 owner。
- **UI 只走 typed 应用入口**：会话打开、状态/队列、配置、发送/恢复/停止、权限回答及统计均通过 `WorkspaceClient` / typed `NodeApi` 端口。禁止 UI 保存原生 Agent 句柄、按 Native/HTTP 分流业务、直接启动子进程，或以通用 JSON/raw command dispatcher 绕过应用服务。内部兼容 raw 通路不是新业务 API。
- **能力来自绑定会话观察**：执行面用 `SessionAccess` 的精确命令、`snapshot_limit`、`dialog_reply`；配置面用 `SessionConfiguration` 的精确命令/协商选项。节点全局 capability、本地句柄存在、协议大类和 `for_agent(None)` 等乐观默认均不能代替观察。失败、未知、旧服务 404/401 时禁用相应动作并明确提供恢复路径，不能回退本机或推定“全部支持”。观察不是授权，也不是 installed/loaded/active/executable 的资源证明。
- **端口和 executor 边界不扩大**：应用 future / 事件端口保持 object-safe 且不强制 `Send`；遵守现有 current-thread / LocalSet。底层 runtime 流自己的 `Send` 约束不意味着所有业务 future 必须多线程化。新增端口默认 `Unsupported` 不等于每个实现都已具备能力。
- **协议与依赖不另起炉灶**：优先复用 extcore/RPC/ACP；当前 `rpi-extcore` 使用 manifest `=0.2.11` 与 Git revision `1e3c9551a054fe3bb6f2ef905429268bc4a7e5bd`，升级须单独核对 lockfile 和原生契约，不能替换成 crates.io 同版本。没有已确认工作流与上游契约，不增加 crate、第二套调度器或通用插件执行框架。

## 1.2 会话一致性、权限与生命周期

- 所有异步观察和写入回调必须核对 node/session scope、session generation、connection epoch；目录/读取还需加载 ticket 或 revision，防止同地址重连、切换会话及迟到成功/错误污染新状态。同地址不代表同一次连接，必须重新观察并建立会话连接。
- **ACK 不是 settlement**：发送、恢复、压缩与 abort 的受理不能伪造运行完成；保留 ACK 前结算、pending echo、请求去重、草稿 revision 和迟到响应保护。仅明确 accepted 的操作才按原规则清除该提交版本草稿；未确认结果不得宣称“已入队”或自动重发。
- 配置沿用 15 秒 deadline、权威回读、`requiresRestart`、provider/model 原始身份和协商 id/value 校验。超时只停止本地等待，不代表节点取消；结果未知时保持待核对锁，不用 setter 回包冒充实际状态。刷新能力只作废旧观察，不能吞掉同 scope 在途写入的失败清理。
- ACP permission 回复由节点验证当前 binding、generation、terminal 状态及 outstanding request，再使用原权限转换映射选项。不得接受客户端 raw JSON-RPC responseId/result 来绕过绑定，不扩大 ACP raw 白名单；历史 journal 请求不能重新变成可回复权限。
- 保留订阅/快照顺序：本地 subscribe-before-snapshot 并过滤已覆盖序号，远端按 snapshot cursor 订阅；是否限量必须依据会话观察。保持 ACP journal/replay、消息身份/投影及 resync/gap 语义，不按消息正文全局去重。
- 桌面窗口/导航的生命周期不等于节点执行生命周期。不得因 UI 离页停止 owner，或更改既有关闭顺序、锁、子进程回收与后台托管策略。任务取消/deadline 不得被描述为服务端执行已取消。
- 附件当前 typed prompt 入口为纯文本；有附件时保留明确的“仅发送文本”确认、草稿/附件及作用域保护，不能静默丢弃后宣称上传成功。隐藏 composer 产品开关不因架构整理而启用。

## 1.3 跨平台单一 UI：新增与修改的硬约束

- **Desktop、Web、Android 共用同一套页面、组件、展示状态与交互契约**。响应式布局只调整排列、尺寸和可用空间；不得因为 `target_os`、`target_arch`、UA 或屏幕宽度，复制一份业务页面、另写连接表单/消息界面，或引入不同的默认落地页与业务导航。
- **平台分支止于能力适配层**：Cookie/Keystore、相机权限、系统文件选择、键盘/安全区、剪贴板、安装更新及本机 Node 托管等，可以有不同实现；向共享 UI 提供明确状态、能力和回调，不让页面按平台拼装另一套操作流程。控制器可以适配认证协议和生命周期，但不能各自维护一份完整页面组合。
- **安全策略不是平台皮肤**：托管 Web 的初始宿主认证目标固定；这不禁止认证后的 Web 像桌面一样主动连接其他节点。节点切换须单独取得目标授权，不转发原宿主密码、Cookie或QR票据，不以关闭同源、精确 Host/Origin、共享 fail closed、二维码一次性/身份核对或原生凭据加密来实现。差异用明确的部署策略/能力表达，并提供说明、禁用或恢复路径；不能为追求相同按钮而扩大授权、泄露凭据，或把平台名称当作能力证明。
- 连接入口和移动连接设置统一使用 `connection_panel::ConnectionScreen`；认证后的重连/退出通过 `entry::ConnectionSessionControl` 接入。不得在 Android/Web 控制器中重新组合 tabs/form/scanner 或按平台选择设置页行为。静态 standalone 与固定初始宿主认证是部署策略，不另建视觉实现，也不能据此隐藏 hosted Web 的主动节点切换入口。
- 新增平台差异必须在变更中说明必要性、所属适配器/策略及回归证据。同一 presentation 须使用相同输入和操作脚本覆盖 native/Web 能力配置，另测 fixed-host 等安全策略，并验证 320/390px、明暗主题、44px 点击区、键盘操作和无横向溢出。
- **共享源码不等于视觉已验收**：单测、浏览器适配配置、Android 编译/APK 核验、模拟器与真机分别记录。声明两端视觉一致前，须以相同版本、CSS 视口、主题、密度、内容和状态做成对核验；不能以源码 grep、共用 CSS 或 Web 截图代替 Android 设备结果。

## 2. Agent 与 UI 扩展必须独立

扩展设计允许 `agent-only`、`ui-only`、`combined` 三种形式。组合包可以共享发布身份，但两种能力不能共用一个含义不明的“已启用”状态。

| 事项 | 必须遵守 | 禁止混同 |
| --- | --- | --- |
| Agent 执行 | 执行节点/宿主负责加载、权限和执行 | 客户端显示了卡片 = Agent 已加载 |
| 纯 Agent 扩展 | 不需要自定义 UI；可使用通用工具卡 | 缺少 UI = Agent 不可用 |
| 惰性 UI 设计 | 受控静态预览，不加入 Agent pins | 设计预览 = 动态组件或节点安装 |
| 自定义组件 Bundle | 节点安装、显式 grants、按需隔离 frame | UI 安装 = DLL 加载或宿主切换 |
| UI 显示偏好 | 以设备、扩展版本、contribution 为边界 | 隐藏 UI = 停用 Agent 或撤销权限 |
| 工具/权限声明 | 只是请求和设计元数据 | 清单中的工具已注册、请求的权限已获授权 |

当前 `extension.design.json` 是 RPI Work 的**设计格式**，不是 rpi 原生安装清单。不能把设计导入、源码展示、结构校验、测试快照或个人发布包装成实际安装/编译/执行成功。

## 3. Desired、observed、applied 与 UI preference 分离

- **Desired**：旧设计 pins 表达固定扩展 `(id, version)`，不接管原生配置。原生 exact 会话使用 typed native apply、operation ID 和 revision；使用既有 node/session scope，不以显示名识别资源。
- **Observed**：来自目标执行节点的目录/安装/运行观测，必须带节点身份。没有观测时保持 unknown，不能默认“未安装”或“空闲”。
- **Applied**：以原生应用结果、fresh readback 和 host generation 核对；本地复选框、请求受理或设计导入都不能产生 applied 记录。历史配置记录也不能单独证明当前可执行。
- **UI preference**：设备本地展示偏好；不得增加 Agent 会话 revision、改变 Agent pins、授予权限或改写已发布内容。
- 请求成功只证明相应请求已完成，不能替代后续运行事件、结算或 applied 确认。不得以工具目录反推插件安装状态。
- 尚未支持的操作应通过 capability 和明确的 `Unsupported` 暴露；不得猜 URL、后台试探端点、伪造成功或隐式切换 transport。
- 客户端不支持某种 live UI 时，未来协议应允许 Agent 降级到通用展示。当前拒绝不受支持的**设计导入**，不得因此卸载或停用节点上已有 Agent。

## 4. 运行期配置锁：未知时默认锁定

Agent 会话配置仅在以下条件**同时成立**时可修改：

1. 当前选中会话已有同步后的 projection；切换会话、首次连接或恢复期间不得复用旧会话的空闲状态。
2. transport 和 projection 连接状态均在线，不处于重连、重同步或离线状态。
3. 运行 phase 明确为 `Ready` 或已结算，而不是 Accepted、Preparing、Text、Tool、AwaitingInput、AwaitingSettlement 或 Cancelling。
4. 没有待回答对话，没有正在提交的请求，也没有已提交但仍等待回复/运行事件的请求。

特别注意：

- `projection == None` 不等于空闲；不能用 `is_some_and(is_running)` 的否定推导“允许编辑”。
- prompt HTTP/RPC 返回成功、`sending` 清空，并不意味着节点已完成运行。必须同时考虑 pending-reply / interaction 状态，覆盖运行事件尚未到达的时间窗口。
- 正式会话界面统一使用 `app/runtime.rs::extension_config_locked`；不要在组件里重复拼接一份条件更少的判断。组件未收到明确状态时默认锁定。
- UI 显示/隐藏不受 Agent 运行锁影响。仍可查看声明与只读预览，不能因锁定 Agent 配置而禁止普通展示操作。
- 这只是客户端一致性保护，不能替代节点已有的 revision CAS、权限检查和原生执行 admission。pending/blocked 资源恢复使用单独 typed 恢复入口，不能伪造 idle projection 放开普通编辑。

## 5. 导入、恢复与不可变版本

- 外部输入先做结构和业务验证，再一次性修改资源库；校验失败不得留下包、派生扩展或半份绑定。
- 现代清单使用强类型 schema 和显式验证：校验版本、身份、列表/文本上限、重复 ID、进度范围及包内相对路径；反序列化成功不等于业务校验成功。
- 内层扩展 ID/版本须与外层包一致；Agent entry 须在包源码中存在。源码中存在文件并不意味着文件可安全执行。
- 恢复时从不可变发布包的 canonical sources 重建现代扩展声明。独立保存的 manifest、`node_installed` 等缓存/布尔值不得成为节点事实来源。
- 同 ID、同版本、相同内容可重复导入；同 ID、同版本但不同内容必须拒绝，要求新版本号。
- 导入新版不得自动替换现有会话 pins；找不到旧版本时保留 unresolved 证据并允许安全移除，不能偷偷选择“最新版本”。
- 旧 entry-only 设计按惰性资源兼容。未知现代 schema/widget/runtime 必须明确拒绝，不可退回旧格式绕过校验。
- 配置快照记录 desired revision、固定版本与 unresolved 状态；实际节点应用未知时明确保持未知，不写成成功或失败。UI 偏好不进入 Agent 配置快照，也不注入 prompt 或工具目录。

## 6. 声明式 UI 安全边界

本节限制 `extension.design.json` 惰性设计，不限制独立 Bundle 的已隔离 HTML 协议；二者详见[扩展架构](../architecture/extensions.md)。

- 设计提供数据、宿主提供渲染器，使用 allowlist widget（包括受限 `host-value` 设计声明）。
- 禁止扩展提供任意 HTML、JS、CSS、URL、动态组件、事件处理器或可执行 action；文本经正常 Dioxus 插值转义，不使用 raw HTML 渲染扩展内容。
- widget、slot 和 schema 使用明确 allowlist；限制条目数、文本长度和数值范围。未知字段不能被静默执行或解释为权限。
- `composer-action` 等 slot 标签目前只是设计意图；未实现真实挂载与动作协议前，不展示为可执行入口或实时状态。
- 组件 Bundle 通过已有 MessageChannel 动作桥，必须经声明/grant/宿主能力策略核对；iframe 不得获得同源权限、节点凭据或父页面 DOM。通用安装器不按 HTTP 组件 ID 写业务表单，也不下载节点宿主 DLL。
- 新增 contribution 动作需明确会话归属、关联 ID、版本兼容及失败反馈；UI 可见性不是授权。

## 7. 测试与 review 最低要求

节点/会话应用入口的变更至少覆盖：

- 同一 owner 的本地、HTTP 与 typed trait port 语义一致，跨会话/跨节点隔离；未知能力和旧服务失败时无 native/global fallback；
- ACP 权限原始选项映射、过期/跨会话请求拒绝、快照恢复 pending dialogs、cursor/replay/generation 与关闭后行为；
- 正式 UI 的同地址重连、旧 scope 迟到结果、ACK/settlement 乱序、配置回读与超时待核对、入队 accepted 未确认及草稿保留；
- 仅隔离 fixtures，不读取用户真实会话/凭据或启动已安装 Agent。编译、模拟器/浏览器 fixture、APK 构建签名、安装升级和真机行为分别记录，不能互相代替；
- Android 使用 [共用 NDK 工具链脚本](ci-builds.md#本机复现)后实际检查 client **和 mobile UI**；core/node 通过不能冒充 client 已通过。缺 compiler、磁盘不足或依赖缺失必须报告为阻塞，不通过删除 TLS/依赖、跳过失败或切成桌面 target 掩盖。

扩展相关变更至少覆盖：

- 三种扩展组合、合法清单往返与旧设计兼容；
- 未知字段/schema/widget/runtime、路径穿越、重复 ID、越界数据和非法输入的原子拒绝；
- 固定版本、新版导入不自动重绑、node/session 隔离及 unresolved 版本；
- canonical 清单恢复、丢弃伪造安装布尔值、导入/持久化/导出的原文一致性；
- UI 隐藏不改变 Agent pins、revision、发布物或资源快照；
- **无 projection、离线、重连、重同步、发送中、提交已确认但运行事件延迟、等待回答、取消中**的锁定，以及在线空闲/已结算后的解锁；
- 浏览器纯文本转义、运行锁下仍可更改 UI 偏好、刷新恢复，以及 390px / 320px 布局。

确认 bug 时，优先增加能让旧逻辑失败的回归测试，再做最小修复。模块 harness 人工切换锁只能证明组件行为，不能单独证明正式应用的连接/发送状态接线正确；必须另测正式使用的状态判断或真实 fixture 时序。

真实原生扩展验收还须运行 [隔离 Demo](extension-demo.md)，区分模型 fixture 与真实工具事件，记录宿主/SDK 版本、动态库 hash、成功的工具终态及 `agent_settled` outcome。计数完成不能代替 Agent 成功结算。清理须覆盖启动、执行失败及正常退出，卸载后用新宿主确认工具消失；不能依赖强杀或修改全局插件目录。开发适配器、token 和本机测试入口不能进入正式客户端 capability 或被描述为通用 live UI 协议。

基础验证命令：

```shell
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p work-ui --target wasm32-unknown-unknown --locked
node tools/review/check-structure.cjs
```

WASM 检查需先安装相应 Rust target。UI 改动还应重建模块，不能用旧 WASM 冒充本次源码的验证结果：

```powershell
pwsh -File tools/review/build-module-review.ps1 -Feature all
# 另一个终端启动测试服务，测试完关闭本次启动的进程。
node tools/review/ui-review-server.mjs
node tests/modules/verify-extensions-module.cjs
node tests/modules/verify-resources-module.cjs
```

独立 harness 使用 `tests/harness/ui-modules/Cargo.lock`，增加依赖时一并校验。真实模型调用和宿主 smoke 属于显式可选验证；不得把 ignored、未运行、模块 fixture 通过描述成真实节点安装/执行已通过。

## 8. 文档与交付

主题相关修改遵循 [视觉规格契约](theme-design.md)：先将受控参数解析为语义 token，再由组件消费，不把 CSS 文本、字体 URL 或布局选择器作为主题输入。新增文字使用角色字号或 `--type-scale`，常规间距使用 `--space-*`；不要新增独立固定配色或字号绕过主题。v1/v2 往返、取消恢复、跨主题无残留、密度组合、手机 16px 输入/44px 触控下限以及 reduced-motion 必须保持。主题不拥有业务布局和 Agent 状态；新增字段需同时有实际消费者和 computed-style 回归。

- 行为或契约变化同步更新架构说明；“已实现”“预留类型”“未来协议”必须分开描述。
- Review 结论给出严重级别、定位、可复现场景、影响和修复/测试结果，不用“暂未发现问题”代替实际检查范围。
- 测试日志、截图和临时构建物遵循 [测试产物规则](testing-artifacts.md)，不提交本机索引、异常文件、凭据或会话数据。
- 交付时说明测试实际运行范围、未验证能力、是否提交及是否 push；不擅自发布、调用真实模型或改写已有提交。
