# 已迁移：RPI Work 工作台

唯一源码与构建入口已迁至 [`apps/workbench-ui`](../../apps/workbench-ui)。本目录仅保留历史链接说明，不再提供应用源码、运行时依赖或正式构建产物。

确认的视觉/交互基准仍是 [`design/reui-message-layout`](../reui-message-layout)，完整迁移范围与未完成项见[迁移清单](../../docs/design/confirmed-workbench-migration.md)。

产品构建不含样例；独立fixture构建还需显式 `?fixture=1`。迁出目录不代表整套1:1、权限/结算或Native/Android已完成，旧 `apps/work-ui` 未改动。
