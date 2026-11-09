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

手机可以使用响应式Web或Android原生APK；iOS原生IPA尚未接入。客户端不捆绑相邻仓库的 `rpi-http` / `rpi-web`、rpi HTTP 扩展或模型凭据。桌面在安装兼容 rpi 后默认托管本机 Work owner；Web/Android 连接已有 Work 节点，显式远程桌面也只作为客户端。

## Android APK

Actions制品名称为 `rpi-work-android-arm64-apk`，包含 `rpi-work-<version>-aarch64-linux-android.apk` 与SHA-256；无固定签名密钥的本地/PR构建文件名带 `-dev`，不用于更新发布。包名 `com.pirust.rpiwork`，最低Android 7.0/API 24，compile/target SDK 35；面向ARM64手机。APK复用正式Rust/Dioxus界面与原生HTTP/WebSocket客户端。

工具链固定Dioxus CLI 0.7.10、JDK 17、Android SDK 35与NDK 28.2.13676358，CLI下载校验官方release SHA-256。`scripts/ci/build-android.ps1` 与直接 Cargo 检查共用 `scripts/ci/android-toolchain.ps1`：从显式 `-NdkPath`、`ANDROID_NDK_HOME`、`NDK_HOME` 或 SDK 中的固定版本解析 NDK，校验 host 工具并配置目标专属 CC/CXX/AR、API 24 flags 和 Cargo linker；PATH 与环境改动仅在本次执行中生效，成功/失败均恢复。已有显式 NDK 路径无效时明确失败，不偷偷回退另一份 NDK。只设置 `ANDROID_HOME` 不会让直接 Cargo 自动找到 `ring` 所需的交叉编译器。

`scripts/ci/build-android.ps1` 只从新的输出目录收集APK，并执行 `apksigner verify`，缺少APK或签名无效都会使任务失败。

Android 启动器图标使用 `assets/branding/rpi.svg`（与官方 Web favicon 相同）。运行 `node tools/dev/generate-brand-icons.cjs` 可离线生成五档普通 PNG 和带安全留白的自适应前景/背景资源，源哈希及资源哈希保存在 `assets/branding/android-icons.json`。Dioxus 0.7.10 会写入默认 Android 图标，不能只依赖 `bundle.icon`；构建脚本在重复生成前只清除上次覆盖的五档 launcher PNG，避免与 Dioxus 再次生成的 WebP 重名，再在最终 Gradle release 打包前调用 `scripts/ci/android-brand.ps1` 校验并覆盖这些资源。普通及圆形启动器引用同一官方图标。

专项检查：`tests/structure/android-brand.ps1` 验证覆盖、重复执行、非图标资源保留和损坏资源拒绝；`node tests/e2e/verify-brand-assets.cjs` 验证源 SVG 栅格一致性；`node tests/e2e/verify-android-brand.cjs <APK路径>` 在 SDK 35、Python 和仓库浏览器测试依赖可用时检查编译后 manifest、六个实际图像资源、自适应引用及 ARM64 ELF。最新专项记录位于 `artifacts/android-official-icon-*.json`；这些不代替实体手机安装和启动器显示验收。

脚本在Dioxus生成项目后修正递增versionCode，调用Gradle `assembleRelease`，再通过 `apksigner` 使用固定证书签名并验证。仓库Secrets已经配置Android证书和Ed25519更新签名密钥；私钥不进入源码。此前0.1.0临时开发签名包需卸载后安装固定签名版。发布、密钥备份与覆盖升级说明见 [应用更新](automatic-updates.md)。这不是应用商店发行配置。

Android 固定为手机客户端，入口为“连接我的电脑”。Android 原生入口与 Web 手机设置复用同一套连接面板、手动/扫码标签、密码表单和扫码控件，默认显示手动输入地址；也可切换到相机/相册扫码，核对目标后确认连接。成功认证后记住电脑地址，并将端点绑定的登录会话通过 Android Keystore AES-GCM 加密保存到 `noBackupFilesDir`，不会保存密码或扫码 ticket，也不把登录凭据写入 WebView localStorage；应用禁用备份。下次打开时，HTTPS 节点会自动恢复；HTTP 节点仅预填并提示未加密，需要用户确认当前是可信局域网、点击“恢复上次连接”后才发送已保存的登录凭据（有效时不需再输密码）。恢复时先用无 cookie 的独立连接复核已保存的节点身份，再检查服务端登录状态，确认有效后进入同一个工作台；身份无法确认时只保存地址，不降级为不绑定节点的登录。离线保留记录可重试，过期或明确拒绝会清除登录但保留地址。“忘记此电脑并退出”会清除本机保存状态并尝试退出远程登录；更换电脑不会停止节点任务。当前 desktop 登录最多有效 12 小时，宿主重启、共享关闭/变更或密码变更仍可能要求重新登录，客户端不会延长或绕过该限制。电脑使用本机节点，也可连接其他节点。由节点托管的 Web 登录仍固定使用当前页面的同源节点地址，只输入节点密码，不把固定地址登录变成任意远程节点连接；共用 UI 不表示共用网络实现或授权协议。清单声明网络和扫码相机权限，允许连接局域网 HTTP 节点；主题、资源与元 Agent 草稿使用应用 WebView 本地存储。真机 Keystore、进程重启恢复、摄像头授权和其他硬件行为仍需设备验证。

