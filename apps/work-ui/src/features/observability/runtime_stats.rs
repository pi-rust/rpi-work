use crate::features::resources::session_config::NativeSessionAccess;
use crate::shared::components::{Icon, Modal};
use dioxus::{core::Task, prelude::*};
use serde_json::Value;
use std::collections::BTreeMap;
use work_client::{AgentClient, RuntimeState, SessionBranch, WorkspaceClient};
use work_core::{ConnectionState, MessageBlock, RunPhase, RunProjection, ToolStatus};

pub const CSS: &str = include_str!("runtime_stats.css");

fn number(source: Option<&Value>, keys: &[&str]) -> Option<f64> {
    let source = source?;
    keys.iter().find_map(|key| {
        source
            .get(key)
            .or_else(|| source.get("usage")?.get(key))?
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0)
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metrics {
    pub input: Option<f64>,
    pub output: Option<f64>,
    pub cached: Option<f64>,
    pub writes: Option<f64>,
    pub uncached: Option<f64>,
    pub total: Option<f64>,
    pub cache_percent: Option<f64>,
    pub cost: Option<f64>,
    pub reasoning: Option<f64>,
    pub requests: Option<f64>,
    pub retries: Option<f64>,
    pub first_token_ms: Option<f64>,
    pub speed: Option<f64>,
    pub compactions: Option<f64>,
}

impl Metrics {
    // get_state.stats uses native input (excluding cache reads/writes). An explicit
    // uncachedTokens already includes writes: never add cacheWrite a second time.
    pub fn session(source: Option<&Value>) -> Self {
        let mut metrics = Self::reported(source);
        metrics.uncached = number(source, &["uncachedTokens", "uncached_tokens"]).or_else(|| {
            number(source, &["inputTokens", "input_tokens", "input"])
                .zip(metrics.writes)
                .map(|(input, writes)| input + writes)
        });
        metrics.input = metrics
            .cached
            .zip(metrics.uncached)
            .map(|(cached, uncached)| cached + uncached);
        metrics.finish(source);
        metrics
    }

    // Only explicit settlement usage belongs here. No fallback to get_state.stats.
    pub fn turn(source: Option<&Value>) -> Self {
        let mut metrics = Self::reported(source);
        let input = number(source, &["inputTokens", "input_tokens", "input"]);
        metrics.input = input
            .zip(metrics.cached)
            .zip(metrics.writes)
            .map(|((input, cached), writes)| input + cached + writes);
        metrics.uncached = input
            .zip(metrics.writes)
            .map(|(input, writes)| input + writes);
        metrics.finish(source);
        metrics
    }

    fn reported(source: Option<&Value>) -> Self {
        Self {
            output: number(source, &["outputTokens", "output_tokens", "output"]),
            cached: number(
                source,
                &[
                    "cachedTokens",
                    "cacheReadTokens",
                    "cache_read_tokens",
                    "cacheRead",
                    "cache_read",
                ],
            ),
            writes: number(
                source,
                &[
                    "cacheWriteTokens",
                    "cache_write_tokens",
                    "cacheWrite",
                    "cache_write",
                ],
            ),
            total: number(source, &["totalTokens", "total_tokens"]),
            cost: number(source, &["costTotal", "totalCost", "cost_total"])
                .or_else(|| number(source.and_then(|v| v.get("cost")), &["total"]))
                .or_else(|| number(source, &["cost"])),
            reasoning: number(source, &["reasoningTokens", "reasoning_tokens"]),
            requests: number(source, &["requests", "requestCount"]),
            retries: number(source, &["retries", "retryCount"]),
            first_token_ms: number(source, &["firstTokenMs", "first_token_ms"]),
            speed: number(source, &["tokensPerSecond", "tokens_per_second"]),
            compactions: number(source, &["compactionCount", "compactions"]),
            ..Self::default()
        }
    }

    fn finish(&mut self, source: Option<&Value>) {
        self.output = self.output.or_else(|| {
            self.total
                .zip(self.input)
                .filter(|(total, input)| total >= input)
                .map(|(total, input)| total - input)
        });
        self.total = self
            .total
            .or_else(|| self.input.zip(self.output).map(|(i, o)| i + o));
        self.cache_percent = number(source, &["cacheHitRatio", "cache_hit_ratio"])
            .filter(|ratio| *ratio <= 1.0)
            .map(|ratio| ratio * 100.0)
            .or_else(|| {
                self.cached
                    .zip(self.input)
                    .filter(|(c, i)| *i > 0.0 && c <= i)
                    .map(|(c, i)| c / i * 100.0)
            });
    }
}

pub fn shown(value: Option<f64>) -> String {
    value.map_or_else(|| "—".into(), |n| format!("{n:.0}"))
}
pub fn tokens(value: Option<f64>) -> String {
    value.map_or_else(
        || "—".into(),
        |n| {
            if n >= 1000.0 {
                format!("{:.1}k", n / 1000.0).replace(".0k", "k")
            } else {
                format!("{n:.0}")
            }
        },
    )
}
fn grouped(value: Option<f64>) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    let digits = format!("{value:.0}");
    let mut result = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result
}
pub fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "—".into(), |n| format!("{n:.0}%"))
}
fn cost(value: Option<f64>) -> String {
    value.map_or_else(|| "—".into(), |n| format!("${n:.4}"))
}

