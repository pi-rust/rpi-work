# 界面样式验收

以 `design/prototype/` 中已确认的 HTML 为样式参考。原型文件保持原样；真实节点数据、下载后安装的资源状态，以及后续确认的连接流程与早期演示内容不同。

本轮统一了标题默认边距、输入区的 Skills / BTW / 常用能力 / 附件入口、上下文侧栏、资源卡片操作区、会话资源弹窗和手机设置导航。主题仍通过共享语义变量切换，不写死 Reddit 配色。

复验步骤：

```powershell
pwsh -File tools/dev/build-ui-web.ps1
# 保持 tools/dev/dev-web.mjs 的实际客户端服务运行在 14180。
node tools/review/ui-review-server.mjs
# 在另一个终端运行：
$env:WORK_UI_URL = 'http://127.0.0.1:14180/'
$env:REVIEW_REFERENCE_URL = 'http://127.0.0.1:14182/prototype/'
$env:REVIEW_ALL_VIEWPORTS = '1'
node tools/review/audit-ui-pages.cjs
node tools/review/audit-ui-responsive-geometry.cjs
node tools/review/build-style-gallery.cjs
```

打开 `http://127.0.0.1:14182/artifacts/style-restoration-gallery.html`，选择页面与屏幕宽度，可并排或叠加查看实现和原型。

页面审查覆盖 25 类页面和 5 种视口，适用截图数量以当次报告为准。手机手动地址表单也纳入审查；设备不支持的电脑二维码/手机连接页面和手机端不提供的 Provider 配置标为不适用，并记录原因。审查会检查页面横向溢出和可见卡片、导航、弹窗、输入区内容裁切。另有 63 项主界面位置、尺寸和字号的严格相等检查。截图与构建记录关联，生成图册前会验证源码哈希，拒绝过期截图。

这些检查不能证明所有页面逐像素相同；仍需结合叠加截图检查细节，并保留真实节点、功能和 Android 安装包回归。

Android 安装后，还要在指定测试设备打开会话，运行键盘回归；脚本通过真实系统点击打开输入法，检查 WebView 缩小、输入框和发送按钮可见，以及收起键盘后恢复，不会发送消息：

```powershell
$env:ANDROID_SERIAL = 'emulator-5580' # 显式指定自己的测试设备
node tests/e2e/verify-android-keyboard.cjs
```

Activity 使用 [`adjustResize`](https://developer.android.com/develop/ui/views/touch-and-input/keyboard-input/visibility) 在软键盘出现时调整窗口，避免默认平移模式遮挡底部输入区。

## 图标与实际 Web 参照对齐

图标以实际 `rpi-web/frontend`（Lucide React **0.468.0**）为参照，而不是旧原型的手绘近似。仅只读参照目录，不更改上游或依赖缓存。

- 功能图标映射在 `assets/icons/lucide-map.json`，当前 41 个语义名；`lucide-catalog.json` 保存 SVG primitives、许可版本及源模块哈希。`shared/icons.rs` 是生成的编译期静态 allowlist，不接受用户 SVG，也不产生运行时 npm/网络依赖。未知名称回退为 `circle-help`。
- 图形使用 `currentColor`、24×24 viewBox、Lucide 默认 2px stroke；角色尺寸为 12/14/16px，保留已有助手头像、工具行及手机发送按钮的上下文尺寸。图标与触控区分开计算，手机菜单、附件和发送仍至少 44px。并非全应用尺寸与 Web 逐像素一致。
- 助手头像使用 `code-xml`；附件入口使用 `paperclip`；关闭、侧栏、设置、工具操作等统一取自同一目录。工作区品牌标记采用 Web 的 25px 角色尺寸，保留原有品牌栏高度。
- 品牌源只有 `assets/branding/rpi.svg`，与 Web 的 `public/favicon.svg` 相同。离线生成 PNG、多分辨率 ICO 和 ICNS；Windows exe resource 与窗口配置显式使用该品牌，Dioxus bundle 配置及 macOS 手工包引用相应资源。

重新生成与检查（生成器参数必须指向已安装的本地 pinned 包，不会自动安装依赖）：

```powershell
node tools/dev/generate-ui-icons.cjs ../rpi-package/packages/rpi-web/frontend/node_modules/lucide-react --check
# 需要重新生成时去掉 --check；品牌资源变更后运行：
node tools/dev/generate-brand-icons.cjs
node tests/e2e/verify-brand-assets.cjs
& ./tools/dev/build-ui-web.ps1 -DebugBuild
node tests/support/run-static-ui.cjs tests/e2e/verify-icon-alignment.cjs tests/e2e/verify-brand-icon.cjs
& ./tests/support/verify-native-brand.ps1 -Executable ./target/debug/work-ui.exe
```

证据汇总为 `artifacts/icon-alignment-checks.json`。浏览器检查覆盖正式 WASM、隔离 fixture 下桌面/320/390px、明暗主题、手机侧栏、外观弹窗、精确 SVG 节点、正方形比例、按钮中心线、accessible name 和保留草稿；工具展示、消息可读性、手机输入区及标题栏另有回归。

**验收边界**：Windows exe 的 shell 图标颜色采样及 13 个 PNG/ICO/ICNS frame 与源 SVG 栅格一致性已通过；原生测试未取得本轮拥有窗口的句柄，实际窗口/任务栏显示仍未验收。macOS 安装包、Android 新包及实体设备本轮未验收，不能由资源正确或 Web 回归通过推断安装后显示正确。严格 native/WASM UI clippy 仍分别被 7/8 项既有 lint 阻塞，node clippy 通过。
