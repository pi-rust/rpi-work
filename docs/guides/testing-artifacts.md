# 测试源码与生成产物

源码仓库保留可复现的测试、fixture、原型与最终验收结论；截图、下载文件、逐次运行的JSON报告和本机诊断记录不再提交。

| 内容 | 存放位置 | 是否跟踪 |
| --- | --- | --- |
| Rust测试、浏览器测试、fixture、模块验证入口 | `apps/`、`crates/`、`tests/`、`scripts/` | 是 |
| 原型及其页面样例数据 | `design/prototype/` | 是 |
| 架构、功能设计、接口说明、最终验收摘要 | `docs/` | 是 |
| 原型与上游源码审查基准 | `tests/fixtures/baselines/prototype-review-baseline.json`、`tests/fixtures/baselines/upstream-review-baseline.json` | 是，属于审查输入 |
| 截图、对照图库、导出样例、运行结果JSON | `artifacts/`中的其他文件 | 否，由验证脚本生成 |
| 编译包、检查日志、checkpoint、临时工具 | `target/` | 否 |
| Agent会话和临时任务指令 | `.rpi/`、`docs/rpi-*.md` | 否 |

生成产物不随源码分发；新 clone 不自带本机截图、报告或构建证据。文档中的 `artifacts/...`、`target/...` 是运行输出位置，不代表仓库提供下载文件。整理文档或 Git 跟踪范围时，不应顺带删除正在使用的本机产物。

## 验证入口

当前复验须按所改模块运行下列入口，不能引用某次历史结果代替重跑。[归档索引](../archive/README.md)区分原型验收、视觉审阅、会话迁移及 Android 补验；其中的通过/失败只对应各次源码与环境，不是当前完整验收声明。

常用入口：

```powershell
pwsh -File scripts/ci/verify-rust-workspace.ps1
pwsh -File tools/dev/build-ui-web.ps1
# 每个套件遵循自身前置条件；先用 --list 查看，真实节点/模型验证须显式准备。
node tests/e2e/verify-ui-suite.cjs --list
# 已准备目标页面与相应隔离节点后运行所需套件。
node tests/e2e/verify-ui-suite.cjs
# Android client + mobile UI 的目标编译检查，与 APK/真机验收分别记录。
pwsh -File scripts/ci/check-android.ps1 -Offline
pwsh -File tests/structure/android-toolchain.ps1
$env:REVIEW_ALL_VIEWPORTS = '1'
node tools/review/audit-ui-pages.cjs
node tools/review/build-style-gallery.cjs
```

浏览器测试使用本仓库 `tools/package.json` 和锁文件，通过 `npm ci --prefix tools` 安装 Playwright。使用 `npm run install-browser --prefix tools` 安装 Chromium，或设置 `CHROME_PATH`。正式 suite 中的真实节点只读检查，以及历史审计所需的真实集成报告，仍需要对应隔离节点；普通单元测试和 fixture 测试不需要模型凭据。具体入口见 [项目结构](project-structure.md)。

验证后会在本机生成 `artifacts/ui-final-validation.json`、`artifacts/ui-pages-review-all.json`、`artifacts/ui-review-gallery.html` 等。真实模型检查为显式可选操作，不由普通CI自动调用。需要共享运行证据时另行归档为CI artifact或验收附件。

GitHub 编译流水线保留平台产物与 SHA-256，默认 14 天，见 [CI说明](ci-builds.md)。需要共享本机证据时单独上传为 CI artifact 或验收附件；归档文档与源码历史不代替原始运行日志。
