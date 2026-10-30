# 真实原生扩展与实时 UI Demo

这是开发验证入口，使用**实际安装的 rpi、实际编译安装的 cdylib、真实工具执行与 RPC 事件、真实 Dioxus 组件**。只有模型响应是本地固定 fixture，不调用外部模型，不使用用户模型凭据。

## 已验证与未实现

本机实测：Windows x64，`rpi 0.11.0`，发布版 `rpi-plugin-sdk =0.8.3`（ABI 4）。其他平台有动态库文件名适配，但本轮未实机验证。

| 能力 | 结果与范围 |
| --- | --- |
| 原生扩展安装 | 真实 `rpi package link --locked`，写入隔离项目的 registry 与 DLL，并记录产物 SHA-256 |
| 原生扩展加载 | 新建真实 RPC 宿主，查询工具/命令目录并调用 DLL 注册的 `work-demo-status` |
| 工具执行 | 本地 provider fixture 请求 `work_demo_counter`，真实宿主驱动 execute/poll/destroy；连续两轮从 0 增至 8 |
| 实时进度 | 每轮有 3 个真实 `tool_execution_update`，随后真实 `tool_execution_end` 和运行结算 |
| 实时 UI | 开发适配器将该工具事件映射为受限 UI 数据，Dioxus 实时挂载/更新生产 `UiWidgets` 渲染器 |
| UI 与 Agent 独立 | 隐藏 UI 不取消执行，恢复显示后读取最新进度；并发运行请求拒绝 |
| 卸载 | 结束宿主后 `rpi package unlink`，再启动新宿主，确认工具已消失 |
| 正式产品扩展管理 | **尚未接入** `WorkspaceClient` 的通用安装/加载/会话 applied 协议 |
| 正式产品通用 live UI | **尚未接入** contribution 协商、任意扩展发现和正式插槽生命周期 |
| 远程 HTTP 节点 / 真实外部模型 | 本轮未验证；Demo 不是 rpi-http，不调用外部付费模型 |

不能把此 Demo 的成功描述成“正式应用已经支持任意插件安装及动态 UI”。它证明真实宿主能力和最小受控展示链路可行。

## 目录

- [`examples/extension-demo/`](../../examples/extension-demo/)：独立 Cargo workspace，真实 `cdylib` 与锁文件，不依赖邻接仓库。
- [`tools/dev/extension-demo.mjs`](../../tools/dev/extension-demo.mjs)：隔离安装、RPC 驱动、本地模型 fixture、loopback 开发适配器与清理。
- [`tests/harness/ui-modules/src/extension_demo.rs`](../../tests/harness/ui-modules/src/extension_demo.rs)：测试专用 Dioxus 页面。
- [`apps/work-ui/src/features/extensions/mod.rs`](../../apps/work-ui/src/features/extensions/mod.rs)：复用的宿主纯数据 widget 渲染器。
- [`tests/modules/verify-extension-demo.cjs`](../../tests/modules/verify-extension-demo.cjs)：实际宿主 + DLL + 浏览器回归，不是模拟工具事件的 fixture。

链路如下：

```text
本地固定 provider 响应（只替代模型生成）
    → 真实 rpi Agent loop
    → 真实 DLL work_demo_counter
    → tool_execution_update / end（RPC JSONL）
    → Demo adapter（校验工具名、调用 ID 和计数边界）
    → token 保护的 loopback SSE
    → 校验 ExtensionManifest
    → Dioxus UiWidgets 实时更新
```

## 环境准备

需要已安装 `rpi`、Rust/Cargo（含本机动态库编译工具链）和 Node.js。浏览器测试还需 WASM target 与本仓库 Playwright 依赖：

```powershell
rpi --version
rustup target add wasm32-unknown-unknown
npm ci --prefix tools
npm run install-browser --prefix tools
```

若 rpi 不在 PATH，设置 `RPI_WORK_SMOKE_BIN` 为明确的可执行文件路径。已有 Chrome 可通过 `CHROME_PATH` 指定。第一次构建可能需要从 crates.io 下载已锁定依赖；这不是模型 API 调用。

## 1. 一条命令验证原生安装与执行

在仓库根目录：

```powershell
node tools/dev/extension-demo.mjs --verify
```

这会自动安装、加载、执行两轮、核对 DLL 内部计数、关闭宿主、卸载，再启动新宿主确认扩展消失。成功输出 `PASS`，证据写到 `artifacts/extension-demo-validation.json`。

