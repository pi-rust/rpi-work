# 主题视觉规格（Theme Design v2）

主题不再只是一组颜色。正式应用将**配色 → 视觉规格 → 语义 token → 组件**分层，支持字体、字号、行高、字重、间距、圆角、尺寸、边框、阴影和动效。主题是当前设备的外观数据，不参与 Agent 配置、权限或插件执行。

## 使用方式

可直接导入的完整示例：[Codex 阅读](../../examples/themes/codex-reading.json)。

1. 在「外观与主题」点击「导入主题」，选择该 JSON。
2. 立即查看实际工作台和「主题视觉规格」摘要，而非只看配色缩略图。
3. 保存后在本设备持久化；取消、Esc 或关闭弹窗会恢复原主题及未保存的主题库。
4. 「导出主题 JSON」保留完整视觉规格；「导出偏好 JSON」只保存主题 ID、模式和密度。

修改示例的 `theme.id` 后可创建独立主题。相同 ID、不同内容会被拒绝，内置主题不能被覆盖。当前不提供逐字段可视化编辑器；编辑 JSON 后通过同一导入/预览/保存流程使用。字体不会下载，所选系统字体栈缺少某字体时由系统回退。

## 数据格式和兼容

- `kind: "rpi-work-theme"`，`schemaVersion: 1`：旧格式，仅有 `font` / `headingFont` / `shape` 和 palette；继续按原样导入、导出，不静默升级。
- `schemaVersion: 2`：必须包含完整 `theme.design`；`font` / `headingFont` / `shape` 仍是必填的旧兼容描述，但实际视觉值由 `design` 优先决定。
- v1 不能夹带 `design`，v2 不能缺少它，未知版本或未知字段均拒绝。
- 外观偏好 JSON 仍为 `rpi-work-appearance / 1`；浏览器持久化 key 仍为 `rpi-work-appearance-v1`。其中的自定义主题保留可选设计规格，旧偏好仍可恢复。
- 当前默认仍为 Codex 黑白 + 浅色 + 标准密度，已有有效主题选择不强制覆盖。

完整字段结构见示例，下面给出 `theme.design` 的规格。

### typography

| 字段 | 含义 / 单位 | 允许范围 |
| --- | --- | --- |
| `bodyFont` / `headingFont` / `codeFont` | 正文、标题、代码的字体栈 | `system`、`humanist`、`code`、`serif` |
| `scale` | 全局字号倍率 | 0.85–1.5 |
| `bodySize` | 正文基础字号，px | 整数 12–22 |
| `navSize` | 导航基础字号，px | 整数 11–20 |
| `assistSize` | 辅助文本基础字号，px | 整数 10–18 |
| `titleSize` | 工作台 / 弹窗标题基础字号，px | 整数 16–32 |
| `messageSize` | 对话消息基础字号，px | 整数 12–24 |
| `codeSize` | 对话代码基础字号，px | 整数 10–20 |
| `bodyLineHeight` / `messageLineHeight` | 无单位行高 | 1.3–2.2 |
| `bodyWeight` / `headingWeight` | 正文与核心标题字重 | 300–800，步长 50 |

`scale` 与角色字号相乘。其他已有组件的小字号经 `--type-scale` 统一缩放；独立业务语义强调（例如 Markdown strong）不强行变成正文或标题字重。

字体栈为宿主固定值：`system` 使用系统 UI 字体；`humanist` 优先本机 Inter；`code` 优先 Cascadia Code / SFMono / Consolas；`serif` 优先 Georgia / 本机中文衬线字体。不能填写任意 font-family 字符串或字体 URL。

### geometry

| 字段 | 含义 / 单位 | 允许范围 |
| --- | --- | --- |
| `spacingScale` | 语义间距倍率 | 0.75–1.5 |
| `radii` | xs / sm / md / lg / xl，px | 恰好五个递增或相等的整数，每个 0–32 |
| `controlHeight` | 导航、核心表单/按钮尺寸基准，px | 整数 32–56 |
| `hitSize` | 手机核心操作触控目标，px | 整数 44–64 |
| `borderWidth` | 宿主共享控件/分隔线边框，px | 整数 1–3 |

