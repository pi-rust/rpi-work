# 第三方组件与许可证

本原型把第三方组件源码放进仓库（shadcn 的 copy-and-own 模式），因此按其许可证要求保留版权与许可声明。

| 来源 | 版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| [ReUI](https://github.com/keenthemes/reui)（Keenthemes Inc） | registry `base-nova` | **MIT**（Copyright © 2025 Keenthemes Inc） | `src/components/reui/`：`code-block`、`timeline` 及其依赖 |
| [shadcn/ui](https://github.com/shadcn-ui/ui) | CLI v4 / preset `nova` | MIT | `src/components/ui/` 基元、`components.json` 与主题 token |
| [Base UI](https://base-ui.com)（`@base-ui/react`） | 1.x | MIT | `base-nova` 样式的交互原语 |
| [shiki](https://shiki.style) | 4.x | MIT | `code-block` 的语法着色 |
| [Geist Variable](https://vercel.com/font)（`@fontsource-variable/geist`） | 5.x | SIL Open Font License 1.1 | 界面字体 |

## ReUI 的免费/付费边界

- **开源仓库与 `reui.io` 组件目录（1,264 个免费示例、27 个自研组件）**：MIT，可商用、可修改、可再分发，需保留版权声明（本文件与上游 `LICENSE.md` 已覆盖）。
- **ReUI Pro**（付费 blocks / icons / templates）：独立的一次性商业授权，安装时需要 `REUI_LICENSE_KEY`，**不在 MIT 范围内**。本原型只使用免费 registry 项（`@reui/code-block`、`@reui/timeline` 等），没有引入任何 Pro 内容。

若未来引入 Pro 资产，必须在 `design/reui-codex/` 下单独记录授权凭据的获取方式，并且不要把 Pro 源码提交到本仓库。
