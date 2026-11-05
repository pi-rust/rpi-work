# 历史文档索引

本目录保存已完成的迁移过程、已被替代的方案和特定构建的审查/验收记录。保留原始结论、失败与未验收项，不将后续成功回填为早期通过；文中的“当前”“本轮”“仍未完成”仅针对各次记录。

当前使用方法见[文档导航](../README.md)，架构约束见[开发规范](../guides/development-standards.md)。历史方案不自动成为现行需求，历史检查通过也不证明新源码、已安装应用或真实设备通过。

## 已完成的实现与验证记录

- [会话中心分层迁移与 Android 补验](node-session-migration.md)：七批迁移过程、当时的五方法端口/工具链阻塞、最后一批统一 typed 入口及后续双 ABI 编译补验。原章节序号保留；当前归属说明见[节点与会话](../design/node-session-ownership.md)。
- [Codex 功能面与多轮视觉审阅](codex-visual-review.md)：功能盘点、截图、各轮测试与曾发现的问题；不作为当前“最新验收成绩单”。当前交互见[使用说明](../guides/codex-interactions.md)。
- [原型还原验收](ui-parity-acceptance.md)、[验收矩阵](ui-parity-matrix.md)、[当时目标](ui-parity-goal.md)。
- [实现审阅](implementation-review.md)、[页面审阅](ui-pages-review.md)、[原型验证](prototype-validation.md)。
- [上游功能审阅](upstream-functional-review.md)：当时观察到的协议/能力，当前适配以[传输架构](../architecture/transport.md)为准。

## 已被替代的原型方案

- [产品与交互设计 v0.1](prototype/product-design-v0.1.md)：早期路线和原型交付阶段；现行范围见[产品说明](../design/product-design.md)。
- [手机扫码与逐设备配对提案](prototype/mobile-connection.md)：包括未落地的电脑审批、项目授权和高级表单；现行 Web/Android 入口见[节点连接](../guides/node-connection.md)。
- [资源下载/安装模拟](prototype/resource-acquisition.md)、[多 Package 与会话版本探索](prototype/session-packages.md)：不作为首版需求或实际资源加载事实。现行产品方向见[产品共识](../design/头脑风暴文档.md)，未解决加载问题见[资源验证](../reviews/rpi-session-resource-validation.md)。

## 不在这里清理的内容

- `local/` 中 Git ignored 的本机协作记录保留，不作为正式文档入口。
- 历史文档提到的 `target/`、`artifacts/` 路径用于定位原始证据，不保证新 clone 中存在；本轮整理不删除这些产物。
- 尚未解决的资源加载契约和专题审查仍在 `design/` / `reviews/`；没有因归档旧方案而宣称其问题已解决。
