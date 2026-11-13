# Clarity · 工作区优先 / 黑白默认

这是 Work UI 的**独立高保真静态原型**，不是已发布客户端，也不是导入主题 JSON 后的效果。所有节点、会话、模型、用量、资源数量和 diff 均是本地演示数据；没有连接真实节点、模型或原生桥接。

当前视觉层暂不使用毛玻璃或背景模糊，恢复直接、清晰的白色工作面与灰色边界。左侧会话导航、中间消息区和右侧文件 inspector 各自保持清楚的空间边界；视觉回退不改变节点、会话、权限、资源和供应商的状态边界。

## 当前设计原则

**移除原型额外添加的顶部“新建会话”大按钮与“工作台”导航。** 对话本身就是工作区；新建入口归属项目文件夹图标，展开箭头仍只负责折叠，已有会话仍直接点击打开。原型文件夹点击只展示节点 / 项目作用域说明，不真实创建，也不切走当前会话或丢失草稿。

1. **工作区优先**：一条顶栏、一个工作面；左侧恢复既有约 272px 尺度并可收起，右侧上下文默认关闭。会话区保留“最近的会话 → 当前项目与会话 → 项目树 / 会话”、折叠、项目文件夹新建 / 项目操作与会话操作入口，底部保留归档。正文阅读宽度上限 900px，不为填满屏幕而无限拉长一行文字。
2. **常用操作常驻**：模型、思考等级直接显示在输入器底部，桌面 / 手机均可直接打开选择器，不经过“配置”。附件 / 会话能力 / 其他设置放进 `+` 更多入口。
3. **手机切会话是一级操作**：点击顶栏会话标题 / 下拉箭头，直接打开会话切换器，不必先进入项目导航；默认不聚焦搜索输入，避免打开时立刻弹键盘。Ctrl / Cmd + K 则直接聚焦搜索。
4. **展示方式放进设置**：在 **设置 → 界面与显示 → 会话展示**选择“按节点分组 / 平铺会话”；侧栏和手机切换面板没有排列 tab。默认按节点，只在原有项目树外保留必要节点标题；平铺时侧栏仍保留项目树并标注节点，不改成新卡片列表。手机切换器遵循同一偏好，平铺按示例最近活动跨节点排序，每条仍显示 `节点 · 项目`。顶栏始终显示当前节点与项目。
5. **默认黑白灰**：黑色主操作，白色工作面，灰色边界和选中态，不再以蓝色为默认强调色。绿色在线 / 完成、Git 增删等有语义的状态色保留；不是装饰性配色。
6. **项目文件统一入口**：顶栏只保留一个“项目文件”入口；项目文件面板固定在右侧，作为 Apple 风格的 inspector，同时展示当前项目目录和工作区改动。点击改动文件在侧栏内打开只读 Diff，不再为“文件改动”单独设置顶栏入口。会话正文的“3 个文件有变更”摘要也只打开右侧 Diff，不跳离当前工作区。
7. **不靠缩字号塞内容**：工作区正文 15–16px、输入 16px；主要手机操作触控区域至少 44px。320px 下长模型名可省略，但保留完整 title / 可访问入口；思考等级和发送仍可见。短手机视口保留两行输入布局与两个常用选择器；较宽短视口可使用单行布局。

## 截图

当前截图均在 `screenshots/settings-collaboration/`：

- [桌面工作台 · 1440 × 1000](screenshots/minimal-commands/01-workspace-desktop.png)
- [手机工作台 · 390 × 844](screenshots/minimal-commands/05-workspace-mobile.png)
- [窄屏工作台 · 320 × 844](screenshots/minimal-commands/06-workspace-mobile-320.png)
- [手机会话切换 · 按节点](screenshots/minimal-commands/12-session-switcher-grouped-mobile.png)
- [手机会话切换 · 平铺](screenshots/minimal-commands/13-session-switcher-flat-mobile.png)
- [桌面会话列表 · 平铺](screenshots/minimal-commands/14-sessions-flat-desktop.png)
- [收起左导航](screenshots/minimal-commands/07-workspace-focus.png)
- [桌面 · 1280 × 800](screenshots/minimal-commands/10-workspace-desktop-1280.png)
- [手机短视口 · 390 × 500](screenshots/minimal-commands/11-workspace-short-mobile.png)
- [上下文抽屉](screenshots/minimal-commands/08-context-on-demand.png)
- [附件与更多](screenshots/minimal-commands/09-configuration-on-demand.png)
- [项目文件侧栏 · 桌面](screenshots/settings-collaboration/37-project-files-desktop.png)
- [项目文件侧栏 · 手机](screenshots/settings-collaboration/37-project-files-mobile.png)
- [项目文件侧栏内 Diff · 桌面](screenshots/settings-collaboration/38-project-file-diff-sidebar-desktop.png)
- [项目文件侧栏内 Diff · 手机](screenshots/settings-collaboration/38-project-file-diff-sidebar-mobile.png)

