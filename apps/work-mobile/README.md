# RPI Work 移动端

Flutter 客户端面向 Android 和 iOS。手机通过一次性二维码连接现有 RPI Work 节点，项目、会话和任务继续由该节点管理。

## 连接和会话

1. 在节点所属的 macOS 电脑打开 GPUI 工作台，进入协作设置，开启「允许远程协作」，再依次选择「一次性授权」和「签发授权」。二维码中的节点地址须能从手机访问。
2. 在手机打开相机，或从系统相册选取二维码。核对界面显示的节点地址，再点「授权连接」。
3. 按项目筛选会话或搜索标题，也可以新建项目和会话。会话菜单提供重命名、置顶、归档和恢复。
4. 会话支持查看历史、发送消息与剪贴板附件、接收流式 Markdown 回复以及查看工具结果。停止和回答节点交互请求取决于当前会话发布的能力。

点输入栏的回形针打开「附件」面板，在这里查看或移除草稿附件。点「选择文件」打开系统文件选择器。长按输入框选择「粘贴」也可添加剪贴板中的图片或文件。粘贴纯文本会在输入框中插入文字。附件加入当前会话草稿，可逐个移除，随消息一起发送。支持 PNG/JPEG（最多 4 张，单张 1 MiB、合计 2 MiB，宽高不超过 2000 像素）和 UTF-8 文本文件（消息和文本附件合计 32 KiB）。图片发送需要会话支持有序内容块。

历史消息和实时消息沿用 GPUI 的附件显示规则。文本附件和项目文件引用保留在消息正文中，用户发送的图片显示为 `[图片附件]`，Markdown 图片显示「图片（未加载）：」和替代文字。点击回复中的 HTTP(S) 链接可在系统浏览器打开。

连接期间，客户端在内存中保留授权和各会话的输入草稿。关闭 App 或切换节点后重新扫码。会话离开前台时暂停订阅，恢复时重新读取快照并追赶事件；发送超时会保留草稿供核对。

支持 HTTPS 和局域网 HTTP。通过公网访问时使用 HTTPS。当前实现覆盖项目与对话主流程。旧版客户端的资源管理、系统自更新及高级会话配置尚未迁移。移动端使用独立应用标识 `com.pirust.rpiwork.mobile`，可与旧 Android 客户端并存。

## 本地构建

使用 Flutter 3.50.1 和 Dart 3.14。Android 构建使用 JDK 21、Android SDK 36 和 NDK 28.2.13676358。iOS 需安装完整 Xcode 和 iOS SDK，真机运行需要在 Xcode 配置签名。

```sh
cd apps/work-mobile
flutter pub get
flutter analyze
flutter test
flutter run -d DEVICE_ID
```

Android 调试 APK 可用 `flutter build apk --debug` 构建。用于分发的 Android 和 iOS 构建需要单独配置发布签名。

## 真实节点协议验证

从仓库根目录编译独立 fixture，再运行移动端测试。测试在临时目录启动 Rust 节点和协议 Agent，验证二维码授权、项目和会话读取、发送提示、实时回复、结算和断线后的事件补发。

```sh
cargo build --locked -p work-node-service --example desktop_fixture -p work-client --bin rpc-fixture
export WORK_MOBILE_NODE_FIXTURE="$PWD/target/debug/examples/desktop_fixture"
export WORK_MOBILE_RPC_FIXTURE="$PWD/target/debug/rpc-fixture"
cd apps/work-mobile
flutter test
```
