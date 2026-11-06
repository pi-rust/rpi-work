# 节点扩展迁移执行记录

## 已确认的方向

- 用户要求继续完成目标：能力尽量沉淀到 rpi 扩展；Work UI 只组合面板与客户端。
- 使用已有 rpi 插件 ABI，像 rpi-web 一样加载原生 DLL 并注册入口。
- 不新增节点服务 C ABI。此前讨论的独立 C ABI 方案已撤回。
- HTTP 关闭后本机仍应能访问同一个 owner；不得另建一套会话注册表冒充共享。

## 当前源码位置

- Work：`D:/Projects/rpi-work`，保留用户原有未提交修改。
- 共享包：`D:/Projects/rpi-package-work-components`，分支 `codex/work-components`。
- rpi 启动修正：`D:/Projects/pi-rust-http-bootstrap`，分支 `codex/http-service-bootstrap`。
- Work 消费 `vendor` 生成快照；同步脚本是 `tools/dev/sync-component-packages.py`。

## 已迁出的包

| 包 | 职责 |
| --- | --- |
| rpi-components | UI 包、目录、授权、隔离面板通信桥 |
| rpi-http-component | 远程访问专用 UI 和动作映射 |
| rpi-node-api | 节点 DTO、能力描述和 NodeApi 端口；没有 UI 投影 |
| rpi-node-core | 原 work-node 的实际 owner、资源服务和原生/ACP 执行适配 |
| rpi-http-runtime | 认证、Host/Origin、共享撤权、二维码授权 |
| rpi-http-node | 原 work-server 的 HTTP/SSE/WebSocket 适配及生命周期 |
| rpi-node-host | 数据锁、owner 启动与停止、配置和内置组件绑定 |
| rpi-node-launcher | 父端进程监督、原生包校验下载、下次启动选择 |
| rpi-http-extension | 真实 rpi 插件 ABI 入口 `--http` 和独立 rpi-node-service 程序 |

Work 的 work-node、work-server 已是 re-export 兼容壳。work-host 默认 GUI 启动
已切换为真实 rpi 扩展子进程 + 私有 WorkClient；旧宿主源文件已核对后迁走。
work-cli serve / work-ui --headless 仍直接组合共享 node-host 的服务入口。
Work 的消息投影仍留在 work-core；WireEvent 的投影
方法通过本地 `WireEventProjection` trait 提供，节点 DTO 为共享包中的同一类型。

## 已验证

- 真实已安装 `rpi 0.20.0` 加载 `rpi_http_extension.dll`，通过旧有 ABI 启动 HTTP、
  认证、组件目录和无凭据状态读取；stdin EOF 后正常清理，模型调用数为零。
- 原生验证脚本 `tests/e2e/verify-native-http-extension.cjs` 支持新 rpi 无模型
  配置的启动（RPI_NATIVE_MODEL_FREE=1）；旧 rpi 的 ABI 兼容检查才使用隔离
  合成配置。生产代码不注入合成模型密钥。
- 原 rpi 启动在插件接管 stdio 前强制解析模型密钥，HTTP 服务本身不需要模型。
  最小修正是通用声明 `ownsStdio:true, requiresAgent:false`：跳过无关的模型和
  AgentHarness 创建，仍使用已有生命周期和 stdio owner 约束；不改 ABI 结构。
  修正在独立 pi-rust worktree，构建通过；真实 DLL 在没有模型密钥、没有
  合成模型配置的情况下启动/认证/退出验证通过，未替换用户安装的 rpi。
- `rpi-http-node::HttpServer::start_control` 已实现只绑定 loopback 的私有控制
  入口，同一个 owner；私有能力只通过父子 stdin 管道传递，不在环境、argv、
  ready、日志或 Agent 子进程里。`--node-control-stdin` 仅声明管道交接方式；
  不通过环境标记传递，避免 Agent 子进程继承后误进入控制启动模式。
  公开 HTTP 不接受控制头。本机能力有独立认证生命周期，公开共享 off/on
  不会失去本机控制能力；公开的旧 cookie 保持撤权。
- WorkClient 的 native-only `local_control` 已支持私有 HTTP/WS，禁止代理和
  重定向、拒绝非 loopback URL，不保存能力凭据；同 owner 的真实 HTTP/WS
  集成验证通过。GUI 默认宿主切换已完成；配对兼容层按声明的共享能力调用，
  不按 rpi-http 组件 ID 查询。
- 本轮回归通过：registry 10、HTTP 安全 14、HTTP node 13、node host 32、
  component/QR/shared-owner 12，以及私有 WorkClient HTTP/WS 集成；WASM
  work-ui 检查通过（3 个既有 dead_code 警告）。
- rpi-node-launcher 已实现：受监督的 rpi 进程树；Bundle 原生平台描述；无节点
  凭据的 HTTPS 下载、有界 HTTPS 重定向、大小/摘要校验、内容地址暂存、下次启动
  selection。Work UI 通用安装器提供独立原生代码信任操作，明确下次打开启用。
  下载的真实 DLL 经测试 TLS 下载暂存后，已通过真实 rpi ABI 启动/私有 API/退出。
- 原生与 WorkClient 端到端监督测试、正式 WASM 浏览器安装/挂载/卸载、独立面板
  桌面/手机布局都通过；组件和 launcher 严格 Clippy 通过。节点核心 88 项通过。
