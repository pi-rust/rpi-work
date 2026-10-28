# 界面样式验收

以 `prototype/` 中已确认的 HTML 为样式参考。原型文件保持原样；真实节点数据、下载后安装的资源状态，以及后续确认的连接流程与早期演示内容不同。

本轮统一了标题默认边距、输入区的 Skills / BTW / 常用能力 / 附件入口、上下文侧栏、资源卡片操作区、会话资源弹窗和手机设置导航。主题仍通过共享语义变量切换，不写死 Reddit 配色。

复验步骤：

```powershell
pwsh -File scripts/build-ui-web.ps1
# 保持 scripts/dev-web.mjs 的实际客户端服务运行在 4180。
node scripts/ui-review-server.mjs
# 在另一个终端运行：
$env:WORK_UI_URL = 'http://127.0.0.1:4180/'
$env:REVIEW_REFERENCE_URL = 'http://127.0.0.1:4182/prototype/'
$env:REVIEW_ALL_VIEWPORTS = '1'
node scripts/audit-ui-pages.cjs
node scripts/audit-ui-responsive-geometry.cjs
node scripts/build-style-gallery.cjs
```

打开 `http://127.0.0.1:4182/artifacts/style-restoration-gallery.html`，选择页面与屏幕宽度，可并排或叠加查看实现和原型。

页面审查覆盖 25 类页面和 5 种视口，共 117 组适用截图。手机手动地址表单也纳入审查；设备不支持的电脑二维码/手机连接页面，共 8 项标为不适用。审查会检查页面横向溢出和可见卡片、导航、弹窗、输入区内容裁切。另有 63 项主界面位置、尺寸和字号的严格相等检查。截图与构建记录关联，生成图册前会验证源码哈希，拒绝过期截图。

这些检查不能证明所有页面逐像素相同；仍需结合叠加截图检查细节，并保留真实节点、功能和 Android 安装包回归。

Android 安装后，还要在指定测试设备打开会话，运行键盘回归；脚本通过真实系统点击打开输入法，检查 WebView 缩小、输入框和发送按钮可见，以及收起键盘后恢复，不会发送消息：

```powershell
$env:ANDROID_SERIAL = 'emulator-5580' # 显式指定自己的测试设备
node scripts/verify-android-keyboard.cjs
```

Activity 使用 [`adjustResize`](https://developer.android.com/develop/ui/views/touch-and-input/keyboard-input/visibility) 在软键盘出现时调整窗口，避免默认平移模式遮挡底部输入区。
