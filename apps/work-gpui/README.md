# work-gpui — GPUI 桌面原型

验证「共享 view model + 第二个原生渲染器」是否成立的最小可运行工程。设计说明见
[`docs/design/gpui-desktop-prototype.md`](../../docs/design/gpui-desktop-prototype.md)。

```bash
# fixture（与 tests/fixtures/ui-fixture.cjs 同内容）
cargo run --manifest-path apps/work-gpui/Cargo.toml

# 只读连接真实节点（密码从环境变量读取）
RPI_WORK_PASSWORD=... cargo run --manifest-path apps/work-gpui/Cargo.toml -- --connect http://127.0.0.1:13031
```

## 为什么是独立 workspace

`gpui 0.2.2` 依赖 `cocoa = "=0.26.0"`，`dioxus-desktop 0.7` 要求 `cocoa ^0.26.1`。
`cocoa` 带 `links` 键，一个 workspace 只能有一个版本，因此本工程自带
`[workspace]` 与独立 `Cargo.lock`，不进入根 workspace（也就不进入 `cargo test --workspace`）。

## 结构

| 文件 | 作用 |
| --- | --- |
| `src/main.rs` | 应用启动、窗口、`--connect` 参数 |
| `src/data.rs` | `Snapshot`：fixture 数据，或 `work-client` 只读快照 |
| `src/shell.rs` | 视图：侧栏、转写、工具卡、输入区、底栏 |
| `src/theme.rs` | 把 `work-viewmodel::theme` 的 token 解析成 GPUI 颜色 |

渲染逻辑全部依赖 `work-viewmodel`：`transcript`（行序）、`conversation::tool_display`
（工具名/目标）、`conversation::code_tokens`（代码着色）、`observability::metrics`
（底栏指标）、`theme`（配色）。

## 构建前提

- macOS：默认 feature 需要 `xcrun metal`（完整 Xcode）。仅装 Command Line Tools 时
  本工程用 `runtime_shaders` 在运行时编译着色器（见 `Cargo.toml` 注释）。
- Linux：需要 X11/Wayland 与 Vulkan 开发包；本原型未在 Linux 上验证。

## 平台支持

| 平台 | 状态 |
| --- | --- |
| macOS | ✅ 本机验证（Metal；CLT-only 时用 `runtime_shaders`） |
| Linux | ⚠️ 需要 X11/Wayland + Vulkan 开发包，未在本机验证 |
| Windows | ⚠️ 未验证（gpui 有 DirectX 后端） |
| **iOS / Android** | ❌ **不支持**：`gpui 0.2.2` 只有 mac/linux/windows 平台后端，源码里没有触摸事件类型，也没有 Activity/UIView 生命周期接入 |

移动端继续走 `apps/work-ui`（Dioxus，Android）或 `design/reui-codex`（Web）；三条渲染器共用
`work-viewmodel` / `work-client` 的规则。

## 首帧自检

```bash
RPI_GPUI_TRACE=1 cargo run --manifest-path apps/work-gpui/Cargo.toml
```

输出示例（证明 view model 驱动了渲染）：

```
first frame: 4 rows (2 transcript + 2 tools), 2 projects, 4 sessions, metrics input=Some(32000.0) context=Some(38.0)
  row user "帮我为工作台增加一个深色主题，并保持手机端可用。"
  row assistant "主题切换已经准备好，接下来运行组件测试。"
  row tool read src/theme.rs
  row tool bash cargo test -p work-ui --lib
```
