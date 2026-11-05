# 会话分层迁移与 Android 补验记录

> 已完成工作的历史记录，不是待执行任务。各批“仍未完成”、五方法端口、NDK clang 和 harness 缺依赖描述均是当时状态；后续批次及 Android 补验保留原样，不能脱离顺序当作当前结论。当前架构见[传输边界](../architecture/transport.md)，现行约束见[开发规范](../guides/development-standards.md)，剩余边界见[节点与会话归属](../design/node-session-ownership.md)。

原文第 7–13 节序号与历史验证结果保留，便于追溯旧引用。

## 7. 分层迁移：共享契约第一、二批

第一、二批增加 `work-core::api`；以下为当时的切分范围，owner 的最新归位状态见第 9 节：

- 项目、会话元数据、协议能力、运行状态、目录/文件、队列、对话及快照等 38 个数据类型已从 `work-client` 提取；包括第二批迁出的 `PendingDialog`、`Snapshot` 和 `WireEvent`。`work-client` 保留原名称的 re-export，类型身份、字段名称、默认值及 JSON 格式不变。
- `NodeApi` 是不绑定网络或执行宿主的最小内部接口，目前包含 `capabilities`、`projects`、`sessions`、`open_session`、`snapshot`。`NodeFuture` 不要求 `Send`，以保留现有原生适配的本地 executor 约束；打开会话可能恢复原生宿主，不代表提交模型输入。快照读取仍委托给既有 workspace，不另建执行宿主或复制所有权。
- `NodeError` / `NodeErrorKind` 提供不含 HTTP、extcore 或 I/O 类型的错误契约。现有 `LocalWorkspace` 通过一个过渡适配实现该接口，原生服务错误转换为分类及安全文案；没有创建第二个 owner。
- 纯事件归一化、消息内容/usage/settlement 转换、`dialog_from_wire` 和 ACP 权限 payload 选择/校验已迁至 core。SSE 解码、消息身份合并、快照重建/重放与投影仍留在 client；第二批没有修改这些算法，也没有改变工具选择语义。共享的权限校验不代表授权、请求仍待审批或响应 ID 有效，实际校验/发送仍由 ACP 适配承担。
- **源码兼容边界**：`PendingDialog` 的既有固有方法直接返回 `AcpPermissionError`，`WireEvent::into_core_event` 直接返回 `EventDecodeError`，不再依赖 `ClientError`；不能宣称这些返回签名完全兼容。client 实现了 `From`，在其错误边界保留原 `Acp` / `Sse` 分类和文案；client 的 `normalize_event` / `dialog_from_wire` 薄包装仍返回原 `ClientError`（后者保留 `Json` 分类）。UI/CLI 中需要确定错误类型的调用点已显式转换。
- 第一、二批结束时，UI、`WorkspaceClient::Local` 和 Server 仍使用原有调用路径；新接口尚未替换这些入口。当时 `LocalWorkspace` 与 ACP 会话绑定/sidecar 管理仍在 `work-client`；自有 RPC/ACP 进程适配实现已按第 8 节迁入 `work-node`。后续仍需处理剩余节点应用接口及执行依赖闭包，再将既有 owner 逐步归位到节点侧，不能另起一个 owner。退出/关机顺序和桌面关闭后的后台生命周期没有改变。
- 既有 `Capabilities` 仍是协议描述，不是逐用户/逐会话的实时能力判断。扩展目录、安装/加载观察和细粒度授权没有因为本次契约提取而实现。

验收覆盖 38 个共享 DTO 的类型身份、序列化兼容、非 `Send` 接口对象、core-only 的 9 个快照/事件/权限测试，以及隔离 RPC fixture 的同 owner 快照和错误行为。fixture 仅补全 `get_messages` 的 `truncated` / `pendingResume` 布尔字段以匹配既有 extcore 快照输出；没有放宽生产解析。另按第二批开始前的源码基线核对了迁出算法、消息合并与快照重建，除路径、错误类型、可见性及格式外未改动逻辑。

