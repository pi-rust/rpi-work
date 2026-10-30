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
    for status in ["idle", "completed", "unknown", "aborted"] {
        session.status = status.into();
        assert_eq!(sidebar_session_status(&session), None);
    }
    session.status = "running".into();
    assert_eq!(
        sidebar_session_status(&session),
        Some(("running", "运行中"))
    );
    session.pending_resume = true;
    assert_eq!(
        sidebar_session_status(&session),
        Some(("waiting", "等待处理"))
    );
    session.error = Some("failed".into());
    assert_eq!(
        sidebar_session_status(&session),
        Some(("failed", "执行失败"))
    );
}
