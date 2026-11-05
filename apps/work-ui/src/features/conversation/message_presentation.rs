//! Shared, escaped message presentation. No message/protocol ownership lives here.
use dioxus::prelude::*;

use crate::shared::components::Icon;

#[component]
pub(super) fn CopyButton(
    text: String,
    label: String,
    #[props(default)] class_name: String,
) -> Element {
    // Associate feedback with the copied source, not just this component slot:
    // streaming updates must not claim that the latest content was copied.
    let mut feedback = use_signal(|| None::<(String, bool)>);
    let mut busy = use_signal(|| false);
    let current = feedback.read();
    let current = current.as_ref().filter(|(source, _)| source == &text);
    let copied = current.is_some_and(|(_, success)| *success);
    let failed = current.is_some_and(|(_, success)| !success);
    let notice = if copied {
        "已复制"
    } else if failed {
        "复制失败，请手动选择内容"
    } else {
        ""
    };
    rsx! {
        span { class: "conversation-copy",
            button {
                r#type: "button", class: "button tool-icon-button {class_name}",
                aria_label: "{label}", title: "{label}",
                "data-copied": copied, aria_busy: *busy.read(),
                disabled: text.is_empty() || *busy.read(),
                onclick: move |_| {
                    let source = text.clone();
                    let encoded = serde_json::to_string(&source).unwrap_or_else(|_| "\"\"".into());
                    busy.set(true);
                    spawn(async move {
                        let script = format!("try {{ await navigator.clipboard.writeText({encoded}); dioxus.send(true); }} catch (_) {{ dioxus.send(false); }}");
                        let success = document::eval(&script).recv::<bool>().await.unwrap_or(false);
                        feedback.set(Some((source, success)));
                        busy.set(false);
                    });
                },
                Icon { name: (if copied { "check" } else { "copy" }).to_owned() }
            }
            span { class: if failed { "copy-error" } else { "tool-label-hidden" }, role: "status", "{notice}" }
        }
    }
}

#[component]
pub(super) fn CodeBlock(source: String, language: String) -> Element {
    let tokens = super::code_tokens::tokenize(&source, &language);
    rsx! {
        div { class: "conversation-code",
            div { class: "code-language",
                span { if language.is_empty() { "text" } else { "{language}" } }
                CopyButton { text: source.clone(), label: "复制代码", class_name: "markdown-copy" }
            }
            pre { tabindex: "0", aria_label: "代码，可滚动",
                code {
                    for (index, token) in tokens.iter().enumerate() {
                        span { key: "token-{index}", class: token.class, "{token.text}" }
                    }
                }
            }
        }
    }
}

/// Only a list-item prefix is interpreted; inline/code text is never scanned.
pub(super) fn task_item(text: &str) -> Option<(bool, &str)> {
    let (marker, rest) = text.split_at_checked(3)?;
    let checked = match marker {
        "[ ]" => false,
        "[x]" | "[X]" => true,
        _ => return None,
    };
    if rest.is_empty() {
        Some((checked, rest))
    } else {
        rest.strip_prefix(' ')
            .or_else(|| rest.strip_prefix('\t'))
            .map(|content| (checked, content))
    }
}

#[cfg(test)]
mod tests {
    use super::task_item;

    #[test]
    fn task_markers_are_readonly_list_prefixes() {
        assert_eq!(task_item("[ ] open"), Some((false, "open")));
        assert_eq!(task_item("[x] done"), Some((true, "done")));
        assert_eq!(
            task_item("[X]\t完成 **检查**"),
            Some((true, "完成 **检查**"))
        );
        assert_eq!(task_item("[x]"), Some((true, "")));
        for text in [
            "",
            "中",
            "中文",
            "[x]ordinary",
            "[a] x",
            "before [x] x",
            "`[x] x`",
        ] {
            assert_eq!(task_item(text), None, "{text}");
        }
    }
}
