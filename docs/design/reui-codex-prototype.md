# ReUI · Codex 桌面 Web 原型（500 轮计划）

> 目标：在 `design/reui-codex/` 用 **ReUI**（MIT 的 shadcn 生态组件库）实现一版 **Codex 式布局**的工作台前端，并逐步接到真实节点。
> 预算：500 轮。一轮 = 一次可合并的改动 + 一次可复现验收（`npm run build` + Playwright 截图 + 必要单测）。

## 0. 目标（goal）

**一句话**：在 `design/reui-codex/` 交付一版以 ReUI（MIT）+ Codex 布局为基础、规则全部由 Rust
（`work-viewmodel` / `work-client` 的 wasm 绑定）驱动的 Web 工作台，500 轮内达到可替代
`apps/work-ui` 的 Web 入口能力。

**完成的定义（Definition of Done）**

1. P0–P10 全部验收通过（见 §2 表格），每阶段一次提交 + 可复现验收脚本。
2. 实时事件、发送/中止、文件 Diff、资源与设置、320/390 布局与当前 Dioxus Web 版功能对齐（对照表逐项勾选）。
3. 展示规则零 TypeScript 重复：行序、工具摘要、代码分词、指标格式、主题 token 只能来自 Rust。
4. 产物可由 `work-node-service` 同源托管，登录 → 工作台 → 发送全链路可用。

**轮次预算**：500 轮（§2 合计 540，含 40 轮缓冲）。

## 0.1 许可证结论（先回答能不能免费用）

| 对象 | 许可 | 能否商用 | 说明 |
| --- | --- | --- | --- |
| `keenthemes/reui` 仓库、`reui.io/components`、1,264 个免费示例、27 个自研组件 | **MIT**（© 2025 Keenthemes Inc） | ✅ 可商用、可改、可再分发 | 保留版权声明即可；本项目在 `design/reui-codex/NOTICE.md` 记录 |
| **ReUI Pro**（付费 blocks / icons / templates） | 一次性商业授权 | 需购买 | 安装需 `REUI_LICENSE_KEY`，不在 MIT 范围；本原型未使用 |
| shadcn/ui、Base UI、shiki、Geist 字体 | MIT / MIT / MIT / OFL-1.1 | ✅ | 见 `NOTICE.md` |

结论：**免费可用，商用也允许**；只有 Pro 资产要付费。本原型只用免费 registry 项。

## 1. 关键架构决策：Rust 规则如何进 React

Codex 布局最容易做错的地方，是把「行序、工具摘要、代码词典、指标格式」在 TypeScript 里重写一遍，从此两套真相。

因此 P2 之前必须先落地：

```
crates/client/work-viewmodel            (现有，7.9k 行 / 129 测试)
        │  wasm-bindgen
        ▼
crates/client/work-viewmodel-wasm  ──►  npm 包 @rpi/work-viewmodel
        │
        ▼
design/reui-codex (React)  调用 orderedRows() / toolTarget() / tokenize() / formatMetrics()
```

- React 只做布局与交互，规则仍由 Rust 单测守护。
- 若 wasm 方案受阻（体积/启动），降级方案是 Node 侧 HTTP 端点 `POST /api/v1/present/rows`，仍复用 Rust 实现。

## 2. 阶段与轮次预算

