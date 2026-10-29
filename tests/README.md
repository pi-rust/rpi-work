# 测试入口

所有命令从仓库根目录运行，开发依赖在 `tools/` 安装，不依赖相邻项目。

```powershell
npm ci --prefix tools
npm run install-browser --prefix tools
# 已有 Chrome 时也可以设置 CHROME_PATH。

cargo test --workspace --locked
node tools/review/check-structure.cjs
```

正式应用的测试先通过 `tools/dev/build-ui-web.ps1` 构建，并运行 `tools/dev/dev-web.mjs`。然后在另一个终端：

```powershell
$env:WORK_UI_URL = 'http://127.0.0.1:4180/'
node tests/e2e/verify-device-flow.cjs
node tests/e2e/verify-node-lifecycle.cjs
node tests/e2e/verify-ui-auth-scope.cjs
node tests/e2e/verify-ui-conversation.cjs
node tests/e2e/verify-new-conversation.cjs
node tests/e2e/verify-workspace-management.cjs
node tests/e2e/verify-sidebar-projects.cjs
node tests/e2e/verify-reply-follow.cjs
node tests/e2e/verify-ui-provider-cards.cjs
node tests/e2e/verify-ui-resource-cleanup.cjs
node tests/e2e/verify-ui-suite.cjs --list
```

- `e2e/`：正式应用与隔离节点的集成检查；名字含 `real` / `live` 的检查有额外环境要求。
- `design/`：冻结 HTML 原型的行为及设计对照检查。
- `modules/`：模块验证入口；部分脚本也可通过 `WORK_UI_URL` 检查正式应用。
- `fixtures/`：测试节点、样例与只读审查基准，不参与产品运行。
- `harness/ui-modules/`：独立 Rust 验证工程，使用自己的 Cargo.lock。
- `support/`：统一的 Playwright 依赖解析。

完整历史 suite 包含真实节点只读检查，需要预先配置隔离节点。真实模型检查必须显式传入 `--run-model`；常规 fixture 测试不会调用模型或向现有会话发送消息。

`verify-ui-provider-cards.cjs` 检查供应商所属模型、直接切换、凭据状态及无模型空状态；`verify-ui-resource-cleanup.cjs` 检查空资源库启动和旧样例缓存迁移，确保保留用户资源与会话配置。测试资源由 fixture 或显式导入提供，不依赖正式 UI 内置样例。

`verify-new-conversation.cjs` 检查项目右侧直接创建会话、当前项目继承、输入框聚焦、重复点击、失败重试、快速切换项目、草稿保留、首条消息自动命名及 390/320px 移动端操作。

`verify-sidebar-projects.cjs` 是 Codex 风格侧栏专项检查：1440/390/320px 下默认五条最近会话、置顶优先、显示更多/收起及剩余数量、较旧选中会话额外保留、`pi-rust` 展开/折叠的 reload 持久化、独立项目滚动与固定顶部新建/底部设置、idle 状态隐藏、hover/键盘 focus 菜单及项目新建按钮可操作。脚本通过隔离 fixture API 创建并命名 24 条会话、置顶与归档种子会话，仅拦截 GET 列表固定排序时间，不断言特定相对时间文案、不发送消息、不修改 fixture 文件。结果写入 `artifacts/sidebar-projects-validation.json`，各尺寸截图为 `artifacts/sidebar-projects-{width}.png`，失败时附加 `artifacts/sidebar-projects-failure.png`。
