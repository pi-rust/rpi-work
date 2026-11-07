# 产品范围与文档入口

RPI Work 是围绕节点、项目和会话组织工作的个人 Agent 工作台。桌面、Web 与 Android 访问节点持有的权威数据；桌面额外托管本地 Node，不因此使用另一套会话语义。

本文维护产品范围和阅读入口，不列分批实现日志。早期 v0.1 讨论稿及原型交付计划已[归档](../archive/prototype/product-design-v0.1.md)；后续产品方向见[Matrix-Agent 产品共识](头脑风暴文档.md)，其中建议和未确定项不视为已实现。

## 客户端与默认节点

- **Web 与桌面平级**：都可以连接其他节点，但默认使用当前节点。桌面默认使用它托管的本机节点；Web 默认使用提供当前入口的节点。默认节点不是永久绑定，hosted Web 也应允许用户主动切换其他节点并返回当前节点，不要求先换成独立部署。
- **移动客户端只能连接其他节点**：不在手机上托管本机节点，不提供手机本机工作空间；连接目标可以是桌面或服务器，局域网和公网均可。手机浏览器仍遵循 Web 规则，不因窄屏变成原生移动客户端。
- **节点选择与认证分离**：自动使用当前节点不等于免认证；初始宿主登录目标固定不等于进入后禁止切换节点。切换到其他节点须取得该目标的授权，不能复用当前节点的凭据。各节点分别持有自己的项目和会话。

这是产品约定，具体实现与验收状态见[节点与会话归属 §1.1、§5、§6](node-session-ownership.md)，不能用默认入口验证代替多节点切换验收。

## 核心对象与工作流

- **Node**：执行与数据归属边界。项目文件、会话历史、运行时和模型凭据属于所选节点；同一台电脑可以显式运行多个独立节点。
- **Project**：节点上的工作目录与任务组织上下文；可用于普通工作或元Agent资源开发。
- **Session**：绑定节点、项目和执行后端的持续交互。用户不必先创建独立的长期 Agent 定义；新会话沿用节点的默认后端和已有创建偏好。
- **Run**：一次实际执行尝试。受理、运行、等待确认、取消和最终结算分别表达；接口 ACK 不等于工作完成。

基本流程是：确定操作节点（Web／桌面默认当前节点，移动客户端先连接外部节点；各端均可主动切换目标）→ 完成所需认证 → 打开项目 → 创建或恢复会话 → 提交任务 → 查看消息/工具/确认 → 审阅文件与变更 → 明确继续或结束。切换客户端视图不应停止节点任务，网络断开也不等于执行终止。

RPI 会话资源已通过 typed 原生服务实现主 Package、独立 Skills/扩展、tools default/only/none、精确应用与实际 readback；元 Agent 使用同一条源码开发、真实测试和本地发布路径，见[资源架构](../architecture/resources.md)。ACP 定制和源码扩展自动构建不在已实现范围。旧 desired pins、静态设计预览和目录安装仍不能代替当前宿主 applied/执行证据。

## 当前使用说明

| 工作流 | 入口与边界 |
| --- | --- |
| 桌面本机及远程节点 | [节点连接](../guides/node-connection.md)、[桌面 HTTP 与共享](../guides/desktop-http.md)；本机进程内访问与共享 HTTP 使用同一 owner |
| 宿主 Web / 独立 Web / Android | [认证与扫码](../guides/node-connection.md)；三种入口的认证方式不同，不能由屏幕宽度决定 |
| 对话、队列、确认与文件审阅 | [项目、会话与消息交互](../guides/codex-interactions.md)；以节点支持、真实事件及权威回读为准 |
| RPI 与 ACP 后端 | [执行后端](../guides/acp-backends.md)；切换后端须另建会话，不能将握手成功等同模型回复已验收 |
| 资源库、Agent/UI 扩展与元Agent | [原生资源指南](../guides/native-resource-authoring.md)、[扩展架构](../architecture/extensions.md)；真实测试与旧设计预览分开 |
| 主题与外观 | [主题视觉规格](../guides/theme-design.md)；UI 偏好不修改 Agent 配置 |
| 构建、更新与测试 | [构建](../guides/ci-builds.md)、[更新](../guides/automatic-updates.md)、[测试入口](../guides/testing-artifacts.md)；编译、fixture、安装与真机验收分别记录 |

## 架构与开发约束

依赖方向为 **客户端 → Node 应用服务 → runtime**，节点契约位于 `work-node-api`，纯客户端投影在 `work-core`，节点保留唯一 owner。新增客户端业务使用 typed 应用端口，不通过 UI 原生句柄或 raw dispatcher 绕过节点服务。

- [节点与会话归属](node-session-ownership.md)：数据归属、身份、多节点与共享边界。
- [架构总览](../architecture/overview.md)：实际 crate、数据归属与生命周期。
- [架构与传输](../architecture/transport.md)：RPC/ACP/HTTP/SSE/WebSocket 适配与能力边界。
- [开发规范](../guides/development-standards.md)：会话一致性、权限、草稿、配置回读和生命周期的硬约束。
- [项目结构](../guides/project-structure.md)：代码位置与常用命令。

## 不能据此推定的能力

逐设备/项目 ACL、稳定 node ID 的客户端多入口归一、同页面多节点同时在线、通用 Agent live contribution、任意源码扩展自动构建和附件上传仍需各自实现与验收。原生资源管理及动态组件 Bundle 已有独立实现，不应按旧设计接口的 Unsupported 推定整体缺失。iOS 暂无安装包；Android 编译检查不能证明 APK 安装升级或真机行为通过。

原型中的设备配对、模拟资源安装和示例工具状态不是上述能力的实现。旧方案、分批迁移、视觉审阅和当时测试结果统一从[归档索引](../archive/README.md)查询，不作为当前待办或发布状态。
