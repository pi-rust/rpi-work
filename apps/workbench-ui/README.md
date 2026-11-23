# RPI Work · 已确认工作台实现（整体迁移未完成）

正式源码与Web构建入口在本目录，不依赖 `design/`；视觉/交互基准仍是已确认的 `design/reui-message-layout/`。前端复用 `work-client-wasm` → `WorkspaceClient` / typed NodeApi，以及 `work-viewmodel-wasm` / RunProjection；不在TypeScript写HTTP endpoint、认证协议、执行状态机或第二个owner。

当前已迁入整窗外壳、独立项目/置顶/最近、真实Web浏览器和项目登记/会话创建。**源码入口迁出不代表完整1:1、Native/Android、权限/结算或正式安装发布已完成**；旧 `apps/work-ui` 未改动。范围与边界见[迁移清单](../../docs/design/confirmed-workbench-migration.md)。

## 构建与启动

在仓库根目录安装浏览器测试依赖 `npm ci --prefix tools`，然后：

```bash
cd apps/workbench-ui
npm ci
npm run build             # Rust WASM → tsc → 产品 dist/
npm run dev              # 默认无本机节点代理，无内置样例
npm run preview          # 预览产品 dist/
```

开发连接自己的节点时，显式设置 `RPI_NODE_URL=http://127.0.0.1:<port>` 再运行dev/preview，并用 `?live=1` 连接同源代理；不默认探测或连接13031。真正由节点托管时，公开 `/.well-known/rpi-work-entry` 指示自动连接当前origin，认证仍走Rust客户端；不采用元数据里的远端跳转地址。

产品默认从零项目/零会话/未知指标启动。普通静态托管、断开、认证错误不会回落样例，运行观察缺失不填充样例模型/思考/成本/上下文/步骤。未连接时可以保留草稿，但发送、队列与创建不冒充可用。

### 显式隔离的测试样例

```bash
npm run build:fixture     # 独立 dist-fixture/，不覆盖 dist/
npm run preview:fixture  # 必须访问 /?fixture=1 才启用样例
# 或 npm run dev:fixture，再访问 /?fixture=1
```

产品Vite alias只导入空初始快照；只有fixture构建才导入 `tests/fixtures/workspace.ts`。URL参数不能为产品构建启用样例；fixture构建也不会默认启用。示例页面标注“示例 · 未连接”，不能发送消息或登记真实数据；显式连接后清除样例，断开后也不自动恢复样例。样例不进入产品模块图或 `dist/`。

`src/lib/types.ts` 是渲染数据类型，Rust仍拥有排序、工具目标、指标格式和主题规则。`src/pkg/`、`dist/`、`dist-fixture/` 是忽略的构建产物，不提交或用设计目录作运行依赖。

## 验收

```bash
npm run build
npm run build:fixture
npm run verify:all        # 自起独立fixture预览，按序跑12组，结束仅清自身进程
npm run lint
```

| 脚本 | 覆盖 |
| --- | --- |
| `verify:entry` | 产品无样例、独立测试构建显式启用、认证失败/缺失观察、显示WASM迟到、ACK不清后来编辑及同地址重连，8项 |
| `verify:a11y` | landmarks、图标名称、示例草稿不假发送、Tab、命令面板与Escape |
| `verify:fixture` / `verify:theme` | Rust投影、工具展示、指标、主题/密度/代码配色（runner提供隔离预览） |
| `verify:mobile` | 320/390视口与抽屉 |
| `verify:settings` / `verify:tree` | 隔离协议fixture的Provider/一次性授权/源码目录 |
| `verify:live` | typed读取、真实命令路径、IME、中止、队列、草稿、配置与断开不回样例 |
| `verify:hosted` | 真实隔离Rust owner同源托管、认证、项目/会话创建及草稿，8项 |
| `verify:browser` | 隔离真实HTTP页面、沙箱、标签/地址栏历史/刷新、表单/选区/草稿保留 |
| `verify:shell` | 7宽×明暗几何、独立列表、设备偏好隔离、节点pin/archive事实及空态 |
| `verify:creation` | 权威ID回读、拒绝/未知结果、只GET核对、重复激活/迟到回调/重连、草稿与弹窗 |

hosted前在仓库根目录执行：

```bash
cargo build -p work-cli --locked
cargo build -p work-node-service --bin work-server-rpc-fixture --locked
```

hosted只使用独立Work/Agent目录及现有无凭据RPC fixture；专用副本名才能启用只读空资源目录profile，默认RPC-only fixture行为不变。不启动已安装rpi、不读真实凭据、不发prompt或调用模型。它是**真实Rust owner + 合成原生协议后端**，不是实际rpi、模型、资源安装或Native窗口验收。其余live命名的脚本是隔离HTTP/WS协议fixture，不可与真实模型验证混称。CI接线不等于远程CI已执行。

截图生成到仓库 `artifacts/confirmed-workbench/`，不提交；日志/诊断留在 `/tmp` 或 `target/`。4193/4181及原型既有截图不由这些脚本清理。

## 已接通与尚未完成

- 外壳：46px整窗顶栏、52px快捷栏、275px侧栏，700px导航抽屉/900px浏览器覆盖层；项目置顶/折叠仅本设备偏好，会话pin/archive来自节点。
- 创建：只登记目标节点的已有目录，`initGit=false`；按返回ID回读后切换。关闭/重连不表示取消；未知写入结果锁住，仅GET核对，不能按名称或差集猜成功。系统目录选择和目录创建禁用。
- 草稿：按节点/会话或空项目隔离，同一render取正确scope；存储被拒仍保留内存草稿。创建/拒绝/切换不清掉旧草稿。
- 浏览器：真实HTTP(S) iframe、沙箱/no-referrer，无节点凭据注入；主题/切栏/视口不重载。Web不能回读跨源页内导航或可靠判定CSP/X-Frame-Options失败，load不等于成功，保留新窗口恢复。不冒充原生完整浏览器。
- 已有资源/Provider/源码接口继续保留，但视觉与每项能力/生命周期还需逐项验收。完整SessionAccess权限、accepted/settlement、消息阅读/经典与滚动状态、居中设置、真实协同/本机桥接及Native/Android适配均未完成。

组件仅使用免费MIT的ReUI/shadcn生态；许可证说明见 [NOTICE.md](NOTICE.md)，没有引入Pro资产或授权密钥。

## 同源托管产品产物

在仓库根目录执行；先通过安全环境配置非空 `RPI_WORK_HTTP_PASSWORD`，且不能与另一个owner共用同一数据目录同时启动：

```bash
RPI_WORK_HTTP_BIND=127.0.0.1 RPI_WORK_HTTP_PORT=13031 RPI_WORK_WEB_LISTEN_PORT=13032 \
RPI_WORK_HTTP_ASSETS="$PWD/apps/workbench-ui/dist" \
  cargo run -p work-cli --locked -- serve
```

打开 `http://127.0.0.1:13032/` 走同源认证。仅托管产品 `dist/`，不托管 `dist-fixture/` 或测试源码；本说明不是安装发布或Native/Android完成声明。