前两批四个 crate（`work-core`、`work-client`、`work-node`、`work-server`）的非忽略测试、桌面/CLI 与 Web 的编译检查、core 的 Clippy 和仓库结构检查通过。Android 的 core 交叉编译通过；client 检查因当前 shell 缺少 NDK clang、无法编译 `ring` 而未完成，不计为通过。已安装 runtime 的 smoke 测试维持忽略；这些结果不等于原生 GUI、Web/手机交互或已安装应用已经升级验收。

## 8. 分层迁移第三批：原生执行适配归位

已将自有 RPC/ACP 子进程适配迁入 `work-node::runtime`，没有新增 crate 或执行实例：

- `RpiRpcClient` / `AcpClient` 的协议实现、stdin/stdout/stderr 处理、请求关联、协商状态、权限请求适配、终止通知及清理逻辑现在由 node 提供。迁移保留完整帧写入/取消顺序、超时、事件先于终止错误的交付顺序、权限 ID 类型和最后一个公共句柄释放时的清理语义。
- `RuntimeTransport` 是原生适配端口，不包含客户端投影，也不是 `NodeApi` 或新的网络命令入口；操作 future 不要求 `Send`。原生 `RuntimeEventStream` 仍保留既有 `Send` 约束，不能据此给应用服务 future 强加多线程要求。命令/配置支持判断仍来自原生协商状态，而不是从大类 capability 推断。
- `RuntimeError` 不含 HTTP/client/workspace 类型，保留既有原生错误类别、负载及文案；其中的诊断仍属内部信息，不能跳过应用/Server 侧的脱敏和授权边界。
- client 的两个同名类型仅持有节点运行时句柄，保持 18 个既有公开固有方法的签名和 `ClientError` 返回类型；共同适配负责错误及流项错误转换，不持额外进程、reader 或 pending map，不增加 `Drop` 行为。`AgentClient`、HTTP/SSE、会话投影及消息合并仍在 client。纯 `wire_event` 构造器由 core 提供，client 原名称保留导出。
- 第三批结束时的原生依赖方向为 `work-client → work-node → work-core`，client→node 按非 wasm target 隔离；node 不反向依赖 client，也未引入普通依赖的 HTTP client 或 extcore。core 仍只依赖 serde/serde_json。锁文件仅增加两个本地 crate 的依赖边，未更换外部包或版本。

**第三批结束时尚未完成的 owner 迁移（已由第 9 节推进）**：当时 `LocalWorkspace` 仍在 client，仍拥有 extcore `Service` 及其宿主；ACP Manager 仍协调绑定、sidecar、journal 和节点提供的 ACP 句柄。因此本批不等于全部原生执行 owner 已归位。`LocalWorkspace::connect_session` / `prepare_agent_session` 仍引用客户端连接/投影类型，后续需先解除这部分依赖，再搬迁既有 workspace/Manager/extcore 服务。UI/Server 入口和共享 `Arc<LocalWorkspace>` 未改动，桌面后台生命周期也未重新定义。

第三批验收：

- 四个 crate 非忽略测试 **191 通过、0 失败、4 忽略**；其中 12 个既有底层测试随实现迁移，新增 9 个错误/流映射、终止交付、lag 分类和非 `Send` 端口测试。现有 client 协议/生命周期及共享 owner fixture 保持原调用方式通过。
- 对照第三批开始前的完整源码基线，核对迁出实现及 18 个客户端公开方法；原生实现除模块路径、错误类型、测试 fixture 路径及格式外不变。客户端投影/重放、workspace/ACP sidecar、Server 和 UI host assembly 未改变；独立只读复核未发现具体问题。
- 桌面/CLI、Web 编译通过；core/node all-target Clippy（`-D warnings`）、定向格式及仓库结构检查通过。client Clippy 完成，仍有迁移前代码中的既有告警，不宣称全仓库零告警。
- Android core **及 node** 交叉编译通过；client 仍受当前 shell 缺少 NDK clang、`ring` 无法构建的限制，未计通过。未执行真实 Agent/model、用户会话或 GUI/手机交互验收，已安装 runtime smoke 维持忽略。


