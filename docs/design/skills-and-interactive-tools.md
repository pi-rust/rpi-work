# Skills 与交互工具展示 · v0.4

本稿补充 `ask_user`、BTW、todo、goal 和 Skills。任务仍然只有「对话 / 变更」两个标签，新增侧栏 Skills 目录和独立旁问区域。当前全部为本地交互样例，未调用真实工具或读取真实技能文件。

v0.6 更新：当前 Skills/Tools/提示词配置以会话为范围，入口统一放到会话顶部；输入框保留单次消息的技能选择。侧栏目录改名「技能库」，用于发现资源。具名 Package、下载与版本绑定以 [会话资源设计](session-packages.md) 为准。

## 1. 先区分几种能力

| 能力 | 作用 | 展示方式 | 是否影响主模型上下文 |
| --- | --- | --- | --- |
| Tool | 读文件、执行命令、检索、提问等动作 | 对话内调用卡或专用结果块 | 依宿主规则记录工具结果 |
| ask_user | 获取明确的用户回答 | 对话内问题卡，不用原始参数 JSON 代替表单 | 回答后返回结构化结果；取消不返回伪答案 |
| BTW | 不打断主任务的旁路问答 | 桌面侧栏，手机独立覆盖视图 | 不自动写入主会话；显式引用才进入主草稿 |
| todo | 当前会话分支的步骤与进度 | 对话内计划卡，折叠调用审计记录 | 按真实工具结果投影 |
| goal | 持续目标与轮次/时间预算 | 目标状态卡；目前仅展示样例 | 根据扩展实际记录处理 |
| Skill | 完成某类任务的方法和指令 | 技能目录、草稿引用、对话内载入事件 | 目录摘要与全文载入分开，不能混为一谈 |

常用能力入口在消息输入区，支持 `/ask`、`/ask_user`、`/btw`、`/skills`、`/todos`、`/goal` 与示例 `/skill:name`。这些只是原型命令路由，不能据此认为现有 RPC 已支持所有斜杠命令。

## 2. ask_user：在原位置完成提问

原型包含：单选、多选、自由输入、其他答案、补充说明、提交、取消、超时和模拟另一设备回应。

问题卡显示问题、上下文、选项说明和当前状态。建议选项可以被标注，但不默认勾选或自动提交。提交前进行校验，只有补充说明而没有答案时不能提交。

生命周期为 `Pending → Answered / Cancelled / Expired / ResolvedElsewhere`。新请求有独立 ID；历史回答只读。取消或超时显示“未回答”，不把默认建议作为答案。

待回答时，主消息输入区收紧，将空间让给问题卡；仍保留 Skills、BTW 和常用能力入口。确认工具权限与澄清需求是两类不同的卡片，不能把单次命令授权当成需求偏好。

真实实现用 `sessionId + requestId + toolCallId` 关联请求。多端竞争以节点返回的 `ui_dialog_closed` 与最终结果为准；本地点击提交不能抢先宣称宿主已接受。迟到请求和重复提交明确失效。

本机依据：

- `rpi-package/packages/rpi-ask-user/README.md` 和 `src/lib.rs`：支持 `question/context/options/allowMultiple/allowFreeform/allowComment/suggest/timeout` 等输入；保持 Pending，等待交互回答。
- `pi-rust/crates/rpi-cli/src/modes.rs`：RPC 具备 `ui_dialog_request`、`ui_dialog_response`、`ui_dialog_closed` 桥接。
- Web 当前已有 pending dialog 投影，但前端完整多选、建议、描述、超时展示仍需逐项核对与补齐，不能只靠一个通用 selector 声称完全支持。

原型的超时由“模拟超时”按钮触发，没有真实超时计时器或宿主回答写回。

## 3. BTW：隔离的旁问，不是另一条主任务

### 布局与交互

桌面使用右侧旁问区，代替当前上下文栏；手机使用同一应用内的覆盖视图。旁问输入、滚动历史、队列与取消状态独立于主任务。

- 隐藏旁问不取消生成；重新打开能恢复当前线程。
- 取消仅停止旁问生成和待处理旁问队列，保留已完成回答。
- 新话题清理当前线程；切换主会话会清理旁问历史与活动请求。
- 旁问排队在提交时记录模型/provider/thinking 绑定；后续修改主模型不能悄悄改变已排队请求。
- “引用到主输入框”保留原草稿，加入引用文本，但不发送；用户再按发送才进入主会话。
- 新回答到来时，用户在阅读历史则保留滚动位置；在底部则跟随新内容。

### 上下文边界

按 rpi-btw 的实际语义，新线程只继承最近已提交用户/助手文本的快照。它不复制未完成输出、工具调用与结果、图片、thinking 或自定义元数据，没有工具权限。

旁问输出不会自动进入主会话历史或 steering/follow-up 队列。主任务结束也不应自动取消旁问。主输入草稿、附件、指定技能不能被旁问悄悄使用。

本机 `rpi-btw/README.md` 描述 SideQuestion protocol v2，线程内存保存、有限队列与最近问答裁剪。原型模拟串行请求、最多 8 条待执行项、保留最近 16 轮、取消/隐藏/新线程，以及会话切换的迟到更新保护；真实提供方推理、64 KiB 参考片段裁剪、字节级历史容量等继续由宿主实现。

### Web 接入缺口

此次核对的 `rpi-web/src/protocol.rs` 未声明完整 SideQuestion 控制命令。TUI 的 `/btw` 已有实现，并不意味着在 Web prompt 中发送同样字符串就能获得相同语义。