右侧 Diff 查看器是大尺寸文件 inspector，支持同时打开多个改动文件并通过 Tab 切换；切换文件只更新侧栏内容，不跳转主工作区。
- [变更视图](screenshots/minimal-commands/04-changes-desktop.png)
- [资源中心](screenshots/minimal-commands/02-resources-desktop.png)
- [外观设置](screenshots/minimal-commands/03-settings-desktop.png)
- [会话展示设置 · 桌面](screenshots/minimal-commands/18-session-display-settings-desktop.png)
- [会话展示设置 · 手机](screenshots/minimal-commands/19-session-display-settings-mobile.png)
- [项目文件夹新建入口 · 作用域预览](screenshots/minimal-commands/20-project-folder-entry-desktop.png)

初版三栏截图保留在 `screenshots/` 根目录；`screenshots/workspace-first/`、`screenshots/node-monochrome/` 与 `screenshots/sidebar-preserved/` 为先前版本，只供对比。当前版本移除了会话排列切换条，左侧以既有产品结构为准。

## 全屏、会话状态与详细命令

- **设置 tab 点击区域**：外观与主题、会话展示、底部状态栏的 tab 使用更大的上下与左右 padding，桌面和手机均保持至少 44px 的可点击高度，不以缩小文字解决拥挤。
- **工作协同**：原型按生产 `features/settings/devices.rs` 的实际边界展示“连接其他设备”“允许远程协助”“允许 Web 访问”“最近连接”“被访问的设备”。桌面保留远程协助与 Web 访问的独立开关；移动端是连接客户端，不展示远程访问开关，只提供“扫码连接”和“输入地址”。连接节点、Web 访问、远程协助、会话权限仍是不同权限边界；不把资源安装、会话迁移或文件共享合成一个开关。
- **资源安装**：点击每个 Package / Skill / Extension 的“查看详情”，统一进入来源、目标、版本和权限确认。当前节点资源显示“已安装”；发现资源显示“安装到当前节点”；本地设计显示“导入到本地设计”。资源安装不自动加入当前会话，原型按钮不连接节点、不写目录、不伪造成功。
- **资源列表**：资源以单列列表行呈现，一行一个资源，不再使用卡片网格；每行只有资源类型图标，不再有每行重复的更多菜单图标。Packages 行显示组成与作用范围，Skills 行显示适用领域与触发方式，Extensions 行显示运行状态与权限边界，左侧强调色区分类型；移动端改为堆叠列表行。远程 GitHub/API 目录后续只展示接口真实提供的名称、描述、下载量、更新时间和来源，不补 icon、标签或权限数据。
- **供应商设置**：原型按生产 `features/settings/providers.rs` 展示当前执行节点的供应商定义、Base URL、模型数量、凭据是否已配置和当前默认状态。供应商定义、API Key、模型发现和当前会话选择分开；API Key 不回显、不在客户端保存，保存定义不测试 URL，获取模型列表需要活动会话。
- **顶栏全屏**：右上角全屏图标调用真实浏览器 Fullscreen API，可进入 / 退出，Escape 或浏览器外部退出后同步按钮状态。不隐藏输入器，不冒充既有“专注阅读”。浏览器不支持时禁用并说明；拒绝请求时提示失败，不伪造全屏。原生桌面仍需接入宿主窗口 API 并单独验收。
- **侧栏状态只用圆点颜色**：运行中绿色，待授权琥珀色，异常红色，完成 / 空闲 / 排队等为灰色；不显示状态文字、图标，也不增加一行。具体状态通过悬停提示及会话按钮的无障碍描述保留。选中态仍由背景 / `aria-current` 表示，不能把选中与运行混为一谈。手机会话切换面板保留文字和图标以便辨认。
- **简洁 / 详细命令切换**：详细模式收敛为“命令一行 + 结果”，耗时与状态合在命令行右侧；长命令自然换行，原文不裁剪。只保留一层容器和一块非空输出，不再重复显示命令 / stdout / stderr 标题、空输出框及说明段落。完整 JSON 参数、工作目录与成功退出码放进按需展开的“详情”；失败退出码仍直接可见。切换只展开命令，不批量展开文件读取，也不影响用户手动查看其他工具。切换仅改变内存展示，不执行命令、不改变模型、会话或草稿；简洁模式也不隐藏失败状态。
- 所有状态、命令、路径、stdout / stderr、耗时和退出码都是固定演示数据；示例失败不是当前仓库编译结果。无真实重试、授权或执行入口。

