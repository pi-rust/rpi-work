# 开源与扩展性审查

> 源码审查快照：下列缺陷、行号和建议仅对应审查时工作区，未逐项重新验收；不应直接作为当前完整问题清单。现行分层约束见[开发规范](../guides/development-standards.md)，资源加载缺口见[专项验证](rpi-session-resource-validation.md)。保留原报告，不据此宣称问题已经解决。

## 结论与范围

当前分层适合继续建设，但距离“第三方不改核心即可完成扩展的开发、分发、使用和升级”仍有明显距离。最大缺口不是插件类型不够多，而是正式产品未完成扩展运行闭环，以及第三方入口仍受宿主硬编码和存储可靠性限制。

本次为当前工作区源码审查，不是已发布二进制审计；工作区已有其他未提交修改。仅新增本报告，未修复业务代码、选择许可证、安装插件或发布制品。下列行号以审查时源码为准。

优先级：P0 为开源许可阻断；P1 为数据安全、确定性功能缺陷或“强扩展”目标的关键路径；P2 为规模化、兼容性和生态工程。不应把目标缺口描述成当前已经存在的可利用安全漏洞。

## 应保留的设计

- `work-core` 与 UI/transport 分离，已有纯状态投影；`AgentTransport` 已提供可注入适配面，不必另造一个包揽一切的 Plugin Trait。
- Agent、UI 分开建模，允许 agent-only、ui-only、combined。
- desired、observed、applied 与设备 UI preference 分离；未知不冒充未安装，本地操作不冒充节点执行成功。
- 不可变版本、固定 pins、导入先校验后修改，以及运行期间默认锁定配置。
- 受控声明式 UI，不接受插件任意 HTML/JS/CSS；普通第三方工具仍有通用工具卡兜底。
- 已有真实原生扩展 Demo、失败清理与模块测试资产，可作为正式接入的基础，但不能当作正式功能已完成。

## 一、优先调整清单

### 1. P0：先补项目许可证与分发许可

**证据**：受版本控制的文件中仅见第三方 vendor 许可证，未见项目级 LICENSE/COPYING；根 `Cargo.toml:1–3`、`crates/work-core/Cargo.toml:1–6` 等没有项目许可证元数据。`scripts/ci/package-build.ps1:45–106` 桌面包未明确带入第三方许可正文。

**调整**：由权利人选定开源许可证，明确代码、文档、图片和示例范围；各 crate 补 license/license-file；安装包随附第三方许可与清单。增加 CONTRIBUTING 和 SECURITY，入口复用现有开发规范。许可证选择不能由工具擅自决定。

**验收**：外部开发者能明确判断能否修改、再分发、发布扩展；打包测试确认安装包包含必需许可文件；锁文件依赖有许可证策略检查。

### 2. P1：正式扩展管理仍未接通，优先做运行闭环

**证据**：`crates/work-client/src/extensions.rs:9–21` 的 capability 全为 false，catalog 返回 Unsupported；`crates/work-core/src/extensions.rs:135–181` 目前仅有能力和 observed/applied 类型。`docs/guides/extension-demo.md` 明确真实安装/加载与 live UI 仅在隔离 Demo 中验证。

**调整**：先核实上游真实 API，按“只读目录和观测 → 显式授权的安装/卸载 → 加载/重启要求 → 会话配置应用确认”逐项接入。安装、启用、运行中、applied 必须独立；具备幂等操作身份、revision 冲突、失败诊断、回读及恢复语义。不要求或承诺原生动态库热卸载；宿主不支持时明确需要重启。

本机和远程分别协商能力，远程节点承担编译/执行，Web/手机不加载本机动态库。不得把设计文件直接当上游安装清单，也不得猜测网络端点。缺失的上游能力应列为上游协议工作项。

**验收**：一个仓库外的第三方示例可经正式应用发现、授权安装、加载、固定到会话，节点确认 applied 后执行；失败、取消、断线、重试、重启和卸载后状态均可解释。旧节点继续明确降级为 Unsupported。

### 3. P1：默认 Skill 模板发布会生成非法 ID

**证据**：`apps/work-ui/src/features/meta_agent/mod.rs:346–357` 生成 `skills/review/SKILL.md`；`:561–562` 对目录名调用只按点分隔的 slug，派生技能 ID 为 `skills/review`。`apps/work-ui/src/features/resources/library.rs:1555–1561,1637–1640` 不允许 ID 包含斜线。

