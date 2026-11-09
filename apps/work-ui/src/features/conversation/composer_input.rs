//! Keep per-keystroke subscriptions out of the workspace/history render scope.
use crate::shared::state::{Draft, Drafts, draft_for, update_draft};
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

const IME_GUARD: &str = include_str!("composer_input.js");
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

#[component]
pub(crate) fn ComposerInput(
    scope: String,
    drafts: Signal<Drafts>,
    disabled: bool,
    placeholder: String,
    onenter: EventHandler<()>,
) -> Element {
    // A key on a static component slot does not remount it. Use a keyed
    // iterator so each scope owns its initial value, revision and IME guard.
    rsx! {
        for scope in [scope] {
            ComposerField { key: "{scope}", scope, drafts, disabled, placeholder: placeholder.clone(), onenter }
        }
    }
}

#[component]
fn ComposerField(
    scope: String,
    drafts: Signal<Drafts>,
    disabled: bool,
    placeholder: String,
    onenter: EventHandler<()>,
) -> Element {
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-composer-{instance}");
    let mut ime_guard_ready = use_signal(|| None::<bool>);
    // The DOM owns local edits. A native WebView may apply an old render batch
    // after the IME has advanced: echoing `value` would cancel its composition.
    let initial = use_hook(|| draft_for(&drafts.peek(), &scope));
    let initial_text = initial.text.clone();
    let mut mirror = use_signal(|| initial);
    let mut local_revision = use_signal(|| None::<u64>);
    let sync_id = serde_json::to_string(&id).unwrap();
    let sync_scope = scope.clone();
    use_effect(move || {
        let draft = draft_for(&drafts.read(), &sync_scope);
        if *ime_guard_ready.read() != Some(true) || *local_revision.peek() == Some(draft.revision) {
            return;
        }
        let expected = mirror.peek().text.clone();
        mirror.set(draft.clone());
        let packet =
            serde_json::json!({"revision":draft.revision,"text":draft.text,"expected":expected});
        let _ = document::eval(&format!(
            "window.__rpiComposerInputs?.get({sync_id})?.sync({packet});"
        ));
    });
    let mount_id = serde_json::to_string(&id).unwrap();
    let cleanup_id = mount_id.clone();
    use_drop(move || {
        let _ = document::eval(&format!(
            "window.__rpiComposerInputs?.get({cleanup_id})?.();"
        ));
    });
    rsx! {
        textarea {
            id, aria_label: "发送给 agent 的消息", rows: 2,
            disabled, placeholder,
            "data-ime-ready": *ime_guard_ready.read() == Some(true),
            onmounted: move |_| {
                let script = format!("const id={mount_id};{IME_GUARD}");
                spawn(async move {
                    let installed = document::eval(&script).recv::<bool>().await.unwrap_or(false);
                    ime_guard_ready.set(Some(installed));
                });
            },
            oninput: move |event| {
                if !scope.is_empty() {
                    update_draft(drafts, &scope, event.value());
                    let draft: Draft = draft_for(&drafts.peek(), &scope);
                    local_revision.set(Some(draft.revision));
                    mirror.set(draft);
                }
            },
            onkeydown: move |event| {
                // The JS capture handler protects IME candidate confirmation;
                // normal Enter must remain usable even while that handler is
                // being installed across the WebView bridge.
                if event.key() == Key::Enter && !event.modifiers().shift() && !event.is_composing() {
                    event.prevent_default();
                    onenter.call(());
                }
            },
            "{initial_text}"
        }
        if *ime_guard_ready.read() == Some(false) {
            small { class: "meta", role: "status", "输入法保护未就绪，请用发送按钮提交。当前文字仍可编辑。" }
        }
    }
}