## 9. 分层迁移第四批：workspace 应用 owner 归位

既有 owner 已迁至 `work-node::workspace`，不是另外实现一套 workspace：

- 原 `LocalWorkspace` 的 `Inner`、extcore `Service`、registry/目录迁移、项目/会话操作及生命周期锁整体归入 node。ACP Manager、会话绑定、sidecar、journal、generation、恢复边界和权限关联一并迁移；extcore 继续固定 `=0.1.5`，不重做运行、历史或资源加载。
- `work-client::LocalWorkspace` 是兼容访问门面，只保存 `Arc<work_node::workspace::LocalWorkspace>`。保留 61 个原公开固有方法签名和 `ClientError` 返回类型；`connect_session` 的快照、投影及消息身份组装留在 client。`node_owner()` 共享原 Arc，`from_node_owner()` 包装已有 owner，不启动第二个 Service/进程。
- ACP 准备返回 node 的 `PreparedAgent` 绑定句柄，构造入口仍由 Manager 控制。原 Mapped 的跨会话、generation、terminal、权限及操作白名单检查不变；client 用薄 `AgentTransport` 包装成原 `AgentClient`，没有新增 reader、任务或状态副本。准备时仍先订阅再 new/load，恢复仍等待明确 replay completion marker，journal replay/live 切换仍在同一锁内完成。
- 节点应用错误为 `WorkspaceError::{Local, Runtime}`，不依赖 HTTP 或 client。`LocalError` 类型由 node 定义、client re-export；本地访问显式映射回原客户端分类/文案/JSON source。内部诊断仍只供内部使用，`NodeApi` 和 HTTP 的安全错误映射随原实现保留。节点 supervisor 既有的另一 `NodeError` 与 core 的中立 `api::NodeError` 仍是不同边界。
- `NodeApi` 现在由节点 owner 直接实现，client 门面转交同一 owner。Server 普通依赖改为 node，`HttpServer::start` 明确接收节点 owner；桌面组装传 `workspace.node_owner()`。Server 的 client 依赖仅用于隔离测试。认证、Host/Origin、共享撤销、限额及流逻辑没有改变。
- workspace/extcore 继续仅在非 wasm、非 Android 的桌面 target 编译，没有把本机执行隐式带入 Web/Android。原生 future 不强加 `Send`，宿主仍使用既有 executor/LocalSet；桌面退出仍是 drain HTTP → shutdown workspace → 释放 executor/数据锁，后台生命周期未重新定义。

第四批验收：

- 四 crate 非忽略测试 **196 通过、0 失败、4 忽略**（与此前相同的统计口径，包含 ACP 隔离子测试进程报告）。相较本批基线新增 5 个测试，既有 owner 内部测试随实现搬迁；扩充现有 fixture 验证 Arc 身份、节点/client/HTTP 相同 PID、PreparedAgent 同 generation、跨会话拒绝与停止后旧句柄拒绝。没有使用真实 Agent、模型或用户目录。
- 对照本批开始前的完整脏源码备份，核对完整 workspace、ACP Manager/句柄/journal/恢复算法、迁移和 NodeApi 脱敏实现及其迁移测试；客户端全部 61 个原公开签名、connector、消息投影/重放未变。64 个不在本批范围内的基线文件字节未变；锁文件仅调整本地依赖边，没有新增外部包或版本。
- 桌面/CLI、Web 编译通过；core/node all-target 严格 Clippy、定向格式和仓库结构检查通过。client/Server Clippy 可完成，但仍有既有告警，不宣称全仓库零告警。
- Android core/node 编译通过；Android client 仍因当前 shell 缺 NDK clang、无法构建 `ring` 而未完成，不计通过。未进行手动 GUI、浏览器、手机或已安装应用升级验收，已安装 runtime smoke 仍忽略。本批尚未取得独立审查者的复核结论。

