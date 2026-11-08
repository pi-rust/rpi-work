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
crates/
  foundation/
    work-node-api/       节点 DTO、NodeApi、会话/原生资源/realtime 契约
    work-core/           重导出 API、事件规范化与运行投影、惰性扩展设计
  node/
    work-node-runtime/   实际 workspace owner、RPC/ACP、资源及源码服务
    work-node/           runtime 兼容导出
  transport/
    work-http-runtime/   认证、Host/Origin、共享与一次性授权
    work-node-service/   同 owner 的 HTTP/SSE/WebSocket 和静态 Web 托管
    work-server/         service 兼容导出、集成测试与 examples
  client/
    work-client/         本机/远程 adapter、连接恢复、消息投影
  host/
    work-node-host/      实际 owner 启动、数据锁、组件绑定与退出
    work-host/           应用宿主门面与本机 client 包装
  components/
    work-component-api/  UI Bundle、能力注册/授权、隔离动作桥
    work-http-component/ 专用远程访问面板
  update/
    work-update/         签名更新与安装适配
assets/vendor/           正式应用使用的二维码、Mermaid 库及许可证
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
docs/                    guides、architecture、design、reviews、archive、images、release-notes
artifacts/               忽略的本次截图与报告
target/                  忽略的编译、下载、日志与临时验证数据
.rpi/                    忽略的本机 Agent 会话
```

`work-node-api` 是共享契约实际位置；`work-core::api` 重导出契约并保留纯客户端投影。`work-node-runtime` 持有权威 `LocalWorkspace`，`work-node` 仅兼容导出；`work-client` 本机 facade 包装同一个 owner，远端使用 HTTP/SSE/WebSocket。`work-node-service` 适配已有 owner，`work-server` 兼容导出并保留集成测试；生产服务依赖不引用 `work-client`。`work-node-host` 实现无 GUI 宿主生命周期，`work-host` 提供应用门面。全部 15 个工作区成员均为私有包。`work-component-api` 是通用组件包契约、校验、授权与隔离面板桥；`work-http-component` 是 Work 内部的远程访问专用面板。桌面不启动 standalone `rpi-http`，也不把 rpi HTTP 扩展当作自己的宿主运行时；HTTP 入口只接在同一 owner 上。完整依赖见[架构总览](../architecture/overview.md)，具体传输见 [架构与传输边界](../architecture/transport.md) 和[组件组合](../architecture/component-composition.md)，新增和修改代码须遵循 [开发规范](development-standards.md)。

正式应用通过 `assets/vendor/` 编译二维码与 Mermaid 资源，不引用设计原型。原型自己的 vendor 快照和内容哈希独立保留，以便复现历史设计；整理文档不重新生成冻结原型。

`main.rs` 只启动应用。`app/entry.rs` 区分桌面、宿主 Web 登录门禁和独立 Web；`mobile_entry.rs` 是 Android 原生 App 的一次性扫码授权入口。`local_node.rs` / `local_http.rs` 将桌面本机访问和 HTTP 绑定到同一 owner，并协调退出顺序。

`app/workspace.rs` 负责跨功能状态协调，纯辅助逻辑拆到 `identity.rs`、`history.rs`、`runtime.rs`，队列同步和旁问容器在 `background.rs`，目录刷新在 `workspace_sync.rs` / `.js`。文件格式预览与 diff 解析属于 `features/projects/`。

## 常用命令

在仓库根目录运行：

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
node tools/review/check-structure.cjs
node tools/review/check-node-boundaries.cjs

# 浏览器开发依赖只安装到本仓库 tools/node_modules。
npm ci --prefix tools
npm run install-browser --prefix tools
# 或将 CHROME_PATH 指向已有 Chrome 的可执行文件。

# release Web + 无头 owner，默认直接托管 13031；先在 .env.local 配置密码。
task dev
# 桌面 + debug Web 构建监听/刷新，14180 代理到 13031。
task dev:desktop
# 以上二选一，同一 data 不可同时启动第二个 owner。
```

另一个终端启动图册和原型服务：

```powershell
node tools/review/ui-review-server.mjs
```

`task dev` 的当前界面为 `http://127.0.0.1:13031/`；热刷新路径为 `http://127.0.0.1:14180/`；原型仍可通过 `http://127.0.0.1:14182/prototype/` 访问，旧的图册地址也保留。这个 URL 是兼容入口，源文件实际位于 `design/prototype/`。

```powershell
$env:WORK_UI_URL = 'http://127.0.0.1:13031/' # 若使用热刷新路径则改为 14180
node tests/e2e/verify-device-flow.cjs
node tests/e2e/verify-node-lifecycle.cjs
node tests/e2e/verify-ui-conversation.cjs
node tests/e2e/verify-ui-suite.cjs --list
node tools/review/audit-ui-responsive-geometry.cjs
```

模块测试通过 `pwsh -File tools/review/build-module-review.ps1 -Feature all` 构建，使用独立的 `tests/harness/ui-modules/Cargo.lock`，不再复制测试源码到临时工程。真实模型验证仍需显式传入 `--run-model`，普通结构检查和浏览器 fixture 测试不会调用模型。

历史脚本和记录从[归档索引](../archive/README.md)查询；它们描述当时的环境与验收条件，不是当前版本的一键验收承诺。截图、下载包、模拟器和本机配置遵循[测试产物规则](testing-artifacts.md)，文档整理不负责清理这些目录。
