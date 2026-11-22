# GPUI 桌面原型

> 状态：原型验证中（`apps/work-gpui`，独立 workspace）。目标不是替换现有 Dioxus 客户端，而是验证「共享 view model + 第二个原生渲染器」这条路是否成立，并量化剩余工作量。

## 1. 目标与非目标

**目标**

- 证明 `work-client` / `work-viewmodel` 可以被一个**完全没有 Dioxus、没有 DOM、没有 CSS**的渲染器直接驱动。
- 用 GPUI 渲染出与 Web/桌面版同构的工作台骨架：标题栏、项目/会话侧栏、转写区（含工具卡）、输入区、运行统计底栏。
- 把「需要重写什么」写清楚，作为后续排期依据。

**非目标**

- 不追求功能对等（扩展面板、内嵌浏览器、HTML 预览、相机扫码都不在原型内）。
- 不替换 `apps/work-ui`；Android 与 Web 仍在 Dioxus 上。
- 不进入产品发布链路（不打进 `work-ui` 安装包，不接签名更新）。

## 2. 为什么是独立 workspace

`gpui 0.2.2` 依赖 `cocoa = "=0.26.0"`，而 Dioxus 桌面栈的 `dioxus-desktop 0.7` 要求 `cocoa ^0.26.1`。`cocoa` 带 `links` 键，同一 workspace 只能存在一个版本，因此两者**无法共存于根 workspace**：

```text
error: failed to select a version for `cocoa`.
    ... required by package `dioxus-desktop v0.7.0`
    ... previously selected package `cocoa v0.26.0` (gpui v0.2.2)
```

所以原型自带 `[workspace]` 表，用独立的 `Cargo.lock` 构建，与 `tests/harness/ui-modules` 的做法一致。代价是它不进入 `cargo test --workspace`，需要在 CI 里单独加一个 job（原型阶段先本地验证）。

## 3. 分层

```text
apps/work-gpui              渲染层（本原型新增）
  ├─ shell.rs               根视图：窗口、布局、焦点
  ├─ store.rs               客户端状态：选中项、草稿、展开状态
  └─ source.rs              数据来源：fixture / work-client
crates/client/work-viewmodel 展示规则（无 GUI 依赖，7.9k 行 / 129 测试）
  ├─ transcript             转写槽位 → 行序、历史/投影归属
  ├─ connection             地址规范化、密码草稿、阶段机、记忆连接
  ├─ conversation::*        工具摘要、代码词法器、历史分页
  ├─ theme                  主题 token 与设计规格
  └─ observability          metrics / context / footer provider
crates/client/work-client    本机/远程 adapter、HTTP/WS、快照与重放
crates/node/work-node-runtime 唯一 owner
```

渲染器只依赖 view model + client；不依赖 `work-ui` 的任何模块。

## 4. 已落地的原型

`apps/work-gpui`（独立 workspace，见 §2）当前状态：

| 能力 | 状态 |
| --- | --- |
| fixture 渲染完整骨架（侧栏 / 转写 / 工具卡展开 / 输入区 / 底栏） | ✅ 可运行 |
| 主题 token 来自 `work-viewmodel::theme`（Codex light） | ✅ |
| 转写行序来自 `TranscriptOrder`（历史 + 投影合并） | ✅ |
| 工具名/目标来自 `tool_display`，只读结果用 `code_tokens` 着色 | ✅ |
| 底栏指标来自 `observability::metrics`（↑32k / 上下文 38% 与 Web 版一致） | ✅ |
| `--connect <origin>`：`work-client` 只读快照（项目/会话） | ✅ 骨架 |
| 实时事件、写操作、会话历史加载 | ⏳ 未实现 |
| 视觉/交互验收 | ⏳ 本机缺少屏幕录制权限，仅验证首帧 trace |

首帧自检输出（`RPI_GPUI_TRACE=1`）：

```text
first frame: 4 rows (2 transcript + 2 tools), 2 projects, 4 sessions, metrics input=Some(32000.0) context=Some(38.0)
  row user "帮我为工作台增加一个深色主题，并保持手机端可用。"
  row assistant "主题切换已经准备好，接下来运行组件测试。"
  row tool read src/theme.rs
  row tool bash cargo test -p work-ui --lib
```

### 构建前提（实测）

- `gpui 0.2.2` 的 `build.rs` 调用 `xcrun metal`；本机只有 Command Line Tools（无 Xcode）时
  构建失败：`metal shader compilation failed: unable to find utility "metal"`。
- 用 gpui 的 `runtime_shaders` feature 可以在没有 Metal 工具链的 macOS 上完成构建并启动，
  代价是把着色器编译推迟到运行时；发布构建应改回默认 feature + 完整 Xcode。

## 5. 原型范围（分阶段）

