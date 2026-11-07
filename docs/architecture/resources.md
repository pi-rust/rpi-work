# 原生资源与源码服务

本文描述当前实现的归属与状态边界。用户操作和详细恢复流程见[原生资源与持久化源码开发](../guides/native-resource-authoring.md)，精选包见[包发现指南](../guides/curated-packages.md)。旧设计提案保留为历史讨论，不作为当前 API 清单。

## 服务链与事实来源

```text
共享 UI → WorkspaceClient / NodeApi
         → 本机 owner 或认证 Work HTTP adapter
         → work-node-runtime::workspace::{resources, authoring, packages, providers}
         → rpi-extcore Service → 原生 rpi 资源管理与会话执行
```

`work-node-api` 定义原生目录、管理请求、精确配置、操作结果和 authoring 类型。Work owner 校验项目、来源根和 opaque references，持有源码/provenance 记录并适配原生服务。extcore/rpi 拥有安装、精确加载、宿主代次、revision CAS、执行 admission、操作恢复与 transcript；Work 不维护另一套 desired 配置执行器。

依赖固定在 `rpi-package` Git revision `17ba89c0098f018db76e22051fe8c73e2792dc1c`（manifest `rpi-extcore =0.2.6`，Cargo.lock 同源）。该修订包含已审核的资源契约，不能仅按版本号换成 crates.io `0.2.6`。升级需核对原生运行时支持及真实 readback；邻接 checkout 不是生产依赖。

## 四组功能

| 功能 | 当前实现 | 仍需区分 |
| --- | --- | --- |
| 原生资源管理 | 项目/全局目录、inspect、install、update、reinstall、remove、recover | 磁盘安装、已加载、模型有效集合分别观察 |
| 会话精确配置 | 主 Package、独立 Skills/扩展、持久指令映射、补充文本、tools `default/only/none` | 保存配置、实际 host generation 和可执行性分别核对 |
| 节点源码开发 | Package/Skill/extension 草稿、revision/digest、静态校验、不可变测试快照、真实测试、个人版本发布 | 校验不编译；源码 extension 不等于可加载 DLL |
| 精选资源与 Provider | 数据包发现/获取后显式安装；Provider 配置与 owned idle host release | 不是任意 URL 市场或停止外部宿主的进程管理器 |

普通新 RPI 会话在首轮前准备并选择真实 base Package，开发会话选择 developer Package。资源源码从 `crates/work-node-runtime/assets/packages` 编译嵌入，并通过内容/provenance 校验交给原生安装器；不覆盖用户拥有的同名安装。旧未配置会话保留 legacy 状态，不能按更宽默认配置伪装精确创建成功。ACP 原生资源定制返回 Unsupported。

## 应用、观测与恢复

精确配置通过 stable operation ID、expected revision 和不可变目标提交；节点保存已授权原始请求/解析后的 native intent，以便断线或重启后用原 ID 查询/重试。相同 ID 改目标冲突。客户端不能用旧 `set_active_tools` 绕过 exact 会话的资源应用。

目录、UI selection、历史配置、fresh loaded inventory、模型有效工具和 execution readiness 是不同事实。宿主代次改变后旧观察不能放行执行；能力未知、资源缺失、应用未结算或 pending/blocked operation 均保持明确状态。

忙碌、Ask、排队或取消未结算时拒绝 apply/release。释放空闲宿主保留会话身份、历史和配置，逐个访问本 owner 已拥有的 Service；跨 Service 部分完成不宣称原子成功。关闭/阻塞会话可读原生保存历史，恢复需原生重新核实后才能执行。模型工具集为零由原生 `none` 实现，不能用无效工具名模拟。

## 源码持久化与执行证据

源码/元数据存放于 `<project>/.rpi/work-authoring`。有界文件 IO 使用 retained blocking worker：即使 UI/HTTP 调用方断开，worker 仍保留 owner、authoring admission、生命周期 lease 和项目 fence，直至实际操作结束。进入原生副作用前按现有顺序释放源码 fence，避免递归获取 core 生命周期锁。

保存核对 revision/digest，先记录 journal 和旧/新内容，再替换源码与元数据；外部/model 修改会使旧验证/测试资格失效。冲突和未知结果保留编辑缓冲与恢复证据，不自动删除 journal 或重放写入。路径穿越、隐藏路径、reparse/symlink 祖先及超限源码拒绝处理；这仍不构成操作系统沙箱。

测试固定不可变 SOURCE，原生安装后创建真实 exact 会话，记录回读，再显式执行测试 prompt。只有 owned `RunTracker` 的 Completed settlement 和未改变的 binding/generation 才能通过；schema 校验、任意助手消息或配置 ACK 均不能替代。

发布要求当前源码身份、不可变快照和已通过执行证据一致，生成节点本地不可变版本。它不公开上传、不改变运行中会话，也不自动安装给其他会话。独立 Skill 的 canonical 名称/目录约束由原生机制决定，不能为绕过冲突重写已测试快照。

## 与其他扩展路径的关系

原生资源页使用 typed `native_*`/authoring 方法。旧 `WorkspaceClient::extensions()` 设计目录仍 Unsupported；该旧方法的状态不能用来否定原生管理已实现。`extension.design.json` 和旧 desired pins 继续是惰性设计，不自动转成 native 配置。

Work 自定义组件 Bundle 是独立的 UI/能力授权协议，安装面板不加载原生扩展。权限、兼容和来源分别验证，见[扩展架构](extensions.md)与[组件组合](component-composition.md)。

## 核查入口

- [节点契约](../../crates/work-node-api/src/api/node.rs)
- [资源适配](../../crates/work-node-runtime/src/workspace/resources.rs)
- [源码服务](../../crates/work-node-runtime/src/workspace/authoring.rs)
- [客户端适配](../../crates/work-client/src/resources.rs)
- [真实原生验收脚本](../../tests/e2e/native-resource-workflow.py)

真实验收须显式提供受核验的 native binary，并使用隔离 Agent/项目/Provider 根和本地模型 fixture。本文本次更新只核对源码与文档，不宣称重新执行了真实 Agent/设备验收。
