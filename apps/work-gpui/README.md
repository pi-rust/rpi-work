# RPI Work · GPUI 工作台

当前唯一维护的 UI 入口。布局参照 [`design/reui-message-layout/`](../../design/reui-message-layout/)，由 GPUI 原生渲染。

在仓库根目录运行：

```bash
# 原生布局样例，窗口标注「示例 · 未连接」
task desktop

# 认证后读取已运行的 Work 节点
# 如需登录，在环境或 .env.local 中设置 RPI_WORK_PASSWORD
task desktop -- --connect http://127.0.0.1:13031

# 无 Task 时可用 Cargo 直接启动
cargo run --manifest-path apps/work-gpui/Cargo.toml
```

项目列表、会话和消息通过 `work-client` 读取。会话切换使用共享 `SessionConnection` 打开首个快照，转写和主题复用 `work-viewmodel`。连接模式目前只读，草稿按会话保存在当前窗口。原生项目概览显示节点、项目和会话信息。实时更新、发送、右侧浏览器以及其他写入能力仍在迁移。

## 构建与平台

GPUI 0.2.2 与旧 Dioxus 的 `cocoa` 版本要求冲突，本应用使用独立 Cargo workspace。macOS 通过 `runtime_shaders` 在运行时编译着色器，可使用 Command Line Tools。Linux 和 Windows 的 GPUI 构建尚待验证。GPUI 0.2.2 的平台后端面向桌面。

```bash
cargo fmt --manifest-path apps/work-gpui/Cargo.toml -- --check
cargo test --manifest-path apps/work-gpui/Cargo.toml
cargo clippy --manifest-path apps/work-gpui/Cargo.toml --all-targets -- -D warnings
```