**剩余边界**：`WorkspaceClient::Local` 和 UI 仍通过兼容门面访问节点，并未全面替换成扩展后的 `NodeApi`；会话能力/配置的直接原生访问分支仍需逐步收敛。当前 `NodeApi` 仍只是五个 catalog/open/snapshot 操作，不是完整应用入口。协议能力仍不等于 supported/installed/loaded/active/executable 的运行态观察；细粒度授权、Package/Skills 精确选择与观察契约以及桌面后台托管策略都未由此次迁移实现。


## 10. 分层迁移第五批：会话配置经应用服务访问

本批收敛资源面板与协商配置组件，不扩大为完整执行入口迁移：

- core 新增 `SessionConfiguration`（`protocol`、`supportedCommands`、`configOptions`）。这是一个会话作用域、限定配置面的运行时观察，不是节点全局能力、授权、安装目录或实际 loaded/active/executable 资源证明；缺失字段和请求失败不能变成“全支持”。原 38 个 DTO 不变。
- node 增加 `session_configuration(id)`，沿用 lifecycle read lease、Service/ACP Manager 的绑定和准备过程。查询可能按既有行为恢复宿主，但不提交模型输入。RPI 支持集从实际 live host 的命令谓词与既有协议白名单交集产生；ACP 返回原实际协商选项，不合成模型目录。命令列表只覆盖本批配置面，不代表 prompt 等其他能力不存在。
- `set_session_config_option(id, config_id, value)` 在节点侧选择绑定后端：ACP 原样交给 `PreparedAgent` 及原 ACP id/value 校验；RPI 只将 `model` / `thinking` 映射到原有 typed 操作。工具选择仍走既有 `set_active_tools`，没有拓宽 ACP raw commands 白名单。idle 仍是既有 UI 限制，不宣称新加了服务端闲置状态授权规则。
- 本地门面、`WorkClient` 和 `WorkspaceClient` 转接相同 typed 方法；Server 增加受原认证、Origin/Host、共享和 admission 边界保护的 `GET/POST /api/v1/sessions/:id/configuration`，成功配置响应标记 `no-store`。旧服务 404、401 和网络失败原样可见，不回退本机执行或乐观支持。
- 资源面板 `SessionConfig` 不再接收 `native_agents`，目录/状态读取与修改都通过 workspace；未知或读取失败时禁用操作，读支持与写支持分别判定。`NegotiatedConfig` 不再接收 `AgentClient`，实际 id/value 经节点写入；分组选择器的纯展示抽为 `NegotiatedConfigControls`，供隔离 harness 复用。
- 配置请求沿用 `scoped_spawn` 并加入组件加载序号与会话 generation 双门控。相同 generation 的重连不能让旧 GET 覆盖新结果；ACP wrapper 在父面板重载时保持挂载以保留在途 busy。读错误与写错误分开，重连后明确的写入失败不会被忽略或被成功 GET 清掉。父面板的首个配置快照供 wrapper 复用，避免串行重复读取。

第五批验收：