### 可实现性的源码依据与边界

| 内容 | 现有接入点 | 实施约束 |
| --- | --- | --- |
| 消息与工具卡 | `apps/work-ui/src/features/conversation/timeline.rs` 的 `ToolCallView` | 已有工具状态、完整参数、输出、耗时、错误提示及独立折叠；重设计复用真实数据，不导入本原型 fixture。 |
| 命令展示 | `features/conversation/command_preview.rs` 的 `CommandPreview` / `command_argument` | 只把明确的非空 `command` 字段视为命令；保留完整原文和现有复制 / 展开能力，不从工具名猜命令。 |
| 简洁 / 详细偏好 | `shared/theme/appearance.rs` 的工具展示设置及 `shared/theme/tool-presentation.css` | 复用既有 `tool_rendering` 与主题偏好，不新增另一套持久化设置或覆盖已保存选择。 |
| 会话状态 | `features/projects/management.rs` 的 `sidebar_activity_status` / `switcher_session_status`，`app/workspace.rs` 的 `sidebar_activity` | 依真实运行态、结算及恢复状态显示；`idle` 不等于完成。圆点只改展示，不改状态来源、作用域或权限。 |

`work_core::MessageBlock::ToolCall` 直接提供的是工具名、参数、输出和 `ToolStatus`；并非每个工具都结构化提供工作目录、退出码或独立 stdout / stderr。生产接入只展示明确提供的字段；缺失显示“未提供”或省略，不能把工具完成当作退出码 0，不能把混合输出猜成 stderr。布局和交互可以落地，但这些附加字段需要按实际工具结果契约接入。现有 `workspace.rs` 的 `conversation_focus` 会暂时隐藏输入器，**不是浏览器 / 原生窗口全屏**。

新增截图：

- [侧栏圆点状态 · 桌面](screenshots/minimal-commands/21-sidebar-session-states-desktop.png)
- [详细命令 · 桌面](screenshots/minimal-commands/22-detailed-commands-desktop.png)
- [失败命令与错误输出 · 桌面](screenshots/minimal-commands/23-failed-command-desktop.png)
- [详细命令 · 手机](screenshots/minimal-commands/24-detailed-command-mobile.png)
- [圆点状态 · 手机侧栏](screenshots/minimal-commands/25-sidebar-session-states-mobile.png)
- [浏览器全屏](screenshots/minimal-commands/26-browser-fullscreen-desktop.png)
- [详细命令 · 320px](screenshots/minimal-commands/27-detailed-command-mobile-320.png)
- [工作协同设置 · 桌面](screenshots/settings-collaboration/28-collaboration-settings-desktop.png)
- [工作协同设置 · 手机（扫码 / 输入地址）](screenshots/settings-collaboration/29-collaboration-settings-mobile.png)
- [Packages 列表](screenshots/settings-collaboration/30-packages-list-desktop.png)
- [Skills 列表](screenshots/settings-collaboration/31-skills-list-desktop.png)
- [Extensions 列表](screenshots/settings-collaboration/32-extensions-list-desktop.png)
- [资源安装确认 · 发现资源](screenshots/settings-collaboration/33-resource-install-detail.png)
- [资源列表 · 移动端](screenshots/settings-collaboration/34-resource-list-mobile.png)
- [供应商设置 · 桌面端](screenshots/settings-collaboration/35-provider-settings-desktop.png)
- [供应商设置 · 移动端](screenshots/settings-collaboration/35-provider-settings-mobile.png)
- [添加供应商 · 桌面端](screenshots/settings-collaboration/36-provider-add-desktop.png)
- [添加供应商 · 移动端](screenshots/settings-collaboration/36-provider-add-mobile.png)

