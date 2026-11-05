use super::timeline::ToolLabel;
use crate::shared::components::Icon;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

const OBSERVER: &str = include_str!("command_preview.js");
const CSS: &str = include_str!("command_preview.css");
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// Only an explicit command field is a command, never an inferred tool name or summary.
pub(super) fn command_argument(args: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(args).ok()?;
    let command = value.get("command")?.as_str()?;
    (!command.trim().is_empty()).then(|| command.to_owned())
}

#[component]
pub(super) fn CommandPreview(command: String) -> Element {
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-command-{instance}");
    let mut expanded = use_signal(|| false);
    let mut overflow = use_signal(|| None::<bool>);
    let mut observer_task = use_signal(|| None::<dioxus::core::Task>);
    let effect_id = id.clone();
    use_effect(use_reactive((&command,), move |(_command,)| {
        if let Some(task) = observer_task.write().take() {
            task.cancel();
        }
        overflow.set(None);
        let encoded_id = serde_json::to_string(&effect_id).unwrap();
        let script = format!("const id={encoded_id};{OBSERVER}");
        observer_task.set(Some(spawn(async move {
            let mut evaluator = document::eval(&script);
            while let Ok(value) = evaluator.recv::<bool>().await {
                overflow.set(Some(value));
            }
        })));
    }));
    let cleanup_id = serde_json::to_string(&id).unwrap();
    use_drop(move || {
        if let Some(task) = observer_task.write().take() {
            task.cancel();
        }
        let _ = document::eval(&format!(
            "window.__rpiCommandPreviews?.get({cleanup_id})?.();"
        ));
    });
    // Without a working measurement bridge show the complete command, not inaccessible clipping.
    let collapsed = *overflow.read() == Some(true) && !*expanded.read();
    rsx! {
        style { {CSS} }
        div { class: "command-preview", id: "{id}",
            div { class: "call-section-title conversation-output-heading",
                ToolLabel { label: "命令", icon: "terminal" }
                super::timeline::CopyOutput { text: command.clone(), command: true }
            }
            div { class: "command-preview-surface",
                pre { id: "{id}-text", class: if collapsed { "command-text collapsed" } else { "command-text" }, "{command}" }
            }
            if *overflow.read() == Some(true) {
                button { r#type: "button", class: "button command-toggle tool-icon-button",
                    aria_label: if collapsed { "展开完整命令" } else { "收起命令" },
                    title: if collapsed { "展开完整命令" } else { "收起命令" },
                    aria_expanded: !collapsed, aria_controls: "{id}-text",
                    onclick: move |_| expanded.toggle(),
                    Icon { name: "chevron" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::command_argument;
    use serde_json::json;

    #[test]
    fn extracts_only_an_explicit_nonempty_string_command() {
        for args in [
            "not json",
            "null",
            "[]",
            r#"{"command":12}"#,
            r#"{"command":"  \n"}"#,
            r#"{"args":{"command":"hidden"}}"#,
            r#"{"path":"command"}"#,
        ] {
            assert_eq!(command_argument(args), None, "{args}");
        }
    }

    #[test]
    fn preserves_the_complete_original_command_including_whitespace() {
        let command = format!("  echo '中文 <script>'\n{}\n", "x".repeat(4096));
        let args = json!({"command":command,"timeout":120}).to_string();
        assert_eq!(command_argument(&args), Some(command));
    }
}
