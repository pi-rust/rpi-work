# 功能合理性审阅：pi-rust / rpi / rpi-web

日期：2026-10-24。审阅对象是本项目功能与接入方案，不是一次上游全仓安全审计。

本机基线：`pi-rust daaa3cc`，workspace `0.8.3`；`rpi-package 450587a`，rpi-web `0.1.7`。两个上游工作区在检查时均干净。文件摘要和完整提交 ID 见 [upstream-review-baseline.json](../../tests/fixtures/baselines/upstream-review-baseline.json)。本次没有修改上游仓库，没有编译 Rust 或连接真实 Provider。

## 结论

整体功能方向合理：**Dioxus 做客户端，复用 rpi-web 的会话/HTTP/SSE，rpi 继续负责 Agent loop、模型、工具、扩展和会话状态。** pi-rust 是底层仓库，rpi 是其中的宿主/CLI；不要把它们当成两个需要分别重写的 Agent 引擎。

单一对话展示文本/计划/工具、独立变更视图、顶部会话资源配置、应用级主题、PC 服务端与手机客户端的划分可以保留。真正需要收敛的是“宿主已有能力”和“Web 已能调用的能力”之间的差别。原型可操作不代表协议已经支持。

## P1：正式接入前必须处理

### 1. 权限确认当前不能覆盖所有命令工具

**发现**：`rpi-permission-gate` 的实际工具匹配只有 `bash / write / edit`，没有 `powershell`。宿主工具仓库已存在单独的 powershell 工具；原型首屏演示的恰好是 powershell 命令。安装 `rpi-permissions` 也只提供规则存储/查询，不自动形成执行拦截。

**依据**：`rpi-package/packages/rpi-permission-gate/src/lib.rs:238`；`pi-rust/crates/rpi-tools/src/tools/powershell.rs:36`；`rpi-package/packages/rpi-permissions/README.md`。此外 `rpi-web/src/host.rs:224` 的子宿主启动参数是 `--mode rpc`，不能仅凭已安装 gate 就假定它被显式启用；运行环境可能另有配置，本次未做启动验证。

**处理**：节点必须报告实际启用的执行拦截策略；Windows powershell 和其他高权限扩展工具需要纳入策略与测试。前端确认只是交互，拒绝必须由节点阻止执行。原型扩展介绍已拆分为 `permission-gate` 与 `permissions`，不再把规则存储描述成已启用审批。

### 2. 图片和附件不能直接按现在的选择器发送

**发现**：RPC `Prompt` 只有字符串 `message`；实际 `rpc_prompt_text` 调用 `lane.prompt_text(content, Vec::new())`。原生底层能处理图片，不代表当前 Web RPC 能传图片。`queue_message` 同样只构造文本用户消息。rpi-http 当前有 `/api/v1/projects/{id}/upload` 项目文件上传路由（20MiB），但原生RPC prompt仍只有字符串、图片并未通过该通道传给模型。当前客户端仅本地附件预览与显式文本引用，尚未接入文件上传。

**依据**：`pi-rust/crates/rpi-cli/src/rpc/protocol.rs:22`、`modes.rs:880`、`modes.rs:1138`；`rpi-package/packages/rpi-web/src/server.rs:143`。RPC 还有 1 MiB frame 上限，不能把界面允许的 10 MB 图片直接塞进 JSONL Base64。

**处理**：补节点上传/解析与受限资源 ID，再扩展多模态命令；在端到端能力确认前，图片/PDF 显示“本地预览”，不能显示“模型已读取”。路径引用、普通文件上传和图片输入分别建模，按模型能力决定是否可发送。

### 3. 扫码、只读设备与项目范围还没有服务端授权实现

**发现**：现有 security 是共享密码 + cookie 会话 + Host/Origin 校验，没有设备身份、逐项目 ACL 或只读客户端角色；路由也没有配对/设备授权 API。

**依据**：`rpi-package/packages/rpi-web/src/security.rs:26`、`security.rs:68`、`server.rs:143`。

**处理**：当前扫码保持交互演示。实际开放手机入口前，必须补邀请交换、管理端批准、设备凭据、每个命令/订阅/文件入口的项目和操作范围校验及撤销。不能把前端隐藏发送框当作只读隔离，也不能把现有共享密码登录包装成逐设备授权。

### 4. 规划、BTW、Skills 和提示词不能靠普通 prompt 自动接通

**发现**：规划与 BTW 扩展确实存在，但 Web allowlist 没有扩展命令调用、SideQuestion 控制、Skills/提示词目录管理等入口。不能直接推定把 `/plan start` 或 `/btw ...` 作为普通 prompt 文本就等于执行了扩展控制命令。模型调用 `plan_mode_start` 和用户切换规划状态也是不同的入口。

会话补充指令也不能直接套用 `customInstructions` 字段：当前 dispatcher 只把这个字段交给 compact，普通 prompt 未使用它。工具控制虽然有 `set_active_tools`，但技能加载和系统指令应用不是工具集合开关。

**依据**：`rpi-web/src/protocol.rs:4`；`pi-rust/crates/rpi-cli/src/modes.rs:524`、`modes.rs:880`；`rpi-btw/README.md`；`rpi-plan-mode/README.md`。

**处理**：补或验证相应桥接能力，返回真正生效的状态和资源来源；UI 展示 desired / applied / rejected。规划限制、设备授权和 active tools 需要协调，不能让普通工具开关悄悄改变规划语义。正式版仍由宿主/扩展拥有状态，客户端不另起一个旁路模型来假装复用 BTW。

## P2：设计与数据语义修正

