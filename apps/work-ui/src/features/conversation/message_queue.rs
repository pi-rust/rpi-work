//! A pending-message list, not a second editor. Mutations belong to the
//! workspace so a late cancellation can still recover into its original draft.
use crate::shared::components::Icon;
use dioxus::prelude::*;
use work_client::{QueueItem, QueueSnapshot};

pub const CSS: &str = include_str!("message_queue.css");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueStatus {
    Queued,
    Claimed,
    Completed,
    Cancelled,
    Failed,
    Unknown,
}

impl QueueStatus {
    pub fn from_wire(status: Option<&str>) -> Self {
        match status.unwrap_or("").to_ascii_lowercase().as_str() {
            "queued" | "pending" => Self::Queued,
            "claimed" | "reserved" | "running" | "in_progress" | "in-progress" => Self::Claimed,
            "completed" | "consumed" | "done" => Self::Completed,
            "cancelled" | "canceled" => Self::Cancelled,
            "failed" | "error" => Self::Failed,
            _ => Self::Unknown,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Queued => "待处理",
            Self::Claimed => "已领取",
            Self::Completed => "已处理",
            Self::Cancelled => "已移除",
            Self::Failed => "失败",
            Self::Unknown => "状态未知",
        }
    }

    pub fn can_cancel(self) -> bool {
        self == Self::Queued
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Claimed | Self::Unknown)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueueAction {
    Refresh,
    History,
    Clear,
    Remove(QueueItem),
    Edit(QueueItem),
    SendNow(QueueItem),
}

impl QueueAction {
    pub fn command(&self) -> &'static str {
        match self {
            Self::Refresh | Self::History => "get_queue",
            Self::Clear => "clear_queue",
            Self::Remove(_) | Self::Edit(_) | Self::SendNow(_) => "cancel_queued",
        }
    }

    pub fn item(&self) -> Option<&QueueItem> {
        match self {
            Self::Remove(item) | Self::Edit(item) | Self::SendNow(item) => Some(item),
            _ => None,
        }
    }
}

/// An HTTP success or an absent item is not proof that we withdrew it before
/// consumption. Only a positive cancellation outcome permits editing/sending.
pub fn cancellation_confirmed(outcome: &str) -> bool {
    matches!(outcome, "cancelled" | "canceled")
}

/// A text-only composer must not withdraw a rich message and silently lose
/// its images/other blocks. Removing such an item remains an explicit action.
pub fn recoverable_text(item: &QueueItem) -> bool {
    fn text_only(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::String(_) => true,
            serde_json::Value::Array(blocks) => blocks.iter().all(text_only),
            serde_json::Value::Object(object) => object.get("content").map_or_else(
                || object.get("type").and_then(serde_json::Value::as_str) == Some("text"),
                text_only,
            ),
            _ => false,
        }
    }
    !item.text.trim().is_empty() && item.message.as_ref().is_none_or(text_only)
}

/// Read the latest main draft at acknowledgement time; never replace text typed
/// while waiting, and never create a private queue draft that the user can lose.
pub fn recovered_text(existing: &str, returned: &str) -> String {
    if existing.is_empty() {
        returned.to_owned()
    } else if returned.is_empty() || existing == returned {
        existing.to_owned()
    } else {
        format!("{existing}\n{returned}")
    }
}

