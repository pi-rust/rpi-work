# RPI Work

当前审查状态（2026-10-27）：正式 Rust / Dioxus 客户端已完成本轮原型还原验收。最终冻结版本通过29套198个行为检查、5种视口63处主布局/排版对照、125组页面配对，以及真实 rpi-web 资源/认证/模型流式结算。原型39个基准文件未改。见[验收结论与边界](docs/ui-parity-acceptance.md)、[逐项矩阵](docs/ui-parity-matrix.md)和[测试源码与本地产物说明](docs/testing-artifacts.md)。

Rust Agent 工作台的功能设计、交互原型与正式实现。真实 rpi-web 已直接提供Dioxus资源与HTTP/SSE API，同源密码登录及一次真实模型流式回答/完成结算已验证（测试关闭全部工具）。设备ACL、原生RPC多模态输入、节点资源安装仍有接口缺口。Android已接入原生APK构建；iOS原生包与真机摄像头尚未验证。

- [打开 HTML 原型](prototype/index.html)
- [功能与交互设计](docs/product-design.md)
- [框架与复用方案：Dioxus / rpi-web / rpi](docs/architecture.md)
- [工具、队列、Provider、上下文、用量与附件设计](docs/runtime-interactions.md)
- [单一对话与多文件审阅（最新交互）](docs/conversation-and-review.md)
- [项目文件树与内容预览](docs/project-files.md)
- [会话追踪、耗时瀑布图与调用详情](docs/session-tracing.md)
- [扩展、Skills 与 Package 的下载前置](docs/resource-acquisition.md)
- [元Agent · 个人能力开发项目](docs/meta-agent.md)
- [打开已有项目与自定义显示名称](docs/project-opening.md)
- [Skills、ask_user、BTW 与常见交互工具](docs/skills-and-interactive-tools.md)
- [流式消息、工具输出、停止与断线恢复](docs/streaming.md)
- [会话顶部资源与可下载 Package](docs/session-packages.md)
- [整体主题抽象与切换](docs/themes.md)
- [手机扫码 / 高级手工连接 PC](docs/mobile-connection.md)
- [基于当前 pi-rust / rpi-web 的功能审阅](docs/upstream-functional-review.md)
- [验证记录](docs/prototype-validation.md)
- [GitHub 多平台编译与下载](docs/ci-builds.md)

## 自动编译

GitHub Actions 会在主分支推送、PR、`v*` 标签和手动触发时，执行 Rust 检查并编译 Windows / macOS / Linux 的 x64 与 ARM64、Web，以及 Android ARM64 APK。运行完成后可下载带 SHA-256 的版本化制品；macOS 包含未签名 `.app`，Android为可安装的开发签名测试包。触发方式、依赖和本地复现见 [多平台编译说明](docs/ci-builds.md)。

## 本地预览

`prototype/index.html` 可以直接用浏览器打开，无第三方字体、脚本或 CDN 依赖。也可以在仓库根目录运行：

```powershell
python -m http.server 4173 --bind 127.0.0.1
```

然后打开 <http://127.0.0.1:4173/prototype/>。

右上角主题按钮或个人设置可切换「纸与苔绿 / 海湾蓝 / 石墨紫 / Reddit 橙」，分别选择浅色、深色或跟随系统，以及标准/紧凑密度。支持即时预览、取消恢复、保存偏好和主题 JSON 导入导出；会话、工具、Package 与变更共享同一套主题变量。

可体验：项目/会话、对话、diff、计划、工具调用详情/失败重试/子任务取消、消息队列、Provider 设置、上下文压缩、token/缓存/费用/速度统计、图片与文档附件、扩展、节点配对、主题和手机布局。

任务只有「对话 / 变更」两个标签：计划、工具与命令输出都在对话内。变更页提供 120 文件样例、目录/搜索/类型/未审阅筛选、本轮范围、单文件 diff 和移动端列表/详情。

会话顶部统一展示具名 Package、固定版本，以及本会话 Skills、Tools、提示词配置；输入框保留 Skills 快捷选择。Packages 资源库支持导出包含正文的 JSON 设计包、重新导入、模拟安装与显式启用；安装更新不会自动替换会话版本。每轮消息保存配置快照。

输入区的「常用能力」可体验 ask_user 单选/多选/自由回答、BTW 旁问、todo 和 goal 状态。侧栏「技能库」展示示例目录、来源与全文载入证据；BTW 使用独立旁路面板，显式引用才进入主草稿。

正常发送即逐段显示回复与工具输出，BTW 也支持流式回答。点击标题旁「流式演示」或输入 `/stream`，展开对话内的「流式演示控制」，可模拟断线恢复、快照重建、停止、失败与等待最终结算。阅读历史时保持滚动位置，点击「新进展」继续跟随。

「执行节点」展示 PC 服务端 / 手机客户端：PC 生成二维码，手机可用相机入口、二维码图片或链接识别；高级模式可手工填写服务地址和配对码。两种模式均经过电脑确认和项目授权，并可体验过期、拒绝、撤销和断线重连。QR 与本地图片解码可用，实际配对和同步仍是模拟，没有开放电脑网络端口。

原型不主动读取项目文件、不执行 shell、不收集密钥，也不连接设备或模型。用户主动选择/粘贴/拖入附件后做本地预览；主动打开相机或选择二维码图片后在本机识别，不上传图像。主题偏好保存在浏览器本地，任务、附件和配对状态刷新后重置。页面中的代码、测试结果、时间、设备与统计都是交互样例。

## Rust 核心实现

已建立最小 Cargo workspace：

