# ACP 执行后端与会话绑定

**快捷入口：会话顶部的「Agent · 当前后端」。** 点击后可查看节点公布的 RPI / Claude / Codex / OpenCode 等执行后端，并选择切换。桌面、浏览器、移动客户端使用同一组件；极窄屏时顶部动作换行，确保入口和现有动作不被截断。**不放在输入框或输入工具栏。**

原有「会话配置 → 执行后端」Tab 保留，用于查看当前绑定、选择后端及调整 Agent 实际提供的协商配置。两个入口共用同一份节点后端目录与创建回调。

已有会话切换到其他后端仍需二次确认；取消或 Escape 保留当前会话与草稿。确认后关闭配置窗口，在同一项目另建会话，不原地改绑。新会话打开后，顶部及配置页读取该会话自己的绑定。

桌面本机已接入 RPI、Claude、Codex 与 OpenCode。执行后端与模型是两个维度：模型、推理强度和模式只使用当前 Agent 通过 ACP `configOptions` 实际公布的选项，不生成虚构的模型目录。

## 会话切换规则

- 尚未选中会话时，顶部 Agent 入口仍显示。选择后端只设置后续新会话的创建偏好，**不会立即创建会话**；再点击项目旁的 `+` 使用该后端创建会话。会话资源配置入口仍要求已选中会话。
- 已有会话选择不同后端，会先确认，再在**同一项目创建独立会话**。不会改绑原会话、迁移上下文、复制权限或停止原任务；原会话和草稿保留。
- 打开已有会话，以它持久保存的绑定为准，而非全局新会话偏好。绑定尚未确认时不拿偏好冒充当前后端；离线、未知绑定或创建中时禁用切换动作。会话/连接 scope 改变会销毁旧切换窗口和待确认操作。工作台会话 ID 与 Agent 返回的 opaque session ID 分开保存。
- ACP 进程由工作空间持有。切走页面不会取消正在执行的提示；停止需要显式操作。删除会话会清理本地绑定及其托管进程，不等同于删除 Agent 自己保存的原生历史。
- ACP 在**节点宿主**运行，不在 Web 或 Android 客户端设备上启动。桌面本机通过进程内 facade 使用同一 owner；`work-node-service` 已提供后端发现、会话绑定、能力、配置、创建与交互答复入口，远程访问取决于目标节点实际公布的能力，不能把所有 HTTP 节点都视为支持 ACP。缺少接口或能力时明确失败，不退回客户端直启 Agent。

「可用」仅代表可定位启动程序和参数有效，**不代表已登录、模型可访问或提示一定完成**。恢复依赖 Agent 真正支持 load/resume；能力缺失或恢复失败会明确报错，不另造新会话冒充原上下文。ACP 会话暂不支持工作台 Fork/Clone，也不承诺与 RPI 相同的资源安装、队列、压缩、原生 BTW 工具限制或多模态能力。

## 安装与启动配置

沿用节点宿主上的各 CLI 登录与 provider 配置，不在客户端输入或保存它们的访问密钥。以下版本来自此前本机验证，不代表当前安装状态或推荐最新版：

| 后端 | 启动入口 | 本机测试版本 |
| --- | --- | --- |
| Claude | `@agentclientprotocol/claude-agent-acp` | adapter 0.99.1 / Claude Code 2.1.282 |
| Codex | `@agentclientprotocol/codex-acp` | adapter 2.5.0 / Codex CLI 0.154.0 |
| OpenCode | 原生 `opencode acp` | OpenCode 1.4.3 |

默认查找 PATH 上的 `claude-agent-acp`、`codex-acp`、`opencode`，OpenCode 默认参数为 `["acp"]`。不会自动安装依赖、运行 `npx` 或拼接 shell 命令。可以在启动桌面应用**之前**配置环境变量：

| 后端 | 程序变量 | 参数变量（JSON 字符串数组） |
| --- | --- | --- |
| Claude | `RPI_WORK_CLAUDE_ACP_BIN` | `RPI_WORK_CLAUDE_ACP_ARGS` |
| Codex | `RPI_WORK_CODEX_ACP_BIN` | `RPI_WORK_CODEX_ACP_ARGS` |
| OpenCode | `RPI_WORK_OPENCODE_ACP_BIN` | `RPI_WORK_OPENCODE_ACP_ARGS` |

程序须为绝对路径，或可在节点宿主的绝对 PATH 目录中解析的单个名称。Windows 仅直接启动 `.exe`，不把 npm 的 `.cmd`、`.ps1` 或无扩展名 shell wrapper 当成本机可执行文件。Node 分发可明确使用 `node.exe` 加入口文件；例如在仓库根目录的 PowerShell 中：

