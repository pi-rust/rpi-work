# Agent 扩展与 UI 扩展

## 三种不同的扩展机制

当前源码同时存在原生资源管理、动态组件面板与惰性设计资源。它们分别有自己的契约，不能用一个“已安装/已启用”状态代表全部能力。

| 机制 | 格式与入口 | 当前能力 | 不代表什么 |
| --- | --- | --- | --- |
| 原生 Agent 资源 | rpi 原生 Package / Skill / 扩展；资源页与会话资源配置 | typed `native_*` 管理、精确配置、实际 readback、恢复及源码测试/本地发布 | 安装不等于本会话 loaded；旧设计 pin 不等于 applied |
| Work 自定义组件 | `work-component-api::Bundle`；设置 → 节点组件 | 内置/HTTPS 下载/粘贴、授权、安装/卸载、按需加载自定义 HTML 面板 | 不安装 DLL、不选择节点宿主；不授予父页面 DOM 或网络凭据 |
| Work 惰性设计 | `rpi-work-design-package` 内的 `extension.design.json` | Agent-only / UI-only / combined、校验、静态预览、原文导出及旧 desired pins | 不编译、安装、加载或执行源码 |

原生闭环见[资源架构](resources.md)与[使用指南](../guides/native-resource-authoring.md)；自定义 UI 的隔离/动作/授权协议见[组件组合](component-composition.md)。下文的 schema、pins、静态 widget 和旧 applied 类型均专指**惰性设计**，不是整个应用的原生或动态 UI 能力上限。

`WorkspaceClient::extensions()` / `extension_capabilities()` 仍是未接通的旧设计目录接口，返回 Unsupported/默认 false。正式原生管理使用另一组已实现的 typed `native_catalog`、`native_manage`、`native_apply` 等操作，不从工具列表推测插件状态。组件包使用独立组件目录，不经该旧接口。

[真实扩展 Demo](../guides/extension-demo.md)保留其特定 Windows/rpi 版本下的 ABI 与受控 widget 验证；它不是正式原生资源页、组件 Bundle 或 Work 节点协议的实现来源。

## 职责边界

| 层 | 当前职责 |
| --- | --- |
| `work-node-api` | 原生资源、authoring 和 typed NodeApi 契约，不持有执行宿主 |
| `work-node-runtime` / extcore / rpi | Work 来源/项目校验和记录；原生安装、精确配置、加载与执行事实 |
| `work-core::extensions` | 惰性设计身份、Agent/UI 声明、旧 observed/applied 预留类型 |
| `work-component-api` / `work-http-component` | 动态 UI 包协议和专用远程访问面板；能力最终由宿主绑定与检查 |
| `work-client` | 本地/远程 typed 适配、能力观察与状态恢复 |
| UI 的 `features/extensions` / `features/resources` | 惰性设计预览、原生资源交互与节点组件组合，各走对应契约 |

原生 Agent 扩展无需自定义面板，默认工具卡仍可工作；UI 包可以独立安装，但宿主未提供/未授予的能力不能被调用。详细物理模块见[总览](overview.md)。

## 惰性设计格式：一份包中的两个独立部分

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

## 惰性设计的三种状态不可混用

1. **Desired**：`SessionBinding.extension_pins` 固定 `(id, version)`；按既有 node/session scope 隔离。同一会话同 ID 只保留一个版本，导入新版不自动升级旧绑定。修改 pin 才增加会话 revision。
2. **Observed / applied**：`NodeExtensionObservation`、`AgentExtensionState` 与 `AppliedSessionExtensions` 预留节点事实类型。这组旧设计目录接口的能力全为 false，不会生成虚构的安装、loaded 或 applied 记录。未知不等于未安装。
3. **UI preference**：`UiExtensionPreference` 按扩展 ID、版本、contribution ID 保存当前设备可见性。隐藏 UI 不停用 Agent、不修改会话 revision、不改变已发布源码。

资源持久化升级到 version 3，旧会话默认没有扩展 pins。恢复现代设计时从不可变包源码重新解析清单，不信任重复保存的 manifest，更不信任旧 `node_installed` 布尔值。

对话每轮的资源快照记录 `extensions.status = "desired-only"`、`nodeApplied = null` 与所选版本的 Agent 声明；丢失版本标记为 unresolved。它只是本地追踪元数据，不向 prompt 或运行工具目录注入内容。UI 偏好不进入 Agent 配置快照。

