# 正式实现审查

> 2026-10-27 最终冻结验收完成：29套198个正式应用行为检查、5视口63处主区几何、125组页面配对，Rust/native/WASM与真实模型链路通过。队列本地提案、元Agent/Skills恢复入口及所有前批修复均闭环。当前结论与未验证边界见 [验收说明](ui-parity-acceptance.md)。以下按过程保留，旧失败、旧端口和旧“待修复/未完成”叙述不代表最终构建。

2026-10-25。审查对象 `apps/work-ui`、`crates/work-client`、`crates/work-core`；基准 `design/prototype/`。所有结论需由后续正式 UI 验收闭环，不以单元测试数量替代验收。

## 初审（修复前）

| 级别 | 问题 | 具体影响 | 验证/修复要求 |
| --- | --- | --- | --- |
| P0 | 760px 媒体查询未闭合 | 桌面新版 CSS 无效、移动侧栏隐藏、旧会话面板挤占对话 | 正式 Dioxus 多视口截图和实际抽屉操作 |
| P0 | 每次操作重新创建 WorkClient/cookie jar | native login 的认证保存在短命 client 中，后续 connect/prompt/SSE 使用新 jar，受保护节点不能正常使用 | 节点连接生命周期内共享 client；切换节点隔离会话；本地 mock cookie 往返验证 |
| P1 | WASM 默认 fetch 未显式携带跨 origin credentials | 手机/Web 客户端跨端口认证 cookie 不能可靠发送 | 按节点部署策略明确同源/反代或 credentials + CORS 支持，实际验证 |
| P1 | open_project 未切换/清理 selected_session 和 SSE | 项目标题已是 B，发送仍可发往 A 会话；文件与聊天所属项目不一致 | 项目选择恢复该项目的会话或进入无会话空态，停止旧订阅 |
| P1 | composer 是全局单字符串且发送前清空 | 切会话串草稿；失败丢失文本；新输入可能被旧请求覆盖 | 按 session 保存草稿，成功后只清除对应提交，失败保留/恢复，防重复提交 |
| P1 | 消息历史被压成“历史#N · role”字符串 | 丢失结构、工具/附件/富文本；历史和 projection 可重复显示相同助手消息 | 使用结构化消息和稳定ID，历史/当前输出只显示一次 |
| P1 | SSE 断流仅对 EOF 重建，错误直接退出，无 backoff | 网络抖动不能自动恢复；持续 EOF 可快速循环请求；状态仍可能显示在线 | 有界退避、连接状态更新、游标恢复；不重发业务 prompt |
| P1 | normalizer 对 message_start/end 未检查 role | 用户或工具消息可能被渲染为助手；默认 assistant-message ID 还可能覆盖前一轮 | 使用上游事件形状验证 role/id/run 边界 |
| P1 | 异步 mutation 写全局状态未绑定节点/会话世代 | rename/dialog/queue 等请求完成后可能污染新会话；发送状态显示到别的会话 | 所有回写检查目标；每个实体独立状态，切换时取消或忽略旧响应 |
| P1 | 模型、thinking、tools 配置在 client 已有 API，但 UI 禁用 | 误把未接入 UI 写成上游协议缺口 | 接通现有API并验证实际发送的命令 |
| P1 | Settings/主题/会话能力/追踪/元Agent等未迁移 | 与已确认原型的交互信息架构差距很大 | 按 docs/archive/ui-parity-goal.md 完成逐页清单 |

## 已有验证的实际范围

初审 `cargo test --workspace`：21 个 transport/reducer/node 测试通过，work-ui 0 个测试。**不能证明 UI 已还原。**

`artifacts/ui-baseline-audit.json` 保存了修复前 WebView 与原型的布局观测。隐藏 WebView 截图捕获并不稳定，尺寸变更可能出现旧帧，因此不把其像素数据用作严格差异率；正式验收改为同源源码编译的 WASM/Dioxus + 独立 Chrome，另保留 native 编译检查。

## 状态

- 第一批基础布局、设置、基础主题：rpi 已实现，独立12项检查通过；主题预览/取消/导入导出等完整行为仍待补齐。
- 后续功能与状态正确性：待实现和验证。
- 整体 1:1 还原：**未完成**。

## 第一批独立浏览器复核

真实源码已成功编译为 WASM 并在独立 Chrome 运行（不是原型替身）。测试节点仅为 `tests/fixtures/ui-fixture.cjs` 的本地 HTTP/SSE fixture，无真实模型请求。

