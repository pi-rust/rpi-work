use work_core::{
    ApplyResult, ConnectionState, Event, EventKind, MessageBlock, OutputMode, RunPhase,
    RunProjection, SettlementOutcome, ToolStatus, Usage,
};

fn event(seq: u64, id: &str, kind: EventKind) -> Event {
    Event {
        id: id.into(),
        seq: Some(seq),
        kind,
    }
}

fn tool<'a>(run: &'a RunProjection, id: &str) -> &'a MessageBlock {
    run.blocks
        .iter()
        .find(|block| block.id() == id)
        .expect("tool block should exist")
}

#[test]
fn text_and_tool_events_keep_stable_order() {
    let mut run = RunProjection::new("run-1");
    run.apply(event(1, "accepted", EventKind::Accepted));
    run.apply(event(
        2,
        "text-1",
        EventKind::TextDelta {
            block_id: "answer".into(),
            delta: "先检查 ".into(),
        },
    ));
    run.apply(event(
        3,
        "tool-start",
        EventKind::ToolStarted {
            tool_id: "shell-1".into(),
            name: "cargo test".into(),
            arguments: "--workspace".into(),
        },
    ));
    run.apply(event(
        4,
        "tool-snapshot",
        EventKind::ToolOutput {
            tool_id: "shell-1".into(),
            content: "running".into(),
            mode: OutputMode::Snapshot,
        },
    ));
    run.apply(event(
        5,
        "tool-log",
        EventKind::ToolOutput {
            tool_id: "shell-1".into(),
            content: "\npassed".into(),
            mode: OutputMode::Append,
        },
    ));
    run.apply(event(
        6,
        "text-2",
        EventKind::TextDelta {
            block_id: "answer".into(),
            delta: "项目结构。".into(),
        },
    ));

    assert_eq!(run.phase, RunPhase::Text);
    assert_eq!(run.blocks[0].id(), "answer");
    assert_eq!(run.blocks[1].id(), "shell-1");
    assert!(matches!(
        tool(&run, "shell-1"),
        MessageBlock::ToolCall {
            output,
            status: ToolStatus::Running,
            ..
        } if output == "running\npassed"
    ));
}

#[test]
fn duplicate_and_replayed_events_do_not_append_again() {
    let mut run = RunProjection::new("run-1");
    let text = event(
        1,
        "text-1",
        EventKind::TextDelta {
            block_id: "answer".into(),
            delta: "hello".into(),
        },
    );
    assert_eq!(run.apply(text.clone()), ApplyResult::Applied);
    assert_eq!(run.apply(text), ApplyResult::IgnoredDuplicate);

    let stale = event(
        1,
        "different-id-same-seq",
        EventKind::TextDelta {
            block_id: "answer".into(),
            delta: " stale".into(),
        },
    );
    assert_eq!(run.apply(stale), ApplyResult::IgnoredStale);
    assert_eq!(
        run.blocks[0],
        MessageBlock::Text {
            id: "answer".into(),
            text: "hello".into(),
        }
    );
}

#[test]
fn failure_and_interruption_retain_partial_work() {
    let mut run = RunProjection::new("run-1");
    run.apply(event(
        1,
        "text",
        EventKind::TextDelta {
            block_id: "answer".into(),
            delta: "partial".into(),
        },
    ));
    run.apply(event(2, "cancel", EventKind::CancelRequested));
    assert_eq!(run.phase, RunPhase::Cancelling);

    run.apply(event(
        3,
        "settled",
        EventKind::AgentSettled {
            outcome: SettlementOutcome::Interrupted,
            usage: Some(Usage {
                input_tokens: Some(10),
                output_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            }),
        },
    ));
    assert_eq!(run.phase, RunPhase::Settled(SettlementOutcome::Interrupted));
    assert_eq!(run.blocks[0].id(), "answer");
    assert_eq!(
        run.usage.as_ref().and_then(|usage| usage.input_tokens),
        Some(10)
    );
}

#[test]
fn resync_starts_a_new_generation_and_does_not_reuse_old_blocks() {
    let mut run = RunProjection::new("run-1");
    run.apply(event(
        12,
        "text",
        EventKind::TextDelta {
            block_id: "old-answer".into(),
            delta: "old".into(),
        },
    ));
    let result = run.apply(event(
        99,
        "resync",
        EventKind::ResyncRequired {
            reason: "event history evicted".into(),
        },
    ));

    assert_eq!(result, ApplyResult::RebuiltForResync);
    assert_eq!(run.generation, 1);
    assert_eq!(run.phase, RunPhase::Ready);
    assert_eq!(run.connection, ConnectionState::Resyncing);
    assert_eq!(run.blocks.len(), 0);
    assert_eq!(run.cursor, None);
}

#[test]
fn connection_loss_does_not_change_run_phase() {
    let mut run = RunProjection::new("run-1");
    run.apply(event(1, "accepted", EventKind::Accepted));
    run.apply(event(
        2,
        "lost",
        EventKind::ConnectionChanged(ConnectionState::Reconnecting),
    ));

    assert_eq!(run.phase, RunPhase::Accepted);
    assert_eq!(run.connection, ConnectionState::Reconnecting);
}

#[test]
fn settled_outcomes_are_preserved_without_inventing_usage() {
    let mut run = RunProjection::new("run-1");
    run.apply(event(
        1,
        "settled",
        EventKind::AgentSettled {
            outcome: SettlementOutcome::Failed,
            usage: None,
        },
    ));

    assert_eq!(run.phase, RunPhase::Settled(SettlementOutcome::Failed));
    assert_eq!(run.usage, None);
}
