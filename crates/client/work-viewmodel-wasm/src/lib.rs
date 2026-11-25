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
use work_core::api::WireEvent;
use work_core::api::events::WireEventProjection as _;
use work_core::{MessageBlock, RunPhase, RunProjection, ToolStatus};
use work_viewmodel::conversation::{code_tokens, tool_display};
use work_viewmodel::observability::metrics::{Context, Metrics, percent, tokens};
use work_viewmodel::session_controls::block_wire;
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

/// Stateful presenter: one `TranscriptOrder` and one `RunProjection` per
/// session scope, so display slots stay stable while history and the live run
/// projection grow independently.
#[wasm_bindgen]
pub struct Presenter {
    orders: HashMap<String, TranscriptOrder>,
    projections: HashMap<String, RunProjection>,
}

#[wasm_bindgen]
impl Presenter {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            projections: HashMap::new(),
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
        let order = self.orders.entry(scope.to_string()).or_default();
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

    /// Read-only conversation layout for fixture/preview callers. Production
    /// sessions receive this same DTO directly from the canonical SDK binding.
    pub fn conversation(&mut self, scope: &str, history_json: &str, blocks_json: &str) -> String {
        let history: Vec<Value> = serde_json::from_str(history_json).unwrap_or_default();
        let inputs: Vec<BlockInput> = serde_json::from_str(blocks_json).unwrap_or_default();
        let blocks: Vec<MessageBlock> = inputs.into_iter().map(BlockInput::into_block).collect();
        let rows = self
            .orders
            .entry(scope.to_string())
            .or_default()
            .update(scope, &history, &blocks);
        let projection = self.projections.get(scope);
        let view = work_viewmodel::conversation::transcript_view::present(
            &rows,
            &history,
            &blocks,
            projection.map(|run| &run.phase),
            projection.map(|run| &run.connection),
            Default::default(),
        );
        serde_json::to_string(&view)
            .unwrap_or_else(|_| "{\"observation\":\"\",\"parts\":[]}".into())
    }

    /// Replay live wire events, then return `{rows, blocks, phase, settled}`.
    ///
    /// The projection owns deduplication, stale-sequence rejection and resync,
    /// so a reconnect that replays old events cannot duplicate rows.
    pub fn apply_events(&mut self, scope: &str, history_json: &str, wire_json: &str) -> String {
        let history: Vec<Value> = serde_json::from_str(history_json).unwrap_or_default();
        let events: Vec<WireEvent> = serde_json::from_str(wire_json).unwrap_or_default();
        let projection = self
            .projections
            .entry(scope.to_string())
            .or_insert_with(|| RunProjection::new(scope));
        for event in events {
            if let Ok(event) = event.into_core_event() {
                projection.apply(event);
            }
        }
        let blocks = projection.blocks.clone();
        let phase = format!("{:?}", projection.phase);
        let settled = matches!(projection.phase, RunPhase::Settled(_));
        let order = self.orders.entry(scope.to_string()).or_default();
        let rows = order.update(scope, &history, &blocks);
        let rows: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                let (source, index) = match row.source {
                    RowSource::History(index) => ("history", index),
                    RowSource::Projection(index) => ("projection", index),
                };
                json!({ "key": row.key, "source": source, "index": index })
            })
            .collect();
        let blocks: Vec<Value> = blocks.iter().map(block_wire).collect();
        json!({ "rows": rows, "blocks": blocks, "phase": phase, "settled": settled }).to_string()
    }

    /// Drop retained slots for a scope (node switch, session reset).
    pub fn forget(&mut self, scope: &str) {
        self.orders.remove(scope);
        self.projections.remove(scope);
    }
}

impl Default for Presenter {
    fn default() -> Self {
        Self::new()
    }
}

