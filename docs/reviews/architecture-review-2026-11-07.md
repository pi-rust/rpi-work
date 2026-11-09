# 架构文档审查：2026-11-07

## 范围与结论

核对工作区 manifest、兼容导出、宿主启动/退出、owner、客户端、HTTP/realtime、组件授权与原生资源入口，并更新架构、导航和直接关联指南。此次只改文档，不修改运行代码，不重启已有节点，不发布版本。

当前架构是 Work 拥有节点与多项目 workspace、客户端共享同一应用语义、rpi/extcore 提供原生执行与资源能力。动态组件面板独立于 Agent 插件 ABI。此前 DLL 宿主耦合已撤回，不应继续用其执行日志说明当前系统。

## 已修正的文档问题

| 发现 | 对阅读/开发的影响 | 更新 |
| --- | --- | --- |
| 把 `work-node` / `work-server` 写成实际实现目录，漏列私有拆分包 | 修改位置和依赖方向错误 | 总览列出 15 个成员，区分 API、runtime、service、host 与兼容门面 |
| 只说明 HTTP/SSE | WebSocket-only 实时通道、认证发现和未知写结果没有说明 | 增加 realtime 专题，写明无降级、无自动重发与 owner 回执边界 |
| 扩展文档将全局能力描述为 false、禁止一切 HTML | 混淆旧惰性设计接口、已实现原生管理和动态组件 | 分开 native、Bundle、design 三条契约 |
| 原生资源/元 Agent 仍被描述为未实现或只有静态测试 | 与 typed native/authoring 源码和正式指南冲突 | 增加资源架构并修正 README、产品范围、归属与规范 |
| `task dev` 被描述为桌面 + 14180 热刷新 | 按文档启动得不到热更新，还可能重复抢 owner 锁 | 按 Taskfile 区分 release 无头 `task dev` 和 debug `task dev:desktop` |
| 规范仍强制 extcore crates.io `0.1.5` | 可能回退已审核资源契约 | 写明当前 `=0.2.6` + immutable Git revision，禁止仅按版本号替换来源 |
| 迁移日志混杂有效/撤回方案，组件 README 保留已删除 launcher | 易误用宿主 DLL、旧命令或无模型启动语义 | 执行日志归档、旧路径保留导航、修正组件说明 |

## 源码中的架构限制

以下是当前边界，文档更新不将其标为已解决：

- `work-node` / `work-server` 兼容导出与 runtime/service 实现同时存在。新增实现应进入实际 crate；集成测试和 examples 仍保留在 facade 包，不能只测试空导出层就宣称覆盖实现。
- 内置 Package 源码在根 `assets/packages` 与 runtime crate 的 `assets/packages` 各有一份，当前六个文件逐字节一致，编译实际使用后者。尚无统一生成/同步入口；后续修改须核对两处，避免文档示例与运行内容漂移。本次不借文档审查改动资源布局。
- 原生资源依赖固定 Git revision 的 extcore；manifest 版本号与 crates.io 同版本不能证明内容等价。后续换成发行版需要重新审核资源契约和 lockfile。
- Work 网络协议与 rpi HTTP API 分别演进，尚不能自动互通。原生资源与设计目录、动态 Bundle、受控 live contribution 也没有统一成一种安装/应用协议。
- realtime 回执为内存有界状态，有 owner epoch 和过期限制；它不是持久化 exactly-once。原生资源操作的持久恢复仍由 core 独立负责。
- 地址隔离尚未升级为 node ID 多入口归一或同时在线的多节点调度；共享密码/短期 App 授权不提供角色、项目或逐设备 ACL。
- Bundle 摘要证明内容一致性，未提供发布者签名；隔离 UI 的能力授权不能约束原生扩展全部副作用。源码 authoring fence 也不构成 OS 文件系统沙箱。

## 核查与验证范围

文档更新已按本地 manifest 和源码核对。结构检查、节点依赖边界检查、4 项结构规则测试和差异空白检查通过；相对链接另作全量核查（包括新增页面，生成产物链接单独识别）。

常用回归入口：

```text
node tools/review/check-structure.cjs
node tools/review/check-node-boundaries.cjs
node --test tests/structure/check-structure.test.cjs
git diff --check
```

本次不重复构建 Rust/WASM，不调用模型，不进行 native/浏览器/Android 真机验收。历史全量测试和发布验证仅属于各自构建；当前文档的流程说明不是新一轮测试通过声明。需要修改运行行为时，再按[测试指南](../guides/testing-artifacts.md)执行对应层级。

## 维护入口

[架构总览](../architecture/overview.md)是模块与所有权入口；[传输](../architecture/transport.md)、[实时通道](../architecture/realtime.md)、[资源](../architecture/resources.md)、[组件](../architecture/component-composition.md)、[扩展](../architecture/extensions.md)各维护自己的契约。使用命令归 `guides/`，提案和原型明确标记，迁移经过归 `archive/`。
