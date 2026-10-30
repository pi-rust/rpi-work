# 工作台主题系统 · v0.7

> 截图与运行报告为本地产物，不随Git分发；文中路径用于定位本地文件。保留规则与复验入口见 [测试产物说明](../guides/testing-artifacts.md)。

2026-10-24。把整体视觉设计抽象为「主题定义 → 语义变量 → 组件」三层，主题切换覆盖会话、工具、Skills、Package、队列、Provider、统计、变更、弹窗和手机布局。

## 正式应用视觉规格 v2

主题已进一步覆盖正文/标题/代码字体、角色字号、全局字体倍率、行高/字重、间距、五级圆角、控件/触控尺寸、边框、阴影和动效，仍保持旧 v1 主题及外观偏好兼容。使用方式、字段范围、优先级和维护入口见 [主题视觉规格指南](../guides/theme-design.md)，可直接导入 [Codex 阅读示例](../../examples/themes/codex-reading.json)。

当前通过 JSON 导入 → 实时预览 → 保存/取消 → 导出编辑，外观设置展示实际规格摘要；未提供逐字段可视化编辑器。所有数值和字体选择均受限，不开放任意 CSS 或外部字体。

## 正式应用当前默认：Codex 黑白

正式 Dioxus 应用默认使用 `codex`：白色主画布、浅灰侧栏与工具表面、黑色主操作、低饱和边框；深色使用石墨灰表面与浅色主操作。借鉴 Codex 的简洁中性色视觉，不是官方 CSS 或品牌标识的逐项复制，不引入外部字体、脚本或资源。成功、失败、警告及 diff 仍保留独立语义色。

默认明暗模式仍为浅色，密度仍为标准。**已有设备保存的有效主题不强制迁移**；如果此前保存了 Reddit 橙，请在「外观与主题」中选择「Codex 黑白」并保存。缺失或无效的主题 ID 回退到 Codex。原四套主题、完整主题 JSON 和自定义主题均保留。

Codex 的视觉默认值由 `ThemeDesign::codex()` 提供：消息 15px / 1.7 行高、导航 14px、标题 16px / 600 字重，圆角 4/8/12/16/20px、40px 控件、44px 手机触控目标及 subtle 阴影。输入正文至少 16px，输入区使用更柔和的圆角表面，发送按钮为圆形，键盘焦点继续清晰可见。旧 v1 Codex JSON 仍按原 ID/字段往返，其未显式描述的视觉参数跟随当前内置默认值；显式 v2 `design` 始终优先，其他四套主题不跟随这组调整。

[`codex-components.css`](../../apps/work-ui/src/shared/theme/codex-components.css) 仅在 Codex 下减轻顶部操作边框和侧栏徽章的视觉重量。手机会话标题可正确省略，不覆盖状态区域；360px 以下，低优先级的空闲/完成徽章只对辅助技术保留，给会话标题让出空间，运行/错误与节点状态仍可见。工具栏在窄屏水平滚动，保留完整可读的资源入口及触控尺寸，不再把文字挤成小字或多行。浅/深/390/320px 截图由 `tests/e2e/verify-codex-theme.cjs` 使用已选择会话的正式应用与本地 HTTP fixture 生成，路径为 `artifacts/codex-theme-{light,dark,mobile-390,mobile-320}.png`。

正式实现以 [`theme_contract.rs`](../../apps/work-ui/src/shared/theme/theme_contract.rs) 的 palette / `build_tokens` 为来源；[`codex.css`](../../apps/work-ui/src/shared/theme/codex.css) 是生成结果，包含启动 fallback 及 light/dark/system 规则，与 JSON 导入导出共用同一组 token。正式应用与模块 harness 加载同一套 stylesheet。

```shell
cargo run -p work-ui --example sync-codex-theme --locked
cargo run -p work-ui --example sync-codex-theme --locked -- --check
```

