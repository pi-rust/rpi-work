# 测试源码与生成产物

源码仓库保留可复现的测试、fixture、原型与最终验收结论；截图、下载文件、逐次运行的JSON报告和本机诊断记录不再提交。

| 内容 | 存放位置 | 是否跟踪 |
| --- | --- | --- |
| Rust测试、浏览器测试、fixture、模块验证入口 | `apps/`、`crates/`、`scripts/` | 是 |
| 原型及其页面样例数据 | `design/prototype/` | 是 |
| 架构、功能设计、接口说明、最终验收摘要 | `docs/` | 是 |
| 原型与上游源码审查基准 | `tests/fixtures/baselines/prototype-review-baseline.json`、`tests/fixtures/baselines/upstream-review-baseline.json` | 是，属于审查输入 |
| 截图、对照图库、导出样例、运行结果JSON | `artifacts/`中的其他文件 | 否，由验证脚本生成 |
| 编译包、检查日志、checkpoint、临时工具 | `target/` | 否 |
| Agent会话和临时任务指令 | `.rpi/`、`docs/rpi-*.md` | 否 |

本次清理只从Git索引移除生成产物，现有本地文件保持原内容。正在浏览的 `artifacts/ui-review-gallery.html` 和配对截图仍可使用；新clone不会自带这些本机验收输出。历史文档提到的 `artifacts/...` 是本地输出路径，不代表仓库提供可下载文件。

## 验证入口

最终验收范围和实际限制见 [验收说明](../archive/ui-parity-acceptance.md)与[验收矩阵](../archive/ui-parity-matrix.md)。常用入口：

```powershell
pwsh -File scripts/ci/verify-rust-workspace.ps1
pwsh -File tools/dev/build-ui-web.ps1
node tests/e2e/verify-ui-suite.cjs
$env:REVIEW_ALL_VIEWPORTS = '1'
node tools/review/audit-ui-pages.cjs
node tools/review/build-style-gallery.cjs
```

浏览器测试使用本仓库 `tools/package.json` 和锁文件，通过 `npm ci --prefix tools` 安装 Playwright。使用 `npm run install-browser --prefix tools` 安装 Chromium，或设置 `CHROME_PATH`。正式 suite 中的真实节点只读检查，以及历史审计所需的真实集成报告，仍需要对应隔离节点；普通单元测试和 fixture 测试不需要模型凭据。具体入口见 [项目结构](project-structure.md)。

验证后会在本机生成 `artifacts/ui-final-validation.json`、`artifacts/ui-pages-review-all.json`、`artifacts/ui-review-gallery.html` 等。真实模型检查为显式可选操作，不由普通CI自动调用。需要共享运行证据时另行归档为CI artifact或验收附件。

GitHub编译流水线保留release压缩包与SHA-256，默认14天，见 [CI说明](ci-builds.md)。这次没有改写历史：旧提交中的生成文件仍可回溯；移出当前版本不等于从整个Git历史中清除。
