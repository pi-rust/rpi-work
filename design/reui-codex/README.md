# ReUI · 工作台迁移工作区（未正式发布）

已确认的 1:1 目标是 `design/reui-message-layout/`，不是本目录的早期 Codex 外壳。迁移范围、真实接口和未完成项见 [迁移清单](../../docs/design/confirmed-workbench-migration.md)。已迁入真实右侧浏览器、整窗顶栏/快捷栏和独立项目/置顶/最近列表，并接通 typed 项目登记/会话创建及权威回读；消息视觉、居中设置、完整权限门槛和正式发布仍未完成。

本目录使用 [ReUI](https://reui.io)（MIT 的 shadcn 生态组件库）和 Rust WASM。fixture 数据来自 `src/data.ts`，与 `tests/fixtures/ui-fixture.cjs`、`apps/work-gpui` 原型同内容，仅用于隔离测试，不代表真实节点状态。

```bash
cd design/reui-codex
npm ci
npm run build:wasm        # 编 work-viewmodel-wasm / work-client-wasm，再生成 src/pkg/
npm run dev               # http://localhost:5173
npm run build             # 先编 wasm，再 tsc + vite build
```

展示规则不在 TypeScript 里：`src/lib/viewmodel.ts` 只是 `crates/client/work-viewmodel-wasm`
的薄封装，行序、工具目标、指标格式、主题 token 都由 Rust 提供（`src/pkg/` 为构建产物，已忽略）。

## 两种数据源

| 模式 | 命令 | 说明 |
| --- | --- | --- |
| fixture（默认） | `npm run preview` | 内置内容，验证布局与 Rust 规则 |
| live | `node scripts/live-fixture.cjs` 拿端口 → `RPI_NODE_URL=http://127.0.0.1:<port> npm run preview -- --port 5180` → 打开 `http://127.0.0.1:5180/?live=1` | 通过 wasm 版 `work-client` 读取真实节点的项目/会话/消息；`/api` 由 Vite 同源代理 |

验收脚本（需要仓库根目录 `npm ci --prefix tools`）：

```bash
npm run verify:a11y              # 自包含：无障碍与键盘 7 条断言
npm run verify:settings          # 自包含：Provider 目录 + 一次性授权 6 条断言
npm run verify:tree              # 自包含：目录树懒加载 4 条断言
npm run verify:mobile            # 自包含：320/390 两档，7 条断言
npm run verify:hosted            # 真实隔离Rust节点 + 合成RPC原生后端，托管/创建/草稿 8 条（构建要求见下）
npm run verify:browser           # 自包含：隔离真实 HTTP 页面、URL/沙箱、历史/标签/表单保留及明暗响应式
npm run verify:shell             # 自包含：7宽×2主题几何、项目置顶/折叠、节点会话元数据、空态与草稿保留
npm run verify:creation          # 自包含：创建/拒绝/未知结果、仅GET核对、迟到回调/重连/草稿、弹窗明暗响应式
npm run verify:all               # 依次跑全部测试；fixture/theme 需 REUI_URL 指向本目录构建的 preview
npm run verify:live              # 自包含：起 fixture + preview，22 条断言
npm run verify:fixture           # fixture 模式 6 条（需先启动 preview 到 5179）
npm run verify:theme             # 主题契约 8 条（需先启动 preview 到 5179）
```

hosted 验收先在仓库根目录构建 `cargo build -p work-cli --locked` 及 `cargo build -p work-node-service --bin work-server-rpc-fixture --locked`。脚本固定独立 Work/Agent 目录和 RPC fixture 程序，不使用已安装 rpi、真实凭据或模型；测试用空资源目录 CLI 仅对 fixture 的专用副本名启用，不代表真实 rpi 或资源安装验收。当前全部11组本地通过；CI新增 settings/tree/browser/shell/creation，hosted仍需本地节点二进制，不宣称已执行远程CI。

## 已实现

| 区域 | 实现 |
| --- | --- |
| 左侧栏 | 52px 快捷栏、275px 项目/置顶/最近独立列表；折叠和项目置顶为按节点入口隔离的设备偏好，置顶/归档会话读取节点事实 |
| 创建 | 已连接节点上的项目登记/会话创建，Rust保留API协议和拒绝/未知结果区分；按返回ID回读列表后切换，不清空旧草稿，不在本地造项目/会话；未知写入结果锁定，仅GET核对 |
| 未接入入口 | 系统目录选择、目录创建、状态筛选、归档管理、会话目标、计划任务及会话导航历史暂时禁用并解释原因；新建仅在真实连接和项目上下文中开放 |
| 命令面板 | ⌘K 搜索项目与会话，↑↓ 选择、Enter 打开、Esc 关闭 |
| 顶栏 | 跨整窗46px工具栏（手机44px）、居中会话标题、真实连接状态、浏览器/运行/文件入口；主题/密度等收进显示选项弹层 |
| 浏览器 | 真实 HTTP(S) iframe、多标签、地址栏导航历史、刷新、页面历史选择、自适应 / 移动视口；切主题/标签/面板保持 DOM 和未保存表单 |
| 转写区 | 用户/助手消息、模型徽标、工具卡（可折叠、耗时、状态、参数、结果） |
| 代码 | ReUI `CodeBlock`（shiki 着色、行号、复制按钮）渲染 `read` 的工具结果 |
| 运行面板 | ReUI `Timeline` 展示运行步骤 + 模型/思考/工具/缓存/成本/上下文、队列 |
| 文件面板 | 改动列表 + unified diff + ReUI Tree 目录浏览（懒加载、看源码）+ 暂存开关 |
| 资源中心 | 节点 native catalog → 计数 + 表格（分组/命名/范围由 `work-viewmodel::resources` 提供） |
| 安装 | 审阅 bundle（文件清单 + sha256）→ 选范围 → 勾选信任 → 获取/安装 → 目录回读 |
| 设置 | 节点与协作事实 + 一次性授权二维码 + Provider 目录与**密钥写入**（CAS revision、回执目录、重启提示） |
| 输入区 | 圆角 composer、模型与思考等级 chip、上下文比例、发送态禁用逻辑 |
| 指标 | 原底栏保留为屏幕阅读器摘要，视觉指标在运行面板读取 Rust contract，不额外占用确认原型的整窗底边 |

## 组件来源

- `src/components/reui/`：ReUI registry 安装（`npx shadcn@latest add @reui/code-block @reui/timeline`）
- `src/components/ui/`：shadcn（preset `nova` / Base UI）
- 许可证与免费/付费边界见 [NOTICE.md](NOTICE.md)

## 已知限制

- 已接实时节点：项目/会话/历史 + WebSocket 事件（工具卡来自真实事件流），密码登录与断开回退可用。
- 可发送消息（⌘↵）、中止运行、切换模型与思考等级（均以节点回读为准）。
- 队列：列出/取消/清空；草稿按 `origin/session` 本地保存，切会话自动恢复。
- Provider 支持写入 API 密钥（CAS + 回执回读）。
- 「应用更新」只说明边界（Web 不支持自更新）并展示节点版本；真正的更新检查与安装属桌面端。
  运行面板的「步骤」列表仍是占位，指标在 live 模式已来自节点 `get_state`。
- `src/pkg/` 需先执行 `npm run build:viewmodel`（约 70s，含 wasm 目标编译）。
- 主题已接 `work-viewmodel` 契约（5 套内置预设 + 明暗 + 密度），代码着色通过 `--code-token-*` 跟随同一套 palette。
- 浏览器保持 `sandbox="allow-scripts"` 与 `referrerPolicy="no-referrer"`，拒绝危险协议和 URL 内显式账户/密码，不注入节点认证或父页面桥；新窗口使用 `noopener noreferrer`。Web 隔离意味着只能记录地址栏发起的历史，不能读取跨源页内导航，也不能可靠检测 CSP/X-Frame-Options 拒绝；始终提供新窗口恢复路径，不把 load 事件称为成功。浏览器自身的 Cookie/网络策略仍由浏览器管理，并非原生完整浏览器引擎。
- 嵌入目标由用户显式打开；不启动开发进程，不伪造项目首页，不因换主题、隐藏面板或改变视口自动刷新。Chromium 既有重绘诊断与布局截图分开，截图从新的页面上下文生成。
- 已验 320/390 的旧外壳及新浏览器交互，不代表已通过确认原型的整窗像素对照。
- 右侧运行面板的步骤仍为静态数据，不随会话切换。

## 部署（同源托管）

```bash
npm run build                       # 产出 dist/
RPI_WORK_HTTP_BIND=127.0.0.1 RPI_WORK_HTTP_PORT=13031 \
RPI_WORK_WEB_LISTEN_PORT=13032 RPI_WORK_HTTP_PASSWORD=... \
RPI_WORK_HTTP_ASSETS=design/reui-codex/dist \
  ../../target/debug/work-cli serve
```

浏览器打开 `http://127.0.0.1:13032/`：页面从 `/.well-known/rpi-work-entry` 发现自己由节点托管，
自动连接同源 API 并提示输入节点密码。