| 阶段 | 内容 | 轮次 | 验收 |
| --- | --- | --- | --- |
| **P0 基建** | Vite + React 19 + Tailwind 4 + shadcn preset `nova`（Base UI）+ ReUI registry + `cn` 别名 + NOTICE | 10 | `npm run build` 通过；`@reui/code-block`、`@reui/timeline` 可用；许可证记录齐全 |
| **P1 Codex 布局骨架** | 侧栏 / 顶栏 / 转写 / 工具卡 / 运行面板 / composer / 底栏 / 明暗主题 | 40 | 1440×900 明暗两张截图；零 page error |
| P2 只读数据 | work-client 只读（项目/会话/历史）；Vite 代理；空态、加载态、错误态 | 60 | 对真实节点渲染 4 个 fixture 会话；断网时给出可恢复错误 |
| P3 实时流 | WebSocket 事件 → 增量行序（经 wasm viewmodel）；断线重连与重放 | 80 | 同一 fixture 事件流下与 Dioxus 客户端行序一致（逐行快照比对） |
| P4 写操作 | 发送、中止、草稿、队列、模型/思考切换、回读校验 | 80 | 乐观消息不跳行；失败回滚；重复提交被抑制 |
| P5 主题对齐 | 12 套主题 token 从 `work-viewmodel::theme` 导出 CSS 变量；密度切换 | 40 | token 与 `codex.css` 逐值一致；主题切换无硬编码色 |
| P6 文件与 Diff | ReUI `tree` + `data-grid` + diff `CodeBlock`；项目文件面板 | 60 | 打开真实文件与 diff；大文件滚动不卡（>5k 行） |
| P7 资源与设置 | 资源中心、Provider、更新、协作页 | 60 | 与当前 Dioxus 版本功能清单逐项对齐（表格化） |
| P8 移动端 | 320/390 断点、侧栏抽屉、composer 单行、扫码入口占位 | 40 | 320×740 无横向滚动；点击区 ≥ 40px |
| P9 可访问性与 IME | 焦点顺序、键盘操作、中文输入法组合、朗读标签 | 40 | 键盘全流程可达；IME 组合期间不重复提交 |
| P10 交付 | 静态产物接入 `work-node-service` web assets；构建脚本与 CI | 30 | 打包后由节点同源托管，登录 → 工作台 → 发送全通 |

合计 **540** 轮，留 40 轮缓冲（缺陷修复与视觉打磨）。

## 3. 已完成

### P6 文件与 Diff（已落地，变更视图）

- `work-client-wasm` 增加 `files` / `file` / `git_status` / `git_diff`（`staged` 透传）。
- 右栏新增「文件」标签：列出节点 worktree 的改动（状态字、已暂存标记、最多 60 条可滚动），
  点选后按 `git diff` 渲染 unified diff；文件读取（`file`）留作目录树（P6b）。
- 验收：`verify:live` 18 条（新增「改动列表来自节点」「选中改动渲染两侧 diff」）。
- 待做：目录树（ReUI `tree`）、大 diff 的 `data-grid` 变更表、暂存切换。

### P5 主题对齐（已落地）

- `src/lib/theme.ts` 把 `theme_tokens(id, dark, compact)` 的 Rust token **改名映射**成 shadcn
  的 CSS 变量（`bg→--background`、`text→--foreground`、`line→--border`、`accent-bg→--accent`…），
  TypeScript 里不定义任何颜色。
- 预设目录、明暗、密度全部来自契约：`presets()` 目前 5 套内置（纸与苔绿 / 海湾蓝 / 石墨紫 /
  Reddit 橙 / Codex 黑白），自定义上限 12 套。密度用 `density-factor` 缩放根字号，rem 布局整体跟随。
- 选择写入 `localStorage`，刷新后保持。
- 验收：`npm run verify:theme` 7 条（token 落地、目录、切预设、切暗色、密度缩放、持久化、
  零 page error）。
- 代码着色：`CodeBlock` 使用 ReUI 自带的 `css-variables` shiki 主题，token 颜色读
  `--code-token-*`，由 `themeStyle()` 从 Rust token 生成 —— 换主题时代码配色同步变化。

### P4 写操作（已落地，含队列与草稿）

- 队列：`queue` / `queue_message` / `cancel_queued` / `clear_queue` 全部经 wasm 客户端；
  运行面板展示队列项与「清空」，取消后按节点结果回读。
- 草稿：`src/lib/drafts.ts` 按 `origin/session` 存 `localStorage`，切会话/切节点互不串味，
  配额写失败时退回内存不打断输入。
- 验收：`npm run verify:live` **16 条**（新增队列读取、取消回读、跨会话草稿恢复）。

- `work-client-wasm` 增加 `prompt` / `abort` / `runtime_state` / `models` /
  `thinking_levels` / `set_model` / `set_thinking`；切换后一律**回读权威 runtime state**。
