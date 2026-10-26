# GitHub 多平台编译

工作流：`.github/workflows/build.yml`，名称 **Build all platforms**。

- 推送到 `main` / `master`、推送 `v*` 标签、PR，以及 Actions 的 **Run workflow** 都会触发。
- 先检查格式、执行 locked workspace tests 和严格 Clippy，再编译各平台 release。
- Cargo 缓存按架构与用途隔离；同分支新提交会取消旧任务；单个平台失败不会取消其他平台。
- 只需要仓库读取权限，不需要模型密钥、签名证书或额外 GitHub Token。不调用真实模型，不自动创建 Release 或部署。

## 平台和制品

| 平台 | 架构 / Rust target | Runner | 内容 |
| --- | --- | --- | --- |
| Windows | x64 / `x86_64-pc-windows-msvc` | `windows-2022` | ZIP：`work-ui.exe`、`work-cli.exe` |
| Windows | ARM64 / `aarch64-pc-windows-msvc` | `windows-11-arm` | ZIP：`work-ui.exe`、`work-cli.exe` |
| macOS | Intel / `x86_64-apple-darwin` | `macos-15-intel` | tar.gz：`RPI Work.app`、`work-cli` |
| macOS | Apple Silicon / `aarch64-apple-darwin` | `macos-15` | tar.gz：`RPI Work.app`、`work-cli` |
| Linux | x64 / `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | tar.gz：`work-ui`、`work-cli` |
| Linux | ARM64 / `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | tar.gz：`work-ui`、`work-cli` |
| Web / 手机浏览器 | `wasm32-unknown-unknown` | `ubuntu-24.04` | ZIP：独立静态资源与 rpi-web assets 两种目录 |

从 [GitHub Actions](https://github.com/pi-rust/rpi-work/actions/workflows/build.yml) 选择成功运行，在 **Artifacts** 下载对应平台。每份 Artifact 包含版本化压缩包及 `.sha256`；包内 `build-info.json` 记录版本、target、profile、Git commit、Rust 版本和打包时间。macOS/Linux 的内层 tar.gz 保留可执行权限。

这些是未签名的编译包，不是 Windows MSI、macOS DMG 或应用商店发行包。Windows 需要 WebView2；Linux 动态依赖 GTK 3、WebKitGTK 4.1、libxdo 和兼容 glibc，当前基线为 Ubuntu 24.04。macOS `.app` 未配置 Apple 签名或公证。

当前手机入口为响应式 Web。工程尚未接入 Dioxus Android/iOS 原生构建，因此不会输出 APK/IPA。桌面包也不捆绑相邻仓库的 `rpi-http` / `rpi-web` 或模型凭据，需要连接另行运行的兼容节点。

## Web 构建

`scripts/ci/install-wasm-bindgen.ps1` 从 **Cargo.lock** 读取 wasm-bindgen 版本，安装一致的 CLI；`scripts/build-ui-web.ps1` 会再次验证版本，避免 Rust/WASM 与生成 JS 的协议不匹配。

Web 压缩包中：

- `standalone/`：静态 HTTP 资源；不能直接用 `file://` 打开。静态服务器本身不提供 Agent API。
- `rpi-web/`：可作为兼容 rpi-web 的 `--assets <目录>`，让页面与节点 API 同源。

Web 任务另跑 WASM 严格 Clippy。CI 不运行依赖本机浏览器、相邻工程和隔离节点的 UI 审查脚本；仓库中的198项UI验收记录是此前独立审查证据，不冒充本工作流的云端测试结果。

## 本机复现

脚本使用跨平台 PowerShell 7（`pwsh`）。在对应原生系统上执行；Linux 先安装 workflow 中列出的开发依赖。

```powershell
# Windows x64 示例；其他平台换用表中的原生 target。
rustup target add x86_64-pc-windows-msvc
cargo build --locked --release --target x86_64-pc-windows-msvc -p work-ui -p work-cli
pwsh -File scripts/ci/package-build.ps1 -Target x86_64-pc-windows-msvc

# Web
rustup target add wasm32-unknown-unknown
pwsh -File scripts/ci/install-wasm-bindgen.ps1
pwsh -File scripts/build-ui-web.ps1
pwsh -File scripts/ci/package-build.ps1 -Target wasm32-unknown-unknown
```

输出目录为 `target/ci-dist/`。所有二进制位置从 `cargo metadata` 取得，兼容自定义 `CARGO_TARGET_DIR`，不依赖作者电脑的 Cargo 路径。

配置依据：[GitHub runner 列表](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)、[Dioxus 0.7 平台依赖](https://dioxuslabs.com/learn/0.7/getting_started/)、[wasm-bindgen CLI](https://wasm-bindgen.github.io/wasm-bindgen/reference/cli.html)。
