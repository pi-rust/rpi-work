# 原型还原验收

> 截图与运行报告为本地产物，不随Git分发；文中路径用于定位本地文件。保留规则与复验入口见 [测试产物说明](testing-artifacts.md)。

2026-10-27（Asia/Shanghai）。rpi CLI 编写生产 Rust / CSS，Codex 独立审查、退回修复、冻结源码后重新构建并验收。最后冻结点为 `target/parity-checkpoints/queue-proposals-frozen`；没有修改只读原型基准。验收阶段未提交或推送代码；实现与CI随后已分别提交。

本次按 [原始目标](ui-parity-goal.md) 完成正式 Dioxus 原型迁移与还原验收，覆盖桌面和响应式手机 Web。逐项结果见 [验收矩阵](ui-parity-matrix.md)。这不等于宣称原型演示中的所有服务端能力已经在上游实现，或移动原生发行版已经验证。

## 主要修复

- 修正错误 CSS 作用域、重复面板、Inspector 归属与滚动、输入区结构；恢复紧凑对话、移动抽屉、快捷会话和主题。
- 统一结构化历史与流式消息、工具/计划/Ask、真实最终结算与重连；修正认证 Cookie、会话/节点身份、异步回写、草稿丢失和串会话问题。
- 补齐队列、BTW、模型/Tools、用量/缓存/context、项目文件树、预览、多文件变更和会话追踪。
- 恢复会话顶部 Packages/Skills/Tools/提示词、多 Package 固定版本、来源与状态、导入/导出、Skills 快捷引用和实际恢复入口。
- 补齐元Agent 的个人源码、编辑器、不可变快照、开发会话、关联 Inspector、发布回执；保留项目自定义名称。
- 恢复 Provider 卡片、连接设置、设备角色、地址二维码、扫码/手工输入；清理迟到相机和 Meta 提示计时器。
- 上下文开关/range、队列暂停/排序/编辑保留为明确可操作的本地提案；不会伪造节点配置成功或改变真实队列。只有用户显式入队才发送命令。

## 最终证据

| 验证 | 结果 |
| --- | --- |
| 正式应用行为 | 29套、198检查通过 |
| 主布局和排版 | 5种视口、63处与原型精确对照通过 |
| 页面截图 | 25状态 × 5视口，共125组配对，无捕获错误；另有队列展开等行为截图 |
| Rust | fmt、locked workspace tests、native/WASM严格Clippy、native UI/CLI构建通过 |
| Web | 正式release WASM构建；实际rpi-web同源HTML/JS/WASM/API、密码认证、390/320通过 |
| 真实模型 | 正式页面→认证rpi-web→rpi→模型→流式回复→completed；全部工具关闭 |
| 原型/源码身份 | 39个基准文件未变；44个生产源码文件与构建SHA一致；验证期间无源码变化 |

视口：1440×1000、1280×800、768×1024、390×844、320×740。不是将整幅页面宣称为零像素差异：真实模型/会话/文件/用量、空态和协议可用性不同于样例，模态框内容高度随真实内容变化。对新增与关键桌面/手机状态做了截图人工复核；固定布局的63处有精确断言。

- 完成审计数据（本地生成：`artifacts/ui-completion-audit.json`）
- 行为检查结果（本地生成：`artifacts/ui-final-validation.json`）
- Rust检查结果（本地生成：`artifacts/engineering-validation.json`）
- 125组图片对照（本地生成：`artifacts/ui-review-gallery.html`）
- 真实模型结果（本地生成：`artifacts/real-model-validation.json`）
- 实际摘要协议兼容性（本地生成：`artifacts/real-context-summary-validation.json`）

复验：

```powershell
./scripts/build-ui-web.ps1
./scripts/verify-rust-workspace.ps1
node scripts/verify-ui-suite.cjs
$env:REVIEW_ALL_VIEWPORTS = '1'
node scripts/audit-ui-pages.cjs
node scripts/finalize-ui-review.cjs
```

最后一项还会核对真实集成报告是否属于当前bundle；如果重新构建了不同源码，需要相应更新同源集成证据。真实模型验证是独立的显式选项，不会被普通suite自动反复调用。

## 实际边界

1. 原生 Package 安装、逐设备授权、自动压缩设置写入没有当前可用节点接口。本地资源、显式引用、提案、真实状态与恢复入口已保留，不能称为节点已生效。
2. 已安装 rpi 0.9.2 对 `get_session_branch` 返回 unknown command；界面提供更新/重试。邻仓新源码支持不代表已安装二进制支持。不会用状态行或客户端摘要冒充原分支完整摘要。
3. rpi-http 已有20MiB项目文件上传路由；本客户端未接入该路由，原生RPC prompt目前仍是文本。图片只能本地预览，不能声称模型已看到图片。
4. Android/iOS原生包、真机摄像头和原生WebView硬件操作未验证。已验证响应式浏览器交互与native编译。

本任务真实验证只使用自有的43340/43341隔离实例，独立registry/session目录；43341配置测试密码。3030/3031属于其他进程，未修改或终止。此处保留服务仅供本地验收预览，不是外网部署。