下文涉及 `design/prototype/` 的实现与原型截图属于历史设计参考，本轮未修改原型。

## 1. 可切换的主题

| 主题 | 风格 | 字体与轮廓 |
| --- | --- | --- |
| Codex 黑白 `codex`（正式应用默认） | 白色 / 石墨灰，黑白强调 | 系统无衬线、柔和圆角 |
| 纸与苔绿 `paper` | 暖白 / 深苔绿，赤陶强调色 | 人文无衬线、柔和圆角 |
| 海湾蓝 `ocean` | 冷白 / 深海蓝，海蓝强调色 | 系统无衬线、利落轮廓 |
| 石墨紫 `graphite` | 中性灰 / 深石墨，紫色强调 | 系统正文、等宽标题、方正轮廓 |
| Reddit 橙 `reddit` | 橙红强调、冷灰 / 炭黑表面 | 系统无衬线、柔和卡片 |

Reddit 橙参考 [Reddit 官方品牌页](https://redditinc.com/brand) 的 OrangeRed `#FF4500`，用于品牌图形；正文操作色在浅色中加深、深色中提亮，保证主要文本对比度。中性表面为本原型设计，并非对 Reddit 全站 CSS 的逐项复制。没有引入 Reddit 标识或远程字体。主题 JSON 可从外观设置导出。本地截图：浅色（本地生成：`artifacts/prototype-theme-reddit-light.png`）、深色（本地生成：`artifacts/prototype-theme-reddit-dark.png`） 和 手机（本地生成：`artifacts/prototype-theme-reddit-mobile.png`） 截图。

每套主题都提供浅色和深色配色。**主题风格、明暗模式、界面密度分别选择**：例如海湾蓝 + 跟随系统 + 紧凑，不需要维护成另一个独立主题。

入口在右上角主题按钮，以及个人设置中的「外观与主题」。手机保留图标入口。选择时即时预览，点击「完成」后保存；取消、Esc、关闭按钮或点击遮罩会恢复打开前的外观。

主题是本设备的应用偏好，跨本地会话使用。它不会写进会话的 Skills/Tools/提示词 Package，也不修改 Agent 配置。切换时保留正文、工具展开状态、输入草稿、队列与 Package 绑定；界面密度变化可能改变可见行数。

## 2. 主题定义

历史 HTML 原型源文件：[theme-catalog.js](../../design/prototype/theme-catalog.js)。正式应用的对应模型位于上文 Rust 文件。定义采用纯数据和纯函数，不依赖 Agent 或网络：

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
| `design/prototype/theme-catalog.js` | 内置主题、字体栈、变量解析、对比度计算 |
| `design/prototype/theme-tokens.css` | 从默认主题生成的 CSS 回退值 |
| `design/prototype/themes.js` | 偏好读取、系统明暗监听、预览/保存/回退、导入导出 |
| `design/prototype/themes.css` | 主题选择界面及共享组件外观约定 |
| 原有各组件 CSS | 布局与语义变量的使用 |

新增内置主题：

1. 在 `THEME_PRESETS` 增加唯一 ID、名称、字体、形状及两套 palette。
2. 运行 `node tools/dev/sync-theme-tokens.cjs`，生成默认 CSS 回退值。
3. 运行 `node tests/design/verify-themes.cjs`，检查对比度、预览与响应式布局；主题选择器自动读取目录。

`node tools/dev/sync-theme-tokens.cjs --check` 检查生成文件是否同步。`tools/review/archive/migrate-theme-tokens.py` 是本次旧 CSS 迁移脚本，正常增加主题或组件无需再次执行。

## 5. 偏好、启动与可移植性

主题脚本在 head 中先于组件渲染读取偏好，设置根节点变量、明暗标记和原生控件的 `color-scheme`，减少深色启动时闪白。无 JavaScript 时由生成的默认 CSS 提供基础样式。

本机偏好保存在 `rpi-work-appearance-v1`，兼容此前仅保存浅深色的 `rpi-work-prototype-theme`。跟随系统时监听系统变化，明确选择浅色或深色时保持所选模式。浏览器拒绝持久化时当前页面仍可应用主题，但不能承诺刷新后保留。

可以导出和重新导入 `kind: rpi-work-theme / schemaVersion: 1` 的 JSON，包含主题定义、首选明暗模式和密度。自定义主题保存后可在本浏览器重新打开；取消预览会丢弃本次未保存的导入。最多保存 12 个自定义主题。

导入只接受已知字体/形状、完整的浅深色配置和六位十六进制颜色，不加载任意 CSS、脚本、外部字体或 URL。主要正文与辅助文字对各表面的对比度，以及按钮文字/强调色对比度，要求至少 4.5:1。同一 ID 内容不同则要求新 ID，避免覆盖已有定义。

这是 **Theme JSON**，与上一版 **Agent Package JSON** 使用不同的 kind/schema。主题是视觉资源；Agent Package 管理运行时能力。未来如果做统一资源分发，应保留类型和应用范围，导入主题不意味着启用工具或提示词。

## 6. Dioxus 实现

正式应用已实现：

- `Theme` / `Palette`：可序列化的完整主题资源及浅深配色。
- `AppearanceSettings`：主题、明暗模式、密度；默认 Codex + light + standard。
- `build_tokens`：纯函数生成语义变量，用于主题 JSON、预览和 Codex 生成 CSS。
- `use_appearance` / `AppearancePicker`：根组件的外观状态、预览、取消、保存与导入导出；Web 使用浏览器偏好，原生使用本机配置路径。
- `apply_script`：设置根变量与语义属性，订阅系统明暗变化；切回内置主题会清除自定义 inline token。

主题状态放在应用外观层，和会话运行状态分别维护。预览使用临时配置，确认才提交偏好；不通过切换整个 ConversationView 的 key 来刷新外观。

## 7. 验证

正式应用 Codex 回归（先构建 WASM，再在另一终端启动 review server）：

```powershell
pwsh -File tools/dev/build-ui-web.ps1
node tools/review/ui-review-server.mjs
# 保持 server 运行，在另一终端执行：
node tests/e2e/verify-codex-theme.cjs
```

测试使用本地 HTTP 节点 fixture，不调用外部模型；覆盖默认外观、预览取消与草稿保留、保存/刷新、系统深浅切换、旧主题兼容、390/320px 布局与首屏 CSS fallback。Rust 测试另检查 JSON 往返、主要文本对比度至少 4.5:1、生成 CSS 与 token 一致。截图 `artifacts/codex-theme-light.png`、`codex-theme-dark.png`、`codex-theme-mobile.png` 和回归报告 `artifacts/codex-theme-validation.json` 为本地产物，不提交。

以下为历史 HTML 原型回归：

`node tests/design/verify-themes.cjs` 验证：生成文件一致、组件无固定 palette 色值、主要文字对比度、预览/取消/Esc/关闭恢复、对话和草稿节点保留、偏好保存、系统明暗响应、四主题 × 两模式在 375/390/768/1440px 的布局、Package 与 diff 主题覆盖、JSON 下载导入、自定义主题持久化、非法颜色和低对比度拒绝、旧偏好迁移，以及无外部请求。

原有六组浏览器交互回归也已通过。记录：`artifacts/themes-validation.json`。截图：石墨紫深色（本地生成：`artifacts/prototype-theme-graphite-dark.png`）、海湾蓝变更页（本地生成：`artifacts/prototype-theme-ocean-changes.png`）、手机主题选择（本地生成：`artifacts/prototype-theme-picker-mobile.png`）、手机海湾蓝（本地生成：`artifacts/prototype-theme-ocean-mobile.png`）。

自动对比度检查覆盖主要文本和关键表面，不能替代所有状态、视觉障碍场景与真机检查。
