# Agent 扩展与 UI 扩展

## 当前实现范围

源码已将 Agent 能力与 UI 声明分开建模，支持 **agent-only、ui-only、combined** 三类设计资源。资源库可导入、校验、预览并原文导出；会话可保存固定版本的 Agent 扩展 **desired 配置**，UI 可见性独立保存在当前设备。

**正式应用的资源库不是可执行插件加载器。** 目前客户端未接入节点扩展安装、卸载、授权、加载、会话应用或通用动态 UI 挂载协议。本机 RPC 与远程 HTTP 的 `WorkspaceClient` 均声明不支持扩展管理；导入源码不会运行它，不会向 Agent 注入工具，也不会请求猜测的 HTTP 地址。

独立的 [真实扩展 Demo](../guides/extension-demo.md) 已在 Windows / rpi 0.11.0 实测原生 cdylib 安装、加载、工具 partial、卸载，并通过开发适配器实时更新 Dioxus 受控 widget。只有模型响应使用本地固定 fixture；这不等于正式应用已接入通用节点管理或 live UI 协商。现有 rpi 原生扩展机制不因客户端暂不支持管理而被替换。

## 职责边界

| 层 | 当前职责 | 不承担的职责 |
| --- | --- | --- |
| `work-core::extensions` | 可序列化设计契约、身份/版本、Agent/UI 独立声明、节点观测与会话 applied 类型 | Dioxus、网络、动态代码执行 |
| `work-client` | `WorkspaceClient::extension_capabilities()` 与 `extensions()` 的明确能力边界 | 把工具列表推断成已安装插件、伪造节点成功 |
| `work-ui/features/extensions` | 校验设计清单，渲染受控静态 UI 预览 | HTML/JS/CSS 插件加载、执行插件源码 |
| `work-ui/features/resources` | 不可变发布设计、desired 会话 pins、设备 UI 偏好 | 声称 desired 已经应用到节点 |
| rpi / 远程节点 | 节点侧安装、权限、加载、Agent 执行及 applied 事实来源；客户端接入前须核实协议 | 由客户端复选框直接代替运行事实 |

这些是代码职责，不是额外增加的一串网络服务。本机与远程沿用现有 [transport 架构](transport.md)。暂不引入通用 Plugin Trait 或独立插件宿主 crate。

## 一份包中的两个独立部分

`extension.design.json` 是 **RPI Work 设计格式**，不是上游 rpi 安装清单。它仍封装在 `rpi-work-design-package` 的 `design.sources` 中；外层 ID、版本与内层必须一致，Agent entry 必须在 sources 中存在。

```json
{
  "schemaVersion": 1,
  "kind": "rpi-work-extension-design",
  "id": "personal.review",
  "name": "Review",
  "version": "0.1.0",
  "agent": {
    "runtime": "rpi-native",
    "entry": "src/lib.rs",
    "tools": ["review"],
    "permissions": ["project.read"]
  },
  "ui": {
    "schemaVersion": 1,
    "contributions": [{
      "id": "status",
      "slot": "session-inspector",
      "title": "Review status",
      "widgets": [{"type": "text", "text": "静态设计预览，尚未执行"}]
    }]
  }
}
```

- 删除 `ui` 即为纯 Agent 设计；没有自定义 UI 仍可使用宿主默认工具调用卡。
- 删除 `agent` 即为纯 UI 设计；不出现 Agent 会话选择，不需要节点安装状态。
- 两个部分不能同时缺失。combined 的一份设计共享发布 ID、版本；未来独立发布的 UI 包可有自己的身份，跨包关联协议尚未实现。
- `rpi-native` 只是当前允许的 runtime 标签。entry、工具和权限都是惰性声明，不证明源码可编译、权限已授予或工具已经加载。
- 新版元Agent Extension 模板生成这份组合设计。旧 entry-only 设计继续导入和原文保留，标记为旧版惰性设计，不自动获得 Agent 或 UI 能力。

