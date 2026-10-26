# 工作台主题系统 · v0.7

2026-10-24。把整体视觉设计抽象为「主题定义 → 语义变量 → 组件」三层，主题切换覆盖会话、工具、Skills、Package、队列、Provider、统计、变更、弹窗和手机布局。

## 1. 可切换的主题

| 主题 | 风格 | 字体与轮廓 |
| --- | --- | --- |
| 纸与苔绿 `paper` | 暖白 / 深苔绿，赤陶强调色 | 人文无衬线、柔和圆角 |
| 海湾蓝 `ocean` | 冷白 / 深海蓝，海蓝强调色 | 系统无衬线、利落轮廓 |
| 石墨紫 `graphite` | 中性灰 / 深石墨，紫色强调 | 系统正文、等宽标题、方正轮廓 |
| Reddit 橙 `reddit` | 橙红强调、冷灰 / 炭黑表面 | 系统无衬线、柔和卡片 |

Reddit 橙参考 [Reddit 官方品牌页](https://redditinc.com/brand) 的 OrangeRed `#FF4500`，用于品牌图形；正文操作色在浅色中加深、深色中提亮，保证主要文本对比度。中性表面为本原型设计，并非对 Reddit 全站 CSS 的逐项复制。没有引入 Reddit 标识或远程字体。可下载 [主题 JSON](../artifacts/rpi-work-theme-reddit.json)，查看 [浅色](../artifacts/prototype-theme-reddit-light.png)、[深色](../artifacts/prototype-theme-reddit-dark.png) 和 [手机](../artifacts/prototype-theme-reddit-mobile.png) 截图。

每套主题都提供浅色和深色配色。**主题风格、明暗模式、界面密度分别选择**：例如海湾蓝 + 跟随系统 + 紧凑，不需要维护成另一个独立主题。

入口在右上角主题按钮，以及个人设置中的「外观与主题」。手机保留图标入口。选择时即时预览，点击「完成」后保存；取消、Esc、关闭按钮或点击遮罩会恢复打开前的外观。

主题是本设备的应用偏好，跨本地会话使用。它不会写进会话的 Skills/Tools/提示词 Package，也不修改 Agent 配置。切换时保留正文、工具展开状态、输入草稿、队列与 Package 绑定；界面密度变化可能改变可见行数。

## 2. 主题定义

源文件：[theme-catalog.js](../prototype/theme-catalog.js)。定义采用纯数据和纯函数，不依赖 Agent 或网络：

```text
ThemeDefinition
├─ id / name / description
├─ font / headingFont
├─ shape: soft | crisp | square
└─ palettes
   ├─ light
   └─ dark

AppearanceConfig
├─ theme: ThemeDefinition.id
├─ mode: light | dark | system
└─ density: standard | compact
```

每个 palette 有 `bg / side / panel / card / text / muted / line / code / accent / accentBg / selected`。主题还可以提供独立的 `brand` 色，和正文/按钮的 `accent` 分开。由 `buildThemeTokens` 生成边框、焦点、阴影、状态、diff、终端、提示消息、图表及其他组件变量。

字体使用系统字体栈，没有远程字体请求。新增主题可以选择字体栈和轮廓方案，不需要重写业务组件。密度统一影响常用间距、消息字号/行高和导航行高；移动端表单和主题操作保持适合触控的尺寸。

## 3. 语义变量约定

| 类别 | 示例 | 消费组件 |
| --- | --- | --- |
| 表面与文本 | `--bg / --side / --card / --body-text / --muted` | 页面、侧栏、消息、弹窗 |
| 强调与选中 | `--accent / --accent-bg / --accent-line / --selected-bg` | 主按钮、焦点、当前文件/会话 |
| 状态 | `--success / --warning / --danger / --info` 及对应背景/边框 | 工具、Ask、错误、结算 |
| 变更 | `--diff-add / --diff-add-bg / --diff-del / --diff-del-bg` | diff 行，新增/删除语义保持稳定 |
| 终端与提示 | `--terminal-* / --toast-*` | 日志、通知 |
| 分类图表 | `--chart-system / --chart-history / --chart-tools / --chart-files` | 上下文组成 |
| 字体 | `--sans / --mono / --font-heading / --font-message` | 正文、代码、标题、消息 |
| 形状 | `--radius-xs/sm/md/lg/xl/pill` | 卡片、输入框、按钮、徽标 |
| 节奏与动效 | `--space-* / --density-factor / --motion-*` | 常用间距、过渡 |

所有业务组件 CSS 中的固定十六进制颜色已经迁移到语义变量，并移除了旧的 `.theme-dark` 颜色覆盖规则。新增组件继续使用这些变量，避免为每个主题添加一份组件样式。

错误、删除等使用 danger；选中边框使用 accent；成功与新增使用 success。不要因为默认配色接近就混用，否则换到海蓝或紫色时会出现错误语义。对话里的代码示例和用户图片属于内容，保持其原始呈现。

## 4. 文件与维护

| 文件 | 职责 |
| --- | --- |
| `prototype/theme-catalog.js` | 内置主题、字体栈、变量解析、对比度计算 |
| `prototype/theme-tokens.css` | 从默认主题生成的 CSS 回退值 |
| `prototype/themes.js` | 偏好读取、系统明暗监听、预览/保存/回退、导入导出 |
| `prototype/themes.css` | 主题选择界面及共享组件外观约定 |
| 原有各组件 CSS | 布局与语义变量的使用 |

新增内置主题：

1. 在 `THEME_PRESETS` 增加唯一 ID、名称、字体、形状及两套 palette。
2. 运行 `node scripts/sync-theme-tokens.cjs`，生成默认 CSS 回退值。
3. 运行 `node scripts/verify-themes.cjs`，检查对比度、预览与响应式布局；主题选择器自动读取目录。

`node scripts/sync-theme-tokens.cjs --check` 检查生成文件是否同步。`scripts/migrate-theme-tokens.py` 是本次旧 CSS 迁移脚本，正常增加主题或组件无需再次执行。

## 5. 偏好、启动与可移植性

主题脚本在 head 中先于组件渲染读取偏好，设置根节点变量、明暗标记和原生控件的 `color-scheme`，减少深色启动时闪白。无 JavaScript 时由生成的默认 CSS 提供基础样式。

本机偏好保存在 `rpi-work-appearance-v1`，兼容此前仅保存浅深色的 `rpi-work-prototype-theme`。跟随系统时监听系统变化，明确选择浅色或深色时保持所选模式。浏览器拒绝持久化时当前页面仍可应用主题，但不能承诺刷新后保留。

可以导出和重新导入 `kind: rpi-work-theme / schemaVersion: 1` 的 JSON，包含主题定义、首选明暗模式和密度。自定义主题保存后可在本浏览器重新打开；取消预览会丢弃本次未保存的导入。最多保存 12 个自定义主题。

导入只接受已知字体/形状、完整的浅深色配置和六位十六进制颜色，不加载任意 CSS、脚本、外部字体或 URL。主要正文与辅助文字对各表面的对比度，以及按钮文字/强调色对比度，要求至少 4.5:1。同一 ID 内容不同则要求新 ID，避免覆盖已有定义。

这是 **Theme JSON**，与上一版 **Agent Package JSON** 使用不同的 kind/schema。主题是视觉资源；Agent Package 管理运行时能力。未来如果做统一资源分发，应保留类型和应用范围，导入主题不意味着启用工具或提示词。

## 6. Dioxus 承接方式

后续 Rust 实现可映射为：

- `ThemeDefinition`：可序列化的主题资源。
- `AppearanceConfig`：主题 ID、明暗模式、密度。
- `ResolvedThemeTokens`：纯函数解析的当前变量集合。
- `AppearanceStore`：Web 使用浏览器偏好存储，桌面/原生使用平台配置存储。
- `SystemAppearance`：平台明暗状态与变化订阅。
- `ThemeProvider`：Dioxus 根组件持有 signal/context，更新根变量和语义属性，组件共享 CSS。

主题状态放在应用外观层，和会话运行状态分别维护。预览使用临时配置，确认才提交偏好；不通过切换整个 ConversationView 的 key 来刷新外观。

这些类型是框架提案，当前未创建 Rust crate 或验证 Dioxus/WebView 构建。HTML 原型已实现真实浏览器主题切换、存储和文件导入导出。

## 7. 验证

`node scripts/verify-themes.cjs` 验证：生成文件一致、组件无固定 palette 色值、主要文字对比度、预览/取消/Esc/关闭恢复、对话和草稿节点保留、偏好保存、系统明暗响应、四主题 × 两模式在 375/390/768/1440px 的布局、Package 与 diff 主题覆盖、JSON 下载导入、自定义主题持久化、非法颜色和低对比度拒绝、旧偏好迁移，以及无外部请求。

原有六组浏览器交互回归也已通过。记录：[themes-validation.json](../artifacts/themes-validation.json)。截图：[石墨紫深色](../artifacts/prototype-theme-graphite-dark.png)、[海湾蓝变更页](../artifacts/prototype-theme-ocean-changes.png)、[手机主题选择](../artifacts/prototype-theme-picker-mobile.png)、[手机海湾蓝](../artifacts/prototype-theme-ocean-mobile.png)。

自动对比度检查覆盖主要文本和关键表面，不能替代所有状态、视觉障碍场景与真机检查。
