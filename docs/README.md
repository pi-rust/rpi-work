# 文档导航

## 当前使用与开发

- [产品范围与阅读入口](design/product-design.md)
- [项目结构与开发入口](guides/project-structure.md)
- [开发规范与 review 清单](guides/development-standards.md)
- [构建与平台产物（含 Android NDK 检查）](guides/ci-builds.md)
- [电脑、Web 与 Android 连接](guides/node-connection.md)
- [桌面宿主 HTTP、共享与 Web 登录](guides/desktop-http.md)
- [真实聊天接入](guides/live-chat.md)
- [ACP 执行后端与会话绑定](guides/acp-backends.md)
- [项目、会话与消息交互](guides/codex-interactions.md)
- [更新与签名](guides/automatic-updates.md)
- [主题视觉规格](guides/theme-design.md)
- [样式对照](guides/ui-style-review.md)
- [测试源码、产物与复验入口](guides/testing-artifacts.md)
- [版本说明](release-notes.md)

## 当前架构与约束

- [架构总览与模块依赖](architecture/overview.md)
- [架构与传输边界](architecture/transport.md)
- [实时通道、回执与断线恢复](architecture/realtime.md)
- [原生资源与源码服务](architecture/resources.md)
- [组件包、专用面板与能力授权](architecture/component-composition.md)
- [节点、客户端与会话归属](design/node-session-ownership.md)
- [原生扩展、动态组件和惰性设计的独立契约](architecture/extensions.md)
- [原生资源与持久化源码开发](guides/native-resource-authoring.md)
- [精选 Package 发现与安装](guides/curated-packages.md)
- [架构文档审查记录（2026-11-07）](reviews/architecture-review-2026-11-07.md)
- [真实原生扩展与实时 UI Demo（开发验证，不是产品通用协议）](guides/extension-demo.md)

## 产品方向、历史契约与审查

- [Matrix-Agent 产品与资源共识（含建议与未确定项）](design/头脑风暴文档.md)
- [Package、Skills 与扩展管理基础梳理（源码核验与实施建议）](design/resource-management-foundation.md)
- [会话资源配置最小契约（历史提案，当前实现见资源指南）](design/session-resource-contract.md)
- [rpi 会话资源验证：事实与未解决缺口](reviews/rpi-session-resource-validation.md)
- [开源与可扩展性审查快照（待逐项复核）](reviews/open-source-extensibility-review.md)

## 历史设计与验证

[归档索引](archive/README.md)收录已被替代的手机配对/资源方案、早期产品讨论、会话分层迁移与 Android 补验、视觉审阅和原型验收。仍保留在 `design/` 的交互探索已标明设计用途；使用方式以 `guides/` 为准。

`guides/` 维护当前使用方式，`architecture/` 说明源码边界，`design/` 区分产品方向与原型探索，`reviews/` 保留尚有参考价值的专题审查，`archive/` 保存历史方案及验收记录。未填写的验收项不代表已实现或通过；历史记录中的“本轮”“最新”和测试次数只对应当时构建。

`target/`、`artifacts/` 中的日志、哈希与截图是本机或 CI 产物，不保证随 clone 提供。`archive/local/` 是不进入 Git 的本机任务记录，不能作为当前能力或正式规范。
