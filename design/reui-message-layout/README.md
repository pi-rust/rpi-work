# 消息区与布局 · HTML 交互评审稿

静态 HTML/CSS/JS，不依赖 React、WASM、节点或模型。只优化新 UI，不修改旧 `work-ui`。

## 预览与验证

```bash
# 在本工作树根目录运行（已有 4181 服务时不要重复启动）
python3 -m http.server 4181 --bind 127.0.0.1 --directory design/reui-message-layout

# 需要 tools/ 下已有 Playwright 及 Chromium
node design/reui-message-layout/verify.cjs
```

打开 `http://127.0.0.1:4181/`。刷新可看到最新原型；右上 **外观** 打开设置，默认不遮挡正文。

脚本截图默认输出到 `/tmp/rpi-message-layout/`；可通过 `RPI_LAYOUT_SHOTS` 指定目录。

## 这轮不再用简化壳子

| 区域 | 之前 | 现在 |
| --- | --- | --- |
| 侧栏 | 232px，项目和会话分离 | 264px，项目内嵌会话，搜索、新对话、本地节点、设置 |
| 顶栏 | 文本占位 | 44px，收起侧栏、会话标题、主题、外观、运行/文件 |
| 阅读列 | 单轮平铺 | 完整双轮，用户输入 → commentary → 活动 → 最终回答 |
| 工具调用 | 大卡片默认展开、暴露 JSON | 读取/搜索分组摘要；每个输出可独立展开 |
| 最终回答 | 单段文字 | 段落、列表、行内代码、代码块、文件变更摘要 |
| 输入区 | 不可编辑的占位文字 | textarea、模型/思考、附件文件名预览、演示发送、分支行 |
| 右栏 | 无 | 300px，运行状态 / 文件 diff，可关闭 |
| 设置 | 常驻浮层挡住正文，三个空开关 | 按需打开的模态窗口；所有选项生效并持久化 |

**代码块通栏**仅在空间足够时向两侧各延伸 24px，窄阅读区不突破正文。

## 对照依据与边界

### 本仓库的新 UI 源码

对照基线：`0369cf6`。

- `../reui-codex/src/components/workspace-sidebar.tsx`：264px 侧栏、项目/会话层级。
- `../reui-codex/src/App.tsx`：44px 顶栏、300px 右栏、28px 底栏、768px 消息列。
- `../reui-codex/src/components/transcript.tsx`：气泡、正文、可展开工具输出。
- `../reui-codex/src/components/composer.tsx`：固定输入区、模型/思考、发送/IME 交互。

以上是**外壳尺寸对齐**，不是宣称与 React 每个组件完全相同。本稿有意先预览紧凑活动、Markdown 和变更摘要，尚未迁回 React/GPUI。

### Codex 公开源码

本轮通过 GitHub Contents API 实际读取以下文件，核对消息类型和分组语义：

- [messages.rs](https://github.com/openai/codex/blob/cf3ff1530d821ec2d26452da4971d8e3e7d18d25/codex-rs/tui/src/history_cell/messages.rs)：用户消息、reasoning summary、agent message 分离。
- [activity_group.rs](https://github.com/openai/codex/blob/cf3ff1530d821ec2d26452da4971d8e3e7d18d25/codex-rs/tui/src/history_cell/activity_group.rs)：相邻活动分组，细节位置与调用顺序稳定。
- [patches.rs](https://github.com/openai/codex/blob/cf3ff1530d821ec2d26452da4971d8e3e7d18d25/codex-rs/tui/src/history_cell/patches.rs)：修改文件摘要和 diff 预览分开。

**这些是 TUI 渲染模块，不是桌面 App 的 CSS。** 阅读气泡的具体圆角、字体、activity 样式是本原型的候选设计，不应说成由 TUI 源码直接推导出的 App 像素规范。

[官方 App 介绍](https://openai.com/index/introducing-the-codex-app/)核实了按项目组织线程、线程内 review diff 等功能。页面主图是品牌图，不足以做桌面窗口叠图验证。本轮尚无同版本、同尺寸的 Codex App 目标截图，因此**没有完成可证明的像素级 1:1**。

后续要固定一组目标截图（阅读、工具展开、设置、文件面板），同窗口尺寸叠图校准，再将确认的 HTML 结构迁回新 UI；不要继续以“看起来像”作为 1:1 验收。

## 所有设置均可操作

- 阅读 / 经典布局：改变用户消息几何、角色标签、轮距。
- 轮次分隔：真正显示细线。
- 代码块通栏：足够宽时扩展代码块。
- 消息悬浮操作：复制 / 引用；键盘聚焦也可见。
- 紧凑活动视图：聚合摘要与逐条工具卡切换。
- 深色模式：覆盖壳子、正文、代码、diff、输入区、模态框。

设置存于独立的 `rpi-message-layout-review-v2` localStorage 键，不读写产品设置。
草稿仅保存在当前标签页的 sessionStorage。损坏或不可用的 storage 回退到内存默认值。

## 验收

`verify.cjs` 共 26 项检查，包括：

- 侧栏/顶栏/右栏/底栏精确尺寸；正文与输入框的左右边缘一致。
- 1280、1440、1680px 桌面窗口，右栏关闭/运行/文件三状态均无横向溢出。
- 所有设置的实际样式及刷新持久化；损坏设置安全回退。
- 活动/工具展开、工具输出复制、引用、文件 diff、搜索。
- 演示发送不调用模型，输入作为文本渲染；IME composition 不提交。
- 浏览器零 page error。

**这些是本原型的结构和交互测试，不是 Codex App 像素差异测试。** 示例 diff、模型、指标、会话列表均为演示数据；附件只显示文件名，不上传。会话列表未实现真实切换，点击会明确提示。无审批相关新增内容，无移动端开发。