- 四 crate 测试 **202 通过、0 失败、4 忽略**，沿用包含隔离 ACP 子进程报告的既有统计口径；相对本批基线新增 5 个独立测试。覆盖 DTO/缺失观察、RPI 命令与值映射、ACP 分组真实 id/value、错误值/未知 id 拒绝、相同 PID/已有句柄、跨会话配置隔离、恢复/关闭，以及 HTTP 认证、Origin、共享关闭和旧服务 404/401 不回退。
- UI 配置门控 3 个定向测试通过；专用 `agent-backends` WASM harness 重建及自动浏览器 **10 组检查通过**。其中 7 组保留内存 transport 的展示/会话隔离/320–390px 检查，新增真实 workspace 访问 wrapper 的可控 HTTP fixture，动态覆盖 GET 乱序/过期错误、重连时在途写 busy、父面板首载快照复用、写失败保留与重试。fixture HTTP 不等于真实 ACP 或生产 Server；后两者另由 Rust 隔离 fixture 验证。
- 独立只读审查发现 harness 旧接口、重连竞态、父面板重挂及写错误丢失后均已修正；最后一次定向复核未发现这几条路径的新具体问题。没有将未迁移分支当成已验证。
- 桌面/CLI 与 Web 编译、Android core/node、core/node 严格 Clippy、定向格式及仓库结构检查通过。client/Server Clippy 可完成，但仍有既有告警。Android client 仍因缺 NDK clang 无法编译 `ring`，未计通过。附加的通用 `ui-modules` harness 检查仍因其既有缺失 `web_time` 直接依赖而失败；专用 `agent-backends` harness 可独立编译和运行，不将通用 harness 计为通过。
- 与本批脏源码基线对照，原运行时协议、ACP Manager/journal、消息投影/重放、宿主生命周期及授权实现保持不变；主 `Cargo.lock` 字节不变，无新增外部依赖。独立 harness 锁文件补齐先前本地 node 依赖迁移引出的既有包，未升级共同包版本。C 盘满时将本项目部分 `work_ui` 增量缓存移到 D 盘保留，未清理其他项目或用户数据。

**第五批完成时仍未收敛的范围（部分已在第六批收敛）**：主工作台的模型/思考选择与执行/对话等旧原生分支、运行统计视图的 `for_agent(None)` 乐观默认仍在，未被本批宣称修复。`NodeApi` 仍是五方法最小端口，本批 typed 配置入口属于既有 workspace 应用服务，尚未扩充到该 trait。工具空列表的上游实际加载问题、Package/Skills 精确应用/观察契约、细粒度授权和桌面后台托管策略均不变。未运行真实 Agent/模型/用户会话，未做原生 GUI、手机或已安装应用升级验收。


## 11. 分层迁移第六批：主工作台模型/思考配置经应用服务访问

本批承接第五批 typed 配置入口，迁移 `app/workspace.rs` 中由用户操作驱动的主工作台模型/思考配置流程：

- 模型/思考目录加载前先通过 `WorkspaceClient::session_configuration` 观察绑定会话，不再通过 `native_agents`、全局 `Capabilities` 或可选 `TransportDescriptor` 推测配置支持。RPI 目录读取与写入分别按实际命令门控；ACP 和未知协议不能被解释成 RPI 模型/思考目录。
- `config_requests::CatalogAccess` 是该 UI 的纯配置观察映射，不是新的权限或执行边界。配置观察失败、旧服务 404、401 和网络错误可见；清空未观察到的目录并禁用写入，不回退原生访问或全局能力。模型和思考目录在观察成功后仍独立加载，不相互等待。
- 模型/思考写入统一为 `WorkspaceClient::set_session_config_option`，写后及手动刷新核对统一走 `WorkspaceClient::runtime_state`。这几条配置 action 不再持有或调用 `AgentClient`；setter response 仍不能替代权威回读。
- 沿用原 15 秒配置 deadline、`requiresRestart`、模型供应商/原始 id 一致性检查、思考等级原始值、重复点击保护、待核对状态阻止重复写/发送及草稿保留。观察 GET 与两个目录 GET 受 generation、连接 epoch、session id 和加载 ticket 保护；超时仍只停止本地等待，不承诺节点操作取消，也不自动重复 setter。
- ACP composer 分支改由绑定会话协议判定，不再用本机句柄决定远端会话配置面。保持 `COMPOSER_CONTROLS_VISIBLE = false`，没有擅自启用隐藏控件。协议在同一上下文的刷新期间保持稳定，避免未来启用时重挂 wrapper 而丢失在途 busy；首次配置快照供既有 wrapper 复用。
- `ThinkingPicker` 分开临时忙/运行锁与 `write_supported`：未知配置不允许写入，但错误仍可查看和手动刷新，避免把恢复入口一起锁死。已知不支持写入且无错误的触发器保持原禁用行为。

第六批验收：

