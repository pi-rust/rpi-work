# 开发规范

本文约束后续新增与修改代码。当前已实现范围以源码及 [transport 架构](../architecture/transport.md)、[Agent / UI 扩展架构](../architecture/extensions.md) 为准；设计目标、类型预留、原型与测试 fixture 不构成能力已实现的证据。

## 1. 代码职责与依赖

- `work-core`：传输与 UI 无关的契约、状态与投影，不依赖 Dioxus 或具体网络客户端。
- `work-client`：协议适配与能力边界。工作空间通过 `WorkspaceClient` 区分本机与远程；Agent 操作沿用对应 Agent client，不能因 UI 需要而发明 wire 方法。
- `work-node`：原生本机运行时生命周期、数据目录与锁、宿主解析及退出清理，不把桌面本机悄悄替换成回环 HTTP 服务。
- `work-ui/features/*`：领域交互与宿主控制的渲染；跨功能协调放在 `app/`，可测试的状态判断优先抽为纯函数。
- 正式应用不得依赖 `design/prototype/`、测试 fixture、邻接仓库或仅本机存在的开发资源。测试 harness 不进入正式安装包。
- 不为尚无真实协议的功能创建通用 Plugin Trait、独立服务或动态加载器；先确认上游契约，再增加最小适配。

## 2. Agent 与 UI 扩展必须独立

扩展设计允许 `agent-only`、`ui-only`、`combined` 三种形式。组合包可以共享发布身份，但两种能力不能共用一个含义不明的“已启用”状态。

| 事项 | 必须遵守 | 禁止混同 |
| --- | --- | --- |
| Agent 执行 | 执行节点/宿主负责加载、权限和执行 | 客户端显示了卡片 = Agent 已加载 |
| 纯 Agent 扩展 | 不需要自定义 UI；可使用通用工具卡 | 缺少 UI = Agent 不可用 |
| 纯 UI 扩展 | 当前只提供客户端受控预览，不加入 Agent pins | 必须安装到节点才能显示 UI |
| UI 显示偏好 | 以设备、扩展版本、contribution 为边界 | 隐藏 UI = 停用 Agent 或撤销权限 |
| 工具/权限声明 | 只是请求和设计元数据 | 清单中的工具已注册、请求的权限已获授权 |

当前 `extension.design.json` 是 RPI Work 的**设计格式**，不是 rpi 原生安装清单。不能把设计导入、源码展示、结构校验、测试快照或个人发布包装成实际安装/编译/执行成功。

## 3. Desired、observed、applied 与 UI preference 分离

- **Desired**：客户端表达期望的会话配置，固定扩展 `(id, version)`；使用既有 node/session scope，不以显示名识别资源。
- **Observed**：来自目标执行节点的目录/安装/运行观测，必须带节点身份。没有观测时保持 unknown，不能默认“未安装”或“空闲”。
- **Applied**：节点确认接受的实际会话配置，与 desired revision 分开；本地复选框、请求提交成功或导入成功都不能产生 applied 记录。
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
- 这只是客户端一致性保护，不能替代未来节点的 revision 校验、权限检查和原子应用边界。

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

- 扩展提供数据，宿主提供渲染器；当前只允许受控 `text`、`fields`、`progress` widget。
- 禁止扩展提供任意 HTML、JS、CSS、URL、动态组件、事件处理器或可执行 action；文本经正常 Dioxus 插值转义，不使用 raw HTML 渲染扩展内容。
- widget、slot 和 schema 使用明确 allowlist；限制条目数、文本长度和数值范围。未知字段不能被静默执行或解释为权限。
- `composer-action` 等 slot 标签目前只是设计意图；未实现真实挂载与动作协议前，不展示为可执行入口或实时状态。
- 未来新增动作协议必须明确节点权限、会话归属、关联 ID、版本兼容及失败反馈，不能让 UI 可见性成为授权检查。

## 7. 测试与 review 最低要求

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
