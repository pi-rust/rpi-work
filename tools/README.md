# 本地开发与审查工具

`package.json` 和 `package-lock.json` 仅管理浏览器开发依赖，正式 Rust 客户端不需要 Node.js。执行 `npm ci --prefix tools` 安装。

| 入口 | 用途 |
| --- | --- |
| `dev/build-ui-web.ps1` | 编译真实 WASM 客户端并记录源码与资源哈希 |
| `dev/dev-web.mjs` | 本地静态资源、API 代理与可选源码监听 |
| `dev/sync-theme-tokens.cjs` | 设计原型主题 token 生成与校验 |
| `review/ui-review-server.mjs` | 原型、模块及图册的只读预览 |
| `review/check-structure.cjs` | 本地引用、文档链接、资源边界及原型基准检查 |
| `review/audit-ui-pages.cjs` | 按设备能力生成适用页面截图和布局报告 |
| `review/audit-ui-responsive-geometry.cjs` | 主界面严格布局对照 |
| `review/build-style-gallery.cjs` | 从当前构建与报告生成对照图册 |
| `review/build-module-review.ps1` | 编译模块验证工程 |
| `review/archive/` | 历史诊断、固定批次验收和协作入口，按当时环境使用 |

平台构建与签名脚本在 `scripts/ci/`。使用步骤见 [项目结构](../docs/guides/project-structure.md)。