- 正式应用 WASM 重建后，隔离 HTTP/SSE fixture 的模型/思考选择器自动浏览器 **47 项全部通过**，零 prompt、队列提交和凭据写入。保留原 **38 项**供应商分组/歧义、原始值、双击/忙状态、权威回读、晚到响应、重连、配置超时、`requiresRestart`、草稿及窄屏检查；增加 **9 项**会话观察 401/404/500、观察超时及迟到成功/错误、跨会话/连接 epoch 隔离、思考错误手动恢复，以及全局 capability 与会话观察相反时的门控。
- Browser fixture 新增 configuration GET/POST，沿用隔离会话状态。测试按实际 typed wire 验证 `configId` / `value`，故意使用不带 runtime state 的 ACK，再通过真实 fixture `get_state` 验证写入与读回。测试内部的逻辑命令归一化只用于既有策略与计数，不改变产品传输或伪造状态确认。
- 第五批专用 ACP/配置 harness **10 组**回归通过（内存 transport 展示及可控 HTTP wrapper，覆盖边界同第 10 节）。它与正式应用 fixture 都不是生产 Server/真实 ACP；node/Server 的隔离运行时测试另由四 crate 回归覆盖。
- 四 crate 测试仍为 **202 通过、0 失败、4 忽略**，与本批基线相同，包含既有 ACP fixture 子进程报告口径。本批不改后端；新增两个 UI 纯能力映射单元测试，配置请求 5、picker 9、资源配置面板 3 个定向测试通过。
- 桌面/CLI、Web、Android core/node 检查，core/node 严格 Clippy、定向格式和结构检查通过。UI Clippy 可完成但保留既有告警，本批新增告警已处理。所有 crate、所有锁文件及其余 **380 个**基线文件字节不变；主工作台的消息发送/继续/停止、会话打开与运行事件处理代码未变。
- 本批未取得独立审查，只做源码对照、自查及自动化验证，不将第五批的独立复核当成本批验收。未运行真实 Agent/模型/用户会话，未做原生 GUI/手机或安装升级验收；Android client 缺 NDK clang、通用 UI harness 缺既有 `web_time` 直接依赖的限制仍在，未计通过。

**仍未收敛**：主工作台会话初始化/运行事件后的状态及队列刷新、发送/恢复/执行、观测统计等仍有原生句柄分支；统计视图 `for_agent(None)` 的乐观默认未在本批修复。`NodeApi` 仍保持五方法最小端口；不宣称所有客户端已统一、细粒度授权已实现或资源实际 loaded/active/executable 已观察。单 owner、ACP 协议/Manager/journal、消息投影/重放、宿主生命周期及后台托管策略均保持不变。


## 12. 分层迁移第七批：剩余会话中心入口统一经 typed 端口访问

本批承接第六批，将剩余会话访问、执行、对话与观测入口收敛到节点应用端口。第 9 至 11 节所述五方法端口、UI 原生句柄执行分支及 `for_agent(None)` 乐观默认是各批结束时的历史状态，现由本节更新：

