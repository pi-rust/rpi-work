# RPI Work

> 个人维护的跨平台 Agent 工作台：用 Rust + Dioxus 连接本机或远程执行节点，在桌面、浏览器和 Android 上管理项目、会话、工具运行、文件预览与代码审阅。
>
> A personal, cross-platform Agent workbench built with Rust and Dioxus. Connect to a local or remote execution node from desktop, browser, or Android, and manage projects, sessions, tool runs, file previews, and code review in one workspace.

[下载最新版 / Latest release](https://github.com/pi-rust/rpi-work/releases/latest) · [全部版本 / All releases](https://github.com/pi-rust/rpi-work/releases) · [构建状态 / CI](https://github.com/pi-rust/rpi-work/actions/workflows/build.yml) · [文档 / Documentation](#文档--documentation)

## 当前状态 / Status

- 最新版本 / Latest release: **RPI Work 0.1.20**
- 项目类型 / Project type: **个人项目 / Personal project**
- 维护状态 / Maintenance: 持续开发中，部分能力仍在演进 / Actively developed; some APIs and features are still evolving
- 当前发布平台 / Current platforms: Windows、macOS、Linux、Android ARM64 和 Web / Windows, macOS, Linux, Android ARM64, and Web

## 功能概览 / What it does

### 中文

- 桌面端使用本机 Work owner，也可以连接远程节点。
- 浏览器 Web 入口与桌面共享同一个节点的项目、会话和运行状态。
- Android 通过一次性授权二维码连接可信电脑节点。
- 支持项目与会话管理、流式对话、工具调用、队列、停止与断线恢复。
- 支持文件树、文件预览、Git 状态、Diff、搜索和移动端布局。
- 支持模型供应商、会话配置、资源包、Skills、扩展和运行时信息管理。
- 节点无法连接时显示可操作的恢复提示，而不是空白工作区。
- 桌面工作协同页面可以发现局域网节点，并在确认连接前展示节点身份和健康状态。

### English

- Use a local Work owner on desktop or connect to a remote node.
- The browser Web entry shares the node's projects, sessions, and runtime state.
- Android connects to a trusted computer node through a one-time authorization QR code.
- Manage projects and sessions, stream conversations, run tools, queue work, stop runs, and recover after disconnects.
- Browse files, preview content, inspect Git status, review diffs, search, and use mobile layouts.
- Manage model providers, session configuration, packages, Skills, extensions, and runtime details.
- When a node is unavailable, the UI shows an actionable recovery state instead of a blank workspace.
- Desktop collaboration can discover LAN nodes and show their identity and health before connection confirmation.

## 下载与安装 / Download and install

从 [GitHub Releases](https://github.com/pi-rust/rpi-work/releases/latest) 的 **Assets** 中选择对应平台的文件。每个安装包都有对应的 `.sha256` 校验文件。

Choose the package for your platform from the **Assets** section of [GitHub Releases](https://github.com/pi-rust/rpi-work/releases/latest). Every package is accompanied by a `.sha256` checksum file.

| 平台 / Platform | 架构 / Architecture | 说明 / Notes |
| --- | --- | --- |
| Windows | x64 / ARM64 | 解压 ZIP，运行 `work-ui.exe`；需要 WebView2 Runtime / Extract the ZIP and run `work-ui.exe`; WebView2 Runtime is required |
| macOS | Intel / Apple Silicon | 解压后打开 `RPI Work.app`；当前未配置 Apple 签名与公证 / Open `RPI Work.app`; Apple signing and notarization are not configured |
| Linux | x64 / ARM64 | 解压 tar.gz，运行 `work-ui`；需要 GTK 3、WebKitGTK 4.1 和 libxdo / Extract the archive and run `work-ui`; GTK 3, WebKitGTK 4.1, and libxdo are required |
| Android | ARM64 | 安装 APK；使用固定签名支持后续覆盖升级 / Install the APK; the persistent signing key supports upgrades |
| Web | WASM | 由静态服务器或兼容的 `rpi-web` 托管；必须使用 HTTP(S)，不能直接打开 `file://` / Serve with a static server or compatible `rpi-web`; HTTP(S) is required, not `file://` |

桌面包同时包含 `work-cli`。Release 还会提供 `update.json` 和 `update.json.sig`，供客户端验证更新包。详细的平台依赖和打包规则见[多平台编译说明](docs/guides/ci-builds.md)。

Desktop packages also include `work-cli`. Releases also contain `update.json` and `update.json.sig` for client-side update verification. See the [multi-platform build guide](docs/guides/ci-builds.md) for packaging details.

## 快速开始 / Quick start

### 桌面端 / Desktop

```bash
# 下载并启动对应平台的 RPI Work 包
# Download and launch the RPI Work package for your platform
```

1. 启动 RPI Work；桌面端默认使用本机工作空间。
2. 打开或添加项目，配置模型供应商，然后创建会话。
3. 需要使用其他电脑时，进入 **设置 → 工作协同**，输入地址、扫码或选择发现到的局域网节点。
4. 节点离线时，首页会显示错误原因和 **打开连接设置** 操作。

1. Launch RPI Work; desktop defaults to the local workspace.
2. Open or add a project, configure a model provider, and create a session.
3. To use another computer, open **Settings → Collaboration**, enter its address, scan a code, or select a discovered LAN node.
4. If the node is unavailable, the home screen shows the reason and an **Open connection settings** action.

本机 Agent 执行需要兼容的 `rpi` 可执行文件。桌面启动时会检查本机环境；如果缺少运行时，应用会提供检测、安装或选择已有程序的入口。模型凭据保存在执行节点，不写入安装包。

Local Agent execution requires a compatible `rpi` executable. Desktop startup checks the local environment and provides detection, installation, or file-selection recovery when it is missing. Model credentials stay on the execution node and are not bundled with the application.

### Web 端 / Web

Web 入口必须由节点提供可用的 Web assets，或者配置同源 Web proxy。普通 API/协作端口不等于 Web UI 入口；浏览器需要从同源获取页面、静态资源和 `/api/v1/*` 请求。

The Web entry requires either usable Web assets served by the node or a configured same-origin Web proxy. An API/collaboration port alone is not a Web UI entry; the browser needs the page, static assets, and `/api/v1/*` requests from the same origin.

开发环境可使用：

```bash
task dev
```

默认地址通常为 `http://127.0.0.1:13031/`。修改 UI 并需要热刷新时使用：

```bash
task dev:desktop
```

开发路径通常使用 `14180` Web 入口和 `13031` owner/API 端口。不要在同一个数据目录下同时启动多个 owner。

The default development URL is usually `http://127.0.0.1:13031/`. For UI hot reload, use `task dev:desktop`; it normally uses Web port `14180` and owner/API port `13031`. Do not start multiple owners against the same data directory.

常用配置 / Common configuration:

```dotenv
RPI_WORK_HTTP_BIND=0.0.0.0
RPI_WORK_HTTP_PORT=13031
RPI_WORK_HTTP_PASSWORD=change-me
RPI_WORK_HTTP_ASSETS=target/web-release/parity-web
# RPI_WORK_HTTP_WEB_PORT=      # 使用同源静态 assets 时留空 / leave empty for direct same-origin assets
```

不要把密码提交到仓库、命令行参数或公开日志中。公网部署应使用 HTTPS 反向代理，并保留 Host、Origin 和认证校验。

Never commit the password or place it in command-line arguments or public logs. For public deployment, use an HTTPS reverse proxy and keep Host, Origin, and authentication checks enabled.

### Android

Android 原生 App 通过桌面或已登录 Web 入口生成的一次性授权二维码连接节点。授权码短期有效、只能兑换一次，不包含长期密码。首次连接失败时，可在连接页面手动输入地址、扫码或恢复上次连接；进入工作区后节点离线会从首页进入 **工作协同** 重新连接。

The Android app connects through a one-time authorization QR code generated by the desktop or an authenticated Web entry. The code is short-lived, single-use, and does not contain a long-term password. If the first connection fails, use the connection screen to enter an address, scan again, or restore the previous connection. Once inside the workspace, an unavailable node can be repaired through **Collaboration** from the home screen.

## 架构与连接边界 / Architecture and boundaries

### 中文

- `work-ui`：Dioxus 桌面、Android 和 Web 客户端。
- `work-client`：本机/远程 `WorkspaceClient`、HTTP/WebSocket、快照、实时流和恢复逻辑。
- `work-node-runtime`：实际的 workspace owner、RPC/ACP、会话、项目、资源和文件服务。
- `work-node-service` 与 `work-node-host`：同一个 owner 上的认证 HTTP/WebSocket、Web assets、共享和一次性授权。
- `work-cli`：连接已运行节点或启动无头 owner 的 CLI。

桌面本机和远程节点使用统一的客户端抽象，但数据所有权仍归节点。切换节点不会把一个节点的项目、会话或凭据混到另一个节点。Web/Android 使用认证 HTTP 接口和 WebSocket；本机 Agent 使用隔离的 stdio RPC/ACP 路径。

### English

- `work-ui`: Dioxus desktop, Android, and Web clients.
- `work-client`: local/remote `WorkspaceClient`, HTTP/WebSocket transport, snapshots, realtime streams, and recovery.
- `work-node-runtime`: the actual workspace owner, RPC/ACP, sessions, projects, resources, and file services.
- `work-node-service` and `work-node-host`: authenticated HTTP/WebSocket, Web assets, sharing, and one-time authorization on the same owner.
- `work-cli`: CLI for connecting to an existing node or starting a headless owner.

The client abstraction is shared across local and remote nodes, but data ownership remains node-scoped. Switching nodes does not mix projects, sessions, or credentials. Web/Android use authenticated HTTP and WebSocket paths; local Agent execution uses isolated stdio RPC/ACP paths.

## 从源码运行 / Run from source

需要 stable Rust、Node.js（开发和浏览器验证）以及对应平台的桌面依赖。

You need stable Rust, Node.js for development/browser checks, and the desktop dependencies for your platform.

```bash
cargo run --locked -p work-ui
```

推荐检查命令 / Recommended checks:

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

CLI 示例 / CLI examples:

```bash
cargo run --locked -p work-cli -- --help
cargo run --locked -p work-cli -- projects
cargo run --locked -p work-cli -- sessions
cargo run --locked -p work-cli -- watch SESSION_ID
```

更多开发入口见 [`Taskfile.yml`](Taskfile.yml)、[本机构建步骤](docs/guides/ci-builds.md) 和 [测试与产物](docs/guides/testing-artifacts.md)。

See [`Taskfile.yml`](Taskfile.yml), the [local build guide](docs/guides/ci-builds.md), and [testing and artifacts](docs/guides/testing-artifacts.md) for more development workflows.

## 目录结构 / Repository layout

| 目录 / Directory | 作用 / Purpose |
| --- | --- |
| `apps/work-ui` | 桌面、Android、Web UI / desktop, Android, and Web UI |
| `apps/work-cli` | CLI 与无头 owner 入口 / CLI and headless owner entry |
| `crates/client/work-client` | 客户端传输、连接恢复与投影 / client transport, recovery, and projections |
| `crates/node/work-node-runtime` | workspace owner 与执行运行时 / workspace owner and execution runtime |
| `crates/transport/work-node-service` | HTTP/WebSocket 服务和协议路由 / HTTP/WebSocket service and routes |
| `crates/host/work-node-host` | GUI/无头宿主、HTTP 配置与共享 / GUI/headless host, HTTP configuration, and sharing |
| `crates/update/work-update` | 更新发现、签名验证和安装 / update discovery, signature verification, and installation |
| `docs` | 指南、架构、设计和审查记录 / guides, architecture, design, and reviews |
| `tests` | 集成测试、结构检查和 fixtures / integration tests, structure checks, and fixtures |

## 安全与隐私 / Security and privacy

- 不要把节点密码、模型密钥、Android 签名材料或更新签名密钥提交到仓库。
- 共享和 Web 监听默认应只用于可信网络；公网请使用 HTTPS 和额外的网络访问控制。
- Android 授权二维码是一次性短期凭据，不应截图公开或重复使用。
- 节点发现只提供候选地址；客户端仍会探测节点身份、协议和健康状态，发现本身不等于授权。
- 第三方资源按照各自的上游许可证使用，见 [`assets/vendor`](assets/vendor) 中的清单。
- 发现安全问题请不要公开发布可利用细节，优先通过 GitHub Security Advisories 或私下联系维护者。

- Do not commit node passwords, model keys, Android signing material, or update-signing keys.
- Treat sharing and Web listeners as trusted-network features; use HTTPS and additional network controls for public deployments.
- Android authorization QR codes are short-lived, single-use credentials; do not publish screenshots of them.
- Discovery only supplies candidate addresses; the client still verifies node identity, protocol, and health. Discovery is not authorization.
- Third-party resources remain subject to their own upstream licenses; see the inventory in [`assets/vendor`](assets/vendor).
- For security issues, avoid publishing exploitable details and prefer GitHub Security Advisories or private contact with the maintainer.

## 许可证与商业状态 / License and commercial status

### 开源许可证 / Open-source license

本仓库由个人维护。除第三方资源另有许可证外，本项目源代码按 [MIT License](LICENSE) 发布。

This is a personally maintained project. Unless a third-party asset states otherwise, the project source code is released under the [MIT License](LICENSE).

**重要说明 / Important:** MIT License 本身允许复制、修改、再分发和商业使用。下面的“商用待定”不撤销 MIT 已授予的代码权利，而是表示项目目前没有单独的商业产品、托管服务、SLA、技术支持、官方发行版品牌授权或商业合作方案。

**Important:** The MIT License permits copying, modification, redistribution, and commercial use. “Commercial plans pending” does not revoke rights already granted by MIT; it means that this project currently has no separate commercial product, hosted service, SLA, paid support, official distribution program, or brand-partnership terms.

- 代码 / Code: MIT，允许商业使用，但须保留版权和许可声明 / MIT; commercial use is allowed with the required notices.
- RPI Work 名称、Logo 和品牌 / Name, logo, and branding: 不随 MIT 自动授权 / not automatically licensed by MIT.
- 第三方依赖和 bundled assets / Third-party dependencies and bundled assets: 以各自许可证为准 / governed by their own licenses.
- 官方商业服务、托管、支持和品牌合作 / Official commercial services, hosting, support, and branding partnerships: **待定 / TBD**.

如果你需要把 RPI Work 集成到商业产品中，请先核对本仓库代码、第三方依赖和资源的许可证边界；不要把官方名称或 Logo 当作代码许可证的一部分。

If you plan to integrate RPI Work into a commercial product, review the license boundaries for this repository, its dependencies, and bundled assets. Do not treat the official name or logo as part of the code license.

## 贡献 / Contributing

欢迎提交 Issue、文档改进和 Pull Request。提交代码前请运行格式检查、相关测试和 Clippy；涉及 Web、Android 或 UI 的改动应同时说明验证平台和未覆盖的边界。

Issues, documentation improvements, and pull requests are welcome. Before submitting code, run formatting checks, relevant tests, and Clippy. For Web, Android, or UI changes, include the platforms you verified and any uncovered boundaries.

- [开发规范 / Development standards](docs/guides/development-standards.md)
- [项目结构 / Project structure](docs/guides/project-structure.md)
- [架构总览 / Architecture overview](docs/architecture/overview.md)
- [节点连接 / Node connection](docs/guides/node-connection.md)
- [自动更新与发布 / Updates and releases](docs/guides/automatic-updates.md)

## 文档 / Documentation

- **产品与架构 / Product and architecture**: [产品范围](docs/design/product-design.md) · [架构总览](docs/architecture/overview.md) · [传输架构](docs/architecture/transport.md) · [节点与会话归属](docs/design/node-session-ownership.md)
- **使用与连接 / Usage and connection**: [项目、会话与消息](docs/guides/codex-interactions.md) · [节点连接与扫码](docs/guides/node-connection.md) · [ACP 后端](docs/guides/acp-backends.md) · [主题视觉规格](docs/guides/theme-design.md)
- **资源与扩展 / Resources and extensions**: [扩展架构](docs/architecture/extensions.md) · [原生资源开发](docs/guides/native-resource-authoring.md) · [组件组合](docs/architecture/component-composition.md)
- **开发与验证 / Development and validation**: [多平台编译](docs/guides/ci-builds.md) · [自动更新](docs/guides/automatic-updates.md) · [测试与产物](docs/guides/testing-artifacts.md) · [历史记录](docs/archive/README.md)

## 发布 / Releases

稳定版本通过 `v*` 标签触发 GitHub Actions，构建多平台安装包，生成 SHA-256 校验文件，并创建签名更新清单。当前发布版本为 [`v0.1.20`](https://github.com/pi-rust/rpi-work/releases/tag/v0.1.20)。

Stable releases are triggered by `v*` tags. GitHub Actions builds the platform packages, generates SHA-256 checksums, and creates a signed update manifest. The current release is [`v0.1.20`](https://github.com/pi-rust/rpi-work/releases/tag/v0.1.20).

发布新版本时同步更新 `apps/work-ui/Cargo.toml`、`docs/release-notes.md` 和本 README 中的当前版本，再创建与 UI 包版本一致的标签。

For a new release, update `apps/work-ui/Cargo.toml`, `docs/release-notes.md`, and the current-version reference in this README, then create a tag matching the UI package version.