此前版本曾在支持 ARM 转换的 Android 14 x86_64 模拟器安装和启动；这是历史记录，不代表新版共享连接 UI 已进行设备验收。本轮共享 UI 包验证 ARM64 编译、签名、资源与浏览器/认证回归时没有附加 ADB 设备，未验证该新 APK 的实体手机安装、摄像头、系统输入法和跨设备 LAN 连通性。

## Web 构建

`scripts/ci/install-wasm-bindgen.ps1` 从 **Cargo.lock** 读取 wasm-bindgen 版本，安装一致的 CLI；`tools/dev/build-ui-web.ps1` 会再次验证版本，避免 Rust/WASM 与生成 JS 的协议不匹配。

Web 压缩包中：

- `standalone/`：静态 HTTP 资源；不能直接用 `file://` 打开。静态服务器本身不提供 Agent API。
- `rpi-web/`：可作为兼容 rpi-web 的 `--assets <目录>`，让页面与节点 API 同源。

Web 任务另跑 WASM 严格 Clippy。CI 不运行依赖本机浏览器、相邻工程和隔离节点的 UI 审查脚本；仓库中的198项UI验收记录是此前独立审查证据，不冒充本工作流的云端测试结果。

## 共享手机连接 UI 专项验证

以下为独立的测试构建，不修改正式应用入口，也不提供生产查询参数旁路：

```powershell
# 需要 wasm32 target 与匹配锁文件版本的 wasm-bindgen CLI。
pwsh -File tools/review/build-connection-panel.ps1
node tests/e2e/verify-connection-panel.cjs
```

测试专用二进制 `tests/harness/ui-modules/src/bin/connection-panel.rs` 通过 path 模块挂载正式共享连接组件；构建产物位于 `target/parity-connection-panel-web/`，中间编译产物也限定在 `target/` 下。脚本检查仓库与测试 harness 的 wasm-bindgen 锁定版本及 CLI 版本一致，不自动升级锁文件。浏览器测试使用仓库的 browser support，自行启动随机端口的隔离静态服务器，不依赖正式应用服务器；截图和报告写入 `target/connection-panel-review/`，仅在密码为空时截图。

专项覆盖 320/390/768 宽度、浅色/深色、手动默认页与扫码切换、标签、44px 点击区域、无横向溢出、忙碌/错误、取消和固定地址登录模式。表单提交、扫码结果与相机按钮在此 harness 中仅驱动合成回调，不调用节点 API，也不证明原生 HTTP、身份认证、真实摄像头或实体手机行为。`verify-device-flow.cjs` 另保留隔离节点的地址安全校验、相机取消竞态、地址草稿与 Enter 密码连接检查；这些浏览器模拟也不等于真机验收。构建或测试尚未通过时，不应把预期覆盖当作已验证结果。

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
pwsh -File tools/dev/build-ui-web.ps1
pwsh -File scripts/ci/package-build.ps1 -Target wasm32-unknown-unknown

# Android：先安装上述SDK/NDK、JDK 17和Dioxus CLI 0.7.10，
# 设置JAVA_HOME、ANDROID_HOME。无NDK override时自动选SDK/ndk/28.2.13676358。
rustup target add aarch64-linux-android
# 先检查client及mobile UI；-Offline适用于依赖已缓存的环境。
pwsh -File scripts/ci/check-android.ps1 -Offline
# 可选：指定另一套完整NDK或模拟器ABI，显式路径不会写入用户环境。
pwsh -File scripts/ci/check-android.ps1 -Target x86_64-linux-android -NdkPath '<NDK目录>'
# 编译并签名APK（与上面的cargo check是不同验收层级）。
pwsh -File scripts/ci/build-android.ps1
# 隔离工具链规则回归；-NativeProbe另使用已安装NDK编译C并链接两种ABI的Rust ELF。
pwsh -File tests/structure/android-toolchain.ps1 -NativeProbe
```

输出目录为 `target/ci-dist/`。所有二进制位置从 `cargo metadata` 取得，兼容自定义 `CARGO_TARGET_DIR`，不依赖作者电脑的 Cargo 路径。

配置依据：[GitHub runner 列表](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)、[Dioxus 0.7 平台依赖](https://dioxuslabs.com/learn/0.7/getting_started/)、[Dioxus Android](https://dioxuslabs.com/learn/0.7/guides/platforms/mobile/)、[wasm-bindgen CLI](https://wasm-bindgen.github.io/wasm-bindgen/reference/cli.html)。