### 5. 结算结果不止 completed / failed / interrupted

**发现**：本机 rpi 输出 `aborted`，还可能输出 `suspended`；压缩可能是 `compacted / declined`，恢复可能是 `skipped`。原型此前把 `aborted` 和其他结果都落成 failed。

**已修正**：保留原始 outcome 和 `pendingResume`；aborted 映射停止，suspended 单独显示挂起，compacted 映射压缩完成，declined/skipped 表示未执行，未知值显示待核对。停止仍等待 `agent_settled`，不以 abort 请求成功直接结算。

依据：`pi-rust/crates/rpi-cli/src/modes.rs:609`、`modes.rs:633`、`modes.rs:779`、`modes.rs:847`。验证覆盖这些原生结果值，而不只是测试模拟器自造的 interrupted。

### 6. follow-up 不是“等 agent_settled 后再发”

**发现**：宿主在 Agent loop 即将停止时 drain follow-up；它可以使当前运行继续。`nextTurn` 才是保留给下一次显式运行的通道。对结算事件后再发 follow-up 的假设可能让消息一直等待。

**已修正**：队列文案改为“当前回答收尾时由宿主继续处理”；UI 的 steering 在适配器中映射为 `deliverAs: "steer"`。暂停、重排、原子编辑仍列为缺失能力，不通过取消重发假装已支持。

依据：`pi-rust/crates/rpi-harness/src/agent_harness.rs:4683`、`agent_harness.rs:4695`；`modes.rs:1138`。`nextTurn` 已有 native 分支，但 Web 队列投影完整性仍需验证后再增加产品入口。

### 7. Package 应复用现有项目包基础，但不承诺现成的会话版本锁定

**发现**：上游现在已有 `rpi package link/list/update/unlink`、`.rpi/packages/<name>` 和项目 registry，约定包含 extensions 及可选 skills/prompts/themes 目录。之前只按全局 native extension 安装器理解包能力已不完整。

**限制**：registry 按 name 替换，资源目录也是按项目包名解析，尚不是原型演示的多版本不可变安装与每会话绑定。link 的构建/复制逻辑也不能自动推导成所有 sidecar 资源已经完整打包。普通 `.rpi/skills` 的优先级并非永远高于项目 package：当前链路是显式配置路径 → 项目 package 目录 → 常规项目目录，再结合全局目录与 first-wins 解析。

**处理**：优先适配上游 registry 和资源发现，新增会话 profile/revision、不可变资源快照和版本策略。现有设计 JSON 保持“设计资源包”类型，不宣称能交给 `rpi install`。原型技能目录已改为“实际来源优先级由宿主解析”。

依据：`pi-rust/crates/rpi-cli/src/packages.rs:1`、`packages.rs:175`、`packages.rs:606`；`resource_dirs.rs:344`。

### 8. 模型、thinking、模式必须归属于会话

**发现并修正**：原型此前用全局 state 存模型和模式；切换会话会把上一会话的选择带过去。现在按会话保存/恢复 model、thinking、mode，新建会话选择也不覆盖原会话。

正式版仍以 `get_state / set_model / get_available_thinking_levels` 的成功响应为准；模型级别不能硬编码。新增或修改 API Key 返回 `requiresRestart: true` 时，应明确显示等待重启，而不是继续使用旧绑定后宣称凭据已生效。依据：`modes.rs:894`、`modes.rs:1552`、`modes.rs:1564`。

## 可直接复用的范围

| 功能 | 当前基础 | 判断 |
| --- | --- | --- |
| 对话、工具状态与流式输出 | message/tool events、SSE、tool timing | 合理，做客户端投影 |
| 停止、恢复与压缩 | abort、resume_pending、compact + settlement | 可接，注意结果与恢复语义 |
| 当前上下文和统计 | get_state.context、get_stats | 已有基础；窗口/阈值取宿主，缺失指标不编造 |
| 模型与思考强度 | 模型发现、设置与动态 thinking levels | 可接，等待确认并回读 |
| 项目、会话、历史、fork、diff | 已有 HTTP 路由和 RPC | 首版重点 |
| Ask/确认 | ui_dialog_request/response/closed + pending dialogs | 可接，按 requestId 收敛两端结果 |
| Skills/提示词/Package | 原生资源发现与项目包已有，Web 管理缺失 | 复用原生、补桥接 |
| BTW / 规划入口 | 扩展已有，专用 Web 控制需验证/新增 | 暂不计入已打通 |
| 图片/PDF/设备配对 | 当前传输或授权链路缺失 | 原型保留，单独完成纵向接入 |
| Reddit 等界面主题 | 客户端变量系统 | 已实现，不依赖 Agent loop |

## 建议的接入顺序

1. 固定本机已核对的 rpi/rpi-web 版本与兼容性，完成一个真实会话：打开项目、prompt、流式工具卡、停止/恢复、快照重连、diff、Ask。
2. 在同一链路验证执行 gate，特别是 Windows powershell；模型设置、上下文、统计以宿主回读为准。
3. 增加 Skills/提示词/规划/BTW 的能力桥接，并在现有项目包之上实现会话 profile。不要先重造包安装器和 Agent loop。
4. 手机初期仍可作为同一用户的客户端使用已配置的受保护服务入口；带项目隔离/只读授权的扫码产品化，必须与设备授权 API 一起交付。随后补附件上传和多模态 RPC。

本次只修正本仓库原型与文档。`verify-runtime-semantics.cjs` 是浏览器端语义回归，不是对真实 RPC 服务的兼容认证；上游源码能力和运行时可用性仍需上述纵向 PoC 确认。