**影响**：结构校验和快照可以通过，发布时被资源库拒绝，直接破坏默认作者流程。

**调整与验收**：身份从资源 ID 或明确的技能声明生成，不从文件路径临时猜测。回归覆盖默认创建 Skill → 校验 → 快照 → 发布 → 导出重导入，验证 ID 稳定、合法且无碰撞。

### 4. P1：元Agent源码可能被静默覆盖；发布回执不代表持久化完成

**证据**：

- `apps/work-ui/src/features/meta_agent/mod.rs:294–304` 读取/解析失败返回空库；`:315–340` 吞掉浏览器读写错误和原生写入错误，原生 effect 随后直接覆盖文件。
- `apps/work-ui/src/features/resources/library.rs:329–333` 直接覆盖整份资源库，虽有错误提示及读取失败保护，但没有可靠替换协议。
- `apps/work-ui/src/app/workspace.rs:3253–3255` 的发布回调只确认内存导入；资源库 `library.rs:411–417` 即返回成功；`meta_agent/mod.rs:827–839` 据此登记 release、提示发布成功，落盘由另一个 effect 完成。

**调整**：区分不存在、损坏和不可读；读取失败禁止自动覆盖。提取资源存储层，原生采用经平台验证的原子替换、备份恢复与明确错误回执，Web/Android 明确 localStorage 失败。发布记录依据持久化成功登记，跨两个存储需要可恢复/幂等的提交语义，而非假设同时成功。

**验收**：损坏 JSON、磁盘满、权限拒绝、写入中断、localStorage 配额不足，均不丢失上一份有效源码；不得显示持久化发布成功；重启后不存在“有 release、无对应包”的不可解释状态。

### 5. P1：工具名和斜杠命令硬编码，第三方能力进不来

**证据**：

- `meta_agent/mod.rs:429–438` 只允许 5 个工具名，`resources/library.rs:1564–1582` 允许 10 个，两处契约不同，且都拒绝自定义第三方工具声明。
- `app/workspace.rs:2391–2432` 只识别 `/btw`、`/compact`、`/new`、`/help`，拦截其余斜杠输入；即使节点具有插件命令，该入口也没有发现/路由机制。

**调整**：工具 ID 语法校验、节点可用性和授权分开；合法但未知的声明可保留为 unresolved，不能直接执行。命令表区分宿主命令、经协议发现的节点命令和普通文本，明确命名冲突、参数、帮助、自动补全和路由；未协商的命令仍拒绝，不直接把任意字符串转成特权调用。

**验收**：自定义命名空间工具可在作者页面和资源库得到一致结果并往返保存；有能力的节点命令可发现和执行；内置命令不被覆盖；旧节点和不存在的命令不被伪报成功。

### 6. P1（目标关键路径）：UI slot 是静态标签，不是真实扩展点

**证据**：`crates/work-core/src/extensions.rs:60–88` 定义 4 个 slot 和 3 种 widget；`apps/work-ui/src/features/extensions/mod.rs:85–122` 全部在预览面板渲染，未挂载到对应业务位置。`features/conversation/timeline.rs:430–450` 的计划富展示按固定工具名识别。

**调整**：先正式落地 conversation-card 与 session-inspector 的只读 contribution，再考虑 composer-action/settings-section 的受控交互。用宿主贡献注册/分派表承载挂载位置、数据契约和显示策略，减少在 workspace/timeline 不断增加插件专属 match。不能仅因为当前 enum 封闭就否定它：widget 类型仍由宿主批准，扩展作者只组合受控组件。

live 数据需带 node/session/run/tool-call/extension/contribution 关联、schema、序号及边界；处理挂载/卸载、切换会话、取消订阅、断线快照、重放和节流。UI 隐藏不能取消 Agent；不支持的 UI 应回退通用展示。未知扩展事件不应被强行解释成核心运行状态，可复用保留原始 WireEvent 的通道新增有边界的分派。

**验收**：第三方声明无需修改宿主即可在真实插槽展示进度；双会话、双节点和连续两轮无串数据；重复事件不重复显示；隐藏/恢复获取最新状态；未知 UI 不阻止已支持的 Agent 执行；卸载后无悬挂订阅。