- composer：⌘↵ 发送、发送中变为「中止」、失败保留草稿并显示节点错误文案。
- 模型与思考等级下拉来自节点目录，选中即以回读结果更新标题栏与运行面板。
- 运行面板指标改为节点 `get_state` 的 `stats`/`context`（fixture 模式下仍用内置数据）。
- 一个实测发现：写命令走实时 socket，而不是 HTTP 请求体，因此验收脚本改为断言
  WebSocket 帧（`prompt`/`abort` 命令与其文本）。
- 验收：`npm run verify:live` 13 条全绿（新增发送、拒绝保留草稿、模型回读、思考回读、中止）。

### P3 实时流（已落地）

- `work-viewmodel-wasm::Presenter::apply_events(scope, history, wire)`：wasm 侧持有
  `RunProjection`，把 `WireEvent` 解码后 `apply`，返回 `{rows, blocks, phase, settled}`。
  去重、陈旧序号拒绝、resync 都由 Rust 投影负责 —— 重连重放不会重复行。
- `work-client-wasm::NodeClient::{subscribe,stop}`：`spawn_local` 消费 `EventStream`，
  逐个事件回调给 JS；返回订阅号供切换会话时停止。
- Vite 代理启用 `ws: true`，`/api/v1/realtime` 与 HTTP 同一来源；`?live=1` 自动连接。
- 验收：`npm run verify:live` 8 条（含「WS 事件产生 read/bash 工具卡」与「展开看到流式结果」）。

### P2 只读数据（已落地）

- 新增 `crates/client/work-client-wasm`：`NodeClient::{auth,login,projects,sessions,messages}`，
  协议与错误形状留在 Rust，JS 只做 promise 适配。
- Vite 同源代理 `/api` → `RPI_NODE_URL`，浏览器不需要 CORS 或额外 host 允许列表。
- `src/lib/workspace.ts`：fixture / live 双数据源、连接生命周期（connecting / auth-required /
  error / ready）、断开回退，全部喂给同一个 `TranscriptPresenter`。
- `messages()` 用 `message_summaries` 取最新用户消息做锚点，再 `message_window` 取窗口，
  满足原生节点「恰好一个游标」的约束。
- 验收：`npm run verify:fixture`（6 条）与 `npm run verify:live`（6 条）全绿，截图见提交。


### P2/P3 前置：`crates/client/work-viewmodel-wasm`（已落地）

- 新增 workspace 成员，`crate-type = ["cdylib", "rlib"]`，导出 `Presenter::rows`、`tool_target`、
  `tool_icon`、`tokenize_file`、`metrics_session`、`context_reported`、`format_tokens`、
  `theme_tokens`、`theme_presets`、`normalize_address`。
- 前端 `src/lib/viewmodel.ts` 是薄封装；`npm run build:viewmodel` 生成 `src/pkg/`（构建产物，gitignore）。
- 浏览器验收 6 条断言：行序来自 wasm、工具目标来自 Rust、底栏 `32k / 4.8k / 75% / 38% / $0.0184`
  由 Rust 格式化、主题 token 来自 `theme_contract`、明暗截图、零 page error。
- Rust 侧 4 个单测覆盖同样的规则（`cargo test -p work-viewmodel-wasm`）。

- P0、P1 全绿：可运行于 `npm run dev` / `npm run preview`，明暗两套截图见提交说明。
- 组件：ReUI `CodeBlock`（shiki 着色 + 行号 + 复制）、ReUI `Timeline`（运行步骤）、shadcn `Button/Textarea/ScrollArea/Badge/Avatar/Separator/Collapsible/Tooltip`。
- 布局：Codex 三段式（左栏 264px / 中央 max-w-3xl 对话 / 右侧 300px 运行面板）+ 底部 28px 指标栏。

## 4. 每轮约定

1. 改动只落在 `design/reui-codex/`（或 P2 起的 `crates/client/work-viewmodel-wasm/`）。
2. 每轮结束必须：`npx tsc -b` 通过、`npm run build` 通过、截图或断言更新。
3. 每轮一个提交，提交信息写清「看到的行为变化」。
4. 不引入 ReUI Pro 资产；如确需，先在 `NOTICE.md` 记录授权来源。
5. 与 `apps/work-ui`（Dioxus）保持功能对照表，避免两套产品语义漂移。
