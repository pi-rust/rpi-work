# RPI Work

用 Rust + Dioxus 构建的个人 Agent 工作台。在桌面和手机上访问同一个执行节点，围绕项目完成对话、工具执行、文件预览和代码审阅。

[下载最新版](https://github.com/pi-rust/rpi-work/releases/latest) · [全部版本](https://github.com/pi-rust/rpi-work/releases) · [构建状态](https://github.com/pi-rust/rpi-work/actions/workflows/build.yml) · [开发与设计文档](#文档)

![桌面工作台：项目导航、对话内工具调用、任务上下文和用量统计](docs/images/workspace-desktop.png)

<details>
<summary>查看手机界面</summary>

<img src="docs/images/workspace-mobile.png" alt="手机端会话与工具调用" width="390">

</details>

界面截图来自正式 Dioxus 客户端的验收环境，使用示例会话、文件和统计数据。

## 下载与安装

在 [GitHub Releases](https://github.com/pi-rust/rpi-work/releases/latest) 的 **Assets** 中选择对应平台：

| 平台 | 架构 | 安装包与启动方式 |
| --- | --- | --- |
| Windows | x64 / ARM64 | 解压 ZIP，运行 work-ui.exe；需要 WebView2 Runtime |
| macOS | Intel / Apple Silicon | 解压 tar.gz，打开 RPI Work.app；当前未配置 Apple 签名与公证 |
| Linux | x64 / ARM64 | 解压 tar.gz，运行 work-ui；基于 Ubuntu 24.04，依赖 GTK 3 / WebKitGTK 4.1 / libxdo |
| Android | ARM64 | 安装 APK；最低 Android 7.0，使用固定签名以支持后续覆盖升级 |
| Web | WASM | 解压 Web ZIP，由静态服务器或兼容的 rpi-web 托管 |

每个平台提供 SHA-256 校验文件；Release 另含供客户端验证的 update.json 和 update.json.sig。桌面包同时包含 work-cli。详细依赖与打包方式见 [多平台编译说明](docs/guides/ci-builds.md)。

Android 0.1.0 使用临时开发签名，首次迁移到固定签名版需要卸载重装，手机本地偏好会清除；电脑节点上的项目和会话保留。iOS 尚未提供安装包。

## 快速开始

### 1. 桌面直接使用本机；手机 / Web 另连节点

当前源码中，原生桌面默认在进程内使用纯 Rust 的已发布 `rpi-extcore =0.1.5`，由它通过 stdio `rpi --mode rpc` 执行 Agent 与控制操作。桌面不需要、也不会启动 `rpi-http`，没有本机 HTTP 登录步骤。只需安装兼容的 `rpi`（应用可执行文件旁或 PATH；也可用 `RPI_WORK_RPI_BIN` 指定），模型凭据仍在 rpi 中配置。这是源码行为变化，尚未发布新二进制，也未升级已安装的应用。

手机和 Web 是客户端，需要连接电脑或服务器上可达的兼容节点，例如在项目目录运行：

~~~shell
rpi-http --project . --rpi-bin rpi --port 3030
~~~

从源码启动节点时，在 rpi-package 仓库执行：

~~~shell
cargo run --locked -p rpi-http -- --project <项目目录> --rpi-bin <rpi可执行文件> --port 3030
~~~

也可使用兼容的 rpi-web 服务；模型 Provider、凭据和工具在执行节点配置。

### 2. 连接并打开项目

1. 桌面端启动 RPI Work，自动使用本机原生工作空间（稳定身份 `rpi-work://local`）。
2. 需要远程执行时，进入 **设置 → 执行节点 → 连接其他节点**；可随时切回本机。
3. 打开或添加已有项目，可通过原生目录选择器选择任意当前用户可访问的目录，或输入路径；新建目录按所填完整路径创建，不受远程节点项目根目录限制。可自定义显示名称；点击项目右侧的 **＋** 直接新建会话并输入需求，无须先填写标题。全局 **新建任务** 和 `/new` 沿用当前项目；首条消息发送成功后自动生成标题，之后仍可重命名。

手机进入 **连接我的电脑**，扫描电脑显示的地址二维码，也可从相册识别二维码；需要时展开 **手动输入地址**，填写自己的 HTTP(S) 域名或局域网 IP 与端口。扫码与手动连接共用节点认证，登录后加载项目和会话。电脑默认使用本机节点，需要远程节点时在 **执行节点 → 连接其他节点** 填写地址。节点需正确配置监听地址、允许的 Host 和认证；逐设备配对和项目 ACL 尚需上游协议支持。

项目及会话的 **…** 菜单提供重命名、归档、恢复和分支操作；会话支持置顶、Fork 与 Clone。项目归档只收纳本设备侧栏，会话归档保存到节点，均不删除文件或历史。等待回复显示 loading；在底部时跟随流式输出，上翻后暂停，点击“最新消息”恢复。详见[项目、会话与消息交互](docs/guides/codex-interactions.md)。

桌面每次启动优先本机，最近连接的远程节点需手动切换；手机和 Web 可恢复上次连接。远程节点密码登录后直接加载项目与会话，密码不会写入连接记录。本机沿用既有 `rpi-work/node` 数据目录（可由 `RPI_WORK_DATA_DIR` 覆盖），持有独占锁，退出时关闭所拥有的原生子进程；不开放网络监听。`rpi-work://local` 不是手机可连接的地址，“手机连接这台电脑”分享 / QR 必须指向另行启动的可达 HTTP(S) 节点。详见 [节点连接交互](docs/guides/node-connection.md)。

本机 Web 客户端可通过 `tools/dev/dev-web.mjs` 与实际节点同源访问，支持节点认证、真实模型流式回复和对话内工具结果。启动方式及真实模型复验见 [真实节点聊天](docs/guides/live-chat.md)。

### 3. 更新客户端

**设置 → 应用更新** 默认在启动时及每 6 小时检查最新稳定 Release，可关闭自动检查或手动检查。下载后验证清单签名、文件大小和 SHA-256：

- 桌面：安装并重启，失败时尝试回滚。
- Android：校验包名、版本和签名证书后，由系统确认安装。
- Web：电脑服务端部署新版本后重新加载页面。

会话运行中或保留未发送草稿、附件时暂停安装。详见 [自动更新与签名发布](docs/guides/automatic-updates.md)。

## 主要能力

- **项目与会话**：打开已有项目、显示名称、会话搜索与切换、创建、重命名、fork / clone、历史恢复。
- **单一对话**：流式回复、工具调用和输出、计划、ask 确认、BTW 旁问、停止与断线恢复。
- **文件与变更**：项目文件树、内容预览、Git 状态与 diff、多文件搜索和筛选、移动端列表与详情。
- **运行信息**：队列、上下文压缩、会话追踪与耗时，以及节点报告的 token、缓存、费用等统计。
- **会话能力**：统一展示 Skills、Tools、提示词和具名 Package；支持版本配置、设计包导入导出。
- **元Agent**：个人能力开发项目，用于编写和整理 Skills、Package 与扩展草稿。
- **外观与移动布局**：四种预设主题，浅色 / 深色 / 跟随系统、界面密度和主题 JSON 导入导出。

当前源码通过 `WorkspaceClient` 统一桌面本机原生工作空间与远程 HTTP/SSE：本机项目目录、目录索引、文件和 Git 由本地 Rust 服务提供，不虚构 RPC wire 方法；Agent 执行与控制走 stdio RPC。Web、Android 与显式远程连接仍使用认证 HTTP/SSE。本机目录索引为 `<data>/rpc/projects.json`，会话注册表为 `<data>/rpc/projects/<id>/sessions.json`；旧应用目录索引与匹配项目的注册项可导入，源文件保留，不改写 transcript，不承诺全局原生历史发现。节点资源安装、逐设备授权、直接向模型发送图片等能力仍有协议缺口；附件本地预览不等于模型多模态输入。HTML 原型中的演示状态不代表后端已实现。历史功能边界见 [上游功能审阅](docs/archive/upstream-functional-review.md)，当前传输边界见 [transport 架构](docs/architecture/transport.md)。

## 从源码运行

安装 stable Rust 和对应平台依赖，克隆仓库后执行：

~~~shell
cargo run --locked -p work-ui
~~~

检查代码：

~~~shell
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
~~~

CLI 默认使用原生 RPC（不是默认连接 HTTP 节点）：

~~~shell
cargo run --locked -p work-cli -- agent-info
cargo run --locked -p work-cli -- state
cargo run --locked -p work-cli -- commands
~~~

检查已运行的远程节点时，显式设置 `RPI_WORK_TRANSPORT=http`，再运行 `health`、`capabilities`、`sessions` 或 `watch <session-id>`。仅 HTTP 模式的默认地址为 `http://127.0.0.1:3030/`，可通过 `RPI_WORK_URL` 覆盖；CLI 的 HTTP/ACP 选择不改变桌面默认架构。Web 开发工具位于 `tools/dev/`，跨平台打包脚本位于 `scripts/ci/`，见 [本机构建步骤](docs/guides/ci-builds.md#本机复现)。

本次源码验证已完成 `work-client` / `work-node` / `work-ui` 测试、desktop 与 WASM 检查，以及在 rpi 0.11.0 上显式运行 ignored `local_workspace_smoke`（无模型调用）。未手工验证原生 GUI 目录选择器或真实模型聊天；不代表已发布或已安装应用已更新。

### 代码结构

| 目录 | 职责 |
| --- | --- |
| apps/work-ui | Dioxus 桌面、Android 与 Web 客户端 |
| apps/work-ui/src/app | 启动、工作空间协调、连接生命周期和会话标识 |
| apps/work-ui/src/features | 对话、项目、资源、元 Agent、设置和可观测性 |
| apps/work-ui/src/shared | 公共组件、状态辅助与主题 |
| apps/work-cli | 节点诊断与会话订阅命令行 |
| crates/work-core | 消息、运行状态与事件投影 |
| crates/work-client | WorkspaceClient 本机 / 远程 facade、rpi-extcore 原生工作空间、HTTP/SSE 与 RPC/ACP 适配 |
| crates/work-node | 本机 rpi 路径解析、数据锁与兼容节点监管 |
| crates/work-update | 更新发现、签名验证、下载与安装助手 |
| assets/vendor | 正式客户端使用的二维码库及许可证 |
| design/prototype | 保留内容与校验基准的 HTML 设计参考 |
| tests | 集成测试、原型测试、模块验证入口及 fixture |
| tools/dev、tools/review | 本地开发、截图对照和诊断工具 |
| docs/guides、docs/design、docs/archive | 当前使用指南、架构设计和历史记录 |

目录职责、依赖边界和常用入口见 [项目结构](docs/guides/project-structure.md)。Node.js 与 `tools/package.json` 只用于开发和浏览器验证，不是客户端运行时依赖。

## 构建与版本发布

| 触发方式 | 结果 |
| --- | --- |
| 推送 main / master、PR、手动 Run workflow | 检查并构建八个平台制品，上传 **Actions → Artifacts**，保留 14 天 |
| 推送与 UI 版本一致的 v* 标签 | 全部构建成功后，上传安装包、校验文件和签名清单到 **GitHub Release** |

主分支构建成功不会自动创建 Release。发布稳定版本时，先更新 apps/work-ui/Cargo.toml 和 docs/release-notes.md，提交后打标签：

~~~shell
git tag -a v0.1.9 -m "RPI Work v0.1.9"
git push origin v0.1.9
~~~

标签中的版本必须与 UI 包版本一致。发布任务先创建草稿，上传完整制品并验证签名，最后公开 Release，供客户端下载和自动检查更新。签名配置、失败排查和密钥备份见 [发布说明](docs/guides/automatic-updates.md#发布流程)。

## 文档

- **产品与架构**：[功能设计](docs/design/product-design.md) · [Dioxus / rpi 框架](docs/design/architecture.md) · [上游功能边界](docs/archive/upstream-functional-review.md)
- **对话与项目**：[流式消息](docs/design/streaming.md) · [多文件审阅](docs/design/conversation-and-review.md) · [项目文件](docs/design/project-files.md) · [会话追踪](docs/design/session-tracing.md)
- **能力与扩展**：[Agent / UI 扩展边界](docs/architecture/extensions.md) · [真实原生扩展与实时 UI Demo](docs/guides/extension-demo.md) · [Skills 与交互工具](docs/design/skills-and-interactive-tools.md) · [会话 Package](docs/design/session-packages.md) · [元Agent](docs/design/meta-agent.md)
- **设备与外观**：[手机与电脑连接](docs/guides/node-connection.md) · [主题](docs/design/themes.md) · [字体与完整视觉规格](docs/guides/theme-design.md)
- **构建与验收**：[开发规范](docs/guides/development-standards.md) · [多平台编译](docs/guides/ci-builds.md) · [自动更新](docs/guides/automatic-updates.md) · [原型还原验收](docs/archive/ui-parity-acceptance.md) · [本地测试产物](docs/guides/testing-artifacts.md)

查看 HTML 原型：在仓库根目录运行 python -m http.server 4173 --bind 127.0.0.1，打开 <http://127.0.0.1:4182/prototype/>。原型使用示例数据，设计稿与历史验收记录单独保留在文档中。