- core 增加纯 `SessionAccess` 观察契约，包含实际绑定运行时的 descriptor、精确支持命令、`snapshot_limit` 和 `dialog_reply`。观察来自 actual runtime，不由全局 capability、协议大类或本机句柄是否存在推断；supported 只描述支持，不等于 authorization，也不证明资源已 installed/loaded/active/executable。
- `NodeApi` 扩展为 55 个 typed 方法，覆盖本批会话中心入口。新增方法保留默认 `Unsupported`，使旧 mock 或仅实现部分方法的适配仍可使用；不能据此宣称所有现有或未来实现都已实现全部方法。端口 future 与端口事件流不要求 `Send`，允许 browser 适配；这不改变第 8 节底层原生 `RuntimeEventStream` 自身的既有约束。
- `work-node` 的既有 sole owner 提供 typed access/dialog 及会话操作，不另建 Service、运行时或权限状态副本。ACP 对话沿用现有 permissions payload 转换，并在节点侧依据 binding、generation 和 current outstanding map 验证当前请求；支持回复不代表请求已获授权，旧请求、跨会话请求及重放不能绕过校验。不扩大 ACP raw commands 白名单。
- Server 增加会话作用域的 `GET /api/v1/sessions/:id/access` 和 `POST /api/v1/sessions/:id/dialogs`，沿用既有认证、Host/Origin、共享与 admission 边界；对话只接收 typed 请求，拒绝 raw 回复入口，不将客户端能力观察当作服务端授权。
- `WorkClient`、`WorkspaceClient` 与本地 `LocalWorkspace` 门面转接 typed ports。统一 connect 根据实际 `SessionAccess::snapshot_limit` 策略取得快照；local 保持 subscribe-before-snapshot，remote 使用事件 cursor 衔接。访问、连接与客户端 projection 分离，没有把消息身份合并、快照重建或重放归入节点 owner，也没有因适配方式不同再建一份会话。
- ACP snapshot 补入的 pending dialogs 仅来自 runtime 当前仍有效的 outstanding 请求；历史 journal 中曾出现过的权限请求不能仅因重放再次成为可回复对话。snapshot 展示与 typed dialog 回复仍分别经过当前绑定和有效性边界。
- UI 的 `workspace`、`runtime_stats`、`message_queue` 不再按 native handle 是否存在选择执行路径，会话初始化、事件后的状态/队列读取及执行操作统一经 workspace typed 端口。能力未知时 fail closed，各操作按精确 commands 门控；删除 `for_agent(None)` 的乐观默认，不将“没有观察到”解释为支持。
- 同地址连接 epoch 改变也重新打开 session，并隔离旧请求、旧流和迟到结果；不因地址未变而沿用失效访问观察。backend catalog 仅在 `Online` 状态加载，连接中或离线不能触发目录访问，也不回退本机执行。
- 保留草稿与既有交互约束：ACK 不是 settlement，不能据此宣告执行结束；attachment 路径明确为纯文本，不暗示已实现二进制上传；`COMPOSER_CONTROLS_VISIBLE = false` 不变。配置仍沿用 15 秒 deadline，超时进入 unknown/待核对状态，只停止本地等待，不承诺节点取消，也不自动重发有副作用的操作。

兼容与范围边界：

- 公开 `AgentClient` 和 `prepare_agent_session` 兼容入口仍保留，但本批 UI 会话中心路径不再使用它们；保留接口不代表重新引入 UI 原生执行分叉。默认 `Unsupported` 仍是端口的明确失败结果，不是自动降级、乐观支持或本机 fallback。
- extcore 继续固定 `=0.1.5`，单 owner、ACP 绑定/journal 与宿主生命周期沿用既有实现。Package/Skills 实际加载的上游问题不在本批修复范围内；精确命令支持不替代资源加载观察或细粒度授权。

第七批验证记录：

