# ReUI · Codex 布局原型

用 [ReUI](https://reui.io)（MIT 的 shadcn 生态组件库）+ Codex 式布局，实现一版工作台界面。数据来自 `src/data.ts`，与 `tests/fixtures/ui-fixture.cjs`、`apps/work-gpui` 原型同内容，便于三个渲染器横向比对。

```bash
cd design/reui-codex
npm ci
npm run build:viewmodel   # cargo build -p work-viewmodel-wasm + wasm-bindgen → src/pkg/
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
npm run verify:live              # 自包含：起 fixture + preview，16 条断言
npm run verify:fixture           # fixture 模式 6 条（需先启动 preview 到 5179）
npm run verify:theme             # 主题契约 7 条（需先启动 preview 到 5179）
```

## 已实现

| 区域 | 实现 |
| --- | --- |
| 左侧栏 | 工作区切换、`新对话 ⌘J`、项目折叠列表（含会话计数）、会话选中、节点状态、设置入口 |
| 顶栏 | `项目 / 会话` 面包屑、在线状态、明暗主题切换、右侧运行面板开关 |
| 转写区 | 用户/助手消息、模型徽标、工具卡（可折叠、耗时、状态、参数、结果） |
| 代码 | ReUI `CodeBlock`（shiki 着色、行号、复制按钮）渲染 `read` 的工具结果 |
| 运行面板 | ReUI `Timeline` 展示运行步骤 + 模型/思考/工具/缓存/成本/上下文 |
| 输入区 | 圆角 composer、模型与思考等级 chip、上下文比例、发送态禁用逻辑 |
| 底栏 | `↑ 32k ↓ 4.8k 缓存 75% 成本 $0.0184 上下文 38% 工具 2/失败 0` |

## 组件来源

- `src/components/reui/`：ReUI registry 安装（`npx shadcn@latest add @reui/code-block @reui/timeline`）
- `src/components/ui/`：shadcn（preset `nova` / Base UI）
- 许可证与免费/付费边界见 [NOTICE.md](NOTICE.md)

## 已知限制

- 已接实时节点：项目/会话/历史 + WebSocket 事件（工具卡来自真实事件流），密码登录与断开回退可用。
- 可发送消息（⌘↵）、中止运行、切换模型与思考等级（均以节点回读为准）。
- 队列：列出/取消/清空；草稿按 `origin/session` 本地保存，切会话自动恢复。
- **还没有**文件 Diff、资源/设置页、12 套主题（P5–P7）；
  运行面板的「步骤」列表仍是占位，指标在 live 模式已来自节点 `get_state`。
- `src/pkg/` 需先执行 `npm run build:viewmodel`（约 70s，含 wasm 目标编译）。
- 主题已接 `work-viewmodel` 契约（5 套内置预设 + 明暗 + 密度），代码着色通过 `--code-token-*` 跟随同一套 palette。
- 未做 320/390 窄屏布局验收。
- 右侧运行面板为静态数据，不随会话切换。