- `crates/work-core`：UI/网络无关的运行状态 reducer，处理稳定消息块、工具输出、游标去重、结算语义与重同步。
- `crates/work-client`：对接现有 rpi-http/rpi-web `/api/v1` 的 HTTP/SSE transport，包含认证、项目/目录/会话管理（创建、打开、重命名、fork/clone/delete）、项目文件和文件夹创建、Git status/diff、prompt/abort、队列、确认、历史/runtime/model/tool/compact 命令、快照、SSE 解码与事件归一化；断线按游标重建快照，不重发业务 prompt。
- `crates/work-node`：桌面本地 rpi-http/rpi-web 进程监管、参数组装、端口就绪检测和退出清理；就绪检测先确认受管子进程，避免把外部占用端口误认成自身节点。

运行验证：

```powershell
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

另有一个最小真实 transport 入口 `apps/work-cli`，可在已运行的 rpi-http 或 rpi-web 上检查节点并订阅会话。默认地址是 `http://127.0.0.1:3030/`，也可通过 `RPI_WORK_URL` 覆盖：

```powershell
cargo run -p work-cli -- health
cargo run -p work-cli -- capabilities
cargo run -p work-cli -- projects
cargo run -p work-cli -- folders src
cargo run -p work-cli -- sessions
cargo run -p work-cli -- state <session-id>
$env:RPI_WORK_URL = 'http://127.0.0.1:3030/'
cargo run -p work-cli -- watch <session-id>
```

## 真实节点快速验证

使用现有 rpi-http API-only 服务启动节点（在 `rpi-package` 根目录可执行 `cargo run --locked -p rpi-http -- --project <project> --rpi-bin <rpi> --port 3030`）；如需浏览器备用界面，也可以使用 rpi-web standalone 或 `rpi --webserver`。确保允许客户端使用的 Host（例如 `127.0.0.1:3030`）出现在 `allowedHosts` 中；受保护节点还需要先通过 UI 登录。然后执行：

```powershell
$env:RPI_WORK_URL = 'http://127.0.0.1:3030/'
cargo run -p work-cli -- health
cargo run -p work-cli -- capabilities
cargo run -p work-cli -- projects
cargo run -p work-cli -- sessions
cargo run -p work-cli -- watch <session-id>
```

`health` 对 rpi-http/rpi-web 的 `service`/`protocol` 响应和带 `ok` 响应都兼容；`watch` 会先恢复 snapshot，再从 `lastSeq` 之后订阅 SSE，发送 `Last-Event-ID`，断线时按 snapshot 重建，不把连接关闭或 `agent_end` 当作最终成功。

## 首版历史验证（不能代替当前源码验收）

| 验收项 | 状态 | 证据/边界 |
| --- | --- | --- |
| reducer 稳定顺序、去重、重同步、失败/中断保留内容、最终结算 | 通过 | `cargo test --workspace` |
| rpi-http HTTP/SSE | 通过 | 已实测 health、capabilities、projects、sessions、create、snapshot、queue、SSE |
| rpi-web HTTP/SSE 兼容 | 通过 | 已实测同一套 CLI/API 流程 |
| 项目文件、Git status/diff、队列、dialog | 通过 | client 测试 + 真实节点路由探测 |
| Dioxus native 启动 | 首版历史通过 | 曾启动 `work-ui.exe`；当前源码需重新构建，不能用旧进程作为证明 |
| Dioxus WASM 编译 | 通过 | `cargo check --target wasm32-unknown-unknown -p work-ui` |
| 完整真实模型回合、工具执行和实际 dialog 触发 | 待凭据/交互 | 需要配置 provider 并由用户在运行节点中发送 prompt |
| 设备配对、附件传输、移动原生打包 | 未纳入首版 | 需要新增协议和平台环境，不用模拟状态宣称完成 |

`apps/work-ui` 是首个 Dioxus 客户端壳，使用 `work-client` 真实读取节点健康、认证状态、能力、会话和项目。当前 UI 已按 `prototype/index.html` 的工作空间骨架对齐：桌面为项目导航 / 对话工作区 / 任务上下文三层结构，移动端提供导航抽屉、会话切换和单列 composer；支持项目搜索/添加、会话搜索与管理、快照历史恢复、SSE 实时事件、断线恢复、发送 prompt、停止请求、队列/dialog、项目文件 modal、文件内容、Git 状态和单文件 diff。视觉对齐包括 prototype bar、面包屑、状态徽章、tab、消息时间线、底部 composer、Inspector 分区、响应式断点和触摸焦点状态。仍未伪造附件上传、设备配对、Packages/Skills 协议或移动原生打包能力。可执行：

```powershell
cargo run -p work-ui
```

它目前是纵向 MVP 的节点连接/认证、项目搜索与创建、会话创建/搜索/重命名/fork/clone/delete、历史快照和断线恢复、原型同构侧栏/对话区/Inspector、真实 projection 消息和工具块、审批/dialog、队列 composer、Project Files modal、Changes 文件列表与单文件预览、项目目录/文件内容、Git 变更文件与 diff。完整变更写回、逐设备配对授权、直接图片模型输入和移动原生打包仍未完成。rpi-http 已有项目文件上传路由；当前客户端未接入该路由，上传文件也不等于原生RPC多模态输入。原型中的这些功能仍明确标为模拟，不会由客户端伪造完成状态。


## 当前推荐

以 **Dioxus 客户端 + rpi-http API 服务（兼容 rpi-web）+ rpi RPC 宿主** 为首版纵向闭环。手机先通过响应式 Web 使用电脑/服务器上的 Agent，再验证 Dioxus Android/iOS 包装。Tauri + 现有 React 前端保留为更快复用 UI 的备选。

此仓库包含设计材料、静态原型和已接入真实服务的 Rust workspace；未修改相邻 `pi-rust`、`rpi-package` 仓库。