### 7. P2：契约分散在 UI，版本与兼容规则不够完整

**证据**：包模型、解析和存储在 `resources/library.rs`，元Agent自有校验，扩展内层清单解析在 `features/extensions/mod.rs`；`work-core` 已有 ExtensionManifest，但未覆盖整个设计包。`library.rs:112–188` 恢复时不检查格式版本，`:191–199` 写出 version 3。扩展/包版本基本只接受三段数字，未形成宿主/SDK/ABI/平台兼容协商。

**调整**：把可序列化包契约、纯校验、身份及迁移移至现有 `work-core` 的资源模块；UI 只保留交互，存储/安装留在对应服务层。先不为了目录拆分新增大量 crate。发布机器可读 schema、错误码和契约 fixtures，支持 CLI 离线校验。执行制品的兼容声明与当前惰性设计格式分开，明确宿主版本、SDK/ABI、平台架构、必要/可选能力及依赖关系。

继续保留安全 schema 的严格校验，不为“向前兼容”直接关闭 deny_unknown_fields。采用显式版本协商、可选 UI 降级和版本迁移；未知未来存储版本只读/拒绝覆盖，避免旧客户端静默降级重写。是否支持预发布 SemVer、依赖解算和锁文件应先形成稳定规则，再实现。

**验收**：同一包在元Agent、UI 导入和 CLI 中校验一致；旧版数据可迁移，version 999 不被重写；兼容性不足有具体原因；Agent 和可选 UI 兼容性独立判定。

### 8. P2：信任模型必须先于开放安装

**证据**：`work-core/src/extensions.rs:27–35` 权限仅为请求；`docs/guides/extension-demo.md` 明确 cdylib 使用当前用户权限运行，不是 OS 沙箱。`features/extensions/mod.rs:15–34` 兼容旧惰性设计，并没有对全部 sources 路径做统一解包验证。

**调整**：区分源码设计包、已构建原生包和声明式 UI 的信任级别；原生包要求明确授权与来源/摘要校验，构建脚本同样视作任意代码。权限声明不等于有效限制；若承诺细粒度隔离，需要节点侧真实代理/OS 沙箱等技术，不能靠 manifest 字符串。优先复用上游签名/安装能力，不复制另一套包管理器。

解析前检查原始输入字节数（当前粘贴入口 `library.rs:629–632` 先解析再做包大小检查）；统一校验全部 sources 的路径、类型、重复/冲突和数量，未来落盘再防链接逃逸。当前设计导入不执行、不按 source key 解包，因此这些是输入加固及未来执行前置要求，不是当前任意执行/写文件漏洞。

**验收**：撤销权限或断开节点后旧 UI action 不可复用；跨节点/会话请求被拒绝；恶意路径、超大输入、签名/摘要不匹配和不兼容制品在产生副作用前被拒绝。

### 9. P2：主题和作者流程还缺维护生命周期

**证据**：`shared/theme/appearance.rs:142–154` 同 ID 不同内容拒绝，最多 12 个自定义主题；界面暂无对应替换/删除流程。`meta_agent/mod.rs:390–411` 按源码总量限制 200 KiB，但发布附加完整 testSnapshot（`:827–828`），资源库又按整个 JSON 限制 200 KiB（`library.rs:372–373`）。

**调整**：主题增加显式替换、删除、取消和当前主题回退，不放开任意 CSS。作者校验以最终发布 payload 计算预算，避免源码和快照重复造成“结构通过却无法发布”；区分源码/包体大小，并提供可操作诊断。

**验收**：12 个主题后能删除再导入；主题更新可取消；接近边界和高转义源码在校验阶段即可准确反馈发布限制。

### 10. P2：把第三方开发与兼容性测试加入正式交付

**证据**：`.github/workflows/build.yml:25–52` 已运行结构、fmt、workspace test、clippy，具有多平台构建；但真实 extension-demo 是独立 workspace（`examples/extension-demo/Cargo.toml`），根 workspace test 不覆盖它。当前 workflow 未接入扩展模块浏览器回归、示例兼容验证、依赖安全/许可检查；发布任务 `build.yml:183–196` 的第三方 Actions 仍使用可移动引用。

**调整**：