正式会话界面仅在 projection 已同步、连接在线且明确空闲时允许更改 Agent 配置；无 projection、离线/重连/同步中、发送中、请求已确认但运行事件尚未到达、运行中及等待交互时均锁定。统一判断位于 `app/runtime.rs::extension_config_locked`，组件缺省为锁定；UI 显示偏好仍可改变。旧设计 pins 尚无节点确认协议，此 UI 锁不能证明服务端应用；原生 exact 配置另走已实现的 revision/admission 与恢复契约。后续修改须遵循 [开发规范](../guides/development-standards.md) 的状态边界和回归清单。

## 惰性设计的声明式 UI 与安全限制

当前受控 slot 标签：`session-inspector`、`composer-action`、`conversation-card`、`settings-section`、`runtime-footer`。前四个现在只标明预期位置，**均只在资源详情/会话扩展面板静态预览，没有插入实际输入框、消息或设置页**；`composer-action` 不代表可调用动作。

`runtime-footer` 是受控 Host 数据契约的声明入口，不代表当前客户端已经挂载 live UI。使用它的 contribution 必须声明 `host.capability: "runtime-footer"` 和有限的 `host.fields`，不能请求任意运行时路径。

当前 widget：

- `text`：纯文本；
- `fields`：受控 label/value 列表；
- `progress`：有上限的静态进度；
- `host-value`：只读 Host 字段的设计声明，必须引用同一 contribution 的 `host.fields`，当前正式应用仍只显示为设计预览。

可声明的 Host 字段是固定枚举，例如 `session-model`、`usage-input-tokens`、`context-used-percent`、`tools-calls`；可声明的视觉形式是 `text`、`badge`、`count`、`context-ring`。这套字段不是 Tool 列表，也不证明 Agent 扩展已安装、授权或加载。客户端内部的 `UiContributionEntry` 只接受已校验清单作为 Host 输入；`FooterHost` 再按 `AppSettings.contributions.visible/order` 过滤和排序。工作区当前只把资源库中的已校验 manifest 作为显式 `design_only` 预览输入传给 Footer；预览项带有“设计预览”标记，仍不表示节点已安装、授权或加载。没有节点/Host 提供的 contribution catalog 时，不会把资源包当作正式 live plugin。

此设计 schema 不允许任意 HTML、脚本、CSS、URL、组件代码、可执行 action 或事件回调。自定义 HTML 仅通过独立 Bundle 的隔离 frame 协议加载，不插入父页面。Dioxus 插值负责文本转义。清单禁止未知字段、未知 runtime/schema/widget，限制 ID、版本、路径、列表数量与文本长度，并拒绝目录穿越、重复 contribution ID 和越界进度。导入校验在修改资源库之前完成，失败不留下半份包。

当前导入遇到不支持的 UI schema 会拒绝**设计文件**，并不会卸载或禁用任何节点 Agent。未来 live 协议必须独立协商 Agent 与 UI 能力：客户端不支持某种 UI 时降级为通用展示，不能因此使已有 Agent 扩展不可执行。

## 惰性设计与原生 / live contribution 的后续对接

原生安装与精确配置已经实现，后续若将旧设计资源接到正式执行，应显式解析为受支持的原生来源并重新授权，不能将旧 desired pins 或设计声明当作 applied 证据。

受控 `runtime-footer` contribution 仍是设计预览；通用 Agent live contribution、跨包 Agent/UI 关联、设备同步和自动升级不是已完成能力。需要时单独协商来源、会话关联、数据权限及版本兼容；不能从 Tool 列表推断自定义数据，也不能把通用组件面板协议冒充所有 contribution slot 都已实现。客户端不支持自定义 UI 时仍应保留纯 Agent 的通用展示。

## 验证入口

```shell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

浏览器回归：先用 `tools/review/build-module-review.ps1 -Feature all` 构建测试模块，运行 `node tools/review/ui-review-server.mjs`，然后执行 `node tests/modules/verify-extensions-module.cjs`。测试涵盖三种设计、文本转义、非法 UI 原子拒绝、desired 版本与设备 UI 隔离、会话隔离、运行锁和刷新恢复。测试模块不随正式应用发布。