## 三种状态不可混用

1. **Desired**：`SessionBinding.extension_pins` 固定 `(id, version)`；按既有 node/session scope 隔离。同一会话同 ID 只保留一个版本，导入新版不自动升级旧绑定。修改 pin 才增加会话 revision。
2. **Observed / applied**：`NodeExtensionObservation`、`AgentExtensionState` 与 `AppliedSessionExtensions` 预留节点事实类型。目前客户端能力全为 false，不会生成虚构的安装、loaded 或 applied 记录。未知不等于未安装。
3. **UI preference**：`UiExtensionPreference` 按扩展 ID、版本、contribution ID 保存当前设备可见性。隐藏 UI 不停用 Agent、不修改会话 revision、不改变已发布源码。

资源持久化升级到 version 3，旧会话默认没有扩展 pins。恢复现代设计时从不可变包源码重新解析清单，不信任重复保存的 manifest，更不信任旧 `node_installed` 布尔值。

对话每轮的资源快照记录 `extensions.status = "desired-only"`、`nodeApplied = null` 与所选版本的 Agent 声明；丢失版本标记为 unresolved。它只是本地追踪元数据，不向 prompt 或运行工具目录注入内容。UI 偏好不进入 Agent 配置快照。

正式会话界面仅在 projection 已同步、连接在线且明确空闲时允许更改 Agent 配置；无 projection、离线/重连/同步中、发送中、请求已确认但运行事件尚未到达、运行中及等待交互时均锁定。统一判断位于 `app/runtime.rs::extension_config_locked`，组件缺省为锁定；UI 显示偏好仍可改变。尚无节点确认协议，所以此锁定也不应被当作已完成服务端并发控制。后续修改须遵循 [开发规范](../guides/development-standards.md) 的状态边界和回归清单。

## 声明式 UI 与安全限制

当前受控 slot 标签：`session-inspector`、`composer-action`、`conversation-card`、`settings-section`。它们现在只标明预期位置，**均只在资源详情/会话扩展面板静态预览，没有插入实际输入框、消息或设置页**；`composer-action` 不代表可调用动作。

当前 widget：

- `text`：纯文本；
- `fields`：受控 label/value 列表；
- `progress`：有上限的静态进度。

不允许任意 HTML、脚本、CSS、URL、组件代码、可执行 action 或事件回调。Dioxus 插值负责文本转义。清单禁止未知字段、未知 runtime/schema/widget，限制 ID、版本、路径、列表数量与文本长度，并拒绝目录穿越、重复 contribution ID 和越界进度。导入校验在修改资源库之前完成，失败不留下半份包。

当前导入遇到不支持的 UI schema 会拒绝**设计文件**，并不会卸载或禁用任何节点 Agent。未来 live 协议必须独立协商 Agent 与 UI 能力：客户端不支持某种 UI 时降级为通用展示，不能因此使已有 Agent 扩展不可执行。

## 后续接入顺序

1. 核实 rpi / 远程节点真实扩展 API 与权限语义，先接只读目录和 observed 状态。
2. 独立协商安装、卸载、加载、会话配置能力，再接 desired → applied 确认、错误、revision 与运行锁边界。
3. 在明确事件关联、数据权限和版本兼容后启用 live UI contribution；客户端只渲染受控协议，不执行节点给的前端脚本。
4. 需要纯 UI 跨包关联、设备同步或升级策略时再扩展协议，不用当前同包设计格式冒充上游安装格式。

## 验证入口

```shell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

浏览器回归：先用 `tools/review/build-module-review.ps1 -Feature all` 构建测试模块，运行 `node tools/review/ui-review-server.mjs`，然后执行 `node tests/modules/verify-extensions-module.cjs`。测试涵盖三种设计、文本转义、非法 UI 原子拒绝、desired 版本与设备 UI 隔离、会话隔离、运行锁和刷新恢复。测试模块不随正式应用发布。
