//! wasm-bindgen surface for `work-viewmodel`.
//!
//! The Web prototype must not re-implement transcript ordering, tool labels,
//! code tokenizing or metric formatting in TypeScript: that would create a
//! second source of truth next to the Dioxus client. This crate exposes the
//! Rust rules as plain JSON functions, so React only owns layout.
//!
//! Every function is synchronous and allocation-bounded by the same limits the
//! desktop client uses.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use work_core::{MessageBlock, ToolStatus};
use work_viewmodel::conversation::{code_tokens, tool_display};
use work_viewmodel::observability::metrics::{Context, Metrics, percent, tokens};
use work_viewmodel::theme::theme_contract;
use work_viewmodel::transcript::{RowSource, TranscriptOrder};

/// Wire shape for one projection block. Kept separate from `MessageBlock` so
/// the JS contract is stable even when the internal enum grows.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum BlockInput {
    Text {
        id: String,
        text: String,
    },
    Thinking {
        id: String,
        text: String,
    },
    Summary {
        id: String,
        text: String,
    },
    Tool {
        id: String,
        name: String,
        arguments: String,
        output: String,
        #[serde(default)]
        status: String,
    },
}

impl BlockInput {
    fn into_block(self) -> MessageBlock {
        match self {
            Self::Text { id, text } => MessageBlock::Text { id, text },
            Self::Thinking { id, text } => MessageBlock::Thinking { id, text },
            Self::Summary { id, text } => MessageBlock::PublicSummary { id, text },
            Self::Tool {
                id,
                name,
                arguments,
                output,
                status,
            } => MessageBlock::ToolCall {
                id,
                name,
                arguments,
                output,
                status: match status.as_str() {
                    "failed" => ToolStatus::Failed,
                    "running" => ToolStatus::Running,
                    "cancelled" => ToolStatus::Cancelled,
                    _ => ToolStatus::Completed,
                },
            },
        }
    }
}

/// Stateful presenter: one `TranscriptOrder` per session scope, so display
/// slots stay stable while history and the run projection grow independently.
#[wasm_bindgen]
pub struct Presenter {
    orders: HashMap<String, TranscriptOrder>,
}

#[wasm_bindgen]
impl Presenter {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
        }
    }

    /// Stable display rows for one scope.
    ///
    /// `historyJson` is the canonical message array the node returns, and
    /// `blocksJson` the run projection (`[{kind:"text",...}]`). Returns
    /// `[{key, source:"history"|"projection", index}]` in display order.
    pub fn rows(&mut self, scope: &str, history_json: &str, blocks_json: &str) -> String {
        let history: Vec<Value> = serde_json::from_str(history_json).unwrap_or_default();
        let inputs: Vec<BlockInput> = serde_json::from_str(blocks_json).unwrap_or_default();
        let blocks: Vec<MessageBlock> = inputs.into_iter().map(BlockInput::into_block).collect();
        let order = self
            .orders
            .entry(scope.to_string())
            .or_default();
        let rows = order.update(scope, &history, &blocks);
        let payload: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                let (source, index) = match row.source {
                    RowSource::History(index) => ("history", index),
                    RowSource::Projection(index) => ("projection", index),
                };
                json!({ "key": row.key, "source": source, "index": index })
            })
            .collect();
        serde_json::to_string(&payload).unwrap_or_else(|_| "[]".into())
    }

    /// Drop retained slots for a scope (node switch, session reset).
    pub fn forget(&mut self, scope: &str) {
        self.orders.remove(scope);
    }
}

impl Default for Presenter {
    fn default() -> Self {
        Self::new()
    }
}

/// Short human label for a tool call, e.g. `src/theme.rs` or a shell command.
#[wasm_bindgen]
pub fn tool_target(name: &str, arguments_json: &str) -> String {
    let args = serde_json::from_str::<Value>(arguments_json).unwrap_or(Value::Null);
    tool_display::target(name, &args.to_string())
}

#[wasm_bindgen]
pub fn tool_icon(name: &str) -> String {
    tool_display::icon(name).to_string()
}

#[wasm_bindgen]
pub fn tool_is_read(name: &str) -> bool {
    tool_display::is_read(name)
}

#[wasm_bindgen]
pub fn tool_is_shell(name: &str) -> bool {
    tool_display::is_shell(name)
}

/// Per-line tokens for the shared lexer: `[[{text, class}]]`.
#[wasm_bindgen]
pub fn tokenize_file(source: &str, path: &str) -> String {
    let lines: Vec<Vec<Value>> = code_tokens::tokenize_file(source, path)
        .into_iter()
        .map(|line| {
            line.into_iter()
                .map(|token| json!({ "text": token.text, "class": token.class }))
                .collect()
        })
        .collect();
    serde_json::to_string(&lines).unwrap_or_else(|_| "[]".into())
}