- 设置内真实连接链路、snapshot/SSE 接收已走通。
- **阻断项：设置返回工作空间后，DOM 保留 `style="display: none"`，标题和历史已恢复但整个对话区仍隐藏。** 复现和截图见 `tests/e2e/verify-ui-parity.cjs`、`artifacts/ui-parity-failure.png`、`artifacts/ui-parity-validation.json`。已退回 rpi，第一批尚未验收通过。
- 首次 debug WASM 带 Dioxus 热更新 WebSocket，静态服务器返回404；正式验收使用 release 构建以分离调试器噪声与应用错误，不能隐藏真实业务错误。
- 搜索控件尚未还原为原型的单行触发按钮；项目表单尚未统一使用原生 dialog。

更新：rpi 已在第一批末尾修复 workspace 显示切换并统一项目 dialog。release WASM 的 `verify-ui-parity.cjs` 12项通过：真实fixture HTTP/SSE恢复、节点设置位置、5视口紧凑布局、tablet Inspector、手机抽屉/会话modal、Reddit深色持久化、无浏览器错误。历史失败截图仍保留作过程证据，不代表最新状态。基础布局检查通过仍不代表原型页面与交互全部还原。

第二批前，`verify-ui-behavior.cjs` 在独立测试节点确认了7个失败：空队列占整块面板、footer缺指标、草稿跨会话串用、失败发送丢文本、切项目仍指向旧会话、变更文件搜索无效、受保护节点登录后请求无认证。完整证据 `artifacts/ui-behavior-validation.json`。这些都是已交给rpi的第二批范围。

用量语义审查：本机 `pi-rust/crates/rpi-ai/src/types.rs` 的 Usage.input 是未缓存输入，cacheRead/cacheWrite单列，不能按cacheRead/input算命中率；get_state.stats已提供 `cacheHitRatio`、`cachedTokens`、`uncachedTokens`、`costTotal`。context窗口字段是 `window`。测试fixture已按真实字段形状修正，75%命中场景为未缓存8k+缓存24k。应优先采用宿主的ratio，缺失时按已知口径计算，未知显示—。

进一步核对：`session/state.rs:248` 的 uncachedTokens 汇总 input+cacheWrite，因此会话总输入为cachedTokens+uncachedTokens，总输出可在totalTokens>=总输入时相减得到。`modes.rs:633` 的真实 agent_settled 不带 usage，不能只等settlement.usage才显示footer；fixture已移除这个非真实字段，使用get_state.stats验收。

第二批末尾更新：footer真实输入/输出与Esc取消都已修正。作者验证27项Rust测试、13项基础parity、8项behavior通过。独立流式检查7/8通过，当前用户消息仍缺失，第三批继续修复。

## 精确布局差距（第二批后）

`node tools/review/audit-ui-geometry.cjs` 在同样1440×1000、Paper浅色标准密度对照原型，数据差异不影响以下布局结论。完整数值见 `artifacts/ui-geometry-review.json`，变更视图截图 actual/reference 也保存在artifacts。

- 正式header宽1204，原型904；因为原型Inspector是根布局的第三列，覆盖从y34到1000，而正式Inspector在header下面（y86到1000），足足错开52px。Inspector标题正式44px，原型61px。必须还原列/行归属，不能只检查header高度52px。
- 原型在变更页隐藏Inspector并占满主区域：文件列表260px、diff到窗口右缘；正式仍留Inspector，列表220px且左右额外padding22px，diff只剩640px。需要原型review-mode的完整宽度。
- composer原型白色外框包含textarea+底部控件，x266宽844；正式只给textarea画框，x258宽860，控件在框外。
- 全局字体正式13px/normal，原型14px/1.6；timeline padding正式28px30px20px，原型28px38px25px。这些都需要对照原型修正。

上述差距说明，已有基础几何与行为测试通过仍不等于1:1；最终必须跑精确对照并看截图。

## 独立模块审查进展

- 元Agent首稿存在RSX/Rust闭包编译错误，以及WASM中std::time不可用问题，均退回rpi修复。随后补了异步revision/source校验、发布成功回执与双击版本保护。
- MetaAgentPage当前发布接口是 `Callback<Value, Result<String,String>>`，资源库拒收不能登记发布。
- 独立release Dioxus模块浏览器验收8项通过：创建、结构校验/测试、发布、编辑失效、不可变导出、刷新持久化、320/390源码编辑、无JS错误。证据 `meta-module-validation.json`。**这还不是完整应用集成通过。**
- 资源库首稿18个编译错误已修，完整manifest/源码/快照保持、版本冲突、owner隔离、原子导入与节点回执边界的数据测试已通过。浏览器审查发现导入弹框内看不到成功/失败消息，正在修复；尚未发模块ready。
- 流式实时用户消息补入已修，集成者报告现有3套正式UI检查均通过；精确布局/项目文件仍在独立修复，不把旧测试结果当作最新全应用完成证明。