需要额外版本化能力：旁问创建/提交/取消/查询/清理、owner 与线程 ID、旁问事件流、绑定模型信息、配额、历史策略和生命周期。通过同一节点认证连接，凭据由宿主管理，不能由插件参数或浏览器重建 provider 私有绑定。

旁问用量应单列，或在会话总览中明确合并范围，不能把它算成主轮输出速度或主轮缓存命中率。本原型没有真实旁问 usage 数据，回复为预设文本。

## 4. Skills：展示使用证据，而不是做成另一组 Tools

### 目录

侧栏 Skills 页面支持搜索、项目/全局/资源包来源筛选、本会话已载入筛选。示例覆盖：

1. 项目 `theme-design`：已模拟载入全文。
2. 全局 `responsive-review`：可发现、尚未载入。
3. 全局 `code-review`：仅手动指定。
4. 全局同名 `theme-design`：被项目版本覆盖。
5. 资源包 `release-notes`：可发现。
6. `broken-skill`：元数据解析失败，不可指定。

上述名称、路径和 SKILL.md 内容全部为本稿内置样例，不是扫描真实用户技能的结果。

### 状态语义

| 状态 | 可以证明什么 | 不应声称什么 |
| --- | --- | --- |
| 已发现 | 宿主解析出资源元信息 | 模型已读全文 |
| 可发现 | 摘要进入模型候选目录 | 本任务必定使用 |
| 仅手动指定 | 排除自动发现，仍可按支持的显式路径调用 | 完全卸载或内容不存在 |
| 已指定，待发送 | 用户把技能加入草稿/队列 | 已载入全文 |
| 全文已载入 | 宿主有全文注入/成功读取证据 | 全部建议已被采用 |
| 已引用/采用 | 存在明确引用或结构化执行证据 | 仅凭模型措辞就确认采用 |
| 被覆盖/解析失败 | 该来源没有成为可用技能 | 以同名文件悄悄替换实际来源 |

原型只展示可确定的载入记录，“是否采用”显示暂无结构化证据；没有编造准确使用率或 Skill 成本。

### 消息中使用

输入区可以为下一条消息指定技能，形成可移除的草稿 chip。发送时展示“已指定”引用，并追加模拟全文载入事件；排入队列时仍是待加载引用，直到模拟交付才创建载入记录。

自动匹配记录与用户显式指定记录分开描述。对话中的技能卡展示名称、来源、触发原因、证据、路径、关联资源和详情入口。关联 `references/*` 文件仅表示存在引用，不显示成已经读取。

技能详情包含概览、只读 SKILL.md 样例和本会话使用记录。会话顶部的开关仅控制后续可用目录，不能被显示成已经使用；修改可发现性也不能撤回已经进入上下文的指令。

### 本机实现依据

- `rpi-harness/src/skills.rs` 区分候选目录 `format_skills_for_system_prompt` 与显式全文注入 `format_skill_invocation`。
- `disable-model-invocation` 控制是否进入模型候选目录；不能当成权限沙箱。
- `rpi-cli/src/resource_dirs.rs` 有项目与全局同名资源优先级与诊断。
- `rpi-cli/src/transcript_view.rs::skill_tool_name` 已能识别 `read` 某个 SKILL.md，并渲染技能名称。这个证据只说明读取动作，不证明遵循程度。

没有独立激活事件时，读取推断需要明确标为“从 read 调用识别”，并检查成功结果和有效路径；不能从文件名猜测并发明“技能执行成功”。

## 5. todo、goal 和其他常见工具

todo 仍聚合到对话中的计划进度卡，附带折叠的真实工具调用记录。任务 ID 使用宿主返回值，不把数组序号当稳定 ID。当前示例中的操作是 `todo list`，不是创建外部任务系统。

goal 入口展示目标、轮次和时间预算的样例。原型不创建真实 goal，不开启自动推进，也不设置应用定时任务。若正式接入，状态、预算消耗和暂停/完成必须来自扩展事件。

MCP、子 Agent、文件/命令/网络工具继续使用现有内联调用卡，专用 ask/技能/todo 结果复用同一个调用 ID 以免重复计数。BTW 是旁路控制，不伪装成主 agent 的普通工具调用。

## 6. 框架增量

在既有 Dioxus 架构边界增加以下投影：

```text
QuestionRequest: session / request / toolCall / question / options / state
SideThread: owner / thread / fixed reference / request bindings / queue / history
SkillDescriptor: name / source / path / discoverability / diagnostics / shadowedBy
SkillSelection: draft message or queue entry / descriptor reference
SkillUseEvent: request or read call / method / evidence / content version / references
```

这些不是 rpi-web 现有 API 名称。技能目录、资源诊断、显式加载和结构化使用事件需要新增或验证接口。不要让客户端靠扫描任意全局路径模拟宿主资源发现，也不要让 Web/WASM 前端负责读取节点凭据。

`QuestionCard`、`SkillUseCard`、`TodoSummary` 都是 `ConversationView` 的内联块；`SkillCatalogue` 是工作空间设置层页面；`SideQuestionPanel` 有独立队列与取消作用域，生命周期绑定节点和主会话 owner。

## 7. 验证与范围

`tests/design/verify-capabilities.cjs` 检查单选/多选/自由回答、显式提交、取消/超时/其他设备回应、技能来源与诊断、草稿与载入分离、队列指定、goal/todo 展示、BTW 隔离/取消/隐藏/引用/会话切换、375–1440px 布局。

记录见 `artifacts/capabilities-validation.json`。没有调用真实插件、推理服务或上传文件；没有修改相邻仓库，也没有加载任何真实用户 SKILL.md 指令。
