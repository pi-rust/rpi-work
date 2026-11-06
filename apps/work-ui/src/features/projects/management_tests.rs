use super::*;

#[test]
fn relative_time_handles_unknown_future_and_boundaries() {
    assert_eq!(relative_updated_at(0, 100), "");
    assert_eq!(relative_updated_at(100, 0), "");
    assert_eq!(relative_updated_at(200, 100), "刚刚");
    let base = 1_000;
    for (minutes, expected) in [
        (0, "刚刚"),
        (1, "1分"),
        (59, "59分"),
        (60, "1小时"),
        (1440, "1天"),
        (43_200, "1个月"),
        (525_600, "1年"),
    ] {
        assert_eq!(relative_updated_at(base, base + minutes * 60_000), expected);
    }
}

#[test]
fn preview_includes_selected_without_duplicating_or_expanding_everything() {
    assert!(session_preview_visible(0, false, false));
    assert!(session_preview_visible(4, false, false));
    assert!(!session_preview_visible(5, false, false));
    assert!(session_preview_visible(99, false, true));
    assert!(session_preview_visible(99, true, false));
}

#[test]
fn only_explicit_active_failed_or_waiting_states_get_status_dots() {
    let mut session: work_client::SessionSummary = serde_json::from_value(serde_json::json!({
        "sessionId":"test", "projectId":"project", "title":"Test", "status":"idle",
        "createdAt":0, "updatedAt":0, "lastSeq":0
    }))
    .unwrap();
    for status in [
        "idle",
        "completed",
        "unknown",
        "aborted",
        "offline",
        "disconnected",
        "interrupted",
    ] {
        session.status = status.into();
        assert_eq!(sidebar_session_status(&session), None);
    }
    for (status, expected) in [
        ("running", ("running", "运行中")),
        ("streaming", ("running", "运行中")),
        ("busy", ("running", "运行中")),
        ("loading", ("loading", "加载中")),
        ("sending", ("sending", "发送中")),
        ("cancelling", ("cancelling", "停止中")),
    ] {
        session.status = status.into();
        assert_eq!(sidebar_session_status(&session), Some(expected));
    }
    for status in ["waiting", "waiting_for_input", "awaiting_approval"] {
        session.status = status.into();
        assert_eq!(
            sidebar_session_status(&session),
            Some(("waiting", "等待处理"))
        );
    }
    session.pending_resume = true;
    for status in [
        "idle",
        "running",
        "interrupted",
        "offline",
        "loading",
        "sending",
        "cancelling",
    ] {
        session.status = status.into();
        assert_eq!(
            sidebar_session_status(&session),
            Some(("waiting", "待恢复"))
        );
    }
    session.error = Some("failed".into());
    assert_eq!(
        sidebar_session_status(&session),
        Some(("failed", "执行失败"))
    );
    session.error = None;
    for status in ["error", "failed"] {
        session.status = status.into();
        assert_eq!(
            sidebar_session_status(&session),
            Some(("failed", "执行失败"))
        );
    }
}

#[test]
fn live_activity_maps_labels_tones_and_busy_indicators() {
    for (activity, expected, spinning) in [
        (SidebarActivity::Loading, Some(("loading", "加载中")), true),
        (SidebarActivity::Sending, Some(("sending", "发送中")), true),
        (
            SidebarActivity::WaitingReply,
            Some(("loading", "等待回复")),
            true,
        ),
        (SidebarActivity::Running, Some(("running", "运行中")), true),
        (
            SidebarActivity::AwaitingInput,
            Some(("waiting", "等待确认")),
            false,
        ),
        (
            SidebarActivity::Cancelling,
            Some(("cancelling", "停止中")),
            true,
        ),
        (
            SidebarActivity::Disconnected,
            Some(("offline", "已断开")),
            false,
        ),
        (SidebarActivity::Idle, None, false),
    ] {
        assert_eq!(activity.status(), expected, "{activity:?}");
        assert_eq!(activity.spinning(), spinning, "{activity:?}");
    }
}

#[test]
fn explicit_live_idle_clears_stale_summary_without_claiming_success() {
    let mut session: work_client::SessionSummary = serde_json::from_value(serde_json::json!({
        "sessionId":"test", "projectId":"project", "status":"running",
        "createdAt":0, "updatedAt":0, "lastSeq":0
    }))
    .unwrap();
    assert_eq!(
        sidebar_activity_status(&session, None),
        Some(("running", "运行中"))
    );
    assert_eq!(
        sidebar_activity_status(&session, Some(SidebarActivity::Idle)),
        None
    );
    assert_eq!(
        sidebar_activity_status(&session, Some(SidebarActivity::Disconnected)),
        Some(("offline", "已断开"))
    );
    session.pending_resume = true;
    assert_eq!(
        sidebar_activity_status(&session, None),
        Some(("waiting", "待恢复"))
    );
    assert_eq!(
        sidebar_activity_status(&session, Some(SidebarActivity::WaitingReply)),
        Some(("loading", "等待回复"))
    );
    session.error = Some("transport failed".into());
    assert_eq!(
        sidebar_activity_status(&session, None),
        Some(("failed", "执行失败"))
    );
    assert_eq!(
        sidebar_activity_status(&session, Some(SidebarActivity::Loading)),
        Some(("loading", "加载中"))
    );
    assert_eq!(
        sidebar_activity_status(&session, Some(SidebarActivity::Idle)),
        None
    );
}

#[test]
fn sidebar_spinner_is_dedicated_and_slows_under_reduced_motion() {
    assert!(CSS.contains(".session-activity-spinner{"));
    assert!(!CSS.contains(".activity-spinner"));
    assert!(CSS.contains("width:12px;height:12px;flex:0 0 12px"));
    assert!(CSS.contains(
        "@media(prefers-reduced-motion:reduce){.session-activity-spinner{animation:session-activity-spin 1.6s linear infinite !important}}"
    ));
    assert!(CSS.contains("border-top-color:currentColor"));
}

#[test]
fn switcher_states_require_completion_evidence_and_prefer_live_activity() {
    let mut session: work_client::SessionSummary = serde_json::from_value(serde_json::json!({
        "sessionId":"test", "projectId":"project", "title":"Test", "status":"idle",
        "createdAt":0, "updatedAt":0, "lastSeq":0
    }))
    .unwrap();
    assert_eq!(switcher_session_status(&session, None), ("idle", "空闲"));
    for (status, expected) in [
        ("running", ("running", "运行中")),
        ("streaming", ("running", "运行中")),
        ("waiting_for_input", ("waiting", "等待回答")),
        ("failed", ("failed", "异常")),
        ("completed", ("completed", "完成")),
        ("unknown", ("unknown", "状态未知")),
    ] {
        session.status = status.into();
        assert_eq!(switcher_session_status(&session, None), expected);
    }
    session.status = "idle".into();
    session.latest_settlement = Some(serde_json::json!({"outcome":"completed"}));
    assert_eq!(
        switcher_session_status(&session, None),
        ("completed", "完成")
    );
    assert_eq!(
        switcher_session_status(&session, Some(SidebarActivity::Running)),
        ("running", "运行中")
    );
    assert_eq!(
        switcher_session_status(&session, Some(SidebarActivity::WaitingReply)),
        ("waiting", "等待回答")
    );
    session.error = Some("failed".into());
    assert_eq!(switcher_session_status(&session, None), ("failed", "异常"));
    session.error = None;
    session.pending_resume = true;
    assert_eq!(
        switcher_session_status(&session, None),
        ("waiting", "待恢复")
    );
}