- 复用现有 rpi-plugin-sdk 与真实示例，不另造 Agent ABI；给出 Work 的设计/UI 契约包和模板。
- 提供“创建 → 离线校验 → 本机隔离测试 → 打包 → 正式应用使用”的快速路径，以及纯 Agent、纯 UI、组合包三套最小例子。
- CI 增加示例编译/测试、协议 fixtures、schema 快照和至少一条正式插槽浏览器链路；真实宿主 smoke 单列固定版本，可定时/发布前执行，不默认调用付费模型。
- 公布宿主、extcore、plugin SDK/ABI、客户端 UI schema 和节点协议的兼容矩阵。不要按彼此版本号不同就误判不兼容。
- 加入依赖许可/安全策略，第三方 Actions 固定提交 SHA，签名发布设置受保护审批。

**验收**：外部开发者仅按公开文档和已发布依赖即可完成示例，不依赖相邻私有仓库；新增常规插件不修改核心源文件、不要求重编客户端；不兼容插件失败可诊断且不会破坏其他扩展。

## 二、建议的扩展面与职责

| 扩展面 | 当前基础 | 应补能力 | 主要归属 |
| --- | --- | --- | --- |
| Tools、Skills、Agent hooks / Provider / MCP 等节点资源 | 执行节点负责，Work 有资源声明与部分目录 | 按上游真实能力逐项发现、配置和观测；不要在 UI 复制运行时 | rpi/节点；Work 负责适配 |
| 节点插件命令 | Work 入口仅内置命令 | 目录、帮助、自动补全、明确路由与冲突规则 | work-client + command UI |
| UI contribution、工具结果富展示 | 静态预览和通用工具卡 | 真实 slot、关联事件、受控动作与降级 | work-core 契约 + work-ui 宿主 |
| 主题 | 受控 token 与 JSON 导入导出 | 更新、移除、兼容和作者预览闭环 | shared/theme |
| 自定义 Agent/transport | AgentTransport + CLI RPC/ACP/HTTP | 若要桌面可选择，先定义 workspace 与 agent 的能力组合和连接配置 | work-client + app/connections |
| 项目文件预览器、审阅面板、其他 UI 扩展 | 当前属于内置 feature | 确有用例后新增受控 contribution；不承诺加载任意前端组件 | work-ui |

`WorkspaceClient` 当前是 Local/Remote 的封闭 enum（`crates/work-client/src/workspace.rs:15–21`），而 `AgentClient::from_transport` 已可注入实现（`transport.rs:223–230`）。这不是现有功能错误；若开源定位包含自定义 workspace 后端，应再提取所需的 workspace 扩展接口，而不是立即把所有本机文件/Git 服务都改成通用插件。

## 三、实施顺序和发布门槛

1. **基础可靠性与许可**：选许可证并补分发文件；修 Skill ID、存储覆盖及发布回执；统一工具声明校验。每项先加能使旧逻辑失败的回归。
2. **契约收敛**：抽纯包契约/校验/迁移，公开 schema、能力与兼容规则；落实只读目录和节点观测。
3. **Agent 最小闭环**：复用已核实的安装/授权/加载机制，接 desired → applied 和回读、失败恢复，再开放正式管理入口。
4. **UI 最小闭环**：先只读 slot 和结构化数据，再受控 action；接命令发现和路由。把真实 Demo 的验证思路迁移到正式链路，不把开发 token/适配器直接带入产品。
5. **生态可维护性**：第三方样例、兼容矩阵、CI、资源更新/移除和诊断。确有规模后再考虑插件市场、自动更新、依赖解析及隔离运行时。

可以先以“设计资源与受限扩展预览”定位公开源码，但若要宣传“强插件平台”，至少应达到：第三方仓库不改核心，能通过正式客户端完成安装、执行、UI 展示、固定版本和卸载，且失败不丢数据、权限边界清晰、旧节点可降级。

## 四、本次实际验证

- `cargo test -p work-core -p work-client --lib --locked --offline`：62 通过，1 个内部子进程 fixture ignored，0 失败。
- `node --test tests/structure/check-structure.test.cjs`：4 通过。
- `node tools/review/check-structure.cjs`：通过（在本报告写入前运行）。

未运行完整 workspace/UI/WASM 构建、浏览器回归、真实插件 Demo、远程节点安装、真实模型、发布包许可证扫描或平台签名验证。上面具体业务缺陷基于代码路径核对，尚未补充复现测试或修复，不应以本次库测试通过否定这些发现。