/// Footer metrics in the same shape the Dioxus footer renders.
#[wasm_bindgen]
pub fn metrics_session(stats_json: &str) -> String {
    let stats = serde_json::from_str::<Value>(stats_json).ok();
    let metrics = Metrics::session(stats.as_ref());
    json!({
        "input": metrics.input,
        "output": metrics.output,
        "cached": metrics.cached,
        "writes": metrics.writes,
        "uncached": metrics.uncached,
        "total": metrics.total,
        "cachePercent": metrics.cache_percent,
        "cost": metrics.cost,
        "requests": metrics.requests,
        "retries": metrics.retries,
        "speed": metrics.speed,
        "inputText": tokens(metrics.input),
        "outputText": tokens(metrics.output),
        "cacheText": percent(metrics.cache_percent),
    })
    .to_string()
}

/// Context window readout for the same node state payload.
#[wasm_bindgen]
pub fn context_reported(context_json: &str) -> String {
    let context = serde_json::from_str::<Value>(context_json).ok();
    let context = Context::reported(context.as_ref());
    json!({
        "used": context.tokens,
        "window": context.window,
        "percent": context.percent,
        "threshold": context.threshold,
        "percentText": percent(context.percent),
    })
    .to_string()
}

#[wasm_bindgen]
pub fn format_tokens(value: Option<f64>) -> String {
    tokens(value)
}

#[wasm_bindgen]
pub fn format_percent(value: Option<f64>) -> String {
    percent(value)
}

/// Theme token map (`--bg`, `--accent`, ...) for a preset, as JSON.
#[wasm_bindgen]
pub fn theme_tokens(theme_id: &str, dark: bool, compact: bool) -> String {
    let Some(theme) = theme_contract::presets()
        .into_iter()
        .find(|theme| theme.id == theme_id)
    else {
        return "{}".into();
    };
    let map = theme_contract::build_tokens(&theme, dark, compact);
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}

/// Preset ids + names, so a renderer can offer a theme picker without
/// hardcoding the catalogue.
#[wasm_bindgen]
pub fn theme_presets() -> String {
    let presets: Vec<Value> = theme_contract::presets()
        .into_iter()
        .map(
            |theme| json!({ "id": theme.id, "name": theme.name, "description": theme.description }),
        )
        .collect();
    serde_json::to_string(&presets).unwrap_or_else(|_| "[]".into())
}

/// Address normalization used before any connection attempt.
#[wasm_bindgen]
pub fn normalize_address(raw: &str, phone: bool) -> Option<String> {
    work_viewmodel::connection::normalize_address(raw, phone).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_merge_history_and_projection_in_display_order() {
        let mut presenter = Presenter::new();
        let history = json!([{ "id": "user-1", "role": "user", "content": "hi" }]).to_string();
        let blocks = json!([
            { "kind": "text", "id": "answer", "text": "hello" },
            { "kind": "tool", "id": "tool-1", "name": "read", "arguments": "{\"path\":\"a.rs\"}", "output": "x", "status": "completed" }
        ])
        .to_string();
        let rows = presenter.rows("scope", &history, &blocks);
        let rows: Vec<Value> = serde_json::from_str(&rows).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["source"], "history");
        assert_eq!(rows[1]["source"], "projection");
        assert_eq!(rows[2]["source"], "projection");
        // Keys stay stable when the same input is presented again.
        let again: Vec<Value> =
            serde_json::from_str(&presenter.rows("scope", &history, &blocks)).unwrap();
        assert_eq!(rows, again);
    }

    #[test]
    fn tool_target_and_tokens_come_from_the_shared_rules() {
        assert_eq!(
            tool_target("read", "{\"path\":\"src/lib.rs\"}"),
            "src/lib.rs"
        );
        assert_eq!(
            tool_target("bash", "{\"command\":\"cargo test\"}"),
            "cargo test"
        );
        let lines: Vec<Vec<Value>> =
            serde_json::from_str(&tokenize_file("fn main() {}", "a.rs")).unwrap();
        assert!(
            lines[0]
                .iter()
                .any(|token| token["class"] == "code-token-keyword")
        );
    }

    #[test]
    fn metrics_and_context_match_the_desktop_formatting() {
        let metrics: Value = serde_json::from_str(&metrics_session(
            "{\"uncachedTokens\":8000,\"cacheReadTokens\":24000,\"cacheWriteTokens\":4800,\"outputTokens\":4800,\"costTotal\":0.0184}",
        ))
        .unwrap();
        assert_eq!(metrics["inputText"], "32k");
        assert_eq!(metrics["outputText"], "4.8k");
        assert_eq!(metrics["cacheText"], "75%");
        let context: Value = serde_json::from_str(&context_reported(
            "{\"tokens\":38000,\"window\":100000,\"percent\":38}",
        ))
        .unwrap();
        assert_eq!(context["percentText"], "38%");
    }

    #[test]
    fn theme_tokens_are_the_generated_contract() {
        let tokens: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&theme_tokens("codex", false, false)).unwrap();
        assert_eq!(tokens.get("bg").map(String::as_str), Some("#ffffff"));
        assert_eq!(tokens.get("accent").map(String::as_str), Some("#202020"));
        assert!(theme_presets().contains("\"codex\""));
    }
}