| 阶段 | 内容 | 本原型 |
| --- | --- | --- |
| v0 | fixture 数据渲染完整骨架：侧栏、转写、工具卡、输入区、底栏统计 | ✅ 本次 |
| v1 | `--connect <origin>`：work-client 只读快照（项目/会话/历史） | ⏳ 骨架留位 |
| v2 | 实时事件：`work-client` 事件流 → view model 行序 → 增量渲染 | 后续 |
| v3 | 写操作：发消息、中止、队列、模型切换 | 后续 |
| v4 | 主题切换 / 密度 / 窄窗口布局 | 后续 |

## 6. 复用清单

| 资产 | 行数 | 原型中的用法 |
| --- | --- | --- |
| `work-viewmodel::transcript` | 624 | 行序与工具调用归属 |
| `work-viewmodel::conversation::tool_display` | 695 | 工具名/目标/预览/图标键 |
| `work-viewmodel::conversation::code_tokens` | 927 | 代码着色（原型先接 tokenize，渲染 span） |
| `work-viewmodel::conversation::history_window` | 242 | 历史分页窗口 |
| `work-viewmodel::connection` | 2,151（含 auth / memory） | 阶段文案、地址规范化、记住连接 |
| `work-viewmodel::observability` | 1,082 | 底栏指标与格式 |
| `work-viewmodel::theme` | 1,220 | 主题 token（`build_tokens`） |
| `work-client` | 17.5k | v1 起的唯一数据来源 |

## 7. 必须重写的部分（对照 `apps/work-ui`）

| 模块 | 行数 | 重写方式 |
| --- | --- | --- |
| `app/workspace.rs` | 6,308 | 拆成 GPUI 视图树：header / sidebar / transcript / composer / inspector |
| `features/conversation/*` | 11.9k | markdown 富文本、工具卡、时间线、滚动锚定全部重写为 GPUI 元素 |
| `features/resources/*` | 5.9k | 列表/表格重绘，无 HTML |
| `features/settings/*` | 4.5k | 表单控件（GPUI 无原生 `<input>`，需自建 TextInput） |
| `features/projects/*` | 3.5k | 文件树、diff 视图 |
| `features/observability/*` | 4.5k | 轨迹/用量面板 |
| CSS | 9,775（200 个 `@media`） | 全部改成 Rust 侧样式常量与断点分支 |
| JS 桥 | 15 个文件 / 164 处 `document::eval` | 改为原生实现或直接砍掉 |
| iframe 面板 | 扩展面板、HTML 预览、内嵌浏览器 | GPUI 无 WebView 合成能力，需要独立窗口或产品降级 |

## 8. 渲染映射

| view model | GPUI |
| --- | --- |
| `TranscriptOrder::ordered(...)` | `Vec<Row>` → `uniform_list` 或纵向 flex |
| `tool_display::icon/target/edit_preview` | 图标键 → svg，文本进 `Text` |
| `code_tokens::tokenize` | token → `TextRun`（前景色/字重） |
| `observability::metrics::{Metrics, Context}` | 底栏 `div` 三段 + tooltip |
| `theme::theme_contract::build_tokens` | `Rgba`/`Hsla` 调色板 |
| `connection::Phase` | 标题栏状态点与文案 |

## 9. 成本估算

- 原型（本次）：2–3 天，验证可行性与 API 匹配度。
- 可用桌面版（无扩展面板/内嵌浏览器）：2–3 人 × 2–3 月。
- 功能对等（含扩展面板、内嵌浏览器、mermaid、扫码）：3–6 人月，且需要产品决策处理 WebView 能力缺口。
- 长期成本：与 `apps/work-ui` **双份维护**，除非移动端也迁出 Dioxus（GPUI 无 Android 支持，短期不可能）。

## 10. 平台支持（实测）

- macOS：可用（Metal；仅 Command Line Tools 时用 `runtime_shaders`）。
- Linux / Windows：gpui 有后端，本机未验证。
- **iOS / Android：不支持**。`gpui 0.2.2` 的 `src/platform/` 只有 `mac` / `linux` / `windows`，
  源码里没有触摸事件类型（`TouchEvent` 0 命中），也没有移动端生命周期接入。
  移动端继续由 `apps/work-ui`（Dioxus/Android）或 `design/reui-codex`（Web）承接。

## 11. 风险

| 风险 | 说明 |
| --- | --- |
| API 不稳定 | gpui 尚未 1.0，Zed 内部 API 变动频繁，升级需跟改 |
| Linux 支持 | 依赖 X11/Wayland + vulkan，CI 需要额外的系统依赖 |
| 文本能力 | 富文本选择、IME、无缩放字体排版需自行搭建 |
| WebView 缺口 | 扩展面板协议（`index.html` + `bridge.js`）无法在窗口内合成 |
| 无障碍 | GPUI 无障碍支持弱于 DOM |

## 12. 验证

- `cargo build --manifest-path apps/work-gpui/Cargo.toml`
- `cargo run --manifest-path apps/work-gpui/Cargo.toml`：窗口出现并渲染 fixture 工作台
- 与 Web 版对照同一组 fixture 数据（`tests/fixtures/ui-fixture.cjs` 的会话/工具/指标）肉眼比对
- 原型不进入 `cargo test --workspace`；结构检查 `node tools/review/check-structure.cjs` 仍覆盖仓库边界
