use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

const LIBRARY: &str = include_str!("../../../../../assets/vendor/mermaid.min.js");
const RENDERER: &str = include_str!("mermaid.js");
const CSS: &str = include_str!("mermaid.css");
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn is_mermaid(language: &str) -> bool {
    language.eq_ignore_ascii_case("mermaid")
}

#[component]
pub(super) fn MermaidDiagram(source: String, closed: bool) -> Element {
    let instance = use_hook(|| NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed));
    let id = format!("rpi-mermaid-{instance}");
    let mut status = use_signal(|| "等待完整 Mermaid 代码…".to_owned());
    let mut notice = use_signal(String::new);
    let mut eval_task = use_signal(|| None::<dioxus::core::Task>);
    let effect_id = id.clone();
    use_effect(use_reactive((&source, &closed), move |(source, closed)| {
        if let Some(task) = eval_task.write().take() {
            task.cancel();
        }
        status.set(
            if closed {
                "正在绘制图表…"
            } else {
                "等待完整 Mermaid 代码…"
            }
            .to_owned(),
        );
        let script = mount_script(&effect_id, &source, closed);
        eval_task.set(Some(spawn(async move {
            // Probe before encoding/transferring the 5 MB offline bundle. Streaming
            // source updates only send the lightweight mount command thereafter.
            let installed = document::eval("dioxus.send(!!window.__rpiMermaid);")
                .recv::<bool>()
                .await
                .unwrap_or(false);
            if !installed {
                let _ = document::eval(&bootstrap_script()).recv::<bool>().await;
            }
            let mut evaluator = document::eval(&script);
            while let Ok(value) = evaluator.recv::<String>().await {
                status.set(value);
            }
        })));
    }));
    let cleanup_id = serde_json::to_string(&id).unwrap();
    use_drop(move || {
        if let Some(task) = eval_task.write().take() {
            task.cancel();
        }
        let _ = document::eval(&format!("window.__rpiMermaid?.dispose({cleanup_id});"));
    });
    let ready = status.read().is_empty();
    let viewer_id = serde_json::to_string(&id).unwrap();
    let copy_source = source.clone();
    rsx! {
        style { {CSS} }
        figure { class: "conversation-mermaid", id: "{id}", tabindex: "-1",
            figcaption { class: "mermaid-heading",
                span { "Mermaid" }
                button { r#type: "button", class: "button", aria_label: "放大查看 Mermaid 图表",
                    aria_haspopup: "dialog", disabled: !ready,
                    onclick: move |_| {
                        let _ = document::eval(&format!("window.__rpiMermaid?.openViewer({viewer_id});"));
                    },
                    "放大查看"
                }
                button { r#type: "button", class: "button", aria_label: "复制 Mermaid 源码",
                    onclick: move |_| {
                        let source = serde_json::to_string(&copy_source).unwrap();
                        spawn(async move {
                            let script = format!("try {{ await navigator.clipboard.writeText({source}); dioxus.send(true); }} catch (_) {{ dioxus.send(false); }}");
                            let copied = document::eval(&script).recv::<bool>().await.unwrap_or(false);
                            notice.set(if copied { "已复制" } else { "复制失败，请手动选择源码" }.to_owned());
                        });
                    },
                    "复制源码"
                }
                span { role: "status", "{notice}" }
            }
            p { class: "mermaid-status", role: "status", hidden: ready, "{status}" }
            div { class: "mermaid-image", hidden: !ready,
                img { alt: "Mermaid 图表；文字描述请查看下方源码", decoding: "async" }
            }
            details { class: "mermaid-source", open: !closed,
                summary { "查看源码" }
                pre { code { "{source}" } }
            }
        }
    }
}

fn bootstrap_script() -> String {
    let library = serde_json::to_string(LIBRARY).unwrap();
    format!("if(!window.__rpiMermaid){{const library={library};{RENDERER}}}dioxus.send(true);")
}

fn mount_script(id: &str, source: &str, closed: bool) -> String {
    // JSON, never raw source interpolation into JavaScript or srcdoc.
    let id = serde_json::to_string(id).unwrap();
    let source = serde_json::to_string(source).unwrap();
    format!(
        "window.__rpiMermaid.mount({id},{source},{closed},value=>{{try{{dioxus.send(value);}}catch(_){{}}}});"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_mermaid_fences_are_diagrams() {
        for language in ["mermaid", "MERMAID", "Mermaid"] {
            assert!(is_mermaid(language));
        }
        for language in ["", "js", "mermaid extra", "mermaid<script>"] {
            assert!(!is_mermaid(language));
        }
    }

    #[test]
    fn diagram_source_is_json_encoded_not_executable_script() {
        let source = "</script>\";alert('no');\\\n中文";
        let script = mount_script("safe-id", source, false);
        assert!(script.contains(&format!(
            ",{},false,",
            serde_json::to_string(source).unwrap()
        )));
        assert!(
            script.len() < 512,
            "source updates must not transfer the bundle"
        );
        assert!(RENDERER.contains("sandbox"));
        assert!(!RENDERER.contains("allow-same-origin"));
    }
}
