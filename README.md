# RPI Work

> 以 Rust 和 GPUI 构建的原生桌面 Agent 工作台。GPUI 是当前唯一维护的 UI 入口。

[English README](README.en.md) · [下载最新版](https://github.com/pi-rust/rpi-work/releases/latest) · [全部版本](https://github.com/pi-rust/rpi-work/releases) · [构建状态](https://github.com/pi-rust/rpi-work/actions/workflows/build.yml) · [文档](#文档)

最近发布的历史版本为 **RPI Work 0.1.22**。当前 GPUI 工作台从源码启动。

> 本项目采用 [MIT License](LICENSE)。个人商业计划、官方服务和商业支持方案仍在规划中；MIT 授权本身允许商业使用，完整说明见[法律与商业状态](docs/legal/licensing.md)。

## 启动 GPUI

在仓库根目录运行：

```bash
task desktop
```

`task`、`task dev` 和 `task dev:desktop` 都指向同一原生窗口。未安装 Task 时可直接运行：

```bash
cargo run --manifest-path apps/work-gpui/Cargo.toml
```

默认打开标注「示例 · 未连接」的布局样例。连接已启动的 Work 节点以读取真实项目、会话和消息：

```bash
task desktop -- --connect http://127.0.0.1:13031
```

需要认证时，在环境或 `.env.local` 中设置 `RPI_WORK_PASSWORD`。当前 GPUI 支持纯文本发送、实时回复与断线恢复。正式安装包仍在迁移。参见 [GPUI 构建说明](apps/work-gpui/README.md)。

## 架构

- [`apps/work-gpui`](apps/work-gpui)：原生桌面工作台，使用独立 Rust workspace。
- `work-client` 与 `work-viewmodel`：共享客户端、会话投影、转写和主题契约。
- `work-node-runtime`、`work-node-service` 与 `work-node-host`：节点数据所有权、Agent 执行和认证传输。
- `work-cli`：独立 CLI 和无头节点入口。
- `design/reui-message-layout/`：已确认的视觉和交互参考，详见[迁移记录](docs/design/confirmed-workbench-migration.md)。

`apps/work-ui`（Dioxus）和 `apps/workbench-ui`（React/ReUI）已废弃，源码作为历史参考。已有桌面、Web 和 Android 安装包属于 Dioxus 版本。

## 开发检查

推荐检查命令：

```bash
cargo fmt --manifest-path apps/work-gpui/Cargo.toml -- --check
cargo test --manifest-path apps/work-gpui/Cargo.toml
cargo clippy --manifest-path apps/work-gpui/Cargo.toml --all-targets -- -D warnings
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

## 历史发布

[历史版本 v0.1.22](https://github.com/pi-rust/rpi-work/releases/tag/v0.1.22) 使用 Dioxus。GPUI 安装包与更新链路待迁移。