## 打开与验证

浏览器直接打开 [`index.html`](index.html)，不需要服务器、构建或外部网络。

```sh
node design/clarity-workbench/verify.cjs
```

使用仓库既有 Playwright 开发依赖，生成截图与 [`verification.json`](verification.json)。验证覆盖：项目文件统一入口、目录树与工作区改动列表、右侧大尺寸文件 inspector、多个文件 Tab 切换、改动文件进入只读 Diff；真实浏览器全屏进入 / 退出 / Escape / 外部退出同步与模拟拒绝、侧栏无状态文字且保留圆点 / tooltip / 无障碍描述、切换器文字状态、详细命令切换及参数原文与失败保留、无额外新建 / 工作台导航、文件夹入口不切换会话与草稿且区分节点 / 项目作用域、设置页切换会话展示方式、侧栏保留最近会话与项目树、侧栏 / 切换器无排列 tab、默认黑白配色、模型 / 思考直接切换、手机顶栏切会话、节点分组 / 平铺 / 搜索 / 空结果、同 raw session ID 跨节点草稿隔离、资源来源语义、草稿保留、模态焦点恢复、六档宽度、短视口、长标题与放大文字、无文档级水平溢出、无真实网络和脚本异常。

这些是**静态原型的浏览器检查**，不是产品 Dioxus、原生桌面、手机系统键盘或真实节点协议验收。长正文和节点列表允许内部滚动。

## 交互和边界

- 会话 fixture 使用 `(node id, session id)` 标识，特意包含两个节点相同 raw session ID，避免按标题或 raw ID 混用。
- 原型按节点 / 会话在内存保留各自草稿与模型 / 思考选择，刷新即消失，不写 storage 或真实偏好。
- 切换节点 / 会话仅切换示意归属与内存状态；正文仍为固定示例，会明确显示说明，不伪造真实历史、连接或执行。
- 模型 / 思考弹层均可 Escape 关闭并恢复触发按钮焦点；手机会话切换使用原生模态底部面板，列表可滚动，搜索入口固定；排列偏好只在设置页修改。
- 工具过程按需展开；变更标注“当前项目 Git diff，不代表全部由此会话产生”。
- 资源统一导航，但“当前节点 / 发现资源 / 本地设计”仍分开；发现不等于已安装，本地设计不等于执行权限。
- 设置仍区分本设备外观与当前节点执行配置。主题选择 / 明暗 / 密度入口用于展示交互位置，不代表完整主题切换实现；黑白默认通过本原型的 `monochrome.css` 实现。
- 没有真实发送、安装、保存配置、文件选择、复制或 Git 操作。低频占位操作有“设计预览”说明；常用选择不弹 toast 遮挡输入器。

字体使用本机 Inter / Segoe UI / Noto Sans SC / PingFang SC / Microsoft YaHei，不下载字体。图标复用仓库 Lucide 0.468.0 路径（ISC，见 `assets/icons/LICENSE`），另有简单太阳 / 月亮图形。

## 产品实施状态

**产品落地继续暂停，先确认此版截图。** 当前入口及新增 CSS / JS 仅服务此设计目录，不可被生产代码直接加载。

此前开始的资源 / 设置生产布局草稿已暂存至 `target/clarity-implementation-paused/product-layout.patch` 并只撤回本次委派的生产布局改动，避免留下未接入的组件；原有桌面主题导入修复与用户主题文件保留。旧补丁不是新版设计实现，不能直接应用后视为完成。

真正落地沿用现有 `apps/work-ui/src/features/settings/mod.rs` 的 `SessionDisplaySettings` 与 `shared/app_settings.rs` 的 `session_display_mode`，不新增第二套偏好，不重写左侧的最近会话、项目树与管理逻辑。本次只修正原型，没有改动这些生产模块。

真正落地需继续保留：真实节点 / 会话作用域与 epoch、防旧结果写入、provider-aware 模型身份、模型配置回读 / unknown 恢复、授权 / 执行中 / 停止 / 队列 / 错误状态、当前节点资源与信任边界、节点切换时的草稿隔离。节点分组只改变信息呈现，不隐式连接节点或迁移会话；跨节点切换必须走现有真实连接 / 鉴权流程。黑白默认也不得强制覆盖用户已保存的主题偏好。
