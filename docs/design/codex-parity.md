# 与 openai/codex 的对齐范围

> 触发问题：能否"功能完整对齐" [`openai/codex`](https://github.com/openai/codex)。
> 结论：**不能按字面对齐，需要拆成三条线**；否则会去复刻一个 TUI 的键盘宏，而把真正的产品能力落在后面。

## 1. 那个仓库是什么

`openai/codex`（Rust，`codex-rs/` 下 60+ crate）是 **终端里的编码 agent**：

| 它的形态 | 说明 |
| --- | --- |
| `codex` CLI / TUI | 主产品，交互是**终端全屏 TUI + 斜杠命令** |
| `codex app` | 桌面应用体验（本仓库不含其界面源码） |
| `codex` IDE 扩展 | VS Code / Cursor / Windsurf |
| Codex Web | `chatgpt.com/codex`，云端 agent，**闭源** |
| `app-server*` | 供 app / IDE 复用的 JSON-RPC 后端协议 |

我们的产物是 **app 形态的工作台**（`apps/work-ui` 桌面/Android/Web，`design/reui-codex` Web 原型，`apps/work-gpui` 桌面原型），
自有 agent 运行时是 `rpi`（`work-node-runtime` + ACP/RPC）。所以：

- **界面形态** 对标 `codex app`（侧栏 + 会话 + 输入区 + 审批/工具面板），不是 TUI；
- **能力面** 对标 Codex 暴露给用户的能力（沙箱策略、审批、技能、MCP、模型/思考、恢复…），
  但**实现方是 `rpi`**，我们只做"暴露与呈现"；
- **协议层** 不复刻 `app-server`：`rpi-work` 已有 `NodeApi` + HTTP/WebSocket + 快照重放。

## 2. 能力矩阵

图例：✅ 已具备 · 🟡 部分 · ❌ 缺失 · 🚫 明确不做

| Codex 能力（来源） | `apps/work-ui`（产品） | `design/reui-codex`（Web 原型） | `apps/work-gpui`（GPUI 原型） |
| --- | --- | --- | --- |
| 会话列表 / 新建 / 重命名（README） | ✅ | ✅ | 🟡 静态列表 |
| 会话历史加载与分页（`resume`） | ✅ | ✅ 消息窗口 | ❌ |
| 流式输出（`streaming`） | ✅ SSE/WS | ✅ WS → Rust 投影 | ❌ |
| 工具调用展示（read/bash/edit…） | ✅ | ✅ 卡 + 代码着色 | 🟡 静态两卡 |
| **审批与沙箱策略**（`docs/sandbox.md`、`docs/execpolicy.md`） | 🟡 有确认弹窗与只读授权 | ❌ | ❌ |
| **技能 / AGENTS.md**（`docs/skills.md`、`docs/agents_md.md`） | ✅ 资源中心 + 会话配置 | ✅ 资源目录 + 安装闸门 | ❌ |
| 模型 / 思考等级切换（`docs/config.md`） | ✅ | ✅ 回读校验 | ❌ |
| 队列 follow-up | ✅ | ✅ | ❌ |
| 文件浏览 / Git diff / 暂存（`apply-patch`） | ✅ | ✅ 树 + 表格 + diff | ❌ |
| 图片 / 附件（`attachment-store`） | ✅ | 🟡 只显示消息内图片 | ❌ |
| MCP 服务器（`mcp*` crate） | ✅ 经 rpi-package | ❌ | ❌ |
| 多执行后端（ACP：Claude/Gemini…） | ✅ 执行后端切换 | ❌ | ❌ |
| 命令面板 / 搜索 | ✅ Ctrl+K | ✅ ⌘K | ❌ |
| 计划 / 任务清单（todo） | ✅ | ❌ | ❌ |
| 云任务（Codex Web / `cloud-tasks*`） | 🚫 | 🚫 | 🚫 |
| 终端 TUI 交互 | 🚫 | 🚫 | 🚫 |
| ChatGPT 账号登录 | 🚫（用 rpi 的 provider 体系） | 🚫 | 🚫 |
| 非交互 `codex exec`（`docs/exec.md`） | ✅ 由 `work-cli` 承担 | 🚫 | 🚫 |

## 2.1 消息区布局对照（本轮对齐）

| 元素 | Codex app | 我们的原型 |
| --- | --- | --- |
| 用户输入 | 右侧紧凑气泡，不占满阅读列 | ✅ `max-w-[85%]` 圆角气泡，右上角小圆角 + 「你」标签 |
| 助手正文 | 左对齐、无气泡、限宽阅读列 | ✅ `max-w-[46rem]`，标签 + 模型徽标 |
| 工具调用 | 折叠行，摘要 + 耗时 + 状态 | ✅ 可折叠卡片，目标/耗时/状态点 |
| 列宽 | 居中限宽（约 46–48rem） | ✅ 容器 `max-w-[48rem]` |
| 轮次间距 | 轮间明显大于轮内 | ✅ `gap-8`（轮内 `gap-2/gap-3`） |

## 3. 差距分级

**P0（不补齐就不算"能用"）**

1. 审批与沙箱策略的可见性：谁批准了什么、只读/可写边界、一次性授权（Codex 的 `execpolicy` 对应面）。
2. 计划 / 任务清单：长任务需要进度视图，否则只能靠翻对话。
3. 资源与技能：AGENTS.md / Skill 的生效范围（全局/项目/会话）在 UI 上要能核对。

**P1（日常效率）**

4. 附件（拖拽/粘贴文件）与图片输入。
5. MCP servers 的列表、健康与授权。
6. 多后端切换在原型侧的对照（Web 原型目前只支持 rpi 后端）。

**P2（体验）**

7. GPUI 原型的连接/发送/实时流（当前只有静态 fixture）。
8. 快捷键体系对齐（`docs/slash_commands.md` 的等价物 → ⌘K / /命令）。

## 4. 明确不做

- 复刻终端 TUI 与斜杠命令的输入模型（我们是 app 形态，输入区 + 面板已经覆盖）。
- 复刻 `app-server` JSON-RPC（`rpi-work` 的 NodeApi + WS 已覆盖同一职责）。
- Codex Web 的云任务（离线节点模型不适用）。

## 5. 下一批建议（按 P0 → P1）

| 批次 | 内容 | 数据来源 | 验收方式 |
| --- | --- | --- | --- |
| A | 审批/授权面板 | 会话事件（`ui_dialog_request`/`ui_dialog_closed`）+ `work-client-wasm::dialog_response` | 🟡 卡片已渲染并有选项；**回答回程未验证**（既没有 WS 帧也没有 HTTP `/dialogs` 请求），下一步要查 `dialog_response` 的发送条件 |
| B | 计划/任务清单面板（todo 投影） | `RunProjection` 的 todo 事件 | 原型 e2e：勾选状态持久 |
| C | 资源作用域核对（全局/项目/会话） | `native_catalog` + 会话配置 | 已有 `verify:settings` 扩展 |
| D | GPUI 连接 + 发送 + 实时流 | `work-client` 经 tokio 桥接 | 首帧 trace 后追加"发送→settled"断言 |

对齐工作的度量：上面矩阵里 **P0 全 ✅、P1 至少 🟡** 才算"功能对齐到可用"，而不是把 Codex 的每个子命令都搬一遍。
