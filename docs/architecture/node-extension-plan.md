# 节点扩展迁移执行记录

> 本文是历史执行记录。当前有效的物理边界以
> [节点、组件与界面组合](component-composition.md) 为准：
> rpi 能力在 rpi/extcore，HTTP 适配在 rpi-http；Work 节点管理、组件包和专用
> 面板在 Work。Work 不再把 `rpi-http-extension` 当作自己的节点宿主运行时。

## 2026-11-07 边界修正（当前有效）

后续审核确认：Work 管理节点和宿主，HTTP 扩展只把 rpi 能力服务化。

- 删除 Work UI 中“节点运行时”下载、选择、恢复默认和原生信任入口；组件页只
  保留通用 UI 包的导入/下载/授权/安装/卸载和按需面板挂载。
- 删除 `task desktop:runtime`、CI 强制 `-p rpi-http-extension`、桌面归档中的
  原生扩展文件，以及发布流程对 HTTP 组件制品的强制检查。
- 专用 HTTP 设置面板仍由 Work 内部组件包提供，源码从
  `vendor/rpi-http-component` 迁至 `crates/work-http-component`；通用桥从
  `vendor/rpi-components` 迁至 `crates/work-component-api`。
- Work 宿主回到本进程已有 `LocalWorkspace` 同一 owner，`work-server` HTTP
  只接在它上面。Work 的多项目 owner 不是 `rpi-http` 的实现，也不声称 rpi 的
  新 HTTP 插件已支持 Work 现有远程工作空间协议。
- 节点 API、运行时、宿主、Work HTTP 适配及组件协议都已迁回本地私有 crates，
  全部 `publish = false`；旧 vendor 快照、同步生成器和节点 DLL 监督器已移除。
- 本轮全工作区测试 807 通过、0 失败、5 ignored；原生 UI/CLI、正式 WASM、
  组件安装/挂载/卸载和响应式面板验证通过。隔离宿主测试明确把旧 DLL 路径设为
  不存在，仍正常启动；分享关闭拒绝远程访问，本机继续访问同一 owner。
- 原夹带节点逻辑的扩展 PR #226 已关闭；纯 HTTP 入口改为独立 PR #230。
  Work 仅本地合并，不发布、不推送主分支。宿主旧补丁已通过获授权的撤回 PR #279
  从 rpi 主仓库移除，没有新增宿主功能。

下文记录的是修正前的迁移过程与验证，不再代表当前 Work 的宿主启动方式或
组件分发方式。

## 已确认的方向

- 用户要求继续完成目标：能力尽量沉淀到 rpi 扩展；Work UI 只组合面板与客户端。
- 使用已有 rpi 插件 ABI，像 rpi-web 一样加载原生 DLL 并注册入口。
- 不新增节点服务 C ABI。此前讨论的独立 C ABI 方案已撤回。
- HTTP 关闭后本机仍应能访问同一个 owner；不得另建一套会话注册表冒充共享。

## 当前源码位置

- Work：`D:/Projects/rpi-work`。
- 当前组件源码在 Work 仓库：`crates/work-component-api`（通用桥与包契约）和
  `crates/work-http-component`（远程访问专用面板）。
- rpi 能力与 HTTP 适配属于 rpi 仓库：`rpi/extcore` 执行能力，`rpi-http` 提供
  HTTP 适配。
- `vendor/` 历史快照已移除，代码由 Work 本地私有包直接维护。

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

> 上述 work-host 监督扩展的启动方式已被 2026-11-07 边界修正取代：Work 宿主
> 回到进程内同一 `LocalWorkspace` owner，不再把该 DLL 当作宿主运行时。

## 已验证

- 真实已安装 `rpi 0.20.0` 加载 `rpi_http_extension.dll`，通过旧有 ABI 启动 HTTP、
  认证、组件目录和无凭据状态读取；stdin EOF 后正常清理，模型调用数为零。
- 历史原生验证脚本 `tests/e2e/verify-native-http-extension.cjs`（已随宿主耦合删除）
  支持新 rpi 无模型配置的启动（RPI_NATIVE_MODEL_FREE=1）；旧 rpi 的 ABI 兼容
  检查才使用隔离合成配置。生产代码不注入合成模型密钥。
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

