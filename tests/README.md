# 测试入口

所有命令从仓库根目录运行，开发依赖在 `tools/` 安装，不依赖相邻项目。

```powershell
npm ci --prefix tools
npm run install-browser --prefix tools
# 已有 Chrome 时也可以设置 CHROME_PATH。

cargo test --workspace --locked
node tools/review/check-structure.cjs
```

## 跨平台单一 UI 回归

连接入口和移动连接设置必须直接使用生产 `connection_panel::ConnectionScreen`；规则见[开发规范 §1.3](../docs/guides/development-standards.md#13-跨平台单一-ui新增与修改的硬约束)。

```powershell
node --test tests/structure/unified-ui.test.cjs tests/structure/remembered-connection.test.cjs
pwsh -File tools/review/build-connection-panel.ps1
node tests/e2e/verify-connection-panel.cjs
# 正式 WASM 已按下文构建后：
node tests/support/run-static-ui.cjs tests/e2e/verify-connection-failures.cjs tests/e2e/verify-device-flow.cjs tests/e2e/verify-mobile-scanner.cjs tests/e2e/verify-ui-auth-scope.cjs
```

结构约束已接入 CI，防止控制器重拼页面及按平台分叉导航；不能代替行为测试。共享页面 harness 对 native/Web **合成能力配置**运行同一组操作，对比 320/390/768、明暗主题下的可见内容、几何和 computed style，并覆盖恢复、取消、重连、退出与固定宿主策略；它不运行 Android Keystore/Wry，也不构成真机视觉验收。

`verify-connection-failures.cjs` 验证尚未认证节点时的连接失败、加速 deadline、错误密码和重试反馈。`verify-desktop-owner-http.cjs` 使用隔离 native owner fixture，覆盖固定宿主入口、迟到登录结果取消、重连、退出失败及权威退出、共享撤销；可用 `WORK_UI_BUNDLE` 指定隔离构建目录，不能指向真实用户 owner 做写操作。

正式应用的测试先通过 `tools/dev/build-ui-web.ps1` 构建。目录同步等长时用例建议用隔离静态入口，避免开发服务器热刷新中断测试：

```powershell
node tests/modules/workspace-refresh-clock.cjs
node tests/support/run-static-ui.cjs tests/e2e/verify-workspace-refresh.cjs tests/e2e/verify-workspace-sync.cjs tests/e2e/verify-node-lifecycle.cjs
cargo test -p work-client --locked --test workspace_http_catalog
```

静态入口仅在随机回环端口服务 `target/parity-web`（或环境变量 `WORK_UI_BUNDLE` 指定的已构建目录），不代理真实节点、不触发热刷新，依次运行指定脚本并设置各自的 `WORK_UI_URL`。native HTTP contract 使用 Rust 内存节点，验证独立 facade 对同一 owner 的目录／元数据一致和不同 owner 的同 ID 隔离；不是原生 GUI 或 native/browser 同场验收。

`verify-workspace-refresh.cjs` 覆盖侧栏手动刷新：外部新增会话、在途旧快照后补拉、重复点击合并、失败保留及重试、节点切换清理、草稿/选中会话保留和390/320布局。`workspace-refresh-clock.cjs` 直接验证生产调度脚本的即时触发、不重叠、队列合并、可见性及dispose清理。均只用隔离fixture或纯内存调度，不替代原生GUI/实体手机验收。

也可以运行 `tools/dev/dev-web.mjs`，然后在另一个终端：

```powershell
$env:WORK_UI_URL = 'http://127.0.0.1:14180/'
node tests/e2e/verify-device-flow.cjs
node tests/e2e/verify-connection-share.cjs
node tests/e2e/verify-node-lifecycle.cjs
node tests/e2e/verify-ui-auth-scope.cjs
node tests/e2e/verify-ui-conversation.cjs
node tests/e2e/verify-turn-timing.cjs
node tests/e2e/verify-tool-spacing-timing.cjs
node tests/e2e/verify-skill-tools.cjs
node tests/e2e/verify-composer-input.cjs
node tests/e2e/verify-footer-plugins.cjs
node tests/e2e/verify-mobile-composer.cjs
node tests/e2e/verify-history-pagination.cjs
node tests/e2e/verify-native-timeline.cjs
node tests/e2e/verify-session-loading.cjs
node tests/e2e/verify-message-images.cjs
node tests/e2e/verify-tool-message-order.cjs
node tests/e2e/verify-message-stream-order.cjs
node tests/e2e/verify-cross-turn-message-identity.cjs
node tests/e2e/verify-compact-message-identity.cjs
node tests/e2e/verify-loading-motion.cjs
node tests/e2e/verify-sidebar-loading.cjs
node tests/e2e/verify-conversation-width.cjs
node tests/e2e/verify-provider-tools-polish.cjs
node tests/e2e/verify-web-tool-display.cjs
node tests/e2e/verify-composer-picker-popovers.cjs
node tests/e2e/verify-attachment-send.cjs
node tests/e2e/verify-new-conversation.cjs
node tests/e2e/verify-session-creation-isolation.cjs
node tests/e2e/verify-workspace-management.cjs
node tests/e2e/verify-workspace-sync.cjs
node tests/e2e/verify-sidebar-projects.cjs
node tests/e2e/verify-sidebar-resize.cjs
node tests/e2e/verify-reply-follow.cjs
node tests/e2e/verify-ui-provider-cards.cjs
node tests/e2e/verify-ui-resource-cleanup.cjs
node tests/e2e/verify-ui-suite.cjs --list
```

## 会话首屏与原生 timeline 链路回归

```powershell
cargo test -p work-node-service --test native_timeline --locked
cargo test -p work-client --lib --locked
node tests/support/run-static-ui.cjs tests/e2e/verify-session-loading.cjs tests/e2e/verify-native-timeline.cjs
```

`work-node-service/tests/native_timeline.rs` 必须使用真正 `HttpServer`、同一 owner、隔离 RPC fixture；覆盖生产 `WorkspaceClient` 的严格页校验、GET `message-summaries` / `message-window`、参数、原生失败/leaf 冲突、能力交集与身份/共享安全。仅有浏览器 mock 路由不能证明服务端接口接通。两个原生只读命令的兼容桥只补已知名字，仍要求 live host 广告；不以全量 `get_messages` 过滤模拟新接口。

`verify-session-loading.cjs` 把文件、Git、初始队列和状态请求同时挂起，断言首屏、timeline 摘要和实时事件先显示；检查连接复用刚观测的 access、单次 open、48 条快照和切会话迟到结果。压缩对话框为其写入安全边界保留独立的能力观测，因此整个页面不只一次 access；客户端测试单独验证连接不重复观测、重连重新协商。挂起用因果门闩而非生产耗时阈值，不代表实际设备/真实大历史的性能基准。候选构建可由 `WORK_UI_BUNDLE` 指定，避免覆盖正在服务的静态资源。

- `e2e/`：正式应用与隔离节点的集成检查；名字含 `real` / `live` 的检查有额外环境要求。
- `design/`：冻结 HTML 原型的行为及设计对照检查。
- `modules/`：模块验证入口；部分脚本也可通过 `WORK_UI_URL` 检查正式应用。
- `fixtures/`：测试节点、样例与只读审查基准，不参与产品运行。
- `harness/ui-modules/`：独立 Rust 验证工程，使用自己的 Cargo.lock。
- `support/`：统一的 Playwright 依赖解析。

完整历史 suite 包含真实节点只读检查，需要预先配置隔离节点。真实模型检查必须显式传入 `--run-model`；常规 fixture 测试不会调用模型或向现有会话发送消息。

`verify-project-node-info.cjs` 使用正式 WASM 和隔离 HTTP/WebSocket fixture 检查项目文件夹前的节点类型／在线角标、悬停与键盘／焦点、项目和会话菜单、真实归属与独立会话、草稿／选择／展开状态保留、脱敏错误、健康／认证／未就绪、刷新恢复及去重、每工作空间单一周期检查、15 秒超时与迟到成功、会话流异常与节点可达区分、跨节点／同地址重连隔离、长名称和明暗主题 1440／390／320px。报告为 `artifacts/project-node-info-validation.json`，截图以 `project-node-info-` 开头；零 prompt／模型配置／队列／凭据写入。云图标代表远程 HTTP 连接而非已证实的云部署，30 秒健康观察不是机器资源监控；浏览器模拟不替代 Android 实机验收。

`verify-provider-tools-polish.cjs` 使用正式 WASM 与隔离 fixture，检查聊天内供应商／模型本地搜索（名称、ID、大小写、清除、无结果）、过滤后重复 ID 仍禁选、原生折叠与重新打开、工具参数／输出内部限高，以及浅色／深色 1280／390／320px 的消息和选择器布局。只浏览，不提交模型配置、凭据或 prompt，保留草稿；报告为 `artifacts/provider-tools-polish-validation.json`，截图以 `provider-tools-polish-` 开头。不替代桌面或 Android 实机验证。

`verify-skill-tools.cjs` 使用正式 WASM 与隔离 fixture 检查真实 `SKILL.md` 读取卡片：历史／实时、POSIX／Windows路径、运行／部分输出／完成／失败、普通文件／备份／目录／shell／未知工具不误判、原始工具名及完整参数／结果／复制、独立折叠、草稿与安全转义。简单／详情 × 浅深主题 × 1280／390／320px共12组布局检查；报告为 `artifacts/skill-tools-validation.json`。零模型／prompt／配置／凭据写入，不将资源选择或读取成功冒充技能流程执行，不替代原生桌面或 Android 实机验收。

`verify-web-tool-display.cjs` 对照 Web 工具展示，使用正式 WASM 与隔离 HTTP/WebSocket fixture，检查单一左侧状态、完整路径／命令 title、只读源码与 HTML 转义、安全 Markdown 与 raw 复制、edit 参数增删预览和真实结果分离、命令四行预览、历史失败默认展开及 live 失败仅展开一次。另覆盖独立折叠、输出／计时更新不重置 DOM 或草稿焦点、明暗主题 1280／390／320px 和移动 44px 触控区。运行：`node tests/support/run-static-ui.cjs tests/e2e/verify-web-tool-display.cjs`；报告 `artifacts/web-tool-display-validation.json`，截图同名前缀。零真实命令／模型／prompt／配置／凭据写入；不替代桌面或 Android 实机验收。

`verify-message-readability.cjs` 使用正式 WASM 和隔离 fixture 检查中英文长回复、段落／列表／标题／代码的阅读节奏、上置头像与角色标题、正文 760px 阅读宽度和仍保持 1000px 的工作区上限、24px 消息边界、20px 正文／工具边界、纯工具零额外分组留白。消息外观对齐邻近 Web：助手使用 21px 圆角 `</>` SVG 标识与 `rpi`，颜色沿用语义品牌主题；用户的“你”标题右对齐并与 88%／650px 气泡等宽。检查标题／正文 8px 间隔、11px／15px 气泡内边距、h2／h3 的 21px／18px 字阶及缩放；正文保留至少 15px 和更大的主题设置，最小行高调整为 1.75。覆盖明暗主题 1280／390／320px、手机 1.5 倍文字缩放和草稿保留；更大的主题字号／行高继续生效。运行：`node tests/support/run-static-ui.cjs tests/e2e/verify-message-readability.cjs`；报告 `artifacts/message-readability-validation.json`，截图同名前缀。零 prompt／模型／配置／凭据写入，不替代原生桌面或 Android 实机验收。

`verify-composer-picker-popovers.cjs` 验证聊天内模型／思考的非模态锚定小菜单：当前模型目录名称与 raw ID 降级、思考中文标签与原始协议值、直接切换、外部点击、Escape／方向键／Home／End／Tab、禁用项跳过、长名称与明暗主题 1280／390／320px。另模拟缺少 Popover API 的浏览器，检查降级菜单同步隐藏、焦点与实例清理；不代表旧 WebView 或实体手机验收。报告为 `artifacts/composer-picker-popovers-validation.json`，截图同名前缀；仅浏览、零 prompt／配置／凭据写入。原有 `verify-composer-model-picker.cjs` 可通过 `WORK_UI_PICKER_CASE` 按名称前缀筛选用例；默认仍执行全部。

`verify-ui-provider-cards.cjs` 检查供应商所属模型、直接切换、凭据状态及无模型空状态；`verify-ui-resource-cleanup.cjs` 检查空资源库启动和旧样例缓存迁移，确保保留用户资源与会话配置。测试资源由 fixture 或显式导入提供，不依赖正式 UI 内置样例。

`verify-composer-model-picker.cjs` 检查主输入区「供应商 → 模型」分组、跨供应商重复 ID 禁选、独立发现、加载／目录错误／空目录与刷新恢复、读取／设置能力分别缺失、setter 与回读失败／不一致、双击去重、模型／思考配置互斥和发送保护、迟到刷新／切会话／同地址重连、草稿保留及明暗主题 1280／390／320px。报告为 `artifacts/composer-model-picker-validation.json`，使用正式 WASM 与隔离 fixture，零 prompt／队列／凭据写入及真实模型调用。`verify-mobile-composer.cjs` 与 `verify-sidebar-loading.cjs` 另断言移动发送／等待／停止状态的 32px 可见圆、普通 12px 箭头和至少 44px 点击区域，桌面保持原尺寸；浏览器测试不替代 Android 实机验收。

`verify-footer-plugins.cjs` 使用正式 WASM 与隔离 HTTP/WebSocket fixture，最新 `artifacts/footer-plugins-validation.json` 记录 **13/13 个用例实际全部 PASS**、无错误，明暗主题 1280／390／320px 的六张截图为 `artifacts/footer-plugins-{light,dark}-{1280,390,320}.png`。覆盖四个内置插件的默认顺序与真实累计用量、可选上报指标、普通只读 span 无统计/上下文/工具弹窗及零点击命令、设置启停/单项显隐/插件排序/重载/恢复默认、全关无 footer 占位、损坏存储保持原字节直到显式重置、配额失败保留内存、会话/节点隔离与断连未知值、缺失上下文不反推 token/百分比、真实零值、异常数值、已加载工具调用/失败口径、身份文本转义与长 ID 全文换行，以及明暗主题布局。新增高级指标、大字体、短键盘视口及安全区模拟覆盖，额外截图为 `artifacts/footer-style-{width}-{height}-{scale}.png`（当前组合为 `1280-900-1.5`、`320-844-1.5`、`390-430-1`）。测试不发送 prompt/queue，不写模型/思考/工具配置或凭据，不调用真实模型；注册表 provider 错误与描述符隔离另由 Rust 单测覆盖，不是浏览器外部插件加载测试。原生上下文 modal 仍从 inspector「查看任务上下文」→ 管理上下文（「管理当前上下文」按钮）进入，footer 不提供入口。

实现位于 `apps/work-ui/src/features/observability/footer_plugins/`，typed 只读 registry 默认依次注册 `session-identity`、`session-usage`、`context-usage`、`tool-activity`。偏好使用 `localStorage` 的 `rpi-work-footer-v1`：`version: 1`，`plugins` 必须含四个 ID 各一次，每项为 `{ id, enabled, hidden_items }`，数组顺序即插件顺序；设置入口为「设置 → 底部状态栏」。这是客户端内置注册，不是节点安装、任意外部代码/manifest 注入或 rpi `setFooter` API，资源库 Extension 设计预览不因此可执行。此次样式修正不改变注册表、偏好或计算口径。指标外层只读 `span` 使用 `role="group"`，`aria-label` 包含完整标签、值与说明；内层 `.footer-value` 设 `aria-hidden="true"` 避免重复朗读。没有 `title`、tooltip 属性或伪元素提示，不新增 popup/Tooltip 组件，点击或悬停均不打开弹窗。长模型 ID 完整内联换行，不省略、不使用 ellipsis。字体沿用 UI 的 sans 并使用 `tabular-nums`，默认桌面 12px、手机 11px（最小 11px），行间距 2px。正常字体、默认指标的手机 footer 高度 ≤80px；大字体、高级指标或长 ID 允许自然增高，不硬裁切。上述浏览器结果是实际运行产物，但不代表完整回归或 Android 实机验收完成；真实手机软键盘与安全区仍待验证。

`verify-mobile-composer.cjs` 另检查 footer 在桌面及明暗主题 390/320px 下正常字体、默认指标的高度 **≤80px**、不横向溢出（不将此限制推广至大字体、高级指标或长 ID），指标无按钮/链接/键盘入口，点击不打开弹窗，累计输入/输出、缓存与上下文保持只读；附件预览/移除、模型选择、草稿及发送按钮的原有约束继续保留。这些浏览器布局与交互断言不替代 Android 实机软键盘验收，也不代表本次重跑通过。

`verify-turn-timing.cjs` 使用正式 WASM 和隔离 HTTP/WebSocket fixture 检查工具卡实时耗时、并行工具独立冻结、回合到结算才结束且不相加、失败／中断及缺失开始／结束不补算、历史假时间戳不冒充计时、历史合并后按工具 ID 保留已观测耗时、会话隔离、时钟桥接失败、草稿与焦点、键盘折叠及明暗主题 1280／390／320px。测试按需打开右侧检查器的“回合耗时”，折叠历史不挂载隐藏步骤节点；此用例不提供节点计时元数据，验证的是包含等待／传输／调度的客户端接收计时，而非服务端执行耗时或持久化回放。结果为 `artifacts/turn-timing-validation.json`，全程零模型／发送调用，不代替 Android 实机验收。

`verify-tool-spacing-timing.cjs` 将用户反馈的两条 bash 命令作为纯 fixture 文本，不执行命令。覆盖孤立历史工具结果、正常调用／关联结果、混合相邻卡片及隐藏容器，明暗主题与 1280／390／320px 下简洁模式相邻卡片间隙不超过 1px，详细模式不添加整轮容器留白。工具卡读取节点真实 `webTiming: {elapsedMs,running}`：结束结果优先于调用元数据，缺失／负数／字符串／仅 running 历史仍未知，零值有效；live 节点基数继续计时、真实结束后冻结、投影转历史及重新打开会话保留节点耗时，跨会话相同 ID 不串用。工具徽标说明区分“节点执行耗时”和兼容的“客户端接收计时”，不从消息时间戳或其他回合推算。修复前隔离复现的 16px 多余间隙与丢失徽标保存在 `artifacts/tool-spacing-timing-red-validation.json`；当前报告为 `artifacts/tool-spacing-timing-validation.json`，零真实模型／凭据／配置写入，不替代原生桌面或 Android 实机验收。

`verify-tool-newlines.cjs` 用正式 WASM 和隔离 HTTP/WebSocket fixture 检查空流式快照、仅换行的增量、仅空白历史文本及空白＋工具调用不生成助手头像或额外空行；真实正文出现后仍正常显示，空快照可清除正文但保留稳定消息槽。简洁／详细模式覆盖 1280／390／320px，保留工具输出的真实换行／空行、原文复制、私有 thinking 隐藏与草稿；结算及重新打开会话后不复发。运行：`node tests/support/run-static-ui.cjs tests/e2e/verify-tool-newlines.cjs`。红测报告 `artifacts/tool-newlines-red-validation.json` 记录修复前 80px／84px 间距；当前报告 `artifacts/tool-newlines-validation.json`。不执行真实工具或模型，不替代原生桌面／Android 验收。

`verify-composer-input.cjs` 覆盖 70 条 Markdown 历史下的连续输入和合并存储写入、中文／emoji 编辑及光标／撤销、组合输入与 229 确认不误发、Shift+Enter、流式编辑及显式入队、退出前最后一字、重载和会话隔离、引用插入、延迟确认不清新 revision、100 份其他草稿、拒绝／损坏存储和 IME 安装失败降级。另用 Chromium CDP 原生 composition 和延迟旧 input 快照，检查不回写正在编辑的 value、不残留拼音；覆盖 composition 中两次引用与原生 input 交错的延迟合并、确认清空不得覆盖新输入。结果位于 `artifacts/composer-input-validation.json`，包含本机 debug WASM 的对照数据；浏览器 IME 模拟和本机耗时不代表 Windows 桌面／实体手机输入法或速度验收。只向隔离 fixture 发送指定测试消息，不调用模型。

本轮会话稳定性回归补充：

- `verify-composer-input.cjs` 在启动前显式开启 `__rpiAppRenderProbe`，统计实际 App 执行（不是 DOM mutation），要求非空草稿连续输入 36 字符时重算次数不随字符数增长，并以空／非空按钮变化作探针正向校验；同时记录 input→下一帧延迟和 long tasks，仅作本机诊断。正常页面只做一次探测，不逐次渲染跨桥计数。
- `verify-tool-newlines.cjs` 覆盖正文／工具／正文的原始块顺序、“正文＋工具 A”后接另一消息工具 B/C 的零额外分组空白，以及多个 projection 工具合入历史后的展开状态保留、失败手动关闭不重开、跨会话同工具 ID 隔离。展开记录仅保存在当前 scope 内存，刷新或切 scope 后丢弃。
- `verify-message-stream-order.cjs` 增加历史正式回复与匿名完成事件重叠（含 history-only `webTiming`）的打开／切回／重连单份显示、未结束流保留、不同明确 ID 的同文同时间回复保留、实时匿名 user 段及正式 ID 晋升。`crates/client/work-client/src/snapshot_dedup_tests.rs` 另覆盖完全无 ID 的同文 user 多轮、快照覆盖一对一、结构不匹配不吞并、无效 timestamp 不当覆盖证据及工具确有结果才过滤。不会按正文全局去重，也不声称无身份、无时间戳的任意历史都可无损关联。
- 重复渲染二次回归：`crates/client/work-client/src/duplicate_regression_tests.rs` 覆盖同一 open segment 在首次正式 ID 到达时修订 timestamp、整段快照覆盖、已存在 canonical 历史与匿名行／投影的身份收敛，以及晋升后的无正文 delta 继续更新同一投影槽。只凭明确 alias 转移 owner，既不按正文全局去重，也不把尚无正式身份依据的未完成流当成已覆盖。`verify-message-stream-order.cjs` 对应验证完成前后、结算与重连的单份显示和稳定 DOM 槽。
- 重复渲染三次回归：新增缺省 role 的 delta／user end、快照内未结束的 canonical 晋升、分页与已证明 alias 重叠的先失败 fixture。snapshot replay 复用 live owner transfer，分页按带客户端来源标记的 canonical identity 归并并保留最新 tail；无来源标记、未晋升匿名 ID、不同正式 ID 的同文消息仍保留。`verify-message-stream-order.cjs` 追加未结束晋升／无 role delta 的打开与重连，以及旧页正式 owner 与 live alias 的单份显示；同时断言最新正文与增量实际可见、更新保留同一 DOM owner。快照恢复同时发布既有历史 owner 的新正文，不追加 event-only 历史行；未含 start／完整正文的事件窗口保留权威历史，不把已持久化的 delta 再追加一次。另有混合正文／工具与私有块的分段保持测试。证据：`artifacts/message-dedup-third-{client-red,ui-red,web-red,client-tests,ui-tests,e2e,web-build,native-build}.log`；构建不代表正在运行的桌面客户端已替换。
- **真实 compact wire 重复回归**：`verify-compact-message-identity.cjs` 使用上游 `rpc/events.rs` 的顶层 `messageId` 与无 `message` 对象的默认 compact delta，而不是把 ID 放进 `message.entryId` 的旧简化 fixture。修复前的正式 bundle 在 `message_end` 后显示两份相同回复；修复后断言流中／结束／结算、匿名晋升且 timestamp 改变、快照重开和真正断流重连都只有一份，并保留不同正式 ID 的合法同文同时间回复。Rust 另覆盖 user echo、无 ID 兼容、事件 ID 不当消息身份和 native 合成流。运行：`node tests/support/run-static-ui.cjs tests/e2e/verify-compact-message-identity.cjs`；报告 `artifacts/compact-message-identity-validation.json`，修复前截图 `artifacts/compact-message-identity-before-fix.png`，4 个初始失败单测的记录 `artifacts/compact-message-identity-before-fix.log`。不调用模型，不按正文全局去重，不冒充 live backend／桌面／Android 验收；正在运行的旧客户端不会因测试构建而自动替换。
- `verify-btw-message-identity.cjs` 用正式 WASM 与隔离 HTTP/WebSocket fixture 验证旁问 projection→完成消息→正式结算的缓存回收、关闭／重开不重复、相邻回答和主草稿保留；仅向本地 fixture 发送一条指定旁问，不调用真实模型。BTW 单测另检查已完成匿名 owner 的后续 canonical 晋升、按 slot 重命名及多片段回收；重建时清除 slot 关联，避免凭旧索引吞消息。运行：`node tests/support/run-static-ui.cjs tests/e2e/verify-btw-message-identity.cjs`；报告 `artifacts/btw-message-identity-validation.json`。这些用例不代替现场复现、原生桌面或 Android 验收。

`verify-user-timeline.cjs` 检查时间线默认隐藏，由顶部“显示时间线／隐藏时间线”按钮控制；支持 Enter／Space、pressed／expanded 状态和 44px 点击区域。同一客户端切会话保留显隐状态，刷新恢复隐藏；隐藏时卸载时间线、清理监听和预览，不为凑齐十个刻度自动补载，普通历史分页仍可使用。显示后，初始 48 条消息中输入不足十条时才通过真实游标连续补载，够十条或历史耗尽即停。33 项浏览器用例覆盖显示／隐藏的草稿和阅读锚点、隐藏期间迟到分页不继续补载、工具密集／纯工具页、错误及手动重试、缺失能力／游标／旧节点、切会话晚到结果、拖动、真实 CDP 触摸、键盘及明暗主题 1440／390／320px。运行：`node tests/support/run-static-ui.cjs tests/e2e/verify-user-timeline.cjs`；报告 `artifacts/user-timeline-validation.json`。零模型调用，不替代原生桌面或 Android 实机验收。

`verify-backend-switcher.cjs` 检查“会话详情”和“设计引用”共用一个“会话设置”窗口，顶部设置入口默认详情，设计引用入口直达设计页；Agent 只在“设计引用 → 执行后端”中出现一次。保留真实节点目录的可用性限制、离线／绑定待确认锁定；无会话或新建中不展示可修改的会话配置；选中会话后确认创建同项目独立新会话、取消／Escape 保留草稿、切会话销毁旧确认状态，以及 1440／390／320px 明暗主题与 44px 入口／选项。运行：`WORK_BACKEND_ASSETS=target/parity-web-server node tests/e2e/verify-backend-switcher.cjs`；报告 `artifacts/backend-switch/validation.json`。使用正式 WASM 与隔离 HTTP/WebSocket fixture，不启动真实 Agent、不调用模型，不代表当前运行的桌面已更新。

`verify-session-settings-conflict.cjs` 检查其他操作留下的全局 409 不会被当成会话设置错误，原错误仍保留在工作台；真实配置／工具目录读取冲突显示明确来源并支持手动“重新读取配置”，不会自动重试、重放 setter 或更改工具。真实配置修改失败仍在对应组件显示；关闭重开不残留旧配置错误，草稿保持。运行：`node tests/support/run-static-ui.cjs tests/e2e/verify-session-settings-conflict.cjs`；报告 `artifacts/session-settings-conflict-validation.json`。拒绝的修改请求全部在隔离浏览器中拦截，不写真实会话、不调用模型。

`verify-mobile-resources.cjs` 检查统一会话设置在明暗主题 390／320px 的主标签、六类设计资源、单一窗口、键盘与草稿保持；`verify-ui-multi-package.cjs` 检查合并后多包选择、版本固定、Skills／Tools／提示词独立配置及本地指令持久化。运行：`node tests/support/run-static-ui.cjs tests/e2e/verify-mobile-resources.cjs tests/e2e/verify-ui-multi-package.cjs`；仅使用隔离 fixture，不连接真实模型。

`verify-history-pagination.cjs` 使用隔离 HTTP/WebSocket fixture 的 150 条真实 ID 消息检查 48 条初始快照、上滚多页与到达开头、请求去重、失败重试、运行结算顺序、异步媒体尺寸后的阅读锚点及持续指针手势优先。另验证切会话、重连中的晚到响应、离开会话标签后的取消／重试、缺失能力及忽略游标的旧节点、390/320px、草稿和零模型调用。状态测试还在 `crates/client/work-client/src/message_queries.rs` 与 `history_window.rs`；浏览器结果写入 `artifacts/history-pagination-validation.json`。这不替代真实桌面、Android 或 live HTTP 节点验收。

`verify-message-images.cjs` 验证真实 `type=image / mimeType / data` 形状的 PNG/JPEG/GIF/WebP 历史消息、关联／孤立工具结果、实时 image-only 消息及后续字幕增量。另检查文本／图片顺序、thinking 排除、输出复制不含 base64、惰性 Blob 查看器的焦点／缩放／快速重开、更新时关闭及撤销旧 URL、迟到 bitmap 结果清理、错误编码／伪装 SVG／外部 URL／尺寸超限／真实解码失败、390/320px、阅读跟随、导航卸载和草稿，且零模型调用。报告为 `artifacts/message-images-validation.json`；单测还覆盖 native 连接合并与 Trace 结构化图片遮盖，不等于真实桌面或 Android E2E。

`verify-message-stream-order.cjs` 覆盖旧回复仅来自快照 events 投影时，新消息带完整正文的流式更新仍位于旧回复和工具下方；partial→完整历史、工具卡取得历史 owner、超过 48 条的结算快照、匿名→正式 ID、下一轮继续输出、历史前插、切会话及重连均保留消息次序和稳定 DOM 锚点。脚本先在旧两组循环渲染上失败，再用正式 Web 与隔离 HTTP/WebSocket fixture 验证，零 prompt／队列／停止／模型请求。报告为 `artifacts/message-stream-order-validation.json`，截图为 `artifacts/message-stream-order.png`，红测为 `artifacts/message-stream-order-red.log`。`verify-reply-follow.cjs` 另覆盖确认返回前的早到回复必须位于 optimistic 用户消息之后，正式 echo 取得同一个 DOM 槽。`transcript_order.rs` 和 `activity.rs` 单测覆盖迟到远端用户的 canonical 顺序修正，以及 pending 期间分页不得把旧的同文用户消息当成 echo。仅验证客户端展示，不改真实记录，也不代表正在运行的桌面客户端已更新。

`verify-cross-turn-message-identity.cjs` 针对新回复出现在上一轮的身份归并问题：上一轮与当前轮的明确消息 ID 不同、但 timestamp 相同时，逐步检查完整正文更新、message_end 与 canonical 结算均保留旧正文及原有 DOM，新回复位于当前用户问题之后；另覆盖省略 message_start 时明确 ID 改变、用户边界之后无 agent_start 的匿名首个增量，以及没有 timestamp 的匿名→正式 ID 迁移。脚本在修复前的正式 Web 构建上复现了旧正文被新正文覆盖，修复后使用隔离 HTTP/WebSocket fixture 通过，零 prompt／模型／凭据写入。报告为 `artifacts/cross-turn-message-identity-validation.json`；客户端单测还覆盖精确 ID 优先、歧义时间戳不得吞并消息、工具结果边界及图片字幕不得写入上一条消息。不替代原生桌面或 Android 实机验收。

`verify-tool-message-order.cjs` 用正式 Web UI 与隔离 HTTP/WebSocket fixture 检查无正文的子 Agent 调用仍保留消息位置、参数及唯一结果卡；31 次工具调用超过 48 条快照窗口后，逆序返回、回合结算及下一轮消息都不能改变调用卡片顺序或丢失旧调用。报告为 `artifacts/tool-message-order-validation.json`，零 prompt／队列／真实模型调用；不替代真实桌面或 Android 验收。

`verify-conversation-width.cjs` 使用正式 Web 与隔离 HTTP/WebSocket fixture 检查 1280／1920／2560／390／320px 下正文、Ask、输入框、脚注与统计共用 1200px 最大宽度及中轴，保留消息时间线占位；队列弹窗共享最大宽度变量，并按视口／安全区收缩。页面与默认统计不横向溢出，320px 下仅压缩指标间距，不缩字或额外隐藏指标；高级指标保留内部滚动。另检查 Markdown 原生 HR 的单条柔和语义线、侧栏拖动线 hover／drag 减淡和清晰键盘焦点。当前 Markdown 解析器未将 `---` 转为 HR 时，测试在真实 Markdown 容器中插入原生 HR，仅验证 CSS，不声称验证了解析器支持。报告为 `artifacts/conversation-width-validation.json`，截图为 `artifacts/conversation-width-{width}.png` 及 `conversation-width-queue-{width}.png`；零 prompt／队列写入／模型调用，不替代桌面或 Android 实机验收。

`verify-sidebar-loading.cjs` 在明暗主题 1280／390／320px 下检查标题右侧 loading、运行中即使有草稿仍显示停止、点击只 abort 并保留草稿、停止中禁用与迟到确认不覆盖结算；其他发送／工具等待／确认／失败和减少动态效果检查继续保留。报告为 `artifacts/sidebar-loading-validation.json`。

`verify-loading-motion.cjs` 验证当前会话与“正在处理”的圆圈按最近两秒的实时输出增量平滑变速：无输出时普通模式为 0.8 秒／圈、减少动态效果模式为 1.6 秒／圈；高速输出时分别限速至最快 0.25 秒／圈和 1 秒／圈。接口缺少逐 token 计数，正文、思考和工具参数的增量大小仅用于估算动画节奏，不作为精确 tokens/s 或计费统计。脚本检查低速→高速→静默、工具等待与结果、跨会话隔离、新回合重置、媒体偏好动态切换，以及改速保持同一动画实例和时钟推进。其他装饰动画仍关闭，等待用户确认和结算后不显示旋转进度。使用正式 Web UI 与隔离 HTTP/WebSocket fixture，零模型调用；报告为 `artifacts/loading-motion-validation.json`，不替代桌面或 Android 实机验收。

`verify-attachment-send.cjs` 使用正式 Web/WASM 与隔离 HTTP/WebSocket fixture 检查真正的附件发送，而非旧的“仅发送文字”确认：PNG/JPEG 的真实 base64 字节通过 `contentBlocks` 送达节点，文本／代码附件发送完整 UTF-8；选择和预览不触发请求。覆盖仅图片 Enter、组合发送、pending 与正式回显、拒绝后保留及显式重试、成功仅清理已确认附件且不覆盖新草稿、会话隔离、能力缺失／发送前能力丢失 fail-closed、旧节点全文文本附件，以及大图的有界 JPEG 视觉副本。运行中 Enter 拒绝图片排队、Stop 仅 abort 并保留草稿／图片，文本附件在队列明确确认后才清理。首个文件读取中禁止发送，批量部分读取／限额失败需要明确清除提示，不能自动只发送其余文件。PDF／Office／压缩包等原文件暂不支持，损坏图片、超限文本／图片均不静默截断或漏发。预览覆盖明暗 1440／390／320px、Escape、44px 点击目标和无横向溢出。报告 `artifacts/attachment-send-validation.json`，截图 `artifacts/attachment-send-{light,dark}-{1440,390,320}.png`。旧 `verify-attachment-send-guard.cjs` 保留为该脚本的兼容入口，不再验证已退役的本地-only 确认流程。相关协议测试：`cargo test -p work-client --test prompt_content`、`cargo test -p work-node-runtime --test prompt_content`、`cargo test -p work-node-runtime --lib workspace::access::prompt_tests`；仅使用 stub 子进程，不启动真实 Agent／模型，不替代桌面／Android／真实视觉模型验收。

`verify-workspace-sync.cjs` 使用正式 Web/WASM、两个独立 browser contexts 和隔离 HTTP/WebSocket fixture，两个客户端分别通过 UI 连接并登录测试密码节点。覆盖 UI 创建／重命名会话、置顶／归档、新增／重命名项目的跨客户端自动同步，被动端当前会话与草稿保留、visible／窗口 focus 触发刷新、刷新失败保留旧列表并自动重试、单个目录挂起不阻断另一个成功响应，以及 route 延迟的 projects／sessions 旧响应在本端已确认修改、切换节点和同地址新 epoch 后不得回写（包括短暂回写监测）。也检查当前会话跨端改名会更新标题而不重置草稿。常规同步等待使用最多 15 秒的条件／响应等待，不靠固定 sleep 或全局 reload；visible／focus 为浏览器事件模拟。报告为 `artifacts/workspace-sync-validation.json`，成功截图为 `artifacts/workspace-sync.png`。仅写隔离 fixture 的目录／会话元数据，不接触真实节点或用户数据，不发送 prompt／queue、不调用模型；不代表原生桌面 GUI 或 Android 实机已验收。

`verify-new-conversation.cjs` 检查项目右侧直接创建会话、当前项目继承、输入框聚焦、重复点击、失败重试、快速切换项目、草稿保留、首条消息自动命名及 390/320px 移动端操作。

`verify-session-creation-isolation.cjs` 用两个独立 HTTP/WebSocket fixture 和有超时的显式 POST／详情响应 gate，验证迟到成功／失败在切节点、同地址重连及 A→B→A 后不能污染新连接、选中旧会话或清除新创建 busy。另覆盖创建期间进入设置、离开后返回、切变更标签、切已有会话时的自动打开撤销、失败状态隔离、草稿／本地附件保留、已提交成功会话从真实列表重新发现、正常新建聚焦和双击去重。全程零 prompt／queue／模型调用；报告为 `artifacts/session-creation-isolation-validation.json`，修复前的导航抢占复现记录为 `artifacts/session-creation-isolation-before-fix.json`。浏览器取消请求不意味着服务端已回滚，fixture 不替代真实桌面／Android／live backend 验收。

`verify-sidebar-resize.cjs` 用正式 Web UI 与隔离 HTTP/WebSocket fixture 验证左侧栏右边缘的真实鼠标拖动、指针捕获、200–480px 及视口40%限幅、松手结束、取消／丢失捕获／失焦／跨断点恢复、方向键／Home／End调宽、双击默认值、保存与刷新恢复、异常存储容错，以及不重挂输入框、不丢草稿。还检查桌面／平板默认值、390px 手机抽屉与单一控制器清理。零 prompt／队列／真实模型调用；报告为 `artifacts/sidebar-resize-validation.json`，截图为 `artifacts/sidebar-resize.png`。不替代桌面 WebView 或 Android 实机验收。

`verify-sidebar-projects.cjs` 是 Codex 风格侧栏专项检查：1440/390/320px 下默认五条最近会话、置顶优先、显示更多/收起及剩余数量、较旧选中会话额外保留、`pi-rust` 展开/折叠的 reload 持久化、独立项目滚动与固定顶部新建/底部设置、idle 状态隐藏、hover/键盘 focus 菜单及项目新建按钮可操作。脚本通过隔离 fixture API 创建并命名 24 条会话、置顶与归档种子会话，仅拦截 GET 列表固定排序时间，不断言特定相对时间文案、不发送消息、不修改 fixture 文件。结果写入 `artifacts/sidebar-projects-validation.json`，各尺寸截图为 `artifacts/sidebar-projects-{width}.png`，失败时附加 `artifacts/sidebar-projects-failure.png`。

### Desktop owner HTTP (isolated)

`cargo test -p work-node-service --locked` compares HTTP and native access to the **same** `Arc<LocalWorkspace>` with temporary registries, random ports and an RPC fixture, including identical child PID, metadata and WebSocket boundaries. It also covers default-deny sharing, live WebSocket revocation, old-cookie invalidation, rapid off/on transitions, and survival of the same owner/child PID. It never attaches to the installed desktop.

For formal Web -> real native owner E2E (no route mocks): build the Web bundle, then `cargo build -p work-node-service --bins --examples --locked`. Set `WORK_OWNER_FIXTURE` to `<cargo-target>/debug/examples/desktop_fixture[.exe]` and `WORK_RPC_FIXTURE` to `<cargo-target>/debug/work-server-rpc-fixture[.exe]`, and run `node tests/e2e/verify-desktop-owner-http.cjs`. The test starts only the isolated fixture and shuts it down cooperatively; report: `artifacts/desktop-owner-http-validation.json`.

The owner browser test now also verifies the hosted password gate (including 390/320px phone browsers), zero workspace API reads before login, wrong-password rejection, hosted-origin precedence over stale node history, automatic session loading and valid-cookie reload. QR API checks use a separate unauthenticated request context: authenticated issue, one-time redemption, replay rejection and sharing revocation. Additional report: `artifacts/entry-owner-browser-report.json`. Loopback deliberately disables phone authorization QR rendering; these tests do not claim Android camera/device acceptance.

`crates/transport/work-node-service/tests/qr_auth.rs` and pairing unit tests cover hosted metadata versus explicit 404, Host/Origin/marker boundaries, ticket TTL/capacity, hashing, concurrent single-use consumption, node isolation, session limits and sharing generations. Client unit tests check the authoritative Cookie-based auth state and refusal to forward redemption credentials through redirects. `mobile_entry.rs` protocol tests reject ordinary address QR, malformed/expired/overlong authorization data and invalid phone addresses; the native entry component is also type-checked by desktop unit tests. An actual Android target check still requires its NDK/clang toolchain; no successful APK or physical-device validation is implied. Independent Web refresh/share/auth/lifecycle regressions run via `tests/support/run-static-ui.cjs`, whose well-known path correctly returns 404.

`verify-authorization-qr.cjs` tests the authorization-QR UI using formal Web assets, isolated HTTP fixtures and intentionally fake QR issuance. It decodes the actual generated SVG, checks exact fields without logging the payload, current-node versus address-draft isolation, late responses after switching nodes, expiry pixel removal **without automatic reissue**, singleton reuse on repeated opening and Settings/layout remounts, and absence of ticket text/copy/localStorage leakage. Run with `node tests/support/run-static-ui.cjs tests/e2e/verify-authorization-qr.cjs`; counts/results-only report: `artifacts/authorization-qr-validation.json`. Server authenticity and single-use behavior are covered separately by the real-adapter tests; this UI test is not Android hardware validation.

### Desktop default LAN entrance

The desktop now defaults to an authenticated IPv4 LAN listener (`0.0.0.0:13031`) and enables sharing only when its preference is absent; saved false and invalid/unreadable preferences remain closed. App API and browser Web origins are resolved separately. Startup mounts one authorization component at workspace scope; readiness may trigger one automatic App authorization offer, never a timer-driven renewal. Ordinary address-only Web sharing is still explicit and still requires a real Web entrance.

- `app/local_http_address.rs`, `sharing_preference.rs` and `settings/authorization_offer.rs` test API-without-assets, explicit loopback/proxy precedence, first-run versus saved/invalid policy, and automatic-offer consumption. `artifacts/desktop-lan-qr-preference-before-fix.log` records the initially failing default-policy test.
- Build `work-ui` and the credential-free `rpc-fixture`, then run `node tests/e2e/verify-desktop-lan-http.cjs` on Windows. This launches the **actual desktop binary** with isolated temporary data/profile and random API ports, verifies private-route/loopback health against its freshly created node identity, Host/Origin and auth rejection, real single-use issuance/redemption, and saved-off/invalid/loopback restarts. Passwords stay in the child environment; tickets/cookies never enter logs/reports. It verifies **HTTP, not QR rendering or another device's routing/firewall**. Reports: `artifacts/desktop-lan-http-validation.json` and `desktop-lan-http-run.log`. Some execution environments do not expose child sockets to the caller's OS enumeration; the report records this, and uses verified process identity plus the isolated node-id match, not a claimed OS listener observation.
- `node tests/e2e/verify-desktop-lan-qr.cjs` additionally attempts real desktop WebView2 QR decoding and startup/modal interaction through a loopback-only CDP test port. This requires WebView2 to honor debug arguments. On the current machine it does not expose the requested debugger; `artifacts/desktop-lan-qr-native-validation.json` records the **failed/blocked** native GUI attempt. Do not count this as a passing desktop QR or Android-camera acceptance test. No raw QR screenshot, ticket, DOM dump or credential-bearing exception is saved; any successful screenshot masks the QR.

No firewall rules, existing desktop/service processes or user workspace data are changed by these tests.

## Default port contract

`node tests/e2e/verify-default-ports.cjs` checks the Rust/CLI/Taskfile/migration-tool defaults and evaluates the actual dev/review port expressions with unset, empty, explicit low-port and zero environments. Defaults are desktop `13031`, standalone `13030`, Web `14180` and review `14182`; overrides stay authoritative. This source/configuration check starts no services and does not substitute for the desktop HTTP or browser UI tests.

`node tests/e2e/verify-default-port-listeners.cjs` additionally starts the actual Web/review scripts with port variables unset, checks Web `14180` → owned API fixture `13030` and reference `14182`, then closes only its own children. Build Web assets first; occupied fixed ports fail the test rather than attaching to or stopping any existing service. No real node/model calls.
