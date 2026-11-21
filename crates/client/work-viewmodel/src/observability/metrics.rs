//! Runtime metrics and their portable formatting rules.
//!
//! Extracted verbatim from the UI crate's `runtime_stats.rs`; the widget
//! components stayed behind.

use serde_json::Value;
use std::collections::BTreeMap;
use work_core::{MessageBlock, RunProjection, ToolStatus};

// Epochs invalidate observations, not the lifetime of a same-scope write.
pub fn observation_matches(
    expected: (&str, Option<&str>, u64, u64),
    current: (&str, Option<&str>, u64, u64),
) -> bool {
    expected == current
}

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
pub fn grouped(value: Option<f64>) -> String {
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
pub fn cost(value: Option<f64>) -> String {
    value.map_or_else(|| "—".into(), |n| format!("${n:.4}"))
}

pub fn compact_outcome_label(outcome: &str) -> &'static str {
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