```powershell
# 可选：仅安装到本仓库的临时工具目录，不全局安装。
npm install --prefix target/acp-tools --no-save --package-lock=false `
  @agentclientprotocol/claude-agent-acp@0.99.1 `
  @agentclientprotocol/codex-acp@2.5.0

$node = (Get-Command node.exe -ErrorAction Stop).Source
$adapters = (Resolve-Path target/acp-tools/node_modules/@agentclientprotocol).Path
$env:RPI_WORK_CLAUDE_ACP_BIN = $node
$env:RPI_WORK_CLAUDE_ACP_ARGS = ConvertTo-Json -InputObject @("$adapters/claude-agent-acp/dist/index.js") -Compress
$env:RPI_WORK_CODEX_ACP_BIN = $node
$env:RPI_WORK_CODEX_ACP_ARGS = ConvertTo-Json -InputObject @("$adapters/codex-acp/dist/index.js") -Compress
# 然后在同一个环境中启动桌面应用；已有进程不会自动继承这些变量。
```

`target/acp-tools` 是可删除的本地测试依赖，不是发布包内置 adapter。正式使用应将程序放在稳定位置并更新路径。参数变量无效、程序缺失或当前节点不能启动 ACP 时，菜单禁用对应选项并显示原因。

## 持久化、权限与运行状态

- 节点 sidecar：`<data>/rpc/agent-sessions.json`，schemaVersion 1，保存绑定、摘要和真实消息事件；不创建假 RPI 占位会话，不保存启动命令或访问凭据。**消息正文仍是本地明文历史**，应按聊天记录保护数据目录。
- 文本增量在内存中完整回放，部分输出最多每 250ms 做一次快照；权限、工具及回合结束立即落盘。异常断电可能丢失尚未检查点化的最后一小段文本，不承诺每个 token 的断电持久性。
- 重启后将已检查点化的中断输出保留为历史，标记失败/中断，而非成功或仍在后台运行；重复启动不会重复追加同一条结束消息。
- load/resume 的历史通知在协议响应的明确顺序边界前不重复入库；恢复期间的权限请求取消，不自动批准，也不因等待恢复响应而互相阻塞。响应之后的真实通知正常保留。
- 权限卡按工作台会话和进程代际命名空间隔离，原始 JSON-RPC 数字与字符串 ID 严格区分。旧会话、旧进程、重复或跨会话答复被拒绝；仍向原 Agent 发送其原始 option ID。
- 取消使用 ACP notification。提示有独立的 10 分钟截止，不与普通控制请求的 30 秒截止混用；超时或消费失败清理进程并报告终止，不把请求接受当成完成。
- ACP Agent 是具有当前用户权限的本机 CLI，**不是安全沙箱**。权限交互能力取决于 Agent；切换 configOptions 中的模式可能改变其权限策略，应阅读 Agent 自己的说明。工作台不会自动选中免权限模式，也不会为连接测试批准工具。

## 真实连接结果与验收范围

此前本机验证向每种后端各发过一次极短、禁止工具的 `ACP_OK` 提示；以下是那次运行的记录，不是当前环境的重新验收结果：

| 后端 | initialize / session-new | 真实提示结果 |
| --- | --- | --- |
| Claude | 成功，返回真实配置 | 成功，`end_turn`，回复 `ACP_OK` |
| Codex | 成功，返回真实配置 | 60 秒 smoke 截止内未完成，**尚未调通完整回复** |
| OpenCode | 成功，返回真实配置 | 60 秒 smoke 截止内未完成，**尚未调通完整回复** |

Codex 本机登录检查成功；OpenCode 未发现已保存的 provider 凭据，但握手和默认模型目录可以返回。两次超时的具体原因尚未确定，不能仅凭握手判断模型可用，也不能将无保存凭据认定为超时的唯一原因。原始本机证据见 `artifacts/acp-live-handshake.json`、`artifacts/acp-live-prompt.json`；这些是本地验收产物，不是内置演示响应。

可复验的层次：

```powershell
# 节点所有权、原生执行器及客户端适配的隔离测试，不调用真实模型。
cargo test -p work-node-runtime -p work-client -p work-node-service --locked

# 真实 Dioxus 菜单/configOptions 组件，内存 transport，不调用模型。
./tests/support/build-agent-backends.ps1
node tests/e2e/verify-agent-backends.cjs

# 默认只握手/建会话；--prompt 才会发模型请求，可能计费，请显式选择。
node tools/review/smoke-acp-agents.mjs --adapters target/acp-tools
# 单后端可加 --backend claude；此处不默认运行 --prompt。
```

此前浏览器证据：`artifacts/agent-backends-validation.json`、`artifacts/agent-backends-320.png`、`artifacts/agent-backends-390.png`，以及 `artifacts/agent-backends-resource-tab-desktop.png`、`artifacts/agent-backends-resource-tab-390.png`、`artifacts/agent-backends-resource-tab-320.png`。这些本机产物只说明各次测试的范围，不构成当前功能或远程能力的完整清单。当前会话能力和配置边界以 `SessionAccess` / `SessionConfiguration` 及节点的真实响应为准；HTTP 隔离测试见 `crates/transport/work-node-service/tests/session_access_acp.rs`、`session_configuration.rs` 和 `session_services.rs`。fixture 通过不等于真实桌面 WebView、Android 或模型端到端验收。

完整回归入口与此前消息/主题行为见 [项目、会话与消息交互](codex-interactions.md)、[功能面审阅](../archive/codex-visual-review.md)。源码、隔离验证与发布状态分别判断；已安装应用不会自动获得本地源码改动。
