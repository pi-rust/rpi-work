# 测试源码与生成产物

源码仓库保留可复现的测试、fixture、原型与最终验收结论；截图、下载文件、逐次运行的JSON报告和本机诊断记录不再提交。

| 内容 | 存放位置 | 是否跟踪 |
| --- | --- | --- |
| Rust测试、浏览器测试、fixture、模块验证入口 | `apps/`、`crates/`、`scripts/` | 是 |
| 原型及其页面样例数据 | `prototype/` | 是 |
| 架构、功能设计、接口说明、最终验收摘要 | `docs/` | 是 |
| 原型与上游源码审查基准 | `artifacts/prototype-review-baseline.json`、`artifacts/upstream-review-baseline.json` | 是，属于审查输入 |
| 截图、对照图库、导出样例、运行结果JSON | `artifacts/`中的其他文件 | 否，由验证脚本生成 |
| 编译包、检查日志、checkpoint、临时工具 | `target/` | 否 |
| Agent会话和临时任务指令 | `.rpi/`、`docs/rpi-*.md` | 否 |

本次清理只从Git索引移除生成产物，现有本地文件保持原内容。正在浏览的 `artifacts/ui-review-gallery.html` 和配对截图仍可使用；新clone不会自带这些本机验收输出。历史文档提到的 `artifacts/...` 是本地输出路径，不代表仓库提供可下载文件。

## 验证入口

最终验收范围和实际限制见 [验收说明](ui-parity-acceptance.md)与[验收矩阵](ui-parity-matrix.md)。常用入口：

```powershell
pwsh -File scripts/verify-rust-workspace.ps1
pwsh -File scripts/build-ui-web.ps1
node scripts/verify-ui-suite.cjs
$env:REVIEW_ALL_VIEWPORTS = '1'
node scripts/audit-ui-pages.cjs
node scripts/finalize-ui-review.cjs
```

浏览器审查脚本目前面向原有Windows审查环境，使用Playwright/Chrome，部分脚本复用相邻工程的开发依赖。正式suite中的真实节点只读检查，以及最终审计所需的真实集成报告，也需要对应隔离节点；普通单元测试不需要模型凭据。详情见验收说明，不把上述命令当成全新clone上无前置条件的一键测试。

验证后会在本机生成 `artifacts/ui-final-validation.json`、`artifacts/ui-pages-review-all.json`、`artifacts/ui-review-gallery.html` 等。真实模型检查为显式可选操作，不由普通CI自动调用。需要共享运行证据时另行归档为CI artifact或验收附件。

GitHub编译流水线保留release压缩包与SHA-256，默认14天，见 [CI说明](ci-builds.md)。这次没有改写历史：旧提交中的生成文件仍可回溯；移出当前版本不等于从整个Git历史中清除。
