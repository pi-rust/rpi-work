# 项目结构与依赖边界

```text
apps/
  work-ui/
    android/             Android 权限与安装适配
    src/
      main.rs            程序入口
      app/               启动、工作空间协调、连接与会话标识
      features/
        conversation/    消息、工具、输入区、队列与跟随滚动
        projects/        项目选择、文件预览与变更审阅
        resources/       Packages、Skills、Tools 与会话配置
        extensions/      Agent/UI 设计契约适配与受控静态 UI 预览
        meta_agent/      元 Agent 资源编写工作台
        settings/        连接、Provider、更新设置
        observability/   用量、上下文与会话追踪
      shared/            公共组件、状态辅助和主题
  work-cli/              节点诊断 CLI
crates/                  work-core、work-client、work-node、work-update
assets/vendor/           正式应用使用的第三方二维码库和许可证
design/prototype/        冻结的 HTML 原型和原型资源
tests/
  e2e/                   正式应用的集成回归
  design/                HTML 原型及设计对照测试
  modules/               独立 UI 模块的浏览器测试
  fixtures/              隔离 HTTP/SSE 节点与源码审查基准
  harness/ui-modules/    模块验证用 Rust 工程，不进入安装包
  support/               本仓库浏览器测试依赖解析
tools/
  dev/                   开发预览、Web 构建与主题生成
  review/                截图、布局审查、对照图册
    archive/             历史的一次性诊断与协作工具
scripts/ci/              平台打包、签名和 Rust 检查
docs/                    guides、architecture、design、archive、images、release-notes
artifacts/               忽略的本次截图与报告
target/                  忽略的编译、下载、日志与临时验证数据
.rpi/                    忽略的本机 Agent 会话
```

`work-core` 不依赖 UI 和具体传输；`work-client` 通过 `WorkspaceClient` 区分本机 Rust 工作空间与远程 HTTP/SSE，Agent 执行使用对应的 RPC/ACP/HTTP 客户端。本机桌面默认使用进程内 `rpi-extcore` 与 stdio RPC，不启动 HTTP 服务；`work-node` 负责本机运行时、数据目录锁和宿主生命周期。具体边界见 [transport 架构](../architecture/transport.md)，新增和修改代码须遵循 [开发规范](development-standards.md)。

正式应用通过 `assets/vendor/` 编译二维码资源，不再引用设计原型。原型自己的 vendor 快照独立保留，以便复现已确认的设计；基准哈希只调整目录键名，原型文件内容不变。

`main.rs` 只启动应用。`app/workspace.rs` 负责跨功能状态协调，纯辅助逻辑拆到 `identity.rs`、`history.rs`、`runtime.rs`，队列同步和旁问容器在 `background.rs`。文件格式预览与 diff 解析属于 `features/projects/`。

## 常用命令

在仓库根目录运行：

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
node tools/review/check-structure.cjs

# 浏览器开发依赖只安装到本仓库 tools/node_modules。
npm ci --prefix tools
npm run install-browser --prefix tools
# 或将 CHROME_PATH 指向已有 Chrome 的可执行文件。

pwsh -File tools/dev/build-ui-web.ps1
$env:PORT = '4180'
node tools/dev/dev-web.mjs
```

另一个终端启动图册和原型服务：

```powershell
node tools/review/ui-review-server.mjs
```

当前界面为 `http://127.0.0.1:4180/`；原型仍可通过 `http://127.0.0.1:4182/prototype/` 访问，旧的图册地址也保留。这个 URL 是兼容入口，源文件实际位于 `design/prototype/`。

```powershell
$env:WORK_UI_URL = 'http://127.0.0.1:4180/'
node tests/e2e/verify-device-flow.cjs
node tests/e2e/verify-node-lifecycle.cjs
node tests/e2e/verify-ui-conversation.cjs
node tests/e2e/verify-ui-suite.cjs --list
node tools/review/audit-ui-responsive-geometry.cjs
```

模块测试通过 `pwsh -File tools/review/build-module-review.ps1 -Feature all` 构建，使用独立的 `tests/harness/ui-modules/Cargo.lock`，不再复制测试源码到临时工程。真实模型验证仍需显式传入 `--run-model`，普通结构检查和浏览器 fixture 测试不会调用模型。

历史脚本和记录归档保留；它们描述当时的环境和验收条件，不构成当前版本的一键验收承诺。生成截图、下载包、模拟器及本机配置继续保留在本机，本次没有清空这些目录。