#[component]
pub fn QueuePanel(
    snapshot: Option<QueueSnapshot>,
    history: Option<QueueSnapshot>,
    busy: bool,
    error: Option<String>,
    can_cancel: bool,
    can_clear: bool,
    can_send: bool,
    onaction: EventHandler<QueueAction>,
) -> Element {
    let mut expanded = use_signal(|| true);
    let items: Vec<_> = snapshot
        .as_ref()
        .into_iter()
        .flat_map(|snapshot| &snapshot.items)
        .filter(|item| QueueStatus::from_wire(item.status.as_deref()).is_active())
        .collect();
    if items.is_empty() && error.is_none() && !busy && history.is_none() {
        return rsx! {};
    }
    let count = items.len();
    let preview = items.first().map_or("", |item| item.text.as_str());
    let cancellable = items
        .iter()
        .any(|item| QueueStatus::from_wire(item.status.as_deref()).can_cancel());
    rsx! {
        section { class: "message-queue composer-queue-panel", role: "region", aria_label: "待处理消息",
            button {
                r#type: "button", class: "composer-queue-heading",
                aria_label: if *expanded.read() { "收起队列" } else { "展开队列" },
                aria_expanded: *expanded.read(),
                onclick: move |_| { let next = !*expanded.peek(); expanded.set(next); },
                Icon { name: "message" }
                span { class: "composer-queue-count", "{count}" }
                if *expanded.read() { span { "条待处理消息" } }
                else { span { class: "composer-queue-preview", "{preview}" } }
                span { class: "composer-queue-chevron", Icon { name: "chevron" } }
            }
            // Errors stay visible even if the list is collapsed or now empty.
            if let Some(error) = error {
                p { class: "queue-error", role: "alert", "{error}" }
            }
            if *expanded.read() {
                div { class: "composer-queue-list",
                    for item in items {
                        { let status = QueueStatus::from_wire(item.status.as_deref()); let editable = can_cancel && status.can_cancel() && !busy; let edit = item.clone(); let remove = item.clone(); let send = item.clone(); rsx! {
                            div { key: "{item.entry_id}", class: "composer-queue-item", "data-entry-id": "{item.entry_id}",
                                div { class: "composer-queue-content",
                                    p { class: "queue-item-text", if item.text.trim().is_empty() { "非文本消息" } else { "{item.text}" } }
                                    if status != QueueStatus::Queued { small { class: "queue-item-status", "{status.label()}" } }
                                }
                                div { class: "composer-queue-actions",
                                    if can_send && status.can_cancel() && recoverable_text(item) {
                                        button { r#type: "button", disabled: !editable, aria_label: "立即发送排队消息", title: "立即发送", onclick: move |_| onaction.call(QueueAction::SendNow(send.clone())), Icon { name: "play" } }
                                    }
                                    button { r#type: "button", disabled: !editable || !recoverable_text(item), aria_label: "编辑排队消息", title: "取回输入框编辑", onclick: move |_| onaction.call(QueueAction::Edit(edit.clone())), Icon { name: "edit" } }
                                    button { r#type: "button", disabled: !editable, aria_label: "移除排队消息", title: "移除", onclick: move |_| onaction.call(QueueAction::Remove(remove.clone())), Icon { name: "close" } }
                                }
                            }
                        } }
                    }
                }
                details { class: "queue-options",
                    summary { "队列选项" }
                    div { class: "queue-options-actions",
                        button { r#type: "button", disabled: busy, onclick: move |_| onaction.call(QueueAction::Refresh), "刷新" }
                        button { r#type: "button", disabled: busy || !can_clear || !cancellable, onclick: move |_| onaction.call(QueueAction::Clear), "清空待执行" }
                        button { r#type: "button", disabled: busy, onclick: move |_| onaction.call(QueueAction::History), "查看历史" }
                    }
                    if let Some(history) = history {
                        div { class: "queue-history", aria_label: "已处理消息",
                            if !history.items.iter().any(|item| !QueueStatus::from_wire(item.status.as_deref()).is_active()) {
                                p { "暂无已处理消息" }
                            }
                            for item in history.items.iter().filter(|item| !QueueStatus::from_wire(item.status.as_deref()).is_active()) {
                                p { key: "{item.entry_id}", small { "{QueueStatus::from_wire(item.status.as_deref()).label()} · " } "{item.text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_pending_states_can_be_changed() {
        for state in ["queued", "pending", "QUEUED"] {
            assert!(QueueStatus::from_wire(Some(state)).can_cancel());
        }
        for state in [
            None,
            Some("reserved"),
            Some("claimed"),
            Some("running"),
            Some("consumed"),
            Some("completed"),
            Some("cancelled"),
            Some("failed"),
            Some("future"),
        ] {
            assert!(!QueueStatus::from_wire(state).can_cancel());
        }
    }

    #[test]
    fn only_confirmed_withdrawal_permits_recovery_or_send_now() {
        assert!(cancellation_confirmed("cancelled"));
        assert!(cancellation_confirmed("canceled"));
        for outcome in [
            "not_found",
            "claimed",
            "already_consumed",
            "completed",
            "unknown",
            "",
            "success",
        ] {
            assert!(!cancellation_confirmed(outcome));
        }
    }

    #[test]
    fn return_to_composer_preserves_current_and_newer_drafts() {
        assert_eq!(recovered_text("", "queued"), "queued");
        assert_eq!(recovered_text("new typing", "queued"), "new typing\nqueued");
        assert_eq!(recovered_text("same", "same"), "same");
        assert_eq!(recovered_text("new typing", ""), "new typing");
    }

    #[test]
    fn taking_back_text_never_discards_rich_content() {
        let mut item: QueueItem = serde_json::from_value(serde_json::json!({"entryId":"rich", "queue":"followUp", "text":"caption", "status":"queued"})).unwrap();
        assert!(recoverable_text(&item));
        item.message =
            Some(serde_json::json!({"role":"user", "content":[{"type":"text", "text":"caption"}]}));
        assert!(recoverable_text(&item));
        item.message = Some(
            serde_json::json!({"role":"user", "content":[{"type":"text", "text":"caption"}, {"type":"image", "data":"untouched"}]}),
        );
        assert!(!recoverable_text(&item));
        item.message = None;
        item.text.clear();
        assert!(!recoverable_text(&item));
    }

    #[test]
    fn completed_messages_do_not_leave_an_empty_shelf() {
        for state in ["completed", "consumed", "cancelled", "failed"] {
            assert!(!QueueStatus::from_wire(Some(state)).is_active());
        }
        for state in ["pending", "claimed", "unknown"] {
            assert!(QueueStatus::from_wire(Some(state)).is_active());
        }
    }
}
