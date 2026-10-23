# 流式消息与运行状态

这一版让主会话和 BTW 旁问都能逐段呈现回复。主会话按照「准备摘要 → 正文 → 工具 → 工具输出 → 正文 → 结算」排列，仍然只有「对话 / 变更」两个任务标签。

当前是浏览器内的事件模拟器：没有连接模型、SSE 或真实工具。正常发送就会流式显示；标题旁「流式演示」、输入 `/stream` 或「常用能力 → 流式消息」可启动内置场景。

## 1. 呈现与交互

| 对象 | 对话中的展示 | 更新方式 |
| --- | --- | --- |
| 请求已接收 | 同一条 Agent 消息显示准备状态 | 请求 accepted 只表示接收 |
| 准备摘要 | 默认折叠的简短状态说明 | 只展示允许公开的摘要，不展示内部思维链 |
| 正文 | 行内光标，内容逐段出现 | 只追加当前文本块，不重建整条历史 |
| 工具 | 原位置的可折叠工具卡、参数和状态 | 通过稳定的 toolCallId 更新 |
| 工具输出 | 卡片内逐步增加日志 | 区分快照覆盖与增量追加 |
| 最终回复 | 继续追加到这条消息的后续文本块 | 与前面的工具保持顺序 |
| 本轮结算 | 完成、失败或停止结果 | 以 `agent_settled.outcome` 为准 |
| 连接 | 输入区上方显示连接、重连、恢复状态 | 与任务执行状态分别保存 |

用户停留在底部时跟随新内容；向上阅读后保持滚动位置，用「新进展 ↓」返回底部。生成时仍能编辑下一条草稿、放入消息队列、切换到变更页或旁问。原型保留此前的会话切换限制：主任务运行中需先结束本轮，正式客户端应支持节点后台运行后切换会话。

BTW 使用自己的片段更新、取消标记与队列。取消保留部分回答；隐藏面板继续生成；明确引用后才进入主草稿。主流式消息不会逐片段重建 BTW 面板或输入框。

## 2. 状态机与异常

任务状态：`accepted → preparing → text ↔ tool → awaiting_settlement → completed / failed / interrupted`。停止经过 `cancelling`，在宿主确认前不能宣称已经停下。

连接状态另存为 `online / reconnecting / resyncing`。断线不是执行失败，也不是停止；已经生成的正文和输出继续保留。

- **结束与完成**：`message_end` 结束一个消息；`agent_end` 不能直接当作本轮成功。等待 `agent_settled` 并检查 outcome。HTTP/SSE 连接关闭也不是完成依据。
- **停止**：先发取消请求、显示等待确认；收到 interrupted 后停止光标，保留部分内容。真实实现若停止命令失败，应显示失败并继续同步真实运行状态，不能本地伪造停止成功。
- **重连**：携带确认过的事件游标，补收断线期间事件。原型故意重放最后一个事件验证去重，不重发用户 prompt 或工具请求。
- **恢复失效**：收到 `resync_required`、事件缺口或宿主重启时重新取快照，并重建当前投影，避免把快照拼到旧内容后面。原型的「模拟恢复失效」从本地事件源重建模拟快照；没有测试真实 HTTP 快照和服务重启。
- **执行失败**：保留部分正文和工具输出，标记未完成工具失败。重试需用户明确发起新一轮，不能自动重复有副作用的操作。
- **清理**：停止、失败、重置场景后清除定时器；回调按 runId/会话隔离，旧运行不能更新新运行。

原型的「流式演示控制」提供断线、立即重连、恢复失效、运行失败和暂缓最终结算。勾选「暂缓最终结算」可看到 `agent_end` 后仍未完成，再手动接收结算。这组故障按钮属于设计验证入口，正式产品放到诊断模式。

## 3. 对接现有 rpi-web

本轮查阅了相邻仓库 `rpi-package/packages/rpi-web/frontend/src/App.tsx`、`transcript.ts` 和服务端事件处理，沿用已经确定的 HTTP/SSE 边界。