- CI 增加内置 DLL/so/dylib 和可下载组件汇总；六平台元数据/摘要与 Rust 合约
  一致性通过 fixture 验证。尚未执行发布，也未声称跨平台二进制运行都已验证。
- 共享 HTTP 安全单元测试和 Work 同 owner/二维码/撤权集成回归已通过。
- Work core 契约/投影回归已通过。一次宽范围 Work client 回归遇到既有 HTTP
  分页/访问/配置 fixture 的 WebSocket fallback 失败，已确认夹具只有 HTTP，
  已明确 pin HttpSseOnly，相关回归通过；正式传输错误分类未放宽。

## 配置与运行时选择

专用面板已支持监听 IP、端口、分享地址和新节点密码；由扩展校验并原子保存到
节点目录的 `http-component.json`。密码只保存摘要，不返回明文。保存设置不重启
owner 或停止任务，下次节点启动生效。显式启动参数优先于保存配置，再使用环境
默认值。公开监听失败时，私有入口仍可修复配置，保留同一节点身份和密码摘要。

通用组件页分别显示当前运行时来源、下次选择和待重启状态。恢复默认仅移除选择，
不删除正在加载的 DLL、不停止当前节点。显式环境覆盖时不允许安装产生误导的
下次启动承诺。浏览器及远程节点客户端不提供本机原生安装。

发布汇总脚本已有六平台 fixture 回归，真实下载与加载在 Windows 验证。发布脚本
在任何 GitHub release 修改前校验组件摘要及原生文件。尚未发布或替换安装的 rpi。

## 构建空间和最新二进制

D 盘在宽范围构建时已满。批量递归缓存删除被自动审核拒绝；没有重复绕过。
已用 Cargo 自带 `cargo clean -p rpi-cli --profile dev` 清理本次独立 worktree 的
可再生包产物，释放约 1.3GB；新的重型验证在 C 盘临时 target 下关闭 debug/incremental。

- 当前测试用新 rpi：`C:/Users/bigfish/AppData/Local/Temp/rpi-core-service-validation-target/debug/rpi.exe`
  （旧 D:/Projects/pi-rust-http-bootstrap/target/debug/rpi.exe 已被 cargo clean 移除）。
- 核心测试 target：`C:/Users/bigfish/AppData/Local/Temp/rpi-components-validation-target`。
- 最终原生 UI、CLI 和 DLL：
  `C:/Users/bigfish/AppData/Local/Temp/rpi-work-validation-target/debug/`。
  原生 UI 完整链接通过；D 盘默认 target 的 UI 不是本轮验证构建。
- WASM：`D:/Projects/rpi-work/target/component-review/parity-web`；已有正式浏览器验证。
- 最新新 rpi 构建成功，registry 10 项通过；Work core 47 项通过。

不要用为父 rpi 注入假模型密钥的方式交付生产启动；那只是第一轮 ABI 加载验证。
不要静默保留 Work UI 内的第二个 owner 作为失败 fallback。

## 最终目标审计（2026-11-07）

| 要求 | 落点 | 最终验证 |
| --- | --- | --- |
| 真实原生扩展、复用现有 ABI | rpi-http-extension；独立 rpi 通用纯服务启动补丁 | 无模型密钥的实际 DLL 加载、认证及 EOF 退出通过 |
| 节点能力沉淀、单一 owner | rpi-node-core / node-host / http-node；Work 兼容 re-export | 核心 88 项、共享 owner、会话、SSE/WS、二维码与撤权集成通过 |
| Work UI 通用组合 | 通用组件页、能力发现、WorkspaceClient | 完整原生 UI 构建、正式 WASM 构建和真实 owner 浏览器流程通过 |
| 精心设计且按需加载的扩展面板 | rpi-http-component；隔离 MessageChannel 桥 | 桌面/手机、懒加载、动作桥、sandbox、配置保存浏览器验证通过 |
| 可下载原生运行时 | rpi-node-launcher Store；独立原生代码授权 | 测试 HTTPS 下载实际 DLL，再通过原有 rpi ABI 启动并退出通过 |
| 可视化配置、下次启动生效 | 专用面板及 http-component.json | 真实 ABI 保存、重启、密码认证、公开监听失败后的本机修复通过 |
| 当前与下次运行时选择 | work-host runtime；通用组件状态区 | 显式路径和下载缓存两条监督启动均通过；恢复默认不终止当前节点 |
| 分发可检查 | CI、打包、组件汇总、上传前校验 | 六平台 fixture 与 Rust Bundle 精确一致；Python/PowerShell 语法通过 |

最终再次执行：`verify-native-http-extension`、`verify-http-configuration`、
`verify-supervised-host`（显式/缓存两种）、真实 HTTPS 下载 ABI 单测、
`verify-component-panels`、`verify-components-workspace` 和分发合约 fixture，全部通过。
生成快照 101 文件一致。原生及 WASM 保留 3 个既有 dead_code 警告；共享工作区
全量 fmt 仅报告既有 rpi-voice 文件格式，未改动无关代码；本次包及 Work 新代码单独检查。

源码和发布流程已完成；没有提交、推送、发布或替换已安装的 rpi，也没有重启现有节点。
Windows 原生和浏览器实际运行已验证，其他平台只验证分发描述与构建定义。
