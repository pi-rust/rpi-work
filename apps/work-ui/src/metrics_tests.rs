use super::runtime_stats::{Context, Metrics, tool_counts};
use serde_json::{Value, json};
use work_core::{MessageBlock, RunProjection, ToolStatus};

#[test]
fn host_stats_provide_total_input_output_and_authoritative_ratio() {
    let metrics = Metrics::session(Some(&json!({
        "cachedTokens":24000,"uncachedTokens":8000,"totalTokens":36800,
        "costTotal":0.0184,"cacheHitRatio":0.75
    })));
    assert_eq!(metrics.input, Some(32000.0));
    assert_eq!(metrics.output, Some(4800.0));
    assert_eq!(metrics.cached, Some(24000.0));
    assert_eq!(metrics.cache_percent, Some(75.0));
    assert_eq!(metrics.cost, Some(0.0184));
    assert_eq!(
        Context::reported(Some(&json!({"percent":38}))).percent,
        Some(38.0)
    );
    assert_eq!(
        Metrics::session(Some(&json!({
            "cachedTokens":24000,"uncachedTokens":8000,"cacheHitRatio":0.5
        })))
        .cache_percent,
        Some(50.0)
    );
}

#[test]
fn nested_usage_includes_cache_writes_in_total_input() {
    let metrics = Metrics::session(Some(&json!({"usage":{
        "input":6000,"output":4800,"cache_read":24000,"cache_write":2000
    }})));
    assert_eq!(metrics.input, Some(32000.0));
    assert_eq!(metrics.output, Some(4800.0));
    assert_eq!(metrics.cache_percent, Some(75.0));
    assert_eq!(metrics.uncached, Some(8000.0));
    let explicit_uncached = Metrics::session(Some(&json!({
        "uncachedTokens":8000,"cachedTokens":24000,"cacheWriteTokens":2000
    })));
    assert_eq!(explicit_uncached.input, Some(32000.0));
    assert_eq!(explicit_uncached.uncached, Some(8000.0));
}

#[test]
fn missing_or_inconsistent_usage_is_not_invented() {
    assert_eq!(Metrics::session(Some(&Value::Null)), Metrics::default());
    assert_eq!(Context::reported(None), Context::default());
    let incomplete = Metrics::session(Some(&json!({"cachedTokens":24000,"inputTokens":8000})));
    assert_eq!(incomplete.input, None);
    assert_eq!(incomplete.output, None);
    assert_eq!(incomplete.cached, Some(24000.0));
    assert_eq!(incomplete.cache_percent, None);
    let inconsistent = Metrics::session(Some(&json!({
        "cachedTokens":24000,"uncachedTokens":8000,"totalTokens":1000
    })));
    assert_eq!(inconsistent.output, None);
    let invalid = Metrics::session(Some(&json!({
        "inputTokens":-1,"cachedTokens":-1,"cacheHitRatio":2
    })));
    assert_eq!(invalid, Metrics::default());
    // A native settlement without usage must not borrow session totals.
    assert_eq!(Metrics::turn(None), Metrics::default());
}

#[test]
fn actual_tool_ids_are_deduplicated_across_history_and_projection() {
    let history = vec![
        json!({"role":"assistant","content":[{"type":"toolCall","id":"call-1"},{"type":"toolCall","name":"missing-id"}]}),
        json!({"role":"toolResult","toolCallId":"call-1","isError":true}),
        json!({"role":"toolResult","toolCallId":"call-1","isError":true}),
    ];
    let mut projection = RunProjection::new("run");
    projection.blocks = vec![
        MessageBlock::ToolCall {
            id: "call-1".into(),
            name: "read".into(),
            arguments: String::new(),
            output: String::new(),
            status: ToolStatus::Failed,
        },
        MessageBlock::ToolCall {
            id: "call-2".into(),
            name: "read".into(),
            arguments: String::new(),
            output: String::new(),
            status: ToolStatus::Completed,
        },
    ];
    assert_eq!(tool_counts(&history, Some(&projection)), (2, 1));
    assert_eq!(tool_counts(&[], None), (0, 0));
}