| 现有接口或事件 | 客户端职责 |
| --- | --- |
| `POST /api/v1/sessions/{id}/commands` | 提交指令，accepted 不代表运行结束 |
| `GET /api/v1/sessions/{id}/events?after=N` | 持续接收；重连按 `after` / Last-Event-ID 恢复 |
| `message_start / message_update / message_end` | 投影消息与内容块；检查 message 的完整内容，不能一律当作文本 delta |
| `tool_execution_start` | 按 toolCallId 创建或更新卡片 |
| `tool_execution_update.partialResult` | 现有前端将它作为当前结果更新；适配时不能盲目追加完整快照 |
| `tool_execution_end.result` | 写入最终结果与错误状态，替换暂存的部分结果 |
| `agent_settled` | 以 outcome 确定本轮终态，再协调统计和队列状态 |
| `resync_required` / 事件历史淘汰 | 重新读取 snapshot 和历史，并按工具 ID / 消息身份消除重叠 |

`prototype/streaming.js` 的 `phase / text_delta / tool_update` 等是**原型内部归一化事件**，不是新增的 rpi RPC 协议。真实适配器应区分累计快照、文本差量、最终替换，并保留工具与正文的原始顺序。生成中的工具参数可以先显示安全文本；完整 JSON 就绪后再结构化呈现，不能执行或解析不完整参数。

游标去重需要绑定节点、会话和当前宿主代际；宿主重启后 seq 可能重新计数，不能永久丢弃所有较小 seq。客户端维护的 generation 是隔离旧连接的本地标记，不假设现有协议已经提供 hostEpoch。快照与回放仍需处理快照读取期间产生的事件及历史重叠，复用现有 transcript 去重逻辑。

## 4. Dioxus 框架边界

延续 `work-client / work-core / work-ui` 的既定分层：

- `work-client`：接收原始 SSE、重连与游标管理、快照恢复；不自动重发业务指令。
- `work-core`：纯 reducer，把归一化事件投影到 `RunProjection`；维护运行状态、连接状态、稳定消息/块/工具 ID、可空用量与代际。
- `work-ui`：`ConversationTimeline / AssistantMessage / TextBlock / ToolCallCard / RunStatusBar`；用稳定 key 更新单块，保留输入状态与滚动锚点。

建议的领域类型是 `RunProjection { run_id, phase, transport, cursor, blocks, usage }` 和 `MessageBlock::{PublicSummary, Text, ToolCall, Ask, Plan, Attachment}`。这是下一阶段 Rust 类型设计提案，尚未创建 crate。HTML 原型只实现其中的摘要、文本、工具流式块；已有 Ask、Plan、Skills 卡继续使用此前的示例机制。

正式接入可按一帧或约 30–60ms 合并细小更新，以实际测量调整；避免逐 token 重建整段 Markdown 和全部历史。未闭合 Markdown 先保留安全文本，代码块逐步补齐，段落稳定后再解析并增量替换；模型 HTML 不直接执行。当前原型使用转义文本，不实现 Markdown 渲染、代码高亮、图片生成进度或大日志虚拟化。

图片/附件引用归属于消息与块 ID。上传、解析、生成附件分别显示进度，和文本 token 进度分开；得到有效资源 ID 后才形成预览卡，不能用未完成的 URL/Base64 直接渲染图片。这部分仍为后续设计实现项。

## 5. 统计口径

流式状态栏显示连接状态、已接收片段数和事件 seq，片段数不是 token。正文字符长度也不当作模型 token。

本轮缺少 Provider 用量时显示「—」，页脚明确保留上一轮已结算样例。不得因为收到结算就虚构 token、命中率、TPS 或费用；后续需要以真实上报建立 `provisional / settled / unavailable` 状态，标明数据来源。最终值可能在重连后校准，以替换或按请求 ID 合并的方式更新，避免累计重复计算；缓存读取、缓存写入和推理输出延续既有口径。

## 6. 验证

`node scripts/verify-streaming.cjs` 覆盖逐段正文、累计工具快照与日志追加、主草稿 DOM 保留、断线回放去重、快照重建、等待结算、停止确认、部分结果保留、失败、滚动保持、BTW 独立流式取消、重置清理、375–1440px 布局及无外部请求。结果记录于 `artifacts/streaming-validation.json`。

此前四组浏览器回归仍需通过。真实 SSE、网络鉴权、服务重启、多设备同时操作、Dioxus/WASM 性能和手机后台恢复仍需在下一阶段验收。