专用面板已支持监听 IP、端口、分享地址和新节点密码；由 Work 节点校验并原子
保存到节点数据目录。密码只保存摘要，不返回明文。保存设置不重启 owner 或
停止任务，下次节点启动生效。显式启动参数优先于保存配置，再使用环境默认值。
公开监听失败时，本机入口仍可修复配置，保留同一节点身份和密码摘要。

历史版本曾在通用组件页显示当前运行时来源、下次选择和待重启状态，并可下载
原生运行时。该耦合已删除：通用组件页不再出现运行时选择/恢复默认，也不再
下载或信任节点宿主 DLL。组件页只安装 UI 包与能力授权。

发布汇总脚本已有六平台 fixture 回归，真实下载与加载在 Windows 验证。发布脚本
在修正前会校验 HTTP 组件制品；该绑定已删除，Work 发布只保留自己的归档与签名
更新资产。
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

> 下表是修正前的历史验收。标有“已撤销”的行不再代表当前 Work：Work 不下载、
> 选择或监督 `rpi-http-extension` 作为宿主运行时，也不在 CI 打包或发布 HTTP
> 组件制品。当前边界见[组件组合](component-composition.md)。

| 要求 | 落点 | 最终验证 |
| --- | --- | --- |
| 真实原生扩展、复用现有 ABI | rpi-http-extension；独立 rpi 通用纯服务启动补丁 | 历史：无模型密钥的实际 DLL 加载、认证及 EOF 退出通过（Work 不再以此作为宿主） |
| 节点能力沉淀、单一 owner | rpi-node-core / node-host / http-node；Work 兼容 re-export | 历史：核心 88 项、共享 owner、会话、SSE/WS、二维码与撤权集成通过 |
| Work UI 通用组合 | 通用组件页、能力发现、WorkspaceClient | 完整原生 UI 构建、正式 WASM 构建和真实 owner 浏览器流程通过 |
| 精心设计且按需加载的扩展面板 | work-http-component；隔离 MessageChannel 桥 | 桌面/手机、懒加载、动作桥、sandbox、配置保存浏览器验证通过 |
| 可下载原生运行时（已撤销） | 原 rpi-node-launcher Store；原独立原生代码授权 | 历史：测试 HTTPS 下载实际 DLL，再通过原有 rpi ABI 启动并退出通过；该 UI 与发布绑定已删除 |
| 可视化配置、下次启动生效 | 专用面板及 http-component.json | 历史：真实 ABI 保存、重启、密码认证、公开监听失败后的本机修复通过 |
| 当前与下次运行时选择（已撤销） | 原 work-host runtime；原通用组件状态区 | 历史：显式路径和下载缓存两条监督启动均通过；UI 状态与恢复默认已删除 |
| 分发可检查 | CI、打包、组件汇总、上传前校验 | 已收缩为 Work 自身归档与签名更新校验；HTTP 组件强制检查已删除 |

修正前的专项脚本（`verify-native-http-extension`、`verify-http-configuration`、
`verify-supervised-host` 和 `verify-component-assets`）随原生宿主耦合一并删除。
当前保留 `verify-component-panels`、`verify-components-workspace` 以及 Work
自身的签名更新/distribution 校验。

## 提交与交付跟进

以上为首次目标验收时的状态。随后用户授权提交、审核和发布，并明确：Work 只合并
本地 main；只发布扩展。Work PR #1 因此关闭，不推送 Work main、不打 Work 发布标签。

- 共享扩展 PR：https://github.com/pi-rust/rpi-package/pull/226。
- rpi 通用纯服务启动 PR：https://github.com/bigfish1913/pi-rust/pull/278。
- 整合最新上游分支，并保留远端并行提交的 macOS 临时目录规范化修正。
- 修正嵌入文件 Git 换行转换、crates.io 版本约束、仓库身份和发行平台声明。
- Work 全量测试 655 通过、0 失败、3 ignored；最终原生 UI 构建、正式 WASM 构建、
  实际 ABI 启动/配置修复、独立面板及正式界面安装/卸载再验证通过。
- 共享扩展严格 Clippy 本机通过，分发合约测试 21 项通过；跨平台最终检查由 PR CI 完成。
- Work 原有 3 个 dead_code 构建警告仍存在；没有宣称 Work 全工作区严格 Clippy 已通过。
- rpi 辅助只读审核因模型服务连接失败未返回结果；审核结论来自源码检查及实际测试。

生成源快照统一使用 LF，保留测试 TLS DER 的二进制字节，便于各平台复核相同内容。
