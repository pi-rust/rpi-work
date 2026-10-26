# 会话追踪与耗时

结构参考 [Langfuse 的会话、Trace 与 Observation 数据模型](https://langfuse.com/docs/observability/data-model)：一个会话包含多个回合，一次请求对应一个 Trace，模型调用、工具、上下文准备和运行时结算等步骤形成有父子关系的 Observation。

## 交互

- 输入框下方的「追踪 · 耗时」打开当前会话追踪弹窗；每条流式回复也有本轮追踪入口，不增加顶部导航高度或独立任务标签。
- 桌面左侧切换历史回合，右侧显示分层瀑布图；手机使用回合选择器和纵向详情。点选步骤查看概览、输入/输出、错误和元数据。
- 回合展示总耗时、首段延迟、输入/输出 Token、费用。步骤展示类型、开始偏移和持续时间。并行工具保留父子结构和重叠时间轴，回合总耗时不将子步骤耗时相加。
- 初始会话包含明确标记的历史示例，演示嵌套调用、失败工具、缓存与费用。其他会话从空状态开始。

## 当前数据口径

历史示例的计时、用量和费用都是样例；本地流式演示按 `performance.now()` 记录事件，保存每个事件的原始时间戳。它反映模拟器事件生成的时间，不代表真实模型耗时或移动端接收延迟。未接入 Langfuse、OpenTelemetry、真实模型或 rpi 遥测。

回放时以回合 ID 与序号去重，使用原始事件时间，不将重放时刻当作新执行。`agent_end` 保持等待结算，收到 `agent_settled` 才固定根 Trace 的状态与时长。停止、失败和挂起使用各自的结果语义；重置场景清除本地回合追踪。

「首段延迟」为回合开始到首次文本片段的时间，「首段等待」在模型阶段详情中以该阶段开始为基准。两者都不是生产模型的 tokenizer TTFT。当前模拟器没有模型 Token、成本遥测，始终显示 `—`；不会借用上一轮页脚统计。历史示例只合计叶子模型调用的用量，缓存读取已包含在输入总量中，命中率为缓存读取 / 输入总量。

本地压缩操作作为独立 Trace 记录本地状态操作耗时及前后上下文规模，明确注明预置摘要、未调用模型，不构造虚假的模型调用时长。

输入/输出仅保留用户提供的消息、公开状态摘要和模拟工具返回；不记录内部推理链、Provider 密钥或认证配置。详情使用文本渲染。实时更新不替换输入框、整段对话或用户正在阅读的详情节点。

## 正式框架边界

PC 的 rpi 原生宿主应产生真实模型请求、工具和压缩的起止事件；rpi-web 负责按会话鉴权、事件恢复和历史查询；work-core 维护 `SessionTrace / TraceObservation / TraceUsage` 投影；Dioxus 客户端展示与查询投影。导出到 Langfuse 或其他 OpenTelemetry 服务由 PC 端可选 exporter 完成，手机只读取授权的追踪信息。

必要字段为 `session_id / run_id / trace_id / observation_id / parent_id / event_id / seq / kind / status / start_time / end_time / provider / model / usage / error / source`。必须区分宿主执行时间和客户端接收时间，累计用量仅由权威模型调用统计；恢复快照按 ID 替换，不能重复累加费用。

## 验证

`node scripts/verify-traces.cjs` 覆盖分层和并行时间线、用量汇总、失败详情、原文转义、流式更新、重放去重、等待结算、停止/失败、本地压缩、会话隔离、320–1440px、手机预览与焦点恢复。结果：[traces-validation.json](../artifacts/traces-validation.json)；截图：[桌面](../artifacts/prototype-session-trace-desktop.png)、[手机](../artifacts/prototype-session-trace-mobile.png)。