- 隔离 fixtures 增加 local、HTTP 和 trait port 路径的 ACP 权限、跨会话隔离及 seq 重放测试；使用隔离运行时，不以真实 Agent、模型或用户会话作为测试前提。
- `work-core`、`work-client`、`work-node`、`work-server` 四 crate 完整回归 **216 通过、0 失败、4 忽略**；UI 完整单元测试 **261 通过、0 失败**。合并日志为 **477 通过**，沿用包含隔离 ACP 子进程报告的统计口径，未将忽略的 installed-runtime smoke 计作通过。
- 最终正式 WASM 的 **172 组浏览器检查全部通过**：配置 47、继续执行 19、历史分页 11、多客户端同步 12、认证 6、运行统计/压缩 10、Inspector 13、附件保护 14、对话 8、新会话能力观察 4、队列 7、明暗主题/宽度矩阵 21。能力观察覆盖 404/503 后显式重选恢复、全局支持但会话不支持 prompt、同地址重连重新观察，以及不支持 dialog 时不得发送 typed 回复。压缩覆盖能力刷新后迟到失败能释放同一请求 pending，队列缺少 accepted 时保留草稿并明确未确认。所有流量使用隔离 fixture；这些组并非真实 Agent/设备验收。
- 专用 ACP/配置 WASM harness **10 组通过**；通用 `ui-modules` harness 的 conversation/queue/settings WASM 编译现已通过，补入其已锁定版本的 `web-time` 直接依赖。桌面/CLI、Web、Android core/node 检查及 core/node 全 targets 严格 Clippy 通过；client/Server/UI 全 targets Clippy 可完成但仍有告警，不计作 warning-free。定向格式与仓库结构检查通过。
- 独立静态审查及定向复核确认修复 HTTP 直连错误限量 snapshot、同 scope 压缩失败 pending、未确认入队文案等问题；最终复核覆盖重连、观察任务重置、当前 ACP pending dialogs 与队列挂载。队列在 workspace 内保持 conversation 相对宽度，在其他页面全局挂载，条件互斥，避免隐藏祖先与重复弹窗。独立复核不是全仓无缺陷证明。
- 验收证据位于 `target/node-finish-0ou38dw2/`（入口 `target/node-finish-path.txt`）：脏源码备份/SHA256、基线差异、完整回归、最终构建来源、浏览器和平台日志。最终 WASM 构建记录的 **221 个源码哈希全部匹配**。主锁文件仅新增 core 到既有 `futures-core` 的依赖边，没有新 package 或共同包版本升级。验证中 C 盘满后仅将本项目旧 `work_ui` 增量缓存迁到证据目录保留，未删除用户数据或清理其他项目。
- Android client 仍受当前环境缺少 NDK `clang.exe` 的限制，不能计为通过。未进行原生 GUI、手机、已安装应用升级或真实 Agent 交互验收；细粒度授权与 Package/Skills 上游加载/观察契约不在本次完成范围。

## 13. Android 工具链补验与开发约束固化

上节保留的是第七批当时的失败记录。本次后续排查确认本机已安装 NDK `28.2.13676358`，`clang.exe` 实际存在；原始 `failed to find tool "clang.exe"` 是直接 Cargo 进程未发现/未配置目标工具链，并非 NDK 文件缺失。

- 新增共用 `scripts/ci/android-toolchain.ps1` 和 `scripts/ci/check-android.ps1`，既有 APK 构建复用该工具链。按 target 设置编译器、archiver、API 24 flags/linker 与进程 PATH；不改用户/系统环境，不移除 TLS，不改 Rust 依赖。成功/失败均恢复进程环境，保留 cc-rs 的 global、TARGET、underscore、hyphen flags 叠加语义。
- 使用已安装 NDK 在 **Windows PowerShell 5.1** 实际执行 `--locked --offline`：**ARM64 与 x86_64 的 `work-client`、`work-ui --features mobile` 均检查通过**。现有 `render_project_file` 未使用告警仍保留，不宣称 warning-free。
- 隔离工具链测试覆盖含空格路径、固定版本发现、显式 override、缺 compiler 拒绝、两种 ABI/变量拼写、C/C++ flags 及成功/异常环境恢复。另对两种 ABI 实际用 NDK 编译 C、链接 Rust cdylib 并核对 Android ELF machine；独立复核另验证四层 flags 同时存在的 C/C++ 宏与异常恢复。
- 证据：`target/android-toolchain-gxdz5t56/`，入口 `target/android-toolchain-path.txt`。含新的脏源码/SHA256 基线、双 ABI 编译日志、工具链测试、独立审查与完成记录。此前失败日志不覆盖、不改作成功。
- 会话分层与一致性要求已归入 [开发规范](../guides/development-standards.md) 的硬约束；Android 操作方法见 [CI 构建](../guides/ci-builds.md)。

因此解除的是 **Android client/mobile UI 的交叉编译检查阻塞**，不是完整 APK 构建/签名、安装升级、模拟器、真机或真实 Agent 验收。Windows 的 check/probe 成功也不代表依赖 PowerShell 7 的 APK 脚本已在 PowerShell 5 验收；Linux/macOS host 分支尚未在本次环境执行。

