# Codex 功能面审阅

> 历史视觉审阅与分轮验证快照。文中的“本轮”“最新”、测试数量及失败仅对应各次记录，不代表当前构建或发布状态。当前交互见[使用说明](../guides/codex-interactions.md)，后续会话迁移与 Android 补验见[迁移记录](node-session-migration.md)。

## 范围与参考

本记录区分三件事：正式 Dioxus 客户端已有功能、视觉打磨前的基线问题、本轮实际实施与验收。源码入口和协议事实优先于 HTML 原型、历史验收及演示数据；不把界面入口或本地设计状态当成后端执行能力。

公开参考：[OpenAI — Introducing the Codex app](https://openai.com/index/introducing-the-codex-app/)。文章用于理解按项目组织任务、在对话中审阅变更、集中管理能力等交互方向，不是本项目的能力承诺。官方当前 UI 截图不可得，本轮不声称与官方当前界面逐像素对比，也不以文章中的并行 Agent、worktree 等描述证明本项目已经接入对应功能。

v0.1.9 已正式发布，包含桌面本机 `rpi-extcore` 路径及 Codex 默认主题；Codex 加原四套主题共五套预设。当前源码后续修改不会自动升级已安装版本；版本发布事实与本轮视觉验收分别记录。

## 消息与队列宽度收窄

按用户要求，将共享内容宽度从1200px收至1000px，消息、Ask、输入区与底栏同步居中。原队列摘要只有面板两侧margin而无内容宽度上限，现改为共享cap及与timeline一致的非对称留白。队列弹窗从根部移入会话面板，使用inline-size容器的实际可用宽度，不再只以viewport作为上限；不改RPC、草稿、投递／取消或本地提案语义。

新增 `verify-conversation-width.cjs`：浅深主题六种视口、桌面上下文开关及320px非对称安全区，共20组几何全部通过；包括弹窗、队列摘要、正文与输入区同宽，底部按钮可达及无页面溢出。额外核验root宽度token继承，零prompt、入队、取消及凭据写入（只打开fixture已有会话）。已查看1920px浅色主界面及390px深色队列弹窗。日志为 `target/conversation-width-validation.log`；旧对话、手机输入区、继续、footer、完整timeline、reply-follow、历史分页、Mermaid八套回归通过，日志 `target/conversation-width-regression.log`。

当前全量UI单测209项通过、native／WASM严格Clippy、fmt、结构及diff检查通过，Web源码哈希0差异；冻结design未改。构建首次因并行源码变化被保护拒绝，稳定后重建成功，未绕过哈希保护。新测试曾因阻拦已有会话open、把footer内部滚动误当页面溢出及无界等待装饰动画而阻塞，均修正测试准备／等待边界，正式宽度与布局断言保留。只更新源码与Web，未操作运行中的桌面程序、调用付费模型、提交或发布；不代替原生实机验收。

## 本轮修复：失败继续、配置核对与 Mermaid 跟随

失败结算后的空输入主按钮最初为带刷新图标的「继续」；按后续要求改为纯实心播放三角图标，取消文字胶囊，使用28px圆盘与手机至少44×44px命中范围，保留「继续会话」无障碍名称和悬停说明。Enter 共用同一分派。只有明确 `pendingResume` 且协商支持恢复命令时调用 `resume_pending`，否则发送字面文本「继续」，不重放旧消息；草稿优先、附件不吞、问题／权限及待核对配置不能绕过。原生 Agent 使用 `supports_command` 判断命令，而非把命令名当能力名。请求按作用域与独立 ticket 去重，拒绝可手动重试，ACK不伪造完成；先结算后ACK、同作用域新恢复遇到旧响应、fallback ACK后重复空Enter均有保护。

配置修复不再只处理超时：setter接受后即标记未确认，普通回读错误、缺失／空字段、不匹配及需要重启都保持发送／配置锁，明确权威回读才解除；setter明确拒绝仍可重试。Mermaid关闭预览后的焦点恢复会改变scrollTop，跟随逻辑现在区分48px近底阈值与≤1px真实底部，修复gap42px却隐藏最新入口的问题，保留主动上翻阅读暂停。

时间线短行跳转的高亮探测点与12px导航锚点对齐；正文历史键盘监听忽略editable、IME、修饰键及已处理事件。旧测试改用 `/btw`，不恢复隐藏快捷按钮；结算摘要不再被错误要求伪造为聊天消息。拖动测试的一次偶发失败来自初始follow帧在坐标截取后自动显露尾节点，改为先真实hover导航条、等帧再取拖动坐标，原有阈值、滚动及兼容click去重断言均保留。

最新正式Web验证：继续 **19/19**、配置 **38/38**、时间线完整 **24/24**；Mermaid及旧对话、reply-follow、历史分页、footer、mobile composer、sidebar loading、latest-message、composer-input、attachment guard、Codex surfaces共 **14套浏览器脚本通过**。报告为 `artifacts/session-continue-validation.json`、`artifacts/composer-model-picker-validation.json`、`artifacts/user-timeline-validation.json`、`artifacts/mermaid-validation.json`；日志 `target/session-continue-final-core-validation.log` 与 `target/session-continue-final-regression-validation.log`。core日志同时保留后来已修正的timeline首次失败，不将该次运行冒称全过。已实际查看浅色1280px与深色320px继续按钮截图，另生成390px及普通发送对照。

最终 **206项UI单测**、native／WASM all-targets严格Clippy、fmt、结构及diff检查通过；结构记录为173个本地JS导入、49个内嵌资源、179条文档链接、39个冻结design文件未改。验证还做了两处不改变运行语义的工具链修正：现有启动地址Option写法遵循Clippy，现有tokio异步单测限定非WASM目标。Web `build.json` 源码哈希0差异。未调用付费模型、未操作运行中的桌面程序、未提交／发布／改版本；浏览器fixture里的“恢复”不等于真实native RPC/ACP、桌面WebView或Android实机验收，也不宣称解决生成ACK或结算永久缺失的全部loading问题。

## 真实入口与中文功能清单

| 功能面 | 真实入口 | 已有能力 | 依赖 / 尚未接入 / 不应推断 |
| --- | --- | --- | --- |
| 项目与会话 | 侧栏项目区「添加项目」、项目「＋」、全局「新建任务」、搜索及「…」菜单 | 打开或创建目录、项目显示名、搜索切换、会话创建与重命名、置顶、归档恢复、Fork / Clone、历史恢复 | 依赖本机目录权限或兼容远程节点接口。项目归档仅收纳本设备侧栏，会话归档保存到节点；归档不删除文件、历史，也不自动停止任务 |
| 主对话与工具 | 工作空间「对话」、输入区、消息内工具卡 | 流式回复、工具参数与输出、运行 / 等待确认状态、停止、断线恢复、滚动跟随与最新消息入口 | 执行依赖已配置 rpi、模型凭据及节点。计划和 ask 卡来自节点事件；请求接受、`agent_end` 或停止已提交不等于 `agent_settled`，不伪造完成 |
| 队列与压缩 | 会话队列区、任务上下文中的运行操作 | 展示节点队列、引导 / 后续投递、支持的取消 / 清空及压缩状态 | 以节点能力及回读为准；本地编辑提案不等于服务端原子重排，压缩请求接受不等于压缩完成；缺少统计分项时不能补造数据 |
| BTW 旁问 | `/btw` 命令；桌面旁栏、手机独立面板 | 真实独立普通 session、独立历史与取消、显式引用公开回答到主草稿 | **不是原生 BTW 插件加载**。不自动读取主对话；发送前必须回读确认 active tools 明确为空，未确认不发送。关闭面板只断开本地订阅，节点任务可能继续；引用不会自动发送主消息 |
| 附件与引用 | 主输入区附件入口、预览与移除操作 | 支持类型的本地图片 / 文本预览、大小限制、显式文本引用及失败保留 | **本地预览不是上传**。客户端尚未接入附件上传或图片直送模型；不能宣称模型已看到图片，也不因底层或远程节点有相关能力而视为客户端已打通 |
| 文件与 Git 变更 | 工作空间「变更」、任务上下文的项目文件入口 | 文件树、内容预览、Git 状态与 diff、搜索筛选；手机列表与详情切换 | 数据来自本机 Rust 服务或授权的兼容节点；预览和 diff 不代表已经执行修改、提交、worktree 隔离或多 Agent 调度 |
| 会话能力 | 对话区的会话资源配置、Skills / 常用能力入口 | 展示 Skills、Tools、提示词与具名 Package，保存会话选择及版本快照；读取节点实际工具状态 | 设计选择和资源追踪不自动安装资源；扩展固定版本 pins 是 **desired，非 applied**，不注入 prompt 或运行工具目录；真实执行状态须由节点协议确认 |
| 能力库与扩展 UI | 侧栏「能力库 → 扩展 / Packages / 技能库」、资源导入导出与预览 | 设计资源校验、版本选择、原文导出；Agent-only / UI-only / combined 声明，受控静态 widgets 预览，设备 UI 可见性偏好 | 通用节点安装 / 加载、会话 applied 协议及通用动态 live UI 尚未接入。隐藏 UI 不改变 Agent 配置；未知 applied 不等于安装失败或未安装。真实扩展 Demo 仅为开发适配器，不是正式通用协议 |
| 元Agent | 添加项目时选元Agent用途，再在对应项目导航打开「元Agent」 | 个人源码编辑、结构校验、不可变测试快照、个人设计版本发布、关联真实开发会话与返回源稿 | **结构校验不是执行**：不编译 Rust、不运行源码、不验证模型行为；发布设计版本不安装或启用节点插件。开发会话中的项目文件修改不会自动写回客户端草稿，须显式保存 |
| 运行信息与追踪 | 顶部「查看任务上下文」、统计及会话追踪入口 | 运行耗时、轮次追踪、队列 / 压缩、节点报告的 token、缓存和费用信息 | 展示取决于节点事件与字段；缺失、未知和估算须保留其语义，不能把工具目录或本地快照当成插件已加载 / applied 的证据 |
| 外观与主题 | 顶部「外观与主题」 | 五套预设、浅色 / 深色 / 跟随系统、密度、主题 JSON 导入导出、预览 / 取消 / 保存；v1 / v2 兼容 | 已有有效主题不强制迁移。完整视觉参数通过 JSON 编辑，目前没有逐字段可视化编辑器；保留安全本机字体栈、有界数值及可访问性约束 |
| 执行节点与更新 | 设置「执行节点」「连接其他节点」「应用更新」；手机「连接我的电脑」 | 桌面默认本机，可显式切远程；手机扫码 / 相册 / 手动地址及认证；稳定 Release 检查、签名与哈希验证、平台安装流程 | 本机 `rpi-work://local` 不监听网络，不是手机可连地址；分享须指向另行启动的可达 HTTP(S) 节点。模型凭据在 rpi / 执行节点配置；逐设备授权及项目 ACL 仍需上游支持，iOS 尚无安装包 |

### 最小事实依据

- 主导航、项目用途、工作空间与设置入口：`apps/work-ui/src/app/workspace.rs`。
- BTW 隔离与工具回读、附件预览：`apps/work-ui/src/features/conversation/composer_extras.rs`。
- 会话资源与设计库：`apps/work-ui/src/features/resources/`；[Agent / UI 扩展边界](../architecture/extensions.md)。
- 元Agent 结构校验、测试快照与发布限制：`apps/work-ui/src/features/meta_agent/mod.rs`。
- [项目、会话与消息交互](../guides/codex-interactions.md)、[transport 架构](../architecture/transport.md)、[主题说明](../design/themes.md)、[版本说明](../release-notes.md)。历史记录仅作为背景，不替代当前协议事实。

## 视觉基线：实施前现象

以下为实施前确认的基线；源码字号还会受主题倍率、密度与覆盖规则影响。下表另列正式 WASM 中的实际 computed style，不把声明值一概视为最终计算字号。

1. **工具与辅助信息偏小**：工具摘要、参数段标题、状态、队列、输入区底部和 Inspector 辅助文字中仍有 8–11px 小字。共享样式如 `.native-tool summary` 为 11px、`.call-status` 为 9px、`.composer-foot` 为 8px、`.inspector-footer` 为 9px，阅读层级容易过密。
2. **空态过于通用**：共享 `.empty-state` 使用通用虚线边框，未充分区分未选项目 / 会话、资源为空、无变更及节点无数据等语境；单纯套用同一个占位框不足以说明下一步操作。
3. **不同功能面层级不一致**：主对话、能力库、会话配置、元Agent、设置与追踪之间，标题、说明、卡片边界和操作区密度存在不一致；需要统一阅读优先级，而不是把所有功能面做成同一种卡片。

基线来自正式客户端共享样式、功能面结构及正式 WASM 截图 / computed style，不是官方截图测量。截图使用真实 Dioxus 页面与受控本地 HTTP fixture；测试数据不证明真实模型、节点安装或多模态链路可用。

## 本轮实施记录

基于已发布 v0.1.9（`4952a00`）继续打磨；本轮未提交、推送、修改版本或发布。仅修改源码，不会自动更新已安装应用。

- `apps/work-ui/src/app/workspace.rs`：只给原有会话空态增加 `workspace-welcome` 类；文案、告警、按钮禁用条件及 handler 均未改变。
- `apps/work-ui/src/shared/theme/codex-components.css`：新增规则全部限定 `[data-theme="codex"]`，沿用已有字体、间距及触控 token，没有新增远程资源或任意 CSS 导入。
  - **任务入口**：未选会话 / 空会话去掉通用虚线框、居中布局、默认 24px 标题；不把资源为空和无 diff 的占位一并改成欢迎页。
  - **对话与工具**：区分工具名称、参数、输出和终态角色；辅助信息不再挤在 8–10px 小字里，仍保留工具展开 / 收起及运行语义色。
  - **输入区与统计**：桌面控制 14px、辅助信息 12px；手机输入安全下限仍为 16px。能力区、统计项允许换行，手机主要控制与统计按钮沿用 44px 触控目标，草稿和业务状态不受主题切换影响。
  - **资源与设置**：说明文字使用正文角色，版本使用代码角色，安装 / desired 状态与辅助信息使用辅助角色；操作文字使用导航角色。卡片保留真实安装边界说明，资源标题平衡换行。手机三项设置导航按内容分配宽度，返回仍为原有图标按钮，不改变 Provider 的设备角色限制。
  - **文件 / diff / Inspector**：文件树条目、代码与运行辅助信息统一到对应角色。Diff 基底改用卡片表面，不再套终端黑底；增删色仍独立。变更标题与数量留出间距，手机文件预览、统计和上下文仍可操作。
- `tests/e2e/verify-codex-surfaces.cjs`：新增正式应用功能面回归、截图及实测字号记录。`README.md`、文档索引及主题说明同步补充真实入口和限制。

### 实测变化

默认 Codex、浅色、标准密度、1440px 桌面窗口；不同倍率 / 密度按 token 计算，不将下列数值当成硬编码上限。

| 元素 | 实施前 | 实施后 |
| --- | ---: | ---: |
| 工具名称 | 11px | 14px |
| 工具参数 / 状态 | 9px | 12px |
| 展开工具输出 | 10px | 12px |
| 桌面模型控制 | 11px | 14px |
| 常用能力 / 统计按钮 | 9px | 12px |
| 输入区底部说明 | 8px | 12px |
| 资源描述 | 11px | 15px |
| 资源版本 / 操作 | 9 / 10px | 12 / 14px |
| 设置描述 / 辅助信息 | 12 / 10px | 15 / 12px |
| Inspector 路径 / 小标题 | 8 / 10px | 12 / 14px |
| 文件树条目 | 11px | 14px |
| 文件预览标题 | 12px | 12px（保持代码角色） |

CDP 在前后两轮均确认中文实际使用本机 `Microsoft YaHei`，不是远程自定义字体；本轮没有因截图观感而替换字体栈。测量证据为 `artifacts/codex-surfaces-before-validation.json` 与 `artifacts/codex-surfaces-validation.json`。

## 本轮验证记录

### 构建与检查

- `./scripts/ci/verify-rust-workspace.ps1`：fmt、工作区测试、native / WASM Clippy（`-D warnings`）、native build 全部通过。
- `./tools/dev/build-ui-web.ps1`：正式 WASM release 构建通过。
- `./tools/review/build-module-review.ps1 -Feature all`：完整模块 harness 构建通过。
- `cargo run -p work-ui --example sync-codex-theme --locked -- --check`：生成 token 一致性检查；本轮没有修改 Rust 默认 token 或生成 CSS。
- `node --test tests/structure/check-structure.test.cjs`、`node tools/review/check-structure.cjs`、`git diff --check`：结构、文档链接、冻结设计和差异检查。

### 八套浏览器回归

全部在最终构建上通过：

1. `tests/e2e/verify-codex-surfaces.cjs`：空态、浅深对话 / 展开工具、资源、设置、Inspector、统计、文件与浅深 diff；1280 / 390 / 320px 布局；菜单与上下文对话框；草稿保留、键盘焦点及 390×400 短窗口中的发送按钮可见性。
2. `tests/e2e/verify-codex-theme.cjs`：默认主题、旧主题、系统浅深、手机标题与操作区、触控和窄屏资源入口。
3. `tests/e2e/verify-theme-design.cjs`：显式 v2 规格、导入 / 导出、保存 / 取消、密度、减少动效和手机安全约束。
4. `tests/e2e/verify-theme-contract.cjs`：完整颜色契约、冲突 / 注入拒绝、持久化、系统模式和内置主题恢复。
5. `tests/modules/verify-settings-module.cjs`：在正式应用验证设备角色、节点 / 扫码、Provider、偏好及手机布局。
6. `tests/e2e/verify-updates.cjs`：检查更新、失败重试、偏好持久化、Web 部署提示与手机布局。
7. `tests/modules/verify-extensions-module.cjs`：扩展声明、安全拒绝、UI 隐藏独立性、desired pins、运行锁及手机边界。
8. `tests/modules/verify-resources-module.cjs`：完整设计资源往返、不可变版本、会话隔离、未安装状态、持久化与手机边界。

新增测试确认 `prompt` 请求为 0，应用 JavaScript 错误为空；数据来自本地 fixture，不调用外部付费模型。Reddit 的工具状态仍为原 9px；合法自定义 v2 在 1.25 倍率下正文为 20px、原工具状态为 11.25px、发送目标为 64px，预览取消后恢复 Codex 12px 参数和原草稿，证明没有把 Codex 样式扩散到旧 / 自定义主题。其他四套主题的专属样式未修改，但不声称逐页截图比对了所有主题。

### 复现方式

先运行上述构建，再在独立终端启动 `node tools/review/ui-review-server.mjs`；正式应用位于 `/app/`，扩展 / 资源模块位于 `/modules/all/`。另一个 PowerShell 终端执行：

```powershell
$env:WORK_UI_URL = 'http://127.0.0.1:4182/app/'
node tests/e2e/verify-codex-surfaces.cjs
node tests/e2e/verify-codex-theme.cjs
node tests/e2e/verify-theme-design.cjs
node tests/e2e/verify-theme-contract.cjs
node tests/modules/verify-settings-module.cjs
node tests/e2e/verify-updates.cjs
Remove-Item Env:WORK_UI_URL
node tests/modules/verify-extensions-module.cjs
node tests/modules/verify-resources-module.cjs
```

`CODEX_SURFACES_BASELINE=1` 仅用于在**打磨前的构建**上记录基线，不会自动回退源码。最终逐套执行日志位于本机 `target/codex-surfaces-after.log`，review server 已在测试结束后关闭；日志和截图不是发布附件。

### 实际截图

以下为本地产物，不提交 Git；实施前截图使用 `codex-surfaces-before-` 前缀，实施后使用 `codex-surfaces-` 前缀。

- 空态：`artifacts/codex-surfaces-welcome.png`。
- 主对话与工具：`artifacts/codex-surfaces-conversation-{light,dark,1280,390,320}.png`。
- 资源 / 设置：`artifacts/codex-surfaces-{resources,settings}.png` 及各自的 `-390.png`、`-320.png`。
- Inspector / 统计：`artifacts/codex-surfaces-{inspector,stats}.png`。
- 上下文：`artifacts/codex-surfaces-context-{1280,390,320}.png`。
- 文件预览：`artifacts/codex-surfaces-files.png` 及 `-390.png`、`-320.png`。
- Diff：`artifacts/codex-surfaces-review.png`、`artifacts/codex-surfaces-review-dark.png`。

### 发现、保留限制与下一轮优先项

- 已修复包卡片专用选择器覆盖 Codex 描述字号，以及放大设置按钮时误覆盖手机返回按钮的两处真实级联问题；没有使用 blanket `!important`。
- 初次 Rust 工作区测试遇到 ACP permission fixture 的 5 秒超时；该单测随后独立连续三次及完整工作区复跑均通过，未修改无关传输代码。
- **自定义大字号统计栏仍有边界**：320px 窗口、示例 v2 改为 1.25 倍率 / 64px 目标时，统计栏 `clientWidth=289`、`scrollWidth=322`。该组合不使用 Codex 专属选择器，本轮仅记录，不以修改旧 / 自定义主题来掩盖问题；默认 Codex 在 390 / 320px 的统计栏无横向溢出。
- 短窗口检查不等于 Android / iOS 真实软键盘实机验收；未验证原生 GUI、其他 OS 实机或外部模型。
- 尚未统一所有元Agent编辑、队列表单和 Trace 细节；后续可沿相同语义字号逐页打磨，不应宣称全部功能面已逐项完成。
- 产品能力下一步仍是通用节点安装 / 加载 / observed-applied、真实附件上传和模型输入、以及安全的逐设备授权；本轮未新增这些协议，也没有把 BTW 普通 session 改成原生插件。
- 官方当前截图仍不可得，不声称逐像素复刻。冻结的 `design/prototype/` 39 文件和基线未改，`.codegraph/`、`NUL` 未动；历史 v0.1.9 发布不等于本轮源码已经发布。

## 后续实现：主题驱动的工具展示模式

接续上面的 Codex 专属视觉打磨，工具展示独立抽象为共享设置，不按配色 ID 写死。

- 默认「跟随主题」；所有旧 v1、旧偏好和未指定的 v2 主题默认 `simple`。简单模式改成无边框单行摘要，保留工具名、关键参数和状态，按需展开；`detailed` 不覆盖原有卡片样式。
- 主题默认位于 `theme.design.toolRendering`；本机显式选择优先，跟随主题时移除覆盖。完整主题导出保留主题定义，并用可选顶层 `preferredToolRendering` 携带本机选择；偏好 JSON 使用可选 `toolRendering`。未知值、类型、CSS 字符串均拒绝，旧文件保持兼容。
- `ThemeDesign` 使用受控枚举；`appearance.rs` 统一应用、持久化、预览与导入导出。根属性 `data-tool-rendering` 驱动共享 `tool-presentation.css`。工具节点只增加摘要 tooltip，两种模式共用同一 DOM 和业务状态，不重建工具、不丢展开状态与草稿。
- 简单模式支持原生键盘展开、完整参数和输出、复制；手机沿用 44px 触控下限。失败仍显示红色状态和展开提示，Ask 保留交互且不会自动回复。既有取消 / 等待结果状态映射未改，本次没有为测试编造取消协议。

### 实际验收

- 新增三个 Rust 测试，覆盖缺省枚举、旧 v2 往返、主题与偏好优先级、恢复和非法值拒绝。`work-ui` 共 98 项测试通过。
- 最终 fmt、工作区测试、native / WASM Clippy `-D warnings`、native build、正式 WASM 及 all 模块构建全部通过。严格检查发现的 Serde 小型 Copy 引用谓词与空集合断言问题已修正，无需放宽 lint。
- 新增 `tests/e2e/verify-tool-rendering.cjs`，连同上面八套回归，共 **九套** 在最终构建上通过。验证旧偏好默认简单、详情保留原卡片、展开与复制、模式切换不丢展开状态、取消与重载、完整主题 / 偏好 JSON 往返、主题默认与本机覆盖、非法枚举原子拒绝、运行 / 完成 / 失败及未回答 Ask、浅深色、390 / 320px 边界和触控。
- 新测试确认 `prompt` 请求和隐式 `ui_dialog_response` 均为 0，应用 JavaScript 错误为空；只使用本地 HTTP fixture。报告为 `artifacts/tool-rendering-validation.json`，最终九套执行日志为 `target/codex-surfaces-after.log`。
- 截图：`artifacts/tool-rendering-{simple,detailed}-{light,dark,390,320}.png`，以及 `artifacts/tool-rendering-simple-expanded.png`。旧 `codex-surfaces-*` 截图随新默认重新生成；原始基线 `codex-surfaces-before-*` 保留。
- 重建并启动 review server 后，设置 `WORK_UI_URL=http://127.0.0.1:4182/app/`，运行 `node tests/e2e/verify-tool-rendering.cjs`；其余八套命令见上文。review server 已由本机 runner 关闭。

本轮仍未提交、推送或发布，版本未变；上述结果不代表已安装的 v0.1.9 发行包已自动升级。上节记录的自定义大字号统计栏、真实手机软键盘和节点通用协议等限制仍保留。

## 修复：最新消息点击无跳转

正式 WASM 与真实浏览器输入复现并修复两类问题：

1. 全局 `button:active` 的 `scale(.97)` 替换了悬浮按钮的 `translate(-50%, -100%)` 定位，按下时按钮移位，松手命中消息正文，无法形成有效点击。减少动效下稳定复现：320px 测量由 `(113.77,417.25)` 跳到 `(161.39,457.85)`。在共享 `scroll_follow.css` 给按下状态保留原定位，不修改全局按钮样式。
2. 可视区域放大或内容缩短时没有同步 unread / following，出现 `scrollHeight=clientHeight=624`、内容已全部可见但按钮仍在的情况。`scroll_follow.rs` 现在同步可视高度和底部几何，忽略隐藏容器，清理无效按钮；显式跳转只走一条立即滚动路径，清除旧滚动意图并恢复跟随，删除重复 DOM capture click。

新增 `tests/e2e/verify-latest-message.cjs` 在修改前实际失败：旧构建能复现陈旧按钮；随后针对按下定位的断言也在未重建 CSS 的旧构建上失败。前置证据保存在 `target/latest-message-{before,active-before}.json` 和 `artifacts/latest-message-before-fix.png`。没有用强制点击或程序直接设置滚动位置替代验收：使用真实鼠标滚轮、鼠标按下/松开、手机尺寸下的触摸 tap 和 Enter。

最终回归确认 1440 / 390 / 320px 跳到实际底部、按钮隐藏、后续输出继续跟随、减少动效下按下位置不变、草稿保留且新增测试 prompt 请求为 0。共享 scroll 模块确认各会话独立阅读位置、卸载清理和重挂恢复；reply-follow 验证发送、停止及运行终态。其旧行高断言改为校验实际主题 `--reading-line-height`，不再把当前 Codex 1.7 错判为旧主题 1.55。

最终 **十二套浏览器回归**（新增 latest-message、共享 scroll 模块、reply-follow 加前面的九套）通过；工作区 fmt / test、native 和 WASM 严格 Clippy、native build、正式 WASM / all 模块构建通过。测试报告为 `artifacts/latest-message-validation.json`，点击前后截图为 `artifacts/latest-message-{before,after}-{1440,390,320}.png`。日志 `target/codex-surfaces-after.log` 已更新为本次十二套结果。未改节点协议、版本或冻结设计，未提交 / 发布；不代表 Android/iOS 真实设备实测。

## 后续实现：离线 Mermaid 图表

本节独立记录 Mermaid 后续实现与浏览器证据，不将前面各轮的构建 / 测试结果自动视为本功能已通过，也不表示已安装发行包已更新。

### 实现与边界

- `apps/work-ui/src/features/conversation/mermaid.rs`、`mermaid.js`、`mermaid.css`：显式 `mermaid` 围栏（大小写不敏感）显示图表，普通代码块不变。历史与流式消息共用组件，未闭合围栏保留源码并等待闭合；支持复制原始源码、键盘展开、失败状态及后续图表恢复。剪贴板失败提示手动选择，不伪装复制成功。
- 使用官方 npm `mermaid@12.2.0` 的原样 `dist/mermaid.min.js`，本地离线加载，无 CDN 或远程绘图服务；包来源、integrity、文件 SHA-256 和许可证见 [`assets/vendor/README.md`](../../assets/vendor/README.md)。未改写上游 bundle，保留嵌套依赖许可证注释。
- 固定 strict 安全等级、关闭 HTML labels，限制源码 20,000 字符、边数 500；拒绝图内配置指令及开头的 YAML frontmatter。边数限制是运行时配置，本脚本未单独构造超过 500 条边的验收用例。图表使用当前主题颜色，浅深切换重新绘制，响应式图像与独立源码滚动区保留窄屏边界。
- 仅在不透明源的 `sandbox="allow-scripts"` iframe 渲染，无 `allow-same-origin`；CSP 禁止网络连接、图片和字体资源，只允许带 nonce 的本地脚本及内联样式。用户源码作为数据，不执行用户 JavaScript。结果只以 Blob URL 的 `<img>` 呈现，不向主页面注入 Mermaid SVG / HTML；图像内链接不可点击，外部图片不受支持。多个图表共享一个隔离渲染器，卸载清理 Blob URL 与任务。

### 复验命令与已读取证据

先按上文方式准备正式 WASM 并在独立终端运行 `node tools/review/ui-review-server.mjs`，再执行：

```powershell
$env:WORK_UI_URL = 'http://127.0.0.1:4182/app/'
node tests/e2e/verify-mermaid.cjs
Remove-Item Env:WORK_UI_URL
```

文档收尾时已读取 `artifacts/mermaid-validation.json`（本机文件修改时间 2026-10-30 16:00:43 +08:00），其中六组检查均有成功记录、无 failure 项，`errors` 与 `external` 均为空；不是仅从脚本存在推断通过。本次文档工作没有重跑构建、Rust 测试或浏览器脚本。报告对应正式 WASM 与隔离本地 HTTP/SSE fixture，不调用外部模型；现有应用 GitHub Release 检查被脚本显式排除在 Mermaid 外部请求统计之外，不据此声称整个应用从不联网。

脚本覆盖中文流程图与时序图、历史恢复和闭合后流式绘制、普通代码不变、剪贴板原文、Enter 展开、浅深主题重新绘制与展开状态保留、非法语法 / 配置 / frontmatter / 超长源码回退及后续恢复；也检查 HTML / script 标签、外部图片与 click 链接不在主页面执行或产生绘图外连。390 / 320px 检查横向边界、44px 复制目标、草稿、滚动跟随、零 prompt 请求与应用 JavaScript 错误。剪贴板拒绝及渲染超时的提示路径来自源码审阅，未在此脚本单独模拟。

本地验收产物（不提交 Git）：

- 报告：`artifacts/mermaid-validation.json`；流程图 SVG：`artifacts/mermaid-flow.svg`。
- 浅色流程图：`artifacts/mermaid-flow-light.png`。
- 深色流程图：`artifacts/mermaid-flow-dark.png`。
- 流式时序图：`artifacts/mermaid-sequence.png`。
- 390px：`artifacts/mermaid-mobile-390.png`。
- 320px：`artifacts/mermaid-mobile-320.png`。

上述五张截图与报告已确认存在；目录中遗留的 `mermaid-failure.png` 不是这次成功报告的验收截图。**Android / iOS 原生手机实机及原生 WebView 渲染尚未验证**，浏览器窄屏不能替代实机、软键盘或剪贴板权限验收；也未逐一验证所有 Mermaid 图类型或全部主题。用户用法与完整限制见[交互指南](../guides/codex-interactions.md#mermaid-图表)。

## 侧栏 loading 与主发送按钮回归

侧栏状态接入当前会话的真实 projection、打开请求和按节点／会话隔离的提交记录，显示加载、发送、等待回复、运行、确认及停止状态。切走后只保留最后观测阶段或节点摘要，没有新增所有后台会话的实时订阅。失败与明确 `pendingResume` 的提示保留，不把 HTTP 接受或 `agent_end` 当完成。

主按钮的提交／停止等待只显示一个居中 loading；可停止时只显示方块，不叠旋转环。停止请求在途及接受后均禁用重复操作，草稿保留；真实 settlement 若先于停止 HTTP 响应到达，迟到响应不会恢复 loading 或覆盖终态提示。

正式 Web 与全部模块构建、122 项 UI 单测、native／WASM 严格 Clippy 已通过。正式 WASM + 隔离 HTTP/SSE fixture 的五套回归已通过：`verify-sidebar-loading.cjs`、`verify-reply-follow.cjs`、`verify-ui-conversation.cjs`、`verify-codex-surfaces.cjs`、`verify-codex-theme.cjs`。新增侧栏回归覆盖浅深主题 × 1280／390／320px 的状态隔离、按钮图标居中与尺寸稳定、停止迟到应答、确认／失败、草稿和 reduced-motion；报告为 `artifacts/sidebar-loading-validation.json`，`errors=[]`。截图为 `artifacts/sidebar-loading-{light,dark}-{1280,390,320}.png` 与对应 `-composer.png`。测试不调用真实模型，不据此声称原生桌面 WebView 或手机实机通过；此次没有替换被运行中进程占用的桌面可执行文件，也没有提交或发布。

## Codex 风格图标与空参数展示

共享 `Icon` 使用本地绘制的 35 个单色线框图形、24×24 网格与 1.5 圆角描边；工具与输入区不再维护独立 SVG 路径，侧栏、设置等原有调用同步统一。读取、命令、编辑、搜索、网页、计划使用明确语义图标，补齐下载等之前的占位回退。对齐的是视觉风格，不声称使用 Codex 官方原始素材。

历史和实时工具详情对缺失、空白、null、空对象／数组／字符串隐藏整个调用参数区；`false`、`0`、非空内容及部分 JSON 仍可检查。没有摘要时不重复工具名，保留结果、复制、图片、权限和错误提示。简单／详情模式、浅／深色与桌面／390／320px 均已实测。

当前 UI 单测 **127 passed**，native／WASM 严格 Clippy、正式 Web、all 模块及 project 专用模块构建通过。十套浏览器回归通过：工具展示、侧栏 loading、reply-follow、正式对话、Codex surfaces、Codex theme，以及 conversation／extras／project／settings 模块。project 首次运行缺少更新后的专用构建，补建 `-Feature project` 后重跑通过；没有修改产品代码规避测试。

`artifacts/tool-rendering-validation.json` 有 10 项成功记录、`errors=[]`；新增夹具检查历史／实时空值和有效值、完整输出复制、可见 SVG 描边／网格／无障碍属性，且零 prompt 和隐式权限回答。已查看空参数截图 `artifacts/tool-rendering-simple-light-desktop-empty-args.png`、320px 深色参数截图与终端图标截图，以及正式应用 `artifacts/codex-surfaces-conversation-light.png`。不以浏览器夹具代替原生 WebView、手机实机或真实 Agent 验收；未替换被占用的桌面 exe，未提交或发布。

## Provider 与思考等级统一 Web 弹层

按用户确认，以本项目现有浏览器版为参照，不改成 Codex 网页锚定菜单。Provider 保留供应商／模型分组，改用共享箭头与选中图标；思考入口改用同一个 `Modal`／主题化列表，不再依赖操作系统原生 select。两者随会话／连接作用域一起卸载，关闭恢复焦点；思考列表只接受节点实际报告的原始级别。配置互斥、运行锁、重复提交保护不变，思考等级现在也必须由权威回读精确确认才更新并关闭；失败或不一致保留弹层。迟到的 Provider 刷新不能覆盖较新的思考配置。

正式 Web 构建、**162 项 UI 单测**与 native／WASM 严格 Clippy 通过。`artifacts/composer-model-picker-validation.json` 当前 **24 项 PASS、errors=[]**，覆盖两个弹层、真实 fixture setter/readback、错误／回读不一致／重试、能力边界、跨会话／连接 epoch、迟到刷新、焦点、草稿与浅深色 1280／390／320px。Provider 与思考各有六张尺寸／主题截图；已实际查看 `artifacts/composer-model-picker-light-1280.png`、`artifacts/composer-thinking-picker-dark-320.png`。`verify-mobile-composer.cjs`、`verify-codex-surfaces.cjs`、`verify-sidebar-loading.cjs` 也通过，发送／停止维持 28px 可见圆与手机 44px 命中区。

验证中将原生 dialog 的隐藏与逻辑关闭完成分开等待，避免 Escape 后立即发送 Enter 时撞上焦点恢复；旋转 spinner 的尺寸按 intrinsic CSS 检查，仍验证实际边界中心，不把旋转中的包围盒误当尺寸变大。旧 `verify-ui-conversation.cjs` 在已被前序改动移除的 BTW 快捷按钮处阻塞，未计为通过，也未为该脚本恢复已隐藏入口。`verify-live-chat.cjs` 只适配新控件并语法检查，**未执行付费 live 测试**。用户明确选择只更新源码与 Web，因此没有关闭运行中的桌面程序、构建或替换该 exe，也未声称桌面 WebView 端到端通过；未提交、发布或改版本。

### 后续：配置 loading 超时与核对恢复

排查确认模型目录、思考等级发现、setter 和权威回读均缺少 UI 侧期限；思考发现还会静默吞错。新增 `app/config_requests.rs`，用现有 native／WASM timer 为每次短配置请求提供 15 秒期限，不更改全局 HTTP/SSE 或模型生成期限。超时结束本地等待、释放 loading/busy，但不声称远端任务被取消。思考弹层在发现中／失败时仍可打开、查看错误及手动刷新，真正能力缺失／空等级保留禁用。

setter 或其后回读超时后显示「结果待核对」，保持旧显示和草稿，不增加成功 revision、不自动关闭、不自动重发。可重新浏览及刷新核对，新的配置写入与发送／Enter 暂时受保护；只有显式 `get_state` 报告受影响的非空字段才解除。核对再次超时或返回空状态也不误解锁，迟到 ACK／回读不关闭新弹层、不覆盖新状态。原有停止动作不因待核对标记被禁用。

最新正式 Web、**165 项 UI 单测**、native／WASM 严格 Clippy 和四套浏览器回归（模型／思考、手机输入区、侧栏 loading、Codex surfaces）通过。专用报告现为 **30 项 PASS、errors=[]**；六个新增场景用浏览器 clock 推进真实 gloo deadline，HTTP gate 持有真实 fixture 返回，两个 setter 先实际改变 fixture 状态再延迟确认，验证不重发及手动核对。超时恢复弹层新增 320／1280px 边界与溢出断言，已查看 `artifacts/composer-config-timeout-model-ack-320.png` 和 `artifacts/composer-config-timeout-thinking-readback-320.png`。完整日志 `target/config-loading-final-validation.log`。这些不是原生实机或真实付费模型验收；仍未关闭／替换桌面 exe，未提交或发布。

## Footer 插件化后的高度回归修复

用户确认问题为排版变高、换行并挤占输入区，而非数据或插件开关。定位到 `footer_plugins.css` 的主容器 `flex-wrap:wrap`、value 换行、手机 identity 强制占满一行；原回归允许默认高度 80px，且把长身份换行当成预期，因此漏检。修改后的紧凑断言先在旧正式 WASM 失败：1280px 默认 footer **50.6px／两行**。基线保存在 `artifacts/footer-plugins-wrap-baseline.json`、`artifacts/footer-plugins-wrap-baseline-dark-1280.png` 和 `target/footer-wrap-baseline.log`。

修复限于 FooterHost 容器可访问属性、插件 footer CSS 及对应测试／文档：单行 nowrap、不压缩各指标，超宽内容只在底栏自身横向滚动；无省略号、tooltip 或数据删除。恢复方向键浏览的容器焦点、3px 滚动条和焦点指示，断线提示也保持同行。插件注册、指标计算、偏好 schema、开关／排序／存储不变，所有指标仍为无动作 span，不触发节点命令或弹层。

修复后浅深主题 1280px 默认 **28.8px**、390／320px 默认 **27.1px**，长身份不增高；1.5 倍字号约 **36.2–38.7px**。13 项 footer 专用 fixture 用例全部通过，增加单行／默认≤32px／大字号≤44px、无内部截断、首尾值可达、方向键实际滚动、scroll恢复及短视口／模拟安全区断言。保留完整数据、安全转义、只读、损坏保护及设置持久化回归；不再把“所有指标同时处于视口内”当成窄屏必要条件。无弹层／tooltip 检查仍保留，只忽略 timeline 自己的隐藏空 tooltip 占位。已查看最新 `artifacts/footer-plugins-light-390.png` 和 `artifacts/footer-plugins-dark-1280.png`。

正式 Web、165 项 UI 单测、双端严格 Clippy、结构／fmt／diff检查通过；五套浏览器回归（footer、mobile composer、Codex surfaces、sidebar loading、composer model picker）通过，日志 `target/footer-compact-final-validation.log`。相关布局测试现在区分容器内有意横向滚动与页面溢出，且收紧高度，不放宽输入区边界。冻结 design 文件未改；未运行付费模型、未关闭／替换桌面 exe、未提交／发布，不声称原生实机通过。

### 再修正：紧凑纯文本不等于旧视觉

用户指出上一次仅处理了高度，样式仍与以前不同；经确认，参照是插件化前的输入输出小图标、上下文圆环、缓存数字绿色高亮，而非纯文本横向条。实际对照 HEAD 旧 `RuntimeFooter`、`runtime_stats.css` 及 `artifacts/composer-{1440,390}.png` 后恢复这些元素：mono 字体、inline-flex 指标与原来的数值间距，桌面右侧非交互「会话累计」，手机核心用量／缓存／上下文／工具的分布。保留当前可读字号，不恢复旧 9px 小字或点击弹层。窄屏恢复原费用／速度隐藏和累计说明隐藏规则。插件顺序仍尊重设置，因此不以 CSS 强行复原跨插件的上下文／费用固定次序。

新增 typed `FooterVisual`，让注册表提供短可见文案及安全数值，完整 `item.value` 仍用于可访问名和范围说明；Host 不解析显示字符串或执行 HTML。上下文只用实际 tokens/window 的合法比例绘环，未知不当0；缓存仅有效已确认数值染绿。新默认关闭额外的会话身份插件，设置仍可开启／排序；已有完整 v1 配置包括全开配置均原样保留，不迁移、不覆盖。所有插件关闭仍无占位。

浅深色 1280／390／320px 新截图已查看；默认桌面 **27px**、手机 **25.5px**，默认指标均无需横向滚动；大字号或显式打开长身份才使用容器内滚动兜底。footer 13 项浏览器用例全通过，补查图标／环12px、缓存 label 与数值不同色、mono 字体、旧响应式、身份启用、保存全开配置与全部边界。长条 ArrowRight 浏览需等待原生滚动动画结束再校验复位，未禁用动画或放宽内容边界。四套主浏览器回归（footer、mobile composer、Codex surfaces、model picker）及单独 sidebar loading 均通过，日志 `target/footer-original-style-main-validation.log`、`target/footer-original-style-sidebar-validation.log`。

正式 Web、footer **13 项 Rust 单测**、native／WASM 严格 Clippy、fmt和结构检查通过。当前全量 UI 单测 **174 passed／1 failed**，失败是未在本轮修改的侧栏 `sidebar_spinner_is_dedicated_and_remains_visible_without_motion` 对 `animation:none` 的源码字符串断言，与当前并行 CSS 的 reduced-motion 动画规则不一致；未修改该断言或侧栏代码掩盖，日志 `target/footer-original-style-cargo.log`。因此不声称全量单测通过。仍未提交／发布、未关闭或替换运行中的桌面 exe，浏览器fixture不代表原生实机验收。

## 左侧 timeline 常显、十节点视窗与边缘历史加载

用户确认要常显而非点击展开；超过十个节点仅隐藏视窗外的部分，滚动查看，到较早端自动加载历史。`user_timeline.css` 将视窗上限改为十个真实节点步长，桌面24px、手机44px，短视口自然缩小；所有已加载节点仍挂载，未截成前十条。保留左侧定位、正文避让、向右预览及输入区边界。

`user_timeline.js` 为向早端浏览增加一次性意图，仅滚轮向上、Home／上键、原生触屏下滑、桌面向上边缘拖动到顶才触发；初始化／导航／程序化滚动不加载，不在响应后继续拉页。复用 `HistoryPager` 的独立自动事件，仍受真实游标、enabled、sent、Rust claim、scope/generation/revision 保护；错误不自动重试，手动重试仍可用。无可用分页动作时清理 observer，修复历史分页回归暴露的禁用后监听残留，不改旧断言。前插历史保持时间线可见节点与正文双锚点。

手机手势改为原生 pan-y，滑动只滚动时间线，点击才跳转。补测先复现原生 tap 抢走输入焦点，再只拦截兼容 mousedown 的焦点默认动作，未取消 touch/pan-y；真实 CDP tap／swipe 回归同时检查草稿／焦点、内部滚动及正文不动。

正式 Web、**176 项全量 UI 单测**、双端严格 Clippy、fmt／JS语法／结构／diff检查通过。六项新增 `ten-node` 回归先独立通过，记录 `artifacts/user-timeline-ten-node-validation.json`；最终选跑 **23/23** 时间线用例（24项中的已有 settlement-summary 可见性失败用例明确排除，断言未改，历史失败证据 `artifacts/user-timeline-left-full-attempt.json` 保留）。历史分页和 mobile composer 两套也通过，日志 `target/user-timeline-ten-final-validation.log`；不声称时间线24项全通过。已查看桌面／手机十一节点截图 `artifacts/user-timeline-ten-{1440,390}.png`，并产出原浅深主题／320px截图。未发模型请求、未替换桌面 exe、未提交发布。