**安装的是项目包，不是全局扩展。** 每次使用独立的 `target/extension-demo-*/project/.rpi` 和 `target/extension-demo-*/agent`；不会写入用户的全局扩展目录、会话或认证配置。

## 2. 手动体验实时 UI

终端 A 构建并启动测试页面：

```powershell
pwsh -File tools/review/build-module-review.ps1 -Feature all
node tools/review/ui-review-server.mjs
```

终端 B 启动真实扩展 Demo：

```powershell
node tools/dev/extension-demo.mjs
```

等待输出 `DEMO_READY`，其中包含本次随机端口与 token 的 `endpoint`。保持终端 B 打开：

1. 浏览器打开 `http://127.0.0.1:4182/modules/all/`。
2. 点击顶部 `extension-demo`。
3. 将本次 `endpoint` 粘到 **Demo endpoint**，点击“连接真实 Demo”。
4. 确认显示原生安装、DLL 加载均为 `true`，初始计数为 0。
5. 点击“运行真实计数工具”，观察进度从 1 到 4；再运行一次增至 8。
6. 在运行中点击“隐藏实时 UI”：工具仍继续；点击“显示实时 UI”呈现最新值。

也可以在页面 URL 的 hash 中传入 `demo=<URL 编码后的 endpoint>`，页面读取后不持久化 token，仍需点击 `extension-demo` 和连接按钮。

结束时在终端 B 输入 `stop` 或按 Ctrl+C，等待进程正常退出：它会停止宿主、卸载项目包并核对工具已消失。然后关闭终端 A 的测试服务器。不要直接强杀进程；如果机器意外退出，残留只在对应的 `target/extension-demo-*` 中，确认宿主已退出后可删除该次目录。

## 3. 自动浏览器验证

构建模块后运行：

```powershell
pwsh -File tools/review/build-module-review.ps1 -Feature all
node tests/modules/verify-extension-demo.cjs
```

测试自行选择端口并启动/关闭测试服务器与 Demo runner，不需要占用现有 4182 服务。检查实际中间进度、UI 隐藏后继续执行、第二轮计数、重复运行拒绝、token/CORS、320px 布局，以及卸载后的新宿主。

另有负向回归，确认“工具完成但 Agent 后续失败”不能误报成功、缺失宿主程序不挂起、启动阶段收到 `stop` 也执行清理：

```powershell
node tests/modules/verify-extension-demo-failures.cjs
```

负向回归会覆盖通用原生证据文件；需要保留成功样本时，最后再运行 `node tools/dev/extension-demo.mjs --verify` 或浏览器验证。`--verify --fail-after-tool` 是刻意注入本地 provider 400 的诊断模式，**预期退出码为 1**，不是 Demo 不可用。

本地产物（不提交 Git）：

- `artifacts/extension-demo-validation.json`：宿主版本、DLL hash、真实工具事件、清理核对结果；
- `artifacts/extension-demo-browser-validation.json`：浏览器断言结果；
- `artifacts/extension-demo-negative-validation.json`：刻意失败与启动中断的回归结果；
- `artifacts/extension-demo-desktop.png`、`artifacts/extension-demo-mobile.png`：实际页面截图；
- `target/extension-demo-*/commands.log`、`host-stderr.log`：本次隔离宿主诊断。

## 安全与协议边界

- Demo 工具只修改宿主进程内计数，不读写业务文件、不访问网络、不启动子进程；原生 DLL 本身仍以当前用户权限运行，不是 OS 沙箱。
- `__rpi` 宿主上下文允许存在，模型参数不使用 `deny_unknown_fields` 拒绝注入字段；`steps` 限定为 1–10 的整数。Demo fixture 固定请求 4 步。
- SSE 适配器只消费已知工具、当前调用 ID、固定 Demo 标记及有界数值；UI 数据由适配器构造，不接受任意插件 HTML/JS/CSS 或 action。
- 开发接口只绑定 loopback，使用每次生成的随机 token，并拒绝非 loopback 浏览器 Origin。token 不是节点登录或产品授权机制，不应分享、持久化或用于生产部署。
- UI 页面只位于测试 harness。正式应用没有增加 Demo 端点、隐式连接或真实插件管理 capability。
- 模型 fixture 只运行在 loopback；源码构建可能使用 Cargo 网络，`--offline` 是 rpi 启动网络检查开关，不能理解为整个操作系统断网。

新增扩展与真实宿主验收须遵循 [开发规范](development-standards.md)，整体分层见 [扩展架构](../architecture/extensions.md)。
