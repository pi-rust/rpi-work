use dioxus::{core::Task, prelude::*};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use work_client::{QueueItem, QueueSnapshot, WorkClient};

use crate::components::Modal;

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
            Self::Queued => "待执行",
            Self::Claimed => "已预留 · 不可取消",
            Self::Completed => "已消费",
            Self::Cancelled => "已取消",
            Self::Failed => "失败",
            Self::Unknown => "未知状态",
        }
    }

    fn is_pending(self) -> bool {
        self == Self::Queued
    }

    fn can_cancel(self) -> bool {
        self == Self::Queued
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueDraft {
    pub text: String,
    pub revision: u64,
    pub delivery: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueDraftStore {
    scopes: BTreeMap<String, QueueDraft>,
    proposals: BTreeMap<String, LocalQueueProposal>,
}

impl QueueDraftStore {
    fn get(&self, scope: &str) -> QueueDraft {
        self.scopes
            .get(scope)
            .cloned()
            .unwrap_or_else(|| QueueDraft {
                delivery: "followUp".to_owned(),
                ..QueueDraft::default()
            })
    }

    fn update(&mut self, scope: &str, update: impl FnOnce(&mut QueueDraft)) {
        let draft = self
            .scopes
            .entry(scope.to_owned())
            .or_insert_with(|| QueueDraft {
                delivery: "followUp".to_owned(),
                ..QueueDraft::default()
            });
        update(draft);
        draft.revision = draft.revision.saturating_add(1);
    }

    fn clear_if_revision(&mut self, scope: &str, revision: u64) {
        if let Some(draft) = self.scopes.get_mut(scope) {
            if draft.revision == revision {
                draft.text.clear();
                draft.revision = draft.revision.saturating_add(1);
            }
        }
    }
}

/// App-owned store for queue drafts. It intentionally does not persist message content.
pub fn use_queue_drafts() -> Signal<QueueDraftStore> {
    use_signal(QueueDraftStore::default)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LocalQueueProposal {
    items: Vec<QueueProposal>,
    selected: usize,
    paused: bool,
}

impl LocalQueueProposal {
    fn from_items(items: &[QueueItem]) -> Self {
        Self {
            items: items.iter().map(QueueProposal::from_item).collect(),
            ..Self::default()
        }
    }

    fn move_selected(&mut self, upward: bool) {
        if self.paused || self.items.is_empty() {
            return;
        }
        let next = if upward {
            self.selected.saturating_sub(1)
        } else {
            (self.selected + 1).min(self.items.len() - 1)
        };
        self.items.swap(self.selected, next);
        self.selected = next;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QueueProposal {
    entry_id: String,
    original_text: String,
    original_queue: String,
    original_status: String,
    text: String,
    delivery: String,
}

impl QueueProposal {
    fn from_item(item: &QueueItem) -> Self {
        Self {
            entry_id: item.entry_id.clone(),
            original_text: item.text.clone(),
            original_queue: item.queue.clone(),
            original_status: item.status.clone().unwrap_or_else(|| "unknown".to_owned()),
            text: item.text.clone(),
            delivery: if item_queue_class(&item.queue) == "steering" {
                "steer".to_owned()
            } else {
                "followUp".to_owned()
            },
        }
    }
}

fn pending_count(snapshot: &QueueSnapshot) -> usize {
    snapshot
        .items
        .iter()
        .filter(|item| QueueStatus::from_wire(item.status.as_deref()).is_pending())
        .count()
}

fn item_queue_label(queue: &str) -> &'static str {
    match queue {
        "steer" | "steering" => "引导消息",
        "followUp" | "follow_up" | "follow-up" => "后续消息",
        _ => "未知投递方式",
    }
}

fn item_queue_class(queue: &str) -> &'static str {
    match queue {
        "steer" | "steering" => "steering",
        "followUp" | "follow_up" | "follow-up" => "follow-up",
        _ => "unknown",
    }
}

#[component]
pub fn QueueShelf(snapshot: Option<QueueSnapshot>, onopen: EventHandler<()>) -> Element {
    let Some(snapshot) = snapshot else {
        return rsx! {};
    };
    if snapshot.items.is_empty() {
        return rsx! {};
    }

    let pending = pending_count(&snapshot);
    let steering = snapshot
        .items
        .iter()
        .filter(|item| {
            item_queue_class(&item.queue) == "steering"
                && QueueStatus::from_wire(item.status.as_deref()).is_pending()
        })
        .count();
    let follow_up = snapshot
        .items
        .iter()
        .filter(|item| {
            item_queue_class(&item.queue) == "follow-up"
                && QueueStatus::from_wire(item.status.as_deref()).is_pending()
        })
        .count();
    let finished = snapshot.items.len().saturating_sub(pending);
    let summary = if pending > 0 {
        format!("{pending} 条待执行 · {steering} 引导 · {follow_up} 后续")
    } else {
        format!("无待执行 · {finished} 条已处理")
    };

    rsx! {
        div { class: "message-queue queue-shelf", role: "region", aria_label: "消息队列摘要",
            button {
                class: "queue-shelf-main",
                r#type: "button",
                onclick: move |_| onopen.call(()),
                aria_label: "打开消息队列",
                span { class: "queue-shelf-title", "消息队列" }
                span { class: "queue-shelf-count", if pending > 0 { "{pending}" } else { "—" } }
                span { class: "queue-shelf-summary", "{summary}" }
                span { class: "queue-shelf-chevron", "›" }
            }
            button {
                class: "queue-shelf-add",
                r#type: "button",
                onclick: move |_| onopen.call(()),
                aria_label: "添加队列消息",
                "+"
            }
        }
    }
}

#[component]
pub fn QueueEntry(snapshot: Option<QueueSnapshot>, onopen: EventHandler<()>) -> Element {
    let count = snapshot.as_ref().map_or(0, pending_count);
    let label = if snapshot.is_none() {
        "打开队列并添加消息".to_owned()
    } else if count == 0 {
        "添加队列消息".to_owned()
    } else {
        format!("队列 · {count} 条待执行")
    };

    rsx! {
        div { class: "message-queue queue-entry",
            button {
                r#type: "button",
                class: "queue-entry-button",
                onclick: move |_| onopen.call(()),
                aria_label: "打开消息队列",
                span { class: "queue-entry-mark", "＋" }
                span { "{label}" }
            }
        }
    }
}

#[component]
pub fn DeliveryPicker(value: String, onchange: EventHandler<String>) -> Element {
    rsx! {
        label { class: "message-queue delivery-picker",
            span { "投递方式" }
            select {
                aria_label: "消息投递方式",
                value: "{value}",
                onchange: move |event| onchange.call(event.value()),
                option { value: "steer", "引导消息 · 在安全边界交付" }
                option { value: "followUp", "后续消息 · 当前回答收尾后" }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueueAction {
    Refresh,
    Enqueue,
    Cancel,
    Clear,
}

#[derive(Clone)]
struct QueueActionResult {
    notice: Option<String>,
    accepted: bool,
}

#[component]
pub fn QueueDialog(
    scope: String,
    session_id: String,
    client: Signal<Option<WorkClient>>,
    drafts: Signal<QueueDraftStore>,
    snapshot: Option<QueueSnapshot>,
    onchange: EventHandler<QueueSnapshot>,
    onclose: EventHandler<()>,
) -> Element {
    let mut composer_open = use_signal(|| false);
    let mut proposal_open = use_signal(|| false);
    let mut proposal_replacement = use_signal(|| None::<LocalQueueProposal>);
    let scope_key = format!("{scope}\u{1f}\u{1f}{session_id}");
    let mut generation = use_signal(|| 0u64);
    let mut active_scope = use_signal(|| scope_key.clone());
    let busy = use_signal(|| false);
    let loading = use_signal(|| false);
    let error = use_signal(|| None::<String>);
    let notice = use_signal(|| None::<String>);
    let loaded_scope = use_signal(|| None::<String>);
    let tasks = use_signal(|| Rc::new(RefCell::new(Vec::<Task>::new())));
    let task_list = tasks.read().clone();

    // A keyed parent normally remounts this component. This guard also covers a
    // host that changes scope without remounting it.
    if active_scope.peek().as_str() != scope_key {
        active_scope.set(scope_key.clone());
        proposal_open.set(false);
        proposal_replacement.set(None);
        composer_open.set(false);
        let next = *generation.read() + 1;
        generation.set(next);
    }

    {
        let tasks = task_list.clone();
        use_drop(move || {
            for task in tasks.borrow_mut().drain(..) {
                task.cancel();
            }
        });
    }

    let stored = drafts.read().get(&scope_key);
    let current_text = stored.text.clone();
    let current_delivery = if stored.delivery == "steer" {
        "steer".to_owned()
    } else {
        "followUp".to_owned()
    };
    let current_revision = stored.revision;
    let items = snapshot
        .as_ref()
        .map(|value| value.items.clone())
        .unwrap_or_default();
    let pending = snapshot.as_ref().map_or(0, pending_count);
    let can_submit = !*busy.read() && !current_text.trim().is_empty();

    // The initial GET is real, but it can only report for this scope and token.
    let effect_session = session_id.clone();
    let effect_scope = scope_key.clone();
    let render_scope_key = scope_key.clone();
    let effect_generation = *generation.read();
    let effect_onchange = onchange;
    let effect_generation_signal = generation;
    let effect_client = client;
    let mut effect_loaded_scope = loaded_scope;
    let mut effect_loading = loading;
    let mut effect_error = error;
    let effect_tasks = task_list.clone();
    use_effect(move || {
        // Read the live client inside the effect: a None -> Some connection must
        // start the first GET. Do not mark the scope during render, otherwise
        // this effect can observe its own marker and skip the request.
        let already_loaded = effect_loaded_scope.read().as_deref() == Some(effect_scope.as_str());
        if already_loaded {
            return;
        }
        let Some(node) = effect_client.read().clone() else {
            return;
        };
        effect_loaded_scope.set(Some(effect_scope.clone()));
        effect_loading.set(true);
        let token = effect_generation;
        let effect_session = effect_session.clone();
        let task = spawn(async move {
            match node.queue(&effect_session, true).await {
                Ok(value) if *effect_generation_signal.read() == token => {
                    effect_onchange.call(value);
                    effect_loading.set(false);
                }
                Err(problem) if *effect_generation_signal.read() == token => {
                    effect_error.set(Some(format!("读取消息队列失败：{problem}")));
                    effect_loading.set(false);
                }
                _ => {}
            }
        });
        effect_tasks.borrow_mut().push(task);
    });

    let run_action = {
        let client = client;
        let session_id = session_id.clone();
        let request_scope = scope_key.clone();
        let onchange = onchange;
        let mut busy = busy;
        let mut loading = loading;
        let mut error = error;
        let mut generation = generation;
        let tasks = task_list.clone();
        move |action: QueueAction, detail: Option<String>, submitted_revision: u64| {
            if *busy.read() && action != QueueAction::Refresh {
                return;
            }
            let Some(node) = client.read().clone() else {
                error.set(Some(
                    "当前未连接执行节点；草稿已保留，连接后可重试。".into(),
                ));
                return;
            };
            let token = *generation.read() + 1;
            generation.set(token);
            busy.set(action != QueueAction::Refresh);
            loading.set(true);
            error.set(None);
            let request_session = session_id.clone();
            let request_scope = request_scope.clone();
            let deliver_as = drafts.read().get(&request_scope).delivery;
            let request_text = detail.clone().unwrap_or_default();
            let generation_signal = generation;
            let mut busy = busy;
            let mut loading = loading;
            let mut error = error;
            let mut notice = notice;
            let mut drafts = drafts;
            let tasks_for_future = Rc::clone(&tasks);
            let task = spawn(async move {
                let operation: Result<QueueActionResult, String> = match action {
                    QueueAction::Refresh => Ok(QueueActionResult {
                        notice: None,
                        accepted: false,
                    }),
                    QueueAction::Enqueue => match node
                        .queue_message(&request_session, &request_text, &deliver_as)
                        .await
                    {
                        Ok(result) if result.accepted == Some(true) => Ok(QueueActionResult {
                            notice: Some(format!(
                                "已接受并排队（{}）",
                                if deliver_as == "steer" {
                                    "引导消息"
                                } else {
                                    "后续消息"
                                }
                            )),
                            accepted: true,
                        }),
                        Ok(_) => Ok(QueueActionResult {
                            notice: Some(
                                "服务已响应，但没有确认 accepted；仅显示已排队，草稿已保留。"
                                    .into(),
                            ),
                            accepted: false,
                        }),
                        Err(problem) => Err(format!("加入队列失败：{problem}")),
                    },
                    QueueAction::Cancel => match node
                        .cancel_queued(&request_session, detail.as_deref().unwrap_or_default())
                        .await
                    {
                        Ok(result) => Ok(QueueActionResult {
                            notice: Some(format!("取消请求结果：{}", result.outcome)),
                            accepted: false,
                        }),
                        Err(problem) => Err(format!("取消队列项失败：{problem}")),
                    },
                    QueueAction::Clear => node
                        .clear_queue(&request_session)
                        .await
                        .map(|_| QueueActionResult {
                            notice: Some("已发送清空请求，正在以服务端队列为准刷新。".into()),
                            accepted: false,
                        })
                        .map_err(|problem| format!("清空待执行消息失败：{problem}")),
                };
                if *generation_signal.read() != token {
                    return;
                }
                let refreshed = node.queue(&request_session, true).await;
                if *generation_signal.read() != token {
                    return;
                }
                match refreshed {
                    Ok(value) => {
                        onchange.call(value);
                        match operation {
                            Ok(result) => {
                                if let Some(message) = result.notice {
                                    notice.set(Some(message));
                                }
                                if result.accepted {
                                    drafts
                                        .write()
                                        .clear_if_revision(&request_scope, submitted_revision);
                                }
                            }
                            Err(problem) => error.set(Some(problem)),
                        }
                    }
                    Err(problem) => {
                        error.set(Some(format!("操作完成，但重新读取队列失败：{problem}")));
                    }
                }
                busy.set(false);
                loading.set(false);
                let _ = tasks_for_future;
            });
            tasks.borrow_mut().push(task);
        }
    };

    let close = {
        let mut generation = generation;
        let onclose = onclose;
        let tasks = task_list.clone();
        move |()| {
            let next = *generation.read() + 1;
            generation.set(next);
            for task in tasks.borrow_mut().drain(..) {
                task.cancel();
            }
            onclose.call(());
        }
    };
    let modal_close = {
        move |_| {
            let script =
                "document.getElementById('message-queue-detail')?.close(); dioxus.send(true);";
            spawn(async move {
                let _ = document::eval(script).recv::<bool>().await;
            });
        }
    };
    let refresh = {
        let mut action = run_action.clone();
        move |_| action(QueueAction::Refresh, None, current_revision)
    };
    let mut enqueue = run_action.clone();
    let mut clear = run_action.clone();
    let dialog_caption = format!("{scope} · {session_id}");
    let proposal_paused = drafts
        .read()
        .proposals
        .get(&scope_key)
        .is_some_and(|value| value.paused);
    let open_proposal = {
        let key = scope_key.clone();
        let source = items.clone();
        move |(entry_id, direction): (String, Option<bool>)| {
            let mut store = drafts.write();
            // Each real-item entry starts with the currently displayed server snapshot.
            // Existing local work is protected by an explicit confirmation below.
            let proposal = LocalQueueProposal::from_items(&source);
            let selected = proposal
                .items
                .iter()
                .position(|item| item.entry_id == entry_id)
                .unwrap_or(0);
            let mut proposal = proposal;
            proposal.selected = selected;
            if let Some(upward) = direction {
                proposal.move_selected(upward);
            }
            if store.proposals.contains_key(&key) {
                proposal_replacement.set(Some(proposal));
            } else {
                store.proposals.insert(key.clone(), proposal);
            }
            proposal_open.set(true);
        }
    };
    let proposal_entry = EventHandler::new(open_proposal);

    rsx! {
        Modal {
            id: "message-queue-detail",
            title: "消息队列",
            class: "message-queue queue-dialog".to_owned(),
            onclose: close,
            div { class: "queue-dialog-inner",

                div { class: "queue-dialog-body",
                    div { class: "queue-semantics", strong { "让下一条消息在合适的时机交付" } p { "引导消息会在安全边界交付；后续消息会在当前回答收尾后交付。" } }
                    details { class: "queue-diagnostics", summary { "节点与作用域诊断" } p { "{dialog_caption}" } p { "节点未提供暂停、重排或编辑接口。" } }
                    if let Some(message) = notice.read().as_ref() { div { class: "queue-notice", role: "status", "{message}" } }
                    if let Some(message) = error.read().as_ref() { div { class: "queue-error", role: "alert", "{message}" } }
                    if *loading.read() { div { class: "queue-loading", role: "status", "正在读取服务端队列…" } }
                    div { class: "queue-toolbar",
                        span { if pending > 0 { "{pending} 条待执行" } else { "没有已确认的待执行消息" } }
                        div { class: "queue-toolbar-actions",
                            button { r#type: "button", class: "queue-tool-button", onclick: refresh, disabled: *busy.read(), "刷新" }
                            button { r#type: "button", class: "queue-tool-button", aria_expanded: *composer_open.read() || !current_text.is_empty(), onclick: move |_| {
                                composer_open.set(true);
                                spawn(async move { let _ = document::eval(r#"requestAnimationFrame(() => document.querySelector('#message-queue-detail textarea[aria-label="队列消息草稿"]')?.focus()); dioxus.send(true);"#).recv::<bool>().await; });
                            }, "+ 添加" }
                            button { r#type: "button", class: "queue-tool-button", aria_expanded: *proposal_open.read(), aria_pressed: proposal_paused,
                                onclick: {
                                    let key = scope_key.clone(); let source = items.clone();
                                    move |_| {
                                        let mut store = drafts.write();
                                        let proposal = store.proposals.entry(key.clone()).or_insert_with(|| LocalQueueProposal::from_items(&source));
                                        proposal.paused = !proposal.paused;
                                        proposal_open.set(true);
                                    }
                                },
                                if proposal_paused { "恢复提案" } else { "暂停提案" }
                            }
                        }
                    }
                    p { class: "queue-proposal-warning", "仅本地提案 · 不会暂停/重排/修改节点已排队消息" }
                    if proposal_replacement.read().is_some() {
                        div { class: "queue-proposal-conflict", role: "group", aria_label: "替换本地提案确认",
                            p { "从当前真实快照重新建立提案会丢弃已有本地编辑和排序。" }
                            button { r#type: "button", class: "queue-tool-button", onclick: { let key = scope_key.clone(); move |_| {
                                if let Some(value) = proposal_replacement.take() { drafts.write().proposals.insert(key.clone(), value); }
                            } }, "确认替换本地提案" }
                            button { r#type: "button", class: "queue-tool-button", onclick: move |_| proposal_replacement.set(None), "保留已有提案" }
                        }
                    }
                    if *proposal_open.read() {
                        QueueProposalEditor { key: "{scope_key}", scope: scope_key.clone(), drafts, source: items.clone(), onclose: move |()| proposal_open.set(false), oncopied: move |()| composer_open.set(true) }
                    } else if drafts.read().proposals.contains_key(&scope_key) {
                        button { class: "queue-tool-button", r#type: "button", onclick: move |_| proposal_open.set(true), "继续本地提案" }
                    }
                    if items.is_empty() {
                        div { class: "queue-empty", "队列为空。可以先写一条草稿，再选择投递方式。" }
                    } else {
                        div { class: "queue-groups",
                            for (index, item) in items.iter().enumerate() {
                                {render_queue_item(item, index + 1, run_action.clone(), *busy.read(), current_revision, proposal_entry)}
                            }
                        }
                    }
                    if *composer_open.read() || !current_text.is_empty() {
                    div { class: "queue-composer",
                        DeliveryPicker {
                            value: current_delivery,
                            onchange: {
                                let delivery_scope = render_scope_key.clone();
                                move |value| drafts.write().update(&delivery_scope, |draft| draft.delivery = value)
                            },
                        }
                        textarea {
                            value: "{current_text}",
                            rows: "4",
                            placeholder: "写下要交给当前会话的消息…",
                            aria_label: "队列消息草稿",
                            oninput: {
                                let text_scope = render_scope_key.clone();
                                move |event| drafts.write().update(&text_scope, |draft| draft.text = event.value())
                            },
                        }
                        p { class: "queue-composer-help", "Enter 不会提交；可用 Shift+Enter 换行。这里不会发起普通 prompt。" }
                        button {
                            r#type: "button",
                            class: "queue-submit",
                            disabled: !can_submit,
                            onclick: { let submitted_text = current_text.clone(); move |_| enqueue(QueueAction::Enqueue, Some(submitted_text.clone()), current_revision) },
                            if *busy.read() { "提交中…" } else { "加入队列" }
                        }
                    }
                    }
                }
                footer { class: "queue-dialog-footer",
                    button { r#type: "button", class: "queue-clear", disabled: *busy.read() || pending == 0, onclick: move |_| clear(QueueAction::Clear, None, current_revision), "清空待执行" }
                    button { r#type: "button", class: "queue-close-footer", onclick: modal_close, "完成" }
                }
            }
        }
    }
}

#[component]
fn QueueProposalEditor(
    scope: String,
    drafts: Signal<QueueDraftStore>,
    source: Vec<QueueItem>,
    onclose: EventHandler<()>,
    oncopied: EventHandler<()>,
) -> Element {
    let proposal = drafts
        .read()
        .proposals
        .get(&scope)
        .cloned()
        .unwrap_or_else(|| LocalQueueProposal::from_items(&source));
    if proposal.items.is_empty() {
        return rsx! { section { class: "queue-proposal", p { "当前真实快照没有可复制的消息。" }, button { class: "queue-tool-button", r#type: "button", onclick: move |_| onclose.call(()), "关闭提案" } } };
    }
    let selected = proposal.selected.min(proposal.items.len() - 1);
    let selected_item = proposal.items[selected].clone();
    let paused = proposal.paused;
    let first = selected == 0;
    let last = selected + 1 == proposal.items.len();
    let draft = drafts.read().get(&scope);
    let has_draft = !draft.text.trim().is_empty();
    let update = EventHandler::new({
        let scope = scope.clone();
        move |change: ProposalChange| {
            let mut store = drafts.write();
            let entry = store.proposals.entry(scope.clone()).or_default();
            if entry.paused || entry.items.is_empty() {
                return;
            }
            entry.selected = entry.selected.min(entry.items.len() - 1);
            match change {
                ProposalChange::Text(value) => entry.items[entry.selected].text = value,
                ProposalChange::Delivery(value) => entry.items[entry.selected].delivery = value,
                ProposalChange::Select(value) => {
                    entry.selected = value.min(entry.items.len().saturating_sub(1));
                }
                ProposalChange::Move(up) => entry.move_selected(up),
            }
        }
    });
    let copy_scope = scope.clone();
    let replace_scope = scope.clone();
    let replace_item = selected_item.clone();
    let copy_text = selected_item.text.clone();
    let copy_delivery = selected_item.delivery.clone();
    rsx! {
        section { class: "queue-proposal", aria_label: "本地队列提案编辑区",
            div { class: "queue-proposal-heading",
                strong { "本地队列提案" }
                span { class: "queue-proposal-badge", "仅本地" }
                button { r#type: "button", class: "queue-tool-button", onclick: move |_| onclose.call(()), "关闭提案" }
            }
            p { class: "queue-proposal-note", "仅本地提案 · 不会暂停/重排/修改节点已排队消息。提案基于打开时快照，真实队列可能已变化。" }
            p { class: "queue-proposal-source", "来源 entryID：{selected_item.entry_id} · 原投递：{item_queue_label(&selected_item.original_queue)} · 原状态：{selected_item.original_status}" }
            label { class: "queue-proposal-field",
                span { "编辑提案文本" }
                textarea { rows: "5", value: "{selected_item.text}", disabled: paused, aria_label: "本地提案编辑文本", oninput: move |event| update.call(ProposalChange::Text(event.value())) }
            }
            fieldset { disabled: paused, class: "queue-proposal-delivery",
                DeliveryPicker { value: selected_item.delivery.clone(), onchange: move |value| update.call(ProposalChange::Delivery(value)) }
            }
            details { summary { "查看原始正文（不可修改）" } p { class: "queue-proposal-original", "{selected_item.original_text}" } }
            div { class: "queue-proposal-controls",
                button { r#type: "button", class: "queue-tool-button", disabled: paused || first, aria_label: "上移本地提案", onclick: move |_| update.call(ProposalChange::Move(true)), "↑ 上移" }
                button { r#type: "button", class: "queue-tool-button", disabled: paused || last, aria_label: "下移本地提案", onclick: move |_| update.call(ProposalChange::Move(false)), "↓ 下移" }
                select { aria_label: "选择本地提案条目", disabled: paused, value: "{selected}", onchange: move |event| if let Ok(value) = event.value().parse::<usize>() { update.call(ProposalChange::Select(value)); },
                    for (index, item) in proposal.items.iter().enumerate() { option { value: "{index}", "{index + 1}. {item.text}" } }
                }
            }
            if paused { p { class: "queue-proposal-paused", "提案已暂停编辑；这不会暂停节点队列。点击顶部“恢复提案”继续编辑。" } }
            div { class: "queue-proposal-actions",
                button { r#type: "button", class: "queue-submit", disabled: paused || has_draft || copy_text.trim().is_empty(), onclick: move |_| {
                    let mut store = drafts.write();
                    let current = store.get(&copy_scope);
                    if !current.text.trim().is_empty() {
                        // Preserve non-empty drafts; the user must explicitly clear it first.
                        return;
                    }
                    store.update(&copy_scope, |draft| { draft.text.clone_from(&copy_text); draft.delivery.clone_from(&copy_delivery); });
                    oncopied.call(());
                }, "复制到队列草稿" }
                button { r#type: "button", class: "queue-tool-button", disabled: paused, onclick: move |_| {
                    let mut store = drafts.write();
                    store.proposals.insert(scope.clone(), LocalQueueProposal::from_items(&source));
                }, "刷新提案（丢弃本地编辑）" }
            }
            if has_draft {
                details { class: "queue-proposal-conflict",
                    summary { "草稿已有内容 · 保留或明确替换" }
                    p { "默认保留已有草稿。确认替换会丢弃草稿正文及投递方式；仍需点击“加入队列”才会发送。" }
                    p { class: "queue-proposal-original", "当前草稿：{draft.text}" }
                    button { r#type: "button", class: "queue-tool-button", disabled: paused || replace_item.text.trim().is_empty(), onclick: move |_| {
                        drafts.write().update(&replace_scope, |draft| { draft.text.clone_from(&replace_item.text); draft.delivery.clone_from(&replace_item.delivery); });
                        oncopied.call(());
                    }, "确认替换现有草稿并复制" }
                }
            }
            p { class: "queue-proposal-help", "已复制内容仍需显式点击“加入队列”才会真实 enqueue；已预留的原消息无法取消或修改。" }
        }
    }
}

enum ProposalChange {
    Text(String),
    Delivery(String),
    Select(usize),
    Move(bool),
}

fn render_queue_item(
    item: &QueueItem,
    ordinal: usize,
    action: impl FnMut(QueueAction, Option<String>, u64) + Clone + 'static,
    busy: bool,
    revision: u64,
    proposal: EventHandler<(String, Option<bool>)>,
) -> Element {
    let edit_id = item.entry_id.clone();
    let up_id = item.entry_id.clone();
    let down_id = item.entry_id.clone();
    let status = QueueStatus::from_wire(item.status.as_deref());
    let entry_id = item.entry_id.clone();
    let can_cancel = status.can_cancel() && !busy;
    let reason = match status {
        QueueStatus::Claimed => "服务已预留，不能再取消。",
        QueueStatus::Completed => "该消息已被消费，不能取消。",
        QueueStatus::Cancelled => "服务端已记录取消。",
        QueueStatus::Failed => "该消息处理失败；不能把历史项当作待执行。",
        QueueStatus::Unknown => "服务端返回未知状态，不能安全操作。",
        QueueStatus::Queued => "",
    };
    let item_class = if status == QueueStatus::Queued {
        "is-pending"
    } else {
        "is-terminal"
    };
    let status_class = if status == QueueStatus::Queued {
        "pending"
    } else {
        "muted"
    };
    rsx! {
        article { class: "queue-item {item_queue_class(&item.queue)} {item_class}",
            div { class: "queue-item-meta",
                span { class: "queue-item-number", "{ordinal}" }
                span { class: "queue-kind {item_queue_class(&item.queue)}", "{item_queue_label(&item.queue)}" }
                span { class: "queue-status {status_class}", "{status.label()}" }
            }
            p { class: "queue-item-text", "{item.text}" }
            if !reason.is_empty() { small { class: "queue-item-reason", "{reason}" } }
            div { class: "queue-item-actions",
                button { r#type: "button", class: "queue-tool-button", aria_label: "上移此消息的本地提案", title: "上移本地提案", onclick: move |_| proposal.call((up_id.clone(), Some(true))), "↑" }
                button { r#type: "button", class: "queue-tool-button", aria_label: "下移此消息的本地提案", title: "下移本地提案", onclick: move |_| proposal.call((down_id.clone(), Some(false))), "↓" }
                button { r#type: "button", class: "queue-tool-button", aria_label: "编辑此消息的本地提案", onclick: move |_| proposal.call((edit_id.clone(), None)), "编辑" }
                button {
                r#type: "button",
                class: "queue-cancel",
                disabled: !can_cancel,
                aria_label: "取消此队列消息",
                onclick: move |_| action.clone()(QueueAction::Cancel, Some(entry_id.clone()), revision),
                if status == QueueStatus::Queued { "移除" } else { "不可取消" }
            } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{QueueStatus, pending_count};
    use work_client::{QueueItem, QueueSnapshot};

    #[test]
    fn completed_items_are_not_pending() {
        let snapshot = QueueSnapshot {
            count: 2,
            items: vec![
                QueueItem {
                    entry_id: "a".into(),
                    queue: "followUp".into(),
                    text: "a".into(),
                    status: Some("completed".into()),
                    completed_seq: None,
                    message: None,
                    run_id: None,
                    seq: None,
                },
                QueueItem {
                    entry_id: "b".into(),
                    queue: "steer".into(),
                    text: "b".into(),
                    status: Some("queued".into()),
                    completed_seq: None,
                    message: None,
                    run_id: None,
                    seq: None,
                },
            ],
        };
        assert_eq!(pending_count(&snapshot), 1);
        assert_eq!(
            QueueStatus::from_wire(Some("reserved")),
            QueueStatus::Claimed
        );
        assert_eq!(
            QueueStatus::from_wire(Some("future-status")),
            QueueStatus::Unknown
        );
    }
}