fn compact_outcome_label(outcome: &str) -> &'static str {
    match outcome {
        "compacted" => "已完成",
        "declined" => "无需压缩",
        "aborted" => "已中止",
        "failed" => "失败",
        _ => "已结算（未知结果）",
    }
}

// Only stable call IDs from actual history/projection count as invocations.
// A result and its originating call are one invocation; configuration is ignored.
pub fn tool_counts(history: &[Value], projection: Option<&RunProjection>) -> (usize, usize) {
    let mut ids = BTreeMap::<String, bool>::new();
    for entry in history {
        let message = entry.get("message").unwrap_or(entry);
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        if matches!(role, "tool" | "toolResult" | "tool_result") {
            if let Some(id) = message
                .get("toolCallId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                let failed = message
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                *ids.entry(id.into()).or_default() |= failed;
            }
        }
        if let Some(content) = message.get("content").and_then(Value::as_array) {
            for block in content {
                if matches!(
                    block.get("type").and_then(Value::as_str),
                    Some("toolCall" | "tool_call" | "tool_use")
                ) {
                    if let Some(id) = block
                        .get("id")
                        .or_else(|| block.get("toolCallId"))
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                    {
                        ids.entry(id.into()).or_default();
                    }
                }
            }
        }
    }
    if let Some(projection) = projection {
        for block in &projection.blocks {
            if let MessageBlock::ToolCall { id, status, .. } = block {
                if !id.is_empty() {
                    *ids.entry(id.clone()).or_default() |= *status == ToolStatus::Failed;
                }
            }
        }
    }
    (ids.len(), ids.values().filter(|failed| **failed).count())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompactResult {
    pub outcome: String,
    pub summary: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Context {
    pub tokens: Option<f64>,
    pub window: Option<f64>,
    pub percent: Option<f64>,
    pub threshold: Option<f64>,
}
impl Context {
    pub fn reported(source: Option<&Value>) -> Self {
        Self {
            tokens: number(source, &["tokens"]),
            window: number(source, &["window"]),
            percent: number(source, &["percent"]).filter(|n| *n <= 100.0),
            threshold: number(source, &["threshold"]),
        }
    }
}

#[component]
pub fn RuntimeFooter(
    scope: String,
    runtime_state: Signal<Option<RuntimeState>>,
    projection: Signal<Option<RunProjection>>,
    transcript: Signal<Vec<Value>>,
    stats_open: Signal<bool>,
    context_open: Signal<bool>,
    ontools: EventHandler,
) -> Element {
    let runtime = runtime_state.read().clone();
    let metrics = Metrics::session(runtime.as_ref().and_then(|r| r.stats.as_ref()));
    let context = Context::reported(runtime.as_ref().and_then(|r| r.context.as_ref()));
    let (calls, _) = tool_counts(&transcript.read(), projection.read().as_ref());
    let used = context.percent.unwrap_or(0.0).clamp(0.0, 100.0);
    rsx! {
        footer { class: "runtime-footer", aria_label: "Agent 运行统计",
            div { class: "footer-metrics",
                button { title: "输入总量，含缓存读写；点击查看统计", onclick: move |_| stats_open.set(true), Icon { name: "diff" } span { "↑ {tokens(metrics.input)}" } span { "↓ {tokens(metrics.output)}" } }
                button { class: "cache-metric", onclick: move |_| stats_open.set(true), "缓存 " b { "{percent(metrics.cache_percent)}" } }
                button { class: "context-meter", onclick: move |_| context_open.set(true), span { class: "context-ring", style: "--used:{used}%" } "上下文 {percent(context.percent)}" }
                button { class: "cost-metric", onclick: move |_| stats_open.set(true), "{cost(metrics.cost)}" }
                button { class: "speed-metric", onclick: move |_| stats_open.set(true), "{shown(metrics.speed)} tok/s" }
                button { class: "tool-metric", onclick: move |_| ontools.call(()), "{calls} tools" }
            }
            button { class: "footer-detail", onclick: move |_| stats_open.set(true), "会话累计" Icon { name: "chevron" } }
        }
    }
}

#[allow(clippy::single_match_else)]
#[component]
pub fn RuntimeDialogs(
    scope: String,
    runtime_state: Signal<Option<RuntimeState>>,
    projection: Signal<Option<RunProjection>>,
    transcript: Signal<Vec<Value>>,
    last_turn: Signal<Option<(String, Value)>>,
    shared_client: Signal<Option<WorkspaceClient>>,
    native_agents: Signal<std::collections::HashMap<String, AgentClient>>,
    selected_session: Signal<Option<String>>,
    session_generation: Signal<u64>,
    compact_revision: Signal<u64>,
    mut compact_pending: Signal<Option<u64>>,
    node_compacting: Signal<bool>,
    mut compact_result: Signal<Option<CompactResult>>,
    pending_ask: bool,
    mut stats_open: Signal<bool>,
    mut context_open: Signal<bool>,
    ontools: EventHandler,
) -> Element {
    let mut turn_scope = use_signal(|| false);
    let mut instructions =
        use_signal(|| "保留任务目标、未完成步骤、变更文件和验证结果。".to_owned());
    let mut proposal = use_signal(|| None::<f64>);
    let mut auto_proposal = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut notice = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut preview = use_signal(|| false);
    let mut branch_summary = use_signal(|| None::<String>);
    let mut branch_error = use_signal(|| None::<String>);
    let mut branch_loading = use_signal(|| false);
    let mut branch_empty = use_signal(|| false);
    let mut summary_request_revision = use_signal(|| 0u64);
    let captured_generation = *session_generation.read();
    let captured_scope = scope.clone();
    let tasks = use_signal(Vec::<Task>::new);
    let mut observed_scope = use_signal(|| captured_scope.clone());
    let mut observed_generation = use_signal(|| captured_generation);
    // Initialize to this render's identity: the first effect must not cancel
    // requests or clear inputs submitted before it runs. Visibility is not scope.
    use_effect(use_reactive!(|scope| {
        let generation = *session_generation.read();
        let previous_scope = observed_scope.peek().clone();
        let previous_generation = *observed_generation.peek();
        if previous_scope != scope || previous_generation != generation {
            crate::shared::state::cancel_requests(tasks);
            observed_scope.set(scope.clone());
            observed_generation.set(generation);
            turn_scope.set(false);
            branch_summary.set(None);
            branch_error.set(None);
            branch_loading.set(false);
            branch_empty.set(false);
            let revision = *summary_request_revision.peek() + 1;
            summary_request_revision.set(revision);
            proposal.set(None);
            auto_proposal.set(false);
            instructions.set("保留任务目标、未完成步骤、变更文件和验证结果。".to_owned());
            notice.set(String::new());
            error.set(String::new());
            preview.set(false);
            busy.set(false);
        }
    }));
    let mut observed_compact_revision = use_signal(|| *compact_revision.read());
    use_effect(move || {
        let revision = *compact_revision.read();
        if revision != *observed_compact_revision.peek() {
            observed_compact_revision.set(revision);
            // Invalidate every in-flight branch read before clearing its UI.
            // A late response must never resurrect old summary state or leave
            // the new read permanently disabled.
            let request_revision = *summary_request_revision.peek() + 1;
            summary_request_revision.set(request_revision);
            branch_loading.set(false);
            branch_summary.set(None);
            branch_empty.set(false);
            branch_error.set(None);
        }
    });
    // Unmounting remains the final lifecycle guard for component-owned tasks.
    use_drop(move || crate::shared::state::cancel_requests(tasks));

    let runtime = runtime_state.read().clone();
    let selected_native = selected_session
        .read()
        .as_ref()
        .and_then(|id| native_agents.read().get(id).cloned());
    let access = NativeSessionAccess::for_agent(selected_native.as_ref());
    let context = Context::reported(runtime.as_ref().and_then(|r| r.context.as_ref()));
    let session_metrics = Metrics::session(runtime.as_ref().and_then(|r| r.stats.as_ref()));
    let metrics = if *turn_scope.read() {
        let turn = last_turn.read();
        Metrics::turn(
            turn.as_ref()
                .filter(|(owner, _)| owner == &scope)
                .map(|(_, usage)| usage),
        )
    } else {
        session_metrics
    };
    let (calls, failures) = tool_counts(&transcript.read(), projection.read().as_ref());
    let current = projection.read().clone();
    let running = current
        .as_ref()
        .is_some_and(|run| !matches!(run.phase, RunPhase::Ready | RunPhase::Settled(_)));
    let offline = current
        .as_ref()
        .is_none_or(|run| run.connection != ConnectionState::Online);
    let active_operation = runtime
        .as_ref()
        .and_then(|r| r.active_operation.as_ref())
        .is_some_and(|v| !v.is_null());
    let awaiting_compact = compact_pending.read().is_some();
    let blocked = running
        || pending_ask
        || active_operation
        || offline
        || *busy.read()
        || awaiting_compact
        || *node_compacting.read();
    let scope_label = if *turn_scope.read() {
        "上一轮"
    } else {
        "会话累计"
    };
    let context_percent = percent(context.percent);
    let used = context.percent.unwrap_or(0.0).clamp(0.0, 100.0);
    let threshold_percent = context
        .threshold
        .zip(context.window)
        .filter(|(_, w)| *w > 0.0)
        .map(|(t, w)| t / w * 100.0);
    let proposal_percent = proposal
        .read()
        .unwrap_or_else(|| threshold_percent.unwrap_or(80.0).round().clamp(20.0, 95.0));
    let block_reason = if *busy.read() || awaiting_compact {
        "压缩请求处理中；等待节点实际结算，请勿重复提交。可刷新节点状态。"
    } else if *node_compacting.read() {
        "节点正在压缩上下文；等待当前压缩结束后再手动提交。"
    } else if pending_ask {
        "正在等待 Ask / 工具确认；请先处理请求并结算本轮，再手动压缩。"
    } else if running || active_operation {
        "节点正在运行（可能包含自动压缩）；请等待当前操作结算，再手动压缩。"
    } else if offline {
        "会话未在线，请恢复连接后再手动压缩。"
    } else {
        "压缩由节点执行，创建摘要并保留最近上下文；累计用量与费用不会因此归零。"
    };

    let captured_scope = scope.clone();
    let compact = move |_| {
        if !access.compact
            || blocked
            || *busy.peek()
            || compact_pending.peek().is_some()
            || *node_compacting.peek()
            || *session_generation.peek() != captured_generation
            || observed_scope.peek().as_str() != captured_scope
        {
            return;
        }
        let (Some(client), Some(id)) = (
            shared_client.read().clone(),
            selected_session.read().clone(),
        ) else {
            return;
        };
        let preserve = instructions.read().clone();
        let native_agent = selected_session
            .read()
            .as_ref()
            .and_then(|id| native_agents.read().get(id).cloned());
        let submitted_revision = *compact_revision.peek();
        compact_pending.set(Some(submitted_revision));
        compact_result.set(None);
        busy.set(true);
        error.set(String::new());
        notice.set("正在请求节点压缩…".into());
        let captured_scope = captured_scope.clone();
        crate::shared::state::scoped_spawn(tasks, async move {
            let result = if let Some(agent) = native_agent.clone() {
                agent.compact(&id, Some(&preserve)).await
            } else {
                client.compact(&id, Some(&preserve)).await
            };
            if *session_generation.peek() != captured_generation
                || observed_scope.peek().as_str() != captured_scope
            {
                return;
            }
            match result {
                Err(e) => {
                    compact_pending.set(None);
                    compact_result.set(Some(CompactResult {
                        outcome: "failed".into(),
                        summary: None,
                        error: Some(e.to_string()),
                    }));
                    error.set(String::new());
                    notice.set(String::new());
                }
                Ok(_) => {
                    // HTTP accepted only starts the operation. The scoped
                    // agent_settled event is the authoritative result.
                    let refreshed = if let Some(agent) = native_agent {
                        agent.state(&id).await.and_then(|value| {
                            serde_json::from_value(value).map_err(work_client::ClientError::from)
                        })
                    } else {
                        client.runtime_state(&id).await
                    };
                    match refreshed {
                        Ok(state) => {
                            if *session_generation.peek() != captured_generation
                                || observed_scope.peek().as_str() != captured_scope
                            {
                                return;
                            }
                            runtime_state.set(Some(state));
                            notice.set(
                                "节点已接受压缩请求，已重新读取当前上下文；等待实际压缩结算。"
                                    .into(),
                            );
                        }
                        Err(e) => {
                            if *session_generation.peek() != captured_generation
                                || observed_scope.peek().as_str() != captured_scope
                            {
                                return;
                            }
                            error.set(format!("节点已响应，刷新上下文失败：{e}"));
                            notice.set(String::new());
                        }
                    }
                }
            }
            busy.set(false);
        });
    };
    let captured_scope = scope.clone();
    let load_summary = Callback::new(move |()| {
        if !access.summary {
            return;
        }
        if *session_generation.peek() != captured_generation
            || observed_scope.peek().as_str() != captured_scope
        {
            return;
        }
        let request_revision = *summary_request_revision.peek() + 1;
        summary_request_revision.set(request_revision);
        let expected_compact_revision = *compact_revision.peek();
        let native_agent = selected_session
            .read()
            .as_ref()
            .and_then(|session_id| native_agents.read().get(session_id).cloned());
        let (Some(client), Some(id)) = (
            shared_client.read().clone(),
            selected_session.read().clone(),
        ) else {
            branch_error.set(Some("尚未选择会话，无法读取完整摘要。".into()));
            return;
        };
        branch_loading.set(true);
        branch_empty.set(false);
        branch_error.set(None);
        branch_summary.set(None);
        let captured_scope = captured_scope.clone();
        crate::shared::state::scoped_spawn(tasks, async move {
            let result = if let Some(agent) = native_agent {
                agent
                    .session_branch(&id, None, None)
                    .await
                    .and_then(|value| {
                        serde_json::from_value::<SessionBranch>(value)
                            .map_err(work_client::ClientError::from)
                    })
            } else {
                client.session_branch(&id).await
            };
            if *session_generation.peek() != captured_generation
                || observed_scope.peek().as_str() != captured_scope
                || *compact_revision.peek() != expected_compact_revision
                || *summary_request_revision.peek() != request_revision
            {
                return;
            }
            match result {
                Ok(branch) => match branch
                    .latest_compaction()
                    .and_then(|entry| entry.summary)
                    .filter(|text| !text.trim().is_empty())
                {
                    Some(summary) => branch_summary.set(Some(summary)),
                    None => branch_empty.set(true),
                },
                Err(error) => {
                    let message = error.to_string();
                    let unsupported = message.contains("unknown command")
                        || message.contains("unsupported")
                        || message.contains("not supported");
                    branch_error.set(Some(if unsupported {
                        format!("不支持读取完整摘要，请更新执行节点：{message}")
                    } else {
                        format!("读取摘要失败：{message}")
                    }));
                }
            }
            branch_loading.set(false);
        });
    });
    let captured_scope = scope.clone();
    let refresh = move |_| {
        if !access.state
            || *busy.peek()
            || *session_generation.peek() != captured_generation
            || observed_scope.peek().as_str() != captured_scope
        {
            return;
        }
        let native_agent = selected_session
            .read()
            .as_ref()
            .and_then(|session_id| native_agents.read().get(session_id).cloned());
        let (Some(client), Some(id)) = (
            shared_client.read().clone(),
            selected_session.read().clone(),
        ) else {
            return;
        };
        busy.set(true);
        let captured_scope = captured_scope.clone();
        crate::shared::state::scoped_spawn(tasks, async move {
            let result = if let Some(agent) = native_agent {
                agent.state(&id).await.and_then(|value| {
                    serde_json::from_value(value).map_err(work_client::ClientError::from)
                })
            } else {
                client.runtime_state(&id).await
            };
            if *session_generation.peek() != captured_generation
                || observed_scope.peek().as_str() != captured_scope
            {
                return;
            }
            match result {
                Ok(state) => {
                    runtime_state.set(Some(state));
                    notice.set("已重新读取节点上下文。".into());
                }
                Err(e) => error.set(format!("刷新失败：{e}")),
            }
            busy.set(false);
        });
    };

    rsx! {
        if *stats_open.read() {
            Modal { id: "runtime-stats", title: "用量与运行统计", class: "runtime-modal".to_owned(), onclose: move |()| stats_open.set(false),
                div { class: "modal-body",
                    div { class: "stats-segmented", aria_label: "用量统计范围",
                        for (is_turn, label) in [(true, "上一轮"), (false, "会话累计")] {
                            button { class: if *turn_scope.read() == is_turn { "active" } else { "" }, aria_pressed: *turn_scope.read() == is_turn, onclick: move |_| turn_scope.set(is_turn), "{label}" }
                        }
                    }
                    p { class: "stats-caption", "{scope_label} · 节点上报；未上报的统计显示 —" }
                    div { class: "metric-grid",
                        div { small { "输入总量" } strong { "{grouped(metrics.input)}" } span { "含缓存读取与写入" } }
                        div { small { "输出 Token" } strong { "{grouped(metrics.output)}" } span { "思考 {grouped(metrics.reasoning)}，包含在输出内" } }
                        div { small { "缓存命中率" } strong { "{percent(metrics.cache_percent)}" } span { "按输入 token 计算" } }
                        div { small { "费用 · USD" } strong { "{cost(metrics.cost)}" } span { "节点上报，非账单" } }
                    }
                    dl { class: "stats-details",
                        for (label, value) in [
                            ("未缓存输入", grouped(metrics.uncached)),
                            ("缓存读取 / 写入", format!("{} / {}", grouped(metrics.cached), grouped(metrics.writes))),
                            ("输入 + 输出总量", grouped(metrics.total)),
                            ("模型请求 / 重试", format!("{} / {}", shown(metrics.requests), shown(metrics.retries))),
                            ("首 token 延迟 / 生成速度", format!("{} / {}", metrics.first_token_ms.map_or_else(|| "—".into(), |n| format!("{:.2}s", n / 1000.0)), metrics.speed.map_or_else(|| "—".into(), |n| format!("{n:.1} tok/s")))),
                            ("工具调用 / 失败", if *turn_scope.read() { "— / —".into() } else { format!("{calls} / {failures}") }),
                            ("上下文压缩", shown(metrics.compactions)),
                        ] { div { dt { "{label}" } dd { "{value}" } } }
                    }
                    div { class: "runtime-info", "命中率 = 缓存读取 ÷ 输入总量。累计比例按 token 加权；缓存读写与思考量不会重复相加。上一轮仅展示显式上报的本轮用量。" }
                }
                div { class: "runtime-modal-actions", button { class: "button", onclick: move |_| { stats_open.set(false); preview.set(false); context_open.set(true); }, "查看上下文" } }
            }
        }
        if *context_open.read() {
            Modal { id: "runtime-context", title: "上下文与压缩", class: "runtime-modal".to_owned(), onclose: move |()| { context_open.set(false); preview.set(false); },
                div { class: "modal-body",
                    if *preview.read() {
                        if let Some(result) = compact_result.read().as_ref() {
                            p { class: "runtime-outcome", "压缩结算：{compact_outcome_label(&result.outcome)}" }
                            if let Some(message) = result.error.as_ref() { p { class: "error-text", "{message}" } }
                        } else {
                            p { class: "runtime-outcome", "压缩结算：尚无本次压缩结算记录" }
                        }
                        if let Some(summary) = branch_summary.read().as_ref() {
                            pre { class: "runtime-summary", "{summary}" }
                        } else if let Some(message) = branch_error.read().as_ref() {
                            p { class: "error-text", "{message}" }
                        } else if *branch_empty.read() {
                            p { class: "runtime-help", "尚无可展示摘要。" }
                        } else if *branch_loading.read() {
                            p { class: "runtime-help", "正在读取当前分支的公开压缩摘要…" }
                        } else {
                            p { class: "runtime-help", "点击刷新摘要读取当前分支最新公开压缩摘要。" }
                        }
                        button { class: "button", disabled: !access.summary || *branch_loading.read(), onclick: move |_| load_summary.call(()), if branch_error.read().is_some() { "重试" } else { "刷新摘要" } }
                        button { class: "button", onclick: move |_| preview.set(false), "返回上下文" }
                    } else {
                        div { class: "context-hero",
                            div { small { "当前上下文 · 节点报告" } strong { "{tokens(context.tokens)} " span { "/ {tokens(context.window)}" } } }
                            span { "{context_percent}" }
                        }
                        div { class: "runtime-context-track", aria_label: "当前上下文占比 {context_percent}",
                            if context.percent.is_some() { span { style: "width:{used}%" } }
                        }
                        div { class: "runtime-settings-row",
                            div { strong { "自动压缩" } small { "仅本地提案 · 节点状态未报告" } }
                            button { class: "runtime-auto-switch", r#type: "button", role: "switch", aria_checked: *auto_proposal.read(), aria_label: "自动压缩 · 本地提案", onclick: move |_| auto_proposal.toggle(), span { aria_hidden: "true" } }
                        }
                        if let Some(result) = compact_result.read().as_ref() {
                            p { role: "status", class: "runtime-outcome", "最近一次压缩结算：{compact_outcome_label(&result.outcome)}" }
                            if let Some(message) = result.error.as_ref() { p { role: "alert", class: "error-text", "{message}" } }
                        }
                        div { class: "runtime-field runtime-threshold-field",
                            label { r#for: "runtime-threshold", "触发阈值" span { "{proposal_percent:.0}% · 本地提案" } }
                            input { id: "runtime-threshold", r#type: "range", min: "20", max: "95", step: "1", value: "{proposal_percent}", aria_label: "触发阈值 · 本地提案", oninput: move |e| { if let Ok(value) = e.value().parse::<f64>() { if value.is_finite() { proposal.set(Some(value.round().clamp(20.0, 95.0))); } } } }
                        }
                        details { class: "runtime-proposal-details",
                            summary { "节点报告与提案说明" }
                            p { "实际报告阈值：{grouped(context.threshold)} tokens（{percent(threshold_percent)}）。" }
                            if threshold_percent.is_some_and(|value| !(20.0..=95.0).contains(&value)) {
                                p { "实际阈值超出滑块范围；本地初始提案已限制在 20–95%。" }
                            } else if threshold_percent.is_none() {
                                p { "节点未报告阈值百分比；本地初始提案为 80%。" }
                            }
                            p { "节点没有自动压缩配置读写 API；提案不会发送到节点，也不改变上下文用量或手动压缩指令。可用底部“刷新状态”重新读取节点。" }
                        }
                        div { class: "runtime-field",
                            label { r#for: "compact-preserve", "需要保留的内容" }
                            textarea { id: "compact-preserve", rows: "2", value: "{instructions}", oninput: move |e| instructions.set(e.value()) }
                        }
                        p { class: "runtime-help", "{block_reason}" }
                        if compact_result.read().is_none() && !notice.read().is_empty() { p { role: "status", class: "runtime-help", "{notice}" } }
                        if !error.read().is_empty() { p { role: "alert", class: "error-text", "{error}" } }
                    }
                }
                if selected_native.is_some() && (!access.state || !access.summary || !access.compact) {
                    p { class: "runtime-help", "当前原生 transport 未提供的状态查询、完整摘要或压缩操作已禁用；未上报的数据保持未知。" }
                }
                if !*preview.read() {
                    div { class: "runtime-modal-actions",
                        button { class: "button", disabled: !access.summary, onclick: move |_| { preview.set(true); load_summary.call(()); }, "查看摘要" }
                        button { class: "button", disabled: !access.state || *busy.read() || offline, onclick: refresh, "刷新状态" }
                        button { class: "button primary", disabled: !access.compact || blocked, onclick: compact, if *busy.read() || awaiting_compact { "等待节点…" } else { "手动压缩" } }
                    }
                }
            }
        }
    }
}