间距组合为 `space-N = Npx × density-factor × spacingScale`。标准密度的 `density-factor=1`，紧凑为 `.86`。紧凑模式在原规格基础上减少消息基础字号 1px、控件高度 5px，不改写导出主题里的 design。

圆形头像、胶囊、图标尺寸及页面布局宽度属于组件语义，不会因为圆角或间距倍率被任意替换。主题只改变已有语义空间 token 的消费位置，尚未把每一个布局数字都迁为主题字段。

### shadow / motion

- `shadow` 是受控枚举：`none`、`subtle`、`soft`、`lifted`，分别解析为宿主固定阴影；应用于共享弹窗及主要浮层 / composer 表面。焦点反馈独立保留，不能用“无阴影”关闭焦点指示。
- `motion.fastMs` / `normalMs` / `pressMs` 为 0–600 的整数毫秒，快速时长不能大于常规时长。作用于宿主核心按钮过渡、抽屉和弹窗动效。0 表示立即变化。
- 系统 `prefers-reduced-motion: reduce` 优先于主题，禁止主题重新开启动画、过渡或平滑滚动。

## 渲染和可访问性边界

1. 所有 design 字段经强类型反序列化和数值范围校验，再转换为 CSS 变量；不接受 CSS 文本、选择器、脚本、URL、事件或远程资源。
2. 手机输入字号至少 16px，主要触控目标至少 44px；主题缩放和紧凑模式不能降低安全下限。
3. 外观弹窗使用正常 flex 底栏，滚动正文和保存/取消操作独立布局，不用固定高度猜测大字号、大触控目标的占用空间。
4. palette 继续检查主要文本与表面对比度至少 4.5:1；未改变成功/失败/diff 的独立语义色。
5. 从自定义主题切回内置主题时清除完整旧 token 集，再应用新规格，不能残留字体、间距、边框或阴影。
6. 保留旧 `message-line-height` token 的标准/紧凑取值；实际消息使用 `reading-line-height`。v2 的两个 token 均来自 `messageLineHeight`。无显式 design 时，原四套主题的实际消息行高仍为 1.55，Codex 使用其专属默认值 1.7；旧 v1 Codex 文档未描述的视觉参数跟随当前内置默认值，显式 design 不受这组默认值覆盖。
7. 组件仍负责响应式布局、图标、业务状态及内容格式。主题不允许导入任意 CSS 来改变组件结构，也不承诺所有业务控件都能脱离布局限制无限放大。

## 开发入口

- [`theme_design.rs`](../../apps/work-ui/src/shared/theme/theme_design.rs)：强类型规格、范围、兼容默认值与规格 token；
- [`theme_contract.rs`](../../apps/work-ui/src/shared/theme/theme_contract.rs)：配色、v1/v2 主题文档和整体 token 解析；
- [`appearance.rs`](../../apps/work-ui/src/shared/theme/appearance.rs)：持久化、预览、取消、主题库和全量 token 应用；
- [`theme-design.css`](../../apps/work-ui/src/shared/theme/theme-design.css)：核心视觉角色与移动/动效安全规则。

修改规格默认值后重新生成 Codex CSS：

```shell
cargo run -p work-ui --example sync-codex-theme --locked
cargo run -p work-ui --example sync-codex-theme --locked -- --check
cargo test -p work-ui shared::theme --locked
```

重建正式 WASM 并启动 review server 后运行：

```powershell
pwsh -File tools/dev/build-ui-web.ps1
node tools/review/ui-review-server.mjs
# 保持 server 运行，在另一终端执行：
node tests/e2e/verify-theme-design.cjs
```

浏览器测试验证示例导入/原文数据往返、实际 computed style、非法规格原子拒绝、取消/保存/刷新、密度组合、切回内置无残留、390/320px 和 64px 大触控目标。节点为本地 HTTP fixture，不调用外部模型。报告和截图 `artifacts/theme-design-*` 是本地产物。

总体主题体系见 [主题设计](../design/themes.md)，后续修改须遵循 [开发规范](development-standards.md)。
