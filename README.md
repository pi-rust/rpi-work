# RPI Work

> 个人维护的跨平台 Agent 工作台：用 Rust + Dioxus 连接本机或远程执行节点，在桌面、浏览器和 Android 上管理项目、会话、工具运行、文件预览与代码审阅。

[English README](README.en.md) · [下载最新版](https://github.com/pi-rust/rpi-work/releases/latest) · [全部版本](https://github.com/pi-rust/rpi-work/releases) · [构建状态](https://github.com/pi-rust/rpi-work/actions/workflows/build.yml) · [文档](#文档)

当前版本：**RPI Work 0.1.20**。项目由个人维护，持续开发中；部分 API 和功能仍可能变化。

> 项目法律和商业状态已移至[单独说明](docs/legal/licensing.md)，不在本 README 中展开。

## 功能概览

- 桌面端使用本机 Work owner，也可以连接远程节点。
- 浏览器 Web 入口与节点共享项目、会话和运行状态。
- Android 通过一次性授权二维码连接可信电脑节点。
- 支持项目与会话管理、流式对话、工具调用、队列、停止和断线恢复。
- 支持文件树、文件预览、Git 状态、Diff、搜索和移动端布局。
- 支持模型供应商、会话配置、资源包、Skills、扩展和运行时信息管理。
- 节点无法连接时显示可操作的恢复提示，而不是空白工作区。
- 桌面工作协同页面可以发现局域网节点，并在确认连接前展示节点身份和健康状态。

## 下载与安装

从 [GitHub Releases](https://github.com/pi-rust/rpi-work/releases/latest) 的 **Assets** 中选择对应平台的文件。每个安装包都有 `.sha256` 校验文件。

| 平台 | 架构 | 说明 |
| --- | --- | --- |
| Windows | x64 / ARM64 | 解压 ZIP，运行 `work-ui.exe`；需要 WebView2 Runtime |
| macOS | Intel / Apple Silicon | 解压后打开 `RPI Work.app`；当前未配置 Apple 签名与公证 |
| Linux | x64 / ARM64 | 解压 tar.gz，运行 `work-ui`；需要 GTK 3、WebKitGTK 4.1 和 libxdo |
| Android | ARM64 | 安装 APK；使用固定签名支持后续覆盖升级 |
| Web | WASM | 由静态服务器或兼容的 `rpi-web` 托管，必须使用 HTTP(S) |

桌面包同时包含 `work-cli`。Release 还提供 `update.json` 和 `update.json.sig`，供客户端验证更新包。详细依赖与打包规则见[多平台编译说明](docs/guides/ci-builds.md)。

## 快速开始

### 桌面端

1. 启动 RPI Work，桌面端默认使用本机工作空间。
2. 打开或添加项目，配置模型供应商，然后创建会话。
3. 需要使用其他电脑时，进入 **设置 → 工作协同**，输入地址、扫码或选择发现到的局域网节点。
4. 节点离线时，首页会显示错误原因和 **打开连接设置** 操作。

本机 Agent 执行需要兼容的 `rpi` 可执行文件。桌面启动时会检查本机环境；如果缺少运行时，应用会提供检测、安装或选择已有程序的入口。模型凭据保存在执行节点，不写入安装包。

### Web 端

Web 入口必须由节点提供可用的 Web assets，或者配置同源 Web proxy。普通 API/协作端口不等于 Web UI 入口；浏览器需要从同源获取页面、静态资源和 `/api/v1/*` 请求。

开发环境：

```bash
task dev
```

默认地址通常为 `http://127.0.0.1:13031/`。修改 UI 并需要热刷新时使用：

```bash
task dev:desktop
```

开发路径通常使用 `14180` Web 入口和 `13031` owner/API 端口。不要在同一个数据目录下同时启动多个 owner。

常用配置：

```dotenv
RPI_WORK_HTTP_BIND=0.0.0.0
RPI_WORK_HTTP_PORT=13031
RPI_WORK_HTTP_PASSWORD=change-me
RPI_WORK_HTTP_ASSETS=target/web-release/parity-web
# RPI_WORK_HTTP_WEB_PORT=    # 使用同源静态 assets 时留空
```

不要把密码提交到仓库、命令行参数或公开日志中。公网部署应使用 HTTPS 反向代理，并保留 Host、Origin 和认证校验。

### Android

Android 原生 App 通过桌面或已登录 Web 入口生成的一次性授权二维码连接节点。授权码短期有效、只能兑换一次，不包含长期密码。首次连接失败时，可在连接页面手动输入地址、扫码或恢复上次连接；进入工作区后节点离线会从首页进入 **工作协同** 重新连接。

## 架构与连接边界

- `work-ui`：Dioxus 桌面、Android 和 Web 客户端。
- `work-client`：本机/远程 `WorkspaceClient`、HTTP/WebSocket、快照、实时流和恢复逻辑。
- `work-node-runtime`：实际的 workspace owner、RPC/ACP、会话、项目、资源和文件服务。
- `work-node-service` 与 `work-node-host`：同一个 owner 上的认证 HTTP/WebSocket、Web assets、共享和一次性授权。
- `work-cli`：连接已运行节点或启动无头 owner 的 CLI。

桌面本机和远程节点使用统一的客户端抽象，但数据所有权仍归节点。切换节点不会把一个节点的项目、会话或凭据混到另一个节点。Web/Android 使用认证 HTTP 接口和 WebSocket；本机 Agent 使用隔离的 stdio RPC/ACP 路径。

## 从源码运行

需要 stable Rust、Node.js（开发和浏览器验证）以及对应平台的桌面依赖。

```bash
cargo run --locked -p work-ui
```

推荐检查命令：

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

CLI 示例：

```bash
cargo run --locked -p work-cli -- --help
cargo run --locked -p work-cli -- projects
cargo run --locked -p work-cli -- sessions
cargo run --locked -p work-cli -- watch SESSION_ID
```

更多开发入口见 [`Taskfile.yml`](Taskfile.yml)、[本机构建步骤](docs/guides/ci-builds.md) 和[测试与产物](docs/guides/testing-artifacts.md)。

## 安全与隐私

- 不要把节点密码、模型密钥、Android 签名材料或更新签名密钥提交到仓库。
- 共享和 Web 监听默认只适合可信网络；公网请使用 HTTPS 和额外的网络访问控制。
- Android 授权二维码是一次性短期凭据，不应截图公开或重复使用。
- 节点发现只提供候选地址；客户端仍会探测节点身份、协议和健康状态，发现本身不等于授权。
- 第三方资源按照各自的上游许可证使用，见 [`assets/vendor`](assets/vendor) 中的清单。

## 贡献

欢迎提交 Issue、文档改进和 Pull Request。提交代码前请运行格式检查、相关测试和 Clippy；涉及 Web、Android 或 UI 的改动应同时说明验证平台和未覆盖的边界。

- [开发规范](docs/guides/development-standards.md)
- [项目结构](docs/guides/project-structure.md)
- [架构总览](docs/architecture/overview.md)
- [节点连接](docs/guides/node-connection.md)
- [自动更新与发布](docs/guides/automatic-updates.md)

## 文档

- **产品与架构**：[产品范围](docs/design/product-design.md) · [架构总览](docs/architecture/overview.md) · [传输架构](docs/architecture/transport.md) · [节点与会话归属](docs/design/node-session-ownership.md)
- **使用与连接**：[项目、会话与消息](docs/guides/codex-interactions.md) · [节点连接与扫码](docs/guides/node-connection.md) · [ACP 后端](docs/guides/acp-backends.md) · [主题视觉规格](docs/guides/theme-design.md)
- **资源与扩展**：[扩展架构](docs/architecture/extensions.md) · [原生资源开发](docs/guides/native-resource-authoring.md) · [组件组合](docs/architecture/component-composition.md)
- **开发与验证**：[多平台编译](docs/guides/ci-builds.md) · [自动更新](docs/guides/automatic-updates.md) · [测试与产物](docs/guides/testing-artifacts.md) · [历史记录](docs/archive/README.md)

## 发布

稳定版本通过 `v*` 标签触发 GitHub Actions，构建多平台安装包，生成 SHA-256 校验文件，并创建签名更新清单。当前发布版本为 [`v0.1.20`](https://github.com/pi-rust/rpi-work/releases/tag/v0.1.20)。

发布新版本时同步更新 `apps/work-ui/Cargo.toml`、`docs/release-notes.md` 和本 README 中的当前版本，再创建与 UI 包版本一致的标签。