## 后续独立审查与真实节点验证

- 正式应用四套浏览器脚本基线36项通过（13布局主题、8行为、8流式、7文件）；同主题/视口桌面10区域几何重新对照全部相同。当前新增模块仍在继续接入，最终须重新构建，不把旧bundle当最终源码证明。
- 正式资源/元Agent联动5项通过：导入完整设计包、会话固定版本/真实Tools回读、按项目用途创建能力、真实开发session+完整源码草稿+返回、发布到同一资源库。
- 这轮发现P1：普通send被静默追加“本轮资源快照”等实现元数据，已交rpi删除；快照应是客户端trace元数据，不能私改用户草稿。该项尚未在新bundle验证。
- Project组件8项浏览器+3目录数据测试通过；修复了nested Escape关闭两层、不能进入子目录、任务标题input与modal h2重复ID。正式导航接入中。
- Trace组件6项浏览器+6追踪数据测试通过（另有共享state测试）；真实边界计时/等待settlement/凭据遮盖/手机布局已核查，历史没有时间显示未知。
- 对话组件9项浏览器通过：安全Markdown、inline tool、字符串options、单选/多选/自由回答、显式permission、失败保留/取消envelope。work-client保留raw UI及别名，主程序多pending request同步仍待接入。
- Extras组件9项浏览器通过：附件文本/栅格图片安全预览、大小限制、分槽共享状态；BTW真实独立session/SSE/followUp、失败恢复、scope切换、关闭重开保留。首稿runtime hook panic与失活闭包已退回修复。
- 设置组件8项浏览器+6设置/主题纯数据测试通过（另有共享state测试）：实际QR栅格解码、Web→Rust扫码返回、关闭后迟到相机权限立即stop、Provider声明能力和requiresRestart、严格偏好导入导出/取消。相机使用test-only synthetic MediaStream，尚无手机真机硬件验证。
- 实际现有rpi-http:3030只读验证通过：health/auth/capabilities/sessions/projects，全部GET，不open/create会话、不调用模型。浏览器仅route本地静态review资产于同源，所有API是真实节点请求，证据`real-node-readonly-validation.json`。
- 真实节点拒绝来自4173的Origin：`rpi-http/src/security.rs::origin_allowed`按允许host校验，不能以fixture回显CORS的成功声称所有真实节点都允许跨站。Web建议同源部署，native reqwest无浏览器Origin；默认WASM地址/错误恢复提示交主作者修正，未改相邻仓库安全策略。

剩余关键项：主导航/追踪接入、composer/extras/ask/队列接入、滚动保持、完整自定义主题契约、最终全App静态源码检查与多视口回归。Goal仍active。

## 2026-10-25 冻结源码的阶段验收

正式release WASM重新构建后，`node tests/e2e/verify-ui-suite.cjs` 通过15套、95个交互检查点；5视口（1440/1280/768/390/320）共63处主工作区布局/内部控件行/正文排版与原型精确一致。证据 `artifacts/ui-final-validation.json` 与 `artifacts/ui-responsive-geometry-review.json`，验证期间源码及39份原型基准文件未变化。

这批修复覆盖：忙碌时Enter实际入队、跨节点草稿、多个Ask独立结算、桌面BTW第三列及手机弹框、滚动跟随/向上阅读、项目别名作用域、模态框卸载清理、自定义主题契约/持久化、扫码与Provider配置、footer统计与真实压缩终态。压缩HTTP接受不冒充完成，真实settled.summary可查看，declined会解除等待，失败保留指令且只显示一个错误。

额外实测 `node tests/e2e/verify-real-model.cjs --run-model` 通过：正式Dioxus→真实rpi-http→rpi→实际routeryo模型→流式回复与completed结算；独立review节点registry/session目录、工具列表为空。该测试是显式调用真实模型，未混入默认fixture套件；默认真实节点审查仍只有GET。

**整体goal仍active。** 这些证据不等于所有页面像素与全部工程门禁完成。当前strict Clippy明确失败：`target/parity-diagnostics/root-clippy-latest.log` 原生work-ui 137项（test140项），需要局部修复、fmt/test/native+WASM Clippy严格通过；还要复核各页面/弹框的视觉和来源覆盖，然后重新冻结源码、构建、全套回归。手机真机相机/移动原生打包、节点资源安装/设备ACL/图片上传协议边界依旧单独登记，不以模拟成功替代。