/// Project a native catalog into the rows a resource list renders.
///
/// Grouping keys, display names, scope labels and the install origin all come
/// from `work_viewmodel::resources::catalog`, so the Web list cannot drift from
/// the desktop one.
#[wasm_bindgen]
pub fn resource_rows(catalog_json: &str, project_id: &str) -> String {
    let catalog: work_core::api::NativeCatalog = match serde_json::from_str(catalog_json) {
        Ok(catalog) => catalog,
        Err(_) => return "{\"rows\":[],\"counts\":[0,0,0]}".into(),
    };
    let installed = work_viewmodel::resources::catalog::installed_resources(&catalog, false);
    let counts =
        work_viewmodel::resources::catalog::counts(catalog.resources.iter().map(|r| &r.kind));
    let rows: Vec<Value> = installed
        .iter()
        .map(|resource| {
            let choice = work_viewmodel::resources::catalog::ResourceChoice::Installed(resource.clone());
            json!({
                "reference": resource.reference,
                "name": choice.name(),
                "nativeName": choice.native_name(),
                "version": choice.version(),
                "kind": work_viewmodel::resources::catalog::kind_key(choice.kind()),
                "kindLabel": work_viewmodel::resources::catalog::kind_label(choice.kind()),
                "icon": work_viewmodel::resources::catalog::kind_icon(choice.kind()),
                "scope": match resource.scope {
                    work_core::api::NativeScope::Global => "global",
                    work_core::api::NativeScope::Project => "project",
                },
                "project": project_id,
                "installed": resource.facts["installed"] == true,
                "origin": work_viewmodel::resources::catalog::installation_origin(resource, &catalog),
            })
        })
        .collect();
    json!({ "rows": rows, "counts": counts }).to_string()
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

/// Renderer-independent safe Markdown DTO; no HTML renderer or execution state.
#[wasm_bindgen]
pub fn markdown_view(source: &str) -> String {
    serde_json::to_string(&work_viewmodel::conversation::markdown::present(source))
        .unwrap_or_else(|_| "{\"nodes\":[],\"copyText\":\"\",\"plain\":true}".into())
}

/// Read-only raw file rows; the original source remains the copy payload.
#[wasm_bindgen]
pub fn file_view(source: &str, path: &str, diff: bool) -> String {
    serde_json::to_string(&work_viewmodel::file_view::present(source, path, diff))
        .unwrap_or_else(|_| "[]".into())
}

/// Read-only validated unified-diff rows. No review or write identities.
#[wasm_bindgen]
pub fn diff_view(source: &str, path: &str) -> String {
    serde_json::to_string(&work_viewmodel::diff_view::present(source, path))
        .expect("diff presentation contains only serializable JSON values")
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

/// Seal a one-time ticket into the QR payload the phone expects.
#[wasm_bindgen]
pub fn seal_ticket(ticket: &str) -> String {
    work_viewmodel::qr::seal(ticket)
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
    fn file_binding_preserves_shared_rows_without_inventing_diff_line_numbers() {
        for diff in [false, true] {
            let text = "pub fn main() {}\r\n+literal\r\n";
            let actual: Value = serde_json::from_str(&file_view(text, "main.rs", diff)).unwrap();
            let expected =
                serde_json::to_value(work_viewmodel::file_view::present(text, "main.rs", diff))
                    .unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn diff_binding_preserves_validated_numbers_and_whole_patch_fallback() {
        for source in [
            "--- a/x\r\n+++ b/x\r\n@@ -1 +1 @@\r\n--- body\r\n+++ body\r\n",
            "@@@ invalid",
            "--- a/x\n+++ b/x\n@@ -1,2 +1 @@\n-old\n+new\n",
        ] {
            let actual: Value = serde_json::from_str(&diff_view(source, "x")).unwrap();
            let expected =
                serde_json::to_value(work_viewmodel::diff_view::present(source, "x")).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn markdown_binding_preserves_the_shared_safe_dto_and_copy_payload() {
        let source = "## Shared\n\n**body**\n\n```rust\nfn main() {}\n```";
        let actual: Value = serde_json::from_str(&markdown_view(source)).unwrap();
        let expected =
            serde_json::to_value(work_viewmodel::conversation::markdown::present(source)).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual["copyText"], "Shared\n\nbody\n\nfn main() {}");
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
    fn live_events_build_tool_blocks_and_keep_duplicates_out() {
        let mut presenter = Presenter::new();
        let history = json!([{ "id": "user-1", "role": "user", "content": "hi" }]).to_string();
        let wire = json!([
            { "id": "1", "seq": 1, "event_type": "agent_start", "data": { "type": "agent_start" } },
            { "id": "2", "seq": 2, "event_type": "message_update", "data": { "type": "message_update", "message": { "entryId": "answer" }, "eventType": "text_delta", "delta": "hello" } },
            { "id": "3", "seq": 3, "event_type": "tool_execution_start", "data": { "type": "tool_execution_start", "toolCallId": "read-1", "toolName": "read", "args": { "path": "src/theme.rs" } } },
            { "id": "4", "seq": 4, "event_type": "tool_execution_end", "data": { "type": "tool_execution_end", "toolCallId": "read-1", "toolName": "read", "result": { "content": [{ "type": "text", "text": "fn theme() {}" }] } } }
        ])
        .to_string();
        let first: Value =
            serde_json::from_str(&presenter.apply_events("scope", &history, &wire)).unwrap();
        let blocks = first["blocks"].as_array().unwrap();
        assert_eq!(
            blocks
                .iter()
                .filter(|block| block["kind"] == "tool")
                .count(),
            1
        );
        assert_eq!(blocks[0]["kind"], "text");
        assert_eq!(blocks[0]["text"], "hello");
        assert_eq!(first["rows"].as_array().unwrap().len(), 3);

        // The same batch replayed again must not duplicate rows or blocks.
        let again: Value =
            serde_json::from_str(&presenter.apply_events("scope", &history, &wire)).unwrap();
        assert_eq!(first["rows"], again["rows"]);
        assert_eq!(first["blocks"], again["blocks"]);
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
