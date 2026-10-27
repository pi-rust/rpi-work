# GitHub 多平台编译

工作流：`.github/workflows/build.yml`，名称 **Build all platforms**。

- 推送到 `main` / `master`、推送 `v*` 标签、PR，以及 Actions 的 **Run workflow** 都会触发。
- 先检查格式、执行 locked workspace tests 和严格 Clippy，再编译各平台 release。
- Cargo 缓存按架构与用途隔离；同分支新提交会取消旧任务；单个平台失败不会取消其他平台。
- 编译任务只需仓库读取权限，不调用真实模型。Android固定签名与更新清单使用仓库Secrets；`v*` 标签的发布任务在全部构建成功后，以最小写权限创建签名Release，不部署服务端。

## 平台和制品

| 平台 | 架构 / Rust target | Runner | 内容 |
| --- | --- | --- | --- |
| Windows | x64 / `x86_64-pc-windows-msvc` | `windows-2022` | ZIP：`work-ui.exe`、`work-cli.exe` |
| Windows | ARM64 / `aarch64-pc-windows-msvc` | `windows-11-arm` | ZIP：`work-ui.exe`、`work-cli.exe` |
| macOS | Intel / `x86_64-apple-darwin` | `macos-15-intel` | tar.gz：`RPI Work.app`、`work-cli` |
| macOS | Apple Silicon / `aarch64-apple-darwin` | `macos-15` | tar.gz：`RPI Work.app`、`work-cli` |
| Linux | x64 / `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | tar.gz：`work-ui`、`work-cli` |
| Linux | ARM64 / `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | tar.gz：`work-ui`、`work-cli` |
| Android | ARM64 / `aarch64-linux-android` | `ubuntu-24.04` | 固定签名APK及SHA-256；无密钥的PR为dev包 |
| Web / 手机浏览器 | `wasm32-unknown-unknown` | `ubuntu-24.04` | ZIP：独立静态资源与 rpi-web assets 两种目录 |

从 [GitHub Actions](https://github.com/pi-rust/rpi-work/actions/workflows/build.yml) 选择成功运行，在 **Artifacts** 下载对应平台。桌面与Web制品包含版本化压缩包及 `.sha256`，包内 `build-info.json` 记录版本、target、profile、Git commit、Rust 版本和打包时间；Android提供APK及 `.sha256`。macOS/Linux 的内层 tar.gz 保留可执行权限。

桌面包未配置发行签名，也不是 Windows MSI 或 macOS DMG。Windows 需要 WebView2；Linux 动态依赖 GTK 3、WebKitGTK 4.1、libxdo 和兼容 glibc，当前基线为 Ubuntu 24.04。macOS `.app` 未配置 Apple 签名或公证。

手机可以使用响应式Web或Android原生APK；iOS原生IPA尚未接入。客户端不捆绑相邻仓库的 `rpi-http` / `rpi-web` 或模型凭据，需要连接另行运行的兼容节点。

## Android APK

Actions制品名称为 `rpi-work-android-arm64-apk`，包含 `rpi-work-<version>-aarch64-linux-android.apk` 与SHA-256；无固定签名密钥的本地/PR构建文件名带 `-dev`，不用于更新发布。包名 `com.pirust.rpiwork`，最低Android 7.0/API 24，compile/target SDK 35；面向ARM64手机。APK复用正式Rust/Dioxus界面与原生HTTP/SSE客户端。

工具链固定Dioxus CLI 0.7.10、JDK 17、Android SDK 35与NDK 28.2.13676358，CLI下载校验官方release SHA-256。`scripts/ci/build-android.ps1` 只从新的输出目录收集APK，并执行 `apksigner verify`，缺少APK或签名无效都会使任务失败。

脚本在Dioxus生成项目后修正递增versionCode，调用Gradle `assembleRelease`，再通过 `apksigner` 使用固定证书签名并验证。仓库Secrets已经配置Android证书和Ed25519更新签名密钥；私钥不进入源码。此前0.1.0临时开发签名包需卸载后安装固定签名版。发布、密钥备份与覆盖升级说明见 [应用更新](automatic-updates.md)。这不是应用商店发行配置。

Android固定为手机客户端，入口为“连接我的电脑”，仅提供相机/相册扫码，识别并确认后连接电脑；不提供角色切换和手工地址表单。电脑使用本机节点，也可连接其他节点。清单声明网络和扫码相机权限，允许连接局域网HTTP节点；主题、资源与元Agent草稿使用应用WebView本地存储。真机摄像头授权和其他硬件行为仍需设备验证。

本机已验证ARM64 APK签名、原生库架构，并在支持ARM转换的Android 14 x86_64模拟器完成安装和启动；这不等同于ARM64真机验证。

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

# Android：先安装上述SDK/NDK、JDK 17和Dioxus CLI 0.7.10，
# 设置JAVA_HOME、ANDROID_HOME、NDK_HOME，再执行：
rustup target add aarch64-linux-android
pwsh -File scripts/ci/build-android.ps1
```

输出目录为 `target/ci-dist/`。所有二进制位置从 `cargo metadata` 取得，兼容自定义 `CARGO_TARGET_DIR`，不依赖作者电脑的 Cargo 路径。

配置依据：[GitHub runner 列表](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)、[Dioxus 0.7 平台依赖](https://dioxuslabs.com/learn/0.7/getting_started/)、[Dioxus Android](https://dioxuslabs.com/learn/0.7/guides/platforms/mobile/)、[wasm-bindgen CLI](https://wasm-bindgen.github.io/wasm-bindgen/reference/cli.html)。
